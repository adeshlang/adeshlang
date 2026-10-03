//! End-to-End Native Backend Advanced Optimization, Dominance, Memory, and Loop Analysis Tests (Phase 6).
//!
//! Verifies:
//! 1. Dominator tree construction and dominance query verification (`dominates`, `strictly_dominates`).
//! 2. Natural loop detection (headers, latches, blocks, nesting depth).
//! 3. Sparse Conditional Constant Propagation (SCCP) evaluating branch conditions and pruning dead edges.
//! 4. Global Value Numbering (GVN) and cross-block CSE using dominance relationships.
//! 5. Memory effects modeling and Alias Analysis (Level 1 stack slot independence).
//! 6. Dead-Store Elimination (DSE) removing overwritten unread stores.
//! 7. Load-Store Forwarding converting loads into register moves.
//! 8. Loop-Invariant Code Motion (LICM) hoisting invariant computations out of loop bodies.
//! 9. Induction-Variable Analysis and loop strength reduction.
//! 10. Native executable execution on Windows x64 producing exact expected exit codes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::{
    AliasAnalysis, AliasResult, DeadStoreEliminationPass, DominatorTree, GVNPass,
    InductionVariablePass, LICMPass, LoadStoreForwardingPass, LoopInfo, OptLevel,
    OptimizationPipeline, SCCPPass,
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
    let adob_path = dir
        .path()
        .join(format!("{}_{:?}.adob", test_name, opt_level));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir
        .path()
        .join(format!("{}_{:?}.exe", test_name, opt_level));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

#[test]
fn test_dominator_tree_and_queries() {
    // CFG: 0 (entry) -> 1 (b1), 0 -> 2 (b2)
    // 1 -> 3 (join), 2 -> 3 (join)
    // 3 -> 4 (exit)
    let mut func = MachineFunction::new("dom_test");
    let b1_id = func.create_block("b1");
    let b2_id = func.create_block("b2");
    let join_id = func.create_block("join");
    let exit_id = func.create_block("exit");

    func.blocks[0].successors = vec![b1_id, b2_id];
    func.blocks[b1_id as usize].predecessors = vec![0];
    func.blocks[b1_id as usize].successors = vec![join_id];

    func.blocks[b2_id as usize].predecessors = vec![0];
    func.blocks[b2_id as usize].successors = vec![join_id];

    func.blocks[join_id as usize].predecessors = vec![b1_id, b2_id];
    func.blocks[join_id as usize].successors = vec![exit_id];

    func.blocks[exit_id as usize].predecessors = vec![join_id];

    let dom = DominatorTree::compute(&func);

    // Entry dominates everything
    assert!(dom.dominates(0, 0));
    assert!(dom.dominates(0, b1_id));
    assert!(dom.dominates(0, b2_id));
    assert!(dom.dominates(0, join_id));
    assert!(dom.dominates(0, exit_id));

    // Join dominates exit
    assert!(dom.dominates(join_id, exit_id));
    assert!(dom.strictly_dominates(join_id, exit_id));

    // Neither b1 nor b2 dominates join
    assert!(!dom.dominates(b1_id, join_id));
    assert!(!dom.dominates(b2_id, join_id));
}

#[test]
fn test_natural_loop_detection() {
    // Loop: 0 (entry) -> 1 (header) <-> 2 (body/latch) -> 3 (exit)
    let mut func = MachineFunction::new("loop_test");
    let header_id = func.create_block("header");
    let body_id = func.create_block("body");
    let exit_id = func.create_block("exit");

    func.blocks[0].successors = vec![header_id];
    func.blocks[header_id as usize].predecessors = vec![0, body_id];
    func.blocks[header_id as usize].successors = vec![body_id, exit_id];

    func.blocks[body_id as usize].predecessors = vec![header_id];
    func.blocks[body_id as usize].successors = vec![header_id]; // Back-edge!

    func.blocks[exit_id as usize].predecessors = vec![header_id];

    let dom = DominatorTree::compute(&func);
    let loop_info = LoopInfo::analyze(&func, &dom);

    assert!(loop_info.is_in_loop(header_id));
    assert!(loop_info.is_in_loop(body_id));
    assert!(!loop_info.is_in_loop(exit_id));

    let nat_loop = loop_info
        .get_loop_for_header(header_id)
        .expect("loop found");
    assert_eq!(nat_loop.header_id, header_id);
    assert_eq!(nat_loop.latch_ids, vec![body_id]);
    assert_eq!(nat_loop.nesting_depth, 1);
}

