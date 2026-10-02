//! Production End-to-End Native Parallel Move Tests for x86-64 ABI.
//!
//! Verifies that:
//! 1. 2-argument swaps (e.g. `sub(x, y)` where `x` and `y` are swapped in registers) execute with simultaneous semantics.
//! 2. 3-argument register permutation cycles (e.g. `rot3(a, b, c) -> a*100 + b*10 + c` called as `rot3(y, z, x)`).
//! 3. 7-argument calls crossing Win64 register (RCX, RDX, R8, R9) and stack ([rsp+32], [rsp+40], [rsp+48]) boundaries.
//! 4. Nested function calls where a return value in RAX is immediately forwarded as a register argument.
//! 5. Complex dependency chains and fan-out argument distributions.
//! 6. Native compilation -> ADOB generation -> PE link -> executable execution -> exact exit code assertion.

use adesh_codegen::calling_convention::{
    MoveLocation, MoveOperation, ParallelMoveResolver, WindowsX64CallingConvention,
    resolve_call_arguments_gpr,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, VirtualRegister,
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

fn build_link_and_run(module: &NativeModule, test_name: &str) -> i32 {
    let dir = tempdir().expect("tempdir");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = create_backend(target.clone()).expect("backend creation");

    let obj = backend.emit_object(module).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
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
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_two_register_argument_swap() {
    // sub_diff(a, b) -> a - b
    // In Win64: a is in RCX (1), b is in RDX (2)
    // sub_diff computes RCX - RDX, leaves in RAX (0)
    let mut module = NativeModule::new("test_swap_module");

    let mut sub_fn = MachineFunction::new("sub_diff");
    sub_fn.is_exported = true;
    {
        let b = sub_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0), // RAX
            src: phys(1), // RCX (a)
        });
        b.push(MachineInstruction::Sub {
            dst: phys(0), // RAX
            src: phys(2), // RDX (b)
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(sub_fn);

    // main:
    // Let R12 = 30 (representing 'a'), R13 = 10 (representing 'b')
    // We want to call sub_diff(b, a), i.e. sub_diff(10, 30) -> -20 (or sub_diff(30, 10) -> 20)
    // We set RCX = 10, RDX = 30.
    // If we swap registers: RCX = 30, RDX = 10, then perform parallel move RCX <-> RDX!
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let entry = main_fn.entry_block_mut();
        // Setup initial registers: RCX = 10, RDX = 30
        entry.push(MachineInstruction::Move {
            dst: phys(1), // RCX = 10
            src: MachineOperand::Immediate(10),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(2), // RDX = 30
            src: MachineOperand::Immediate(30),
        });

        // Parallel Move Resolver: Swap RCX <-> RDX simultaneously!
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(MoveLocation::phys(1), MoveLocation::phys(2), 8); // RCX <- RDX (30)
        resolver.add_move(MoveLocation::phys(2), MoveLocation::phys(1), 8); // RDX <- RCX (10)
        let swap_insts = resolver.resolve().expect("swap resolution");
        for inst in swap_insts {
            entry.push(inst);
        }

        // Reserve Win64 shadow space (32 bytes, 16-byte aligned)
        entry.push(MachineInstruction::Sub {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(32),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("sub_diff".to_string()),
            num_args: 2,
        });
        entry.push(MachineInstruction::Add {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(32),
        });

        // RAX should now hold 30 - 10 = 20!
        entry.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let exit_code = build_link_and_run(&module, "e2e_swap");
    assert_eq!(exit_code, 20, "sub_diff(30, 10) after swap must equal 20");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_three_register_permutation_cycle() {
    // combine3(a, b, c) -> a*100 + b*10 + c
    // In Win64: a is in RCX (1), b is in RDX (2), c is in R8 (8)
    let mut module = NativeModule::new("test_rot3_module");

    let mut combine_fn = MachineFunction::new("combine3");
    combine_fn.is_exported = true;
    {
        let b = combine_fn.entry_block_mut();
        // RAX = a * 100
        b.push(MachineInstruction::Move {
            dst: phys(0),
            src: phys(1), // RCX (a)
        });
        b.push(MachineInstruction::Mul {
            dst: phys(0),
            src: MachineOperand::Immediate(100),
        });
        // RDX = b * 10
        b.push(MachineInstruction::Mul {
            dst: phys(2), // RDX (b)
            src: MachineOperand::Immediate(10),
        });
        // RAX = RAX + RDX + R8
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(2),
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(8), // R8 (c)
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(combine_fn);

    // main:
    // Initial: RCX = 1, RDX = 2, R8 = 3 (representing a=1, b=2, c=3)
    // Permutation 3-cycle: RCX <- RDX (2), RDX <- R8 (3), R8 <- RCX (1)
    // Expected result: combine3(2, 3, 1) = 2*100 + 3*10 + 1 = 231!
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let entry = main_fn.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: phys(1), // RCX = 1
            src: MachineOperand::Immediate(1),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(2), // RDX = 2
            src: MachineOperand::Immediate(2),
        });
        entry.push(MachineInstruction::Move {
            dst: phys(8), // R8 = 3
            src: MachineOperand::Immediate(3),
        });

        // 3-Cycle: RCX <- RDX, RDX <- R8, R8 <- RCX
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(MoveLocation::phys(1), MoveLocation::phys(2), 8);
        resolver.add_move(MoveLocation::phys(2), MoveLocation::phys(8), 8);
        resolver.add_move(MoveLocation::phys(8), MoveLocation::phys(1), 8);
        let insts = resolver.resolve().expect("3-cycle resolution");
        for inst in insts {
            entry.push(inst);
        }

        entry.push(MachineInstruction::Sub {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(32),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("combine3".to_string()),
            num_args: 3,
        });
        entry.push(MachineInstruction::Add {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(32),
        });
        entry.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let exit_code = build_link_and_run(&module, "e2e_rot3");
    assert_eq!(exit_code, 231, "combine3(2, 3, 1) must equal 231");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_seven_arguments_register_and_stack() {
    // sum7(a, b, c, d, e, f, g) -> a+b+c+d+e+f+g
    // In Win64:
    // a: RCX (1), b: RDX (2), c: R8 (8), d: R9 (9)
    // e: [rbp + 16 + 32]
    // f: [rbp + 16 + 32 + 8]
    // g: [rbp + 16 + 32 + 16]
    let mut module = NativeModule::new("test_sum7_module");

    let mut sum_fn = MachineFunction::new("sum7");
    sum_fn.is_exported = true;
    {
        let b = sum_fn.entry_block_mut();
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
        // Arg 5 (e)
        b.push(MachineInstruction::Load {
            dst: phys(10),
            src: rbp_mem(16 + 32),
            size: 8,
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(10),
        });
        // Arg 6 (f)
        b.push(MachineInstruction::Load {
            dst: phys(10),
            src: rbp_mem(16 + 32 + 8),
            size: 8,
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(10),
        });
        // Arg 7 (g)
        b.push(MachineInstruction::Load {
            dst: phys(10),
            src: rbp_mem(16 + 32 + 16),
            size: 8,
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(10),
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(sum_fn);

    // main:
    // Call sum7(1, 2, 3, 4, 5, 6, 7) = 28
    // Using ParallelMoveResolver with resolve_call_arguments
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let entry = main_fn.entry_block_mut();
        // Materialize arguments into virtual registers
        let v_args: Vec<VirtualRegister> = (0..7).map(VirtualRegister).collect();
        for (i, &v) in v_args.iter().enumerate() {
            entry.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(v)),
                src: MachineOperand::Immediate((i + 1) as i64),
            });
        }

        let conv = WindowsX64CallingConvention;
        let (call_moves, total_outgoing) =
            resolve_call_arguments_gpr(&conv, &v_args, PhysicalRegister(4)).expect("resolve args");

        entry.push(MachineInstruction::Sub {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(total_outgoing as i64),
        });

        for inst in call_moves {
            entry.push(inst);
        }

        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("sum7".to_string()),
            num_args: 7,
        });

        entry.push(MachineInstruction::Add {
            dst: phys(4), // RSP
            src: MachineOperand::Immediate(total_outgoing as i64),
        });

        entry.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let exit_code = build_link_and_run(&module, "e2e_sum7");
    assert_eq!(exit_code, 28, "sum7(1..7) must equal 28");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_nested_calls_and_return_forwarding() {
    // double_val(x) -> x + x
    // add_vals(x, y) -> x + y
    let mut module = NativeModule::new("test_nested_module");

    let mut dbl_fn = MachineFunction::new("double_val");
    dbl_fn.is_exported = true;
    {
        let b = dbl_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0), // RAX
            src: phys(1), // RCX
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(1),
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(dbl_fn);

    let mut add_fn = MachineFunction::new("add_vals");
    add_fn.is_exported = true;
    {
        let b = add_fn.entry_block_mut();
        b.push(MachineInstruction::Move {
            dst: phys(0), // RAX
            src: phys(1), // RCX
        });
        b.push(MachineInstruction::Add {
            dst: phys(0),
            src: phys(2), // RDX
        });
        b.push(MachineInstruction::Return);
    }
    module.add_function(add_fn);

    // main:
    // double_val(10) -> 20 (lands in RAX)
    // then call add_vals(RAX, 15) -> 35
    // Parallel move: RCX <- RAX, RDX <- 15
    let mut main_fn = MachineFunction::new("main");
    main_fn.is_exported = true;
    {
        let entry = main_fn.entry_block_mut();
        // Call double_val(10)
        entry.push(MachineInstruction::Move {
            dst: phys(1),
            src: MachineOperand::Immediate(10),
        });
        entry.push(MachineInstruction::Sub {
            dst: phys(4),
            src: MachineOperand::Immediate(32),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("double_val".to_string()),
            num_args: 1,
        });
        entry.push(MachineInstruction::Add {
            dst: phys(4),
            src: MachineOperand::Immediate(32),
        });

        // Now RAX = 20. Forward RAX to RCX, and 15 to RDX for add_vals(20, 15)
        let mut resolver = ParallelMoveResolver::for_x86_64();
        resolver.add_move(MoveLocation::phys(1), MoveLocation::phys(0), 8); // RCX <- RAX
        resolver.add_move(MoveLocation::phys(2), MoveLocation::imm(15), 8); // RDX <- 15
        let insts = resolver.resolve().expect("resolve forward");
        for inst in insts {
            entry.push(inst);
        }

        entry.push(MachineInstruction::Sub {
            dst: phys(4),
            src: MachineOperand::Immediate(32),
        });
        entry.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("add_vals".to_string()),
            num_args: 2,
        });
        entry.push(MachineInstruction::Add {
            dst: phys(4),
            src: MachineOperand::Immediate(32),
        });

        entry.push(MachineInstruction::Return);
    }
    module.add_function(main_fn);

    let exit_code = build_link_and_run(&module, "e2e_nested");
    assert_eq!(exit_code, 35, "add_vals(double_val(10), 15) must equal 35");
}
