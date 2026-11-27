//! Heterogeneous Architecture Descriptors, Memory Models, Data Layouts, and Runtime Profiles.
//!
//! This module separates orthogonal target properties so the compiler and linker
//! never conflate CPU register width with object-container width (for example,
//! 8-bit AVR uses an ELF32 object container while remaining an 8-bit Harvard MCU)
//! or assume flat 64-bit byte-addressable memory across all targets.

use crate::target::{
    Architecture, EmbeddedArchitecture, Endianness, ObjectFormat, OperatingSystem, TargetDescriptor,
};

/// Instruction set architecture family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstructionSetArchitecture {
    X86_64,
    Ia32,
    I8086,
    Avr,
    Dsp56000,
    AArch64,
    Arm32,
    Thumb,
    RiscV64,
    RiscV32,
    Wasm32,
    Wasm64,
    Other(&'static str),
}

impl InstructionSetArchitecture {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86-64",
            Self::Ia32 => "IA-32 (x86-32)",
            Self::I8086 => "Intel 8086 (16-bit real mode)",
            Self::Avr => "AVR (8-bit RISC)",
            Self::Dsp56000 => "Motorola DSP56000 (24-bit fixed-point DSP)",
            Self::AArch64 => "AArch64",
            Self::Arm32 => "ARM32",
            Self::Thumb => "ARM Thumb",
            Self::RiscV64 => "RISC-V 64",
            Self::RiscV32 => "RISC-V 32",
            Self::Wasm32 => "WebAssembly 32",
            Self::Wasm64 => "WebAssembly 64",
            Self::Other(name) => name,
        }
    }
}

/// Target address-space and memory architecture model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryModelKind {
    /// Unified flat virtual/linear byte-addressed memory (x86-64, i686, AArch64, RV64).
    Flat,
    /// 16-bit segment:offset real-mode addressing producing 20-bit physical addresses (8086).
    SegmentedRealMode,
    /// Separate non-interchangeable Flash program memory and SRAM data memory (AVR).
    HarvardSeparate,
    /// Separate Program (P) and dual X/Y Data word-addressed memories (DSP56000).
    HarvardDualData,
    /// Sandboxed linear byte memory with separate code/table/function index spaces (Wasm).
    LinearWasm,
}

impl MemoryModelKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::SegmentedRealMode => "segmented-real-mode",
            Self::HarvardSeparate => "harvard-flash-sram",
            Self::HarvardDualData => "harvard-dual-xy-p",
            Self::LinearWasm => "linear-wasm",
        }
    }
}

/// Distinct logical/physical address spaces supported across architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    Default,
    ProgramFlash,
    DataSram,
    Eeprom,
    IoSpace,
    XData,
    YData,
    CodeSegment,
    DataSegment,
    StackSegment,
    ExtraSegment,
}

/// Register-file organization model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisterModelKind {
    FlatGpr,
    GprWithPairs,
    Segmented16,
    AccumulatorDsp,
    StackMachine,
}

impl RegisterModelKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlatGpr => "flat-gpr",
            Self::GprWithPairs => "8bit-gpr-with-pairs",
            Self::Segmented16 => "16bit-gpr-plus-segments",
            Self::AccumulatorDsp => "24bit-dsp-56bit-accumulators",
            Self::StackMachine => "operand-stack",
        }
    }
}

/// Executable/firmware image format produced by the linker for a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutableFormatKind {
    Elf64,
    Elf32,
    Pe32Plus,
    Pe32,
    MachO64,
    DosCom,
    DosMz,
    AvrElf32IntelHex,
    Dsp56kLod,
    WasmModule,
    None,
}

impl ExecutableFormatKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Elf64 => "elf64",
            Self::Elf32 => "elf32",
            Self::Pe32Plus => "pe32+",
            Self::Pe32 => "pe32",
            Self::MachO64 => "macho64",
            Self::DosCom => "dos-com",
            Self::DosMz => "dos-mz",
            Self::AvrElf32IntelHex => "avr-elf32+ihex",
            Self::Dsp56kLod => "dsp56k-lod",
            Self::WasmModule => "wasm",
            Self::None => "none",
        }
    }
}

/// Target runtime profile separating OS-hosted, minimal, bare-metal, and DSP runtimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeProfile {
    FullNative,
    MinimalNative,
    BareMetalEmbedded,
    SpecializedDsp,
    WasmHosted,
}

impl RuntimeProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FullNative => "Profile A: Full Native Runtime",
            Self::MinimalNative => "Profile B: Minimal Native Runtime (16-bit)",
            Self::BareMetalEmbedded => "Profile C: Bare-Metal Embedded Runtime",
            Self::SpecializedDsp => "Profile D: Specialized DSP Runtime",
            Self::WasmHosted => "WASM/WASI Hosted Runtime",
        }
    }
}

