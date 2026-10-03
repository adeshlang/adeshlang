use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand,
};
use adesh_codegen::register_alloc::{AllocationVerifier, LinearScanAllocator, LivenessAnalysis};
use adesh_codegen::targets::x86_64::X86_64RegisterFile;

#[test]
fn test_cfg_rebuild_and_liveness_fixed_point() {
    let mut func = MachineFunction::new("test_loop");
    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Branch {
        target: "loop_header".to_string(),
    });

    let header_id = func.create_block("loop_header");
    let header = &mut func.blocks[header_id as usize];
    header.push(MachineInstruction::Add {
        dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(1),
    });
    header.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v0)),
        rhs: MachineOperand::Immediate(10),
    });
    header.push(MachineInstruction::BranchCc {
        cc: ConditionCode::LessThan,
        target: "loop_header".to_string(),
    });
    header.push(MachineInstruction::Branch {
        target: "exit".to_string(),
    });

    let exit_id = func.create_block("exit");
    let exit = &mut func.blocks[exit_id as usize];
    exit.push(MachineInstruction::Move {
        dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v1)),
        src: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v0)),
    });
    exit.push(MachineInstruction::Return);

    func.rebuild_cfg();

    assert_eq!(func.blocks[0].successors, vec![1]);
    assert_eq!(func.blocks[1].predecessors, vec![0, 1]);
    assert_eq!(func.blocks[1].successors, vec![1, 2]);
    assert_eq!(func.blocks[2].predecessors, vec![1]);

    let liveness = LivenessAnalysis::compute(&mut func);
    let v0_reg = adesh_codegen::machine_ir::MachineRegister::Virtual(v0);

    assert!(liveness.live_in.get(&1).unwrap().contains(&v0_reg));
    assert!(liveness.live_out.get(&1).unwrap().contains(&v0_reg));
}

#[test]
fn test_mixed_gpr_and_xmm_spill_pressure_and_verifier() {
    let mut func = MachineFunction::new("test_spill_pressure");
    let reg_file = X86_64RegisterFile;
    let allocator = LinearScanAllocator::new(&reg_file);

    let mut gpr_vregs = Vec::new();
    let mut fp_vregs = Vec::new();

    for _ in 0..20 {
        let v = func.alloc_vreg();
        gpr_vregs.push(v);
    }

    for _ in 0..20 {
        let v = func.alloc_fp_vreg();
        fp_vregs.push(v);
    }

    let entry = func.entry_block_mut();

    for &v in &gpr_vregs {
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v)),
            src: MachineOperand::Immediate(42),
        });
    }

    for &v in &fp_vregs {
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v)),
            src: MachineOperand::FloatImmediate(3.14159),
        });
    }

    let sum_gpr = gpr_vregs[0];
    for &v in &gpr_vregs[1..] {
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(
                sum_gpr,
            )),
            src: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v)),
        });
    }

    let sum_fp = fp_vregs[0];
    for &v in &fp_vregs[1..] {
        entry.push(MachineInstruction::FAdd {
            dst: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(
                sum_fp,
            )),
            src: MachineOperand::Register(adesh_codegen::machine_ir::MachineRegister::Virtual(v)),
            size: 8,
        });
    }

    entry.push(MachineInstruction::Return);

    let res = allocator.allocate(&mut func);

    assert!(res.total_spill_bytes > 0);
    assert!(!res.spill_map.is_empty());
    assert!(AllocationVerifier::verify(&func, &res.vreg_map, &res.spill_map).is_ok());
}
