//! Precompiled native standard library ADOB generator (`libadesh_std.adob`).
//!
//! Replaces the historical linkage-stub generator with real, target-ABI-accurate
//! native Machine IR implementations for the Phase 19.7 functional standard
//! library subset:
//! - `std::io` (`adesh_std_io_*`)
//! - `std::string` (`adesh_std_str_*`)
//! - `std::collections` (`adesh_std_col_*`)
//! - `std::math` (`adesh_std_math_*` — pure Machine IR integer ops + IEEE-754 f64)
//! - `std::time` (`adesh_std_time_*`)
//! - `std::fs` (`adesh_std_fs_*`)
//! - `std::path` (`adesh_std_path_*`)
//!
//! Every emitted object embeds an `.adesh.std_abi` metadata section recording
//! `ADESH_STD_ABI_V1`, target architecture/OS/ABI, sorted exported symbol list,
//! and a deterministic 64-bit FNV-1a digest over `.text` and symbols so
//! cross-target mismatches and corrupt archives are rejected loudly at link time.

use std::collections::BTreeSet;

use crate::calling_convention::{
    Aapcs64CallingConvention, CallingConvention, RiscVCallingConvention,
    SystemVX64CallingConvention, WindowsX64CallingConvention,
};
use crate::error::CodegenError;
use crate::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister,
};
use crate::opt::verifier::MachineIRVerifier;
use crate::targets::create_backend;
use adesh_object::bundle::AdobBundle;
use adesh_object::validator::AdobValidator;
use adesh_object::{
    section_flags, AdobObject, AdobSection, Architecture, Endianness, OperatingSystem,
    RuntimeProfile, SectionKind, TargetDescriptor,
};

pub const STDLIB_ABI_VERSION: u32 = 1;
pub const STDLIB_ABI_TAG: &[u8; 16] = b"ADESH_STD_ABI_V1";
pub const STDLIB_METADATA_SECTION: &str = ".adesh.std_abi";

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn preg(r: PhysicalRegister) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(r))
}

fn target_tag(target: &TargetDescriptor) -> String {
    format!("{:?}/{:?}", target.architecture, target.operating_system)
}

fn calling_convention_for(target: &TargetDescriptor) -> Result<Box<dyn CallingConvention>, CodegenError> {
    if target.arch_descriptor().runtime_profile != RuntimeProfile::FullNative {
        return Err(CodegenError::new(
            target_tag(target),
            format!(
                "Target `{:?}/{:?}` does not support native precompiled stdlib ADOB archives (requires FullNative 64-bit ABI)",
                target.architecture, target.operating_system
            ),
        ));
    }
    match &target.architecture {
        Architecture::X86_64 => {
            if target.operating_system == OperatingSystem::Windows {
                Ok(Box::new(WindowsX64CallingConvention))
            } else {
                Ok(Box::new(SystemVX64CallingConvention))
            }
        }
        Architecture::AArch64 => Ok(Box::new(Aapcs64CallingConvention)),
        Architecture::RiscV64 => Ok(Box::new(RiscVCallingConvention)),
        other => Err(CodegenError::new(
            target_tag(target),
            format!(
                "Target architecture `{:?}` is not supported by native stdlib ADOB codegen",
                other
            ),
        )),
    }
}

#[derive(Debug, Clone, Copy)]
enum CallArgSpec {
    ForwardArg(usize),
    Imm(i64),
}

#[derive(Debug, Clone, Copy)]
enum StdlibBody {
    /// Returns constant `STDLIB_ABI_VERSION` (`1`) purely in Machine IR.
    AbiVersion,
    /// Pure Machine IR signed 64-bit `abs(x)`.
    MathAbsI64,
    /// Pure Machine IR signed 64-bit `min(a, b)`.
    MathMinI64,
    /// Pure Machine IR signed 64-bit `max(a, b)`.
    MathMaxI64,
    /// Pure Machine IR signed 64-bit `clamp(x, lo, hi)`.
    MathClampI64,
    /// Calls a `adesh-runtime` C-ABI primitive forwarding/augmenting GPR arguments.
    RuntimeCall {
        callee: &'static str,
        args: &'static [CallArgSpec],
    },
    /// Calls `aot_math_f64(op, x, y)` respecting target-specific FP vs positional slot rules.
    MathF64 { op: i64, binary: bool },
}