/// Explicit runtime feature support matrix for a target profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeCapabilities {
    pub profile: RuntimeProfile,
    pub allocation: bool,
    pub deallocation: bool,
    pub printing: bool,
    pub file_io: bool,
    pub networking: bool,
    pub threads: bool,
    pub atomics: bool,
    pub tls: bool,
    pub exceptions: bool,
    pub async_runtime: bool,
    pub time: bool,
    pub randomness: bool,
}

impl RuntimeCapabilities {
    pub const fn full_native() -> Self {
        Self {
            profile: RuntimeProfile::FullNative,
            allocation: true,
            deallocation: true,
            printing: true,
            file_io: true,
            networking: true,
            threads: true,
            atomics: true,
            tls: true,
            exceptions: true,
            async_runtime: true,
            time: true,
            randomness: true,
        }
    }

    pub const fn minimal_native_dos16() -> Self {
        Self {
            profile: RuntimeProfile::MinimalNative,
            allocation: false,
            deallocation: false,
            printing: true,
            file_io: false,
            networking: false,
            threads: false,
            atomics: false,
            tls: false,
            exceptions: false,
            async_runtime: false,
            time: false,
            randomness: false,
        }
    }

    /// Capabilities of the IA-32 (`i686-unknown-linux-gnu`) static C runtime
    /// (`native_output.c`): freestanding Linux syscalls for printing/abort
    /// and static single-thread Variant II TLS via `mmap2`/`set_thread_area`.
    /// Heap allocation, dynamic objects, threads, and Rust `std` helpers are
    /// not available in static ELF32 builds.
    pub const fn i686_freestanding_native() -> Self {
        Self {
            profile: RuntimeProfile::FullNative,
            allocation: false,
            deallocation: false,
            printing: true,
            file_io: false,
            networking: false,
            threads: false,
            atomics: false,
            tls: true,
            exceptions: false,
            async_runtime: false,
            time: false,
            randomness: false,
        }
    }

    pub const fn bare_metal_embedded() -> Self {
        Self {
            profile: RuntimeProfile::BareMetalEmbedded,
            allocation: false,
            deallocation: false,
            printing: true, // Explicit UART/serial board abstraction only
            file_io: false,
            networking: false,
            threads: false,
            atomics: false,
            tls: false,
            exceptions: false,
            async_runtime: false,
            time: false,
            randomness: false,
        }
    }

    pub const fn specialized_dsp() -> Self {
        Self {
            profile: RuntimeProfile::SpecializedDsp,
            allocation: false,
            deallocation: false,
            printing: true, // Explicit memory-mapped SCI TX (X:$FFE0) abstraction
            file_io: false,
            networking: false,
            threads: false,
            atomics: false,
            tls: false,
            exceptions: false,
            async_runtime: false,
            time: false,
            randomness: false,
        }
    }

    pub const fn wasm_hosted() -> Self {
        Self {
            profile: RuntimeProfile::WasmHosted,
            allocation: true,
            deallocation: true,
            printing: true,
            file_io: false,
            networking: false,
            threads: false,
            atomics: false,
            tls: false,
            exceptions: false,
            async_runtime: false,
            time: false,
            randomness: false,
        }
    }

    pub const fn supports_feature(&self, feature: &str) -> Option<bool> {
        // Const-compatible byte comparison so callers can query by capability key.
        let b = feature.as_bytes();
        match b {
            b"allocation" => Some(self.allocation),
            b"deallocation" => Some(self.deallocation),
            b"printing" => Some(self.printing),
            b"file_io" | b"files" => Some(self.file_io),
            b"networking" => Some(self.networking),
            b"threads" => Some(self.threads),
            b"atomics" => Some(self.atomics),
            b"tls" => Some(self.tls),
            b"exceptions" => Some(self.exceptions),
            b"async" | b"async_runtime" => Some(self.async_runtime),
            b"time" => Some(self.time),
            b"randomness" => Some(self.randomness),
            _ => None,
        }
    }
}

/// Computed structure layout under a target [`DataLayoutSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructLayout {
    pub size_units: u64,
    pub align_units: u16,
    pub field_offsets_units: Vec<u64>,
}

/// Architecture-aware type, pointer, alignment, and memory layout specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataLayoutSpec {
    pub register_width_bits: u16,
    pub pointer_width_bits: u16,
    pub near_pointer_bits: u16,
    pub far_pointer_bits: Option<u16>,
    pub usize_bits: u16,
    pub address_width_bits: u16,
    pub data_bus_width_bits: u16,
    pub native_word_bits: u16,
    pub max_addressable_bytes: u64,
    pub byte_addressable: bool,
    /// Minimum addressable storage unit in bits (`8` for byte-addressed targets, `24` for DSP56000).
    pub min_addressable_unit_bits: u8,
    pub unaligned_access_allowed: bool,
    pub endianness: Endianness,
    pub i8_align: u16,
    pub i16_align: u16,
    pub i24_align: u16,
    pub i32_align: u16,
    pub i64_align: u16,
    pub f32_align: u16,
    pub f64_align: u16,
    pub pointer_align: u16,
    pub aggregate_align: u16,
    pub stack_growth_downward: bool,
    pub natural_stack_alignment_bytes: u16,
}

