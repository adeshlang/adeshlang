//! Native Function Callbacks & Module Object Assembly for AdeshLang Compression.

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::sync::Arc;

use super::adaptive::{compress_adaptive, entropy_estimate};
use super::archives::{
    create_tar_archive, create_zip_archive, create_zip_file_on_disk, extract_tar_archive,
    extract_zip_archive, extract_zip_file_from_disk,
};
use super::checksums::{adler32, crc32, crc32c, xxhash64};
use super::chunking::ChunkedArchive;
use super::codecs::{
    CodecType, compress_bound, compress_brotli, compress_bytes, compress_deflate, compress_gzip,
    compress_lz4, compress_xz, compress_zlib, compress_zstd, decompress_brotli, decompress_bytes,
    decompress_deflate, decompress_gzip, decompress_lz4, decompress_xz, decompress_zlib,
    decompress_zstd,
};
use super::parallel::{parallel_compress, parallel_decompress};
use super::streaming::{StreamDecoder, StreamEncoder};
use std::io::{Read, Write};

fn get_str<'a>(val: &'a Value) -> Option<&'a str> {
    match val {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn value_to_array(val: &Value) -> Option<Vec<Value>> {
    match val {
        Value::Array(arr) => Some(arr.clone()),
        Value::RawArray(_, arr) => Some(arr.clone()),
        Value::DynArray(da) => Some(da.data.clone()),
        Value::Tuple(tup) => Some(tup.clone()),
        Value::Set(s) => Some(s.clone()),
        _ => None,
    }
}

fn value_to_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        other => {
            if let Some(arr) = value_to_array(other) {
                let mut bytes = Vec::with_capacity(arr.len());
                for elem in arr.iter() {
                    if let Value::Number(f) = elem {
                        bytes.push(*f as u8);
                    } else if let Value::U8(n) = elem {
                        bytes.push(*n);
                    } else if let Value::I64(n) = elem {
                        bytes.push(*n as u8);
                    } else {
                        return Err("Expected array of byte numbers".to_string());
                    }
                }
                Ok(bytes)
            } else {
                Err("Expected String or Byte Array".to_string())
            }
        }
    }
}

fn bytes_to_value(bytes: &[u8]) -> Value {
    Value::Array(bytes.iter().map(|&b| Value::Number(b as f64)).collect())
}

// ----------------------------------------------------------------------------
// Builtin Callbacks
// ----------------------------------------------------------------------------

fn builtin_compress(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data_val = args
        .get(0)
        .ok_or("Compression.compress requires data argument")?;
    let data = value_to_bytes(data_val)?;

    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;

    let level = args
        .get(2)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            Value::I64(i) => Some(*i as i32),
            _ => None,
        })
        .unwrap_or(5);

    let compressed = compress_bytes(codec, &data, level)?;
    Ok(bytes_to_value(&compressed))
}

fn builtin_decompress(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data_val = args
        .get(0)
        .ok_or("Compression.decompress requires data argument")?;
    let data = value_to_bytes(data_val)?;

    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;

    let max_output_size = args.get(2).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        Value::I64(i) => Some(*i as usize),
        _ => None,
    });

    let decompressed = decompress_bytes(codec, &data, max_output_size)?;
    Ok(bytes_to_value(&decompressed))
}

