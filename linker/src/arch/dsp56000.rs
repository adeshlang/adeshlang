//! Motorola DSP56000 (24-bit fixed-point Harvard DSP) relocation handling.
//!
//! Handles word-addressed big-endian 24-bit relocations:
//! - `Dsp56kAbs16`: 16-bit absolute word address in a 24-bit extension word `[0x00, hi, lo]`
//! - `Dsp56kAbs24`: 24-bit absolute big-endian word value `[b2, b1, b0]`
//!
//! Refuses all non-DSP56000 relocations loudly.

#![forbid(unsafe_code)]

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};

#[derive(Default)]
pub struct Dsp56000Arch;

impl Dsp56000Arch {
    fn check_word_bounds(reloc: &Relocation, image: &[u8]) -> LinkResult<usize> {
        let offset = usize::try_from(reloc.offset).map_err(|_| {
            LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "DSP56000 relocation offset 0x{:x} overflows buffer index",
                    reloc.offset
                ),
            )
        })?;
        if offset % 3 != 0 {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "DSP56000 relocation `{}` at byte offset 0x{:x} is not 3-byte (24-bit word) aligned",
                    reloc.symbol_name, offset
                ),
            ));
        }
        if offset.checked_add(3).is_none_or(|end| end > image.len()) {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "DSP56000 relocation offset 0x{:x} + 3 exceeds section buffer size 0x{:x}",
                    offset,
                    image.len()
                ),
            ));
        }
        Ok(offset)
    }
}

impl RelocationHandler for Dsp56000Arch {
    fn apply(
        &self,
        reloc: &Relocation,
        _place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        match reloc.kind {
            RelocationKind::Dsp56kAbs16 => {
                let offset = Self::check_word_bounds(reloc, image)?;
                let target = (symbol_va as i64).checked_add(addend).ok_or_else(|| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        i64::MAX,
                        0,
                        0xFFFF,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                if !(0..=0xFFFF).contains(&target) {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        target,
                        0,
                        0xFFFF,
                        None,
                        Some(reloc.offset),
                    ));
                }
                let addr16 = target as u16;
                image[offset] = 0x00;
                image[offset + 1] = ((addr16 >> 8) & 0xFF) as u8;
                image[offset + 2] = (addr16 & 0xFF) as u8;
                Ok(())
            }
            RelocationKind::Dsp56kAbs24 => {
                let offset = Self::check_word_bounds(reloc, image)?;
                let target = (symbol_va as i64).checked_add(addend).ok_or_else(|| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        i64::MAX,
                        -0x80_0000,
                        0xFF_FFFF,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                if !(-0x80_0000..=0xFF_FFFF).contains(&target) {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        target,
                        -0x80_0000,
                        0xFF_FFFF,
                        None,
                        Some(reloc.offset),
                    ));
                }
                let w24 = (target as u32) & 0x00FF_FFFF;
                image[offset] = ((w24 >> 16) & 0xFF) as u8;
                image[offset + 1] = ((w24 >> 8) & 0xFF) as u8;
                image[offset + 2] = (w24 & 0xFF) as u8;
                Ok(())
            }
            other => Err(LinkError::new(
                ErrorCode::UnsupportedRelocation,
                format!(
                    "relocation `{}` for symbol `{}` is unsupported on Motorola DSP56000",
                    other.name(),
                    reloc.symbol_name
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relocation::Relocation;

    #[test]
    fn applies_big_endian_abs16_and_abs24() {
        let handler = Dsp56000Arch;
        let mut buf = vec![0u8; 6];
        let r16 = Relocation {
            offset: 0,
            kind: RelocationKind::Dsp56kAbs16,
            symbol_index: Some(0),
            symbol_name: "fn_target".to_string(),
            addend: 2,
            file_index: None,
        };
        handler.apply(&r16, 0, 0x0120, 2, &mut buf).unwrap();
        assert_eq!(&buf[0..3], &[0x00, 0x01, 0x22]);

        let r24 = Relocation {
            offset: 3,
            kind: RelocationKind::Dsp56kAbs24,
            symbol_index: Some(0),
            symbol_name: "word_const".to_string(),
            addend: 0,
            file_index: None,
        };
        handler.apply(&r24, 1, 0x12_3456, 0, &mut buf).unwrap();
        assert_eq!(&buf[3..6], &[0x12, 0x34, 0x56]);
    }

    #[test]
    fn refuses_unaligned_or_overflow_relocation() {
        let handler = Dsp56000Arch;
        let mut buf = vec![0u8; 6];
        let unaligned = Relocation {
            offset: 1,
            kind: RelocationKind::Dsp56kAbs16,
            symbol_index: Some(0),
            symbol_name: "bad".to_string(),
            addend: 0,
            file_index: None,
        };
        assert!(handler.apply(&unaligned, 0, 0x10, 0, &mut buf).is_err());

        let overflow = Relocation {
            offset: 0,
            kind: RelocationKind::Dsp56kAbs16,
            symbol_index: Some(0),
            symbol_name: "too_high".to_string(),
            addend: 0,
            file_index: None,
        };
        assert!(handler.apply(&overflow, 0, 0x1_0000, 0, &mut buf).is_err());
    }
}
