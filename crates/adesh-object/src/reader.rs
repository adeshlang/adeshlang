//! ADOB Binary Reader / Deserializer with bounds checking and error handling.

use crate::error::{AdobError, AdobErrorCode, AdobResult};
use crate::extension::AdobExtension;
use crate::format::{ADOB_MAGIC, ADOB_VERSION_MAJOR, AdobHeader, AdobObject};
use crate::metadata::{
    BuildMetadata, MemoryOrderModel, OptimizationMetadata, SafetyMetadata, ThreadSafetyMetadata,
    TlsModel,
};
use crate::relocation::{AdobRelocation, RelocationFlags, RelocationKind};
use crate::section::{AdobSection, MemoryPermissions, MemoryRegion, SectionKind};
use crate::symbol::{AdobSymbol, SymbolBinding, SymbolKind, SymbolVisibility};
use crate::target::{
    Abi, Architecture, ComputeDevice, EmbeddedArchitecture, Endianness, Environment,
    GpuArchitecture, NpuArchitecture, OperatingSystem, PointerWidth, TargetDescriptor,
    TargetFeature, TargetFeatures, TpuArchitecture,
};

pub struct AdobReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> AdobReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn read_object(data: &'a [u8]) -> AdobResult<AdobObject> {
        let mut reader = Self::new(data);
        reader.parse()
    }

    fn ensure_bytes(&self, count: usize) -> AdobResult<()> {
        if self
            .offset
            .checked_add(count)
            .is_none_or(|end| end > self.data.len())
        {
            Err(AdobError::new(
                AdobErrorCode::BufferUnderflow,
                format!(
                    "Unexpected EOF: requested {} bytes at offset 0x{:X}, total length is {}",
                    count,
                    self.offset,
                    self.data.len()
                ),
            )
            .with_offset(self.offset))
        } else {
            Ok(())
        }
    }

    fn read_bytes(&mut self, count: usize) -> AdobResult<&'a [u8]> {
        self.ensure_bytes(count)?;
        let start = self.offset;
        self.offset += count;
        Ok(&self.data[start..self.offset])
    }

    fn read_u8(&mut self) -> AdobResult<u8> {
        let bytes = self.read_bytes(1)?;
        Ok(bytes[0])
    }

    fn read_u16_le(&mut self) -> AdobResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32_le(&mut self) -> AdobResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i64_le(&mut self) -> AdobResult<i64> {
        let bytes = self.read_bytes(8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_u64_le(&mut self) -> AdobResult<u64> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_string(&mut self) -> AdobResult<String> {
        let len = self.read_u32_le()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|e| {
            AdobError::new(
                AdobErrorCode::InvalidMetadata,
                format!(
                    "Invalid UTF-8 string at offset 0x{:X}: {}",
                    self.offset - len,
                    e
                ),
            )
            .with_offset(self.offset - len)
        })
    }

    fn read_opt_string(&mut self) -> AdobResult<Option<String>> {
        let has_str = self.read_u8()?;
        if has_str == 1 {
            Ok(Some(self.read_string()?))
        } else {
            Ok(None)
        }
    }

    pub fn parse(&mut self) -> AdobResult<AdobObject> {
        // 1. Header
        let magic = self.read_bytes(4)?;
        if magic != ADOB_MAGIC {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMagic,
                format!("Expected ADOB magic, found {:?}", magic),
            )
            .with_offset(0));
        }

        let major = self.read_u16_le()?;
        let minor = self.read_u16_le()?;
        let patch = self.read_u16_le()?;

        if major != ADOB_VERSION_MAJOR {
            return Err(AdobError::new(
                AdobErrorCode::UnsupportedVersion,
                format!(
                    "Unsupported ADOB major version {}. Expected {}",
                    major, ADOB_VERSION_MAJOR
                ),
            )
            .with_offset(4));
        }

        let flags = self.read_u32_le()?;

        // 2. Target Descriptor
        let dev_id = self.read_u8()?;
        let device = match dev_id {
            0 => ComputeDevice::Cpu,
            1 => ComputeDevice::Gpu,
            2 => ComputeDevice::Npu,
            3 => ComputeDevice::Tpu,
            4 => ComputeDevice::Dsp,
            5 => ComputeDevice::Fpga,
            6 => ComputeDevice::Accelerator,
            7 => ComputeDevice::Embedded,
            _ => ComputeDevice::Custom,
        };

        let arch_id = self.read_u16_le()?;
        let arch_custom = self.read_opt_string()?;
        let architecture = match arch_id {
            0 => Architecture::X86_64,
            1 => Architecture::X86,
            2 => Architecture::AArch64,
            3 => Architecture::Arm,
            4 => Architecture::RiscV64,
            5 => Architecture::RiscV32,
            6 => Architecture::PowerPc64,
            7 => Architecture::PowerPc,
            8 => Architecture::Mips,
            9 => Architecture::Wasm32,
            10 => Architecture::Wasm64,
            11 => Architecture::Gpu(GpuArchitecture::GenericSpirV),
            12 => Architecture::Npu(NpuArchitecture::GenericTensorNpu),
            13 => Architecture::Tpu(TpuArchitecture::GenericMatrixCore),
            14 => Architecture::Embedded(EmbeddedArchitecture::ArmCortexM4F),
            _ => Architecture::Custom(arch_custom.unwrap_or_else(|| "custom".to_string())),
        };

        let os_id = self.read_u8()?;
        let operating_system = match os_id {
            0 => OperatingSystem::None,
            1 => OperatingSystem::Windows,
            2 => OperatingSystem::Linux,
            3 => OperatingSystem::MacOS,
            4 => OperatingSystem::Ios,
            5 => OperatingSystem::Android,
            6 => OperatingSystem::FreeBsd,
            7 => OperatingSystem::NetBsd,
            8 => OperatingSystem::OpenBsd,
            9 => OperatingSystem::Wasi,
            10 => OperatingSystem::CudaRuntime,
            _ => OperatingSystem::Custom,
        };

        let env_id = self.read_u8()?;
        let environment = match env_id {
            0 => Environment::Msvc,
            1 => Environment::Gnu,
            2 => Environment::Musl,
            3 => Environment::Eabi,
            4 => Environment::Eabihf,
            5 => Environment::Android,
            6 => Environment::Sim,
            _ => Environment::Unknown,
        };

        let abi_id = self.read_u8()?;
        let abi = match abi_id {
            0 => Abi::Default,
            1 => Abi::SystemV,
            2 => Abi::WindowsX64,
            3 => Abi::Aapcs64,
            4 => Abi::Aapcs32,
            5 => Abi::RiscvAbi,
            6 => Abi::WasmAbi,
            7 => Abi::AdeshInternal,
            8 => Abi::BareMetal,
            9 => Abi::CudaKernelAbi,
            _ => Abi::Default,
        };

        let ptr_width_id = self.read_u8()?;
        let pointer_width = if ptr_width_id == 4 {
            PointerWidth::U32
        } else {
            PointerWidth::U64
        };

        let endian_id = self.read_u8()?;
        let endianness = if endian_id == 1 {
            Endianness::Big
        } else {
            Endianness::Little
        };

        let feature_count = self.read_u32_le()? as usize;
        let mut features = TargetFeatures::new();
        for _ in 0..feature_count {
            let f_name = self.read_string()?;
            let f_enabled = self.read_u8()? == 1;
            features
                .features
                .push(TargetFeature::new(f_name, f_enabled));
        }

        let target = TargetDescriptor {
            device,
            architecture: architecture.clone(),
            operating_system,
            environment,
            abi,
            object_format: crate::target::ObjectFormat::Adob,
            pointer_width,
            endianness,
            features,
        };

        let sec_count = self.read_u32_le()? as usize;
        let sym_count = self.read_u32_le()? as usize;
        let imp_count = self.read_u32_le()? as usize;
        let exp_count = self.read_u32_le()? as usize;
        let mr_count = self.read_u32_le()? as usize;
        let ext_count = self.read_u32_le()? as usize;

        let header = AdobHeader {
            magic: *ADOB_MAGIC,
            version_major: major,
            version_minor: minor,
            version_patch: patch,
            header_size: 64,
            flags,
            section_count: sec_count as u32,
            symbol_count: sym_count as u32,
            import_count: imp_count as u32,
            export_count: exp_count as u32,
            extension_count: ext_count as u32,
        };

        let mut obj = AdobObject::new(target);
        obj.header = header;

        // 4. Read Sections
        for _ in 0..sec_count {
            let name = self.read_string()?;
            let kind_id = self.read_u8()?;
            let kind = match kind_id {
                0 => SectionKind::Text,
                1 => SectionKind::Rodata,
                2 => SectionKind::Data,
                3 => SectionKind::Bss,
                4 => SectionKind::Tls,
                5 => SectionKind::Unwind,
                6 => SectionKind::Debug,
                7 => SectionKind::AdeshMeta,
                8 => SectionKind::Comdat,
                9 => SectionKind::Extension,
                _ => SectionKind::Custom,
            };
            let sec_flags = self.read_u32_le()?;
            let alignment = self.read_u64_le()?;
            let comdat = self.read_opt_string()?;
            let memory_region = self.read_opt_string()?;

            let data_len = self.read_u64_le()? as usize;
            let data_bytes = self.read_bytes(data_len)?.to_vec();

            let reloc_count = self.read_u32_le()? as usize;
            let mut relocations = Vec::with_capacity(reloc_count);
            for _ in 0..reloc_count {
                let r_offset = self.read_u64_le()?;
                let r_symbol = self.read_u32_le()?;
                let r_symbol_name = self.read_string()?;
                let r_kind_id = self.read_u32_le()?;
                let r_kind = match r_kind_id {
                    0 => RelocationKind::Absolute64,
                    1 => RelocationKind::Absolute32,
                    2 => RelocationKind::PcRelative32,
                    3 => RelocationKind::PcRelative64,
                    4 => RelocationKind::PltRelative32,
                    5 => RelocationKind::GotRelative32,
                    6 => RelocationKind::TlsGd,
                    7 => RelocationKind::TlsLd,
                    8 => RelocationKind::TlsIe,
                    9 => RelocationKind::TlsLe,
                    10 => RelocationKind::X86_64_GotPcrel,
                    11 => RelocationKind::X86_64_Plt32,
                    12 => RelocationKind::X86_64_RexGotPcrelX,
                    20 => RelocationKind::AArch64_Call26,
                    21 => RelocationKind::AArch64_AdrPage21,
                    22 => RelocationKind::AArch64_AddAbsLo12,
                    23 => RelocationKind::AArch64_LdSt64Lo12,
                    30 => RelocationKind::RiscV_Call,
                    31 => RelocationKind::RiscV_PcrelHi20,
                    32 => RelocationKind::RiscV_PcrelLo12I,
                    33 => RelocationKind::RiscV_PcrelLo12S,
                    34 => RelocationKind::RiscV_RvcBranch,
                    35 => RelocationKind::RiscV_RvcJump,
                    40 => RelocationKind::WasmFunctionIndex,
                    41 => RelocationKind::WasmTableIndex,
                    42 => RelocationKind::WasmGlobalIndex,
                    43 => RelocationKind::WasmTypeIndex,
                    44 => RelocationKind::WasmMemoryAddress,
                    custom if custom >= 1000 => RelocationKind::Custom(custom - 1000),
                    _ => RelocationKind::Absolute64,
                };
                let r_addend = self.read_i64_le()?;
                let r_width = self.read_u8()?;
                let r_flags = RelocationFlags(self.read_u32_le()?);

                let mut reloc =
                    AdobRelocation::new(r_offset, r_symbol, r_symbol_name, r_kind, r_addend);
                reloc.width = r_width;
                reloc.flags = r_flags;
                relocations.push(reloc);
            }

            let mut sec = AdobSection::new(name, kind)
                .with_flags(sec_flags)
                .with_alignment(alignment)
                .with_data(data_bytes);
            sec.relocations = relocations;
            if let Some(c) = comdat {
                sec.comdat_group = Some(c);
            }
            if let Some(mr) = memory_region {
                sec.memory_region = Some(mr);
            }

            obj.sections.push(sec);
        }

        // 5. Read Symbols
        for _ in 0..sym_count {
            let s_id = self.read_u32_le()?;
            let s_name = self.read_string()?;
            let b_id = self.read_u8()?;
            let binding = match b_id {
                0 => SymbolBinding::Local,
                1 => SymbolBinding::Global,
                2 => SymbolBinding::Weak,
                3 => SymbolBinding::Unique,
                _ => SymbolBinding::Global,
            };
            let v_id = self.read_u8()?;
            let visibility = match v_id {
                0 => SymbolVisibility::Default,
                1 => SymbolVisibility::Hidden,
                2 => SymbolVisibility::Protected,
                3 => SymbolVisibility::Internal,
                _ => SymbolVisibility::Default,
            };
            let k_id = self.read_u8()?;
            let kind = match k_id {
                0 => SymbolKind::Function,
                1 => SymbolKind::Object,
                2 => SymbolKind::Section,
                3 => SymbolKind::TLS,
                4 => SymbolKind::Import,
                5 => SymbolKind::Export,
                6 => SymbolKind::AcceleratorKernel,
                7 => SymbolKind::Runtime,
                _ => SymbolKind::Function,
            };
            let is_def = self.read_u8()? == 1;
            let sec_idx = self.read_u32_le()?;
            let s_value = self.read_u64_le()?;
            let s_size = self.read_u64_le()?;
            let s_version = self.read_opt_string()?;
            let s_alias = self.read_opt_string()?;

            let sym = AdobSymbol {
                id: s_id,
                name: s_name,
                binding,
                visibility,
                kind,
                is_defined: is_def,
                section_index: if sec_idx == u32::MAX {
                    None
                } else {
                    Some(sec_idx)
                },
                value: s_value,
                size: s_size,
                version: s_version,
                alias_target: s_alias,
            };
            obj.symbols.push(sym);
        }

        // 6. Read Imports & Exports
        for _ in 0..imp_count {
            obj.imports.push(self.read_string()?);
        }
        for _ in 0..exp_count {
            obj.exports.push(self.read_string()?);
        }

        // 7. Read Memory Regions
        for _ in 0..mr_count {
            let mr_name = self.read_string()?;
            let mr_addr = self.read_u64_le()?;
            let mr_size = self.read_u64_le()?;
            let p_id = self.read_u8()?;
            let permissions = match p_id {
                0 => MemoryPermissions::R,
                1 => MemoryPermissions::Rw,
                2 => MemoryPermissions::Rx,
                3 => MemoryPermissions::Rwx,
                _ => MemoryPermissions::Rx,
            };
            obj.memory_regions
                .push(MemoryRegion::new(mr_name, mr_addr, mr_size, permissions));
        }

        // 8. Read Extensions
        for _ in 0..ext_count {
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(self.read_bytes(16)?);
            let ext_major = self.read_u16_le()?;
            let ext_minor = self.read_u16_le()?;
            let ext_tag = self.read_string()?;
            let payload_len = self.read_u32_le()? as usize;
            let payload = self.read_bytes(payload_len)?.to_vec();
            obj.extensions.push(AdobExtension::new(
                uuid,
                (ext_major, ext_minor),
                ext_tag,
                payload,
            ));
        }

        // 9. Read Build Metadata
        if self.offset < self.data.len() {
            let comp_ver = self.read_string()?;
            let triple = self.read_string()?;
            let opt = self.read_u8()?;
            let det = self.read_u8()? == 1;
            obj.build_metadata = BuildMetadata {
                compiler_version: comp_ver,
                adob_version: major,
                target_triple: triple,
                opt_level: opt,
                feature_set: Vec::new(),
                source_hash: None,
                timestamp_utc: None,
                deterministic: det,
            };
        }

        // 10. Read Safety Metadata
        if self.offset < self.data.len() {
            let bounds = self.read_u8()? == 1;
            let overflow = self.read_u8()? == 1;
            let null_ptr = self.read_u8()? == 1;
            let msan = self.read_u8()? == 1;
            let asan = self.read_u8()? == 1;
            let tsan = self.read_u8()? == 1;
            let strict_prov = self.read_u8()? == 1;
            let isolated_heap = self.read_u8()? == 1;
            let stack_canary = self.read_u8()? == 1;
            obj.safety = SafetyMetadata {
                bounds_checking: bounds,
                overflow_checks: overflow,
                null_pointer_checks: null_ptr,
                memory_sanitizer: msan,
                address_sanitizer: asan,
                thread_sanitizer: tsan,
                strict_provenance: strict_prov,
                isolated_heap,
                stack_canary_present: stack_canary,
            };
        }

        // 11. Read Thread Safety Metadata
        if self.offset < self.data.len() {
            let mo = match self.read_u8()? {
                0 => MemoryOrderModel::Relaxed,
                1 => MemoryOrderModel::Acquire,
                2 => MemoryOrderModel::Release,
                3 => MemoryOrderModel::AcqRel,
                _ => MemoryOrderModel::SeqCst,
            };
            let tls = match self.read_u8()? {
                0 => TlsModel::GeneralDynamic,
                1 => TlsModel::LocalDynamic,
                2 => TlsModel::InitialExec,
                _ => TlsModel::LocalExec,
            };
            let atomic_align = self.read_u32_le()?;
            let lock_free = self.read_u8()? == 1;
            let data_race = self.read_u8()? == 1;
            obj.thread_safety = ThreadSafetyMetadata {
                default_memory_order: mo,
                tls_model: tls,
                atomic_alignment: atomic_align,
                lock_free_primitives: lock_free,
                data_race_detection: data_race,
            };
        }

        // 12. Read Optimization Metadata
        if self.offset < self.data.len() {
            let lto = self.read_u8()? == 1;
            let icf = self.read_u8()? == 1;
            let dce = self.read_u8()? == 1;
            let peep = self.read_u8()? == 1;
            let const_prop = self.read_u8()? == 1;
            let branch_fold = self.read_u8()? == 1;
            let strength_red = self.read_u8()? == 1;
            let vectorized = self.read_u8()? == 1;
            obj.optimization = OptimizationMetadata {
                lto_eligible: lto,
                icf_eligible: icf,
                dead_code_eliminated: dce,
                peephole_optimized: peep,
                constant_propagation: const_prop,
                branch_folded: branch_fold,
                strength_reduced: strength_red,
                vectorized,
            };
        }

        Ok(obj)
    }
}