fn builtin_gzip(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.gzip requires data")?)?;
    let level = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as u32),
            _ => None,
        })
        .unwrap_or(6);
    let comp = compress_gzip(&data, level)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_gunzip(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.gunzip requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_gzip(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_zstd(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.zstd requires data")?)?;
    let level = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            _ => None,
        })
        .unwrap_or(5);
    let comp = compress_zstd(&data, level)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_unzstd(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.unzstd requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_zstd(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_brotli(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.brotli requires data")?)?;
    let level = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as u32),
            _ => None,
        })
        .unwrap_or(6);
    let comp = compress_brotli(&data, level)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_unbrotli(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.unbrotli requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_brotli(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_lz4(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.lz4 requires data")?)?;
    let comp = compress_lz4(&data)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_unlz4(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.unlz4 requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_lz4(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_xz(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.xz requires data")?)?;
    let comp = compress_xz(&data)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_unxz(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.unxz requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_xz(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_deflate(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.deflate requires data")?)?;
    let level = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as u32),
            _ => None,
        })
        .unwrap_or(6);
    let comp = compress_deflate(&data, level)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_inflate(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.inflate requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_deflate(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_zlib(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.zlib requires data")?)?;
    let level = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as u32),
            _ => None,
        })
        .unwrap_or(6);
    let comp = compress_zlib(&data, level)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_unzlib(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.unzlib requires data")?)?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let decomp = decompress_zlib(&data, max_sz)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_compress_bound(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let input_len = args
        .get(0)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(0);
    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let bound = compress_bound(codec, input_len);
    Ok(Value::Number(bound as f64))
}

fn builtin_compress_adaptive(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.compressAdaptive requires data")?,
    )?;
    let res = compress_adaptive(&data, None)?;

    let mut map = FastMap::default();
    map.insert(
        "compressedData".to_string(),
        bytes_to_value(&res.compressed_data),
    );
    map.insert(
        "originalSize".to_string(),
        Value::Number(res.original_size as f64),
    );
    map.insert(
        "compressedSize".to_string(),
        Value::Number(res.compressed_size as f64),
    );
    map.insert("ratio".to_string(), Value::Number(res.ratio));
    map.insert(
        "spaceSavedPercent".to_string(),
        Value::Number(res.space_saved_percent),
    );
    map.insert("algorithm".to_string(), Value::Str(res.algorithm));
    map.insert("wasCompressed".to_string(), Value::Bool(res.was_compressed));
    map.insert("checksum".to_string(), Value::Number(res.checksum as f64));

    Ok(Value::Object(Arc::new(map)))
}

fn builtin_entropy_estimate(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.entropyEstimate requires data")?,
    )?;
    let entropy = entropy_estimate(&data);
    Ok(Value::Number(entropy))
}

fn builtin_crc32(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Checksum CRC32 requires data")?)?;
    Ok(Value::Number(crc32(&data) as f64))
}

fn builtin_crc32c(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Checksum CRC32C requires data")?)?;
    Ok(Value::Number(crc32c(&data) as f64))
}

fn builtin_adler32(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Checksum Adler32 requires data")?)?;
    Ok(Value::Number(adler32(&data) as f64))
}

fn builtin_xxhash64(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Checksum XXHash64 requires data")?)?;
    Ok(Value::Number(xxhash64(&data) as f64))
}

fn builtin_create_seekable(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.createSeekable requires data")?,
    )?;
    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let chunk_size = args
        .get(2)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(65536);
    let level = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            _ => None,
        })
        .unwrap_or(5);

    let archive = ChunkedArchive::create(&data, codec, chunk_size, level)?;
    Ok(bytes_to_value(&archive))
}

fn builtin_read_seekable_chunk(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let archive_bytes = value_to_bytes(
        args.get(0)
            .ok_or("Compression.readSeekableChunk requires archive bytes")?,
    )?;
    let chunk_idx = args
        .get(1)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(0);

    let chunk = ChunkedArchive::read_chunk(&archive_bytes, chunk_idx)?;
    Ok(bytes_to_value(&chunk))
}

fn get_object_field(val: &Value, key: &str) -> Option<Value> {
    match val {
        Value::Object(map) => map.get(key).cloned(),
        Value::Instance(inst) => {
            if let Ok(fields) = inst.fields.read() {
                fields.get(key).cloned()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn builtin_create_zip(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let files_arr = args
        .get(0)
        .ok_or("Compression.createZip requires array of {name, content}")?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    let arr = value_to_array(files_arr).ok_or("Compression.createZip requires array argument")?;
    for item in arr.iter() {
        let name_val = get_object_field(item, "name").ok_or("Zip entry requires 'name'")?;
        let name = get_str(&name_val)
            .ok_or("Zip entry 'name' must be string")?
            .to_string();

        if let Some(path_val) =
            get_object_field(item, "filePath").or_else(|| get_object_field(item, "path"))
        {
            let disk_path = get_str(&path_val).ok_or("filePath must be string")?;
            let content = std::fs::read(disk_path)
                .map_err(|e| format!("Failed to read file '{}': {}", disk_path, e))?;
            files.push((name, content));
        } else if let Some(content_val) = get_object_field(item, "content") {
            let content = value_to_bytes(&content_val)?;
            files.push((name, content));
        } else {
            return Err(format!(
                "Zip entry '{}' must provide 'content' or 'filePath'",
                name
            ));
        }
    }

    let file_refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, c)| (n.as_str(), c.as_slice()))
        .collect();
    let zip_bytes = create_zip_archive(&file_refs)?;
    Ok(bytes_to_value(&zip_bytes))
}

fn builtin_create_zip_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let files_arr = args
        .get(0)
        .ok_or("Compression.createZipFile requires array of {name, path}")?;
    let output_zip_path = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("Compression.createZipFile requires output zip path string")?;

    let mut mappings: Vec<(String, std::path::PathBuf)> = Vec::new();
    let arr =
        value_to_array(files_arr).ok_or("Compression.createZipFile requires array argument")?;
    for item in arr.iter() {
        if let Value::Str(path_str) = item {
            let path = std::path::Path::new(path_str);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());
            mappings.push((name, std::path::PathBuf::from(path_str)));
        } else {
            let name_val = get_object_field(item, "name").ok_or("Zip entry requires 'name'")?;
            let name = get_str(&name_val)
                .ok_or("Zip entry 'name' must be string")?
                .to_string();

            let path_val = get_object_field(item, "path")
                .or_else(|| get_object_field(item, "filePath"))
                .ok_or("Zip entry requires 'path'")?;
            let path_str = get_str(&path_val).ok_or("Zip entry 'path' must be string")?;

            mappings.push((name, std::path::PathBuf::from(path_str)));
        }
    }

    let refs: Vec<(&str, &std::path::Path)> = mappings
        .iter()
        .map(|(n, p)| (n.as_str(), p.as_path()))
        .collect();
    let count = create_zip_file_on_disk(&refs, std::path::Path::new(output_zip_path))?;
    Ok(Value::Number(count as f64))
}

fn builtin_extract_zip(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let zip_bytes = value_to_bytes(
        args.get(0)
            .ok_or("Compression.extractZip requires zip bytes")?,
    )?;
    let target_dir_str = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("Compression.extractZip requires target directory")?;

    let count = extract_zip_archive(&zip_bytes, std::path::Path::new(target_dir_str), None)?;
    Ok(Value::Number(count as f64))
}

fn builtin_extract_zip_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let zip_path = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("Compression.extractZipFile requires input zip path string")?;
    let target_dir_str = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("Compression.extractZipFile requires target directory string")?;

    let count = extract_zip_file_from_disk(
        std::path::Path::new(zip_path),
        std::path::Path::new(target_dir_str),
        None,
    )?;
    Ok(Value::Number(count as f64))
}

fn builtin_create_tar(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let files_arr = args
        .get(0)
        .ok_or("Compression.createTar requires array of {name, content}")?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    let arr = value_to_array(files_arr).ok_or("Compression.createTar requires array argument")?;
    for item in arr.iter() {
        let name_val = get_object_field(item, "name").ok_or("Tar entry requires 'name'")?;
        let name = get_str(&name_val)
            .ok_or("Tar entry 'name' must be string")?
            .to_string();

        if let Some(path_val) =
            get_object_field(item, "filePath").or_else(|| get_object_field(item, "path"))
        {
            let disk_path = get_str(&path_val).ok_or("filePath must be string")?;
            let content = std::fs::read(disk_path)
                .map_err(|e| format!("Failed to read file '{}': {}", disk_path, e))?;
            files.push((name, content));
        } else if let Some(content_val) = get_object_field(item, "content") {
            let content = value_to_bytes(&content_val)?;
            files.push((name, content));
        } else {
            return Err(format!(
                "Tar entry '{}' must provide 'content' or 'filePath'",
                name
            ));
        }
    }

    let file_refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, c)| (n.as_str(), c.as_slice()))
        .collect();
    let tar_bytes = create_tar_archive(&file_refs)?;
    Ok(bytes_to_value(&tar_bytes))
}

