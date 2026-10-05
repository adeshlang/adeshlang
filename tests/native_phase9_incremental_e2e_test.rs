//! Phase 9 Incremental Compilation E2E Test Suite.
//!
//! Validates:
//! - ModuleFingerprint computation based on source hash, interface hash, flags, and dependency hashes.
//! - Fingerprint matching and public interface diff detection.
//! - IncrementalCache `should_rebuild` logic and update tracking.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::{IncrementalCache, ModuleFingerprint};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn test_incremental_module_fingerprint() {
    let mut deps = BTreeMap::new();
    deps.insert("dep_core".to_string(), 12345u64);

    let fp1 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 42 }",
        "pub fn calculate() -> i64",
        "-O2 --lto=thin",
        deps.clone(),
    );

    let fp2 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 42 }",
        "pub fn calculate() -> i64",
        "-O2 --lto=thin",
        deps.clone(),
    );

    // Identical inputs produce identical fingerprints
    assert!(fp1.matches(&fp2));
    assert_eq!(fp1.source_hash, fp2.source_hash);
    assert!(!fp1.public_interface_changed(&fp2));

    // Changing implementation details only (interface unchanged)
    let fp3 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { let x = 40 + 2; x }",
        "pub fn calculate() -> i64",
        "-O2 --lto=thin",
        deps.clone(),
    );
    assert!(!fp1.matches(&fp3));
    assert_ne!(fp1.source_hash, fp3.source_hash);
    assert!(!fp1.public_interface_changed(&fp3));

    // Changing public interface
    let fp4 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate(factor: i64) -> i64 { factor * 42 }",
        "pub fn calculate(factor: i64) -> i64",
        "-O2 --lto=thin",
        deps,
    );
    assert!(fp1.public_interface_changed(&fp4));
}

#[test]
fn test_incremental_cache_should_rebuild() {
    let dir = tempdir().expect("tempdir");
    let cache_dir = dir.path().join(".adesh_cache");
    let mut cache = IncrementalCache::new(&cache_dir);

    let fp_a = ModuleFingerprint::compute(
        "mod_a",
        "source code A",
        "interface A",
        "-O2",
        BTreeMap::new(),
    );

    // Initially uncompiled module must be rebuilt
    assert!(cache.should_rebuild(&fp_a));

    // After compilation, cache is updated
    cache.update(fp_a.clone());

    // Subsequent compilation check with identical fingerprint should not rebuild
    assert!(!cache.should_rebuild(&fp_a));

    // Modified source should trigger rebuild
    let fp_a_modified = ModuleFingerprint::compute(
        "mod_a",
        "source code A MODIFIED",
        "interface A",
        "-O2",
        BTreeMap::new(),
    );
    assert!(cache.should_rebuild(&fp_a_modified));
}
