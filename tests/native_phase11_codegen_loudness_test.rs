//! Phase 11 safety gate: the AArch64 encoder refuses to emit a placeholder.
//!
//! The 2026-10-24 audit found the encoder's `_ =>` arm substituting a `NOP` for
//! every `MachineInstruction` it could not encode, so a hello-world
//! `--emit=adob` shipped hundreds of bytes containing seven NOPs and no error.
//! Silent operand drops inside otherwise-handled arms had the same effect: the
//! instruction vanished and the object still validated.
//!
//! These tests pin the replacement behavior — an unencodable instruction or
//! operand form fails the build with a structured error naming the instruction
//! and the containing function — and pin the relocation-offset fix that the same
//! audit recorded as already correct but untested.
//!
//! The RISC-V half covers the same defect class the AArch64 gate found: placeholder
//! registers substituted for unrecognized operands, `sp` hardcoded as a vector and
//! atomic base, `Parity` collapsed onto `beq`, and widths defaulting to the
//! doubleword form. It also pins the RV32 refusal, which now happens while the
//! backend is being selected rather than after a function body has been encoded
//! with RV64 doubleword instructions.

#![allow(dead_code)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    MoveLocation, MoveOperation, NativeModule, PhysicalRegister,
};
use adesh_codegen::opt::VectorType;
use adesh_codegen::targets::aarch64::AArch64Backend;
use adesh_codegen::targets::create_backend;
use adesh_codegen::targets::riscv::RiscVBackend;
use adesh_object::{RelocationKind, TargetDescriptor};

/// `NOP` (encoding 0xD503201F) — allowed only for `MachineInstruction::Nop`.
const NOP_WORD: u32 = 0xD503_201F;
/// `BL #0`, the placeholder a call relocation is patched onto.
const BL_OPCODE_MASK: u32 = 0xFC00_0000;
const BL_OPCODE: u32 = 0x9400_0000;

fn backend() -> AArch64Backend {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("valid triple");
    AArch64Backend::new(target)
}

/// A general-purpose register operand (`X0`..`X30`).
fn x(n: u8) -> MachineOperand {
    MachineOperand::phys(n)
}

/// A SIMD/FP register operand (`V0`/`D0` are numbered from 32).
fn v(n: u8) -> MachineOperand {
    MachineOperand::phys(32 + n)
}

fn memory(base: u8, offset: i32) -> MachineOperand {
    MachineOperand::Memory {
        base: MachineRegister::Physical(PhysicalRegister(base)),
        offset,
        index: None,
    }
}

fn vec() -> VectorType {
    VectorType::v4f32()
}

/// Encode one instruction alone and return the error the backend produced.
fn error_for(inst: MachineInstruction, func_name: &str) -> String {
    let mut func = MachineFunction::new(func_name);
    func.entry_block_mut().push(inst);
    let error = backend()
        .generate_function(&func)
        .expect_err("an instruction with no AArch64 encoding must fail the build");
    error.to_string()
}

/// Every variant the encoder refuses, paired with the name the error must carry.
fn unencodable_instructions() -> Vec<(MachineInstruction, &'static str)> {
    vec![
        (
            MachineInstruction::Mod {
                dst: x(0),
                src: x(1),
            },
            "Mod",
        ),
        (MachineInstruction::Neg { dst: x(0) }, "Neg"),
        (MachineInstruction::Not { dst: x(0) }, "Not"),
        (
            MachineInstruction::Shl {
                dst: x(0),
                src: x(1),
            },
            "Shl",
        ),
        (
            MachineInstruction::Shr {
                dst: x(0),
                src: x(1),
            },
            "Shr",
        ),
        (
            MachineInstruction::Sar {
                dst: x(0),
                src: x(1),
            },
            "Sar",
        ),
        (
            MachineInstruction::Test {
                lhs: x(0),
                rhs: x(1),
            },
            "Test",
        ),
        (MachineInstruction::Push { src: x(0) }, "Push"),
        (MachineInstruction::Pop { dst: x(0) }, "Pop"),
        (
            MachineInstruction::Custom {
                name: "some_pseudo".to_string(),
                operands: vec![x(0)],
            },
            "Custom",
        ),
        (MachineInstruction::FNeg { dst: v(0), size: 8 }, "FNeg"),
        (
            MachineInstruction::FCmp {
                lhs: v(0),
                rhs: v(1),
                size: 8,
            },
            "FCmp",
        ),
        (
            MachineInstruction::FCvtIntToFloat {
                dst: v(0),
                src: x(0),
                is_f64: true,
                is_signed: true,
            },
            "FCvtIntToFloat",
        ),
        (
            MachineInstruction::FCvtFloatToInt {
                dst: x(0),
                src: v(0),
                is_f64: true,
                is_signed: true,
            },
            "FCvtFloatToInt",
        ),
        (
            MachineInstruction::FCvtFloatToFloat {
                dst: v(0),
                src: v(1),
                to_f64: true,
            },
            "FCvtFloatToFloat",
        ),
        (
            MachineInstruction::VectorDiv {
                dst: v(0),
                src: v(1),
                vec_type: vec(),
            },
            "VectorDiv",
        ),
        (
            MachineInstruction::VectorAnd {
                dst: v(0),
                src: v(1),
                vec_type: vec(),
            },
            "VectorAnd",
        ),
        (
            MachineInstruction::VectorOr {
                dst: v(0),
                src: v(1),
                vec_type: vec(),
            },
            "VectorOr",
        ),
        (
            MachineInstruction::VectorXor {
                dst: v(0),
                src: v(1),
                vec_type: vec(),
            },
            "VectorXor",
        ),
        (
            MachineInstruction::VectorLoad {
                dst: v(0),
                src: memory(5, 0),
                vec_type: vec(),
            },
            "VectorLoad",
        ),
        (
            MachineInstruction::VectorStore {
                dst: memory(5, 0),
                src: v(0),
                vec_type: vec(),
            },
            "VectorStore",
        ),
        (
            MachineInstruction::VectorBroadcast {
                dst: v(0),
                src: x(0),
                vec_type: vec(),
            },
            "VectorBroadcast",
        ),
        (
            MachineInstruction::VectorShuffle {
                dst: v(0),
                src: v(1),
                mask: 0b01001110,
                vec_type: vec(),
            },
            "VectorShuffle",
        ),
        (
            MachineInstruction::VectorReduceAdd {
                dst: x(0),
                src: v(0),
                vec_type: vec(),
            },
            "VectorReduceAdd",
        ),
        (
            MachineInstruction::VectorBlend {
                dst: v(0),
                src: v(1),
                mask: 0xF,
                vec_type: vec(),
            },
            "VectorBlend",
        ),
        (
            MachineInstruction::AtomicLoad {
                dst: x(0),
                src: x(1),
                size: 8,
            },
            "AtomicLoad",
        ),
        (
            MachineInstruction::AtomicStore {
                dst: x(0),
                src: x(1),
                size: 8,
            },
            "AtomicStore",
        ),
        (
            MachineInstruction::TlsAddress {
                dst: x(0),
                symbol: "thread_counter".to_string(),
            },
            "TlsAddress",
        ),
    ]
}

