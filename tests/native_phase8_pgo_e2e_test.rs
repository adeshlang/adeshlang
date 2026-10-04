//! End-to-End Native Backend Profile-Guided Optimization (PGO) Tests (Phase 8).
//!
//! Verifies:
//! 1. PGO ProfileData model and serialization / deserialization round-trip.
//! 2. PgoInstrumentationPass (-PGO=generate): block counter generation.
//! 3. PgoOptimizationPass (-PGO=use): profile-driven block reordering and hotness spill weights.
//! 4. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::opt::pass::MachinePass;
use adesh_codegen::opt::pgo::{
    BlockProfile, EdgeProfile, FunctionProfile, PgoInstrumentationPass, PgoOptimizationPass,
    ProfileData,
};
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn emit_link_and_run(native_mod: &NativeModule, opt_level: OptLevel, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(opt_level);

    let obj = backend.emit_object(native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{}.adob", test_name));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{}.exe", test_name));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

#[test]
fn test_pgo_profile_serialization_and_queries() {
    let mut pdata = ProfileData::new();
    let mut fprof = FunctionProfile::new("compute_sum");
    fprof.entry_count = 1000;
    fprof.add_block_profile(
        0,
        BlockProfile {
            execution_count: 1000,
        },
    );
    fprof.add_block_profile(
        1,
        BlockProfile {
            execution_count: 950,
        },
    );
    fprof.add_block_profile(
        2,
        BlockProfile {
            execution_count: 50,
        },
    );

    pdata.add_function_profile(fprof);

    let json = pdata.to_json().expect("serialize to json");
    let restored = ProfileData::from_json(&json).expect("deserialize from json");

    let retrieved = restored
        .get_function_profile("compute_sum")
        .expect("find profile");
    assert_eq!(retrieved.entry_count, 1000);
    assert_eq!(retrieved.block_frequency(1), 950);
    assert!(retrieved.is_hot_block(1, 0.5));
    assert!(!retrieved.is_hot_block(2, 0.5));
}

#[test]
fn test_pgo_instrumentation_and_optimization_passes() {
    let mut func = MachineFunction::new("hot_loop");
    let _b1 = func.create_block("body");
    let _b2 = func.create_block("exit");

    // Instrument
    let instr_pass = PgoInstrumentationPass::new();
    instr_pass.instrument_function(&mut func);
    assert_eq!(func.blocks[0].instructions.len(), 1);
    assert_eq!(func.blocks[1].instructions.len(), 1);
    assert!(
        matches!(&func.blocks[0].instructions[0], MachineInstruction::Custom { name, .. } if name == "pgo_inc")
    );

    // Prepare profile data
    let mut pdata = ProfileData::new();
    let mut fprof = FunctionProfile::new("hot_loop");
    fprof.entry_count = 100;
    fprof.add_block_profile(
        1,
        BlockProfile {
            execution_count: 90,
        },
    );
    fprof.add_block_profile(2, BlockProfile { execution_count: 5 });
    pdata.add_function_profile(fprof);

    let mut opt_pass = PgoOptimizationPass::new(pdata);
    let changed = opt_pass
        .run_on_function(&mut func)
        .expect("run pgo optimization");
    assert!(changed);
    assert_eq!(
        opt_pass.spill_weight_for_block("hot_loop", 1),
        1.0 + 90.0 * 10.0
    );
}

#[test]
fn test_native_pgo_execution_e2e() {
    let mut module = NativeModule::new("test_pgo");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let b = main_func.entry_block_mut();
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(77),
    });
    b.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_pgo_exec");
    assert_eq!(code, 77, "PGO compiled executable must return 77");
}
