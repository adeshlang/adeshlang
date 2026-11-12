//! 32-bit x86 (IA-32) relocation handling.
//!
//! No IA-32 relocation handler exists. Routing `Arch::X86` to the x86-64
//! handler would apply 64-bit relocation semantics to a 32-bit image and still
//! report success, so every relocation is refused with a named error instead.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::relocation::{Relocation, RelocationHandler};

pub struct X86Arch;

impl RelocationHandler for X86Arch {
    fn apply(
        &self,
        reloc: &Relocation,
        _place_va: u64,
        _symbol_va: u64,
        _addend: i64,
        _image: &mut [u8],
    ) -> LinkResult<()> {
        Err(LinkError::new(
            ErrorCode::UnsupportedRelocation,
            format!(
                "relocation {:?} cannot be applied: no IA-32 (i686) relocation handler exists",
                reloc.kind
            ),
        )
        .with_note("the x86-64 handler is deliberately not used for 32-bit x86 images")
        .with_suggestion("link for an x86_64 target, or implement an IA-32 handler"))
    }
}
