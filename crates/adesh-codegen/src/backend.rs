//! Universal Target Backend trait definitions.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, NativeModule};
use adesh_object::{AdobObject, TargetCapabilities, TargetDescriptor, TargetFeatures};

/// Universal Code Generation Backend contract.
pub trait CodegenBackend: Send + Sync {
    /// Target descriptor.
    fn target(&self) -> &TargetDescriptor;

    /// Target capabilities exposed by this backend.
    fn capabilities(&self) -> TargetCapabilities;

    /// Lower an entire high-level or intermediate module to a NativeModule.
    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError>;

    /// Generate machine code for a single machine function.
    fn generate_function(&mut self, function: &MachineFunction) -> Result<Vec<u8>, CodegenError>;

    /// Emit a completed, validated ADOB object from the native module.
    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError>;
}

/// Universal Accelerator Backend contract (GPU, NPU, TPU, DSP).
pub trait AcceleratorBackend: Send + Sync {
    /// Target descriptor for the accelerator device.
    fn target(&self) -> &TargetDescriptor;

    /// Compile accelerator kernel code to binary/bytecode.
    fn compile_kernel(
        &mut self,
        kernel_name: &str,
        source_or_ir: &str,
    ) -> Result<Vec<u8>, CodegenError>;

    /// Emit an ADOB kernel object for linking into fat bundles.
    fn emit_kernel_object(
        &mut self,
        kernel_name: &str,
        payload: &[u8],
    ) -> Result<AdobObject, CodegenError>;

    /// Required target hardware features.
    fn required_features(&self) -> TargetFeatures;
}
