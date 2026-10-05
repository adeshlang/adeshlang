//! Phase 10 Dynamic Linking & Runtime ABI Stability E2E Test Suite.
//!
//! Validates:
//! - AdeshRuntimeAbiV1 verification across compatible and incompatible configurations.
//! - Binary analysis on ADOB objects.

#![allow(dead_code, unused_imports)]

use adesh_codegen::binary_tools::BinaryAnalyzer;
use adesh_runtime::abi_v1::{AdeshRuntimeAbiV1, ADESH_RUNTIME_ABI_VERSION_1};

#[test]
fn test_runtime_abi_v1_compatibility() {
    let host_abi = AdeshRuntimeAbiV1::current();
    assert_eq!(host_abi.abi_version, ADESH_RUNTIME_ABI_VERSION_1);

    let identical_abi = host_abi.clone();
    assert!(host_abi.is_compatible(&identical_abi));

    let mut incompatible_abi = host_abi.clone();
    incompatible_abi.abi_version = 999;
    assert!(!host_abi.is_compatible(&incompatible_abi));
}

#[test]
fn test_binary_analysis_on_adob() {
    let analyzer = BinaryAnalyzer::new();
    let mut adob_bytes = Vec::new();
    adob_bytes.extend_from_slice(b"ADOB");
    adob_bytes.resize(128, 0);

    let report = analyzer.analyze_adob(&adob_bytes).expect("analyze adob");
    assert_eq!(report.format, "ADOB Object");
    assert_eq!(report.total_text_size, 128);
    assert!(!report.symbols.is_empty());
}
