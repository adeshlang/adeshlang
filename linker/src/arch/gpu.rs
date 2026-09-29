//! GPU Architecture (NVIDIA PTX/CUBIN, AMD ROCm/HSACO, Intel/Vulkan SPIR-V).

use crate::error::LinkResult;
use crate::relocation::{DefaultRelocationHandler, Relocation, RelocationHandler};

pub struct GpuArch;

impl RelocationHandler for GpuArch {
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