#[test]
fn aarch64_rejects_every_unencodable_instruction() {
    for (inst, name) in unencodable_instructions() {
        let message = error_for(inst, "loud_case");
        assert!(
            message.contains(name),
            "the error for {name} must name the variant, got: {message}"
        );
        assert!(
            message.contains("loud_case"),
            "the error for {name} must name the function, got: {message}"
        );
    }
}

/// Operand forms that used to be dropped without emitting anything, or that
/// used to substitute a placeholder register, or that used to emit a `BL` with
/// no relocation attached to it.
#[test]
fn aarch64_rejects_unencodable_operand_forms() {
    let cases = [
        (
            MachineInstruction::Move {
                dst: x(0),
                src: MachineOperand::Label("block_1".to_string()),
            },
            "no move form",
        ),
        (
            MachineInstruction::Move {
                dst: v(0),
                src: MachineOperand::Immediate(7),
            },
            "no move form",
        ),
        (
            MachineInstruction::Move {
                dst: v(0),
                src: MachineOperand::Symbol("data_item".to_string()),
            },
            "no move form",
        ),
        (
            MachineInstruction::Add {
                dst: x(0),
                src: MachineOperand::Label("block_1".to_string()),
            },
            "no add form",
        ),
        (
            MachineInstruction::Mul {
                dst: x(0),
                src: MachineOperand::Immediate(3),
            },
            "mul needs two general-purpose register operands",
        ),
        (
            MachineInstruction::Compare {
                lhs: MachineOperand::Immediate(3),
                rhs: MachineOperand::Immediate(4),
            },
            "compare needs a register operand",
        ),
        (
            MachineInstruction::SetCc {
                dst: MachineOperand::StackSlot(-8),
                cc: adesh_codegen::machine_ir::ConditionCode::Equal,
            },
            "setcc destination must be a general-purpose register",
        ),
        (
            MachineInstruction::Call {
                target: MachineOperand::Label("block_1".to_string()),
                num_args: 0,
            },
            "call target must be a register or a symbol",
        ),
        (
            MachineInstruction::Load {
                dst: x(0),
                src: memory(5, 16),
                size: 8,
            },
            "load source is not a frame-pointer or stack-pointer relative slot",
        ),
        (
            MachineInstruction::Store {
                dst: memory(5, 16),
                src: x(0),
                size: 8,
            },
            "store destination is not a frame-pointer or stack-pointer relative slot",
        ),
        (
            MachineInstruction::Load {
                dst: v(0),
                src: MachineOperand::StackSlot(-8),
                size: 8,
            },
            "no FP or SIMD stack load is implemented",
        ),
        (
            MachineInstruction::Store {
                dst: MachineOperand::StackSlot(-8),
                src: v(0),
                size: 8,
            },
            "no FP or SIMD stack store is implemented",
        ),
        (
            MachineInstruction::VectorAdd {
                dst: MachineOperand::StackSlot(-24),
                src: MachineOperand::StackSlot(-8),
                vec_type: vec(),
            },
            "both operands must be V registers",
        ),
        (
            MachineInstruction::AtomicFetchAdd {
                dst: MachineOperand::StackSlot(-8),
                src: x(1),
                size: 8,
            },
            "both operands must be X registers",
        ),
    ];

    for (inst, expected) in cases {
        let message = error_for(inst.clone(), "operand_form_case");
        assert!(
            message.contains(expected),
            "expected {expected:?} in the error for {inst:?}, got: {message}"
        );
    }
}

#[test]
fn aarch64_rejects_unallocated_registers_with_an_actionable_error() {
    let mut func = MachineFunction::new("needs_allocation");
    let v0 = func.alloc_vreg();
    func.entry_block_mut().push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(200),
    });

    let error = backend()
        .generate_function(&func)
        .expect_err("a virtual register cannot be encoded");
    let message = error.to_string();
    assert!(
        message.contains("lower_module"),
        "the error must say allocation was skipped, got: {message}"
    );
    assert!(
        message.contains("needs_allocation"),
        "the error must name the function, got: {message}"
    );
}

