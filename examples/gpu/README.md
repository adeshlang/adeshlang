# GPU Acceleration Examples

AdeshLang supports GPU acceleration for compute-intensive operations.

## Supported Backends

| Backend | Flag | Description |
|---------|------|-------------|
| **CPU** | `--device=cpu` | Default, multi-threaded |
| **CUDA** | `--device=cuda` | NVIDIA GPUs |
| **Metal** | `--device=metal` | Apple Silicon |
| **Vulkan** | `--device=vulkan` | Cross-platform |
| **TPU** | `--device=tpu` | Google TPUs (cloud) |

## Running Examples

### Device Detection

```bash
adeshlang run examples/gpu/device_info.adesh
```

### Matrix Operations on GPU

```bash
# CUDA
adeshlang run examples/gpu/matmul_gpu.adesh --device=cuda

# Metal (macOS)
adeshlang run examples/gpu/matmul_gpu.adesh --device=metal
```

### Parallel Reduction

```bash
adeshlang run examples/gpu/reduction.adesh --device=cuda
```

## Performance Notes

- GPU is beneficial for large data (>10K elements)
- Small operations have transfer overhead
- Use batch operations for best performance
