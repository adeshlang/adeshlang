//! Phase 2 Task P2-4: Struct-by-Value Arguments and Returns End-to-End Tests.
//!
//! Verifies:
//! 1. Windows x64 small struct passing by value in registers (RCX, RDX, R8, R9) for sizes 1, 2, 4, 8 bytes.
//! 2. Windows x64 small struct passing by value on stack (argument 5+).
//! 3. Windows x64 small struct return by value in RAX (sizes 1, 2, 4, 8 bytes).
//! 4. System V AMD64 eightbyte classification for 1-eightbyte and 2-eightbyte structs in registers.
//! 5. End-to-end execution of compiled binaries exercising struct-by-value calls and returns.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::abi::{
    AbiType, ArgumentLocation, EightbyteClass, ReturnLocation, StackArgument, SystemVX64Abi,
    WindowsX64Abi, classify_eightbytes, classify_sysv_arguments, classify_sysv_return,
    classify_win64_arguments, classify_win64_return,
};
use adesh_codegen::ffi::{FfiCallLowerer, ForeignFunctionDeclaration, ForeignType};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    MoveLocation, NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
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
fn test_win64_struct_by_value_classification() {
    let s1 = AbiType::Struct {
        fields: vec![AbiType::Integer {
            bits: 8,
            is_signed: false,
        }],
        size: 1,
        align: 1,
    };
    let s2 = AbiType::Struct {
        fields: vec![AbiType::Integer {
            bits: 16,
            is_signed: false,
        }],
        size: 2,
        align: 2,
    };
    let s4 = AbiType::Struct {
        fields: vec![AbiType::i32()],
        size: 4,
        align: 4,
    };
    let s8 = AbiType::Struct {
        fields: vec![AbiType::i64()],
        size: 8,
        align: 8,
    };
    let s12 = AbiType::Struct {
        fields: vec![AbiType::i64(), AbiType::i32()],
        size: 12,
        align: 8,
    };

    // Args: (s1, s2, s4, s8, s8)
    // 0..3 in RCX, RDX, R8, R9; 4 on stack
    let locs =
        classify_win64_arguments(&[s1.clone(), s2.clone(), s4.clone(), s8.clone(), s8.clone()]);
    assert_eq!(locs[0], ArgumentLocation::Register(PhysicalRegister(1))); // RCX
    assert_eq!(locs[1], ArgumentLocation::Register(PhysicalRegister(2))); // RDX
    assert_eq!(locs[2], ArgumentLocation::Register(PhysicalRegister(8))); // R8
    assert_eq!(locs[3], ArgumentLocation::Register(PhysicalRegister(9))); // R9
    assert_eq!(
        locs[4],
        ArgumentLocation::Stack(StackArgument {
            offset: 48,
            size: 8,
            align: 8,
        })
    );

    // s12 > 8 bytes: indirect by reference
    let locs_large = classify_win64_arguments(&[s12.clone()]);
    assert_eq!(
        locs_large[0],
        ArgumentLocation::IndirectByReference(PhysicalRegister(1))
    );

    // Returns: s1, s2, s4, s8 returned in RAX; s12 returned via HiddenSret(RCX)
    assert_eq!(
        classify_win64_return(&s1),
        ReturnLocation::Register(PhysicalRegister(0))
    );
    assert_eq!(
        classify_win64_return(&s2),
        ReturnLocation::Register(PhysicalRegister(0))
    );
    assert_eq!(
        classify_win64_return(&s4),
        ReturnLocation::Register(PhysicalRegister(0))
    );
    assert_eq!(
        classify_win64_return(&s8),
        ReturnLocation::Register(PhysicalRegister(0))
    );
    assert_eq!(
        classify_win64_return(&s12),
        ReturnLocation::HiddenSret(PhysicalRegister(1))
    );
}