/// An encodable function body must contain no `NOP` at all: the only way one
/// reaches `.text` is an explicit `MachineInstruction::Nop`.
#[test]
fn aarch64_emitted_text_has_no_unintended_nop() {
    let mut func = MachineFunction::new("hello");
    func.is_exported = true;
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: x(0),
            src: MachineOperand::Immediate(1),
        });
        block.push(MachineInstruction::Move {
            dst: x(19),
            src: MachineOperand::Symbol("greeting".to_string()),
        });
        block.push(MachineInstruction::Add {
            dst: x(0),
            src: x(20),
        });
        block.push(MachineInstruction::Compare {
            lhs: x(0),
            rhs: MachineOperand::Immediate(0),
        });
        block.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("aot_print_cstr".to_string()),
            num_args: 1,
        });
        block.push(MachineInstruction::Return);
    }

    let (code, relocs) = backend()
        .generate_function_with_relocations(&func)
        .expect("this body is fully encodable");
    assert_eq!(count_nops(&code), 0);
    assert!(!relocs.is_empty(), "the symbol forms must be relocated");
}

#[test]
fn aarch64_nop_is_emitted_only_when_asked_for() {
    let mut func = MachineFunction::new("with_padding");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Nop);
        block.push(MachineInstruction::Return);
    }
    let code = backend()
        .generate_function(&func)
        .expect("Nop and Return both encode");
    assert_eq!(count_nops(&code), 1, "exactly the requested NOP is present");
}

/// The audit fixed `emit_object` deriving call offsets as `index * 8`; these
/// assertions pin that each call relocation lands on its own `BL`.
#[test]
fn aarch64_call_relocations_land_on_their_bl_sites() {
    let mut module = NativeModule::new("multi_call");
    let plan = [
        ("first", &["aot_print_cstr", "aot_string_concat"][..]),
        (
            "second",
            &["aot_make_i64", "aot_print_newline", "aot_free"][..],
        ),
    ];
    for (name, callees) in plan {
        let mut func = MachineFunction::new(name);
        func.is_exported = true;
        {
            let block = func.entry_block_mut();
            block.push(MachineInstruction::Move {
                dst: x(0),
                src: MachineOperand::Immediate(1),
            });
            for callee in callees {
                block.push(MachineInstruction::Call {
                    target: MachineOperand::Symbol(callee.to_string()),
                    num_args: 1,
                });
            }
            block.push(MachineInstruction::Return);
        }
        module.add_function(func);
    }

    let obj = backend().emit_object(&module).expect("ADOB emission");
    let text = &obj.sections[0];
    assert_eq!(text.name, ".text");
    let call_relocs: Vec<_> = text
        .relocations
        .iter()
        .filter(|reloc| reloc.kind == RelocationKind::AArch64_Call26)
        .collect();
    assert_eq!(
        call_relocs.len(),
        5,
        "two functions with two and three calls emit five call relocations"
    );

    let mut offsets = Vec::new();
    for reloc in &call_relocs {
        let offset = reloc.offset as usize;
        assert_eq!(
            offset % 4,
            0,
            "a call relocation must be instruction-aligned"
        );
        assert!(offset + 4 <= text.data.len(), "relocation escapes .text");
        let word = u32::from_le_bytes(text.data[offset..offset + 4].try_into().unwrap());
        assert_eq!(
            word & BL_OPCODE_MASK,
            BL_OPCODE,
            "relocation for {} at {offset:#x} must point at a BL, found {word:#010x}",
            reloc.symbol_name
        );
        offsets.push(offset);
    }
    let before = offsets.len();
    offsets.sort_unstable();
    offsets.dedup();
    assert_eq!(
        offsets.len(),
        before,
        "every call relocation needs its own offset; the pre-fix code reused `index * 8`"
    );
}

fn count_nops(code: &[u8]) -> usize {
    code.chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("4-byte chunk")))
        .filter(|word| *word == NOP_WORD)
        .count()
}

// ---------------------------------------------------------------------------
// RISC-V
// ---------------------------------------------------------------------------

/// `NOP` (encoding 0x00000013, `addi x0, x0, 0`) — allowed only for `Nop`.
const RV_NOP_WORD: u32 = 0x0000_0013;
/// `auipc ra, 0`, the first word of the call pair a relocation patches.
const RV_AUIPC_RA: u32 = 0x0000_0097;
/// `jalr ra, ra, 0`, the second word of that pair.
const RV_JALR_RA: u32 = 0x0000_80E7;

fn riscv_backend() -> RiscVBackend {
    let target =
        TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("valid triple");
    RiscVBackend::new(target)
}

/// A general-purpose register operand (`x0`..`x31`).
fn rx(n: u8) -> MachineOperand {
    MachineOperand::phys(n)
}

/// A floating-point or vector register operand (`f0`/`v0` are numbered from 32).
fn rf(n: u8) -> MachineOperand {
    MachineOperand::phys(32 + n)
}

/// A memory operand carrying an index register, which RV64 atomics cannot encode.
fn indexed_memory(base: u8, offset: i32, index: u8) -> MachineOperand {
    MachineOperand::Memory {
        base: MachineRegister::Physical(PhysicalRegister(base)),
        offset,
        index: Some((MachineRegister::Physical(PhysicalRegister(index)), 1)),
    }
}

/// Encode a body alone and return the error the backend produced.
fn riscv_error_for(insts: &[MachineInstruction], func_name: &str) -> String {
    let mut func = MachineFunction::new(func_name);
    {
        let block = func.entry_block_mut();
        for inst in insts {
            block.push(inst.clone());
        }
    }
    let error = riscv_backend()
        .generate_function(&func)
        .expect_err("an instruction with no RISC-V encoding must fail the build");
    error.to_string()
}

