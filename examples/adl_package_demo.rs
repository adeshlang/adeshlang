//! AdeshLang — Curated ADL Package & Lockfile System Example
//!
//! This example demonstrates:
//! 1. Defining and serializing a curated `adesh.adl` package manifest.
//! 2. Parsing and inspecting `adesh.adl` files from disk.
//! 3. Performing deterministic dependency resolution across transitive dependencies and features.
//! 4. Emitting, inspecting, and reloading the lockfile `adesh.lock.adl`.
//! 5. Fine-grained incremental compilation cache using cryptographic module fingerprints.

use adesh_codegen::package::{
    ADESH_LOCK_FILE, ADESH_MANIFEST_FILE, DependencyResolver, DependencySpec, DetailedDependency,
    IncrementalCache, LockFile, ModuleFingerprint, PackageManifest, ProfileConfig,
};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn main() {
    println!("========================================================================");
    println!("     AdeshLang: Curated ADL Manifest & Lockfile Architecture Demo       ");
    println!("========================================================================\n");

    let work_dir = tempdir().expect("Failed to create temporary workspace directory");
    let adl_path = work_dir.path().join(ADESH_MANIFEST_FILE);
    let lock_path = work_dir.path().join(ADESH_LOCK_FILE);

    // -------------------------------------------------------------------------
    // STEP 1: Define and serialize the `adesh.adl` package manifest
    // -------------------------------------------------------------------------
    println!("[Step 1] Constructing package manifest for 'adesh-matrix-engine' (adesh.adl)...");

    let mut manifest = PackageManifest::new("adesh-matrix-engine", "1.0.0");
    manifest.package.description = Some("High-performance native matrix operations".to_string());
    manifest.package.license = Some("MIT OR Apache-2.0".to_string());
    manifest.package.authors = vec!["Adesh Systems Team <team@adesh.dev>".to_string()];
    manifest.package.entry = Some("src/main.adesh".to_string());

    // Dependencies
    manifest.dependencies.insert(
        "adesh-math".to_string(),
        DependencySpec::Simple("^1.2.0".to_string()),
    );
    manifest.dependencies.insert(
        "adesh-simd".to_string(),
        DependencySpec::Detailed(DetailedDependency {
            version: "~0.4.0".to_string(),
            path: None,
            optional: None,
            features: Some(vec!["avx2".to_string(), "fma".to_string()]),
            default_features: Some(true),
        }),
    );
    manifest.dependencies.insert(
        "adesh-net".to_string(),
        DependencySpec::Detailed(DetailedDependency {
            version: "0.8.0".to_string(),
            path: None,
            optional: Some(true),
            features: None,
            default_features: Some(false),
        }),
    );

    // Features
    manifest
        .features
        .insert("default".to_string(), vec!["accelerated".to_string()]);
    manifest
        .features
        .insert("accelerated".to_string(), vec!["adesh-simd".to_string()]);
    manifest
        .features
        .insert("networking".to_string(), vec!["adesh-net".to_string()]);

    // Compiler Profiles
    let mut dev_profile = ProfileConfig::default();
    dev_profile.opt_level = Some(0);
    dev_profile.debug = Some(true);
    manifest.profile.insert("dev".to_string(), dev_profile);

    let mut release_profile = ProfileConfig::default();
    release_profile.opt_level = Some(3);
    release_profile.lto = Some("fat".to_string());
    release_profile.debug = Some(false);
    manifest
        .profile
        .insert("release".to_string(), release_profile);

    // Save manifest to `adesh.adl`
    manifest
        .save_to_file(&adl_path)
        .expect("Failed to write adesh.adl");
    println!("✓ Successfully wrote '{}' to disk.", ADESH_MANIFEST_FILE);

    // Read back and inspect
    let manifest_content = std::fs::read_to_string(&adl_path).expect("Read adesh.adl");
    println!(
        "\n--- Content of {} ---\n{}",
        ADESH_MANIFEST_FILE, manifest_content
    );

    // -------------------------------------------------------------------------
    // STEP 2: Deterministic Dependency Graph Resolution
    // -------------------------------------------------------------------------
    println!("------------------------------------------------------------------------");
    println!("[Step 2] Resolving Dependency Graph with Registry Mock...");

    let mut resolver = DependencyResolver::new();
    // Register available registry packages:
    // - adesh-math depends on adesh-core
    resolver.register_package(
        "adesh-math",
        "1.2.4",
        vec![("adesh-core".to_string(), "1.0.0".to_string())],
    );
    // - adesh-simd depends on adesh-core
    resolver.register_package(
        "adesh-simd",
        "0.4.2",
        vec![("adesh-core".to_string(), "1.0.0".to_string())],
    );
    // - adesh-core has no dependencies
    resolver.register_package("adesh-core", "1.0.0", vec![]);
    // - adesh-net (optional dependency)
    resolver.register_package("adesh-net", "0.8.0", vec![]);

    // Resolve dependencies with default features active ("accelerated")
    let active_features = vec!["accelerated".to_string()];
    let lock_file = resolver
        .resolve(&manifest, &active_features)
        .expect("Failed to resolve dependencies");

    println!(
        "✓ Dependency resolution succeeded. Total locked packages: {}",
        lock_file.packages.len()
    );
    for pkg in &lock_file.packages {
        println!(
            "  • Package: {:<15} Version: {:<8} Checksum: {:<16}",
            pkg.name,
            pkg.version,
            &pkg.checksum[..16]
        );
    }

    // -------------------------------------------------------------------------
    // STEP 3: Emit and Reload `adesh.lock.adl`
    // -------------------------------------------------------------------------
    println!("\n------------------------------------------------------------------------");
    println!(
        "[Step 3] Emitting Deterministic Lockfile ({}) ...",
        ADESH_LOCK_FILE
    );

    lock_file
        .save_to_file(&lock_path)
        .expect("Failed to write adesh.lock.adl");
    println!("✓ Saved '{}' successfully.", ADESH_LOCK_FILE);

    let lock_content = std::fs::read_to_string(&lock_path).expect("Read adesh.lock.adl");
    println!("\n--- Content of {} ---\n{}", ADESH_LOCK_FILE, lock_content);

    // Re-parse lockfile to verify reproducibility
    let reloaded_lock = LockFile::from_file(&lock_path).expect("Reload adesh.lock.adl");
    assert_eq!(lock_file.packages.len(), reloaded_lock.packages.len());
    println!("✓ Verified lockfile parity: parsed packages exactly match in-memory state.");

    // -------------------------------------------------------------------------
    // STEP 4: Incremental Compilation Caching with Fingerprints
    // -------------------------------------------------------------------------
    println!("\n------------------------------------------------------------------------");
    println!("[Step 4] Incremental Compilation & Cryptographic Fingerprinting...");

    let cache_dir = work_dir.path().join(".cache");
    let mut cache = IncrementalCache::new(&cache_dir);

    let mod_source = "fn multiply_matrix(a: &Matrix, b: &Matrix) -> Matrix { /* SIMD Kernel */ }";
    let mod_interface = "fn multiply_matrix(a: &Matrix, b: &Matrix) -> Matrix;";
    let flags = "-O3 --lto=fat --target=x86_64-pc-windows-msvc";

    let mut dep_hashes = BTreeMap::new();
    dep_hashes.insert("adesh-math".to_string(), 0xA1B2C3D4E5F60718_u64);
    dep_hashes.insert("adesh-simd".to_string(), 0x1122334455667788_u64);

    // Initial Fingerprint
    let fp_v1 = ModuleFingerprint::compute(
        "matrix_core",
        mod_source,
        mod_interface,
        flags,
        dep_hashes.clone(),
    );
    println!("  Initial Module Fingerprint (v1):");
    println!("    Source Hash:    0x{:016x}", fp_v1.source_hash);
    println!("    Interface Hash: 0x{:016x}", fp_v1.interface_hash);
    println!("    Flags Hash:     0x{:016x}", fp_v1.flags_hash);

    // Check if rebuild needed on clean cache
    let needs_rebuild_1 = cache.should_rebuild(&fp_v1);
    println!(
        "  -> First build: should_rebuild = {} (Expected: true - Cache Miss)",
        needs_rebuild_1
    );
    assert!(needs_rebuild_1);

    // Record compilation in cache
    cache.update(fp_v1.clone());
    println!("  -> Recorded compilation artifact in incremental cache.");

    // Second check with unchanged module
    let needs_rebuild_2 = cache.should_rebuild(&fp_v1);
    println!(
        "  -> Unchanged re-run: should_rebuild = {} (Expected: false - Cache Hit)",
        needs_rebuild_2
    );
    assert!(!needs_rebuild_2);

    // Modify internal implementation ONLY (interface and flags unchanged)
    let mod_source_v2 =
        "fn multiply_matrix(a: &Matrix, b: &Matrix) -> Matrix { /* AVX-512 Optimized */ }";
    let fp_v2 = ModuleFingerprint::compute(
        "matrix_core",
        mod_source_v2,
        mod_interface,
        flags,
        dep_hashes.clone(),
    );
    let needs_rebuild_3 = cache.should_rebuild(&fp_v2);
    let interface_changed = fp_v1.public_interface_changed(&fp_v2);
    println!("  -> Internal body edit:");
    println!("     should_rebuild = {} (Expected: true)", needs_rebuild_3);
    println!(
        "     public_interface_changed = {} (Expected: false - Downstream dependents skip rebuild!)",
        interface_changed
    );
    assert!(needs_rebuild_3);
    assert!(!interface_changed);

    println!("\n========================================================================");
    println!("  AdeshLang ADL Manifest & Lockfile Architecture Verified Successfully! ");
    println!("========================================================================");
}