impl DataLayoutSpec {
    /// Size in addressable units (`bytes` on byte-addressed targets, `24-bit words` on DSP56000).
    pub const fn size_of_scalar_units(&self, bits: u16) -> u64 {
        let unit = self.min_addressable_unit_bits as u64;
        (bits as u64).div_ceil(unit)
    }

    pub const fn align_of_scalar_units(&self, bits: u16) -> u16 {
        match bits {
            0..=8 => self.i8_align,
            9..=16 => self.i16_align,
            17..=24 => self.i24_align,
            25..=32 => self.i32_align,
            _ => self.i64_align,
        }
    }

    pub const fn size_of_float_units(&self, bits: u16) -> u64 {
        let unit = self.min_addressable_unit_bits as u64;
        (bits as u64).div_ceil(unit)
    }

    pub const fn align_of_float_units(&self, bits: u16) -> u16 {
        if bits <= 32 {
            self.f32_align
        } else {
            self.f64_align
        }
    }

    pub const fn size_of_pointer_units(&self) -> u64 {
        let unit = self.min_addressable_unit_bits as u64;
        (self.pointer_width_bits as u64).div_ceil(unit)
    }

    pub const fn align_of_pointer_units(&self) -> u16 {
        self.pointer_align
    }

    pub const fn size_of_usize_units(&self) -> u64 {
        let unit = self.min_addressable_unit_bits as u64;
        (self.usize_bits as u64).div_ceil(unit)
    }

    /// Compute C-ABI struct layout in target addressable units.
    pub fn layout_struct(&self, fields: &[(u64, u16)], packed: bool) -> StructLayout {
        let mut offset = 0u64;
        let mut max_align = self.aggregate_align.max(1);
        let mut offsets = Vec::with_capacity(fields.len());

        for &(field_size, field_align) in fields {
            let effective_align = if packed { 1 } else { field_align.max(1) as u64 };
            let rem = offset % effective_align;
            if rem != 0 {
                offset += effective_align - rem;
            }
            offsets.push(offset);
            offset += field_size;
            if !packed {
                max_align = max_align.max(field_align.max(1));
            }
        }

        let align_u64 = max_align as u64;
        let tail = offset % align_u64;
        if tail != 0 {
            offset += align_u64 - tail;
        }

        StructLayout {
            size_units: offset,
            align_units: max_align,
            field_offsets_units: offsets,
        }
    }

    /// Compute array `(total_size_units, align_units)` respecting element stride alignment.
    pub const fn layout_array(&self, elem_size: u64, elem_align: u16, count: u64) -> (u64, u16) {
        let align = if elem_align == 0 { 1 } else { elem_align };
        let align_u64 = align as u64;
        let rem = elem_size % align_u64;
        let stride = if rem == 0 {
            elem_size
        } else {
            elem_size + (align_u64 - rem)
        };
        (stride * count, align)
    }
}

/// Complete, immutable target architecture descriptor separating ISA, widths,
/// register model, memory model, calling conventions, container/executable formats,
/// data layout, and runtime profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureDescriptor {
    pub architecture_id: &'static str,
    pub architecture_name: &'static str,
    pub isa: InstructionSetArchitecture,
    pub register_width_bits: u16,
    pub pointer_width_bits: u16,
    pub address_width_bits: u16,
    pub data_width_bits: u16,
    /// `(min_bits, max_bits)` per machine instruction.
    pub instruction_width_bits: (u16, u16),
    pub native_word_bits: u16,
    pub max_addressable_bytes: u64,
    pub endianness: Endianness,
    pub register_model: RegisterModelKind,
    pub gpr_count: u8,
    pub special_registers: &'static [&'static str],
    pub memory_model: MemoryModelKind,
    pub address_spaces: &'static [AddressSpace],
    pub calling_conventions: &'static [&'static str],
    pub supported_data_types: &'static [&'static str],
    pub relocation_types: &'static [&'static str],
    pub object_formats: &'static [ObjectFormat],
    pub executable_formats: &'static [ExecutableFormatKind],
    pub runtime_profile: RuntimeProfile,
    pub runtime_capabilities: RuntimeCapabilities,
    pub data_layout: DataLayoutSpec,
    pub hardware_multiply: bool,
    pub hardware_divide: bool,
    pub hardware_float: bool,
}

