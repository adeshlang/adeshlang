# Universal CodeGen Backend Architecture

## 1. Overview

The `adesh-codegen` crate implements a modular, extensible code generation framework. Every backend (CPU, GPU, NPU, TPU, DSP, Embedded) adheres to unified interface contracts:

* **`CodegenBackend`**: Standard backend interface for CPUs and general-purpose architectures.
* **`AcceleratorBackend`**: Specialized backend interface for GPU kernels, neural graphs, and tensor processors.

---

## 2. Compilation Stages

```text
       Adesh LIR (SSA Form)
                │
                ▼
      Target-Independent LIR Optimization
(Constant Folding, Dead-Code Elimination, CSE)
                │
                ▼
         Target Lowering
                │
                ▼
          Machine IR
                │
                ▼
   Register Allocation (Linear Scan)
                │
                ▼
 Target-Specific Instruction Encoding
                │
                ▼
      ADOB Object Generation
```

---

## 3. Core Traits & Components

### `CodegenBackend` Trait
```rust
pub trait CodegenBackend {
    fn target(&self) -> &TargetDescriptor;
    fn capabilities(&self) -> TargetCapabilities;
    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError>;
    fn generate_function(&mut self, function: &MachineFunction) -> Result<Vec<u8>, CodegenError>;
    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError>;
}
```

### `RegisterFile` Trait
```rust
pub trait RegisterFile {
    fn registers(&self) -> &[PhysicalRegister];
    fn allocatable(&self) -> &[PhysicalRegister];
    fn caller_saved(&self) -> &[PhysicalRegister];
    fn callee_saved(&self) -> &[PhysicalRegister];
    fn reserved(&self) -> &[PhysicalRegister];
}
```

### `CallingConvention` Trait
```rust
pub trait CallingConvention {
    fn name(&self) -> &'static str;
    fn arg_registers(&self) -> &[PhysicalRegister];
    fn return_registers(&self) -> &[PhysicalRegister];
    fn caller_saved_registers(&self) -> &[PhysicalRegister];
    fn callee_saved_registers(&self) -> &[PhysicalRegister];
    fn stack_alignment(&self) -> u32;
}
```