/// Every variant with no RISC-V arm, plus the two whose arm refuses outright.
fn riscv_unencodable_instructions() -> Vec<(MachineInstruction, &'static str)> {
    vec![
        (
            MachineInstruction::Test {
                lhs: rx(0),
                rhs: rx(1),
            },
            "Test",
        ),
        (
            MachineInstruction::Custom {
                name: "some_pseudo".to_string(),
                operands: vec![rx(0)],
            },
            "Custom",
        ),
        (
            MachineInstruction::FCmp {
                lhs: rf(0),
                rhs: rf(1),
                size: 8,
            },
            "FCmp",
        ),
        (
            MachineInstruction::FCvtFloatToFloat {
                dst: rf(0),
                src: rf(1),
                to_f64: true,
            },
            "FCvtFloatToFloat",
        ),
        (
            MachineInstruction::VectorDiv {
                dst: rf(0),
                src: rf(1),
                vec_type: vec(),
            },
            "VectorDiv",
        ),
        (
            MachineInstruction::VectorAnd {
                dst: rf(0),
                src: rf(1),
                vec_type: vec(),
            },
            "VectorAnd",
        ),
        (
            MachineInstruction::VectorOr {
                dst: rf(0),
                src: rf(1),
                vec_type: vec(),
            },
            "VectorOr",
        ),
        (
            MachineInstruction::VectorXor {
                dst: rf(0),
                src: rf(1),
                vec_type: vec(),
            },
            "VectorXor",
        ),
        (
            MachineInstruction::VectorCmp {
                dst: rf(0),
                src: rf(1),
                cc: ConditionCode::Equal,
                vec_type: vec(),
            },
            "VectorCmp",
        ),
        (
            MachineInstruction::VectorShiftLeft {
                dst: rf(0),
                src: rf(1),
                count: 2,
                vec_type: vec(),
            },
            "VectorShiftLeft",
        ),
        (
            MachineInstruction::VectorShiftRight {
                dst: rf(0),
                src: rf(1),
                count: 2,
                vec_type: vec(),
            },
            "VectorShiftRight",
        ),
        (
            MachineInstruction::VectorBroadcast {
                dst: rf(0),
                src: rx(0),
                vec_type: vec(),
            },
            "VectorBroadcast",
        ),
        (
            MachineInstruction::VectorShuffle {
                dst: rf(0),
                src: rf(1),
                mask: 0b01001110,
                vec_type: vec(),
            },
            "VectorShuffle",
        ),
        (
            MachineInstruction::VectorBlend {
                dst: rf(0),
                src: rf(1),
                mask: 0xF,
                vec_type: vec(),
            },
            "VectorBlend",
        ),
        (
            MachineInstruction::VectorReduceAdd {
                dst: rx(0),
                src: rf(0),
                vec_type: vec(),
            },
            "VectorReduceAdd",
        ),
        (
            MachineInstruction::VectorLoad {
                dst: rf(0),
                src: rx(10),
                vec_type: vec(),
            },
            "VectorLoad",
        ),
        (
            MachineInstruction::VectorStore {
                dst: rx(10),
                src: rf(0),
                vec_type: vec(),
            },
            "VectorStore",
        ),
        (
            MachineInstruction::AtomicLoad {
                dst: rx(0),
                src: rx(1),
                size: 8,
            },
            "AtomicLoad",
        ),
        (
            MachineInstruction::AtomicStore {
                dst: rx(0),
                src: rx(1),
                size: 8,
            },
            "AtomicStore",
        ),
        (
            MachineInstruction::TlsAddress {
                dst: rx(0),
                symbol: "thread_counter".to_string(),
            },
            "TlsAddress",
        ),
    ]
}

#[test]
fn riscv64_rejects_every_unencodable_instruction() {
    for (inst, name) in riscv_unencodable_instructions() {
        let message = riscv_error_for(&[inst], "riscv_loud_case");
        assert!(
            message.contains(name),
            "the error for {name} must name the variant, got: {message}"
        );
        assert!(
            message.contains("riscv_loud_case"),
            "the error for {name} must name the function, got: {message}"
        );
    }
}

