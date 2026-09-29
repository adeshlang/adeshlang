//! WebAssembly 64-bit Architecture definitions.

use crate::error::LinkResult;
use crate::relocation::{DefaultRelocationHandler, Relocation, RelocationHandler};

pub struct Wasm64Arch;

impl RelocationHandler for Wasm64Arch {
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
