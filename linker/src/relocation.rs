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
    /// x86-64 `R_X86_64_DTPOFF32`: module-relative offset within a TLS
    /// block, produced by `__tls_get_addr` local-dynamic sequences. Only
    /// valid after the pair's TLSLD call was relaxed to `movq %fs:0,%rax`.
    TlsDtpOff32,

    /// AArch64 26-bit PC-relative branch (BL/B)
    AArch64Call26,
    /// AArch64 Page-relative address (ADRP)
    AArch64Adrp,
    /// AArch64 Add immediate 12-bit (ADD)
    AArch64AddLo12,
    /// AArch64 64-bit Load/Store scaled immediate 12-bit (LDR xT, [xN, #off])
    AArch64LdSt64Lo12,
    /// AArch64 Mach-O TLV descriptor address, low 12 bits. Same value as
    /// `AArch64AddLo12`, but the instruction it patches is a load
    /// (`ldr xN, [xN, #imm]`) that the linker must convert into an `add`:
    /// thread-local accessors reach the descriptor through its *address*, and
    /// the descriptor's first field is the binding thunk.
    AArch64TlvDescriptorLo12,
    /// x86-64 Mach-O TLV descriptor address as a 32-bit RIP-relative
    /// displacement. Same value as `PcRelative32`, and the preceding
    /// `movq` opcode is rewritten to `leaq` for the same reason as the
    /// AArch64 form above.
    X86_64TlvDescriptorRel32,

    /// RISC-V 32-bit PC-relative call (AUIPC + JALR)
    RiscvCall,
    /// RISC-V 20-bit PC-relative jump (JAL)
    RiscvJal,
    /// RISC-V 12-bit PC-relative branch (B-type)
    RiscvBranch,
    /// RISC-V 20-bit upper immediate (LUI / AUIPC)
    RiscvHi20,
    /// RISC-V 12-bit lower immediate for arithmetic/loads (I-type, ADDI / LD / LW)
    RiscvLo12I,
    /// RISC-V 12-bit lower immediate for stores (S-type, SD / SW)
    RiscvLo12S,
    /// RISC-V 20-bit PC-relative upper immediate
    RiscvPcrelHi20,
    /// RISC-V 12-bit PC-relative lower immediate for arithmetic/loads (I-type)
    RiscvPcrelLo12I,
    /// RISC-V 12-bit PC-relative lower immediate for stores (S-type)
    RiscvPcrelLo12S,

    /// WebAssembly relocations. Index kinds patch a padded LEB128 placeholder;
    /// the `...I32` kinds patch a fixed little-endian 32-bit word instead.
    WasmFunctionIndex,
    WasmTypeIndex,
    WasmGlobalIndex,
    /// `R_WASM_GLOBAL_INDEX_I32`: a global index stored as a fixed `i32`.
    WasmGlobalIndexI32,
    /// `R_WASM_MEMORY_ADDR_LEB`: a linear-memory address as an unsigned LEB.
    WasmMemoryOffset32,
    /// `R_WASM_MEMORY_ADDR_SLEB`: a linear-memory address as a signed LEB.
    WasmMemoryOffsetSleb,
    /// `R_WASM_MEMORY_ADDR_I32`: a linear-memory address as a fixed `i32`.
    WasmMemoryOffsetI32,
    /// `R_WASM_TABLE_INDEX_SLEB`: a slot in the function table as a signed LEB.
    WasmTableIndexSleb,
    /// `R_WASM_TABLE_INDEX_I32`: a slot in the function table as a fixed `i32`.
    WasmTableIndexI32,
    /// `R_WASM_TABLE_NUMBER_LEB`: the index of the function table itself, as
    /// `call_indirect` and `table.get` name it.
    WasmTableNumber,
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
            | RelocationKind::AArch64LdSt64Lo12
            | RelocationKind::AArch64TlvDescriptorLo12
            | RelocationKind::X86_64TlvDescriptorRel32
            | RelocationKind::RiscvCall
            | RelocationKind::RiscvJal
            | RelocationKind::RiscvBranch
            | RelocationKind::RiscvHi20
            | RelocationKind::RiscvLo12I
            | RelocationKind::RiscvLo12S
            | RelocationKind::RiscvPcrelHi20
            | RelocationKind::RiscvPcrelLo12I
            | RelocationKind::RiscvPcrelLo12S
            | RelocationKind::WasmFunctionIndex
            | RelocationKind::WasmTypeIndex
            | RelocationKind::WasmGlobalIndex
            | RelocationKind::WasmGlobalIndexI32
            | RelocationKind::WasmMemoryOffset32
            | RelocationKind::WasmMemoryOffsetSleb
            | RelocationKind::WasmMemoryOffsetI32
            | RelocationKind::WasmTableIndexSleb
            | RelocationKind::WasmTableIndexI32
            | RelocationKind::WasmTableNumber
            | RelocationKind::TlsGeneralDynamic
            | RelocationKind::TlsInitialExec
            | RelocationKind::TlsLocalExec
            | RelocationKind::TlsDtpOff32 => 4,
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
            RelocationKind::TlsDtpOff32 => "TLS_DTPOFF32",
            RelocationKind::AArch64Call26 => "AARCH64_CALL26",
            RelocationKind::AArch64Adrp => "AARCH64_ADRP",
            RelocationKind::AArch64AddLo12 => "AARCH64_ADD_LO12",
            RelocationKind::AArch64LdSt64Lo12 => "AARCH64_LDST64_LO12",
            RelocationKind::AArch64TlvDescriptorLo12 => "AARCH64_TLV_DESC_LO12",
            RelocationKind::X86_64TlvDescriptorRel32 => "X86_64_TLV_DESC_REL32",
            RelocationKind::RiscvCall => "RISCV_CALL",
            RelocationKind::RiscvJal => "RISCV_JAL",
            RelocationKind::RiscvBranch => "RISCV_BRANCH",
            RelocationKind::RiscvHi20 => "RISCV_HI20",
            RelocationKind::RiscvLo12I => "RISCV_LO12_I",
            RelocationKind::RiscvLo12S => "RISCV_LO12_S",
            RelocationKind::RiscvPcrelHi20 => "RISCV_PCREL_HI20",
            RelocationKind::RiscvPcrelLo12I => "RISCV_PCREL_LO12_I",
            RelocationKind::RiscvPcrelLo12S => "RISCV_PCREL_LO12_S",
            RelocationKind::WasmFunctionIndex => "WASM_FUNC_INDEX",
            RelocationKind::WasmTypeIndex => "WASM_TYPE_INDEX",
            RelocationKind::WasmGlobalIndex => "WASM_GLOBAL_INDEX",
            RelocationKind::WasmGlobalIndexI32 => "WASM_GLOBAL_INDEX_I32",
            RelocationKind::WasmMemoryOffset32 => "WASM_MEMORY_ADDR",
            RelocationKind::WasmMemoryOffsetSleb => "WASM_MEMORY_ADDR_SLEB",
            RelocationKind::WasmMemoryOffsetI32 => "WASM_MEMORY_ADDR_I32",
            RelocationKind::WasmTableIndexSleb => "WASM_TABLE_INDEX_SLEB",
            RelocationKind::WasmTableIndexI32 => "WASM_TABLE_INDEX_I32",
            RelocationKind::WasmTableNumber => "WASM_TABLE_NUMBER",
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
                let val = match (symbol_va as i64).checked_add(addend) {
                    Some(v) => v,
                    None => {
                        return Err(LinkError::relocation_overflow(
                            reloc.kind.name(),
                            &reloc.symbol_name,
                            i64::MAX,
                            i32::MIN as i64,
                            u32::MAX as i64,
                            None,
                            Some(reloc.offset),
                        ));
                    }
                };
                // Valid 32-bit value range: signed i32 / unsigned u32, or 64-bit addresses in 0x1_0000_0000..=0x1_FFFF_FFFF (Mach-O 64-bit image base 0x100000000)
                if (val >= i32::MIN as i64 && val <= u32::MAX as i64)
                    || (val >= 0x1_0000_0000 && val <= 0x1_FFFF_FFFF)
                {
                    let val32 = val as u32;
                    target_slice.copy_from_slice(&val32.to_le_bytes());
                } else {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        i32::MIN as i64,
                        u32::MAX as i64,
                        None,
                        Some(reloc.offset),
                    ));
                }
            }
            RelocationKind::Absolute16 => {
                let val = match (symbol_va as i64).checked_add(addend) {
                    Some(v) => v,
                    None => {
                        return Err(LinkError::relocation_overflow(
                            reloc.kind.name(),
                            &reloc.symbol_name,
                            i64::MAX,
                            i16::MIN as i64,
                            u16::MAX as i64,
                            None,
                            Some(reloc.offset),
                        ));
                    }
                };
                if val >= i16::MIN as i64 && val <= u16::MAX as i64 {
                    target_slice.copy_from_slice(&(val as u16).to_le_bytes());
                } else {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        i16::MIN as i64,
                        u16::MAX as i64,
                        None,
                        Some(reloc.offset),
                    ));
                }
            }
            RelocationKind::Absolute8 => {
                let val = match (symbol_va as i64).checked_add(addend) {
                    Some(v) => v,
                    None => {
                        return Err(LinkError::relocation_overflow(
                            reloc.kind.name(),
                            &reloc.symbol_name,
                            i64::MAX,
                            i8::MIN as i64,
                            u8::MAX as i64,
                            None,
                            Some(reloc.offset),
                        ));
                    }
                };
                if val >= i8::MIN as i64 && val <= u8::MAX as i64 {
                    target_slice[0] = val as u8;
                } else {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        i8::MIN as i64,
                        u8::MAX as i64,
                        None,
                        Some(reloc.offset),
                    ));
                }
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
            RelocationKind::AArch64LdSt64Lo12 => {
                let pageoff = (symbol_va as i64 + addend) & 0xFFF;
                if (pageoff & 0x7) != 0 {
                    return Err(LinkError::new(
                        ErrorCode::RelocationOverflow,
                        format!(
                            "AArch64 LDST64_LO12 target offset 0x{:x} is not 8-byte aligned",
                            pageoff
                        ),
                    ));
                }
                let imm12 = ((pageoff >> 3) as u32) << 10;
                let mut insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                insn = (insn & !(0xFFF << 10)) | imm12;
                target_slice.copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::AArch64TlvDescriptorLo12 => {
                // Mach-O `ARM64_RELOC_TLVP_LOAD_PAGEOFF12`. The instruction
                // the relocation carries is a load of the descriptor through
                // the page base (`ldr xT, [xN, #imm]`); the thread-local
                // accessor must instead take the descriptor's *address*, so
                // the linker rewrites the LDR into an ADD with the unscaled
                // page offset. Keeping Rt/Rn (bits 9:0) preserves the register
                // the compiler chose. Matches ld64 and lld.
                let pageoff = (((symbol_va as i64 + addend) & 0xFFF) as u32) << 10;
                let insn = u32::from_le_bytes(target_slice.try_into().unwrap());
                // Must be a 64-bit LDR immediate (unsigned offset): bits 31:22
                // are 0b1111100101 = 0x3E5, so the opcode field masked off is
                // 0xF940_0000. Anything else means the codegen did not emit the
                // accessor this relocation describes.
                if insn & 0xFFC0_0000 != 0xF940_0000 {
                    return Err(LinkError::new(
                        ErrorCode::UnsupportedRelocation,
                        format!(
                            "{} against `{}` at 0x{:x} does not patch a 64-bit `ldr` (found 0x{:08x})",
                            reloc.kind.name(),
                            reloc.symbol_name,
                            reloc.offset,
                            insn
                        ),
                    ));
                }
                let add_insn = 0x9100_0000 | pageoff | (insn & 0x3FF);
                target_slice.copy_from_slice(&add_insn.to_le_bytes());
            }
            RelocationKind::X86_64TlvDescriptorRel32 => {
                // Mach-O `X86_64_RELOC_TLV`. Like `PcRelative32`, but the RIP
                // base is the *end* of the whole instruction, not the start of
                // the displaced field, so subtract `place + 4`.
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64 + 4);
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
                // The `movq` opcode byte (`0x8B`) sits two bytes before the
                // displacement (`48 8B 3D disp32`). The accessor must load the
                // descriptor's address, so rewrite `movq`→`leaq` (`0x8D`).
                if offset < 2 {
                    return Err(LinkError::new(
                        ErrorCode::RelocationOverflow,
                        format!(
                            "{} against `{}` at 0x{:x} is below the instruction start",
                            reloc.kind.name(),
                            reloc.symbol_name,
                            reloc.offset
                        ),
                    ));
                }
                if image[offset - 2] != 0x8B {
                    return Err(LinkError::new(
                        ErrorCode::UnsupportedRelocation,
                        format!(
                            "{} against `{}` at 0x{:x} does not patch a `movq` (found 0x{:02x})",
                            reloc.kind.name(),
                            reloc.symbol_name,
                            reloc.offset,
                            image[offset - 2]
                        ),
                    ));
                }
                image[offset - 2] = 0x8D;
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
                } else if target_slice.len() >= 4 {
                    let imm = (val >> 1) as u32;
                    let imm20 = (imm >> 19) & 0x1;
                    let imm10_1 = imm & 0x03FF;
                    let imm11 = (imm >> 10) & 0x1;
                    let imm19_12 = (imm >> 11) & 0xFF;
                    let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                    insn = (insn & 0xFFF)
                        | (imm20 << 31)
                        | (imm10_1 << 21)
                        | (imm11 << 20)
                        | (imm19_12 << 12);
                    target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
                }
            }
            RelocationKind::RiscvJal => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let imm = (val >> 1) as u32;
                let imm20 = (imm >> 19) & 0x1;
                let imm10_1 = imm & 0x03FF;
                let imm11 = (imm >> 10) & 0x1;
                let imm19_12 = (imm >> 11) & 0xFF;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0xFFF)
                    | (imm20 << 31)
                    | (imm10_1 << 21)
                    | (imm11 << 20)
                    | (imm19_12 << 12);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
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
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0xFFF) | (hi20 << 12);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvLo12I => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let lo12 = (val & 0xFFF) as u32;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0x000F_FFFF) | (lo12 << 20);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvLo12S => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let lo12 = (val & 0xFFF) as u32;
                let imm11_5 = (lo12 >> 5) & 0x7F;
                let imm4_0 = lo12 & 0x1F;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0x01FF_E07F) | (imm11_5 << 25) | (imm4_0 << 7);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvPcrelHi20 => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let hi20 = ((val.wrapping_add(0x800)) >> 12) as u32;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0xFFF) | (hi20 << 12);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvPcrelLo12I => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let lo12 = (val & 0xFFF) as u32;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0x000F_FFFF) | (lo12 << 20);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::RiscvPcrelLo12S => {
                let val = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let lo12 = (val & 0xFFF) as u32;
                let imm11_5 = (lo12 >> 5) & 0x7F;
                let imm4_0 = lo12 & 0x1F;
                let mut insn = u32::from_le_bytes(target_slice[0..4].try_into().unwrap());
                insn = (insn & 0x01FF_E07F) | (imm11_5 << 25) | (imm4_0 << 7);
                target_slice[0..4].copy_from_slice(&insn.to_le_bytes());
            }
            RelocationKind::WasmFunctionIndex
            | RelocationKind::WasmTypeIndex
            | RelocationKind::WasmGlobalIndex
            | RelocationKind::WasmGlobalIndexI32
            | RelocationKind::WasmMemoryOffset32
            | RelocationKind::WasmMemoryOffsetSleb
            | RelocationKind::WasmMemoryOffsetI32
            | RelocationKind::WasmTableIndexSleb
            | RelocationKind::WasmTableIndexI32
            | RelocationKind::WasmTableNumber => {
                // WASM fields are padded LEB128 placeholders or fixed i32s in
                // section payloads, and their values are index-space numbers,
                // not virtual addresses. The generic layout engine has neither
                // the plan's index maps nor the encoding, so applying one here
                // would corrupt the module.
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "WebAssembly relocation `{}` against `{}` reached the generic \
                         relocation engine; WASM relocations are applied by the module writer",
                        reloc.kind.name(),
                        reloc.symbol_name
                    ),
                ));
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
            RelocationKind::TlsDtpOff32 => {
                // A DTPOFF32 disp32 is only meaningful inside a relaxed
                // local-dynamic sequence (base `%rax` = thread pointer);
                // the layout engine owns that rewrite. Reaching this arm
                // means a relocation producer bypassed the layout path.
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "TLS module-relative relocation (DTPOFF32) against `{}` must be applied \
                         by the layout engine (thread-pointer-relative rewrite required)",
                        reloc.symbol_name
                    ),
                ));
            }
        }

        Ok(())
    }
}
