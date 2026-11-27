//! Relocation model for ADOB.

use crate::symbol::SymbolId;

/// Universal architecture-neutral and architecture-specific relocation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
#[derive(Default)]
pub enum RelocationKind {
    // Universal Kinds
    Absolute64,
    Absolute32,
    Absolute16,
    Absolute8,
    #[default]
    PcRelative32,
    PcRelative64,
    PcRelative16,
    PltRelative32,
    GotRelative32,
    TlsGd,
    TlsLd,
    TlsIe,
    TlsLe,

    /// 32-bit image-base-relative address (PE RVA): `S + A - ImageBase`.
    ImageRelative32,

    // Intel 8086 Real-Mode Specific
    I8086_Segment16,

    // AVR (8-bit Harvard MCU) Specific
    Avr_Call22,
    Avr_Pcrel12,
    Avr_Lo8,
    Avr_Hi8,
    Avr_Addr16,

    // Motorola DSP56000 (24-bit DSP) Specific
    Dsp56k_Abs16,
    Dsp56k_Abs24,

    // x86_64 Specific
    X86_64_GotPcrel,
    X86_64_Plt32,
    X86_64_RexGotPcrelX,
    /// Mach-O `X86_64_RELOC_TLV` on `movq desc@TLV(%rip), %reg` (linker
    /// rewrites the opcode to `leaq` so the register holds the descriptor
    /// address for `callq *(%reg)` / `__tlv_bootstrap`).
    X86_64_TlvDescriptorRel32,

    // AArch64 Specific
    AArch64_Call26,
    AArch64_AdrPage21,
    AArch64_AddAbsLo12,
    AArch64_LdSt64Lo12,
    /// Mach-O `ARM64_RELOC_TLVP_LOAD_PAGEOFF12` on a 64-bit `ldr`; the linker
    /// rewrites it to `add` so the register holds the TLV descriptor address.
    AArch64_TlvDescriptorLo12,

    // RISC-V Specific
    RiscV_Call,
    RiscV_PcrelHi20,
    RiscV_PcrelLo12I,
    RiscV_PcrelLo12S,
    RiscV_GotHi20,
    RiscV_TlsGotHi20,
    RiscV_TlsGdHi20,
    RiscV_RvcBranch,
    RiscV_RvcJump,

    // WASM Specific
    WasmFunctionIndex,
    WasmTableIndex,
    WasmGlobalIndex,
    WasmTypeIndex,
    WasmMemoryAddress,

    // GPU / Embedded / Accelerator Custom
    Custom(u32),
}

/// Bitflags for relocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelocationFlags(pub u32);

impl RelocationFlags {
    pub const RELAXABLE: u32 = 1 << 0;
    pub const PLT_REQUIRED: u32 = 1 << 1;
    pub const GOT_REQUIRED: u32 = 1 << 2;
    pub const WEAK_REF: u32 = 1 << 3;

    pub fn is_relaxable(&self) -> bool {
        (self.0 & Self::RELAXABLE) != 0
    }
}

/// Relocation entry in an ADOB section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobRelocation {
    pub offset: u64,
    pub symbol: SymbolId,
    pub symbol_name: String,
    pub kind: RelocationKind,
    pub addend: i64,
    pub width: u8,
    pub flags: RelocationFlags,
}

impl AdobRelocation {
    pub fn new(
        offset: u64,
        symbol: SymbolId,
        symbol_name: impl Into<String>,
        kind: RelocationKind,
        addend: i64,
    ) -> Self {
        let width = match kind {
            // RiscV_Call patches an 8-byte AUIPC+JALR pair, not a single insn.
            RelocationKind::Absolute64
            | RelocationKind::PcRelative64
            | RelocationKind::RiscV_Call => 8,
            RelocationKind::Absolute32
            | RelocationKind::PcRelative32
            | RelocationKind::PltRelative32
            | RelocationKind::GotRelative32
            | RelocationKind::X86_64_GotPcrel
            | RelocationKind::X86_64_Plt32
            | RelocationKind::AArch64_Call26
            | RelocationKind::AArch64_AdrPage21 => 4,
            _ => 4,
        };

        Self {
            offset,
            symbol,
            symbol_name: symbol_name.into(),
            kind,
            addend,
            width,
            flags: RelocationFlags::default(),
        }
    }

    pub fn with_width(mut self, width: u8) -> Self {
        self.width = width;
        self
    }

    pub fn with_flags(mut self, flags: RelocationFlags) -> Self {
        self.flags = flags;
        self
    }
}