#[test]
fn test_sysv_struct_by_value_classification() {
    // 1-eightbyte struct (i32, i32) -> EightbyteClass::Integer in RDI
    let s8 = AbiType::Struct {
        fields: vec![AbiType::i32(), AbiType::i32()],
        size: 8,
        align: 4,
    };
    let classes_s8 = classify_eightbytes(&s8);
    assert_eq!(classes_s8, vec![EightbyteClass::Integer]);
    let locs_s8 = classify_sysv_arguments(&[s8.clone()]);
    assert_eq!(locs_s8[0], ArgumentLocation::Register(PhysicalRegister(7))); // RDI

    // 2-eightbyte struct (f64, f64) -> Sse, Sse in XMM0, XMM1
    let s16_fp = AbiType::Struct {
        fields: vec![AbiType::f64(), AbiType::f64()],
        size: 16,
        align: 8,
    };
    let classes_fp = classify_eightbytes(&s16_fp);
    assert_eq!(classes_fp, vec![EightbyteClass::Sse, EightbyteClass::Sse]);

    // Return: s8 in RAX; s16_fp in Pair(XMM0, XMM1)
    assert_eq!(
        classify_sysv_return(&s8),
        ReturnLocation::Register(PhysicalRegister(0))
    );
    assert_eq!(
        classify_sysv_return(&s16_fp),
        ReturnLocation::Pair(PhysicalRegister::xmm(0), PhysicalRegister::xmm(1))
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_struct_by_value_4byte_execution_e2e() {
    // Struct { a: i16, b: i16 } packed into 4-byte scalar:
    // a = 12, b = 25 -> packed = (25 << 16) | 12 = 0x0019000C = 1638412
    // Callee extracts (packed & 0xFFFF) + (packed >> 16) = 12 + 25 = 37.
    let mut module = NativeModule::new("test_struct_4byte_module");

    // Callee: unpack_sum_4b(s: Struct4B) -> i64
    let mut callee = MachineFunction::new("unpack_sum_4b");
    callee.is_exported = true;
    let c_packed = callee.alloc_vreg();
    let c_a = callee.alloc_vreg();
    let c_b = callee.alloc_vreg();
    let c_sum = callee.alloc_vreg();

    let c_block = callee.entry_block_mut();
    // Copy RCX into c_packed
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_packed)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    // c_a = c_packed & 0xFFFF
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_a)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_packed)),
    });
    c_block.push(MachineInstruction::And {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_a)),
        src: MachineOperand::Immediate(0xFFFF),
    });
    // c_b = c_packed >> 16
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_b)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_packed)),
    });
    c_block.push(MachineInstruction::Shr {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_b)),
        src: MachineOperand::Immediate(16),
    });
    // c_sum = c_a + c_b
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_a)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_b)),
    });
    // Return in RAX
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Caller (main):
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let v_packed = main_func.alloc_vreg();
    let v_ret = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    // Packed 4-byte struct value: 12 in low 16 bits, 25 in high 16 bits
    let packed_val: i64 = (25 << 16) | 12;
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_packed)),
        src: MachineOperand::Immediate(packed_val),
    });

    let s4_type = ForeignType::Struct {
        fields: vec![ForeignType::Int16, ForeignType::Int16],
        size: 4,
        align: 2,
    };
    let decl = ForeignFunctionDeclaration::c_fn(
        "unpack_sum_4b",
        vec![("s", s4_type)],
        ForeignType::Int64,
        false,
    );
    let win64_abi = WindowsX64Abi;
    let call_insts = FfiCallLowerer::lower_call(&decl, &[v_packed], &win64_abi, Some(v_ret))
        .expect("lowering struct by value call");

    for inst in call_insts {
        m_block.push(inst);
    }

    // Return in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_ret)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_struct_4b_exec");
    // 12 + 25 = 37
    assert_eq!(
        code, 37,
        "unpack_sum_4b with 4-byte struct passed by value should return 37"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_struct_by_value_8byte_return_execution_e2e() {
    // Callee: make_struct_8b(a: i64, b: i64) -> Struct8B { x: i32, y: i32 }
    // Returned packed into RAX: (b << 32) | (a & 0xFFFFFFFF)
    let mut module = NativeModule::new("test_struct_8b_ret_module");

    let mut callee = MachineFunction::new("make_struct_8b");
    callee.is_exported = true;
    let c_a = callee.alloc_vreg();
    let c_b = callee.alloc_vreg();
    let c_res = callee.alloc_vreg();

    let c_block = callee.entry_block_mut();
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_a)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_b)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))), // RDX
    });
    // c_b = c_b << 32
    c_block.push(MachineInstruction::Shl {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_b)),
        src: MachineOperand::Immediate(32),
    });
    // c_res = (c_a & 0xFFFFFFFF) | c_b
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_res)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_a)),
    });
    c_block.push(MachineInstruction::And {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_res)),
        src: MachineOperand::Immediate(0xFFFFFFFF),
    });
    c_block.push(MachineInstruction::Or {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_res)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_b)),
    });
    // Return struct in RAX
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(c_res)),
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Caller (main): calls make_struct_8b(15, 20) -> packed struct in RAX
    // extracts x + y = 15 + 20 = 35.
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let v_a = main_func.alloc_vreg();
    let v_b = main_func.alloc_vreg();
    let v_s8 = main_func.alloc_vreg();
    let v_x = main_func.alloc_vreg();
    let v_y = main_func.alloc_vreg();
    let v_sum = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_a)),
        src: MachineOperand::Immediate(15),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_b)),
        src: MachineOperand::Immediate(20),
    });

    let s8_type = ForeignType::Struct {
        fields: vec![ForeignType::Int32, ForeignType::Int32],
        size: 8,
        align: 4,
    };
    let decl = ForeignFunctionDeclaration::c_fn(
        "make_struct_8b",
        vec![("a", ForeignType::Int64), ("b", ForeignType::Int64)],
        s8_type,
        false,
    );
    let win64_abi = WindowsX64Abi;
    let call_insts = FfiCallLowerer::lower_call(&decl, &[v_a, v_b], &win64_abi, Some(v_s8))
        .expect("lowering struct by value return");

    for inst in call_insts {
        m_block.push(inst);
    }

    // Extract x: v_x = v_s8 & 0xFFFFFFFF
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_x)),
        src: MachineOperand::Register(MachineRegister::Virtual(v_s8)),
    });
    m_block.push(MachineInstruction::And {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_x)),
        src: MachineOperand::Immediate(0xFFFFFFFF),
    });
    // Extract y: v_y = v_s8 >> 32
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_y)),
        src: MachineOperand::Register(MachineRegister::Virtual(v_s8)),
    });
    m_block.push(MachineInstruction::Shr {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_y)),
        src: MachineOperand::Immediate(32),
    });
    // v_sum = v_x + v_y
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(v_x)),
    });
    m_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(v_y)),
    });

    // Return sum in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_sum)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_struct_8b_ret_exec");
    // 15 + 20 = 35
    assert_eq!(
        code, 35,
        "make_struct_8b returning 8-byte struct in RAX should unpack to 35"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_mixed_args_with_struct_on_stack_execution_e2e() {
    // 5 arguments: (a: i64, b: i64, c: i64, d: i64, s: Struct8B)
    // a in RCX, b in RDX, c in R8, d in R9, s on stack at [RBP + 48]
    // Callee calculates: a + b + c + d + (s.x + s.y)
    let mut module = NativeModule::new("test_mixed_stack_struct_module");

    let mut callee = MachineFunction::new("sum_mixed_5args");
    callee.is_exported = true;
    let c_a = callee.alloc_vreg();
    let c_b = callee.alloc_vreg();
    let c_c = callee.alloc_vreg();
    let c_d = callee.alloc_vreg();
    let c_s = callee.alloc_vreg();
    let c_sum = callee.alloc_vreg();

    let c_block = callee.entry_block_mut();
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_a)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))), // RCX
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_b)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))), // RDX
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_c)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(8))), // R8
    });
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_d)),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(9))), // R9
    });
    // Load 5th argument (s) from caller's stack slot at [RBP + 48]
    c_block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_s)),
        src: MachineOperand::Memory {
            base: MachineRegister::Physical(PhysicalRegister(5)), // RBP
            offset: 48,
            index: None,
        },
        size: 8,
    });

    // c_sum = c_a + c_b + c_c + c_d + c_s
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_a)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_b)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_c)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_d)),
    });
    c_block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
        src: MachineOperand::Register(MachineRegister::Virtual(c_s)),
    });

    // Return in RAX
    c_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(c_sum)),
    });
    c_block.push(MachineInstruction::Return);
    module.add_function(callee);

    // Caller (main):
    // a = 1, b = 2, c = 3, d = 4, s = 10
    // Total sum = 1 + 2 + 3 + 4 + 10 = 20.
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let v_a = main_func.alloc_vreg();
    let v_b = main_func.alloc_vreg();
    let v_c = main_func.alloc_vreg();
    let v_d = main_func.alloc_vreg();
    let v_s = main_func.alloc_vreg();
    let v_ret = main_func.alloc_vreg();

    let m_block = main_func.entry_block_mut();
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_a)),
        src: MachineOperand::Immediate(1),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_b)),
        src: MachineOperand::Immediate(2),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_c)),
        src: MachineOperand::Immediate(3),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_d)),
        src: MachineOperand::Immediate(4),
    });
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v_s)),
        src: MachineOperand::Immediate(10),
    });

    let s8_type = ForeignType::Struct {
        fields: vec![ForeignType::Int64],
        size: 8,
        align: 8,
    };
    let decl = ForeignFunctionDeclaration::c_fn(
        "sum_mixed_5args",
        vec![
            ("a", ForeignType::Int64),
            ("b", ForeignType::Int64),
            ("c", ForeignType::Int64),
            ("d", ForeignType::Int64),
            ("s", s8_type),
        ],
        ForeignType::Int64,
        false,
    );
    let win64_abi = WindowsX64Abi;
    let call_insts =
        FfiCallLowerer::lower_call(&decl, &[v_a, v_b, v_c, v_d, v_s], &win64_abi, Some(v_ret))
            .expect("lowering 5-argument call with struct on stack");

    for inst in call_insts {
        m_block.push(inst);
    }

    // Return in RAX
    m_block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v_ret)),
    });
    m_block.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_mixed_stack_struct_exec");
    // 1 + 2 + 3 + 4 + 10 = 20
    assert_eq!(
        code, 20,
        "sum_mixed_5args with 5th argument struct on stack should return 20"
    );
}
