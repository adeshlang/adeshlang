//! Relocation records, relocation types, and relocation handler traits.

use crate::error::{ErrorCode, LinkError, LinkResult};
use std::fmt;

/// Architecture-independent and architecture-specific relocation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelocationKind {
    /// 64-bit absolute address: `S + A`
    Absolute64,
    /// 32-bit absolute address: `(S + A) as u32`
    Absolute32,
    /// 16-bit absolute address: `(S + A) as u16`
    Absolute16,
    /// 8-bit absolute address: `(S + A) as u8`
    Absolute8,

    /// 32-bit PC-relative address: `S + A - P`
    PcRelative32,
    /// 64-bit PC-relative address: `S + A - P`
    PcRelative64,

    /// 32-bit PLT entry relative offset: `L + A - P`
    PltRelative32,
    /// 32-bit GOT entry relative offset: `G + A - P`
    GotRelative32,
    /// 64-bit GOT entry offset: `G + A`
    Got64,

    /// 32-bit offset relative to section base: `S + A - SectionBase`
    SectionRelative32,
    /// 32-bit image-base-relative address (RVA): `S + A - ImageBase`
    ImageRelative32,

    /// TLS General Dynamic / Initial Exec / Local Exec
    TlsGeneralDynamic,
    TlsInitialExec,
    TlsLocalExec,

    /// AArch64 26-bit PC-relative branch (BL/B)
    AArch64Call26,
    /// AArch64 Page-relative address (ADRP)
    AArch64Adrp,
    /// AArch64 Add immediate 12-bit (ADD)
    AArch64AddLo12,

    /// RISC-V 32-bit PC-relative call (AUIPC + JALR)
    RiscvCall,
    /// RISC-V 12-bit PC-relative branch
    RiscvBranch,
    /// RISC-V 20-bit upper immediate (LUI / AUIPC)
    RiscvHi20,
    /// RISC-V 12-bit lower immediate (ADDI)
    RiscvLo12I,

    /// WebAssembly relocations
    WasmFunctionIndex,
    WasmTypeIndex,
    WasmGlobalIndex,
    WasmMemoryOffset32,
}

impl RelocationKind {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            RelocationKind::Absolute64 | RelocationKind::PcRelative64 | RelocationKind::Got64 => 8,
            RelocationKind::Absolute32
            | RelocationKind::PcRelative32
            | RelocationKind::PltRelative32
            | RelocationKind::GotRelative32
            | RelocationKind::SectionRelative32
            | RelocationKind::ImageRelative32
            | RelocationKind::AArch64Call26
            | RelocationKind::AArch64Adrp
            | RelocationKind::AArch64AddLo12
            | RelocationKind::RiscvCall
            | RelocationKind::RiscvBranch
            | RelocationKind::RiscvHi20
            | RelocationKind::RiscvLo12I
            | RelocationKind::WasmFunctionIndex
            | RelocationKind::WasmTypeIndex
            | RelocationKind::WasmGlobalIndex
            | RelocationKind::WasmMemoryOffset32
            | RelocationKind::TlsGeneralDynamic
            | RelocationKind::TlsInitialExec
            | RelocationKind::TlsLocalExec => 4,
            RelocationKind::Absolute16 => 2,
            RelocationKind::Absolute8 => 1,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            RelocationKind::Absolute64 => "ABS64",
            RelocationKind::Absolute32 => "ABS32",
            RelocationKind::Absolute16 => "ABS16",
            RelocationKind::Absolute8 => "ABS8",
            RelocationKind::PcRelative32 => "PC32",
            RelocationKind::PcRelative64 => "PC64",
            RelocationKind::PltRelative32 => "PLT32",
            RelocationKind::GotRelative32 => "GOTPC32",
            RelocationKind::Got64 => "GOT64",
            RelocationKind::SectionRelative32 => "SECREL32",
            RelocationKind::ImageRelative32 => "IMAGEREL32",
            RelocationKind::TlsGeneralDynamic => "TLS_GD",
            RelocationKind::TlsInitialExec => "TLS_IE",
            RelocationKind::TlsLocalExec => "TLS_LE",
            RelocationKind::AArch64Call26 => "AARCH64_CALL26",
            RelocationKind::AArch64Adrp => "AARCH64_ADRP",
            RelocationKind::AArch64AddLo12 => "AARCH64_ADD_LO12",
            RelocationKind::RiscvCall => "RISCV_CALL",
            RelocationKind::RiscvBranch => "RISCV_BRANCH",
            RelocationKind::RiscvHi20 => "RISCV_HI20",
            RelocationKind::RiscvLo12I => "RISCV_LO12_I",
            RelocationKind::WasmFunctionIndex => "WASM_FUNC_INDEX",
            RelocationKind::WasmTypeIndex => "WASM_TYPE_INDEX",
            RelocationKind::WasmGlobalIndex => "WASM_GLOBAL_INDEX",
            RelocationKind::WasmMemoryOffset32 => "WASM_MEMORY_ADDR",
        }
    }
}

