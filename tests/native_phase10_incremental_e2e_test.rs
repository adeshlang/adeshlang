//! Phase 10 Incremental Compilation Cache E2E Test Suite.
//!
//! Validates:
//! - Module fingerprinting across source and public interface signatures.
//! - Accurate cache hit vs rebuild decisions.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::{IncrementalCache, ModuleFingerprint};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn test_incremental_cache_invalidation() {
    let dir = tempdir().expect("tempdir");
    let mut cache = IncrementalCache::new(dir.path());

    let fp1 = ModuleFingerprint::compute(
        "parser",
        "fn parse() -> i32 { 1 }",
        "fn parse() -> i32;",
        "-O2",
        BTreeMap::new(),
    );

    assert!(cache.should_rebuild(&fp1));
    cache.update(fp1.clone());
    assert!(!cache.should_rebuild(&fp1));

    // Internal change only
    let fp2 = ModuleFingerprint::compute(
        "parser",
        "fn parse() -> i32 { 2 }",
        "fn parse() -> i32;",
        "-O2",
        BTreeMap::new(),
    );

    assert!(cache.should_rebuild(&fp2));
    assert!(!fp1.public_interface_changed(&fp2));
}
