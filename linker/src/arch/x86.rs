//! 32-bit x86 (IA-32) relocation handling.
//!
//! Applies 32-bit relocation semantics to IA-32 images. The x8-64 handler is
//! deliberately *not* used here: applying 64-bit widths/relocations to a 32-bit
//! image would silently miscompile. Every relocation kind that is not legal for
//! IA-32 (notably 64-bit widths and non-x86 architectures) is refused loudly.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap in the refusal
//! path, minimal executable size.

#![forbid(unsafe_code)]

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};
use crate::target::Endianness;

pub struct X86Arch {
    endianness: Endianness,
}

impl X86Arch {
    pub fn new(endianness: Endianness) -> Self {
        Self { endianness }
    }

    fn check_bounds(&self, reloc: &Relocation, image: &[u8]) -> LinkResult<usize> {
        let size = reloc.kind.size_in_bytes();
        let offset = usize::try_from(reloc.offset).map_err(|_| {
            LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "relocation offset 0x{:x} overflows addressable buffer index",
                    reloc.offset
                ),
            )
        })?;

        if offset.checked_add(size).is_none_or(|end| end > image.len()) {
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

        Ok(offset)
    }
}

impl Default for X86Arch {
    fn default() -> Self {
        Self::new(Endianness::Little)
    }
}

impl RelocationHandler for X86Arch {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        let offset = self.check_bounds(reloc, image)?;
        let target_slice = &mut image[offset..offset + reloc.kind.size_in_bytes()];

        match reloc.kind {
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
                    let val16 = val as u16;
                    if self.endianness.is_big() {
                        target_slice.copy_from_slice(&val16.to_be_bytes());
                    } else {
                        target_slice.copy_from_slice(&val16.to_le_bytes());
                    }
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
                if val < i32::MIN as i64 || val > u32::MAX as i64 {
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
                let val32 = val as u32;
                if self.endianness.is_big() {
                    target_slice.copy_from_slice(&val32.to_be_bytes());
                } else {
                    target_slice.copy_from_slice(&val32.to_le_bytes());
                }
            }
            RelocationKind::PcRelative32 | RelocationKind::PltRelative32 => {
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
                if self.endianness.is_big() {
                    target_slice.copy_from_slice(&val32.to_be_bytes());
                } else {
                    target_slice.copy_from_slice(&val32.to_le_bytes());
                }
            }
            RelocationKind::X86Got32
            | RelocationKind::X86GotOff32
            | RelocationKind::X86GotPc32 => {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "IA-32 GOT relocation {} against `{}` must be applied by the layout engine (GOT base required)",
                        reloc.kind.name(),
                        reloc.symbol_name
                    ),
                ));
            }
            RelocationKind::Absolute64 | RelocationKind::PcRelative64 => {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "relocation {:?} uses a 64-bit width and is illegal on IA-32 (i686) images",
                        reloc.kind
                    ),
                )
                .with_note("IA-32 addresses and pointers are 32 bits wide")
                .with_suggestion("link for an x86_64 target, or use a 32-bit relocation kind"));
            }
            _ => {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "relocation {:?} is not legal for IA-32 (i686) images",
                        reloc.kind
                    ),
                )
                .with_suggestion("link for an x86_64 target, or remove the relocation"));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relocation::Relocation;

    fn image() -> Vec<u8> {
        vec![0u8; 32]
    }

    #[test]
    fn absolute32_writes_le_u32_and_validates_range() {
        let handler = X86Arch::default();
        let mut img = image();
        let reloc = Relocation::new(0, "sym", RelocationKind::Absolute32, 0);
        handler.apply(&reloc, 0x1000, 0x2000, 0, &mut img).unwrap();
        assert_eq!(u32::from_le_bytes(img[0..4].try_into().unwrap()), 0x2000);
    }

    #[test]
    fn pc_relative32_computes_s_minus_p() {
        let handler = X86Arch::default();
        let mut img = image();
        // S = 0x2000, A = 0, P = 0x1000 => 0x1000
        let reloc = Relocation::new(0, "sym", RelocationKind::PcRelative32, 0);
        handler.apply(&reloc, 0x1000, 0x2000, 0, &mut img).unwrap();
        assert_eq!(i32::from_le_bytes(img[0..4].try_into().unwrap()), 0x1000);
    }

    #[test]
    fn absolute64_is_refused_on_ia32() {
        let handler = X86Arch::default();
        let mut img = image();
        let reloc = Relocation::new(0, "sym", RelocationKind::Absolute64, 0);
        let err = handler
            .apply(&reloc, 0x1000, 0x2000, 0, &mut img)
            .expect_err("Absolute64 must be refused on IA-32");
        assert!(
            err.to_string().contains("64-bit") || err.to_string().contains("IA-32"),
            "error should name the illegal width: {err}"
        );
        assert!(img.iter().all(|b| *b == 0), "image untouched");
    }

    #[test]
    fn riscv_reloc_is_refused_on_ia32() {
        let handler = X86Arch::default();
        let mut img = image();
        let reloc = Relocation::new(0, "sym", RelocationKind::RiscvCall, 0);
        let err = handler
            .apply(&reloc, 0x1000, 0x2000, 0, &mut img)
            .expect_err("RISC-V relocation must be refused on IA-32");
        assert!(
            err.to_string().contains("IA-32"),
            "error should name IA-32: {err}"
        );
        assert!(img.iter().all(|b| *b == 0), "image untouched");
    }

    #[test]
    fn absolute32_accepts_unsigned_32bit_addresses_above_i32_max() {
        let handler = X86Arch::default();
        let mut img = image();
        let reloc = Relocation::new(0, "sym", RelocationKind::Absolute32, 0);
        handler
            .apply(&reloc, 0x1000, 0x8000_2000, 0, &mut img)
            .unwrap();
        assert_eq!(u32::from_le_bytes(img[0..4].try_into().unwrap()), 0x8000_2000);
    }

    #[test]
    fn absolute32_overflow_is_refused() {
        let handler = X86Arch::default();
        let mut img = image();
        // S = 0x1_0000_0000 exceeds u32::MAX
        let reloc = Relocation::new(0, "sym", RelocationKind::Absolute32, 0);
        let err = handler
            .apply(&reloc, 0, 0x1_0000_0000, 0, &mut img)
            .expect_err("overflow must be refused");
        assert!(
            err.to_string().contains("overflowed"),
            "error should name overflow: {err}"
        );
        assert!(img.iter().all(|b| *b == 0), "image untouched");
    }
}