/// A relocation entry referencing a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocation {
    pub offset: u64,
    pub symbol_name: String,
    /// Object-local symbol index as recorded by the object reader (e.g. the
    /// raw COFF symbol-table index). Required for precise resolution: LLVM
    /// COFF objects contain many same-named sections (`.text`, `.rdata`),
    /// whose symbols are distinguishable only by index, never by name.
    pub symbol_index: Option<usize>,
    /// Index of the object file supplying this relocation. Set when the
    /// section is merged into a `MergedSection`; used together with
    /// `symbol_index` to resolve the exact referenced symbol.
    pub file_index: Option<usize>,
    pub kind: RelocationKind,
    pub addend: i64,
}

impl Relocation {
    pub fn new(
        offset: u64,
        symbol_name: impl Into<String>,
        kind: RelocationKind,
        addend: i64,
    ) -> Self {
        Self {
            offset,
            symbol_name: symbol_name.into(),
            symbol_index: None,
            file_index: None,
            kind,
            addend,
        }
    }
}

impl fmt::Display for Relocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "0x{:08x} {:16} {} {:+}",
            self.offset,
            self.kind.name(),
            self.symbol_name,
            self.addend
        )
    }
}

/// Trait for architecture-specific relocation computation and patching.
pub trait RelocationHandler: Send + Sync {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()>;
}

/// Default standard relocation handler for x86_64, aarch64, etc.
pub struct DefaultRelocationHandler;

impl RelocationHandler for DefaultRelocationHandler {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        let offset = reloc.offset as usize;
        let size = reloc.kind.size_in_bytes();