fn builtin_extract_tar(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let tar_bytes = value_to_bytes(
        args.get(0)
            .ok_or("Compression.extractTar requires tar bytes")?,
    )?;
    let target_dir_str = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("Compression.extractTar requires target directory")?;

    let count = extract_tar_archive(&tar_bytes, std::path::Path::new(target_dir_str), None)?;
    Ok(Value::Number(count as f64))
}

fn builtin_parallel_compress(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.parallelCompress requires data")?,
    )?;
    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let chunk_sz = args
        .get(2)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(128 * 1024);
    let level = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            _ => None,
        })
        .unwrap_or(5);
    let threads = args
        .get(4)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(0);

    let comp = parallel_compress(&data, codec, chunk_sz, level, threads)?;
    Ok(bytes_to_value(&comp))
}

fn builtin_parallel_decompress(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.parallelDecompress requires data")?,
    )?;
    let codec_str = args.get(1).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let max_sz = args.get(2).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });
    let threads = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as usize),
            _ => None,
        })
        .unwrap_or(0);

    let decomp = parallel_decompress(&data, codec, max_sz, threads)?;
    Ok(bytes_to_value(&decomp))
}

fn builtin_compress_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let input_path = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("compressFile requires input path string")?;
    let output_path = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("compressFile requires output path string")?;
    let codec_str = args.get(2).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let level = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            _ => None,
        })
        .unwrap_or(5);

    let mut reader = std::fs::File::open(input_path)
        .map_err(|e| format!("Failed to open input file '{}': {}", input_path, e))?;
    let mut writer = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create output file '{}': {}", output_path, e))?;

    let mut buf = vec![0u8; 64 * 1024];
    let mut encoder = StreamEncoder::new(codec, level, 64 * 1024);

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Error reading file: {}", e))?;
        if n == 0 {
            break;
        }
        let chunk_out = encoder.write(&buf[..n])?;
        if !chunk_out.is_empty() {
            writer
                .write_all(&chunk_out)
                .map_err(|e| format!("Error writing output: {}", e))?;
        }
    }
    let final_out = encoder.finish()?;
    if !final_out.is_empty() {
        writer
            .write_all(&final_out)
            .map_err(|e| format!("Error writing final chunk: {}", e))?;
    }

    Ok(Value::Bool(true))
}

