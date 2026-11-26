//! 16-bit Intel 8086 real-mode relocation handling.
//!
//! Supports `Absolute16`, `Absolute8`, `PcRelative16`, and `I8086Segment16`.
//! Refuses all 32-bit, 64-bit, and foreign-architecture relocation kinds loudly.

#![forbid(unsafe_code)]

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};

#[derive(Default)]
pub struct I8086Arch;

impl I8086Arch {
    fn check_bounds(reloc: &Relocation, image: &[u8]) -> LinkResult<usize> {
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

impl RelocationHandler for I8086Arch {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        let offset = Self::check_bounds(reloc, image)?;
        match reloc.kind {
            RelocationKind::Absolute16 => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let val16 = u16::try_from(val).map_err(|_| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        0,
                        u16::MAX as i64,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                image[offset..offset + 2].copy_from_slice(&val16.to_le_bytes());
                Ok(())
            }
            RelocationKind::Absolute8 => {
                let val = (symbol_va as i64).wrapping_add(addend);
                let val8 = u8::try_from(val).map_err(|_| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        val,
                        0,
                        u8::MAX as i64,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                image[offset] = val8;
                Ok(())
            }
            RelocationKind::PcRelative16 => {
                let pcrel = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64);
                let disp16 = i16::try_from(pcrel).map_err(|_| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        pcrel,
                        i16::MIN as i64,
                        i16::MAX as i64,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                image[offset..offset + 2].copy_from_slice(&disp16.to_le_bytes());
                Ok(())
            }
            RelocationKind::I8086Segment16 => {
                let seg = ((symbol_va as i64).wrapping_add(addend)) >> 4;
                let seg16 = u16::try_from(seg).map_err(|_| {
                    LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        seg,
                        0,
                        u16::MAX as i64,
                        None,
                        Some(reloc.offset),
                    )
                })?;
                image[offset..offset + 2].copy_from_slice(&seg16.to_le_bytes());
                Ok(())
            }
            other => Err(LinkError::new(
                ErrorCode::UnsupportedRelocation,
                format!(
                    "relocation {:?} against `{}` is not valid for 16-bit Intel 8086",
                    other, reloc.symbol_name
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_16bit_abs_pcrel_and_segment_and_refuses_32bit() {
        let handler = I8086Arch;
        let mut buf = [0u8; 8];

        handler
            .apply(
                &Relocation::new(0, "str0", RelocationKind::Absolute16, 0),
                0x0100,
                0x0142,
                0,
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[0..2], &0x0142u16.to_le_bytes());

        handler
            .apply(
                &Relocation::new(2, "fn_target", RelocationKind::PcRelative16, -2),
                0x0110,
                0x0120,
                -2,
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[2..4], &0x000Ei16.to_le_bytes());

        handler
            .apply(
                &Relocation::new(4, "data_seg", RelocationKind::I8086Segment16, 0),
                0,
                0x0240,
                0,
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[4..6], &0x0024u16.to_le_bytes());

        let err = handler
            .apply(
                &Relocation::new(0, "wide", RelocationKind::Absolute32, 0),
                0,
                0x1000,
                0,
                &mut buf,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
    }
}