        if offset + size > image.len() {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "relocation offset 0x{:x} + size {} exceeds section data buffer size 0x{:x}",
                    offset,
                    size,
                    image.len()
                ),
            ));
        }

        let target_slice = &mut image[offset..offset + size];

        match reloc.kind {
            RelocationKind::Absolute64 => {
                let val = (symbol_va as i64).wrapping_add(addend) as u64;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
            RelocationKind::Absolute32 => {
                let val = ((symbol_va as i64).wrapping_add(addend)) as u32;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
            RelocationKind::Absolute16 => {
                let val = ((symbol_va as i64).wrapping_add(addend)) as u16;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
            RelocationKind::Absolute8 => {
                let val = ((symbol_va as i64).wrapping_add(addend)) as u8;
                target_slice[0] = val;
            }
            RelocationKind::PcRelative32
            | RelocationKind::PltRelative32
            | RelocationKind::GotRelative32 => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                if val < i32::MIN as i64 || val > i32::MAX as i64 {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        i32::MIN as i64,
                        i32::MAX as i64,
                        None,
                        Some(reloc.offset),
                    ));
                }
                let val32 = val as i32 as u32;
                target_slice.copy_from_slice(&val32.to_le_bytes());
            }
            RelocationKind::PcRelative64 => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                target_slice.copy_from_slice(&(val as u64).to_le_bytes());
            }
            RelocationKind::SectionRelative32 => {
                // The value is `S + A - SectionBase(S)`: the offset of the
                // target within its own input section. The section base is
                // only known at layout time; PE layout applies this
                // relocation itself. Reaching this arm means a relocation
                // producer bypassed the layout path.
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "section-relative relocation against `{}` must be applied by the layout engine (section base required)",
                        reloc.symbol_name
                    ),
                ));
            }
            RelocationKind::ImageRelative32 => {
                // The value is `S + A - ImageBase` (an RVA). The image base
                // is only known at layout time; PE layout applies this
                // relocation itself.
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "image-relative relocation against `{}` must be applied by the layout engine (image base required)",
                        reloc.symbol_name
                    ),
                ));
            }
            RelocationKind::AArch64Call26 => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                if (val & 0x3) != 0 {
                    return Err(LinkError::new(
                        ErrorCode::RelocationOverflow,
                        format!(
                            "AArch64 CALL26 target 0x{:x} is not 4-byte aligned",
                            symbol_va
                        ),
                    ));
                }
                let imm26 = (val >> 2) & 0x03FF_FFFF;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & !0x03FF_FFFF) | (imm26 as u32);
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::AArch64Adrp => {
                let page_sym = (symbol_va as i64 + addend) >> 12;
                let page_place = (place_va as i64) >> 12;
                let page_offset = page_sym - page_place;
                let imm21 = (page_offset & 0x1F_FFFF) as u32;
                let immlo = (imm21 & 0x3) << 29;
                let immhi = ((imm21 >> 2) & 0x7FFFF) << 5;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & !((0x3 << 29) | (0x7FFFF << 5))) | immlo | immhi;
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::AArch64AddLo12 => {
                let imm12 = (((symbol_va as i64 + addend) & 0xFFF) as u32) << 10;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & !(0xFFF << 10)) | imm12;
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvCall => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let hi20 = ((val.wrapping_add(0x800)) >> 12) as u32;
                let lo12 = (val & 0xFFF) as u32;
                if target_slice.len() >= 8 {
                    let mut auipc = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                    let mut jalr = u32::from_le_bytes(target_slice[4..8].try_into().unwrap());
                    auipc = (auipc & 0xFFF) | (hi20 << 12);
                    jalr = (jalr & 0x000F_FFFF) | (lo12 << 20);
                    target_slice[0..4].copy_from_slice(&auipc.to_le_bytes());
                    target_slice[4..8].copy_from_slice(&jalr.to_le_bytes());
                }
            }
            RelocationKind::RiscvBranch => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let imm12 = (val & 0x1FFE) as u32;
                let b_imm12 = ((imm12 >> 12) & 1) << 31;
                let b_imm10_5 = ((imm12 >> 5) & 0x3F) << 25;
                let b_imm4_1 = ((imm12 >> 1) & 0xF) << 8;
                let b_imm11 = ((imm12 >> 11) & 1) << 7;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & 0x01FF_F07F) | b_imm12 | b_imm10_5 | b_imm4_1 | b_imm11;
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvHi20 => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let hi20 = ((val.wrapping_add(0x800)) >> 12) as u32;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & 0xFFF) | (hi20 << 12);
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvLo12I => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let lo12 = (val & 0xFFF) as u32;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & 0x000F_FFFF) | (lo12 << 20);
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::WasmFunctionIndex
            | RelocationKind::WasmTypeIndex
            | RelocationKind::WasmGlobalIndex
            | RelocationKind::WasmMemoryOffset32 => {
                let val = ((symbol_va as i64).wrapping_add(addend)) as u32;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
            RelocationKind::Got64 => {
                let val = (symbol_va as i64).wrapping_add(addend) as u64;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
            RelocationKind::TlsGeneralDynamic
            | RelocationKind::TlsInitialExec
            | RelocationKind::TlsLocalExec => {
                let val = ((symbol_va as i64).wrapping_add(addend)) as u32;
                target_slice.copy_from_slice(&val.to_le_bytes());
            }
        }

        Ok(())
    }
}