fn builtin_decompress_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let input_path = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("decompressFile requires input path string")?;
    let output_path = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("decompressFile requires output path string")?;
    let codec_str = args.get(2).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let max_sz = args.get(3).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });

    let mut reader = std::fs::File::open(input_path)
        .map_err(|e| format!("Failed to open input file '{}': {}", input_path, e))?;
    let mut writer = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create output file '{}': {}", output_path, e))?;

    let mut buf = vec![0u8; 64 * 1024];
    let mut decoder = StreamDecoder::new(codec, max_sz);

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Error reading input file: {}", e))?;
        if n == 0 {
            break;
        }
        let decomp_out = decoder.read_chunk(&buf[..n])?;
        if !decomp_out.is_empty() {
            writer
                .write_all(&decomp_out)
                .map_err(|e| format!("Error writing decompressed file: {}", e))?;
        }
    }

    Ok(Value::Bool(true))
}

fn builtin_compress_file_atomic(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input_path = args
        .get(0)
        .and_then(|v| get_str(v))
        .ok_or("compressFileAtomic requires input path string")?;
    let output_path = args
        .get(1)
        .and_then(|v| get_str(v))
        .ok_or("compressFileAtomic requires output path string")?;
    let codec_str = args.get(2).and_then(|v| get_str(v)).unwrap_or("zstd");
    let codec = CodecType::from_str(codec_str)?;
    let level = args
        .get(3)
        .and_then(|v| match v {
            Value::Number(f) => Some(*f as i32),
            _ => None,
        })
        .unwrap_or(5);

    let tmp_path = format!(
        "{}.tmp.{}",
        output_path,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );

    let mut reader = std::fs::File::open(input_path)
        .map_err(|e| format!("Failed to open input file '{}': {}", input_path, e))?;
    let mut writer = std::fs::File::create(&tmp_path)
        .map_err(|e| format!("Failed to create temp file '{}': {}", tmp_path, e))?;

    let mut buf = vec![0u8; 64 * 1024];
    let mut encoder = StreamEncoder::new(codec, level, 64 * 1024);

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Error reading file: {}", e))?;
        if n == 0 {
            break;
        }
        let chunk_out = encoder.write(&buf[..n])?;
        if !chunk_out.is_empty() {
            if let Err(e) = writer.write_all(&chunk_out) {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(format!("Error writing temp file: {}", e));
            }
        }
    }
    let final_out = encoder.finish()?;
    if !final_out.is_empty() {
        if let Err(e) = writer.write_all(&final_out) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Error writing temp final chunk: {}", e));
        }
    }
    writer
        .flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;
    drop(writer);

    std::fs::rename(&tmp_path, output_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        format!(
            "Atomic rename failed from '{}' to '{}': {}",
            tmp_path, output_path, e
        )
    })?;

    Ok(Value::Bool(true))
}

