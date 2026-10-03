//! End-to-End Native Backend Profile-Guided Optimization (PGO) Tests (Phase 7).
//!
//! Verifies:
//! 1. PGO profile generation (`PgoInstrumentationPass`).
//! 2. PGO profile serialization and deserialization (`ProfileData`, `.profdata` JSON).
//! 3. Profile-guided basic block layout optimization (`PgoOptimizationPass`).
//! 4. Hotness-weighted register spill penalty scaling.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_codegen::opt::{
    BlockProfile, EdgeProfile, FunctionProfile, OptLevel, PgoInstrumentationPass,
    PgoOptimizationPass, ProfileData,
};

#[test]
fn test_pgo_instrumentation_pass() {
    let mut func = MachineFunction::new("hot_loop");
    let _b1 = func.create_block("header");
    let _b2 = func.create_block("latch");

    let pass = PgoInstrumentationPass::new();
    pass.instrument_function(&mut func);

    for block in &func.blocks {
        assert!(
            matches!(&block.instructions[0], MachineInstruction::Custom { name, .. } if name == "pgo_inc"),
            "every block must begin with a pgo_inc counter instruction"
        );
    }
}

#[test]
fn test_pgo_profile_data_serialization() {
    let mut profile_data = ProfileData::new();
    let mut func_prof = FunctionProfile::new("compute_sum");
    func_prof.entry_count = 10000;

    func_prof.add_block_profile(
        0,
        BlockProfile {
            execution_count: 10000,
        },
    );
    func_prof.add_block_profile(
        1,
        BlockProfile {
            execution_count: 9900,
        },
    );
    func_prof.add_block_profile(
        2,
        BlockProfile {
            execution_count: 100,
        },
    );

    func_prof.edge_profiles.insert(
        "0:1".to_string(),
        EdgeProfile {
            transition_count: 9900,
            probability: 0.99,
        },
    );
    func_prof.edge_profiles.insert(
        "0:2".to_string(),
        EdgeProfile {
            transition_count: 100,
            probability: 0.01,
        },
    );

    profile_data.add_function_profile(func_prof);

    let json = profile_data.to_json().expect("serialize PGO data");
    assert!(json.contains("compute_sum"));
    assert!(json.contains("10000"));

    let loaded = ProfileData::from_json(&json).expect("deserialize PGO data");
    let loaded_prof = loaded
        .get_function_profile("compute_sum")
        .expect("find profile");
    assert_eq!(loaded_prof.entry_count, 10000);
    assert_eq!(loaded_prof.block_frequency(1), 9900);
    assert!(loaded_prof.is_hot_block(1, 0.5));
    assert!(!loaded_prof.is_hot_block(2, 0.5));
}

#[test]
fn test_pgo_profile_guided_block_layout_and_spill_weights() {
    let mut func = MachineFunction::new("branch_trace");
    let _b_cold = func.create_block("cold_error");
    let _b_hot = func.create_block("hot_worker");

    func.blocks[0].push(MachineInstruction::Return);
    func.blocks[1].push(MachineInstruction::Return);
    func.blocks[2].push(MachineInstruction::Return);

    let mut profile_data = ProfileData::new();
    let mut func_prof = FunctionProfile::new("branch_trace");
    func_prof.entry_count = 1000;
    func_prof.add_block_profile(
        0,
        BlockProfile {
            execution_count: 1000,
        },
    );
    func_prof.add_block_profile(1, BlockProfile { execution_count: 5 }); // cold
    func_prof.add_block_profile(
        2,
        BlockProfile {
            execution_count: 995,
        },
    ); // hot

    profile_data.add_function_profile(func_prof);

    let mut pgo_opt = PgoOptimizationPass::new(profile_data);

    // Verify spill weights
    let hot_weight = pgo_opt.spill_weight_for_block("branch_trace", 2);
    let cold_weight = pgo_opt.spill_weight_for_block("branch_trace", 1);
    assert!(
        hot_weight > cold_weight * 50.0,
        "hot block must have much higher spill penalty"
    );

    // Apply layout optimization
    let changed = adesh_codegen::opt::pass::MachinePass::run_on_function(&mut pgo_opt, &mut func)
        .expect("pgo layout pass");
    assert!(changed, "PGO layout pass should reorder blocks");

    // Block layout must now be: entry (0) -> hot_worker (2) -> cold_error (1)
    assert_eq!(func.blocks[0].label, "entry");
    assert_eq!(func.blocks[1].label, "hot_worker");
    assert_eq!(func.blocks[2].label, "cold_error");
}
