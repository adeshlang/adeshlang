//! PowerPC 64-bit (ppc64 / ppc64le) Architecture definitions and relocation handler.

use crate::error::LinkResult;
use crate::relocation::{DefaultRelocationHandler, Relocation, RelocationHandler};
use crate::target::Endianness;

pub struct Ppc64Arch(pub Endianness);

impl RelocationHandler for Ppc64Arch {
    fn apply(
        &self,
        reloc: &Relocation,
        place_va: u64,
        symbol_va: u64,
        addend: i64,
        image: &mut [u8],
    ) -> LinkResult<()> {
        DefaultRelocationHandler::new(self.0).apply(reloc, place_va, symbol_va, addend, image)
    }
}