struct StdlibFnSpec {
    name: &'static str,
    body: StdlibBody,
}

const STDLIB_SPECS: &[StdlibFnSpec] = &[
    // ABI version query
    StdlibFnSpec {
        name: "adesh_std_abi_version",
        body: StdlibBody::AbiVersion,
    },
    // std::io
    StdlibFnSpec {
        name: "adesh_std_io_print",
        body: StdlibBody::RuntimeCall {
            callee: "aot_print_string",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::Imm(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_println",
        body: StdlibBody::RuntimeCall {
            callee: "aot_print_string",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::Imm(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_eprint",
        body: StdlibBody::RuntimeCall {
            callee: "aot_eprint_string",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::Imm(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_eprintln",
        body: StdlibBody::RuntimeCall {
            callee: "aot_eprint_string",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::Imm(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_flush",
        body: StdlibBody::RuntimeCall {
            callee: "aot_io_flush",
            args: &[CallArgSpec::Imm(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_flush_err",
        body: StdlibBody::RuntimeCall {
            callee: "aot_io_flush",
            args: &[CallArgSpec::Imm(2)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_io_read_line",
        body: StdlibBody::RuntimeCall {
            callee: "io_stdin_readline",
            args: &[],
        },
    },
    // std::string
    StdlibFnSpec {
        name: "adesh_std_str_from_cstr",
        body: StdlibBody::RuntimeCall {
            callee: "aot_make_string",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_concat",
        body: StdlibBody::RuntimeCall {
            callee: "aot_string_concat",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_len",
        body: StdlibBody::RuntimeCall {
            callee: "aot_str_len",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_eq",
        body: StdlibBody::RuntimeCall {
            callee: "aot_value_eq",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_compare",
        body: StdlibBody::RuntimeCall {
            callee: "aot_str_compare",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_slice",
        body: StdlibBody::RuntimeCall {
            callee: "aot_str_slice_utf8",
            args: &[
                CallArgSpec::ForwardArg(0),
                CallArgSpec::ForwardArg(1),
                CallArgSpec::ForwardArg(2),
            ],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_char_at",
        body: StdlibBody::RuntimeCall {
            callee: "aot_str_char_at",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_contains",
        body: StdlibBody::RuntimeCall {
            callee: "aot_contains",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_index_of",
        body: StdlibBody::RuntimeCall {
            callee: "aot_str_index_of",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_str_to_string",
        body: StdlibBody::RuntimeCall {
            callee: "aot_to_string",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    // std::collections
    StdlibFnSpec {
        name: "adesh_std_col_vec_new",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_vec_new",
            args: &[],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_vec_push",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_vec_push",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_vec_pop",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_vec_pop",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_vec_get",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_vec_get",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_vec_set",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_vec_set",
            args: &[
                CallArgSpec::ForwardArg(0),
                CallArgSpec::ForwardArg(1),
                CallArgSpec::ForwardArg(2),
            ],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_len",
        body: StdlibBody::RuntimeCall {
            callee: "aot_len",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_contains",
        body: StdlibBody::RuntimeCall {
            callee: "aot_contains",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_map_new",
        body: StdlibBody::RuntimeCall {
            callee: "aot_make_dict",
            args: &[],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_map_set",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_map_set",
            args: &[
                CallArgSpec::ForwardArg(0),
                CallArgSpec::ForwardArg(1),
                CallArgSpec::ForwardArg(2),
            ],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_map_get",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_map_get",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_map_remove",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_map_remove",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_set_new",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_set_new",
            args: &[],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_set_add",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_set_add",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_col_set_remove",
        body: StdlibBody::RuntimeCall {
            callee: "aot_col_set_remove",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    // std::math (pure Machine IR integer algorithms)
    StdlibFnSpec {
        name: "adesh_std_math_abs_i64",
        body: StdlibBody::MathAbsI64,
    },
    StdlibFnSpec {
        name: "adesh_std_math_min_i64",
        body: StdlibBody::MathMinI64,
    },
    StdlibFnSpec {
        name: "adesh_std_math_max_i64",
        body: StdlibBody::MathMaxI64,
    },
    StdlibFnSpec {
        name: "adesh_std_math_clamp_i64",
        body: StdlibBody::MathClampI64,
    },
    // std::math (f64 dispatcher)
    StdlibFnSpec {
        name: "adesh_std_math_sin",
        body: StdlibBody::MathF64 { op: 1, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_cos",
        body: StdlibBody::MathF64 { op: 2, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_tan",
        body: StdlibBody::MathF64 { op: 3, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_asin",
        body: StdlibBody::MathF64 { op: 4, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_acos",
        body: StdlibBody::MathF64 { op: 5, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_atan",
        body: StdlibBody::MathF64 { op: 6, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_sqrt",
        body: StdlibBody::MathF64 { op: 7, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_exp",
        body: StdlibBody::MathF64 { op: 8, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_ln",
        body: StdlibBody::MathF64 { op: 9, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_log10",
        body: StdlibBody::MathF64 { op: 10, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_fabs",
        body: StdlibBody::MathF64 { op: 11, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_floor",
        body: StdlibBody::MathF64 { op: 12, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_ceil",
        body: StdlibBody::MathF64 { op: 13, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_round",
        body: StdlibBody::MathF64 { op: 14, binary: false },
    },
    StdlibFnSpec {
        name: "adesh_std_math_pow",
        body: StdlibBody::MathF64 { op: 16, binary: true },
    },
    // std::time
    StdlibFnSpec {
        name: "adesh_std_time_clock",
        body: StdlibBody::RuntimeCall {
            callee: "aot_clock",
            args: &[],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_time_now_ms",
        body: StdlibBody::RuntimeCall {
            callee: "aot_date_now",
            args: &[],
        },
    },
    // std::fs
    StdlibFnSpec {
        name: "adesh_std_fs_read",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_read",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_write",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_write_bool",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_exists",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_exists_bool",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_is_file",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_is_file_bool",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_is_dir",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_is_dir_bool",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_mkdir",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_mkdir_bool",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_delete",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_delete_bool",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_readdir",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_readdir",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_fs_file_size",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_metadata_size",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    // std::path
    StdlibFnSpec {
        name: "adesh_std_path_join",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_path_join2",
            args: &[CallArgSpec::ForwardArg(0), CallArgSpec::ForwardArg(1)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_path_basename",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_path_basename",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_path_dirname",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_path_dirname",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_path_extname",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_path_extname",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
    StdlibFnSpec {
        name: "adesh_std_path_normalize",
        body: StdlibBody::RuntimeCall {
            callee: "aot_fs_path_normalize",
            args: &[CallArgSpec::ForwardArg(0)],
        },
    },
];

/// Builder for generating target-specific native precompiled standard library
/// ADOB modules and multi-target bundles.
pub struct StdlibAdobBuilder {
    pub target: TargetDescriptor,
}

impl StdlibAdobBuilder {
    pub fn new(target: TargetDescriptor) -> Self {
        Self { target }
    }

    /// Canonical list of all `adesh_std_*` symbols provided by `libadesh_std.adob`.
    pub fn exported_symbol_names() -> Vec<&'static str> {
        STDLIB_SPECS.iter().map(|s| s.name).collect()
    }

    /// Returns `true` if `name` is a known Phase 19.7 native stdlib symbol.
    pub fn is_stdlib_symbol(name: &str) -> bool {
        STDLIB_SPECS.iter().any(|s| s.name == name)
    }

    /// The 5 full-native 64-bit targets supported for precompiled `libadesh_std.adob`.
    pub fn supported_native_targets() -> Vec<TargetDescriptor> {
        [
            "x86_64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "aarch64-apple-darwin",
            "riscv64gc-unknown-linux-gnu",
        ]
        .iter()
        .map(|t| TargetDescriptor::from_triple(t).expect("builtin stdlib triple must parse"))
        .collect()
    }

    /// Returns `true` when `target` is a supported 64-bit `FullNative` target
    /// for precompiled `libadesh_std.adob`.
    pub fn is_native_target_supported(target: &TargetDescriptor) -> bool {
        target.arch_descriptor().runtime_profile == adesh_object::RuntimeProfile::FullNative
            && matches!(
                target.architecture,
                Architecture::X86_64 | Architecture::AArch64 | Architecture::RiscV64
            )
    }

    fn build_function(
        &self,
        spec: &StdlibFnSpec,
        conv: &dyn CallingConvention,
    ) -> Result<MachineFunction, CodegenError> {
        let arg_regs = conv.arg_registers();
        let fp_arg_regs = conv.fp_arg_registers();
        let ret_reg = conv.return_registers()[0];
        let is_win64 = self.target.architecture == Architecture::X86_64
            && self.target.operating_system == OperatingSystem::Windows;

        let mut func = MachineFunction::new(spec.name);
        func.is_exported = true;

        match spec.body {
            StdlibBody::AbiVersion => {
                let blk = func.entry_block_mut();
                blk.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: MachineOperand::Immediate(STDLIB_ABI_VERSION as i64),
                });
                blk.push(MachineInstruction::Return);
            }
            StdlibBody::MathAbsI64 => {
                let a0 = arg_regs[0];
                let neg_label = format!("{}_neg", spec.name);
                let done_label = format!("{}_done", spec.name);

                let entry = func.entry_block_mut();
                entry.push(MachineInstruction::Compare {
                    lhs: preg(a0),
                    rhs: MachineOperand::Immediate(0),
                });
                entry.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::LessThan,
                    target: neg_label.clone(),
                });
                entry.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(a0),
                });
                entry.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut neg_blk = MachineBlock::new(1, &neg_label);
                neg_blk.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: MachineOperand::Immediate(0),
                });
                neg_blk.push(MachineInstruction::Sub {
                    dst: preg(ret_reg),
                    src: preg(a0),
                });
                neg_blk.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut done_blk = MachineBlock::new(2, &done_label);
                done_blk.push(MachineInstruction::Return);

                func.blocks.push(neg_blk);
                func.blocks.push(done_blk);
            }
            StdlibBody::MathMinI64 => {
                let (a0, a1) = (arg_regs[0], arg_regs[1]);
                let pick_a_label = format!("{}_pick_a", spec.name);
                let done_label = format!("{}_done", spec.name);

                let entry = func.entry_block_mut();
                entry.push(MachineInstruction::Compare {
                    lhs: preg(a0),
                    rhs: preg(a1),
                });
                entry.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::LessThan,
                    target: pick_a_label.clone(),
                });
                entry.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(a1),
                });
                entry.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut pick_a = MachineBlock::new(1, &pick_a_label);
                pick_a.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(a0),
                });
                pick_a.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut done = MachineBlock::new(2, &done_label);
                done.push(MachineInstruction::Return);

                func.blocks.push(pick_a);
                func.blocks.push(done);
            }
            StdlibBody::MathMaxI64 => {
                let (a0, a1) = (arg_regs[0], arg_regs[1]);
                let pick_a_label = format!("{}_pick_a", spec.name);
                let done_label = format!("{}_done", spec.name);

                let entry = func.entry_block_mut();
                entry.push(MachineInstruction::Compare {
                    lhs: preg(a0),
                    rhs: preg(a1),
                });
                entry.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::GreaterThan,
                    target: pick_a_label.clone(),
                });
                entry.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(a1),
                });
                entry.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut pick_a = MachineBlock::new(1, &pick_a_label);
                pick_a.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(a0),
                });
                pick_a.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut done = MachineBlock::new(2, &done_label);
                done.push(MachineInstruction::Return);

                func.blocks.push(pick_a);
                func.blocks.push(done);
            }
            StdlibBody::MathClampI64 => {
                let (x, lo, hi) = (arg_regs[0], arg_regs[1], arg_regs[2]);
                let low_label = format!("{}_lo", spec.name);
                let check_hi_label = format!("{}_chk_hi", spec.name);
                let high_label = format!("{}_hi", spec.name);
                let done_label = format!("{}_done", spec.name);

                let entry = func.entry_block_mut();
                entry.push(MachineInstruction::Compare {
                    lhs: preg(x),
                    rhs: preg(lo),
                });
                entry.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::LessThan,
                    target: low_label.clone(),
                });
                entry.push(MachineInstruction::Branch {
                    target: check_hi_label.clone(),
                });

                let mut low_blk = MachineBlock::new(1, &low_label);
                low_blk.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(lo),
                });
                low_blk.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut chk_hi = MachineBlock::new(2, &check_hi_label);
                chk_hi.push(MachineInstruction::Compare {
                    lhs: preg(x),
                    rhs: preg(hi),
                });
                chk_hi.push(MachineInstruction::BranchCc {
                    cc: ConditionCode::GreaterThan,
                    target: high_label.clone(),
                });
                chk_hi.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(x),
                });
                chk_hi.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut high_blk = MachineBlock::new(3, &high_label);
                high_blk.push(MachineInstruction::Move {
                    dst: preg(ret_reg),
                    src: preg(hi),
                });
                high_blk.push(MachineInstruction::Branch {
                    target: done_label.clone(),
                });

                let mut done = MachineBlock::new(4, &done_label);
                done.push(MachineInstruction::Return);

                func.blocks.push(low_blk);
                func.blocks.push(chk_hi);
                func.blocks.push(high_blk);
                func.blocks.push(done);
            }
            StdlibBody::RuntimeCall { callee, args } => {
                // Allocate Win64 32-byte shadow/home area when calling external C ABI functions.
                func.stack_size = conv.shadow_space() as u64;
                let blk = func.entry_block_mut();
                for (slot_idx, arg_spec) in args.iter().enumerate() {
                    let dst_reg = arg_regs[slot_idx];
                    match *arg_spec {
                        CallArgSpec::ForwardArg(src_idx) => {
                            let src_reg = arg_regs[src_idx];
                            if src_reg != dst_reg {
                                blk.push(MachineInstruction::Move {
                                    dst: preg(dst_reg),
                                    src: preg(src_reg),
                                });
                            }
                        }
                        CallArgSpec::Imm(imm) => {
                            blk.push(MachineInstruction::Move {
                                dst: preg(dst_reg),
                                src: MachineOperand::Immediate(imm),
                            });
                        }
                    }
                }
                blk.push(MachineInstruction::Call {
                    target: MachineOperand::Symbol(callee.to_string()),
                    num_args: args.len(),
                });
                blk.push(MachineInstruction::Return);
            }
            StdlibBody::MathF64 { op, binary } => {
                func.stack_size = conv.shadow_space() as u64;
                let blk = func.entry_block_mut();

                if is_win64 {
                    // Win64 positional slots for aot_math_f64(op: i64, x: f64, y: f64):
                    // slot 0 -> RCX (op), slot 1 -> XMM1 (x), slot 2 -> XMM2 (y).
                    // Move XMM1 -> XMM2 first so binary `y` is not clobbered by `XMM0 -> XMM1`.
                    if binary {
                        blk.push(MachineInstruction::Move {
                            dst: preg(fp_arg_regs[2]),
                            src: preg(fp_arg_regs[1]),
                        });
                    }
                    blk.push(MachineInstruction::Move {
                        dst: preg(fp_arg_regs[1]),
                        src: preg(fp_arg_regs[0]),
                    });
                    blk.push(MachineInstruction::Move {
                        dst: preg(arg_regs[0]),
                        src: MachineOperand::Immediate(op),
                    });
                } else {
                    // SysV x64, AAPCS64, and RV64 LP64D use independent GPR and FP counters:
                    // x is already in fp_arg_regs[0] (and y in fp_arg_regs[1]), so we only load op.
                    blk.push(MachineInstruction::Move {
                        dst: preg(arg_regs[0]),
                        src: MachineOperand::Immediate(op),
                    });
                }
                blk.push(MachineInstruction::Call {
                    target: MachineOperand::Symbol("aot_math_f64".to_string()),
                    num_args: if binary { 3 } else { 2 },
                });
                blk.push(MachineInstruction::Return);
            }
        }

        Ok(func)
    }

    /// Build the complete functional standard library `NativeModule` for `self.target`.
    pub fn build_native_module(&self) -> Result<NativeModule, CodegenError> {
        self.build_native_module_filtered(None)
    }

    /// Build a dependency-pruned `NativeModule` containing only the requested
    /// `adesh_std_*` symbols (plus `adesh_std_abi_version`).
    pub fn build_native_module_filtered(
        &self,
        required_symbols: Option<&[&str]>,
    ) -> Result<NativeModule, CodegenError> {
        let conv = calling_convention_for(&self.target)?;
        let mut module = NativeModule::new("adesh_std");

        let selected: BTreeSet<&str> = match required_symbols {
            None => STDLIB_SPECS.iter().map(|s| s.name).collect(),
            Some(req) => {
                let mut set = BTreeSet::new();
                set.insert("adesh_std_abi_version");
                for &sym in req {
                    if !Self::is_stdlib_symbol(sym) {
                        return Err(CodegenError::new(
                            target_tag(&self.target),
                            format!(
                                "Unknown native stdlib symbol `{}` requested during dependency pruning",
                                sym
                            ),
                        ));
                    }
                    set.insert(sym);
                }
                set
            }
        };

        for spec in STDLIB_SPECS {
            if selected.contains(spec.name) {
                let func = self.build_function(spec, conv.as_ref())?;
                module.add_function(func);
            }
        }

        Ok(module)
    }

    fn attach_abi_metadata(&self, adob: &mut AdobObject) -> Result<(), CodegenError> {
        let text_bytes = adob
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .map(|s| s.data.as_slice())
            .unwrap_or(&[]);

        let mut exports: Vec<String> = adob
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("adesh_std_") && s.section_index.is_some())
            .map(|s| s.name.clone())
            .collect();
        exports.sort();
        exports.dedup();

        let mut payload = Vec::new();
        payload.extend_from_slice(STDLIB_ABI_TAG);
        payload.extend_from_slice(&STDLIB_ABI_VERSION.to_le_bytes());
        let target_desc = format!(
            "{:?}|{:?}|{:?}|{:?}",
            self.target.architecture,
            self.target.operating_system,
            self.target.abi,
            self.target.endianness
        );
        payload.extend_from_slice(&(target_desc.len() as u32).to_le_bytes());
        payload.extend_from_slice(target_desc.as_bytes());
        payload.extend_from_slice(&(exports.len() as u32).to_le_bytes());
        for sym in &exports {
            payload.extend_from_slice(&(sym.len() as u32).to_le_bytes());
            payload.extend_from_slice(sym.as_bytes());
        }

        let mut hash_input = Vec::with_capacity(text_bytes.len() + payload.len());
        hash_input.extend_from_slice(text_bytes);
        hash_input.extend_from_slice(&payload);
        let digest = fnv1a_64(&hash_input);
        payload.extend_from_slice(&digest.to_le_bytes());

        let meta_sec = AdobSection::new(STDLIB_METADATA_SECTION, SectionKind::AdeshMeta)
            .with_flags(section_flags::READ)
            .with_alignment(8)
            .with_data(payload);
        adob.add_section(meta_sec);
        Ok(())
    }

    /// Compile and emit a validated, ABI-tagged `libadesh_std.adob` for `self.target`.
    pub fn emit_adob(&self) -> Result<AdobObject, CodegenError> {
        self.emit_adob_for_imports_opt(None)
    }

    /// Emit a pruned `libadesh_std.adob` containing only the referenced `adesh_std_*`
    /// symbols (plus `adesh_std_abi_version`).
    pub fn emit_adob_for_imports(&self, referenced: &[&str]) -> Result<AdobObject, CodegenError> {
        self.emit_adob_for_imports_opt(Some(referenced))
    }

    fn emit_adob_for_imports_opt(
        &self,
        referenced: Option<&[&str]>,
    ) -> Result<AdobObject, CodegenError> {
        let mut backend = create_backend(self.target.clone())?;
        let module = self.build_native_module_filtered(referenced)?;
        let lowered = backend.lower_module(&module)?;
        let mut adob = backend.emit_object(&lowered)?;
        self.attach_abi_metadata(&mut adob)?;
        Self::validate_stdlib_adob(&adob, &self.target)?;
        Ok(adob)
    }

    /// Validate a loaded or generated `libadesh_std.adob` against `expected_target`,
    /// ABI version, and `.adesh.std_abi` integrity checksum.
    pub fn validate_stdlib_adob(
        adob: &AdobObject,
        expected_target: &TargetDescriptor,
    ) -> Result<(), CodegenError> {
        let ttag = target_tag(expected_target);
        AdobValidator::validate(adob).map_err(|e| {
            CodegenError::new(&ttag, format!("Invalid stdlib ADOB object: {}", e))
        })?;

        if adob.target.architecture != expected_target.architecture
            || adob.target.operating_system != expected_target.operating_system
            || adob.target.abi != expected_target.abi
            || adob.target.endianness != expected_target.endianness
        {
            return Err(CodegenError::new(
                &ttag,
                format!(
                    "Stdlib ADOB target mismatch: archive built for `{:?}/{:?}/{:?}`, expected `{:?}/{:?}/{:?}`",
                    adob.target.architecture,
                    adob.target.operating_system,
                    adob.target.abi,
                    expected_target.architecture,
                    expected_target.operating_system,
                    expected_target.abi
                ),
            ));
        }

        let meta = adob
            .sections
            .iter()
            .find(|s| s.name == STDLIB_METADATA_SECTION)
            .ok_or_else(|| {
                CodegenError::new(
                    &ttag,
                    format!(
                        "Stdlib ADOB is missing required `{}` metadata section",
                        STDLIB_METADATA_SECTION
                    ),
                )
            })?;

        let data = &meta.data;
        if data.len() < 16 + 4 + 4 + 4 + 8 {
            return Err(CodegenError::new(
                &ttag,
                "Stdlib ADOB `.adesh.std_abi` metadata section is truncated",
            ));
        }
        if &data[0..16] != STDLIB_ABI_TAG {
            return Err(CodegenError::new(
                &ttag,
                "Stdlib ADOB `.adesh.std_abi` tag mismatch",
            ));
        }
        let ver = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        if ver != STDLIB_ABI_VERSION {
            return Err(CodegenError::new(
                &ttag,
                format!(
                    "Stdlib ADOB ABI version {} does not match expected version {}",
                    ver, STDLIB_ABI_VERSION
                ),
            ));
        }

        let text_bytes = adob
            .sections
            .iter()
            .find(|s| s.name == ".text")
            .map(|s| s.data.as_slice())
            .unwrap_or(&[]);
        if text_bytes.is_empty() {
            return Err(CodegenError::new(
                &ttag,
                "Stdlib ADOB `.text` section is empty",
            ));
        }

        let payload_end = data.len() - 8;
        let expected_digest = u64::from_le_bytes(data[payload_end..data.len()].try_into().unwrap());
        let mut hash_input = Vec::with_capacity(text_bytes.len() + payload_end);
        hash_input.extend_from_slice(text_bytes);
        hash_input.extend_from_slice(&data[..payload_end]);
        let actual_digest = fnv1a_64(&hash_input);
        if actual_digest != expected_digest {
            return Err(CodegenError::new(
                &ttag,
                format!(
                    "Stdlib ADOB integrity digest mismatch (expected 0x{:016X}, got 0x{:016X})",
                    expected_digest, actual_digest
                ),
            ));
        }

        Ok(())
    }

    /// Build a multi-architecture standard library fat bundle (`AdobBundle`)
    /// containing real compiled archives for all supported 64-bit native targets.
    pub fn build_multi_target_bundle() -> Result<AdobBundle, CodegenError> {
        let mut bundle = AdobBundle::new();
        for target in Self::supported_native_targets() {
            let builder = StdlibAdobBuilder::new(target);
            let obj = builder.emit_adob()?;
            bundle.add_object(obj);
        }
        Ok(bundle)
    }
}

/// Canonical endiness helper kept referenced cleanly.
#[allow(dead_code)]
fn _expected_endianness(target: &TargetDescriptor) -> Endianness {
    target.endianness
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opt::verifier::MachineIRVerifier;
    use adesh_object::validator::AdobValidator;

    #[test]
    fn test_stdlib_adob_generation_and_verification_all_native_targets() {
        for target in StdlibAdobBuilder::supported_native_targets() {
            let builder = StdlibAdobBuilder::new(target.clone());
            let module = builder
                .build_native_module()
                .expect("native stdlib module builds");
            for func in &module.functions {
                MachineIRVerifier::verify(func).unwrap_or_else(|e| {
                    panic!("Machine IR verification failed for `{}`: {:?}", func.name, e)
                });
            }

            let adob = builder.emit_adob().expect("Failed to emit stdlib ADOB");
            AdobValidator::validate(&adob).expect("Generated stdlib ADOB must be valid");
            StdlibAdobBuilder::validate_stdlib_adob(&adob, &target)
                .expect("Stdlib ABI metadata & checksum must validate");

            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_abi_version"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_io_println"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_str_concat"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_col_vec_push"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_math_sqrt"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_math_abs_i64"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_time_now_ms"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_fs_read"));
            assert!(adob.symbols.iter().any(|s| s.name == "adesh_std_path_join"));

            // Ensure old stub names are NOT emitted as bare stubs in libadesh_std.adob
            assert!(
                !adob
                    .symbols
                    .iter()
                    .any(|s| s.section_index.is_some() && s.name == "aot_make_string"),
                "libadesh_std.adob must not shadow runtime symbols with stubs"
            );
        }
    }

    #[test]
    fn test_stdlib_dependency_pruning_and_determinism() {
        let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
        let builder = StdlibAdobBuilder::new(target.clone());

        let pruned_a = builder
            .emit_adob_for_imports(&["adesh_std_io_println", "adesh_std_math_abs_i64"])
            .expect("pruned stdlib emits");
        let pruned_b = builder
            .emit_adob_for_imports(&["adesh_std_io_println", "adesh_std_math_abs_i64"])
            .expect("pruned stdlib emits deterministically");
        assert_eq!(pruned_a, pruned_b);

        let defined_std_syms: Vec<&str> = pruned_a
            .symbols
            .iter()
            .filter(|s| s.section_index.is_some() && s.name.starts_with("adesh_std_"))
            .map(|s| s.name.as_str())
            .collect();
        assert!(defined_std_syms.contains(&"adesh_std_abi_version"));
        assert!(defined_std_syms.contains(&"adesh_std_io_println"));
        assert!(defined_std_syms.contains(&"adesh_std_math_abs_i64"));
        assert!(!defined_std_syms.contains(&"adesh_std_fs_read"));
    }

    #[test]
    fn test_stdlib_cross_target_and_corrupt_rejection() {
        let linux = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
        let win = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap();
        let wasm = TargetDescriptor::from_triple("wasm32-unknown-unknown").unwrap();

        let adob = StdlibAdobBuilder::new(linux.clone()).emit_adob().unwrap();
        assert!(StdlibAdobBuilder::validate_stdlib_adob(&adob, &win).is_err());

        // Corrupt `.adesh.std_abi`
        let mut corrupt = adob.clone();
        corrupt
            .sections
            .iter_mut()
            .find(|s| s.name == STDLIB_METADATA_SECTION)
            .unwrap()
            .data[20] ^= 0xFF;
        assert!(StdlibAdobBuilder::validate_stdlib_adob(&corrupt, &linux).is_err());

        // Unsupported target must return structured error, not emit stubs
        assert!(StdlibAdobBuilder::new(wasm).emit_adob().is_err());
    }

    #[test]
    fn test_stdlib_multi_target_bundle_roundtrip() {
        let bundle =
            StdlibAdobBuilder::build_multi_target_bundle().expect("Multi-target bundle builds");
        assert_eq!(
            bundle.entries.len(),
            StdlibAdobBuilder::supported_native_targets().len()
        );
        let bytes = bundle.write().expect("bundle write");
        let parsed = AdobBundle::read(&bytes).expect("bundle read");
        for target in StdlibAdobBuilder::supported_native_targets() {
            let obj = parsed
                .find_for_target(&target)
                .expect("every native target must exist in bundle");
            StdlibAdobBuilder::validate_stdlib_adob(obj, &target).unwrap();
        }
    }
}
