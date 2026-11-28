//! AdobBundle: Fat binary and multi-architecture / heterogeneous container.

use crate::error::{AdobError, AdobErrorCode, AdobResult};
use crate::format::AdobObject;
use crate::reader::AdobReader;
use crate::target::{Architecture, ComputeDevice, TargetDescriptor};
use crate::validator::AdobValidator;
use crate::writer::AdobWriter;

pub const BUNDLE_MAGIC: &[u8; 4] = b"ADBB";
pub const BUNDLE_VERSION: u16 = 1;
pub const MAX_BUNDLE_ENTRIES: usize = 256;

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Entry in an ADOB fat bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    pub target: TargetDescriptor,
    pub object: AdobObject,
}

/// Heterogeneous / Multi-architecture Fat Binary bundle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdobBundle {
    pub entries: Vec<BundleEntry>,
}

impl AdobBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(&mut self, object: AdobObject) {
        let target = object.target.clone();
        self.entries.push(BundleEntry { target, object });
    }

    /// Find an object that matches the requested target architecture and device.
    pub fn find_matching(&self, arch: &Architecture, device: ComputeDevice) -> Option<&AdobObject> {
        self.entries
            .iter()
            .find(|e| &e.target.architecture == arch && e.target.device == device)
            .map(|e| &e.object)
    }

    /// Find an object matching the full target descriptor (architecture, OS, ABI,
    /// endianness, and compute device). Never returns an archive built for a
    /// different OS or ABI even if the CPU architecture matches.
    pub fn find_for_target(&self, target: &TargetDescriptor) -> Option<&AdobObject> {
        self.entries
            .iter()
            .find(|e| {
                e.target.architecture == target.architecture
                    && e.target.operating_system == target.operating_system
                    && e.target.abi == target.abi
                    && e.target.endianness == target.endianness
                    && e.target.device == target.device
            })
            .map(|e| &e.object)
    }

    /// Find all accelerator modules (GPU, NPU, TPU) in the bundle.
    pub fn find_accelerators(&self) -> Vec<&AdobObject> {
        self.entries
            .iter()
            .filter(|e| e.target.device != ComputeDevice::Cpu)
            .map(|e| &e.object)
            .collect()
    }

    /// Encode this bundle deterministically with an integrity digest.
    pub fn write(&self) -> AdobResult<Vec<u8>> {
        if self.entries.len() > MAX_BUNDLE_ENTRIES {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMetadata,
                format!(
                    "ADOB bundle entry count {} exceeds maximum {}",
                    self.entries.len(),
                    MAX_BUNDLE_ENTRIES
                ),
            ));
        }

        // Encode entries first and sort deterministically by target triple / key.
        let mut encoded_entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            AdobValidator::validate(&entry.object)?;
            let key = format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}",
                entry.target.architecture,
                entry.target.operating_system,
                entry.target.environment,
                entry.target.abi,
                entry.target.device
            );
            if encoded_entries.iter().any(|(k, _)| k == &key) {
                return Err(AdobError::new(
                    AdobErrorCode::DuplicateSymbol,
                    format!("Duplicate bundle entry for target `{}`", key),
                ));
            }
            let obj_bytes = AdobWriter::write(&entry.object)?;
            encoded_entries.push((key, obj_bytes));
        }
        encoded_entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut buf = Vec::new();
        buf.extend_from_slice(BUNDLE_MAGIC);
        buf.extend_from_slice(&BUNDLE_VERSION.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved flags
        buf.extend_from_slice(&(encoded_entries.len() as u32).to_le_bytes());

        for (_, obj_bytes) in &encoded_entries {
            let len_u32 = u32::try_from(obj_bytes.len()).map_err(|_| {
                AdobError::new(
                    AdobErrorCode::IntegerOverflow,
                    "ADOB bundle entry size exceeds u32::MAX",
                )
            })?;
            buf.extend_from_slice(&len_u32.to_le_bytes());
            buf.extend_from_slice(obj_bytes);
        }

        let digest = fnv1a_64(&buf);
        buf.extend_from_slice(&digest.to_le_bytes());
        Ok(buf)
    }

    /// Decode and validate a serialized ADOB bundle, verifying magic, version,
    /// trailing FNV-1a integrity digest, bounds, and every embedded ADOB object.
    pub fn read(bytes: &[u8]) -> AdobResult<Self> {
        const HEADER_LEN: usize = 4 + 2 + 4 + 4;
        const TRAILER_LEN: usize = 8;
        if bytes.len() < HEADER_LEN + TRAILER_LEN {
            return Err(AdobError::new(
                AdobErrorCode::BufferUnderflow,
                format!(
                    "ADOB bundle is truncated ({} bytes, need at least {})",
                    bytes.len(),
                    HEADER_LEN + TRAILER_LEN
                ),
            ));
        }
        if &bytes[0..4] != BUNDLE_MAGIC {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMagic,
                format!("Invalid ADOB bundle magic: {:?}", &bytes[0..4]),
            ));
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != BUNDLE_VERSION {
            return Err(AdobError::new(
                AdobErrorCode::UnsupportedVersion,
                format!(
                    "Unsupported ADOB bundle version {}; expected {}",
                    version, BUNDLE_VERSION
                ),
            ));
        }

        let payload_end = bytes.len() - TRAILER_LEN;
        let expected_digest = u64::from_le_bytes(
            bytes[payload_end..bytes.len()]
                .try_into()
                .map_err(|_| AdobError::new(AdobErrorCode::BufferUnderflow, "Missing digest"))?,
        );
        let actual_digest = fnv1a_64(&bytes[..payload_end]);
        if actual_digest != expected_digest {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMetadata,
                format!(
                    "ADOB bundle integrity check failed (expected 0x{:016X}, got 0x{:016X})",
                    expected_digest, actual_digest
                ),
            ));
        }

        let count = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
        if count > MAX_BUNDLE_ENTRIES {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMetadata,
                format!(
                    "ADOB bundle entry count {} exceeds maximum {}",
                    count, MAX_BUNDLE_ENTRIES
                ),
            ));
        }

        let mut offset = HEADER_LEN;
        let mut bundle = Self::new();
        for idx in 0..count {
            if offset + 4 > payload_end {
                return Err(AdobError::new(
                    AdobErrorCode::BufferUnderflow,
                    format!("Truncated entry length at bundle index {}", idx),
                )
                .with_offset(offset));
            }
            let obj_len = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;
            offset += 4;
            let obj_end = offset.checked_add(obj_len).ok_or_else(|| {
                AdobError::new(
                    AdobErrorCode::IntegerOverflow,
                    "ADOB bundle entry length overflow",
                )
            })?;
            if obj_end > payload_end {
                return Err(AdobError::new(
                    AdobErrorCode::BufferUnderflow,
                    format!(
                        "ADOB bundle entry {} length {} exceeds payload bounds",
                        idx, obj_len
                    ),
                )
                .with_offset(offset));
            }
            let obj = AdobReader::read_object(&bytes[offset..obj_end])?;
            AdobValidator::validate(&obj)?;
            if bundle.find_for_target(&obj.target).is_some() {
                return Err(AdobError::new(
                    AdobErrorCode::DuplicateSymbol,
                    format!("Duplicate target entry in ADOB bundle: {:?}", obj.target),
                ));
            }
            bundle.add_object(obj);
            offset = obj_end;
        }

        if offset != payload_end {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMetadata,
                format!(
                    "Trailing {} unexpected bytes in ADOB bundle payload",
                    payload_end - offset
                ),
            )
            .with_offset(offset));
        }

        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdobSection, SectionKind};

    #[test]
    fn bundle_roundtrip_and_corruption_rejection() {
        let mut b = AdobBundle::new();
        let linux_t = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
        let win_t = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap();

        let mut obj1 = AdobObject::new(linux_t.clone());
        obj1.add_section(AdobSection::new(".text", SectionKind::Text).with_data(vec![0xC3]));
        let mut obj2 = AdobObject::new(win_t.clone());
        obj2.add_section(AdobSection::new(".text", SectionKind::Text).with_data(vec![0xC3]));

        b.add_object(obj1);
        b.add_object(obj2);

        let bytes = b.write().expect("bundle write");
        let parsed = AdobBundle::read(&bytes).expect("bundle read");
        assert_eq!(parsed.entries.len(), 2);
        assert!(parsed.find_for_target(&linux_t).is_some());
        assert!(parsed.find_for_target(&win_t).is_some());

        let arm_t = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap();
        assert!(parsed.find_for_target(&arm_t).is_none());

        // Flip a payload byte -> integrity check must reject it.
        let mut corrupt = bytes.clone();
        corrupt[16] ^= 0xFF;
        let err = AdobBundle::read(&corrupt).expect_err("corrupt bundle must fail");
        assert_eq!(err.code, AdobErrorCode::InvalidMetadata);
    }
}