/// Operand forms that used to substitute a placeholder register, default a width
/// to the doubleword form, or collapse a condition onto `beq`.
#[test]
fn riscv64_rejects_unencodable_operand_forms() {
    let cases = [
        (
            vec![MachineInstruction::Move {
                dst: rx(5),
                src: MachineOperand::FloatImmediate(1.5),
            }],
            "no `fmv` form takes a float immediate",
        ),
        (
            vec![MachineInstruction::Move {
                dst: rf(0),
                src: MachineOperand::Immediate(7),
            }],
            "no move form covers a register and Immediate(7)",
        ),
        (
            vec![MachineInstruction::Add {
                dst: rf(0),
                src: rx(1),
            }],
            "no add form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Sub {
                dst: rx(0),
                src: MachineOperand::Label("block_1".to_string()),
            }],
            "no sub form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Mul {
                dst: rx(0),
                src: MachineOperand::Immediate(3),
            }],
            "mul needs two general-purpose register operands",
        ),
        (
            vec![MachineInstruction::Div {
                dst: rx(0),
                src: MachineOperand::Immediate(3),
            }],
            "div needs two general-purpose register operands",
        ),
        (
            vec![MachineInstruction::Mod {
                dst: rx(0),
                src: MachineOperand::Immediate(3),
            }],
            "rem needs two general-purpose register operands",
        ),
        (
            vec![MachineInstruction::Neg { dst: rf(0) }],
            "neg destination must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::Not { dst: rf(0) }],
            "not destination must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::And {
                dst: rx(0),
                src: MachineOperand::Symbol("flags".to_string()),
            }],
            "no and form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Or {
                dst: rx(0),
                src: MachineOperand::Symbol("flags".to_string()),
            }],
            "no or form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Xor {
                dst: rx(0),
                src: indexed_memory(10, 0, 11),
            }],
            "no xor form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Shl {
                dst: rx(0),
                src: MachineOperand::Immediate(70),
            }],
            "slli shift amount 70 does not fit the 6-bit shamt field",
        ),
        (
            vec![MachineInstruction::Shr {
                dst: rf(0),
                src: MachineOperand::Immediate(2),
            }],
            "no logical shift-right form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Sar {
                dst: rx(0),
                src: rf(6),
            }],
            "no arithmetic shift-right form covers this destination and source",
        ),
        (
            vec![MachineInstruction::Compare {
                lhs: MachineOperand::Immediate(3),
                rhs: MachineOperand::Immediate(4),
            }],
            "compare needs a general-purpose register operand",
        ),
        (
            vec![MachineInstruction::SetCc {
                dst: MachineOperand::StackSlot(-8),
                cc: ConditionCode::Equal,
            }],
            "setcc destination must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::SetCc {
                dst: rx(5),
                cc: ConditionCode::Equal,
            }],
            "no compare produced the flags this setcc reads",
        ),
        (
            vec![
                MachineInstruction::Compare {
                    lhs: rx(10),
                    rhs: rx(11),
                },
                MachineInstruction::SetCc {
                    dst: rx(5),
                    cc: ConditionCode::Parity,
                },
            ],
            "has no parity condition to encode",
        ),
        (
            vec![MachineInstruction::BranchCc {
                cc: ConditionCode::Equal,
                target: "exit_block".to_string(),
            }],
            "no compare produced the flags this branch reads",
        ),
        (
            vec![
                MachineInstruction::Compare {
                    lhs: rx(10),
                    rhs: rx(11),
                },
                MachineInstruction::BranchCc {
                    cc: ConditionCode::NotParity,
                    target: "exit_block".to_string(),
                },
            ],
            "the RV64 branch forms have no parity condition to encode",
        ),
        (
            vec![MachineInstruction::Branch {
                target: "nowhere_to_go".to_string(),
            }],
            "branch targets label 'nowhere_to_go', which this function does not define",
        ),
        (
            vec![MachineInstruction::Call {
                target: MachineOperand::Label("block_1".to_string()),
                num_args: 0,
            }],
            "call target must be a general-purpose register or a symbol",
        ),
        (
            vec![MachineInstruction::Call {
                target: MachineOperand::StackSlot(-8),
                num_args: 0,
            }],
            "call target must be a general-purpose register or a symbol",
        ),
        (
            vec![MachineInstruction::Load {
                dst: rx(0),
                src: indexed_memory(10, 16, 11),
                size: 8,
            }],
            "indexed register addressing is not implemented; a load needs a register destination",
        ),
        (
            vec![MachineInstruction::Load {
                dst: MachineOperand::Immediate(3),
                src: MachineOperand::StackSlot(-8),
                size: 8,
            }],
            "a load needs a physical register destination and a frame-relative stack slot source",
        ),
        (
            vec![MachineInstruction::Store {
                dst: indexed_memory(10, 16, 11),
                src: rx(0),
                size: 8,
            }],
            "indexed register addressing is not implemented; a store needs a frame-relative stack slot destination",
        ),
        (
            vec![MachineInstruction::Store {
                dst: MachineOperand::Symbol("table".to_string()),
                src: rx(0),
                size: 8,
            }],
            "a store needs a frame-relative stack slot destination and a physical register source",
        ),
        (
            vec![MachineInstruction::Load {
                dst: rx(0),
                src: MachineOperand::StackSlot(-8),
                size: 3,
            }],
            "access size 3 has no RV64 load or store encoding",
        ),
        (
            vec![MachineInstruction::Store {
                dst: MachineOperand::StackSlot(-8),
                src: rx(0),
                size: 16,
            }],
            "access size 16 has no RV64 load or store encoding",
        ),
        (
            vec![MachineInstruction::Load {
                dst: rf(0),
                src: MachineOperand::StackSlot(-8),
                size: 2,
            }],
            "float access size 2 has no RV64 encoding",
        ),
        (
            vec![MachineInstruction::Push { src: rf(0) }],
            "push source must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::Pop { dst: rf(0) }],
            "pop destination must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::FAdd {
                dst: rx(0),
                src: rx(1),
                size: 8,
            }],
            "both operands must be F registers",
        ),
        (
            vec![MachineInstruction::FNeg {
                dst: rx(0),
                size: 8,
            }],
            "fneg operand must be an F register",
        ),
        (
            vec![MachineInstruction::FCvtIntToFloat {
                dst: rx(0),
                src: rx(1),
                is_f64: true,
                is_signed: true,
            }],
            "the destination must be an F register and the source a general-purpose register",
        ),
        (
            vec![MachineInstruction::FCvtFloatToInt {
                dst: rf(0),
                src: rf(1),
                is_f64: true,
                is_signed: true,
            }],
            "the destination must be a general-purpose register and the source an F register",
        ),
        (
            vec![MachineInstruction::VectorAdd {
                dst: rx(0),
                src: rx(1),
                vec_type: vec(),
            }],
            "both operands must be V registers",
        ),
        (
            vec![MachineInstruction::AtomicFetchAdd {
                dst: rx(10),
                src: MachineOperand::StackSlot(-8),
                size: 8,
            }],
            "the atomic addend must be a general-purpose register",
        ),
        (
            vec![MachineInstruction::AtomicFetchAdd {
                dst: MachineOperand::Immediate(4),
                src: rx(11),
                size: 8,
            }],
            "an atomic destination must be a register, a memory operand or a stack slot, not Immediate(4)",
        ),
        (
            vec![MachineInstruction::AtomicFetchAdd {
                dst: rx(10),
                src: rx(11),
                size: 1,
            }],
            "atomic access size 1 has no RV64A encoding",
        ),
        (
            vec![MachineInstruction::AtomicExchange {
                dst: rf(0),
                src: rx(1),
                size: 8,
            }],
            "the destination must name the general-purpose register holding the address",
        ),
        (
            vec![MachineInstruction::AtomicCompareExchange {
                dst: indexed_memory(10, 8, 11),
                expected: rx(11),
                desired: rx(12),
                size: 8,
            }],
            "an indexed atomic address has no register-only AMO encoding",
        ),
    ];

    for (insts, expected) in cases {
        let message = riscv_error_for(&insts, "riscv_operand_form_case");
        assert!(
            message.contains(expected),
            "expected {expected:?} in the error for {insts:?}, got: {message}"
        );
        assert!(
            message.contains("riscv_operand_form_case"),
            "the error for {insts:?} must name the function, got: {message}"
        );
    }
}

