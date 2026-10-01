# NPU / TPU & AI Accelerator Backend Specification

## 1. Overview

AI accelerators execute graph operations and tensor programs rather than scalar instructions. The Adesh accelerator architecture represents neural compute units cleanly without forcing them into CPU instruction semantics.

---

## 2. Tensor Program Representation

* **Tensor Dimensions & Layout**: Multi-dimensional tensor shapes `Vec<Vec<usize>>` and matrix core tiling dimensions.
* **Element Types**: Floating point (`F16`, `BF16`, `F32`, `F64`, `FP8`), Integer (`I8`, `U8`, `I16`, `I32`), and custom quantization structures.
* **Memory Spaces**: Direct Memory Access (`DmaMemory`), Scratchpad memory, and Persistent model weights.
* **Artifact Formats**: `TensorProgram`, `CommandGraph`, `Firmware`, and `DeviceIR`.
