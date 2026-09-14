//! ZIP and TAR Archive Reading, Writing, Virtual Filesystem, and Security Safeguards.

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone)]
pub struct ArchiveSecurityOptions {
    pub max_total_size: usize,
    pub max_entry_size: usize,
    pub max_entries: usize,
    pub max_expansion_ratio: f64,
}

impl Default for ArchiveSecurityOptions {
    fn default() -> Self {
        Self {
            max_total_size: 500 * 1024 * 1024, // 500 MB default total extraction limit
            max_entry_size: 100 * 1024 * 1024, // 100 MB default per-entry limit
            max_entries: 10_000,               // 10,000 max entries limit
            max_expansion_ratio: 100.0,        // 100x max expansion ratio limit
        }
    }
}

pub fn sanitize_archive_path(entry_path: &str, target_dir: &Path) -> Result<PathBuf, String> {
    let clean_path = entry_path.replace('\\', "/");
    let relative = Path::new(&clean_path);

    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err(format!(
                    "Security Violation: Path traversal ('..') detected in archive entry '{}'",
                    entry_path
                ));
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(format!(
                    "Security Violation: Absolute path detected in archive entry '{}'",
                    entry_path
                ));
            }
            _ => {}
        }
    }

    let full_path = target_dir.join(relative);

    // Verify path does not escape target directory
    if let Ok(canonical_target) = target_dir.canonicalize() {
        if let Ok(canonical_full) = full_path.canonicalize() {
            if !canonical_full.starts_with(&canonical_target) {
                return Err(format!(
                    "Security Violation: Archive entry '{}' escapes extraction directory",
                    entry_path
                ));
            }
        }
    }

    Ok(full_path)
}

// ----------------------------------------------------------------------------
// ZIP Archives
// ----------------------------------------------------------------------------

pub struct ZipEntryInfo {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub is_dir: bool,
    pub crc32: u32,
}

pub fn create_zip_archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for (name, content) in files {
        writer
            .start_file(*name, options)
            .map_err(|e| format!("Failed to start file '{}' in ZIP: {}", name, e))?;
        writer
            .write_all(content)
            .map_err(|e| format!("Failed to write content for '{}' in ZIP: {}", name, e))?;
    }

    let cursor = writer
        .finish()
        .map_err(|e| format!("Failed to finalize ZIP archive: {}", e))?;
    Ok(cursor.into_inner())
}

pub fn create_zip_file_on_disk(
    file_mappings: &[(&str, &Path)],
    output_zip_path: &Path,
) -> Result<usize, String> {
    let zip_file = File::create(output_zip_path).map_err(|e| {
        format!(
            "Failed to create output ZIP file '{}': {}",
            output_zip_path.display(),
            e
        )
    })?;
    let mut writer = zip::ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut count = 0usize;
    for (entry_name, disk_path) in file_mappings {
        let mut f = File::open(disk_path)
            .map_err(|e| format!("Failed to open input file '{}': {}", disk_path.display(), e))?;
        writer
            .start_file(*entry_name, options)
            .map_err(|e| format!("Failed to start ZIP entry '{}': {}", entry_name, e))?;

        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f
                .read(&mut buf)
                .map_err(|e| format!("Error reading file '{}': {}", disk_path.display(), e))?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n]).map_err(|e| {
                format!(
                    "Error writing ZIP entry content for '{}': {}",
                    entry_name, e
                )
            })?;
        }
        count += 1;
    }

    writer
        .finish()
        .map_err(|e| format!("Failed to finalize ZIP file: {}", e))?;
    Ok(count)
}

