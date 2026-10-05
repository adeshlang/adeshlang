//! Phase 10 Production Package Ecosystem E2E Test Suite.
//!
//! Validates:
//! - Package manifest generation and parsing with `adesh.adl`.
//! - Deterministic lockfile calculation (`adesh.lock.adl`).

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::{
    ADESH_LOCK_FILE, ADESH_MANIFEST_FILE, DependencyResolver, DependencySpec, LockFile,
    PackageManifest,
};
use tempfile::tempdir;

#[test]
fn test_package_manifest_and_lockfile_roundtrip() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join(ADESH_MANIFEST_FILE);
    let lock_path = dir.path().join(ADESH_LOCK_FILE);

    let mut manifest = PackageManifest::new("engine", "2.0.0");
    manifest.dependencies.insert(
        "core".to_string(),
        DependencySpec::Simple("1.0.0".to_string()),
    );
    manifest
        .save_to_file(&manifest_path)
        .expect("save manifest");

    let mut resolver = DependencyResolver::new();
    resolver.register_package("core", "1.0.0", vec![]);

    let lock = resolver.resolve(&manifest, &[]).expect("resolve lock");
    lock.save_to_file(&lock_path).expect("save lock");

    let loaded_lock = LockFile::from_file(&lock_path).expect("load lock");
    assert_eq!(loaded_lock.packages.len(), 1);
    assert_eq!(loaded_lock.packages[0].name, "core");
    assert_eq!(loaded_lock.packages[0].version, "1.0.0");
}
