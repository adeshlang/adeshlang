//! Phase 10 Reproducible Builds E2E Test Suite.
//!
//! Validates:
//! - Bit-for-bit deterministic fingerprinting across multiple evaluation passes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::ModuleFingerprint;
use std::collections::BTreeMap;

#[test]
fn test_deterministic_module_fingerprints() {
    let source = "fn compute_pi() -> f64 { 3.1415926535 }";
    let interface = "fn compute_pi() -> f64;";
    let flags = "-O3 --target=x86_64-pc-windows-msvc";

    let fp1 = ModuleFingerprint::compute("math", source, interface, flags, BTreeMap::new());
    let fp2 = ModuleFingerprint::compute("math", source, interface, flags, BTreeMap::new());

    assert_eq!(fp1.source_hash, fp2.source_hash);
    assert_eq!(fp1.interface_hash, fp2.interface_hash);
    assert_eq!(fp1.flags_hash, fp2.flags_hash);
    assert!(fp1.matches(&fp2));
}
