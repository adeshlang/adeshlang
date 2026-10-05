//! Phase 10 Package Security & Sandboxing E2E Test Suite.
//!
//! Validates:
//! - Package sandbox permission enforcement.
//! - Checksum integrity validation.

#![allow(dead_code, unused_imports)]

use adesh_codegen::package_security::{PackageSandboxPermissions, PackageSecurityValidator};

#[test]
fn test_package_sandbox_permission_enforcement() {
    let mut perms = PackageSandboxPermissions::default();
    perms.allow_network = false;
    perms.allow_filesystem_write = false;

    // Disallowed network access
    let net_res = PackageSecurityValidator::check_permission(&perms, "network_connect");
    assert!(net_res.is_err());

    // Disallowed fs write
    let fs_res = PackageSecurityValidator::check_permission(&perms, "fs_write");
    assert!(fs_res.is_err());

    // Allowed read operation
    let read_res = PackageSecurityValidator::check_permission(&perms, "fs_read");
    assert!(read_res.is_ok());
}

#[test]
fn test_package_checksum_verification() {
    let artifact = b"test package binary payload";
    let is_valid = PackageSecurityValidator::verify_checksum(artifact, "adob_sha256_mock");
    assert!(is_valid);
}
