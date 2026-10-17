//! Built-in Mach-O ad-hoc code signature generator.
//!
//! Generates a minimal `LC_CODE_SIGNATURE` blob containing a SuperBlob with a
//! single CodeDirectory (CSMAGIC_CODEDIRECTORY) that covers the binary in
//! 4096-byte pages using SHA-256 hashes. This satisfies Apple Silicon (ARM64)
//! and macOS 11+ requirements for ad-hoc signed executables.
//!
//! Reference: XNU `bsd/kern/ubc_info.c` and `security/mac_vnode.c`,
//! Apple Security framework `CodeSigning/CodeDirectory.h`.

use sha2::{Digest, Sha256};

// SuperBlob magic
const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xFADE0CC0;
const CSMAGIC_CODEDIRECTORY: u32 = 0xFADE0C02;

// CodeDirectory version
const CS_SUPPORTSSCATTER: u32 = 0x20100;
const CSVERSION: u32 = CS_SUPPORTSSCATTER;

// CodeDirectory flags
const CS_ADHOC: u32 = 0x0000_0002;

/// Page size used by the code signing verifier.
const PAGE_SIZE: usize = 4096;

/// Hash algorithm: SHA-256
const CS_HASHTYPE_SHA256: u8 = 2;
const SHA256_HASH_LEN: u8 = 32;

/// Load command number for an embedded code signature.
pub const LC_CODE_SIGNATURE: u32 = 0x1D;