fn builtin_detect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(args.get(0).ok_or("Compression.detect requires data")?)?;
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        return Ok(Value::Str("gzip".to_string()));
    }
    if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
        return Ok(Value::Str("zstd".to_string()));
    }
    if data.len() >= 4 && data[0] == b'P' && data[1] == b'K' && data[2] == 3 && data[3] == 4 {
        return Ok(Value::Str("zip".to_string()));
    }
    if data.len() >= 6
        && data[0] == 0xfd
        && data[1] == b'7'
        && data[2] == b'z'
        && data[3] == b'X'
        && data[4] == b'Z'
        && data[5] == 0x00
    {
        return Ok(Value::Str("xz".to_string()));
    }
    if data.len() >= 2
        && data[0] == 0x78
        && (data[1] == 0x9c || data[1] == 0x01 || data[1] == 0xda || data[1] == 0x5e)
    {
        return Ok(Value::Str("zlib".to_string()));
    }
    if data.len() >= 4 && &data[0..4] == b"ADSH" {
        return Ok(Value::Str("seekable".to_string()));
    }
    Ok(Value::Str("unknown".to_string()))
}

fn builtin_decompress_auto(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let data = value_to_bytes(
        args.get(0)
            .ok_or("Compression.decompressAuto requires data")?,
    )?;
    let max_sz = args.get(1).and_then(|v| match v {
        Value::Number(f) => Some(*f as usize),
        _ => None,
    });

    let detected = match builtin_detect(
        _env,
        vec![Value::Array(
            data.iter().map(|&b| Value::Number(b as f64)).collect(),
        )],
    )? {
        Value::Str(s) => s,
        _ => "unknown".to_string(),
    };

    let codec = match detected.as_str() {
        "gzip" => CodecType::Gzip,
        "zstd" => CodecType::Zstd,
        "xz" => CodecType::Xz,
        "zlib" => CodecType::Zlib,
        _ => {
            return Err(format!(
                "Auto decompression unable to determine codec for header signature (detected: {})",
                detected
            ));
        }
    };

    let decompressed = decompress_bytes(codec, &data, max_sz)?;
    Ok(bytes_to_value(&decompressed))
}

