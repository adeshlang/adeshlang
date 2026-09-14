# ML/AI Examples for AdeshLang

This folder contains machine learning and AI examples demonstrating AdeshLang's capabilities.

## Current Status

The ML features are in early development. Current examples demonstrate:
- Basic neural network forward pass computations
- Matrix operations using native arrays
- Activation functions (ReLU, softmax)
- Performance benchmarking

## Running Examples

### Simple Neural Network Forward Pass

```bash
# Run the neural network demo
adeshlang run examples/ml/simple_nn.adesh

# With JIT for better performance
adeshlang run examples/ml/simple_nn.adesh --jit

# With memory profiling
adeshlang run examples/ml/simple_nn.adesh --memory
```

### Other Examples

```bash
# Tensor basics (placeholder demo)
adeshlang run examples/ml/tensor_basics.adesh

# Matrix multiplication (placeholder)
adeshlang run examples/ml/matmul.adesh

# DataLoader demo (placeholder)
adeshlang run examples/ml/dataloader.adesh
```

## Example Output

```
╔════════════════════════════════════════════════════╗
║  SIMPLE NEURAL NETWORK - Forward Pass Demo         ║
╚════════════════════════════════════════════════════╝

Creating MLP(4 -> 3 -> 2)...
Model created!

Creating input [4]...
Input: [0.5, -0.3, 0.8, 0.1]

Running forward pass...

Hidden layer (after ReLU): [0, 0.36, 0]
Output probabilities:
  Class 0: 0.635
  Class 1: 0.365

Performance:
  Forward pass time: 0.03 ms
  1000 iterations in 0.02s
  47000 ops/sec
```

## Planned Features

- **NDArray**: N-dimensional arrays with multiple dtypes (F16, BF16, F32, F64)
- **Memory Layouts**: RowMajor, ColMajor, Strided
- **Device Support**: CPU, CUDA, Metal, Vulkan
- **Dataset & DataLoader**: Full batching and shuffling support
- **Automatic Differentiation**: Gradient computation
- **GPU Acceleration**: CUDA/Metal/Vulkan backends

## CLI Flags

| Flag | Description |
|------|-------------|
| `--jit` | JIT compilation for performance |
| `--memory` | Show memory usage after execution |
| `--profile` | Show execution time |
| `--verbose` | Enable verbose output |
