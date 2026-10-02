//! End-to-end native x86-64 pipeline test: Machine IR -> native codegen ->
//! validated ADOB object -> adeshlink PE link -> execute the binary and check
//! its exit code.
//!
//! Exercises the fixes this suite guards:
//! - cross-function `call rel32` relocations (previously placeholders that
//!   were never patched),
//! - forward-branch fixups (previously miscompiled via `unwrap_or(0)`),
//! - stack arguments stored through `[rsp + disp]` (requires SIB-aware
//!   encoding),
//! - incoming stack arguments loaded from `[rbp + 16 + 8*k]`,
//! - 16-byte stack alignment across the outgoing-argument area.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister,
};
use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn phys(id: u8) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(id)))
}

fn rbp_mem(offset: i32) -> MachineOperand {
    MachineOperand::Memory {
        base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
        offset,
        index: None,
    }
}

fn rsp_mem(offset: i32) -> MachineOperand {
    MachineOperand::Memory {
        base: MachineRegister::Physical(PhysicalRegister(4)), // RSP
        offset,
        index: None,
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn native_x86_64_link_and_execute_calls_branches_stack_args() {
    let dir = tempdir().expect("tempdir");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = create_backend(target.clone()).expect("backend creation");

    let mut module = NativeModule::new("e2e_module");

    // add6(a, b, c, d, e, f) -> a+b+c+d+e+f
    // Win64 x64 calling convention: args 1..4 arrive in RCX/RDX/R8/R9, args
    // 5..6 on the caller's stack at [rbp+16] and [rbp+24] (in the callee).
    let mut add6 = MachineFunction::new("add6");
    add6.is_exported = true;
    {
        let b = add6.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0),
            src: phys(1),
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(2),
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(8),
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(9),
        });
        b.push(MachineInstruction::Load {
            dst: phys(10),
            src: rbp_mem(16 + 32), // arg 5: above return address and the 32-byte shadow space
            size: 8,
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(10),
        });
        b.push(MachineInstruction::Load {
            dst: phys(10),
            src: rbp_mem(16 + 32 + 8), // arg 6
            size: 8,
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(10),
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(add6);

    // main: add6(1, 2, 3, 4, 5, 6), expecting 21.
    // Args 5 and 6 are stored above the 32-byte shadow space; the outgoing
    // area is 48 bytes total (16-byte aligned).
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let entry = main_fn.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: phys(1),
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(2),
            src: MachineOperand::Immediate(2),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(8),
            src: MachineOperand::Immediate(3),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(9),
            src: MachineOperand::Immediate(4),
        });
        // Reserve shadow space (32) + two stack args (16), 16-byte aligned.
        entry.push(MachineInstruction::Sub {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(48),
        });
        entry.push(MachineInstruction::Store {
            dst: rsp_mem(32), // arg 5, above the shadow space
            src: MachineOperand::Immediate(5),
            size: 8,
        });
        entry.push(MachineInstruction::Store {
            dst: rsp_mem(40), // arg 6
            src: MachineOperand::Immediate(6),
            size: 8,
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("add6".to_string()),
            num_args: 6,
        });
        entry.push(MachineInstruction::Add {
            dst: phys(4), // RSP (caller cleans the outgoing area)
            src: MachineOperand::Immediate(48),
        });
        entry.push(MachineInstruction::Compare {
            lhs: phys(0),
            rhs: MachineOperand::Immediate(21),
        });
        // Forward conditional branch: must be resolved via a fixup pass.
        entry.push(MachineInstruction::BranchCc {
            cc: ConditionCode::NotEqual,
            target: "fail".to_string(),
        });
    }
    let ok_id = main_fn.create_block("ok");
    main_fn.blocks[ok_id as usize].push(MachineInstruction::Return);
    let fail_id = main_fn.create_block("fail");
    main_fn.blocks[fail_id as usize].push(MachineInstruction::Move {
        dst: phys(0),
        src: MachineOperand::Immediate(7),
    });
    main_fn.blocks[fail_id as usize].push(MachineInstruction::Return);
    module.add_function(main_fn);

    // 1. Native codegen -> validated ADOB object.
    let obj = backend.emit_object(&module).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");
    let text = obj.find_section(".text").expect(".text section").1;
    assert!(
        !text.relocations.is_empty(),
        "the call to add6 must emit a PC32 relocation"
    );
    assert!(
        text.relocations.iter().any(|r| r.symbol_name == "add6"),
        "relocations must reference the called function"
    );

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let adob_path = dir.path().join("e2e.adob");
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    // 2. Link into a PE executable with the native linker.
    let exe_path = dir.path().join("e2e.exe");
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    // 3. Execute it. The synthesized startup propagates main's return value
    //    through ExitProcess, so the exit code must be 21.
    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    assert_eq!(
        out.status.code(),
        Some(21),
        "native binary must compute 1+2+3+4+5+6 = 21 (got exit code {:?})",
        out.status.code()
    );
}
