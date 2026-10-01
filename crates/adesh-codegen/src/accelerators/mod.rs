//! Accelerator Backends: GPU (CUDA, AMD, Apple, Intel, Generic SPIR-V), NPU, and TPU.

use crate::backend::AcceleratorBackend;
use crate::error::CodegenError;
use adesh_object::{
    AcceleratorArtifact, AcceleratorMetadata, AdobObject, AdobSection, AdobSymbol, ComputeDevice,
    GpuArchitecture, GpuKernelMetadata, NpuArchitecture, SectionKind, SymbolBinding, SymbolKind,
    SymbolVisibility, TargetDescriptor, TargetFeatures, TensorElementType, TensorMetadata,
    TpuArchitecture,
};

/// GPU Accelerator backend emitting GPU kernel payloads and ADOB kernel objects.
pub struct GpuBackend {
    pub target: TargetDescriptor,
}

impl GpuBackend {
    pub fn new(arch: GpuArchitecture) -> Self {
        let mut target = TargetDescriptor::default();
        target.device = ComputeDevice::Gpu;
        target.architecture = adesh_object::Architecture::Gpu(arch);
        Self { target }
    }
}

impl AcceleratorBackend for GpuBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn compile_kernel(
        &mut self,
        _kernel_name: &str,
        source_or_ir: &str,
    ) -> Result<Vec<u8>, CodegenError> {
        // Embed SPIR-V / PTX or compiled bytecode payload
        Ok(source_or_ir.as_bytes().to_vec())
    }

    fn emit_kernel_object(
        &mut self,
        kernel_name: &str,
        payload: &[u8],
    ) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        let mut meta = GpuKernelMetadata::new(kernel_name);
        meta.shared_memory_bytes = 1024;
        meta.register_count = 32;
        meta.grid_dimensions = (64, 1, 1);
        meta.block_dimensions = (256, 1, 1);

        obj.accelerator = AcceleratorMetadata {
            artifact: AcceleratorArtifact::KernelBinary,
            gpu_kernels: vec![meta],
            tensors: Vec::new(),
            device_memory_model: vec![
                adesh_object::DeviceMemoryModel::SharedMemory,
                adesh_object::DeviceMemoryModel::DeviceMemory,
            ],
        };

        let sec = AdobSection::new(".gpu_kernel", SectionKind::Text).with_data(payload.to_vec());
        obj.add_section(sec);

        let sym = AdobSymbol::new_defined(
            0,
            kernel_name,
            SymbolKind::AcceleratorKernel,
            0,
            0,
            payload.len() as u64,
        )
        .with_binding(SymbolBinding::Global)
        .with_visibility(SymbolVisibility::Default);
        obj.add_symbol(sym);
        obj.add_export(kernel_name);

        Ok(obj)
    }

    fn required_features(&self) -> TargetFeatures {
        TargetFeatures::new()
    }
}

/// NPU / TPU Accelerator Backend emitting Tensor graphs and accelerator metadata.
pub struct TensorAcceleratorBackend {
    pub target: TargetDescriptor,
}

impl TensorAcceleratorBackend {
    pub fn npu(arch: NpuArchitecture) -> Self {
        let target = TargetDescriptor {
            device: ComputeDevice::Npu,
            architecture: adesh_object::Architecture::Npu(arch),
            ..Default::default()
        };
        Self { target }
    }

    pub fn tpu(arch: TpuArchitecture) -> Self {
        let target = TargetDescriptor {
            device: ComputeDevice::Tpu,
            architecture: adesh_object::Architecture::Tpu(arch),
            ..Default::default()
        };
        Self { target }
    }
}

impl AcceleratorBackend for TensorAcceleratorBackend {
    fn target(&self) -> &TargetDescriptor {
        &self.target
    }

    fn compile_kernel(
        &mut self,
        _kernel_name: &str,
        source_or_ir: &str,
    ) -> Result<Vec<u8>, CodegenError> {
        Ok(source_or_ir.as_bytes().to_vec())
    }

    fn emit_kernel_object(
        &mut self,
        kernel_name: &str,
        payload: &[u8],
    ) -> Result<AdobObject, CodegenError> {
        let mut obj = AdobObject::new(self.target.clone());

        let mut tensor = TensorMetadata::new(kernel_name);
        tensor.element_types = vec![TensorElementType::F32, TensorElementType::BF16];
        tensor.matrix_dimensions = (128, 128);

        obj.accelerator = AcceleratorMetadata {
            artifact: AcceleratorArtifact::TensorProgram,
            gpu_kernels: Vec::new(),
            tensors: vec![tensor],
            device_memory_model: vec![
                adesh_object::DeviceMemoryModel::DmaMemory,
                adesh_object::DeviceMemoryModel::DeviceMemory,
            ],
        };

        let sec =
            AdobSection::new(".tensor_graph", SectionKind::Custom).with_data(payload.to_vec());
        obj.add_section(sec);

        let sym = AdobSymbol::new_defined(
            0,
            kernel_name,
            SymbolKind::AcceleratorKernel,
            0,
            0,
            payload.len() as u64,
        )
        .with_binding(SymbolBinding::Global);
        obj.add_symbol(sym);
        obj.add_export(kernel_name);

        Ok(obj)
    }

    fn required_features(&self) -> TargetFeatures {
        TargetFeatures::new()
    }
}
