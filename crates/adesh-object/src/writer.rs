use crate::error::AdobResult;
use crate::format::{
    ADOB_MAGIC, ADOB_VERSION_MAJOR, ADOB_VERSION_MINOR, ADOB_VERSION_PATCH, AdobObject,
};
use crate::metadata::{MemoryOrderModel, TlsModel};
use crate::relocation::RelocationKind;
use crate::section::{MemoryPermissions, SectionKind};
use crate::symbol::{SymbolBinding, SymbolKind, SymbolVisibility};
use crate::target::{
    Abi, Architecture, ComputeDevice, Endianness, Environment, OperatingSystem, PointerWidth,
};
use crate::validator::AdobValidator;

pub struct AdobWriter;

impl AdobWriter {
    /// Encode an AdobObject into a validated, deterministic byte vector.
    ///
    /// The object is validated before encoding so structural defects (bad
    /// alignments, duplicate sections, out-of-bounds symbols, dangling
    /// relocations) fail loudly at the point of production instead of being
    /// discovered by the reader later.
    pub fn write(obj: &AdobObject) -> AdobResult<Vec<u8>> {
        AdobValidator::validate(obj)?;

        let mut buf = Vec::with_capacity(1024 + obj.total_section_size() as usize);

        // 1. Header Magic & Version
        buf.extend_from_slice(ADOB_MAGIC);
        buf.extend_from_slice(&ADOB_VERSION_MAJOR.to_le_bytes());
        buf.extend_from_slice(&ADOB_VERSION_MINOR.to_le_bytes());
        buf.extend_from_slice(&ADOB_VERSION_PATCH.to_le_bytes());

        // Header Flags
        buf.extend_from_slice(&obj.header.flags.to_le_bytes());

        // 2. Target Descriptor
        let dev_id: u8 = match obj.target.device {
            ComputeDevice::Cpu => 0,
            ComputeDevice::Gpu => 1,
            ComputeDevice::Npu => 2,
            ComputeDevice::Tpu => 3,
            ComputeDevice::Dsp => 4,
            ComputeDevice::Fpga => 5,
            ComputeDevice::Accelerator => 6,
            ComputeDevice::Embedded => 7,
            ComputeDevice::Custom => 255,
        };
        buf.push(dev_id);

        let (arch_id, arch_custom) = match &obj.target.architecture {
            Architecture::X86_64 => (0u16, None),
            Architecture::X86 => (1, None),
            Architecture::AArch64 => (2, None),
            Architecture::Arm => (3, None),
            Architecture::RiscV64 => (4, None),
            Architecture::RiscV32 => (5, None),
            Architecture::PowerPc64 => (6, None),
            Architecture::PowerPc => (7, None),
            Architecture::Mips => (8, None),
            Architecture::Wasm32 => (9, None),
            Architecture::Wasm64 => (10, None),
            Architecture::Gpu(_) => (11, None),
            Architecture::Npu(_) => (12, None),
            Architecture::Tpu(_) => (13, None),
            Architecture::Embedded(_) => (14, None),
            Architecture::Custom(s) => (255, Some(s.clone())),
        };
        buf.extend_from_slice(&arch_id.to_le_bytes());
        write_opt_string(&mut buf, arch_custom.as_deref());

        let os_id: u8 = match obj.target.operating_system {
            OperatingSystem::None => 0,
            OperatingSystem::Windows => 1,
            OperatingSystem::Linux => 2,
            OperatingSystem::MacOS => 3,
            OperatingSystem::Ios => 4,
            OperatingSystem::Android => 5,
            OperatingSystem::FreeBsd => 6,
            OperatingSystem::NetBsd => 7,
            OperatingSystem::OpenBsd => 8,
            OperatingSystem::Wasi => 9,
            OperatingSystem::CudaRuntime => 10,
            OperatingSystem::Custom => 255,
        };
        buf.push(os_id);

        let env_id: u8 = match obj.target.environment {
            Environment::Msvc => 0,
            Environment::Gnu => 1,
            Environment::Musl => 2,
            Environment::Eabi => 3,
            Environment::Eabihf => 4,
            Environment::Android => 5,
            Environment::Sim => 6,
            Environment::Unknown => 255,
        };
        buf.push(env_id);

        let abi_id: u8 = match obj.target.abi {
            Abi::Default => 0,
            Abi::SystemV => 1,
            Abi::WindowsX64 => 2,
            Abi::Aapcs64 => 3,
            Abi::Aapcs32 => 4,
            Abi::RiscvAbi => 5,
            Abi::WasmAbi => 6,
            Abi::AdeshInternal => 7,
            Abi::BareMetal => 8,
            Abi::CudaKernelAbi => 9,
        };
        buf.push(abi_id);

        let ptr_width_id: u8 = match obj.target.pointer_width {
            PointerWidth::U32 => 4,
            PointerWidth::U64 => 8,
        };
        buf.push(ptr_width_id);

        let endian_id: u8 = match obj.target.endianness {
            Endianness::Little => 0,
            Endianness::Big => 1,
        };
        buf.push(endian_id);

        // Target Features
        buf.extend_from_slice(&(obj.target.features.features.len() as u32).to_le_bytes());
        for f in &obj.target.features.features {
            write_string(&mut buf, &f.name);
            buf.push(if f.enabled { 1 } else { 0 });
        }

        // 3. Counts
        buf.extend_from_slice(&(obj.sections.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.symbols.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.imports.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.exports.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.memory_regions.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.extensions.extensions.len() as u32).to_le_bytes());

        // 4. Sections
        for sec in &obj.sections {
            write_string(&mut buf, &sec.name);
            let kind_id: u8 = match sec.kind {
                SectionKind::Text => 0,
                SectionKind::Rodata => 1,
                SectionKind::Data => 2,
                SectionKind::Bss => 3,
                SectionKind::Tls => 4,
                SectionKind::Unwind => 5,
                SectionKind::Debug => 6,
                SectionKind::AdeshMeta => 7,
                SectionKind::Comdat => 8,
                SectionKind::Extension => 9,
                SectionKind::Custom => 10,
            };
            buf.push(kind_id);
            buf.extend_from_slice(&sec.flags.to_le_bytes());
            buf.extend_from_slice(&sec.alignment.to_le_bytes());
            write_opt_string(&mut buf, sec.comdat_group.as_deref());
            write_opt_string(&mut buf, sec.memory_region.as_deref());

            // Payload
            buf.extend_from_slice(&(sec.data.len() as u64).to_le_bytes());
            buf.extend_from_slice(&sec.data);

            // Section Relocations
            buf.extend_from_slice(&(sec.relocations.len() as u32).to_le_bytes());
            for reloc in &sec.relocations {
                buf.extend_from_slice(&reloc.offset.to_le_bytes());
                buf.extend_from_slice(&reloc.symbol.to_le_bytes());
                write_string(&mut buf, &reloc.symbol_name);
                let r_kind_id: u32 = match reloc.kind {
                    RelocationKind::Absolute64 => 0,
                    RelocationKind::Absolute32 => 1,
                    RelocationKind::PcRelative32 => 2,
                    RelocationKind::PcRelative64 => 3,
                    RelocationKind::PltRelative32 => 4,
                    RelocationKind::GotRelative32 => 5,
                    RelocationKind::TlsGd => 6,
                    RelocationKind::TlsLd => 7,
                    RelocationKind::TlsIe => 8,
                    RelocationKind::TlsLe => 9,
                    RelocationKind::ImageRelative32 => 60,
                    RelocationKind::X86_64_GotPcrel => 10,
                    RelocationKind::X86_64_Plt32 => 11,
                    RelocationKind::X86_64_RexGotPcrelX => 12,
                    RelocationKind::AArch64_Call26 => 20,
                    RelocationKind::AArch64_AdrPage21 => 21,
                    RelocationKind::AArch64_AddAbsLo12 => 22,
                    RelocationKind::AArch64_LdSt64Lo12 => 23,
                    RelocationKind::RiscV_Call => 30,
                    RelocationKind::RiscV_PcrelHi20 => 31,
                    RelocationKind::RiscV_PcrelLo12I => 32,
                    RelocationKind::RiscV_PcrelLo12S => 33,
                    RelocationKind::RiscV_RvcBranch => 34,
                    RelocationKind::RiscV_RvcJump => 35,
                    RelocationKind::WasmFunctionIndex => 40,
                    RelocationKind::WasmTableIndex => 41,
                    RelocationKind::WasmGlobalIndex => 42,
                    RelocationKind::WasmTypeIndex => 43,
                    RelocationKind::WasmMemoryAddress => 44,
                    RelocationKind::Custom(val) => 1000 + val,
                };
                buf.extend_from_slice(&r_kind_id.to_le_bytes());
                buf.extend_from_slice(&reloc.addend.to_le_bytes());
                buf.push(reloc.width);
                buf.extend_from_slice(&reloc.flags.0.to_le_bytes());
            }
        }

        // 5. Symbols
        for sym in &obj.symbols {
            buf.extend_from_slice(&sym.id.to_le_bytes());
            write_string(&mut buf, &sym.name);
            let b_id: u8 = match sym.binding {
                SymbolBinding::Local => 0,
                SymbolBinding::Global => 1,
                SymbolBinding::Weak => 2,
                SymbolBinding::Unique => 3,
            };
            let v_id: u8 = match sym.visibility {
                SymbolVisibility::Default => 0,
                SymbolVisibility::Hidden => 1,
                SymbolVisibility::Protected => 2,
                SymbolVisibility::Internal => 3,
            };
            let k_id: u8 = match sym.kind {
                SymbolKind::Function => 0,
                SymbolKind::Object => 1,
                SymbolKind::Section => 2,
                SymbolKind::TLS => 3,
                SymbolKind::Import => 4,
                SymbolKind::Export => 5,
                SymbolKind::AcceleratorKernel => 6,
                SymbolKind::Runtime => 7,
            };
            buf.push(b_id);
            buf.push(v_id);
            buf.push(k_id);
            buf.push(if sym.is_defined { 1 } else { 0 });
            let sec_idx = sym.section_index.unwrap_or(u32::MAX);
            buf.extend_from_slice(&sec_idx.to_le_bytes());
            buf.extend_from_slice(&sym.value.to_le_bytes());
            buf.extend_from_slice(&sym.size.to_le_bytes());
            write_opt_string(&mut buf, sym.version.as_deref());
            write_opt_string(&mut buf, sym.alias_target.as_deref());
        }

        // 6. Imports & Exports
        for imp in &obj.imports {
            write_string(&mut buf, imp);
        }
        for exp in &obj.exports {
            write_string(&mut buf, exp);
        }

        // 7. Memory Regions
        for mr in &obj.memory_regions {
            write_string(&mut buf, &mr.name);
            buf.extend_from_slice(&mr.address.to_le_bytes());
            buf.extend_from_slice(&mr.size.to_le_bytes());
            let perm_id: u8 = match mr.permissions {
                MemoryPermissions::R => 0,
                MemoryPermissions::Rw => 1,
                MemoryPermissions::Rx => 2,
                MemoryPermissions::Rwx => 3,
            };
            buf.push(perm_id);
        }

        // 8. Extensions
        for ext in &obj.extensions.extensions {
            buf.extend_from_slice(&ext.uuid);
            buf.extend_from_slice(&ext.version.0.to_le_bytes());
            buf.extend_from_slice(&ext.version.1.to_le_bytes());
            write_string(&mut buf, &ext.tag);
            buf.extend_from_slice(&(ext.payload.len() as u32).to_le_bytes());
            buf.extend_from_slice(&ext.payload);
        }

        // 9. Build Metadata
        write_string(&mut buf, &obj.build_metadata.compiler_version);
        write_string(&mut buf, &obj.build_metadata.target_triple);
        buf.push(obj.build_metadata.opt_level);
        buf.push(if obj.build_metadata.deterministic {
            1
        } else {
            0
        });

        // 10. Safety Metadata
        buf.push(if obj.safety.bounds_checking { 1 } else { 0 });
        buf.push(if obj.safety.overflow_checks { 1 } else { 0 });
        buf.push(if obj.safety.null_pointer_checks { 1 } else { 0 });
        buf.push(if obj.safety.memory_sanitizer { 1 } else { 0 });
        buf.push(if obj.safety.address_sanitizer { 1 } else { 0 });
        buf.push(if obj.safety.thread_sanitizer { 1 } else { 0 });
        buf.push(if obj.safety.strict_provenance { 1 } else { 0 });
        buf.push(if obj.safety.isolated_heap { 1 } else { 0 });
        buf.push(if obj.safety.stack_canary_present {
            1
        } else {
            0
        });

        // 11. Thread Safety Metadata
        let mo_id: u8 = match obj.thread_safety.default_memory_order {
            MemoryOrderModel::Relaxed => 0,
            MemoryOrderModel::Acquire => 1,
            MemoryOrderModel::Release => 2,
            MemoryOrderModel::AcqRel => 3,
            MemoryOrderModel::SeqCst => 4,
        };
        buf.push(mo_id);
        let tls_id: u8 = match obj.thread_safety.tls_model {
            TlsModel::GeneralDynamic => 0,
            TlsModel::LocalDynamic => 1,
            TlsModel::InitialExec => 2,
            TlsModel::LocalExec => 3,
        };
        buf.push(tls_id);
        buf.extend_from_slice(&obj.thread_safety.atomic_alignment.to_le_bytes());
        buf.push(if obj.thread_safety.lock_free_primitives {
            1
        } else {
            0
        });
        buf.push(if obj.thread_safety.data_race_detection {
            1
        } else {
            0
        });

        // 12. Optimization Metadata
        buf.push(if obj.optimization.lto_eligible { 1 } else { 0 });
        buf.push(if obj.optimization.icf_eligible { 1 } else { 0 });
        buf.push(if obj.optimization.dead_code_eliminated {
            1
        } else {
            0
        });
        buf.push(if obj.optimization.peephole_optimized {
            1
        } else {
            0
        });
        buf.push(if obj.optimization.constant_propagation {
            1
        } else {
            0
        });
        buf.push(if obj.optimization.branch_folded { 1 } else { 0 });
        buf.push(if obj.optimization.strength_reduced {
            1
        } else {
            0
        });
        buf.push(if obj.optimization.vectorized { 1 } else { 0 });

        Ok(buf)
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn write_opt_string(buf: &mut Vec<u8>, s: Option<&str>) {
    if let Some(val) = s {
        buf.push(1);
        write_string(buf, val);
    } else {
        buf.push(0);
    }
}