#[test]
fn riscv64_rejects_unallocated_registers_with_an_actionable_error() {
    let mut func = MachineFunction::new("riscv_needs_allocation");
    let v0 = func.alloc_vreg();
    func.entry_block_mut().push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(200),
    });

    let error = riscv_backend()
        .generate_function(&func)
        .expect_err("a virtual register cannot be encoded");
    let message = error.to_string();
    assert!(
        message.contains("lower_module"),
        "the error must say allocation was skipped, got: {message}"
    );
    assert!(
        message.contains("riscv_needs_allocation"),
        "the error must name the function, got: {message}"
    );
}

/// A conditional branch is a 12-bit immediate, so a far label cannot be patched
/// by masking the displacement.
#[test]
fn riscv64_rejects_a_conditional_branch_that_does_not_fit_the_b_type_immediate() {
    let mut func = MachineFunction::new("far_conditional_branch");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Compare {
            lhs: rx(10),
            rhs: rx(11),
        });
        block.push(MachineInstruction::BranchCc {
            cc: ConditionCode::Equal,
            target: "far_block".to_string(),
        });
        // `far_block` is laid out after these, so the displacement to it is the
        // length of this tail — well past what a B-type immediate can hold.
        for _ in 0..1100 {
            block.push(MachineInstruction::Nop);
        }
    }
    let mut far = adesh_codegen::machine_ir::MachineBlock::new(1, "far_block");
    far.push(MachineInstruction::Return);
    func.blocks.push(far);

    let error = riscv_backend()
        .generate_function(&func)
        .expect_err("a 4400-byte conditional branch has no B-type encoding");
    let message = error.to_string();
    assert!(
        message.contains("does not fit the B-type immediate"),
        "the error must name the field that overflowed, got: {message}"
    );
    assert!(
        message.contains("far_block"),
        "the error must name the label, got: {message}"
    );
}

/// An encodable body must contain no `NOP` at all: the only way one reaches
/// `.text` is an explicit `MachineInstruction::Nop`.
#[test]
fn riscv64_emitted_text_has_no_unintended_nop() {
    let mut func = MachineFunction::new("riscv_hello");
    func.is_exported = true;
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: rx(10),
            src: MachineOperand::Immediate(1),
        });
        block.push(MachineInstruction::Move {
            dst: rx(11),
            src: rx(10),
        });
        block.push(MachineInstruction::Add {
            dst: rx(10),
            src: rx(12),
        });
        block.push(MachineInstruction::Shl {
            dst: rx(10),
            src: MachineOperand::Immediate(3),
        });
        block.push(MachineInstruction::FAdd {
            dst: rf(0),
            src: rf(1),
            size: 8,
        });
        block.push(MachineInstruction::Store {
            dst: MachineOperand::StackSlot(-16),
            src: rx(10),
            size: 8,
        });
        block.push(MachineInstruction::Load {
            dst: rx(13),
            src: MachineOperand::StackSlot(-16),
            size: 8,
        });
        block.push(MachineInstruction::AtomicFetchAdd {
            dst: rx(14),
            src: rx(15),
            size: 8,
        });
        block.push(MachineInstruction::Compare {
            lhs: rx(13),
            rhs: MachineOperand::Immediate(0),
        });
        block.push(MachineInstruction::SetCc {
            dst: rx(16),
            cc: ConditionCode::LessThan,
        });
        block.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("aot_print_cstr".to_string()),
            num_args: 1,
        });
        block.push(MachineInstruction::Return);
    }

    let (code, relocs) = riscv_backend()
        .generate_function_with_relocs(&func)
        .expect("this body is fully encodable");
    assert_eq!(
        count_rv_nops(&code),
        0,
        "a body without an explicit Nop must not gain one"
    );
    assert!(!relocs.is_empty(), "the symbol form must be relocated");
}

#[test]
fn riscv64_nop_is_emitted_only_when_asked_for() {
    let mut func = MachineFunction::new("riscv_with_padding");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Nop);
        block.push(MachineInstruction::Return);
    }
    let code = riscv_backend()
        .generate_function(&func)
        .expect("Nop and Return both encode");
    assert_eq!(
        count_rv_nops(&code),
        1,
        "exactly the requested NOP is present"
    );
}

/// The atomic address must be the register the operand names. Before the gate, an
/// unrecognized destination fell back to `sp`, so the AMO updated the frame.
#[test]
fn riscv64_atomic_uses_the_operand_address_register_not_the_stack_pointer() {
    let mut func = MachineFunction::new("riscv_atomic_addressing");
    func.is_exported = true;
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::AtomicFetchAdd {
            dst: memory(10, 0),
            src: rx(11),
            size: 8,
        });
        block.push(MachineInstruction::Return);
    }

    let code = riscv_backend()
        .generate_function(&func)
        .expect("amoadd.d with a register-only address encodes");
    // amoadd.d a5(11), a1(11), (a0/10) — rd == rs2 == the addend, rs1 == the address.
    let expected = 0x0000002Fu32 | (11 << 20) | (10 << 15) | (0x3 << 12) | (11 << 7);
    let words: Vec<u32> = code
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("4-byte chunk")))
        .collect();
    assert!(
        words.contains(&expected),
        "the AMO must address x10, got {:x?}",
        words
    );
    let sp_addressed = 0x0000002Fu32 | (11 << 20) | (2 << 15) | (0x3 << 12) | (10 << 7);
    assert!(
        !words.contains(&sp_addressed),
        "the pre-gate placeholder addressed the frame pointer"
    );
}

