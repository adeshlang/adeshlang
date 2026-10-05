//! Phase 9 Self-Hosting Preparation and Bootstrap Audit E2E Test Suite.
//!
//! Validates:
//! - DependencyAuditor: classification of compiler dependencies for self-hosting.
//! - BootstrapMatrix: validation of autonomous native stages (frontend, middle-end, native backend, ADOB, adeshlink, native executable).
//! - Self-hosting readiness verification without external LLVM/linkers.

#![allow(dead_code, unused_imports)]

use adesh_codegen::bootstrap::{
    BootstrapMatrix, BootstrapStageStatus, DependencyAuditor, DependencyClassification,
};

#[test]
fn test_self_hosting_dependency_auditor() {
    let mut auditor = DependencyAuditor::new();
    auditor.register(
        "custom_ast",
        DependencyClassification::Essential,
        "AST representation",
        "Self-host in Adesh std::ast",
    );
    auditor.register(
        "temp_helper",
        DependencyClassification::Replaceable,
        "Temporary parser helper",
        "Replace with native parser",
    );

    let entries = auditor.entries();
    assert!(entries.contains_key("custom_ast"));
    assert!(entries.contains_key("temp_helper"));
    assert!(auditor.replaceable_count() >= 1);
}

#[test]
fn test_bootstrap_matrix_stages() {
    let matrix = BootstrapMatrix::current();
    assert_eq!(matrix.frontend_status, BootstrapStageStatus::Autonomous);
    assert_eq!(matrix.native_backend_status, BootstrapStageStatus::Autonomous);
    assert_eq!(matrix.adeshlink_status, BootstrapStageStatus::Autonomous);
    assert!(matrix.is_fully_autonomous());
}