// ----------------------------------------------------------------------------
// Registration
// ----------------------------------------------------------------------------

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "Compression.compress",
        "compression",
        "Compress data using specified codec",
        builtin_compress,
    );
    registry.register(
        "Compression.decompress",
        "compression",
        "Decompress data using specified codec",
        builtin_decompress,
    );
    registry.register(
        "Compression.gzip",
        "compression",
        "Compress data using GZIP",
        builtin_gzip,
    );
    registry.register(
        "Compression.gunzip",
        "compression",
        "Decompress GZIP data",
        builtin_gunzip,
    );
    registry.register(
        "Compression.zstd",
        "compression",
        "Compress data using Zstd",
        builtin_zstd,
    );
    registry.register(
        "Compression.unzstd",
        "compression",
        "Decompress Zstd data",
        builtin_unzstd,
    );
    registry.register(
        "Compression.brotli",
        "compression",
        "Compress data using Brotli",
        builtin_brotli,
    );
    registry.register(
        "Compression.unbrotli",
        "compression",
        "Decompress Brotli data",
        builtin_unbrotli,
    );
    registry.register(
        "Compression.lz4",
        "compression",
        "Compress data using LZ4",
        builtin_lz4,
    );
    registry.register(
        "Compression.unlz4",
        "compression",
        "Decompress LZ4 data",
        builtin_unlz4,
    );
    registry.register(
        "Compression.xz",
        "compression",
        "Compress data using XZ",
        builtin_xz,
    );
    registry.register(
        "Compression.unxz",
        "compression",
        "Decompress XZ data",
        builtin_unxz,
    );
    registry.register(
        "Compression.deflate",
        "compression",
        "Compress data using DEFLATE",
        builtin_deflate,
    );
    registry.register(
        "Compression.inflate",
        "compression",
        "Decompress DEFLATE data",
        builtin_inflate,
    );
    registry.register(
        "Compression.zlib",
        "compression",
        "Compress data using ZLIB",
        builtin_zlib,
    );
    registry.register(
        "Compression.unzlib",
        "compression",
        "Decompress ZLIB data",
        builtin_unzlib,
    );
    registry.register(
        "Compression.compressBound",
        "compression",
        "Estimate upper bound for compressed output size",
        builtin_compress_bound,
    );
    registry.register(
        "Compression.compressAdaptive",
        "compression",
        "Adaptive payload classification & compression",
        builtin_compress_adaptive,
    );
    registry.register(
        "Compression.entropyEstimate",
        "compression",
        "Estimate Shannon entropy (bits per byte)",
        builtin_entropy_estimate,
    );

    registry.register(
        "Compression.crc32",
        "compression",
        "Compute CRC32 checksum",
        builtin_crc32,
    );
    registry.register(
        "Compression.crc32c",
        "compression",
        "Compute CRC32C checksum",
        builtin_crc32c,
    );
    registry.register(
        "Compression.adler32",
        "compression",
        "Compute Adler32 checksum",
        builtin_adler32,
    );
    registry.register(
        "Compression.xxhash64",
        "compression",
        "Compute XXHash64 checksum",
        builtin_xxhash64,
    );

    registry.register(
        "Compression.createSeekable",
        "compression",
        "Create seekable indexed chunked compressed archive",
        builtin_create_seekable,
    );
    registry.register(
        "Compression.readSeekableChunk",
        "compression",
        "Read chunk from seekable compressed archive",
        builtin_read_seekable_chunk,
    );

    registry.register(
        "Compression.createZip",
        "compression",
        "Create ZIP archive from files",
        builtin_create_zip,
    );
    registry.register(
        "Compression.createZipFile",
        "compression",
        "Create ZIP file archive directly from disk files",
        builtin_create_zip_file,
    );
    registry.register(
        "Compression.extractZip",
        "compression",
        "Extract ZIP archive with security safeguards",
        builtin_extract_zip,
    );
    registry.register(
        "Compression.extractZipFile",
        "compression",
        "Extract ZIP file from disk with security safeguards",
        builtin_extract_zip_file,
    );
    registry.register(
        "Compression.createTar",
        "compression",
        "Create TAR archive from files",
        builtin_create_tar,
    );
    registry.register(
        "Compression.extractTar",
        "compression",
        "Extract TAR archive with security safeguards",
        builtin_extract_tar,
    );

    registry.register(
        "Compression.parallelCompress",
        "compression",
        "Multi-threaded parallel chunk compression",
        builtin_parallel_compress,
    );
    registry.register(
        "Compression.parallelDecompress",
        "compression",
        "Multi-threaded parallel chunk decompression",
        builtin_parallel_decompress,
    );

    registry.register(
        "Compression.compressFile",
        "compression",
        "Stream compress file from disk to disk",
        builtin_compress_file,
    );
    registry.register(
        "Compression.decompressFile",
        "compression",
        "Stream decompress file from disk to disk",
        builtin_decompress_file,
    );
    registry.register(
        "Compression.compressFileAtomic",
        "compression",
        "Atomic stream compress file",
        builtin_compress_file_atomic,
    );
    registry.register(
        "Compression.detect",
        "compression",
        "Magic-byte compression format detection",
        builtin_detect,
    );
    registry.register(
        "Compression.decompressAuto",
        "compression",
        "Auto-detect magic header and decompress",
        builtin_decompress_auto,
    );
}

