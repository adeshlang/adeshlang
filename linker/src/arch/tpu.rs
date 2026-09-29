//! Google TPU Architecture and compiled XLA execution bundle handler.

use crate::error::LinkResult;
use crate::relocation::{DefaultRelocationHandler, Relocation, RelocationHandler};

pub struct TpuArch;

impl RelocationHandler for TpuArch {
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