/// Each call relocation must land on its own `AUIPC`, the first word of the pair
/// the linker patches.
#[test]
fn riscv64_call_relocations_land_on_their_auipc_sites() {
    let mut module = NativeModule::new("riscv_multi_call");
    let plan = [
        ("first", &["aot_print_cstr", "aot_string_concat"][..]),
        (
            "second",
            &["aot_make_i64", "aot_print_newline", "aot_free"][..],
        ),
    ];
    for (name, callees) in plan {
        let mut func = MachineFunction::new(name);
        func.is_exported = true;
        {
            let block = func.entry_block_mut();
            block.push(MachineInstruction::Move {
                dst: rx(10),
                src: MachineOperand::Immediate(1),
            });
            for callee in callees {
                block.push(MachineInstruction::Call {
                    target: MachineOperand::Symbol(callee.to_string()),
                    num_args: 1,
                });
            }
            block.push(MachineInstruction::Return);
        }
        module.add_function(func);
    }

    let obj = riscv_backend().emit_object(&module).expect("ADOB emission");
    let text = &obj.sections[0];
    assert_eq!(text.name, ".text");
    let call_relocs: Vec<_> = text
        .relocations
        .iter()
        .filter(|reloc| reloc.kind == RelocationKind::RiscV_Call)
        .collect();
    assert_eq!(
        call_relocs.len(),
        5,
        "two functions with two and three calls emit five call relocations"
    );

    let mut offsets = Vec::new();
    for reloc in &call_relocs {
        let offset = reloc.offset as usize;
        assert_eq!(offset % 4, 0, "a call relocation must be word-aligned");
        assert!(offset + 8 <= text.data.len(), "relocation escapes .text");
        let auipc = u32::from_le_bytes(text.data[offset..offset + 4].try_into().unwrap());
        let jalr = u32::from_le_bytes(text.data[offset + 4..offset + 8].try_into().unwrap());
        assert_eq!(
            auipc, RV_AUIPC_RA,
            "relocation for {} at {offset:#x} must point at its AUIPC, found {auipc:#010x}",
            reloc.symbol_name
        );
        assert_eq!(
            jalr, RV_JALR_RA,
            "the word after a call relocation must be the matching JALR"
        );
        offsets.push(offset);
    }
    let before = offsets.len();
    offsets.sort_unstable();
    offsets.dedup();
    assert_eq!(
        offsets.len(),
        before,
        "every call relocation needs its own offset"
    );
}

/// Phase 12: a symbol address reaches a RISC-V register through `AUIPC` + `ADDI`,
/// the pair the linker patches with `RiscV_PcrelHi20` / `RiscV_PcrelLo12I`.
/// Before this, `Move { dst: Reg, src: Symbol }` was refused, so
/// `adesh build --target riscv64gc-unknown-linux-gnu` died in codegen on the first
/// string literal and never reached the archive question.
#[test]
fn riscv64_materializes_a_symbol_address_with_auipc_and_addi() {
    let mut func = MachineFunction::new("riscv_symbol_address");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: rx(5),
            src: MachineOperand::Symbol("__str_0".to_string()),
        });
        block.push(MachineInstruction::Return);
    }
    let (code, relocs) = riscv_backend()
        .generate_function_with_relocs(&func)
        .expect("a symbol address is encodable as AUIPC + ADDI");

    let hi = relocs
        .iter()
        .find(|r| r.kind == RelocationKind::RiscV_PcrelHi20)
        .expect("the AUIPC needs a PCREL_HI20 relocation");
    let lo = relocs
        .iter()
        .find(|r| r.kind == RelocationKind::RiscV_PcrelLo12I)
        .expect("the ADDI needs a PCREL_LO12_I relocation");
    assert_eq!(hi.symbol_name, "__str_0");
    assert_eq!(lo.symbol_name, "__str_0");
    assert_eq!(hi.addend, 0);
    assert_eq!(
        lo.addend, 4,
        "the LO12 half is relative to the AUIPC, so the 4-byte gap between the two \
         words has to be added back"
    );
    assert_eq!(
        lo.offset,
        hi.offset + 4,
        "that addend is only right while the pair stays adjacent"
    );

    let at = |off: u64| u32::from_le_bytes(code[off as usize..off as usize + 4].try_into().unwrap());
    assert_eq!(
        at(hi.offset),
        0x0000_0297,
        "auipc t0, 0 — the immediate is left for the linker"
    );
    assert_eq!(
        at(lo.offset),
        0x0002_8293,
        "addi t0, t0, 0 — the immediate is left for the linker"
    );
    assert_eq!(count_rv_nops(&code), 0);
}

/// A parallel move is expanded before encoding, so it reaches `.text` as real
/// moves rather than being refused (or, before the Phase 11 gate, dropped).
#[test]
fn riscv64_parallel_move_expands_to_single_moves() {
    let mut func = MachineFunction::new("riscv_parallel_move");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::ParallelMove {
            moves: vec![MoveOperation::new_qword(
                MoveLocation::PhysicalRegister(PhysicalRegister(10)),
                MoveLocation::PhysicalRegister(PhysicalRegister(11)),
            )],
        });
        block.push(MachineInstruction::Return);
    }
    let (code, _) = riscv_backend()
        .generate_function_with_relocs(&func)
        .expect("a register-to-register parallel move expands to MV");
    assert_eq!(
        mv_pairs(&code),
        vec![(10, 11)],
        "a0 <- a1 must be emitted as `addi a0, a1, 0`"
    );
    assert_eq!(count_rv_nops(&code), 0);
}

