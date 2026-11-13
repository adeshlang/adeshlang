//! Width-correct scalar FP: native ADOB -> native ELF linker -> QEMU.
//! GNU-assembled references independently check result bits and FP exceptions.
#[path = "../../../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use adesh_codegen::{
    CodegenBackend,
    machine_ir::{
        MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
        PhysicalRegister,
    },
    targets::aarch64::AArch64Backend,
};
use adesh_linker::{config::LinkConfig, linker::Linker, target::Target};
use adesh_object::{AdobWriter, TargetDescriptor};
use foreign_exec::LinuxTools;
use std::ffi::OsStr;

fn fp(n: u8) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32 + n)))
}

#[test]
fn scalar_fp_widths_results_and_exceptions_execute() {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut backend =
        AArch64Backend::new(TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap());
    let mut module = NativeModule::new("scalar_fp");
    let mut oracle = String::from(".text\n.global _start\n_start:\nmsr fpcr, xzr\nmov x19, sp\n");
    // Finite, overflow, underflow, subnormal, signed zero, division by zero,
    // infinity, NaN payloads, and an inexact rounding boundary.
    let f32_cases = [
        (0x3fc00000u64, 0x40100000u64),
        (0x7f7fffff, 0x40000000),
        (0x00800000, 0x3f000000),
        (0x00000001, 0x40000000),
        (0, 0x80000000),
        (0x3f800000, 0),
        (0x7f800000, 0xff800000),
        (0x7fc12345, 0x3f800000),
        (0x3f800001, 0x3f800000),
    ];
    let f64_cases = [
        (0x3ff8000000000000u64, 0x4002000000000000u64),
        (0x7fefffffffffffff, 0x4000000000000000),
        (0x0010000000000000, 0x3fe0000000000000),
        (1, 0x4000000000000000),
        (0, 0x8000000000000000),
        (0x3ff0000000000000, 0),
        (0x7ff0000000000000, 0xfff0000000000000),
        (0x7ff8123456789abc, 0x3ff0000000000000),
        (0x3ff0000000000001, 0x3ff0000000000000),
    ];
    for size in [4, 8] {
        let prefix = if size == 4 { "s" } else { "d" };
        let result_gpr = if size == 4 { "w21" } else { "x21" };
        let cases = if size == 4 { &f32_cases } else { &f64_cases };
        for (mnemonic, instruction) in [
            (
                "fadd",
                MachineInstruction::FAdd {
                    dst: fp(0),
                    src: fp(31),
                    size,
                },
            ),
            (
                "fsub",
                MachineInstruction::FSub {
                    dst: fp(0),
                    src: fp(31),
                    size,
                },
            ),
            (
                "fmul",
                MachineInstruction::FMul {
                    dst: fp(0),
                    src: fp(31),
                    size,
                },
            ),
            (
                "fdiv",
                MachineInstruction::FDiv {
                    dst: fp(0),
                    src: fp(31),
                    size,
                },
            ),
        ] {
            let name = format!("{mnemonic}_{size}");
            let mut func = MachineFunction::new(&name);
            func.is_exported = true;
            func.entry_block_mut().push(instruction);
            func.entry_block_mut().push(MachineInstruction::Return);
            module.add_function(func);
            for &(a, b) in cases {
                // Poison the unused high half for S operations, so an accidental
                // D instruction cannot pass as a zero-extended scalar operation.
                let poison = if size == 4 { 0x4008000000000000 } else { 0 };
                oracle.push_str(&format!(
                    "ldr x9, ={a}\nfmov d0, x9\nldr x10, ={b}\nfmov d31, x10\n\
                     msr fpsr, xzr\n{mnemonic} {prefix}2, {prefix}0, {prefix}31\n\
                     fmov {result_gpr}, {prefix}2\nmrs x22, fpsr\n\
                     ldr x9, ={}\nfmov d0, x9\nldr x10, ={}\nfmov d31, x10\n\
                     msr fpsr, xzr\nbl {name}\nfmov x9, d0\ncmp x9, x21\nb.ne fail\n\
                     mrs x9, fpsr\ncmp x9, x22\nb.ne fail\n\
                     mov x9, sp\ncmp x9, x19\nb.ne fail\n",
                    a | poison,
                    b | poison,
                ));
            }
        }
    }
    // Force production FP32 allocation/reloads. Forty 1.5 values sum to 60.0.
    let mut func = MachineFunction::new("fp32_spills");
    func.is_exported = true;
    let regs: Vec<_> = (0..40).map(|_| func.alloc_fp_vreg()).collect();
    let op = |v| MachineOperand::Register(MachineRegister::Virtual(v));
    let block = func.entry_block_mut();
    for &reg in &regs {
        block.push(MachineInstruction::Move {
            dst: op(reg),
            src: fp(0),
        });
    }
    for &reg in &regs[1..] {
        block.push(MachineInstruction::FAdd {
            dst: op(regs[0]),
            src: op(reg),
            size: 4,
        });
    }
    block.push(MachineInstruction::Move {
        dst: fp(0),
        src: op(regs[0]),
    });
    block.push(MachineInstruction::Return);
    let mut spills = NativeModule::new("spills");
    spills.add_function(func);
    let lowered = backend.lower_module(&spills).unwrap();
    assert!(
        lowered.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i, MachineInstruction::Load { .. })),
        "fixture must exercise spills"
    );
    module.functions.extend(lowered.functions);
    oracle.push_str(
        "fmov s0, #1.5\nbl fp32_spills\nfmov w9, s0\nldr w10, =0x42700000\n\
         cmp w9, w10\nb.ne fail\nmov x9, sp\ncmp x9, x19\nb.ne fail\n\
         mov x0, #0\nb exit\nfail:\nmov x0, #92\nexit:\nmov x8, #93\nsvc #0\n",
    );
    let input = dir.path().join("generated.adob");
    std::fs::write(
        &input,
        AdobWriter::write(&backend.emit_object(&module).unwrap()).unwrap(),
    )
    .unwrap();
    let assembly = dir.path().join("oracle.s");
    std::fs::write(&assembly, oracle).unwrap();
    let oracle_obj = dir.path().join("oracle.o");
    let oracle_linux = format!("{}/oracle.o", tools.path(dir.path()));
    let assembly_linux = tools.path(&assembly);
    let assembled = tools.tool(
        "aarch64-linux-gnu-as",
        &[
            OsStr::new("-o"),
            OsStr::new(&oracle_linux),
            OsStr::new(&assembly_linux),
        ],
    );
    assert!(assembled.status.success(), "{assembled:?}");
    let output = dir.path().join("scalar.elf");
    let mut config = LinkConfig::new(
        output.clone(),
        Target::from_triple("aarch64-unknown-linux-gnu").unwrap(),
    );
    config.static_link = true;
    config.entry_point = Some("_start".to_owned());
    Linker::link(&[oracle_obj, input], config).unwrap();
    let output_linux = tools.path(&output);
    let executed = tools.tool("qemu-aarch64", &[OsStr::new(&output_linux)]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert!(
        executed.stdout.is_empty() && executed.stderr.is_empty(),
        "{executed:?}"
    );
}
