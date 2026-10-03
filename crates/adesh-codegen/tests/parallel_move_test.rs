//! Comprehensive Parallel Move Resolver Unit & ABI Tests.

use adesh_codegen::calling_convention::{
    MoveLocation, MoveOperation, ParallelMoveResolver, RegisterClass, SystemVX64CallingConvention,
    WindowsX64CallingConvention, resolve_call_arguments, resolve_call_arguments_gpr,
};
use adesh_codegen::machine_ir::{
    MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister, VirtualRegister,
};
use std::collections::{HashMap, HashSet};

fn phys(id: u8) -> MoveLocation {
    MoveLocation::PhysicalRegister(PhysicalRegister(id))
}

fn rsp_slot(off: i32) -> MoveLocation {
    MoveLocation::StackSlot {
        base: PhysicalRegister(4), // RSP
        offset: off,
    }
}

fn rbp_slot(off: i32) -> MoveLocation {
    MoveLocation::StackSlot {
        base: PhysicalRegister(5), // RBP
        offset: off,
    }
}

// ---------------------------------------------------------------- State Simulator
// Simulates the effect of the emitted MachineInstruction move sequence on a state map
// to mathematically prove equivalence to the simultaneous assignment.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum StateLoc {
    Reg(u8),
    Stack(u8, i32), // base_reg, offset
}

impl StateLoc {
    fn from_move_loc(loc: &MoveLocation) -> Option<Self> {
        match loc {
            MoveLocation::PhysicalRegister(p) => Some(StateLoc::Reg(p.0)),
            MoveLocation::StackSlot { base, offset } => Some(StateLoc::Stack(base.0, *offset)),
            MoveLocation::Memory {
                base,
                offset,
                index,
            } if index.is_none() => {
                if let MachineRegister::Physical(p) = base {
                    Some(StateLoc::Stack(p.0, *offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn from_op(op: &MachineOperand) -> Option<Self> {
        match op {
            MachineOperand::Register(MachineRegister::Physical(p)) => Some(StateLoc::Reg(p.0)),
            MachineOperand::Memory {
                base,
                offset,
                index,
            } if index.is_none() => {
                if let MachineRegister::Physical(p) = base {
                    Some(StateLoc::Stack(p.0, *offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn simulate_moves(
    instructions: &[MachineInstruction],
    initial_state: &HashMap<StateLoc, i64>,
) -> HashMap<StateLoc, i64> {
    let mut state = initial_state.clone();

    for inst in instructions {
        match inst {
            MachineInstruction::Move { dst, src }
            | MachineInstruction::Load { dst, src, .. }
            | MachineInstruction::Store { dst, src, .. } => {
                let val = match src {
                    MachineOperand::Immediate(v) => *v,
                    _ => {
                        let src_loc = StateLoc::from_op(src).expect("valid src loc in simulator");
                        *state.get(&src_loc).unwrap_or(&0)
                    }
                };
                let dst_loc = StateLoc::from_op(dst).expect("valid dst loc in simulator");
                state.insert(dst_loc, val);
            }
            _ => panic!("Unexpected instruction in move resolution: {:?}", inst),
        }
    }

    state
}

fn verify_parallel_resolution(moves: &[MoveOperation]) {
    let resolver = ParallelMoveResolver::for_x86_64();
    let instructions = resolver.resolve_moves(moves).expect("resolution succeeds");

    // Initialize state with unique distinct values
    let mut initial_state = HashMap::new();
    let mut val_counter = 100i64;

    for m in moves {
        if let Some(src_loc) = StateLoc::from_move_loc(&m.src) {
            initial_state.entry(src_loc).or_insert_with(|| {
                let v = val_counter;
                val_counter += 10;
                v
            });
        }
        if let Some(dst_loc) = StateLoc::from_move_loc(&m.dst) {
            initial_state.entry(dst_loc).or_insert_with(|| {
                let v = val_counter;
                val_counter += 10;
                v
            });
        }
    }

    // Expected simultaneous state
    let mut expected_state = initial_state.clone();
    for m in moves {
        if let Some(dst_loc) = StateLoc::from_move_loc(&m.dst) {
            let src_val = match &m.src {
                MoveLocation::Immediate(v) => *v,
                _ => {
                    let src_loc = StateLoc::from_move_loc(&m.src).expect("src loc");
                    *initial_state.get(&src_loc).unwrap_or(&0)
                }
            };
            expected_state.insert(dst_loc, src_val);
        }
    }

    let final_state = simulate_moves(&instructions, &initial_state);

    // Verify all destinations matched expected values
    for m in moves {
        if let Some(dst_loc) = StateLoc::from_move_loc(&m.dst) {
            let expected = expected_state.get(&dst_loc).unwrap();
            let actual = final_state.get(&dst_loc).unwrap_or(&0);
            assert_eq!(
                actual, expected,
                "Mismatch at destination {:?} for move {:?}. Expected {}, got {}",
                dst_loc, m, expected, actual
            );
        }
    }
}

// ---------------------------------------------------------------- Basic Tests

#[test]
fn test_basic_identity_and_transfers() {
    // RAX -> RBX
    verify_parallel_resolution(&[MoveOperation::new_qword(phys(3), phys(0))]);

    // RAX -> RAX (self move)
    verify_parallel_resolution(&[MoveOperation::new_qword(phys(0), phys(0))]);

    // RAX -> RBX, RCX -> RDX
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(phys(2), phys(1)),
    ]);
}

#[test]
fn test_dependency_chain() {
    // RAX -> RBX, RBX -> RCX, RCX -> RDX, RDX -> RSI
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)), // RBX <- RAX
        MoveOperation::new_qword(phys(1), phys(3)), // RCX <- RBX
        MoveOperation::new_qword(phys(2), phys(1)), // RDX <- RCX
        MoveOperation::new_qword(phys(6), phys(2)), // RSI <- RDX
    ]);
}

#[test]
fn test_two_register_swap() {
    // RAX <-> RBX
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)), // RBX <- RAX
        MoveOperation::new_qword(phys(0), phys(3)), // RAX <- RBX
    ]);
}

#[test]
fn test_three_register_cycle() {
    // RAX -> RBX -> RCX -> RAX
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(phys(1), phys(3)),
        MoveOperation::new_qword(phys(0), phys(1)),
    ]);
}

#[test]
fn test_four_register_cycle() {
    // RDI (7) -> RSI (6) -> RDX (2) -> RCX (1) -> RDI (7)
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(6), phys(7)),
        MoveOperation::new_qword(phys(2), phys(6)),
        MoveOperation::new_qword(phys(1), phys(2)),
        MoveOperation::new_qword(phys(7), phys(1)),
    ]);
}