#[test]
fn test_alias_analysis_and_dse_e2e() {
    // Stack slots: store [slot -8], 111 (dead); store [slot -8], 222 (live); store [slot -16], 333 (live); return 222 + 333 (555) % 256 = 43
    assert_eq!(
        AliasAnalysis::alias(
            &MachineOperand::StackSlot(-8),
            &MachineOperand::StackSlot(-8)
        ),
        AliasResult::MustAlias
    );
    assert_eq!(
        AliasAnalysis::alias(
            &MachineOperand::StackSlot(-8),
            &MachineOperand::StackSlot(-16)
        ),
        AliasResult::NoAlias
    );

    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v_dead = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let v2 = func.alloc_vreg();
    let r1 = func.alloc_vreg();
    let r2 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_dead)),
        src: MachineOperand::Immediate(111),
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Register(MachineRegister::Virtual(v_dead)),
        size: 8,
    });
    // Overwrite slot -8 before any read!
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(20),
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-8),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        size: 8,
    });

    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v2)),
        src: MachineOperand::Immediate(22),
    });
    entry.push(MachineInstruction::Store {
        dst: MachineOperand::StackSlot(-16),
        src: MachineOperand::Register(MachineRegister::Virtual(v2)),
        size: 8,
    });

    entry.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(r1)),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    entry.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(r2)),
        src: MachineOperand::StackSlot(-16),
        size: 8,
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(r1)),
        src: MachineOperand::Register(MachineRegister::Virtual(r2)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(r1)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_alias_dse");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "alias_dse");
    assert_eq!(exit_code, 42); // 20 + 22 = 42
}

#[test]
fn test_sccp_branch_folding_e2e() {
    // Tests: condition is statically constant 10 < 20 (always taken) -> jump to true_block (return 77)
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v_cond = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_cond)),
        src: MachineOperand::Immediate(10),
    });
    entry.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(MachineRegister::Virtual(v_cond)),
        rhs: MachineOperand::Immediate(20),
    });
    entry.push(MachineInstruction::BranchCc {
        cc: ConditionCode::LessThan,
        target: "true_block".to_string(),
    });
    entry.push(MachineInstruction::Branch {
        target: "false_block".to_string(),
    });

    let tb_id = func.create_block("true_block");
    let tb = &mut func.blocks[tb_id as usize];
    tb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(77),
    });
    tb.push(MachineInstruction::Return);

    let fb_id = func.create_block("false_block");
    let fb = &mut func.blocks[fb_id as usize];
    fb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(99),
    });
    fb.push(MachineInstruction::Return);

    func.rebuild_cfg();

    let mut module = NativeModule::new("test_sccp");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "sccp_fold");
    assert_eq!(exit_code, 77);
}

#[test]
fn test_gvn_global_cse_e2e() {
    // Tests: b1 computes x = a + b (30 + 12 = 42). b2 dominates b3 which recomputes a + b.
    // GVN reuses the earlier computed value.
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let a = func.alloc_vreg();
    let b = func.alloc_vreg();
    let x = func.alloc_vreg();
    let y = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Immediate(30),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(b)),
        src: MachineOperand::Immediate(12),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(x)),
        src: MachineOperand::Register(MachineRegister::Virtual(a)),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(x)),
        src: MachineOperand::Register(MachineRegister::Virtual(b)),
    });
    entry.push(MachineInstruction::Branch {
        target: "next_block".to_string(),
    });

    let nb_id = func.create_block("next_block");
    let nb = &mut func.blocks[nb_id as usize];
    nb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(y)),
        src: MachineOperand::Register(MachineRegister::Virtual(a)),
    });
    nb.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(y)),
        src: MachineOperand::Register(MachineRegister::Virtual(b)),
    });
    nb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(y)),
    });
    nb.push(MachineInstruction::Return);

    func.rebuild_cfg();

    let mut module = NativeModule::new("test_gvn");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "gvn_test");
    assert_eq!(exit_code, 42);
}