static X86_64_LINUX_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "x86_64",
    architecture_name: "AMD64 / Intel 64",
    isa: InstructionSetArchitecture::X86_64,
    register_width_bits: 64,
    pointer_width_bits: 64,
    address_width_bits: 48,
    data_width_bits: 64,
    instruction_width_bits: (8, 120),
    native_word_bits: 64,
    max_addressable_bytes: 1u64 << 48,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::FlatGpr,
    gpr_count: 16,
    special_registers: &["RIP", "RSP", "RBP", "RFLAGS", "FS", "GS"],
    memory_model: MemoryModelKind::Flat,
    address_spaces: &[AddressSpace::Default],
    calling_conventions: &["sysv64", "win64"],
    supported_data_types: &[
        "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64", "ptr",
    ],
    relocation_types: &[
        "R_X86_64_64",
        "R_X86_64_PC32",
        "R_X86_64_PLT32",
        "R_X86_64_GOTPCREL",
        "R_X86_64_REX_GOTPCRELX",
        "R_X86_64_TPOFF32",
        "R_X86_64_GOTTPOFF",
        "R_X86_64_TLSGD",
    ],
    object_formats: &[
        ObjectFormat::Adob,
        ObjectFormat::Elf,
        ObjectFormat::PeCoff,
        ObjectFormat::MachO,
    ],
    executable_formats: &[
        ExecutableFormatKind::Elf64,
        ExecutableFormatKind::Pe32Plus,
        ExecutableFormatKind::MachO64,
    ],
    runtime_profile: RuntimeProfile::FullNative,
    runtime_capabilities: RuntimeCapabilities::full_native(),
    data_layout: DataLayoutSpec {
        register_width_bits: 64,
        pointer_width_bits: 64,
        near_pointer_bits: 64,
        far_pointer_bits: None,
        usize_bits: 64,
        address_width_bits: 48,
        data_bus_width_bits: 64,
        native_word_bits: 64,
        max_addressable_bytes: 1u64 << 48,
        byte_addressable: true,
        min_addressable_unit_bits: 8,
        unaligned_access_allowed: true,
        endianness: Endianness::Little,
        i8_align: 1,
        i16_align: 2,
        i24_align: 4,
        i32_align: 4,
        i64_align: 8,
        f32_align: 4,
        f64_align: 8,
        pointer_align: 8,
        aggregate_align: 1,
        stack_growth_downward: true,
        natural_stack_alignment_bytes: 16,
    },
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: true,
};

static I686_LINUX_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "i686",
    architecture_name: "Intel IA-32 (i686)",
    isa: InstructionSetArchitecture::Ia32,
    register_width_bits: 32,
    pointer_width_bits: 32,
    address_width_bits: 32,
    data_width_bits: 32,
    instruction_width_bits: (8, 120),
    native_word_bits: 32,
    max_addressable_bytes: 1u64 << 32,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::FlatGpr,
    gpr_count: 8,
    special_registers: &["EIP", "ESP", "EBP", "EFLAGS"],
    memory_model: MemoryModelKind::Flat,
    address_spaces: &[AddressSpace::Default],
    calling_conventions: &["cdecl-sysv-i386", "win32-cdecl"],
    supported_data_types: &["i8", "u8", "i16", "u16", "i32", "u32", "f64", "ptr"],
    relocation_types: &[
        "R_386_32",
        "R_386_PC32",
        "R_386_PLT32",
        "R_386_GOT32",
        "R_386_GOT32X",
        "R_386_GOTOFF",
        "R_386_GOTPC",
        "R_386_TLS_LE",
        "R_386_16",
        "R_386_8",
    ],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::Elf],
    executable_formats: &[ExecutableFormatKind::Elf32],
    runtime_profile: RuntimeProfile::FullNative,
    runtime_capabilities: RuntimeCapabilities::i686_freestanding_native(),
    data_layout: DataLayoutSpec {
        register_width_bits: 32,
        pointer_width_bits: 32,
        near_pointer_bits: 32,
        far_pointer_bits: None,
        usize_bits: 32,
        address_width_bits: 32,
        data_bus_width_bits: 32,
        native_word_bits: 32,
        max_addressable_bytes: 1u64 << 32,
        byte_addressable: true,
        min_addressable_unit_bits: 8,
        unaligned_access_allowed: true,
        endianness: Endianness::Little,
        i8_align: 1,
        i16_align: 2,
        i24_align: 4,
        i32_align: 4,
        i64_align: 4,
        f32_align: 4,
        f64_align: 4,
        pointer_align: 4,
        aggregate_align: 1,
        stack_growth_downward: true,
        natural_stack_alignment_bytes: 16,
    },
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: true,
};

