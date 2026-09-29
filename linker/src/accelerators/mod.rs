//! Hardware accelerator subsystems (GPU, NPU, TPU).

pub mod gpu;
pub mod npu;

pub use gpu::GpuFatbinWriter;
pub use npu::{NpuContainerWriter, TpuBundleWriter};
