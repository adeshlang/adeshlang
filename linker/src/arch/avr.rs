//! 8-bit AVR (ATmega328P `avr5`) relocation handling.
//!
//! Handles Harvard-architecture AVR relocations:
//! - `AvrCall22`: 32-bit `CALL`/`JMP` target (22-bit Flash word address)
//! - `AvrPcrel12`: 16-bit `RCALL`/`RJMP` target (12-bit signed word displacement)
//! - `AvrLo8` / `AvrHi8`: `LDI Rd, K` low/high byte of 16-bit address
//! - `AvrAddr16` / `Absolute16`: 16-bit little-endian Flash/SRAM byte address
//! - `Absolute8`: 8-bit immediate/data byte
//!
//! Refuses all 32-bit, 64-bit, and foreign-architecture relocations loudly.

#![forbid(unsafe_code)]

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};

#[derive(Default)]
pub struct AvrArch;

impl AvrArch {
    fn check_bounds_len(reloc: &Relocation, min_len: usize, image: &[u8]) -> LinkResult<usize> {
        let offset = usize::try_from(reloc.offset).map_err(|_| {
            LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "AVR relocation offset 0x{:x} overflows addressable buffer index",
                    reloc.offset
                ),
            )
        })?;
        if offset.checked_add(min_len).is_none_or(|end| end > image.len()) {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "AVR relocation offset 0x{:x} + size {} exceeds section buffer size 0x{:x}",
                    offset,
                    min_len,
                    image.len()
                ),
            ));
        }
        Ok(offset)
    }
}

impl RelocationHandler for AvrArch {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        match reloc.kind {
            RelocationKind::AvrCall22 => {
                let offset = Self::check_bounds_len(reloc, 4, image)?;
                let target_byte = (symbol_va as i64).wrapping_add(addend);
                if target_byte < 0 || (target_byte & 1) != 0 {
                    return Err(LinkError::new(
                        ErrorCode::RelocationOverflow,
                        format!(
                            "AVR CALL/JMP target `{}` address 0x{:x} must be non-negative and 2-byte aligned",
                            reloc.symbol_name, target_byte
                        ),
                    ));
                }
                let word_addr = (target_byte as u64) >> 1;
                if word_addr > 0x3F_FFFF {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        word_addr as i64,
                        0,
                        0x3F_FFFF,
                        None,
                        Some(reloc.offset),
                    ));
                }
                let w0 = u16::from_le_bytes([image[offset], image[offset + 1]]);
                let patched0 = (w0 & 0xFE0E)
                    | ((((word_addr >> 17) as u16) & 0x1F) << 4)
                    | (((word_addr >> 16) as u16) & 0x01);
                let patched1 = (word_addr & 0xFFFF) as u16;
                image[offset..offset + 2].copy_from_slice(&patched0.to_le_bytes());
                image[offset + 2..offset + 4].copy_from_slice(&patched1.to_le_bytes());
                Ok(())
            }
            RelocationKind::AvrPcrel12 => {
                let offset = Self::check_bounds_len(reloc, 2, image)?;
                let disp_bytes = (symbol_va as i64)
                    .wrapping_add(addend)
                    .wrapping_sub(place_va as i64 + 2);
                if (disp_bytes & 1) != 0 {
                    return Err(LinkError::new(
                        ErrorCode::RelocationOverflow,
                        format!(
                            "AVR RCALL/RJMP displacement {disp_bytes} to `{}` is not 2-byte aligned",
                            reloc.symbol_name
                        ),
                    ));
                }
                let disp_words = disp_bytes >> 1;
                if !(-2048..=2047).contains(&disp_words) {
                    return Err(LinkError::relocation_overflow(
                        reloc.kind.name(),
                        &reloc.symbol_name,
                        disp_words,
                        -2048,
                        2047,
                        None,
                        Some(reloc.offset),
                    ));
                }
                let w = u16::from_le_bytes([image[offset], image[offset + 1]]);
                let patched = (w & 0xF000) | ((disp_words as u16) & 0x0FFF);
                image[offset..offset + 2].copy_from_slice(&patched.to_le_bytes());
                Ok(())
            }
            RelocationKind::AvrLo8 => {
                let offset = Self::check_bounds_len(reloc, 2, image)?;
                let val = (symbol_va as i64).wrapping_add(addend);
                let byte = (val & 0xFF) as u16;
                let w = u16::from_le_bytes([image[offset], image[offset + 1]]);
                let patched = (w & 0xF0F0) | ((byte & 0xF0) << 4) | (byte & 0x0F);
                image[offset..offset + 2].copy_from_slice(&patched.to_le_bytes());
                Ok(())
            }
            RelocationKind::AvrHi8 => {
                let offset = Self::check_bounds_len(reloc, 2, image)?;
                let val = (symbol_va as i64).wrapping_add(addend);
                let byte = ((val >> 8) & 0xFF) as u16;
                let w = u16::from_le_bytes([image[offset], image[offset + 1]]);
                let patched = (w & 0xF0F0) | ((byte & 0xF0) << 4) | (byte & 0x0F);
                image[offset..offset + 2].copy_from_slice(&patched.to_le_bytes());
                Ok(())
            }
            RelocationKind::AvrAddr16 | RelocationKind::Absolute16 => {
                let offset = Self::check_bounds_len(reloc, 2, image)?;
                let val = (symbol_va as i64).wrapping_add(addend);
                let val16 = (val & 0xFFFF) as u16;
                image[offset..offset + 2].copy_from_slice(&val16.to_le_bytes());
                Ok(())
            }
            RelocationKind::Absolute8 => {
                let offset = Self::check_bounds_len(reloc, 1, image)?;
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
            other => Err(LinkError::new(
                ErrorCode::UnsupportedRelocation,
                format!(
                    "relocation {:?} against `{}` is not valid for 8-bit AVR",
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
    fn avr_call22_and_ldi_lo_hi_encoding() {
        let handler = AvrArch;
        // CALL placeholder 0x940E 0x0000 -> word addr 0x0123 (byte addr 0x0246)
        let mut buf = [0x0Eu8, 0x94, 0x00, 0x00];
        handler
            .apply(
                &Relocation::new(0, "fn_target", RelocationKind::AvrCall22, 0),
                0,
                0x0246,
                0,
                &mut buf,
            )
            .expect("call22");
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0x940E);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0123);

        // LDI r24, 0 (0xE080) + LDI r25, 0 (0xE090) for address 0x0246
        let mut ldi = [0x80u8, 0xE0, 0x90, 0xE0];
        handler
            .apply(
                &Relocation::new(0, "str", RelocationKind::AvrLo8, 0),
                0,
                0x0246,
                0,
                &mut ldi,
            )
            .expect("lo8");
        handler
            .apply(
                &Relocation::new(2, "str", RelocationKind::AvrHi8, 0),
                2,
                0x0246,
                0,
                &mut ldi,
            )
            .expect("hi8");
        // lo8(0x0246) = 0x46 -> 0xE486; hi8(0x0246) = 0x02 -> 0xE092
        assert_eq!(u16::from_le_bytes([ldi[0], ldi[1]]), 0xE486);
        assert_eq!(u16::from_le_bytes([ldi[2], ldi[3]]), 0xE092);
    }

    #[test]
    fn avr_refuses_foreign_relocation() {
        let handler = AvrArch;
        let mut buf = [0u8; 8];
        let err = handler
            .apply(
                &Relocation::new(0, "foo", RelocationKind::Absolute64, 0),
                0,
                0x100,
                0,
                &mut buf,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
    }
}