pub fn extract_zip_file_from_disk(
    zip_path: &Path,
    target_dir: &Path,
    sec_options: Option<&ArchiveSecurityOptions>,
) -> Result<usize, String> {
    let zip_file = File::open(zip_path)
        .map_err(|e| format!("Failed to open ZIP file '{}': {}", zip_path.display(), e))?;
    let default_opts = ArchiveSecurityOptions::default();
    let opts = sec_options.unwrap_or(&default_opts);

    let mut archive = zip::ZipArchive::new(zip_file)
        .map_err(|e| format!("Failed to parse ZIP file '{}': {}", zip_path.display(), e))?;

    if archive.len() > opts.max_entries {
        return Err(format!(
            "Archive entry count ({}) exceeds limit ({})",
            archive.len(),
            opts.max_entries
        ));
    }

    let mut total_extracted = 0usize;
    let mut extracted_files = 0usize;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry index {}: {}", i, e))?;

        let entry_name = file.name().to_string();
        let safe_out_path = sanitize_archive_path(&entry_name, target_dir)?;

        if file.is_dir() {
            std::fs::create_dir_all(&safe_out_path).map_err(|e| {
                format!(
                    "Failed to create directory '{}': {}",
                    safe_out_path.display(),
                    e
                )
            })?;
        } else {
            if let Some(parent) = safe_out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create parent directory '{}': {}",
                        parent.display(),
                        e
                    )
                })?;
            }

            let mut outfile = File::create(&safe_out_path).map_err(|e| {
                format!(
                    "Failed to create output file '{}': {}",
                    safe_out_path.display(),
                    e
                )
            })?;

            let mut buf = [0u8; 8192];
            let mut file_extracted = 0usize;
            loop {
                let n = file
                    .read(&mut buf)
                    .map_err(|e| format!("Failed reading entry '{}': {}", entry_name, e))?;
                if n == 0 {
                    break;
                }
                file_extracted += n;
                total_extracted += n;

                if file_extracted > opts.max_entry_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded on entry '{}'",
                        entry_name
                    ));
                }

                if total_extracted > opts.max_total_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded: total extraction > {} bytes",
                        opts.max_total_size
                    ));
                }

                outfile.write_all(&buf[..n]).map_err(|e| {
                    format!(
                        "Failed writing content to '{}': {}",
                        safe_out_path.display(),
                        e
                    )
                })?;
            }
            extracted_files += 1;
        }
    }

    Ok(extracted_files)
}

pub fn list_zip_entries(zip_bytes: &[u8]) -> Result<Vec<ZipEntryInfo>, String> {
    let reader = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to parse ZIP archive: {}", e))?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry index {}: {}", i, e))?;
        entries.push(ZipEntryInfo {
            name: file.name().to_string(),
            size: file.size(),
            compressed_size: file.compressed_size(),
            is_dir: file.is_dir(),
            crc32: file.crc32(),
        });
    }
    Ok(entries)
}

pub fn read_zip_entry(
    zip_bytes: &[u8],
    entry_name: &str,
    sec_options: Option<&ArchiveSecurityOptions>,
) -> Result<Vec<u8>, String> {
    let default_opts = ArchiveSecurityOptions::default();
    let opts = sec_options.unwrap_or(&default_opts);

    let reader = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to parse ZIP archive: {}", e))?;

    let mut file = archive
        .by_name(entry_name)
        .map_err(|e| format!("Entry '{}' not found in ZIP archive: {}", entry_name, e))?;

    let declared_size = file.size() as usize;
    if declared_size > opts.max_entry_size {
        return Err(format!(
            "Decompression bomb protection: ZIP entry '{}' size ({} bytes) exceeds limit ({} bytes)",
            entry_name, declared_size, opts.max_entry_size
        ));
    }

    let mut content = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Error reading ZIP entry '{}': {}", entry_name, e))?;
        if n == 0 {
            break;
        }
        if content.len() + n > opts.max_entry_size {
            return Err(format!(
                "Decompression bomb protection: ZIP entry '{}' expanded size exceeds limit of {} bytes",
                entry_name, opts.max_entry_size
            ));
        }
        content.extend_from_slice(&buf[..n]);
    }

    Ok(content)
}