/// A cycle has no ordering that preserves both values, so the resolver saves one
/// side into a caller-saved temporary first. The temporary must be an ABI
/// temporary (t0/t1/t2), never a reserved or callee-saved register.
#[test]
fn riscv64_parallel_move_breaks_a_cycle_with_a_temporary() {
    let mut func = MachineFunction::new("riscv_parallel_swap");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::ParallelMove {
            moves: vec![
                MoveOperation::new_qword(
                    MoveLocation::PhysicalRegister(PhysicalRegister(10)),
                    MoveLocation::PhysicalRegister(PhysicalRegister(11)),
                ),
                MoveOperation::new_qword(
                    MoveLocation::PhysicalRegister(PhysicalRegister(11)),
                    MoveLocation::PhysicalRegister(PhysicalRegister(10)),
                ),
            ],
        });
        block.push(MachineInstruction::Return);
    }
    let (code, _) = riscv_backend()
        .generate_function_with_relocs(&func)
        .expect("a two-register cycle is broken with a temporary");
    assert_eq!(
        mv_pairs(&code),
        vec![(5, 11), (11, 10), (10, 5)],
        "t0 = a1; a1 = a0; a0 = t0 — the save into t0 must come first, and t0 (x5) \
         is the ABI temporary the resolver is allowed to clobber"
    );
}

/// Expansion is not a licence to encode anything: a location form the single-move
/// encoder cannot express still fails the build.
#[test]
fn riscv64_parallel_move_with_an_indexed_memory_source_is_refused() {
    let mut func = MachineFunction::new("riscv_parallel_indexed");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::ParallelMove {
            moves: vec![MoveOperation::new_qword(
                MoveLocation::PhysicalRegister(PhysicalRegister(10)),
                MoveLocation::Memory {
                    base: MachineRegister::Physical(PhysicalRegister(11)),
                    offset: 0,
                    index: Some((MachineRegister::Physical(PhysicalRegister(12)), 3)),
                },
            )],
        });
        block.push(MachineInstruction::Return);
    }
    let error = riscv_backend()
        .generate_function_with_relocs(&func)
        .expect_err("RV64 has no indexed addressing form for a register load");
    let message = error.to_string();
    assert!(
        message.contains("riscv_parallel_indexed"),
        "the refusal must name the function, got: {message}"
    );
}

/// The encoder names `__str_<n>`, so the object has to define them. Without this
/// the first real link fails `LNK001: undefined reference to __str_0` — the same
/// gap AArch64 had until 2026-11-04.
#[test]
fn riscv64_emit_object_defines_the_string_pool() {
    let mut module = NativeModule::new("riscv_pool");
    let first = module.add_string("hello");
    module.add_string("world");
    let repeated = module.add_string("hello");
    assert_eq!(repeated, first, "the pool deduplicates by value");

    let mut func = MachineFunction::new("main");
    {
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: rx(5),
            src: MachineOperand::Symbol(format!("__str_{first}")),
        });
        block.push(MachineInstruction::Return);
    }
    module.add_function(func);

    let obj = riscv_backend()
        .emit_object(&module)
        .expect("a symbol move and a return encode");
    let rodata = obj
        .sections
        .iter()
        .find(|s| s.name == ".rodata")
        .expect("the string pool becomes a .rodata section");
    assert_eq!(rodata.data, b"hello\0world\0".to_vec());

    let mut defined: Vec<(String, u64)> = obj
        .symbols
        .iter()
        .filter(|s| s.name.starts_with("__str_"))
        .map(|s| (s.name.clone(), s.value))
        .collect();
    defined.sort();
    assert_eq!(
        defined,
        vec![("__str_0".to_string(), 0), ("__str_1".to_string(), 6)],
        "every index the encoder can name is a defined offset into .rodata"
    );

    let text = obj.sections.iter().find(|s| s.name == ".text").expect(".text");
    assert!(
        text.relocations
            .iter()
            .any(|r| r.symbol_name == "__str_0" && r.kind == RelocationKind::RiscV_PcrelHi20),
        "the symbol move must leave a relocation naming the pool entry it reads"
    );
}

/// The `MV rd, rs` pairs (`addi rd, rs, 0`) in emission order.
fn mv_pairs(code: &[u8]) -> Vec<(u8, u8)> {
    code.chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("4-byte chunk")))
        .filter(|word| {
            word & 0x7F == 0x13 && (word >> 12) & 7 == 0 && word >> 20 == 0
        })
        .map(|word| (((word >> 7) & 0x1F) as u8, ((word >> 15) & 0x1F) as u8))
        .collect()
}

fn count_rv_nops(code: &[u8]) -> usize {
    code.chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("4-byte chunk")))
        .filter(|word| *word == RV_NOP_WORD)
        .count()
}

/// Phase 11 item 4: an RV32 triple is refused while the backend is being
/// selected, not after a function body has been encoded with RV64 doubleword
/// instructions. Before this the RV32 target reached the RISC-V backend and
/// either died on `ParallelMove` or, for a body that avoided it, was emitted as
/// RV64 code the hardware cannot run.
#[test]
fn riscv32_is_refused_at_backend_selection() {
    for triple in ["riscv32imac-unknown-none-elf", "riscv32-unknown-linux-gnu"] {
        let target = TargetDescriptor::from_triple(triple).expect("valid triple");
        // The descriptor canonicalizes the triple, so the reported target string
        // is not the text that was asked for.
        let canonical = target.triple_string();
        let error = match create_backend(target) {
            Err(error) => error,
            Ok(_) => panic!("{triple} must be refused while the backend is selected"),
        };
        let message = error.to_string();
        assert!(
            message.contains(canonical.as_str()),
            "{triple}: the refusal must name the requested target ({canonical}): {message}"
        );
        assert!(
            message.contains("RV64"),
            "{triple}: the refusal must name the width actually implemented: {message}"
        );
        assert!(
            message.contains("riscv64gc"),
            "{triple}: the refusal must suggest a triple that works: {message}"
        );
        // A target-level refusal has no function to blame; if one were reported,
        // the error came out of the encoder instead of target validation.
        assert!(
            !message.contains("Function:"),
            "{triple}: RV32 must fail before any function is encoded: {message}"
        );
    }

    // RV64 keeps its backend; only its unencodable forms are loud.
    let rv64 = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("valid triple");
    assert!(
        create_backend(rv64).is_ok(),
        "riscv64gc must still select the RISC-V backend"
    );
}