static I8086_DOS_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "i8086",
    architecture_name: "Intel 8086 (16-bit Real Mode)",
    isa: InstructionSetArchitecture::I8086,
    register_width_bits: 16,
    pointer_width_bits: 16,
    address_width_bits: 20,
    data_width_bits: 16,
    instruction_width_bits: (8, 48),
    native_word_bits: 16,
    max_addressable_bytes: 1u64 << 20,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::Segmented16,
    gpr_count: 8,
    special_registers: &["IP", "SP", "BP", "FLAGS", "CS", "DS", "SS", "ES"],
    memory_model: MemoryModelKind::SegmentedRealMode,
    address_spaces: &[
        AddressSpace::CodeSegment,
        AddressSpace::DataSegment,
        AddressSpace::StackSegment,
        AddressSpace::ExtraSegment,
    ],
    calling_conventions: &["dos16-cdecl-near", "dos16-pascal-far"],
    supported_data_types: &[
        "i8", "u8", "i16", "u16", "i32", "u32", "near-ptr", "far-ptr",
    ],
    relocation_types: &[
        "R_8086_ABS16",
        "R_8086_REL16",
        "R_8086_SEG16",
        "R_8086_FAR32",
    ],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::RawBinary],
    executable_formats: &[ExecutableFormatKind::DosCom, ExecutableFormatKind::DosMz],
    runtime_profile: RuntimeProfile::MinimalNative,
    runtime_capabilities: RuntimeCapabilities::minimal_native_dos16(),
    data_layout: DataLayoutSpec {
        register_width_bits: 16,
        pointer_width_bits: 16,
        near_pointer_bits: 16,
        far_pointer_bits: Some(32),
        usize_bits: 16,
        address_width_bits: 20,
        data_bus_width_bits: 16,
        native_word_bits: 16,
        max_addressable_bytes: 1u64 << 20,
        byte_addressable: true,
        min_addressable_unit_bits: 8,
        unaligned_access_allowed: true,
        endianness: Endianness::Little,
        i8_align: 1,
        i16_align: 2,
        i24_align: 2,
        i32_align: 2,
        i64_align: 2,
        f32_align: 2,
        f64_align: 2,
        pointer_align: 2,
        aggregate_align: 1,
        stack_growth_downward: true,
        natural_stack_alignment_bytes: 2,
    },
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: false,
};

static AVR_ATMEGA328P_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "avr",
    architecture_name: "AVR ATmega328P (8-bit Harvard MCU)",
    isa: InstructionSetArchitecture::Avr,
    register_width_bits: 8,
    pointer_width_bits: 16,
    address_width_bits: 16,
    data_width_bits: 8,
    instruction_width_bits: (16, 32),
    native_word_bits: 8,
    max_addressable_bytes: 32 * 1024, // 32 KiB Flash; 2 KiB SRAM; 1 KiB EEPROM
    endianness: Endianness::Little,
    register_model: RegisterModelKind::GprWithPairs,
    gpr_count: 32,
    special_registers: &[
        "PC",
        "SP (SPL/SPH)",
        "SREG",
        "X (R26:R27)",
        "Y (R28:R29)",
        "Z (R30:R31)",
    ],
    memory_model: MemoryModelKind::HarvardSeparate,
    address_spaces: &[
        AddressSpace::ProgramFlash,
        AddressSpace::DataSram,
        AddressSpace::Eeprom,
        AddressSpace::IoSpace,
    ],
    calling_conventions: &["avr-gcc-abi"],
    supported_data_types: &[
        "i8",
        "u8",
        "i16",
        "u16",
        "i32",
        "u32",
        "sram-ptr",
        "progmem-ptr",
    ],
    relocation_types: &[
        "R_AVR_16",
        "R_AVR_16_PM",
        "R_AVR_7_PCREL",
        "R_AVR_13_PCREL",
        "R_AVR_CALL",
        "R_AVR_LO8_LDI",
        "R_AVR_HI8_LDI",
        "R_AVR_LO8_LDI_PM",
        "R_AVR_HI8_LDI_PM",
    ],
    // Note: AVR uses an ELF32 container even though the CPU is an 8-bit MCU.
    object_formats: &[ObjectFormat::Adob, ObjectFormat::Elf],
    executable_formats: &[ExecutableFormatKind::AvrElf32IntelHex],
    runtime_profile: RuntimeProfile::BareMetalEmbedded,
    runtime_capabilities: RuntimeCapabilities::bare_metal_embedded(),
    data_layout: DataLayoutSpec {
        register_width_bits: 8,
        pointer_width_bits: 16,
        near_pointer_bits: 16,
        far_pointer_bits: None,
        usize_bits: 16,
        address_width_bits: 16,
        data_bus_width_bits: 8,
        native_word_bits: 8,
        max_addressable_bytes: 32 * 1024,
        byte_addressable: true,
        min_addressable_unit_bits: 8,
        unaligned_access_allowed: true,
        endianness: Endianness::Little,
        i8_align: 1,
        i16_align: 1,
        i24_align: 1,
        i32_align: 1,
        i64_align: 1,
        f32_align: 1,
        f64_align: 1,
        pointer_align: 1,
        aggregate_align: 1,
        stack_growth_downward: true,
        natural_stack_alignment_bytes: 1,
    },
    hardware_multiply: true,
    hardware_divide: false,
    hardware_float: false,
};