#[test]
fn test_multiple_disconnected_cycles() {
    // Cycle 1: RAX (0) <-> RBX (3)
    // Cycle 2: RCX (1) <-> RDX (2)
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(phys(0), phys(3)),
        MoveOperation::new_qword(phys(2), phys(1)),
        MoveOperation::new_qword(phys(1), phys(2)),
    ]);
}

#[test]
fn test_fan_out_multiple_reads() {
    // RAX -> RBX, RAX -> RCX, RAX -> RDX, RAX -> [rsp+8]
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(phys(1), phys(0)),
        MoveOperation::new_qword(phys(2), phys(0)),
        MoveOperation::new_qword(rsp_slot(8), phys(0)),
    ]);
}

#[test]
fn test_stack_to_register_and_register_to_stack() {
    // RAX -> [rsp+16], [rsp+16] -> RBX
    verify_parallel_resolution(&[
        MoveOperation::new_qword(rsp_slot(16), phys(0)),
        MoveOperation::new_qword(phys(3), rsp_slot(16)),
    ]);
}

#[test]
fn test_stack_to_stack_swap() {
    // [rsp+8] <-> [rsp+16]
    verify_parallel_resolution(&[
        MoveOperation::new_qword(rsp_slot(16), rsp_slot(8)),
        MoveOperation::new_qword(rsp_slot(8), rsp_slot(16)),
    ]);
}

#[test]
fn test_mixed_cycle_registers_and_stack() {
    // RAX (0) -> RBX (3) -> [rsp+8] -> RCX (1) -> [rbp-16] -> RAX (0)
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(rsp_slot(8), phys(3)),
        MoveOperation::new_qword(phys(1), rsp_slot(8)),
        MoveOperation::new_qword(rbp_slot(-16), phys(1)),
        MoveOperation::new_qword(phys(0), rbp_slot(-16)),
    ]);
}

#[test]
fn test_immediate_moves() {
    verify_parallel_resolution(&[
        MoveOperation::new_qword(phys(1), MoveLocation::imm(42)),
        MoveOperation::new_qword(rsp_slot(24), MoveLocation::imm(999999)),
        MoveOperation::new_qword(phys(2), phys(1)),
    ]);
}

// ---------------------------------------------------------------- ABI Tests

#[test]
fn test_win64_argument_lowering() {
    let conv = WindowsX64CallingConvention;
    let v0 = VirtualRegister(0);
    let v1 = VirtualRegister(1);
    let v2 = VirtualRegister(2);
    let v3 = VirtualRegister(3);
    let v4 = VirtualRegister(4);
    let v5 = VirtualRegister(5);

    // 6 arguments: 4 in RCX, RDX, R8, R9; 2 on stack at [rsp+32], [rsp+40]
    let (insts, total) =
        resolve_call_arguments_gpr(&conv, &[v0, v1, v2, v3, v4, v5], PhysicalRegister(4))
            .expect("resolves");

    assert_eq!(
        total, 48,
        "Win64 shadow (32) + 2 stack args (16) = 48 bytes (16-aligned)"
    );
    assert_eq!(insts.len(), 1);
    if let MachineInstruction::ParallelMove { moves } = &insts[0] {
        assert_eq!(moves.len(), 6);
        let resolved = ParallelMoveResolver::for_x86_64()
            .resolve_moves(moves)
            .expect("resolves");
        assert_eq!(resolved.len(), 6);
    } else {
        panic!("expected MachineInstruction::ParallelMove");
    }
}

