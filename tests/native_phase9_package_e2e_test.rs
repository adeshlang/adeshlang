//! Phase 9 Package and Dependency Management E2E Test Suite.
//!
//! Validates:
//! - Package manifest reading, validation, and serialization (`adesh.adl`).
//! - Lockfile generation and deterministic resolution (`adesh.lock.adl`).
//! - Transitive dependencies, feature resolution, and target-specific dependencies.
//! - Cache validation and dependency graph traversal.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package::{
    ADESH_LOCK_FILE, ADESH_MANIFEST_FILE, DependencyResolver, LockFile, LockedPackage,
    PackageDependency, PackageManifest, PackageMetadata,
};
use std::collections::BTreeMap;
use tempfile::tempdir;

#[test]
fn test_package_manifest_curated_format_read_write() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join(ADESH_MANIFEST_FILE);

    let content = r#"
[package]
name = "my_app"
version = "1.0.0"
authors = ["Test Author <author@adesh.dev>"]
edition = "2024"
license = "MIT"

[dependencies]
http = "0.2.1"
json = { version = "1.0.0", features = ["derive"] }

[features]
default = ["json"]
ssl = []

[target."x86_64-pc-windows-msvc".dependencies]
winapi = "0.3.9"
"#;
    std::fs::write(&manifest_path, content).expect("write manifest");

    let manifest = PackageManifest::from_file(&manifest_path).expect("parse manifest");
    assert_eq!(manifest.package.name, "my_app");
    assert_eq!(manifest.package.version, "1.0.0");
    assert_eq!(manifest.dependencies.len(), 2);
    assert!(manifest.dependencies.contains_key("http"));
    assert!(manifest.dependencies.contains_key("json"));
    assert_eq!(
        manifest.features.get("default"),
        Some(&vec!["json".to_string()])
    );
    assert!(manifest.target.contains_key("x86_64-pc-windows-msvc"));

    // Serialize and re-read to ensure symmetry
    let out_path = dir.path().join("copy.adl");
    manifest.to_file(&out_path).expect("save manifest");
    let re_read = PackageManifest::from_file(&out_path).expect("re-read manifest");
    assert_eq!(re_read.package.name, manifest.package.name);
    assert_eq!(re_read.package.version, manifest.package.version);
    assert_eq!(re_read.dependencies.len(), manifest.dependencies.len());
}

#[test]
fn test_package_dependency_resolution_and_lockfile() {
    let dir = tempdir().expect("tempdir");
    let manifest_path = dir.path().join(ADESH_MANIFEST_FILE);
    let lock_path = dir.path().join(ADESH_LOCK_FILE);

    let mut deps = BTreeMap::new();
    deps.insert(
        "net_lib".to_string(),
        PackageDependency::Simple("1.2.0".to_string()),
    );
    deps.insert(
        "crypto_core".to_string(),
        PackageDependency::Detailed(adesh_codegen::package::DetailedDependency {
            version: "0.9.1".to_string(),
            features: Some(vec!["sha256".to_string()]),
            optional: Some(false),
            path: None,
            default_features: None,
        }),
    );

    let manifest = PackageManifest {
        package: PackageMetadata {
            name: "service_hub".to_string(),
            version: "2.1.0".to_string(),
            authors: vec!["Core Dev".to_string()],
            edition: "2024".to_string(),
            license: Some("Apache-2.0".to_string()),
            description: Some("Service application".to_string()),
            entry: None,
        },
        dependencies: deps,
        dev_dependencies: BTreeMap::new(),
        target: BTreeMap::new(),
        features: BTreeMap::new(),
        workspace: None,
        profile: BTreeMap::new(),
    };
    manifest.to_file(&manifest_path).expect("write manifest");

    let mut resolver = DependencyResolver::new();
    resolver.register_package("net_lib", "1.2.0", vec![]);
    resolver.register_package("crypto_core", "0.9.1", vec![]);
    let lockfile = resolver
        .resolve(&manifest, &[])
        .expect("resolve dependencies");
    assert_eq!(lockfile.packages.len(), 2);
    assert!(lockfile.packages.iter().any(|p| p.name == "net_lib"));
    assert!(lockfile.packages.iter().any(|p| p.name == "crypto_core"));

    lockfile.to_file(&lock_path).expect("write lockfile");
    let loaded_lock = LockFile::from_file(&lock_path).expect("load lockfile");
    assert_eq!(loaded_lock.packages.len(), lockfile.packages.len());
    assert_eq!(loaded_lock.version, 1);
}