static DSP56000_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "dsp56000",
    architecture_name: "Motorola DSP56000 (24-bit Fixed-Point DSP)",
    isa: InstructionSetArchitecture::Dsp56000,
    register_width_bits: 24,
    pointer_width_bits: 16,
    address_width_bits: 16,
    data_width_bits: 24,
    instruction_width_bits: (24, 48),
    native_word_bits: 24,
    // 64K x 24-bit words per X/Y/P space = 192K bytes per space
    max_addressable_bytes: 65536 * 3,
    endianness: Endianness::Big,
    register_model: RegisterModelKind::AccumulatorDsp,
    gpr_count: 16,
    special_registers: &[
        "A (56-bit)",
        "B (56-bit)",
        "X0/X1",
        "Y0/Y1",
        "R0..R7",
        "N0..N7",
        "M0..M7",
        "PC",
        "SR",
        "LA",
        "LC",
    ],
    memory_model: MemoryModelKind::HarvardDualData,
    address_spaces: &[
        AddressSpace::ProgramFlash,
        AddressSpace::XData,
        AddressSpace::YData,
    ],
    calling_conventions: &["dsp56k-reg-stack"],
    supported_data_types: &["i24", "u24", "i48", "i56-acc"],
    relocation_types: &["R_DSP56K_ABS16", "R_DSP56K_IMM24", "R_DSP56K_PCREL16"],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::RawBinary],
    executable_formats: &[ExecutableFormatKind::Dsp56kLod],
    runtime_profile: RuntimeProfile::SpecializedDsp,
    runtime_capabilities: RuntimeCapabilities::specialized_dsp(),
    data_layout: DataLayoutSpec {
        register_width_bits: 24,
        pointer_width_bits: 16,
        near_pointer_bits: 16,
        far_pointer_bits: None,
        usize_bits: 16,
        address_width_bits: 16,
        data_bus_width_bits: 24,
        native_word_bits: 24,
        max_addressable_bytes: 65536 * 3,
        byte_addressable: false,
        min_addressable_unit_bits: 24,
        unaligned_access_allowed: false,
        endianness: Endianness::Big,
        i8_align: 1,
        i16_align: 1,
        i24_align: 1,
        i32_align: 2,
        i64_align: 3,
        f32_align: 2,
        f64_align: 3,
        pointer_align: 1,
        aggregate_align: 1,
        stack_growth_downward: false, // Hardware/software stack grows upward on DSP56000
        natural_stack_alignment_bytes: 3,
    },
    hardware_multiply: true,
    hardware_divide: false,
    hardware_float: false,
};

static AARCH64_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "aarch64",
    architecture_name: "ARM64 (AArch64)",
    isa: InstructionSetArchitecture::AArch64,
    register_width_bits: 64,
    pointer_width_bits: 64,
    address_width_bits: 48,
    data_width_bits: 64,
    instruction_width_bits: (32, 32),
    native_word_bits: 64,
    max_addressable_bytes: 1u64 << 48,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::FlatGpr,
    gpr_count: 31,
    special_registers: &["PC", "SP", "X29 (FP)", "X30 (LR)", "XZR", "NZCV"],
    memory_model: MemoryModelKind::Flat,
    address_spaces: &[AddressSpace::Default],
    calling_conventions: &["aapcs64", "aapcs64-darwin"],
    supported_data_types: &[
        "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64", "ptr",
    ],
    relocation_types: &[
        "R_AARCH64_CALL26",
        "R_AARCH64_ADR_PREL_PG_HI21",
        "R_AARCH64_ADD_ABS_LO12_NC",
        "R_AARCH64_LDST64_ABS_LO12_NC",
    ],
    object_formats: &[
        ObjectFormat::Adob,
        ObjectFormat::Elf,
        ObjectFormat::MachO,
        ObjectFormat::PeCoff,
    ],
    executable_formats: &[
        ExecutableFormatKind::Elf64,
        ExecutableFormatKind::MachO64,
        ExecutableFormatKind::Pe32Plus,
    ],
    runtime_profile: RuntimeProfile::FullNative,
    runtime_capabilities: RuntimeCapabilities::full_native(),
    data_layout: X86_64_LINUX_DESC.data_layout,
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: true,
};

