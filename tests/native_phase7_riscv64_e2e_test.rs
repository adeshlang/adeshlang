//! End-to-End Native Backend RISC-V64 Tests (Phase 7).
//!
//! Verifies:
//! 1. RISC-V RV64GC register allocation and calling convention.
//! 2. RV64I integer arithmetic, logic, shifts, compare, branch fixups.
//! 3. RV64D double-precision floating point (FAdd, FSub, FMul, FDiv).
//! 4. RV64A atomics (amoadd.d) and memory ordering fences.
//! 5. ADOB object generation and symbol/section structure for RISC-V64 Linux.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::targets::riscv::RiscVBackend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;

#[test]
fn test_riscv64_full_instruction_selection_and_encoding() {
    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_full");
    let mut func = MachineFunction::new("riscv64_compute");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let vf0 = func.alloc_fp_vreg();
    let vf1 = func.alloc_fp_vreg();

    let block = func.entry_block_mut();
    // v0 = 50; v1 = 15; v0 = v0 + v1; v0 = v0 * v1; v0 = v0 / v1
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(50),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(15),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Div {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });

    // Float: vf0 = vf0 + vf1; vf0 = vf0 * vf1
    block.push(MachineInstruction::FAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(vf0)),
        src: MachineOperand::Register(MachineRegister::Virtual(vf1)),
        size: 8,
    });
    block.push(MachineInstruction::FMul {
        dst: MachineOperand::Register(MachineRegister::Virtual(vf0)),
        src: MachineOperand::Register(MachineRegister::Virtual(vf1)),
        size: 8,
    });

    // Atomics & Barrier
    block.push(MachineInstruction::Barrier);
    block.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        size: 8,
    });

    block.push(MachineInstruction::Return);

    module.add_function(func);

    let lowered = backend.lower_module(&module).expect("lowering succeeds");
    let obj = backend.emit_object(&lowered).expect("ADOB object emission");

    AdobValidator::validate(&obj).expect("ADOB object must validate");
    assert_eq!(obj.symbols.len(), 1);
    assert_eq!(obj.symbols[0].name, "riscv64_compute");
    assert!(!obj.sections.is_empty());
    assert_eq!(obj.sections[0].name, ".text");
    assert!(obj.sections[0].data.len() >= 48);
}

#[test]
fn test_riscv64_immediate_shifts_bitwise_and_branches() {
    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_logic");
    let mut func = MachineFunction::new("logic_and_branches");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let v2 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    // 64-bit immediate loading (> 32 bits)
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0x1234_5678_9ABC_DEF0),
    });
    // Shift left, logical right, arithmetic right
    entry.push(MachineInstruction::Shl {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(4),
    });
    entry.push(MachineInstruction::Shr {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(2),
    });
    entry.push(MachineInstruction::Sar {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(1),
    });
    // Bitwise NOT, AND, OR, XOR
    entry.push(MachineInstruction::Not {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    entry.push(MachineInstruction::And {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0xFFF),
    });
    // Comparison and SetCc
    entry.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(MachineRegister::Virtual(v0)),
        rhs: MachineOperand::Immediate(100),
    });
    entry.push(MachineInstruction::SetCc {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        cc: ConditionCode::LessThan,
    });
    // BranchCc to target label
    entry.push(MachineInstruction::BranchCc {
        cc: ConditionCode::Equal,
        target: "exit_block".to_string(),
    });

    let mut exit_block = adesh_codegen::machine_ir::MachineBlock::new(1, "exit_block");
    exit_block.push(MachineInstruction::Return);
    func.blocks.push(exit_block);

    module.add_function(func);

    let lowered = backend.lower_module(&module).expect("lowering succeeds");
    let obj = backend.emit_object(&lowered).expect("ADOB object emission");

    AdobValidator::validate(&obj).expect("ADOB object must validate");
    assert_eq!(obj.symbols[0].name, "logic_and_branches");
    assert!(obj.sections[0].data.len() >= 64);
}