pub fn build_compression_module_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "compress".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress))),
    );
    map.insert(
        "decompress".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_decompress))),
    );

    map.insert(
        "gzip".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_gzip))),
    );
    map.insert(
        "compressGzip".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_gzip))),
    );
    map.insert(
        "gunzip".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_gunzip))),
    );
    map.insert(
        "zstd".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_zstd))),
    );
    map.insert(
        "unzstd".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_unzstd))),
    );
    map.insert(
        "brotli".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_brotli))),
    );
    map.insert(
        "unbrotli".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_unbrotli))),
    );
    map.insert(
        "lz4".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_lz4))),
    );
    map.insert(
        "unlz4".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_unlz4))),
    );
    map.insert(
        "xz".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_xz))),
    );
    map.insert(
        "unxz".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_unxz))),
    );
    map.insert(
        "deflate".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_deflate))),
    );
    map.insert(
        "inflate".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_inflate))),
    );
    map.insert(
        "zlib".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_zlib))),
    );
    map.insert(
        "unzlib".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_unzlib))),
    );

    map.insert(
        "compressBound".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress_bound))),
    );
    map.insert(
        "compressAdaptive".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress_adaptive))),
    );
    map.insert(
        "auto".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress_adaptive))),
    );
    map.insert(
        "entropyEstimate".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_entropy_estimate))),
    );

    map.insert(
        "crc32".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_crc32))),
    );
    map.insert(
        "crc32c".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_crc32c))),
    );
    map.insert(
        "adler32".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_adler32))),
    );
    map.insert(
        "xxhash64".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_xxhash64))),
    );

    map.insert(
        "createSeekable".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_seekable))),
    );
    map.insert(
        "readSeekableChunk".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_seekable_chunk))),
    );

    map.insert(
        "createZip".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_zip))),
    );
    map.insert(
        "createZipFile".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_zip_file))),
    );
    map.insert(
        "extractZip".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_extract_zip))),
    );
    map.insert(
        "extractZipFile".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_extract_zip_file))),
    );
    map.insert(
        "createTar".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_tar))),
    );
    map.insert(
        "extractTar".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_extract_tar))),
    );

    map.insert(
        "parallelCompress".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_parallel_compress))),
    );
    map.insert(
        "parallelDecompress".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_parallel_decompress))),
    );

    map.insert(
        "compressFile".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress_file))),
    );
    map.insert(
        "decompressFile".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_decompress_file))),
    );
    map.insert(
        "compressFileAtomic".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_compress_file_atomic))),
    );
    map.insert(
        "detect".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_detect))),
    );
    map.insert(
        "decompressAuto".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_decompress_auto))),
    );

    Value::Object(Arc::new(map))
}