static RISCV64_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "riscv64",
    architecture_name: "RISC-V 64 (RV64GC)",
    isa: InstructionSetArchitecture::RiscV64,
    register_width_bits: 64,
    pointer_width_bits: 64,
    address_width_bits: 48,
    data_width_bits: 64,
    instruction_width_bits: (16, 32),
    native_word_bits: 64,
    max_addressable_bytes: 1u64 << 48,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::FlatGpr,
    gpr_count: 32,
    special_registers: &["pc", "sp (x2)", "ra (x1)", "s0/fp (x8)", "tp (x4)"],
    memory_model: MemoryModelKind::Flat,
    address_spaces: &[AddressSpace::Default],
    calling_conventions: &["lp64d"],
    supported_data_types: &[
        "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64", "ptr",
    ],
    relocation_types: &[
        "R_RISCV_CALL_PLT",
        "R_RISCV_PCREL_HI20",
        "R_RISCV_PCREL_LO12_I",
        "R_RISCV_PCREL_LO12_S",
    ],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::Elf],
    executable_formats: &[ExecutableFormatKind::Elf64],
    runtime_profile: RuntimeProfile::FullNative,
    runtime_capabilities: RuntimeCapabilities::full_native(),
    data_layout: X86_64_LINUX_DESC.data_layout,
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: true,
};

static WASM32_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "wasm32",
    architecture_name: "WebAssembly 32",
    isa: InstructionSetArchitecture::Wasm32,
    register_width_bits: 32,
    pointer_width_bits: 32,
    address_width_bits: 32,
    data_width_bits: 32,
    instruction_width_bits: (8, 80),
    native_word_bits: 32,
    max_addressable_bytes: 1u64 << 32,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::StackMachine,
    gpr_count: 0,
    special_registers: &["operand-stack", "locals", "globals"],
    memory_model: MemoryModelKind::LinearWasm,
    address_spaces: &[AddressSpace::Default],
    calling_conventions: &["wasm-c-abi"],
    supported_data_types: &["i32", "i64", "f32", "f64", "ptr"],
    relocation_types: &[
        "R_WASM_FUNCTION_INDEX_LEB",
        "R_WASM_MEMORY_ADDR_SLEB",
        "R_WASM_TABLE_INDEX_SLEB",
    ],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::Wasm],
    executable_formats: &[ExecutableFormatKind::WasmModule],
    runtime_profile: RuntimeProfile::WasmHosted,
    runtime_capabilities: RuntimeCapabilities::wasm_hosted(),
    data_layout: I686_LINUX_DESC.data_layout,
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: true,
};

static GENERIC_EMBEDDED_32_DESC: ArchitectureDescriptor = ArchitectureDescriptor {
    architecture_id: "embedded32",
    architecture_name: "32-bit Embedded MCU",
    isa: InstructionSetArchitecture::Thumb,
    register_width_bits: 32,
    pointer_width_bits: 32,
    address_width_bits: 32,
    data_width_bits: 32,
    instruction_width_bits: (16, 32),
    native_word_bits: 32,
    max_addressable_bytes: 1u64 << 32,
    endianness: Endianness::Little,
    register_model: RegisterModelKind::FlatGpr,
    gpr_count: 16,
    special_registers: &["PC", "SP", "LR", "xPSR"],
    memory_model: MemoryModelKind::Flat,
    address_spaces: &[AddressSpace::ProgramFlash, AddressSpace::DataSram],
    calling_conventions: &["aapcs32"],
    supported_data_types: &["i8", "u8", "i16", "u16", "i32", "u32", "ptr"],
    relocation_types: &["R_ARM_THM_CALL", "R_ARM_ABS32"],
    object_formats: &[ObjectFormat::Adob, ObjectFormat::Elf],
    executable_formats: &[ExecutableFormatKind::Elf32],
    runtime_profile: RuntimeProfile::BareMetalEmbedded,
    runtime_capabilities: RuntimeCapabilities::bare_metal_embedded(),
    data_layout: DataLayoutSpec {
        natural_stack_alignment_bytes: 8,
        ..I686_LINUX_DESC.data_layout
    },
    hardware_multiply: true,
    hardware_divide: true,
    hardware_float: false,
};

