# GPU Accelerator Backend Specification

## 1. Overview

The GPU backend compiles computational kernels targeting heterogeneous accelerators (NVIDIA CUDA, AMD RDNA/GCN, Apple Silicon GPU, Intel Xe, and Generic SPIR-V).

---

## 2. Kernel Metadata Representation

GPU kernels carry specialized attributes in ADOB:
* **Grid & Block Dimensions**: `(grid_x, grid_y, grid_z)` and `(block_x, block_y, block_z)`.
* **Memory Resources**: Shared memory requirement (bytes), constant buffer requirement, and register allocation budget.
* **Device Memory Model**: Classification of memory spaces (`HostMemory`, `DeviceMemory`, `SharedMemory`, `ConstantMemory`).
* **Artifact Formats**: Native device binary (PTX/SASS, AMDGPU ELF, SPIR-V, Metal Bytecode).