#[test]
fn test_win64_mixed_fp_argument_lowering() {
    let conv = WindowsX64CallingConvention;
    let v0 = VirtualRegister(0);
    let v1 = VirtualRegister(1);
    let v2 = VirtualRegister(2);
    let v3 = VirtualRegister(3);
    let v4 = VirtualRegister(4);

    let typed_args = vec![
        (v0, RegisterClass::Gpr),   // RCX
        (v1, RegisterClass::Float), // XMM1
        (v2, RegisterClass::Gpr),   // R8
        (v3, RegisterClass::Float), // XMM3
        (v4, RegisterClass::Float), // [rsp+32]
    ];

    let (insts, total) =
        resolve_call_arguments(&conv, &typed_args, PhysicalRegister(4)).expect("resolves");

    assert_eq!(
        total, 48,
        "Win64 shadow (32) + 1 stack arg (8 aligned to 16) = 48 bytes"
    );
    if let MachineInstruction::ParallelMove { moves } = &insts[0] {
        assert_eq!(moves.len(), 5);
        assert_eq!(
            moves[0].dst,
            MoveLocation::PhysicalRegister(PhysicalRegister(1))
        ); // RCX
        assert_eq!(
            moves[1].dst,
            MoveLocation::PhysicalRegister(PhysicalRegister::xmm(1))
        ); // XMM1
        assert_eq!(
            moves[2].dst,
            MoveLocation::PhysicalRegister(PhysicalRegister(8))
        ); // R8
        assert_eq!(
            moves[3].dst,
            MoveLocation::PhysicalRegister(PhysicalRegister::xmm(3))
        ); // XMM3
        assert_eq!(
            moves[4].dst,
            MoveLocation::StackSlot {
                base: PhysicalRegister(4),
                offset: 32,
            }
        );
    } else {
        panic!("expected MachineInstruction::ParallelMove");
    }
}

#[test]
fn test_sysv_argument_lowering() {
    let conv = SystemVX64CallingConvention;
    let args: Vec<VirtualRegister> = (0..8).map(VirtualRegister).collect();

    // 8 arguments: 6 in RDI, RSI, RDX, RCX, R8, R9; 2 on stack at [rsp+0], [rsp+8]
    let (insts, total) =
        resolve_call_arguments_gpr(&conv, &args, PhysicalRegister(4)).expect("resolves");

    assert_eq!(
        total, 16,
        "SysV shadow (0) + 2 stack args (16) = 16 bytes (16-aligned)"
    );
    assert_eq!(insts.len(), 1);
    if let MachineInstruction::ParallelMove { moves } = &insts[0] {
        assert_eq!(moves.len(), 8);
        let resolved = ParallelMoveResolver::for_x86_64()
            .resolve_moves(moves)
            .expect("resolves");
        assert_eq!(resolved.len(), 8);
    } else {
        panic!("expected MachineInstruction::ParallelMove");
    }
}

#[test]
fn test_scratch_exhaustion_safety() {
    let mut reserved = HashSet::new();
    // Reserve all physical registers 0..15
    for i in 0..16 {
        reserved.insert(PhysicalRegister(i));
    }

    let resolver = ParallelMoveResolver::for_x86_64().with_reserved_registers(reserved);

    // A cycle requiring a temporary must fail safely with an error, not panic
    let res = resolver.resolve_moves(&[
        MoveOperation::new_qword(phys(3), phys(0)),
        MoveOperation::new_qword(phys(0), phys(3)),
    ]);

    assert!(
        res.is_err(),
        "Must return structured CodegenError on scratch exhaustion"
    );
}

#[test]
fn test_conflicting_destinations() {
    let resolver = ParallelMoveResolver::for_x86_64();
    let res = resolver.resolve_moves(&[
        MoveOperation::new_qword(phys(1), phys(0)),
        MoveOperation::new_qword(phys(1), phys(2)),
    ]);
    assert!(
        res.is_err(),
        "Must fail when two moves target the same destination"
    );
}

#[test]
fn test_immediate_destination_rejected() {
    let resolver = ParallelMoveResolver::for_x86_64();
    let res = resolver.resolve_moves(&[MoveOperation::new_qword(MoveLocation::imm(10), phys(0))]);
    assert!(res.is_err(), "Must fail when destination is an immediate");
}
