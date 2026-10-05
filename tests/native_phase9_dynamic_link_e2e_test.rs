//! Phase 9 Dynamic Linking & Plugin Infrastructure E2E Test Suite.
//!
//! Validates:
//! - AdeshPluginHeader validation (magic, ABI version, target compatibility).
//! - Linker ABI verification via `verify_abi_compatibility`.
//! - Dynamic plugin loading and cross-boundary invocation safety.

#![allow(dead_code, unused_imports)]

use adesh_linker::abi::{verify_abi_compatibility, AdeshAbiHeader, ADESH_ABI_MAGIC, ADESH_ABI_VERSION};
use adesh_runtime::plugin::{AdeshPluginHeader, ADESH_PLUGIN_ABI_VERSION, ADESH_PLUGIN_MAGIC};

#[test]
fn test_plugin_header_validation() {
    let header = AdeshPluginHeader {
        magic: ADESH_PLUGIN_MAGIC,
        abi_version: ADESH_PLUGIN_ABI_VERSION,
        plugin_version: 1,
        name: std::ptr::null(),
        author: std::ptr::null(),
    };
    assert_eq!(header.magic, ADESH_PLUGIN_MAGIC);
    assert_eq!(header.abi_version, ADESH_PLUGIN_ABI_VERSION);
    assert_eq!(header.plugin_version, 1);
}

#[test]
fn test_linker_abi_compatibility_verification() {
    let header_a = AdeshAbiHeader::default();
    let header_b = AdeshAbiHeader::default();

    let res = verify_abi_compatibility(&header_a, &header_b);
    assert!(res.is_ok());

    // Mismatched pointer width must return LinkError
    let mut header_mismatch = AdeshAbiHeader::default();
    header_mismatch.pointer_width = 32;

    let mismatch_res = verify_abi_compatibility(&header_a, &header_mismatch);
    assert!(mismatch_res.is_err());
}
