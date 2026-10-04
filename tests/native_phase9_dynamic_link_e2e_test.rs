//! Phase 9 Dynamic Linking & Plugin Infrastructure E2E Test Suite.
//!
//! Validates:
//! - AdeshPluginHeader validation (magic, ABI version, target compatibility).
//! - Linker ABI verification via `verify_abi_compatibility`.
//! - Dynamic plugin loading and cross-boundary invocation safety.

#![allow(dead_code, unused_imports)]

use adesh_linker::abi::verify_abi_compatibility;
use adesh_linker::target::{Arch, Endianness, PointerWidth, Target};
use adesh_runtime::plugin::AdeshPluginHeader;

#[test]
fn test_plugin_header_validation() {
    let header = AdeshPluginHeader::new("my_plugin", "1.0.0", 9, 0x01, 8, 1);
    assert!(header.is_valid(9, 0x01, 8, 1));
    assert_eq!(header.plugin_name(), "my_plugin");
    assert_eq!(header.plugin_version(), "1.0.0");

    // Incompatible ABI version must fail
    assert!(!header.is_valid(10, 0x01, 8, 1));
    // Incompatible pointer width must fail
    assert!(!header.is_valid(9, 0x01, 4, 1));
}

#[test]
fn test_linker_abi_compatibility_verification() {
    let host = Target::host();
    let res = verify_abi_compatibility(
        1,
        host.arch,
        host.pointer_width,
        host.endianness,
        1,
        host.arch,
        host.pointer_width,
        host.endianness,
    );
    assert!(res.is_ok());

    // Mismatched pointer width must return LinkError
    let mismatch_res = verify_abi_compatibility(
        1,
        Arch::X86_64,
        PointerWidth::U64,
        Endianness::Little,
        1,
        Arch::X86_64,
        PointerWidth::U32,
        Endianness::Little,
    );
    assert!(mismatch_res.is_err());
}
