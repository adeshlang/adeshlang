//! Native encoder -> ADOB -> linker -> QEMU, with a GNU-assembled oracle.
#[path = "../../../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use adesh_codegen::{
    CodegenBackend,
    machine_ir::{
        MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
        PhysicalRegister,
    },
    opt::VectorType,
    targets::aarch64::AArch64Backend,
};
use adesh_linker::{config::LinkConfig, linker::Linker, target::Target};
use adesh_object::{AdobWriter, TargetDescriptor};
use foreign_exec::LinuxTools;
use std::ffi::OsStr;

fn fp(reg: u8) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32 + reg)))
}

#[test]
fn fp_and_simd_stack_roundtrips_execute() {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap();
    let mut backend = AArch64Backend::new(target);
    let mut module = NativeModule::new("stack_roundtrips");
    let mut oracle = String::from(".text\n.global _start\n_start:\nmov x19, sp\n");
    for size in [4u8, 8, 16] {
        // Small frame, unaligned transfer, and beyond every scaled imm12 range.
        for (index, frame, offset) in [(0, 64, -16), (1, 64, -17), (2, 65584, -16)] {
            let name = format!("roundtrip_{size}_{index}");
            let mut func = MachineFunction::new(&name);
            func.is_exported = true;
            func.stack_size = frame;
            let slot = MachineOperand::StackSlot(offset);
            let second = MachineOperand::StackSlot(-48);
            let block = func.entry_block_mut();
            // V31 is a real vector register, not the GPR zero/SP encoding.
            block.push(MachineInstruction::Store {
                dst: slot.clone(),
                src: fp(0),
                size,
            });
            block.push(MachineInstruction::Load {
                dst: fp(31),
                src: slot,
                size,
            });
            block.push(MachineInstruction::Store {
                dst: second.clone(),
                src: fp(31),
                size,
            });
            block.push(MachineInstruction::Load {
                dst: fp(0),
                src: second,
                size,
            });
            block.push(MachineInstruction::Return);
            module.add_function(func);
            let low = match size {
                4 => "0x89abcdef",
                _ => "0x0123456789abcdef",
            };
            let high = if size == 16 {
                "0xfedcba9876543210"
            } else {
                "0"
            };
            oracle.push_str(&format!(
                "adrp x9, pattern\nadd x9, x9, :lo12:pattern\nldr q0, [x9]\n\
                 bl {name}\numov x9, v0.d[0]\nldr x10, ={low}\ncmp x9, x10\nb.ne fail\n\
                 umov x9, v0.d[1]\nldr x10, ={high}\ncmp x9, x10\nb.ne fail\n\
                 mov x9, sp\ncmp x9, x19\nb.ne fail\n"
            ));
        }
    }
    // Exercise the production allocator's FP spill/reload path as well.
    let mut func = MachineFunction::new("scalar_spills");
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
            size: 8,
        });
    }
    block.push(MachineInstruction::Move {
        dst: fp(0),
        src: op(regs[0]),
    });
    block.push(MachineInstruction::Return);
    let mut scalar = NativeModule::new("scalar");
    scalar.add_function(func);
    let lowered = backend.lower_module(&scalar).expect("scalar allocation");
    assert!(
        lowered.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(
                |i| matches!(i, MachineInstruction::Load { dst, size: 8, .. }
            if matches!(dst, MachineOperand::Register(MachineRegister::Physical(p)) if p.0 >= 32))
            ),
        "fixture must exercise an FP reload, not only registers"
    );
    module.functions.extend(lowered.functions);
    for across_call in [false, true] {
        let mut func = MachineFunction::new(if across_call {
            "vector_call_spills"
        } else {
            "vector_spills"
        });
        func.is_exported = true;
        let regs: Vec<_> = (0..40).map(|_| func.alloc_fp_vreg()).collect();
        let block = func.entry_block_mut();
        for &reg in &regs {
            block.push(MachineInstruction::Move {
                dst: op(reg),
                src: fp(0),
            });
        }
        if across_call {
            block.push(MachineInstruction::Call {
                target: MachineOperand::Symbol("clobber_vectors".to_owned()),
                num_args: 0,
            });
        }
        // Both operands alias, including spilled destinations. Every lane matters.
        for &reg in &regs {
            block.push(MachineInstruction::VectorAdd {
                dst: op(reg),
                src: op(reg),
                vec_type: VectorType::v4f32(),
            });
        }
        for &reg in &regs[1..] {
            block.push(MachineInstruction::VectorAdd {
                dst: op(regs[0]),
                src: op(reg),
                vec_type: VectorType::v4f32(),
            });
        }
        block.push(MachineInstruction::Move {
            dst: fp(0),
            src: op(regs[0]),
        });
        block.push(MachineInstruction::Return);
        let mut vector = NativeModule::new("vector");
        vector.add_function(func);
        let lowered = backend.lower_module(&vector).expect("vector allocation");
        let instructions: Vec<_> = lowered.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .collect();
        for load in [true, false] {
            assert!(
                instructions.iter().any(|i| if load {
                    matches!(i, MachineInstruction::Load { size: 16, .. })
                } else {
                    matches!(i, MachineInstruction::Store { size: 16, .. })
                }),
                "fixture must exercise full-width vector spills"
            );
        }
        module.functions.extend(lowered.functions);
    }
    // Independent expected results distinguish FP multiply/subtract from the
    // integer opcodes previously selected for exactly the same vector type.
    for (name, instruction, expected) in [
        (
            "vector_mul",
            MachineInstruction::VectorMul {
                dst: fp(0),
                src: fp(1),
                vec_type: VectorType::v4f32(),
            },
            ["0x40000000", "0x40800000", "0x40c00000", "0x41000000"],
        ),
        (
            "vector_sub",
            MachineInstruction::VectorSub {
                dst: fp(0),
                src: fp(1),
                vec_type: VectorType::v4f32(),
            },
            ["0xbf800000", "0", "0x3f800000", "0x40000000"],
        ),
    ] {
        let mut func = MachineFunction::new(name);
        func.is_exported = true;
        func.entry_block_mut().push(instruction);
        func.entry_block_mut().push(MachineInstruction::Return);
        module.add_function(func);
        oracle.push_str(&format!(
            "adrp x9, lanes\nadd x9, x9, :lo12:lanes\nldr q0, [x9]\nfmov v1.4s, #2.0\nbl {name}\n"
        ));
        for (lane, bits) in expected.iter().enumerate() {
            oracle.push_str(&format!(
                "umov w9, v0.s[{lane}]\nldr w10, ={bits}\ncmp w9, w10\nb.ne fail\n"
            ));
        }
    }
    // Seed every AAPCS64 callee-saved FP register. Allocation must not silently
    // clobber D8..D15 while using caller-saved vectors for these leaf functions.
    oracle.push_str("ldr x20, =0x0123456789abcdef\n");
    for reg in 8..16 {
        oracle.push_str(&format!("fmov d{reg}, x20\n"));
    }
    for name in ["vector_spills", "vector_call_spills"] {
        oracle.push_str(&format!(
            "adrp x9, lanes\nadd x9, x9, :lo12:lanes\nldr q0, [x9]\nbl {name}\n"
        ));
        for (lane, bits) in ["0x42a00000", "0x43200000", "0x43700000", "0x43a00000"]
            .iter()
            .enumerate()
        {
            oracle.push_str(&format!(
                "umov w9, v0.s[{lane}]\nldr w10, ={bits}\ncmp w9, w10\nb.ne fail\n"
            ));
        }
        for reg in 8..16 {
            oracle.push_str(&format!("fmov x9, d{reg}\ncmp x9, x20\nb.ne fail\n"));
        }
    }
    oracle.push_str(
        "fmov d0, #2.0\nbl scalar_spills\numov x9, v0.d[0]\n\
         ldr x10, =0x4054000000000000\ncmp x9, x10\nb.ne fail\n\
         mov x9, sp\ncmp x9, x19\nb.ne fail\n\
         mov x0, #0\nb exit\nfail:\nmov x0, #91\nexit:\nmov x8, #93\nsvc #0\n\
         .global clobber_vectors\nclobber_vectors:\n",
    );
    for reg in (0..8).chain(16..32) {
        oracle.push_str(&format!("movi v{reg}.16b, #0\n"));
    }
    oracle.push_str(
        "ret\n\
         .section .rodata\n.balign 16\npattern:\n\
         .quad 0x0123456789abcdef, 0xfedcba9876543210\n\
         lanes:\n.float 1.0, 2.0, 3.0, 4.0\n",
    );
    let input = dir.path().join("generated.adob");
    let obj = backend.emit_object(&module).expect("stack encodings");
    std::fs::write(&input, AdobWriter::write(&obj).unwrap()).unwrap();
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
    assert!(assembled.status.success(), "assembler: {assembled:?}");
    let output = dir.path().join("stack.elf");
    let mut config = LinkConfig::new(
        output.clone(),
        Target::from_triple("aarch64-unknown-linux-gnu").unwrap(),
    );
    config.static_link = true;
    config.entry_point = Some("_start".to_owned());
    Linker::link(&[oracle_obj, input], config).expect("link generated code");
    let output_linux = tools.path(&output);
    let executed = tools.tool("qemu-aarch64", &[OsStr::new(&output_linux)]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert!(executed.stdout.is_empty(), "{executed:?}");
    assert!(executed.stderr.is_empty(), "{executed:?}");
}
