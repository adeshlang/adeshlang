//! Phase 9 Self-Hosting Preparation and Bootstrap Audit E2E Test Suite.
//!
//! Validates:
//! - DependencyAuditor: classification of compiler dependencies for self-hosting.
//! - BootstrapMatrix: Stage 0 (Rust compiler) -> Stage 1 (Adesh compiler) -> Stage 2 (Self-hosted) compatibility.
//! - Self-hosting readiness metric.

#![allow(dead_code, unused_imports)]

use adesh_codegen::bootstrap::{BootstrapMatrix, DependencyAuditor, DependencyClassification};

#[test]
fn test_self_hosting_dependency_auditor() {
    let mut auditor = DependencyAuditor::new();
    auditor.register("core_ast", DependencyClassification::KernelPure);
    auditor.register("hir_lower", DependencyClassification::KernelPure);
    auditor.register("mem_alloc", DependencyClassification::SystemWrapped);
    auditor.register("cargo_build", DependencyClassification::HostToolchain);

    let report = auditor.audit();
    assert_eq!(report.kernel_pure_count, 2);
    assert_eq!(report.system_wrapped_count, 1);
    assert_eq!(report.host_toolchain_count, 1);
    assert!(report.self_hosting_readiness_score > 0.0);
}

#[test]
fn test_bootstrap_matrix_stages() {
    let matrix = BootstrapMatrix::default_matrix();
    assert_eq!(matrix.stages.len(), 3);
    assert_eq!(matrix.stages[0].stage_name, "stage0_rust_bootstrapper");
    assert_eq!(matrix.stages[1].stage_name, "stage1_adesh_compiler");
    assert_eq!(matrix.stages[2].stage_name, "stage2_self_hosted_compiler");
}
