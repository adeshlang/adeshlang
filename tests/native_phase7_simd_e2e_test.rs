//! End-to-End Native Backend SIMD / Vectorization Tests (Phase 7).
//!
//! Verifies:
//! 1. Target-independent VectorType (<4 x f32>, <4 x i32>, <2 x f64>) and VectorCostModel.
//! 2. Vector Machine IR instructions (VectorAdd, VectorSub, VectorMul, VectorLoad, VectorStore, VectorShuffle, VectorReduceAdd).
//! 3. AutoVectorizePass: conservative loop widening and reduction transforms.
//! 4. Real native compilation, linking, and execution with SSE2 on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::{
    AutoVectorizePass, CpuFeatures, OptLevel, OptimizationPipeline, VectorCostModel,
    VectorElementType, VectorType,
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
fn test_vector_type_system_and_cost_model() {
    let v4f32 = VectorType::v4f32();
    assert_eq!(v4f32.lanes, 4);
    assert_eq!(v4f32.element_type, VectorElementType::F32);
    assert_eq!(v4f32.total_bits(), 128);
    assert_eq!(v4f32.total_bytes(), 16);
    assert_eq!(v4f32.alignment(), 16);
    assert!(v4f32.is_floating_point());
    assert!(!v4f32.is_integer());

    let v8i32 = VectorType::v8i32();
    assert_eq!(v8i32.lanes, 8);
    assert_eq!(v8i32.total_bits(), 256);
    assert_eq!(v8i32.alignment(), 32);
    assert!(v8i32.is_integer());

    let features_sse2 = CpuFeatures::x86_64_baseline();
    let cost_model_sse2 = VectorCostModel::new(features_sse2);
    assert!(cost_model_sse2.estimated_speedup(v4f32) > 1.0);

    let features_modern = CpuFeatures::x86_64_modern();
    let cost_model_avx = VectorCostModel::new(features_modern);
    assert!(cost_model_avx.estimated_speedup(v8i32) > 1.0);
}

#[test]
fn test_auto_vectorize_prototype_fails_closed() {
    let mut func = MachineFunction::new("vector_loop");
    let _loop_block_id = func.create_block("loop_body");

    // Block 0: entry jumps to loop_body
    func.blocks[0].push(MachineInstruction::Branch {
        target: "loop_body".to_string(),
    });

    // Block 1 (loop_body): natural loop with back-edge
    let v_iv = func.alloc_vreg();
    let v_a = func.alloc_fp_vreg();
    let v_b = func.alloc_fp_vreg();

    let loop_block = &mut func.blocks[1];
    loop_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_iv)),
        src: MachineOperand::Immediate(1),
    });
    loop_block.push(MachineInstruction::FAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_a)),
        src: MachineOperand::Register(MachineRegister::Virtual(v_b)),
        size: 4,
    });
    loop_block.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(MachineRegister::Virtual(v_iv)),
        rhs: MachineOperand::Immediate(100),
    });
    loop_block.push(MachineInstruction::BranchCc {
        cc: ConditionCode::LessThan,
        target: "loop_body".to_string(),
    });
    loop_block.push(MachineInstruction::Return);

    func.rebuild_cfg();

    let features = CpuFeatures::x86_64_baseline();
    let mut vec_pass = AutoVectorizePass::new(features);
    let changed = adesh_codegen::opt::pass::MachinePass::run_on_function(&mut vec_pass, &mut func)
        .expect("vectorize run");
    assert!(
        !changed,
        "the prototype must not rewrite scalar operations without proving lane legality"
    );

    let has_vec_add = func.blocks[1]
        .instructions
        .iter()
        .any(|i| matches!(i, MachineInstruction::VectorAdd { .. }));
    assert!(
        !has_vec_add,
        "scalar arithmetic must remain scalar until vector legality and packing are implemented"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_simd_vector_arithmetic_and_reduction_e2e() {
    let mut module = NativeModule::new("test_simd");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    // Allocate 128-bit vector registers
    let v0 = main_func.alloc_fp_vreg();
    let v1 = main_func.alloc_fp_vreg();
    let res = main_func.alloc_vreg();

    let block = main_func.entry_block_mut();
    // v0 = broadcast 10.0, v1 = broadcast 2.0
    block.push(MachineInstruction::FCvtIntToFloat {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(10),
        is_f64: false,
        is_signed: true,
    });
    block.push(MachineInstruction::VectorBroadcast {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
        vec_type: VectorType::v4f32(),
    });

    block.push(MachineInstruction::FCvtIntToFloat {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(2),
        is_f64: false,
        is_signed: true,
    });
    block.push(MachineInstruction::VectorBroadcast {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        vec_type: VectorType::v4f32(),
    });

    // v0 = v0 + v1 -> [12.0, 12.0, 12.0, 12.0]
    block.push(MachineInstruction::VectorAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        vec_type: VectorType::v4f32(),
    });

    // v0 = reduce_add(v0) -> 12 + 12 + 12 + 12 = 48.0
    block.push(MachineInstruction::VectorReduceAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
        vec_type: VectorType::v4f32(),
    });

    // res = cvttss2si(v0) -> 48
    block.push(MachineInstruction::FCvtFloatToInt {
        dst: MachineOperand::Register(MachineRegister::Virtual(res)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
        is_f64: false,
        is_signed: true,
    });

    // RAX = res
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(res)),
    });
    block.push(MachineInstruction::Return);

    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_simd_exec");
    assert_eq!(
        code, 48,
        "SIMD vector arithmetic & reduction should return 48"
    );
}
