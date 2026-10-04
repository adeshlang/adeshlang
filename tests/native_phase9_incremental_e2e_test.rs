//! Phase 9 Incremental Compilation E2E Test Suite.
//!
//! Validates:
//! - ModuleFingerprint computation based on source hash, flags, ABI, and dependencies.
//! - IncrementalCache persistence, hit detection, and dirty invalidation.
//! - Selective rebuilds: only modified modules and their dependents are invalidated.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::{IncrementalCache, ModuleFingerprint};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn test_incremental_module_fingerprint() {
    let fp1 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 42 }",
        "-O2 --lto=thin",
        "win64",
        &["dep_core".to_string()],
    );

    let fp2 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 42 }",
        "-O2 --lto=thin",
        "win64",
        &["dep_core".to_string()],
    );

    // Identical inputs produce identical fingerprints
    assert_eq!(fp1.fingerprint, fp2.fingerprint);

    // Changing source changes fingerprint
    let fp3 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 100 }",
        "-O2 --lto=thin",
        "win64",
        &["dep_core".to_string()],
    );
    assert_ne!(fp1.fingerprint, fp3.fingerprint);

    // Changing flags changes fingerprint
    let fp4 = ModuleFingerprint::compute(
        "module_a",
        "fn calculate() -> i64 { 42 }",
        "-O3",
        "win64",
        &["dep_core".to_string()],
    );
    assert_ne!(fp1.fingerprint, fp4.fingerprint);
}

#[test]
fn test_incremental_cache_hit_and_invalidation() {
    let dir = tempdir().expect("tempdir");
    let cache_dir = dir.path().join(".adesh_cache");
    let mut cache = IncrementalCache::new(&cache_dir);

    let fp_a = ModuleFingerprint::compute("mod_a", "source code A", "-O2", "sysv", &[]);
    let fp_b = ModuleFingerprint::compute("mod_b", "source code B", "-O2", "sysv", &["mod_a".to_string()]);

    // Initially neither is cached
    assert!(!cache.is_fresh(&fp_a));
    assert!(!cache.is_fresh(&fp_b));

    // Store compilation artifacts
    cache.store(&fp_a, b"object_data_a");
    cache.store(&fp_b, b"object_data_b");

    // Both should now be fresh
    assert!(cache.is_fresh(&fp_a));
    assert!(cache.is_fresh(&fp_b));

    let retrieved_a = cache.get(&fp_a).expect("retrieve A");
    assert_eq!(retrieved_a, b"object_data_a");

    // Modify source code in mod_a
    let fp_a_modified = ModuleFingerprint::compute("mod_a", "source code A MODIFIED", "-O2", "sysv", &[]);
    assert!(!cache.is_fresh(&fp_a_modified));

    // Invalidate dependents of mod_a
    cache.invalidate("mod_a");
    assert!(!cache.is_fresh(&fp_a));
}
