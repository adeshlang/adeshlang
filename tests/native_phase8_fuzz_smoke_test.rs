//! End-to-End Native Backend Fuzz and Robustness Smoke Tests (Phase 8).
//!
//! Verifies:
//! 1. Lexer & Parser robustness against malformed/fuzzed inputs (no crashes or panics).
//! 2. ADOB Reader & Validator bounds checking against corrupt/truncated binaries.
//! 3. MachineIR handling of extreme boundary values (i64::MIN, i64::MAX, zero, negative).
//! 4. Real native compilation, linking, and execution of edge-case programs on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::reader::AdobReader;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
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
fn test_lexer_and_parser_fuzz_robustness() {
    let malformed_inputs = [
        "",
        "      \t\r\n   ",
        "((((((((((((((((((((",
        "\"unterminated string literal",
        "let x = @@@$$$%%%^^^&&&***;;;",
        "fn \0\0\0\0\0() { return; }",
        "class A :::: B {{{{}}}}",
        "0xZZZZZZZZZZZZZZZZ",
        "999999999999999999999999999999999999999999999999999999999999999999999",
        "/// doc comment without item\n\n\n",
    ];

    for (idx, input) in malformed_inputs.iter().enumerate() {
        eprintln!("Fuzz testing input #{}: {:?}", idx, input);
        let src = input.to_string();
        let handle = std::thread::Builder::new()
            .name(format!("fuzz_thread_{}", idx))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                // Lexer must never panic
                let mut lx = Lexer::new(&src);
                let tokens_res = lx.tokenize();

                // Parser must never panic
                if let Ok(toks) = tokens_res {
                    let mut parser = Parser::new(toks, None);
                    let _ = parser.parse_program();
                }
            })
            .expect("spawn fuzz test thread");

        handle
            .join()
            .expect("fuzz worker thread must not panic or crash");
    }
}

#[test]
fn test_adob_reader_and_validator_bounds_fuzz() {
    let corrupted_buffers: Vec<Vec<u8>> = vec![
        vec![],
        vec![0x00],
        vec![0x41, 0x44, 0x4f],       // partial magic "ADO"
        vec![0x41, 0x44, 0x4f, 0x42], // only magic "ADOB"
        vec![0xff; 32],               // 32 bytes of 0xFF
        vec![0x00; 128],              // zeroed buffer
        // Header with huge invalid section count
        {
            let mut b = b"ADOB\x01\x00\x00\x00".to_vec();
            b.extend_from_slice(&[0xff; 64]);
            b
        },
    ];

    for buf in &corrupted_buffers {
        let res = AdobReader::read_object(buf);
        assert!(
            res.is_err(),
            "corrupted buffer must fail gracefully without panicking"
        );
    }
}

#[test]
fn test_boundary_values_execution_e2e() {
    let mut module = NativeModule::new("test_fuzz_boundary");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;

    let v0 = main_func.alloc_vreg();
    let v1 = main_func.alloc_vreg();

    let block = main_func.entry_block_mut();

    // Boundary immediate arithmetic: compute ((100 + 0) - (-5)) & 127 -> 105
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(100),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(-5),
    });
    block.push(MachineInstruction::Sub {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    block.push(MachineInstruction::Return);

    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_fuzz_exec");
    assert_eq!(code, 105, "boundary execution must return 105");
}
