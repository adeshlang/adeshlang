//! # Adesh Object (`adesh-object`)
//!
//! Universal ADOB (Adesh Native Object Binary) format specification, serializer,
//! deserializer, and validation library for the Adesh ecosystem.

#![allow(clippy::result_large_err)]

pub mod bundle;
pub mod capabilities;
pub mod error;
pub mod extension;
pub mod format;
pub mod metadata;
pub mod reader;
pub mod relocation;
pub mod section;
pub mod symbol;
pub mod target;
pub mod validator;
pub mod writer;

// Top-level re-exports
pub use bundle::{AdobBundle, BUNDLE_MAGIC, BUNDLE_VERSION, BundleEntry};
pub use capabilities::TargetCapabilities;
pub use error::{AdobError, AdobErrorCode, AdobResult};
pub use extension::{AdobExtension, ExtensionTable};
pub use format::{
    ADOB_MAGIC, ADOB_VERSION_MAJOR, ADOB_VERSION_MINOR, ADOB_VERSION_PATCH, AdobHeader, AdobObject,
};
pub use metadata::{
    AcceleratorArtifact, AcceleratorMetadata, BuildMetadata, DebugInfo, DebugLineRecord,
    DebugSourceFile, DebugVariable, DeviceMemoryModel, GpuKernelMetadata, MemoryOrderModel,
    OptimizationMetadata, SafetyMetadata, SecurityMetadata, TensorElementType, TensorMetadata,
    ThreadSafetyMetadata, TlsModel, UnwindFormat, UnwindMetadata,
};
pub use reader::AdobReader;
pub use relocation::{AdobRelocation, RelocationFlags, RelocationKind};
pub use section::{AdobSection, MemoryPermissions, MemoryRegion, SectionKind, section_flags};
pub use symbol::{AdobSymbol, SymbolBinding, SymbolKind, SymbolVisibility};
pub use target::{
    Abi, Architecture, ComputeDevice, EmbeddedArchitecture, Endianness, Environment,
    GpuArchitecture, NpuArchitecture, ObjectFormat, OperatingSystem, PointerWidth,
    TargetDescriptor, TargetFeature, TargetFeatures, TpuArchitecture,
};
pub use validator::AdobValidator;
pub use writer::AdobWriter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adob_roundtrip() {
        let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
        let mut obj = AdobObject::new(target);

        let text_data = vec![0xE8, 0x00, 0x00, 0x00, 0xC3]; // call rel32; ret
        let mut text_sec = AdobSection::new(".text", SectionKind::Text).with_data(text_data);
        text_sec.add_relocation(AdobRelocation::new(
            0,
            0,
            "main",
            RelocationKind::PcRelative32,
            -4,
        ));
        let sec_idx = obj.add_section(text_sec);

        let sym = AdobSymbol::new_defined(0, "main", SymbolKind::Function, sec_idx, 0, 5);
        obj.add_symbol(sym);
        obj.add_export("main");

        // Validate before write
        AdobValidator::validate(&obj).expect("Validation should succeed");

        // Write
        let encoded = AdobWriter::write(&obj).expect("Encoding should succeed");
        assert!(!encoded.is_empty());
        assert_eq!(&encoded[0..4], ADOB_MAGIC);

        // Read
        let decoded = AdobReader::read_object(&encoded).expect("Decoding should succeed");

        // Validate decoded
        AdobValidator::validate(&decoded).expect("Decoded object should be valid");

        assert_eq!(decoded.sections.len(), 1);
        assert_eq!(decoded.sections[0].name, ".text");
        assert_eq!(decoded.sections[0].data, vec![0xE8, 0x00, 0x00, 0x00, 0xC3]);
        assert_eq!(decoded.symbols.len(), 1);
        assert_eq!(decoded.symbols[0].name, "main");
        assert_eq!(decoded.exports, vec!["main".to_string()]);
    }

    #[test]
    fn test_adob_validator_detects_bad_magic() {
        let bad_bytes = b"BADM\x01\x00\x00\x00";
        let res = AdobReader::read_object(bad_bytes);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, AdobErrorCode::InvalidMagic);
    }
}