impl ArchitectureDescriptor {
    /// Zero-allocation static descriptor lookup by [`Architecture`].
    pub fn for_architecture(arch: &Architecture) -> &'static Self {
        match arch {
            Architecture::X86_64 | Architecture::Wasm64 => &X86_64_LINUX_DESC,
            Architecture::X86 => &I686_LINUX_DESC,
            Architecture::I8086 => &I8086_DOS_DESC,
            Architecture::Embedded(EmbeddedArchitecture::Avr) => &AVR_ATMEGA328P_DESC,
            Architecture::Dsp56000 => &DSP56000_DESC,
            Architecture::AArch64 => &AARCH64_DESC,
            Architecture::RiscV64 => &RISCV64_DESC,
            Architecture::Wasm32 => &WASM32_DESC,
            Architecture::Embedded(_) | Architecture::Arm | Architecture::RiscV32 => {
                &GENERIC_EMBEDDED_32_DESC
            }
            _ => &X86_64_LINUX_DESC,
        }
    }

    /// Descriptor lookup refined by full [`TargetDescriptor`] (OS/ABI/format).
    pub fn for_target(target: &TargetDescriptor) -> &'static Self {
        match (&target.architecture, target.operating_system) {
            (Architecture::I8086, _) | (_, OperatingSystem::Dos) => &I8086_DOS_DESC,
            (Architecture::Embedded(EmbeddedArchitecture::Avr), _) => &AVR_ATMEGA328P_DESC,
            (Architecture::Dsp56000, _) => &DSP56000_DESC,
            (Architecture::X86, _) => &I686_LINUX_DESC,
            (Architecture::X86_64, _) => &X86_64_LINUX_DESC,
            (Architecture::AArch64, _) => &AARCH64_DESC,
            (Architecture::RiscV64, _) => &RISCV64_DESC,
            (Architecture::Wasm32, _) => &WASM32_DESC,
            _ => Self::for_architecture(&target.architecture),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_never_conflate_object_container_with_cpu_width() {
        let avr = TargetDescriptor::from_triple("avr-unknown-unknown").unwrap();
        let avr_desc = avr.arch_descriptor();
        assert_eq!(avr_desc.register_width_bits, 8);
        assert_eq!(avr_desc.pointer_width_bits, 16);
        assert_eq!(avr_desc.memory_model, MemoryModelKind::HarvardSeparate);
        assert!(avr_desc.object_formats.contains(&ObjectFormat::Elf));
        assert_eq!(avr_desc.runtime_profile, RuntimeProfile::BareMetalEmbedded);
        assert!(!avr_desc.runtime_capabilities.threads);
        assert!(!avr_desc.runtime_capabilities.file_io);

        let dos16 = TargetDescriptor::from_triple("i8086-unknown-dos").unwrap();
        let dos_desc = dos16.arch_descriptor();
        assert_eq!(dos_desc.register_width_bits, 16);
        assert_eq!(dos_desc.address_width_bits, 20);
        assert_eq!(dos_desc.memory_model, MemoryModelKind::SegmentedRealMode);
        assert_eq!(dos_desc.data_layout.far_pointer_bits, Some(32));

        let dsp = TargetDescriptor::from_triple("dsp56000-unknown-unknown").unwrap();
        let dsp_desc = dsp.arch_descriptor();
        assert_eq!(dsp_desc.register_width_bits, 24);
        assert_eq!(dsp_desc.data_width_bits, 24);
        assert!(!dsp_desc.data_layout.byte_addressable);
        assert_eq!(dsp_desc.data_layout.min_addressable_unit_bits, 24);
        assert_eq!(dsp_desc.memory_model, MemoryModelKind::HarvardDualData);
    }

    #[test]
    fn target_aware_struct_and_array_layouts_respect_width_and_alignment() {
        // 64-bit x86_64: { u8, ptr(8), u32 } -> 1 (+7 pad) + 8 + 4 (+4 pad) = 24 bytes, align 8
        let l64 = ArchitectureDescriptor::for_architecture(&Architecture::X86_64)
            .data_layout
            .layout_struct(&[(1, 1), (8, 8), (4, 4)], false);
        assert_eq!(l64.size_units, 24);
        assert_eq!(l64.align_units, 8);
        assert_eq!(l64.field_offsets_units, vec![0, 8, 16]);

        // 32-bit i686: { u8, ptr(4), u32 } -> 1 (+3 pad) + 4 + 4 = 12 bytes, align 4
        let l32 = ArchitectureDescriptor::for_architecture(&Architecture::X86)
            .data_layout
            .layout_struct(&[(1, 1), (4, 4), (4, 4)], false);
        assert_eq!(l32.size_units, 12);
        assert_eq!(l32.align_units, 4);

        // 16-bit 8086: { u8, near_ptr(2), u32(4) } -> 1 (+1 pad) + 2 + 4 = 8 bytes, align 2
        let l16 = ArchitectureDescriptor::for_architecture(&Architecture::I8086)
            .data_layout
            .layout_struct(&[(1, 1), (2, 2), (4, 2)], false);
        assert_eq!(l16.size_units, 8);
        assert_eq!(l16.align_units, 2);

        // 8-bit AVR: { u8, ptr(2), u32(4) } -> 1 + 2 + 4 = 7 bytes, align 1 (no padding!)
        let l8 = ArchitectureDescriptor::for_architecture(&Architecture::Embedded(
            EmbeddedArchitecture::Avr,
        ))
        .data_layout
        .layout_struct(&[(1, 1), (2, 1), (4, 1)], false);
        assert_eq!(l8.size_units, 7);
        assert_eq!(l8.align_units, 1);
        assert_eq!(l8.field_offsets_units, vec![0, 1, 3]);

        // 24-bit DSP56000: word-addressed (24-bit words): { i24(1 word), ptr(1 word), i48(2 words) }
        let l24 = ArchitectureDescriptor::for_architecture(&Architecture::Dsp56000)
            .data_layout
            .layout_struct(&[(1, 1), (1, 1), (2, 1)], false);
        assert_eq!(l24.size_units, 4);
        assert_eq!(l24.align_units, 1);
    }
}
