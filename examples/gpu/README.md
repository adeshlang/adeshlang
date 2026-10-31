# GPU Acceleration Examples

> **Status: GPU execution is NOT implemented.** These examples are demonstrations of the planned API. The `.adesh` files define hand-written stub functions (see the "Stub functions for demonstration" header in `device_info.adesh` and the "GPU support is currently under development / Placeholder stubs" note in `matmul_gpu.adesh`) — running them executes stubs on the CPU, not kernels on a device. There is no `--device` flag in the CLI at all.

## Real CLI flags

The only GPU-related flags today (`src/toolchain/cli/args.rs`):

| Flag | Status | Notes |
|------|--------|-------|
| `--gpu` | Parsed, **debug builds only** | Selects the experimental MLIR-GPU backend (`#[cfg(debug_assertions)]`) |
| `--gpu-target <value>` / `--gpu-target=<value>` | Parsed | Accepted values: `auto`, `cuda`/`nvidia`/`nvptx`, `rocm`/`amd`/`rocdl`, `vulkan`/`spirv`, `metal`/`apple` (`GpuTarget::from_str` in `src/toolchain/config/mod.rs`) |
| `--device=cpu` … `--device=tpu` | **Not implemented** | No code reads `--device`; TPU is not a target value |

With `--gpu`, the runner lowers to MLIR and writes the module to disk, then calls `gpu::detect_toolchain`. When no external MLIR/CUDA toolchain is present — which is the case on every CI runner and on a default developer machine — it prints `⚠ GPU toolchain not found (MLIR written to …). Falling back to interpreter.` and runs the program through the interpreter (`run_interpreter_fallback`, `src/cli/backends.rs:699-708`). Only when such a toolchain *is* installed does control reach the MLIR pipeline. No GPU driver is loaded by this repository and no device binary (PTX/HSACO/SPIR-V) is generated from Adesh source.

## Running Examples

The examples below run as plain interpreted programs against their own stub functions:

```bash
# Device detection stub (cuda_available() etc. always return the stub values)
adesh run examples/gpu/device_info.adesh

# Matrix multiplication (placeholder stubs, runs sequentially on CPU)
adesh run examples/gpu/matmul_gpu.adesh

# Parallel reduction (placeholder stubs)
adesh run examples/gpu/reduction.adesh

# Experimental MLIR artifact emission (debug build only; execution still
# happens via the interpreter fallback)
adesh run --gpu --gpu-target=cuda examples/gpu/matmul_gpu.adesh
```

`matmul_gpu.mlir` is a hand-written MLIR sample, not compiler output.

## Performance Notes

- Not applicable until device execution exists; see `docs/gpu/gpu-implementation.md` (verification status section) and `TARGET_MATRIX.md` for the current state of the GPU path.