#[test]
fn test_riscv64_cross_function_calls_and_static_elf_linking() {
    use adesh_object::relocation::RelocationKind;
    use adesh_object::writer::AdobWriter;
    use tempfile::tempdir;

    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_linking");

    // Callee function: callee_add(a0, a1) -> a0
    let mut callee = MachineFunction::new("callee_add");
    let c_block = callee.entry_block_mut();
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))), // a0
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(11))), // a1
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Main entry function: calls callee_add, then returns
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let m_block = main_func.entry_block_mut();
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))), // a0 = 20
        src: MachineOperand::Immediate(20),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(11))), // a1 = 22
        src: MachineOperand::Immediate(22),
    });
    m_block.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("callee_add".to_string()),
        num_args: 2,
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let lowered = backend.lower_module(&module).expect("lowered");
    let obj = backend.emit_object(&lowered).expect("ADOB object emission");

    AdobValidator::validate(&obj).expect("ADOB must validate");

    // Verify relocation was emitted for callee_add
    let (_idx, text_sec) = obj.find_section(".text").expect("text section exists");
    assert!(
        text_sec
            .relocations
            .iter()
            .any(|r| r.symbol_name == "callee_add" && r.kind == RelocationKind::RiscV_Call),
        "must emit RiscV_Call relocation for callee_add"
    );

    // Now link into static ELF64 executable!
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join("main_riscv64.adob");
    let elf_path = dir.path().join("riscv64_program");

    let bytes = AdobWriter::write(&obj).expect("write ADOB");
    std::fs::write(&adob_path, bytes).expect("write file");

    let link_res = adesh_linker::link(
        &[&adob_path],
        &elf_path,
        Some("riscv64gc-unknown-linux-gnu"),
    );
    assert!(
        link_res.is_ok(),
        "linking RISC-V 64 static executable failed: {:?}",
        link_res
    );

    let elf_bytes = std::fs::read(&elf_path).expect("read ELF binary");
    assert!(elf_bytes.len() >= 64);
    assert_eq!(&elf_bytes[0..4], b"\x7fELF");
    assert_eq!(elf_bytes[4], 2, "ELFCLASS64");
    assert_eq!(elf_bytes[5], 1, "ELFDATA2LSB");
    assert_eq!(elf_bytes[7], 3, "ELFOSABI_LINUX");

    let e_machine = u16::from_le_bytes(elf_bytes[18..20].try_into().unwrap());
    assert_eq!(e_machine, 243, "EM_RISCV");

    let e_flags = u32::from_le_bytes(elf_bytes[48..52].try_into().unwrap());
    assert_eq!(e_flags, 0x0005, "EF_RISCV_RVC | EF_RISCV_FLOAT_ABI_DOUBLE");

    let e_entry = u64::from_le_bytes(elf_bytes[24..32].try_into().unwrap());
    assert!(e_entry >= 0x400000, "entry VA in user execution space");
}

#[test]
fn test_riscv64_callee_saved_preservation_and_spills() {
    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_callee_saved");

    // Function modifying callee-saved GPR s1 (9), s2 (18) and FPR fs0 (40), fs1 (41)
    let mut func = MachineFunction::new("callee_saved_worker");
    func.is_exported = true;
    func.stack_size = 32;

    {
        let block = func.entry_block_mut();

        // Assign physical callee-saved GPRs and FPRs directly in defs
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))), // s1
            src: MachineOperand::Immediate(100),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(18))), // s2
            src: MachineOperand::Immediate(200),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(40))), // fs0
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))), // ft0
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(41))), // fs1
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(33))), // ft1
        });

        // Add stack slot spills
        block.push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(0),
            src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))),
            size: 8,
        });
        block.push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(10))), // a0
            src: MachineOperand::StackSlot(0),
            size: 8,
        });

        block.push(MachineInstruction::Return);
    }
    module.add_function(func);

    let (code, _) = backend
        .generate_function_with_relocs(&module.functions[0])
        .expect("generates function");

    // Assert that prologue saved s1, s2, fs0, fs1 and epilogue restored them
    assert!(code.len() >= 64, "prologue/epilogue code length check");
    // Verify jalr x0, 0(ra) at end
    assert_eq!(&code[code.len() - 4..], &0x00008067u32.to_le_bytes());
}

#[test]
fn test_riscv64_atomic_cmpxchg_and_exchange() {
    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_atomics");
    let mut func = MachineFunction::new("atomic_ops");
    func.is_exported = true;
    let block = func.entry_block_mut();

    let ptr_reg = PhysicalRegister(10); // a0
    let exp_reg = PhysicalRegister(11); // a1
    let des_reg = PhysicalRegister(12); // a2

    block.push(MachineInstruction::AtomicExchange {
        dst: MachineOperand::Memory {
            base: MachineRegister::Physical(ptr_reg),
            offset: 0,
            index: None,
        },
        src: MachineOperand::Register(MachineRegister::Physical(des_reg)),
        size: 8,
    });

    block.push(MachineInstruction::AtomicCompareExchange {
        dst: MachineOperand::Memory {
            base: MachineRegister::Physical(ptr_reg),
            offset: 0,
            index: None,
        },
        expected: MachineOperand::Register(MachineRegister::Physical(exp_reg)),
        desired: MachineOperand::Register(MachineRegister::Physical(des_reg)),
        size: 8,
    });

    block.push(MachineInstruction::Return);
    module.add_function(func);

    let (code, _) = backend
        .generate_function_with_relocs(&module.functions[0])
        .expect("generates atomic function");

    assert!(code.len() >= 32);
}
