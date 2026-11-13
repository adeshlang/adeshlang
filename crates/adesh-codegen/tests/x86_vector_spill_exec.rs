//! Native x86 encoder -> ADOB -> native ELF linker -> QEMU, GNU oracle.
#[path = "../../../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use adesh_codegen::{
    CodegenBackend,
    machine_ir::{
        MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
        PhysicalRegister,
    },
    opt::VectorType,
    targets::x86_64::X86_64Backend,
};
use adesh_linker::{config::LinkConfig, linker::Linker, target::Target};
use adesh_object::{AdobWriter, TargetDescriptor};
use foreign_exec::LinuxTools;
use std::ffi::OsStr;

#[test]
fn vector_pressure_preserves_every_lane() {
    let Some(tools) = LinuxTools::discover("as", "qemu-x86_64") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut backend =
        X86_64Backend::new(TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap());
    let mut func = MachineFunction::new("vector_spills");
    func.is_exported = true;
    let regs: Vec<_> = (0..40).map(|_| func.alloc_fp_vreg()).collect();
    let op = |v| MachineOperand::Register(MachineRegister::Virtual(v));
    let xmm0 = MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::xmm(0)));
    let block = func.entry_block_mut();
    for &reg in &regs {
        block.push(MachineInstruction::Move {
            dst: op(reg),
            src: xmm0.clone(),
        });
    }
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
        dst: xmm0,
        src: op(regs[0]),
    });
    block.push(MachineInstruction::Return);
    let mut module = NativeModule::new("x86_vector");
    module.add_function(func);
    let lowered = backend.lower_module(&module).unwrap();
    for load in [true, false] {
        assert!(
            lowered.functions[0]
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| if load {
                    matches!(i, MachineInstruction::Load { size: 16, .. })
                } else {
                    matches!(i, MachineInstruction::Store { size: 16, .. })
                }),
            "fixture must exercise vector reload and writeback"
        );
    }
    let obj = backend.emit_object(&lowered).unwrap();
    let input = dir.path().join("generated.adob");
    std::fs::write(&input, AdobWriter::write(&obj).unwrap()).unwrap();
    let assembly = dir.path().join("oracle.s");
    std::fs::write(
        &assembly,
        ".text\n.global _start\n_start:\n\
        movups lanes(%rip), %xmm0\ncall vector_spills\n\
        cmpeqps expected(%rip), %xmm0\nmovmskps %xmm0, %edi\n\
        xor $15, %edi\nmov $60, %eax\nsyscall\n\
        .section .rodata\n.balign 16\nlanes:\n.float 1.0, 2.0, 3.0, 4.0\n\
        expected:\n.float 80.0, 160.0, 240.0, 320.0\n",
    )
    .unwrap();
    let oracle = dir.path().join("oracle.o");
    let oracle_linux = format!("{}/oracle.o", tools.path(dir.path()));
    let assembly_linux = tools.path(&assembly);
    let assembled = tools.tool(
        "as",
        &[
            OsStr::new("--64"),
            OsStr::new("-o"),
            OsStr::new(&oracle_linux),
            OsStr::new(&assembly_linux),
        ],
    );
    assert!(assembled.status.success(), "{assembled:?}");
    let output = dir.path().join("vector.elf");
    let mut config = LinkConfig::new(
        output.clone(),
        Target::from_triple("x86_64-unknown-linux-gnu").unwrap(),
    );
    config.static_link = true;
    config.entry_point = Some("_start".to_owned());
    Linker::link(&[oracle, input], config).unwrap();
    let output_linux = tools.path(&output);
    let executed = tools.tool("qemu-x86_64", &[OsStr::new(&output_linux)]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert!(
        executed.stdout.is_empty() && executed.stderr.is_empty(),
        "{executed:?}"
    );
}