pub fn extract_zip_archive(
    zip_bytes: &[u8],
    target_dir: &Path,
    sec_options: Option<&ArchiveSecurityOptions>,
) -> Result<usize, String> {
    let default_opts = ArchiveSecurityOptions::default();
    let opts = sec_options.unwrap_or(&default_opts);

    let reader = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to parse ZIP archive: {}", e))?;

    if archive.len() > opts.max_entries {
        return Err(format!(
            "Archive entry count ({}) exceeds limit ({})",
            archive.len(),
            opts.max_entries
        ));
    }

    let mut total_extracted = 0usize;
    let mut extracted_files = 0usize;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry index {}: {}", i, e))?;

        let entry_name = file.name().to_string();
        let safe_out_path = sanitize_archive_path(&entry_name, target_dir)?;

        if file.is_dir() {
            std::fs::create_dir_all(&safe_out_path).map_err(|e| {
                format!(
                    "Failed to create directory '{}': {}",
                    safe_out_path.display(),
                    e
                )
            })?;
        } else {
            if let Some(parent) = safe_out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create parent directory '{}': {}",
                        parent.display(),
                        e
                    )
                })?;
            }

            let mut outfile = File::create(&safe_out_path).map_err(|e| {
                format!(
                    "Failed to create output file '{}': {}",
                    safe_out_path.display(),
                    e
                )
            })?;

            let mut buf = [0u8; 8192];
            let mut file_extracted = 0usize;
            loop {
                let n = file
                    .read(&mut buf)
                    .map_err(|e| format!("Failed reading entry '{}': {}", entry_name, e))?;
                if n == 0 {
                    break;
                }
                file_extracted += n;
                total_extracted += n;

                if file_extracted > opts.max_entry_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded on entry '{}'",
                        entry_name
                    ));
                }

                if total_extracted > opts.max_total_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded: total extraction > {} bytes",
                        opts.max_total_size
                    ));
                }

                outfile.write_all(&buf[..n]).map_err(|e| {
                    format!(
                        "Failed writing content to '{}': {}",
                        safe_out_path.display(),
                        e
                    )
                })?;
            }
            extracted_files += 1;
        }
    }

    Ok(extracted_files)
}

// ----------------------------------------------------------------------------
// TAR Archives
// ----------------------------------------------------------------------------

pub fn create_tar_archive(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut builder = tar::Builder::new(Vec::new());

    for (name, content) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        builder
            .append_data(&mut header, *name, *content)
            .map_err(|e| format!("Failed to append file '{}' to TAR: {}", name, e))?;
    }

    builder
        .into_inner()
        .map_err(|e| format!("Failed to finalize TAR archive: {}", e))
}

pub fn extract_tar_archive(
    tar_bytes: &[u8],
    target_dir: &Path,
    sec_options: Option<&ArchiveSecurityOptions>,
) -> Result<usize, String> {
    let default_opts = ArchiveSecurityOptions::default();
    let opts = sec_options.unwrap_or(&default_opts);

    let mut archive = tar::Archive::new(Cursor::new(tar_bytes));
    let mut extracted_count = 0usize;
    let mut total_extracted = 0usize;

    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to parse TAR entries: {}", e))?;

    for entry_res in entries {
        let mut entry =
            entry_res.map_err(|e: std::io::Error| format!("Failed reading TAR entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("Invalid TAR entry path: {}", e))?;
        let entry_name = path.to_string_lossy().to_string();

        let safe_out_path = sanitize_archive_path(&entry_name, target_dir)?;

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&safe_out_path).map_err(|e| {
                format!(
                    "Failed to create directory '{}': {}",
                    safe_out_path.display(),
                    e
                )
            })?;
        } else if entry.header().entry_type().is_file() {
            if let Some(parent) = safe_out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent dir '{}': {}", parent.display(), e)
                })?;
            }

            let mut outfile = File::create(&safe_out_path).map_err(|e| {
                format!("Failed to create file '{}': {}", safe_out_path.display(), e)
            })?;

            let mut buf = [0u8; 8192];
            let mut file_extracted = 0usize;
            loop {
                let n = entry
                    .read(&mut buf)
                    .map_err(|e| format!("Failed reading TAR entry '{}': {}", entry_name, e))?;
                if n == 0 {
                    break;
                }
                file_extracted += n;
                total_extracted += n;

                if file_extracted > opts.max_entry_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded on TAR entry '{}'",
                        entry_name
                    ));
                }

                if total_extracted > opts.max_total_size {
                    let _ = std::fs::remove_file(&safe_out_path);
                    return Err(format!(
                        "Decompression bomb limit exceeded: total extraction > {} bytes",
                        opts.max_total_size
                    ));
                }

                outfile.write_all(&buf[..n]).map_err(|e| {
                    format!(
                        "Failed writing content to '{}': {}",
                        safe_out_path.display(),
                        e
                    )
                })?;
            }
            extracted_count += 1;
        }
    }

    Ok(extracted_count)
}
