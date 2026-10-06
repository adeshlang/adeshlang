//! Stack canary execution tests: the protected function must return normally
//! when its frame is intact and must trap through `__stack_chk_fail` when the
//! canary slot is overwritten.

#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::safety::StackCanaryPass;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

/// `STATUS_ILLEGAL_INSTRUCTION`, raised by the `ud2` in `__stack_chk_fail`.
const STATUS_ILLEGAL_INSTRUCTION: i32 = 0xC000_001Du32 as i32;

fn build_main(smash_canary: bool) -> NativeModule {
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v = func.alloc_vreg();
    let block = func.entry_block_mut();
    if smash_canary {
        // The pass places the canary at the first slot below the frame
        // (`-(stack_size + 8)`, i.e. `-8` for a frameless function).
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
            src: MachineOperand::Immediate(0x4141_4141),
        });
        block.push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(-8),
            src: MachineOperand::Register(MachineRegister::Virtual(v)),
            size: 8,
        });
    }
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    block.push(MachineInstruction::Return);

    StackCanaryPass::new().instrument_function(&mut func);

    let mut module = NativeModule::new("canary");
    module.add_function(func);
    module
}

fn run(module: &NativeModule, name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(OptLevel::O0);
    let obj = backend.emit_object(module).expect("ADOB emission");
    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");

    let dir = tempdir().expect("tempdir");
    let adob = dir.path().join(format!("{name}.adob"));
    std::fs::write(&adob, bytes).expect("write ADOB");
    let exe = dir.path().join(format!("{name}.exe"));
    adesh_linker::link(&[&adob], &exe, Some("x86_64-pc-windows-msvc")).expect("native link");

    let out = Command::new(&exe).output().expect("execute");
    out.status.code().expect("exit code")
}

#[test]
fn test_stack_canary_intact_frame_returns_normally() {
    assert_eq!(run(&build_main(false), "canary_ok"), 42);
}

#[test]
fn test_stack_canary_smashed_frame_traps() {
    assert_eq!(
        run(&build_main(true), "canary_smashed"),
        STATUS_ILLEGAL_INSTRUCTION
    );
}