/// Generate an ad-hoc code signature blob for a Mach-O binary.
///
/// `binary` is the complete binary bytes up to (but not including) the
/// signature blob itself. The returned bytes should be appended to the binary
/// and the `LC_CODE_SIGNATURE` load command must point to their offset/size.
///
/// # Arguments
/// * `binary` - The binary content to sign (everything before the sig blob)
/// * `identifier` - The bundle identifier string (e.g. the executable filename)
pub fn generate_adhoc_signature(binary: &[u8], identifier: &str) -> Vec<u8> {
    // Number of code pages (each page = 4096 bytes)
    let n_code_slots = (binary.len() + PAGE_SIZE - 1) / PAGE_SIZE;
    let n_special_slots: u32 = 0; // no special slots for ad-hoc

    // SHA-256 each 4096-byte page
    let mut code_hashes: Vec<[u8; 32]> = Vec::with_capacity(n_code_slots);
    for chunk in binary.chunks(PAGE_SIZE) {
        let mut hasher = Sha256::new();
        hasher.update(chunk);
        let hash = hasher.finalize();
        let mut slot = [0u8; 32];
        slot.copy_from_slice(&hash);
        code_hashes.push(slot);
    }

    // Build identifier bytes (NUL-terminated)
    let mut ident_bytes: Vec<u8> = identifier.as_bytes().to_vec();
    ident_bytes.push(0u8);

    // CodeDirectory fixed header is 48 bytes:
    // magic(4) + length(4) + version(4) + flags(4) + hashOffset(4) +
    // identOffset(4) + nSpecialSlots(4) + nCodeSlots(4) + codeLimit(4) +
    // hashSize(1) + hashType(1) + platform(1) + pageSize(1) + spare2(4) +
    // scatterOffset(4) = 48 bytes
    let fixed_header = 48usize;
    let ident_offset: u32 = fixed_header as u32; // identifier immediately follows the 48-byte header
    let hash_offset = (fixed_header + ident_bytes.len()) as u32;
    let cd_length = hash_offset as usize + (n_code_slots + n_special_slots as usize) * 32;

    let mut cd_buf: Vec<u8> = Vec::with_capacity(cd_length);

    // magic
    cd_buf.extend_from_slice(&CSMAGIC_CODEDIRECTORY.to_be_bytes());
    // length
    cd_buf.extend_from_slice(&(cd_length as u32).to_be_bytes());
    // version
    cd_buf.extend_from_slice(&CSVERSION.to_be_bytes());
    // flags = CS_ADHOC
    cd_buf.extend_from_slice(&CS_ADHOC.to_be_bytes());
    // hashOffset (offset from start of CodeDirectory to first hash slot)
    cd_buf.extend_from_slice(&hash_offset.to_be_bytes());
    // identOffset
    cd_buf.extend_from_slice(&ident_offset.to_be_bytes());
    // nSpecialSlots
    cd_buf.extend_from_slice(&(n_special_slots as u32).to_be_bytes());
    // nCodeSlots
    cd_buf.extend_from_slice(&(n_code_slots as u32).to_be_bytes());
    // codeLimit (total bytes of the binary covered)
    cd_buf.extend_from_slice(&(binary.len() as u32).to_be_bytes());
    // hashSize
    cd_buf.push(SHA256_HASH_LEN);
    // hashType
    cd_buf.push(CS_HASHTYPE_SHA256);
    // platform = 0 (ad-hoc)
    cd_buf.push(0u8);
    // pageSize = log2(4096) = 12
    cd_buf.push(12u8);
    // spare2
    cd_buf.extend_from_slice(&0u32.to_be_bytes());
    // scatterOffset = 0
    cd_buf.extend_from_slice(&0u32.to_be_bytes());

    // identifier
    cd_buf.extend_from_slice(&ident_bytes);

    // code slots
    for hash in &code_hashes {
        cd_buf.extend_from_slice(hash);
    }

    assert_eq!(cd_buf.len(), cd_length);

    // SuperBlob with one blob (the CodeDirectory)
    // SuperBlob: magic(4) + length(4) + count(4) + [type(4) + offset(4)] + blob
    let blob_index_size = 8usize; // type(4) + offset(4)
    let superblob_header = 12usize + blob_index_size; // magic+length+count + one index entry
    let superblob_total = superblob_header + cd_length;

    let mut sb_buf: Vec<u8> = Vec::with_capacity(superblob_total);
    // SuperBlob magic
    sb_buf.extend_from_slice(&CSMAGIC_EMBEDDED_SIGNATURE.to_be_bytes());
    // SuperBlob total length
    sb_buf.extend_from_slice(&(superblob_total as u32).to_be_bytes());
    // count = 1
    sb_buf.extend_from_slice(&1u32.to_be_bytes());
    // Index entry: type = CSSLOT_CODEDIRECTORY (0), offset = superblob_header
    sb_buf.extend_from_slice(&0u32.to_be_bytes()); // CSSLOT_CODEDIRECTORY = 0
    sb_buf.extend_from_slice(&(superblob_header as u32).to_be_bytes());
    // The CodeDirectory blob
    sb_buf.extend_from_slice(&cd_buf);

    assert_eq!(sb_buf.len(), superblob_total);
    sb_buf
}

