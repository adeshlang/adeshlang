//! MIPS (mips, mipsle, mips64, mips64le) definitions and relocation handler.

use crate::error::LinkResult;
use crate::relocation::{DefaultRelocationHandler, Relocation, RelocationHandler};

pub struct MipsArch;

impl RelocationHandler for MipsArch {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        DefaultRelocationHandler.apply(reloc, place_va, symbol_va, addend, image)
    }
}
