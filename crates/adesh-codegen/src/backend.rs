//! Universal Target Backend trait definitions.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, NativeModule};
use crate::opt::OptLevel;
use crate::opt::pgo::{PgoConfig, ProfileData};
use adesh_object::{
    AdobObject, AdobRelocation, TargetCapabilities, TargetDescriptor, TargetFeatures,
};

/// Universal Code Generation Backend contract.
pub trait CodegenBackend: Send + Sync {
    /// Target descriptor.
    fn target(&self) -> &TargetDescriptor;

    /// Target capabilities exposed by this backend.
    fn capabilities(&self) -> TargetCapabilities;

    /// Set optimization level for the backend.
    fn set_opt_level(&mut self, _opt_level: OptLevel) {}

    /// Configure profile-guided optimization (instrumentation counters or
    /// profile consumption). Default: unsupported on this backend.
    fn set_pgo(
        &mut self,
        _config: PgoConfig,
        _profile: Option<ProfileData>,
    ) -> Result<(), CodegenError> {
        Err(CodegenError::new(
            "unknown",
            "this backend does not support profile-guided optimization",
        ))
    }

    /// Lower an entire high-level or intermediate module to a NativeModule.
    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError>;

    /// Generate machine code for a single machine function.
    fn generate_function(&mut self, function: &MachineFunction) -> Result<Vec<u8>, CodegenError>;

    /// Generate machine code plus the relocations the function body requires.
    ///
    /// Relocation offsets are relative to the start of the returned code.
    /// Backends that cannot emit relocations inherit the default, which
    /// reports none.
    fn generate_function_with_relocations(
        &mut self,
        function: &MachineFunction,
    ) -> Result<(Vec<u8>, Vec<AdobRelocation>), CodegenError> {
        Ok((self.generate_function(function)?, Vec::new()))
    }

    /// Emit a completed, validated ADOB object from the native module.
    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError>;

    /// Generate direct native target assembly code for the module.
    fn generate_assembly(&mut self, module: &NativeModule) -> Result<String, CodegenError> {
        let lowered = self.lower_module(module)?;
        Ok(format!("# Assembly output for {}\n", lowered.name))
    }
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
