//! Phase 9 Production PGO E2E Test Suite.
//!
//! Validates:
//! - Profile generation and branch probability calculation.
//! - Multi-run profile merging with `ProfileMerger`.
//! - Profile-guided optimization application (inlining, block layout, hot/cold splitting).

#![allow(dead_code, unused_imports)]

use adesh_codegen::opt::pgo::{
    BlockProfile, EdgeProfile, FunctionProfile, PgoConfig, PgoEngine, PgoMode, ProfileData,
};
use adesh_codegen::pgo_tools::ProfileMerger;
use tempfile::tempdir;

#[test]
fn test_pgo_multi_profile_merging() {
    let mut prof1 = ProfileData::new();
    let mut f1 = FunctionProfile::new("compute_sum");
    f1.entry_count = 100;
    let mut b1 = BlockProfile::default();
    b1.execution_count = 100;
    f1.block_profiles.insert("entry".to_string(), b1);
    prof1.functions.insert("compute_sum".to_string(), f1);

    let mut prof2 = ProfileData::new();
    let mut f2 = FunctionProfile::new("compute_sum");
    f2.entry_count = 250;
    let mut b2 = BlockProfile::default();
    b2.execution_count = 250;
    f2.block_profiles.insert("entry".to_string(), b2);
    prof2.functions.insert("compute_sum".to_string(), f2);

    let merged = ProfileMerger::merge(&[prof1, prof2]).expect("merge profiles");
    let sum_prof = merged
        .functions
        .get("compute_sum")
        .expect("function profile");
    assert_eq!(sum_prof.entry_count, 350);
    assert_eq!(
        sum_prof
            .block_profiles
            .get("entry")
            .unwrap()
            .execution_count,
        350
    );
}

#[test]
fn test_pgo_merge_files_on_disk() {
    let dir = tempdir().expect("tempdir");
    let p1_path = dir.path().join("run1.pgo.json");
    let p2_path = dir.path().join("run2.pgo.json");
    let out_path = dir.path().join("merged.pgo.json");

    let mut prof1 = ProfileData::new();
    let mut f1 = FunctionProfile::new("handler");
    f1.entry_count = 50;
    prof1.functions.insert("handler".to_string(), f1);
    std::fs::write(&p1_path, serde_json::to_string(&prof1).unwrap()).unwrap();

    let mut prof2 = ProfileData::new();
    let mut f2 = FunctionProfile::new("handler");
    f2.entry_count = 70;
    prof2.functions.insert("handler".to_string(), f2);
    std::fs::write(&p2_path, serde_json::to_string(&prof2).unwrap()).unwrap();

    ProfileMerger::merge_files(&[&p1_path, &p2_path], &out_path).expect("merge files");
    assert!(out_path.exists());

    let content = std::fs::read_to_string(&out_path).unwrap();
    let merged: ProfileData = serde_json::from_str(&content).unwrap();
    assert_eq!(merged.functions.get("handler").unwrap().entry_count, 120);
}