/// Post-process a fully encoded Mach-O binary to embed an ad-hoc code
/// signature, satisfying XNU's requirement for ARM64 / macOS 11+ executables.
///
/// The approach:
/// 1. Align the binary to 16 bytes.
/// 2. Record `sig_offset` = current end.
/// 3. Generate the signature blob covering `binary[0..sig_offset]`.
/// 4. Find LC_CODE_SIGNATURE in the existing load commands and patch its
///    `dataoff`/`datasize` fields; OR if no such command exists, scan for
///    8+ bytes of zero padding after the last real load command (the header
///    area always has room because we pre-reserve nothing — dyld accepts a
///    binary whose sizeofcmds doesn't cover LC_CODE_SIGNATURE as long as the
///    header magic, ncmds and the signature region are valid).
///
/// Because the encoder doesn't pre-reserve LC_CODE_SIGNATURE, this function
/// locates free space in the header page (bytes between end-of-load-commands
/// and the first section payload) and writes the 16-byte command there,
/// incrementing `ncmds` and `sizeofcmds` in the Mach-O header.
///
/// Returns the patched binary with the signature appended.
pub fn apply_to_macho_bytes(bytes: &mut Vec<u8>, identifier: &str) -> Result<(), String> {
    // Align to 16 bytes before computing the signature.
    while bytes.len() % 16 != 0 {
        bytes.push(0);
    }
    let sig_offset = bytes.len() as u32;

    // Generate signature over everything up to sig_offset.
    let sig_blob = generate_adhoc_signature(&bytes[..sig_offset as usize], identifier);
    let sig_size = sig_blob.len() as u32;

    // Validate minimal Mach-O header (magic + cputype + cpusubtype + filetype
    // + ncmds + sizeofcmds + flags + reserved = 32 bytes).
    if bytes.len() < 32 {
        return Err("binary too small to be a Mach-O".into());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if magic != 0xFEEDFACFu32 {
        return Err(format!("unexpected Mach-O magic: 0x{:08X}", magic));
    }

    let ncmds_off = 16usize;
    let sizeofcmds_off = 20usize;
    let mut ncmds = u32::from_le_bytes(bytes[ncmds_off..ncmds_off + 4].try_into().unwrap());
    let mut sizeofcmds =
        u32::from_le_bytes(bytes[sizeofcmds_off..sizeofcmds_off + 4].try_into().unwrap());

    // Scan load commands for an existing LC_CODE_SIGNATURE.
    let cmd_area_start = 32usize;
    let cmd_area_end = cmd_area_start + sizeofcmds as usize;
    if cmd_area_end > bytes.len() {
        return Err("sizeofcmds extends beyond file".into());
    }

    let mut pos = cmd_area_start;
    let mut existing_lc_off: Option<usize> = None;
    while pos + 8 <= cmd_area_end {
        let cmd = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
        let cmdsize = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if cmdsize == 0 {
            break;
        }
        if cmd == LC_CODE_SIGNATURE {
            existing_lc_off = Some(pos);
            break;
        }
        pos += cmdsize;
    }

    if let Some(lc_off) = existing_lc_off {
        // Patch existing LC_CODE_SIGNATURE dataoff/datasize.
        bytes[lc_off + 8..lc_off + 12].copy_from_slice(&sig_offset.to_le_bytes());
        bytes[lc_off + 12..lc_off + 16].copy_from_slice(&sig_size.to_le_bytes());
    } else {
        // Find room after the last load command to insert LC_CODE_SIGNATURE (16 bytes).
        let free_start = cmd_area_end;
        // The first section payload starts at the file offset stored in the first
        // section header. However we can conservatively use the actual first non-zero
        // byte after the command area as the upper bound.
        // Use a simpler bound: the file's first page (0x4000 bytes) is the header page
        // on macOS. We just need 16 free bytes.
        if free_start + 16 > bytes.len() {
            return Err("no room in header to insert LC_CODE_SIGNATURE".into());
        }
        // Verify the 16 bytes are zero (padding / unused header space).
        if bytes[free_start..free_start + 16].iter().any(|&b| b != 0) {
            return Err(
                "header padding after load commands is non-zero; cannot insert LC_CODE_SIGNATURE"
                    .into(),
            );
        }
        // Write LC_CODE_SIGNATURE at free_start.
        bytes[free_start..free_start + 4].copy_from_slice(&LC_CODE_SIGNATURE.to_le_bytes());
        bytes[free_start + 4..free_start + 8].copy_from_slice(&16u32.to_le_bytes()); // cmdsize=16
        bytes[free_start + 8..free_start + 12].copy_from_slice(&sig_offset.to_le_bytes());
        bytes[free_start + 12..free_start + 16].copy_from_slice(&sig_size.to_le_bytes());
        // Update ncmds and sizeofcmds in the header.
        ncmds += 1;
        sizeofcmds += 16;
        bytes[ncmds_off..ncmds_off + 4].copy_from_slice(&ncmds.to_le_bytes());
        bytes[sizeofcmds_off..sizeofcmds_off + 4].copy_from_slice(&sizeofcmds.to_le_bytes());
    }

    bytes.extend_from_slice(&sig_blob);
    Ok(())
}
