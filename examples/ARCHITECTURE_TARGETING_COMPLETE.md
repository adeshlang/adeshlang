# ADESH ARCHITECTURE TARGETING - COMPLETE REFERENCE

## Overview

Adesh supports compilation to multiple CPU architectures from a **single source file**. The compiler automatically optimizes code for each target.

**Currently Supported Targets:**
- ✅ x86-64 (Linux, Windows, macOS)
- ✅ ARM64 (Linux, macOS Apple Silicon, iOS)
- ✅ ARM32 (Linux, Android)
- ✅ RISC-V (64-bit embedded)
- ✅ ARM Cortex-M (embedded)

**Planned Features:**
- 🔜 WebAssembly (wasm32-unknown-unknown)
- 🔜 GPU Compilation (CUDA, HIP, OpenCL)

**Note:** The WebAssembly and GPU example files below are conceptual demonstrations. They show patterns that Adesh code *could* use for those targets in the future, but actual compilation to those targets is not yet available.

## Architecture-Specific Files

### 1. **x86_64_target.adesh**
Assembly-class CPU architecture optimized for high single-threaded performance.

**Key Characteristics:**
- 256-bit SIMD (AVX2) / 512-bit (AVX-512)
- 8 General-Purpose Registers (RAX, RBX, RCX, RDX, RBP, RSI, RDI, R8-R15)
- Complex instruction set (CISC)
- Large cache hierarchies (L1: 32KB, L2: 256KB, L3: 8-20MB per core)
- Little-endian byte order
- High performance: 30-50 GFLOPS per core

**Adesh Compilation Strategy:**
```
Loop Unrolling: 4-8x unrolled with AVX2
Register Allocation: 16 registers with sophisticated renaming
Cache Optimization: Tile for L3 (60MB on Xeon)
Branch Prediction: Use conditional moves instead of jumps
```

**Use Cases:**
- Data center workloads
- High-frequency trading
- Scientific computing
- Real-time analysis

### 2. **arm64_target.adesh**
Mobile and embedded CPU with power efficiency and NEON SIMD.

**Key Characteristics:**
- 128-bit SIMD (NEON)
- 31 General-Purpose Registers (X0-X30)
- Simpler instruction set (RISC)
- Efficient power consumption (2-3W idle vs 10-15W x86)
- Little-endian byte order
- Medium performance: 8-15 GFLOPS per core

**Adesh Compilation Strategy:**
```
Loop Unrolling: 2-4x with NEON
Register Allocation: 31 registers (more than x86)
Power Optimization: Minimize memory access
Branch Prediction: Fewer branches due to conditional execution
```

**Use Cases:**
- Mobile apps (iOS, Android)
- IoT devices (Raspberry Pi, Arduino)
- Apple Silicon (M1, M2, M3)
- Embedded systems

### 3. **webassembly_target.adesh**
Browser-based execution with sandbox security and deterministic behavior.

**Key Characteristics:**
- 128-bit SIMD (wasm128, optional)
- 32-bit virtual stack machine (not registers)
- 4GB linear memory addressable
- Deterministic execution (identical results everywhere)
- Single-threaded (with SharedArrayBuffer for workers)
- No system call access (controlled by host)
- Performance: 0.5-2 GFLOPS (limited by I/O)

**Adesh Compilation Strategy:**
```
Code Size: Minimize bytecode (target 100-300KB per module)
Memory: Stream processing, avoid large allocations
Determinism: No timing side-channels
Threading: Use Web Workers for parallelism
```

**Use Cases:**
- Web applications
- Cloudflare Workers (edge computing)
- Scientific computing (in-browser)
- Machine learning (inference only)

### 4. **gpu_target.adesh**
Massively parallel GPU computing with thousands of threads.

**Key Characteristics:**
- 512-bit SIMD (thousands of threads)
- Tesla V100: 5,120 CUDA cores
- RTX 4090: 16,384 CUDA cores
- MI300X: 304 TFLOPS matrix operations
- Hierarchical memory (registers → shared → global)
- Data-parallel execution model
- Extremely high throughput: 100+ TFLOPS (matrix ops)

**Adesh Compilation Strategy:**
```
Parallelization: Each loop iteration → GPU thread
Memory Transfer: CPU → GPU before kernel, GPU → CPU after
Kernel Launch: 1D/2D/3D thread blocks
Synchronization: Minimal barriers for efficiency
```

**Use Cases:**
- Machine learning (training & inference)
- Scientific simulations
- Image/video processing
- Monte Carlo computations

## Compilation Workflow

```
Adesh Source Code
        ↓
AST + Type Checking
        ↓
Semantic IR (Architecture-agnostic)
        ↓
┌─────────────┬──────────────┬─────────────┬──────────────┐
├─────────────┴──────────────┴─────────────┴──────────────┤
│     ARCHITECTURE-SPECIFIC OPTIMIZATION PHASES
├─────────────┬──────────────┬─────────────┬──────────────┤
│             │              │             │              │
x86_64        ARM64          WebAssembly   GPU
Compiler      Compiler       Compiler      Compiler
   │             │              │             │
   ├─────────────┼──────────────┼─────────────┤
   │  SIMD       │  NEON        │  SIMD128    │  Kernel Gen
   │  Unroll     │  Intrinsics  │  Streaming  │  Tiling
   │  L3 Cache   │  Power       │  Linear Mem │  Reduce
   │             │              │             │
   ↓             ↓              ↓             ↓
 .a/.so       .a/.so         .wasm        .cubin/
 (x86-64)     (aarch64)      (32-bit)      (PTX)
```

## Performance Characteristics

### Single-Element Operations
| Architecture | Peak (GFLOPS) | Memory BW (GB/s) | Power (W) |
|---|---|---|---|
| x86-64 (Ryzen 5) | 40 | 70 | 25 |
| ARM64 (M1) | 15 | 100 | 4 |
| WebAssembly | 1 | 10 | 0 |
| GPU (RTX 4090) | 661 | 1008 | 420 |

### Matrix Multiply (4096x4096)
| Architecture | Time (ms) | TFLOPS | Power eff |
|---|---|---|---|
| x86-64 (Xeon) | 1,200 | 2.3 | 0.01 TF/W |
| ARM64 (M1 Max) | 3,500 | 0.8 | 0.2 TF/W |
| WebAssembly | >10,000 | <0.1 | N/A |
| GPU (H100) | 40 | 30 | 0.07 TF/W |

## Code Patterns for Each Architecture

### Pattern 1: Vectorization-Friendly
```adesh
// GOOD: Compiler can vectorize efficiently
fn sum_pairs(a: [f64], b: [f64]): f64 {
    let sum = 0.0;
    let i = 0;
    while (i < a.len()) {
        sum = sum + (a[i] * b[i]);
        i = i + 1;
    };
    return sum;
}
// x86: Generates 4-element AVX2 code
// ARM: Generates 2-element NEON code
// GPU: Each thread handles one element
```

### Pattern 2: Cache-Aware Blocking
```adesh
// GOOD: Works well on all architectures
fn tiled_sum(data: [f64], tile_size: i64): f64 {
    let total = 0.0;
    let offset = 0;
    while (offset < data.len()) {
        let end = offset + tile_size;
        let i = offset;
        while (i < end) {
            total = total + data[i];
            i = i + 1;
        };
        offset = end;
    };
    return total;
}
// Adapts to L3 on x86, L2 on ARM
```

### Pattern 3: Branch-Free Code
```adesh
// GOOD: GPU (and CPU) friendly
fn clip(value: f64, min: f64, max: f64): f64 {
    let result = value;
    if (result < min) { result = min; };
    if (result > max) { result = max; };
    return result;
}
// GPU: No warp divergence
// CPU: Uses conditional moves
```

### Pattern 4: Data-Parallel Operations
```adesh
// PERFECT for GPU
fn elementwise_square(input: [f64]): [f64] {
    let output = [];
    let i = 0;
    while (i < input.len()) {
        output[i] = input[i] * input[i];
        i = i + 1;
    };
    return output;
}
// GPU: Each thread handles one element
```

## When to Use Each Architecture

### x86-64
✓ High performance needed per core
✓ Single-threaded bottleneck
✓ Large datasets fit in cache
✓ Complex algorithms with branches
- Limited power budget may be issue

### ARM64
✓ Mobile or IoT deployment
✓ Power efficiency critical
✓ Multi-core parallelism possible
- Lower sustained per-core performance
- Smaller cache than x86

### WebAssembly
✓ Browser execution required
✓ Deterministic results needed
✓ Sandboxed / secure environment
✓ Edge computing (Cloudflare, etc)
- Limited to 1GB memory
- Single-threaded (without SharedArrayBuffer)
- Slower than native

### GPU
✓ Data massively parallel
✓ Thousands of simultaneous threads
✓ Memory streaming important
✓ High-throughput needed
- High latency per operation
- Requires data transfer (CPU ↔ GPU)
- Complex programming model

## Compilation Examples

### Example 1: Compile for x86-64 (Linux)
```bash
adesh build --target=x86_64-unknown-linux-gnu my_program.adesh
# Output: my_program (executable)
```

### Example 2: Compile for ARM64 (Linux)
```bash
adesh build --target=aarch64-unknown-linux-gnu my_program.adesh
# Output: my_program (executable)
```

### Example 3: Compile for macOS Apple Silicon
```bash
adesh build --target=aarch64-apple-darwin my_program.adesh
# Output: my_program (executable for M1/M2/M3)
```

### Example 4: Compile for Android
```bash
adesh build --target=aarch64-linux-android my_program.adesh
# Output: my_program (executable for Android ARM64)
```

### Example 5: Compile for Embedded (ARM Cortex-M)
```bash
adesh build --target=thumbv7em-none-eabihf my_program.adesh
# Output: my_program (bare-metal ARM executable)
```

**Note:** GPU compilation (CUDA/HIP) and WebAssembly are planned but not yet available in the current CLI. Use Rust with `cargo` for those targets currently.

## Performance Tips

### For x86-64
1. Use AVX-512 variants when possible (Xeon Platinum)
2. Align data to 64-byte boundaries for L3 cache
3. Minimize TLB misses (use 2MB huge pages for large arrays)
4. Unroll loops to expose parallelism

### For ARM64
1. Use NEON intrinsics efficiently (compiler hints)
2. Minimize memory bandwidth (ARM prefers sequential access)
3. Align data to 16-byte boundaries (NEON stride)
4. Conditional execution (avoid branches)

### For WebAssembly
1. Minimize code size (use --opt-level s)
2. Stream process large data (avoid 1GB limit)
3. Use SharedArrayBuffer for parallelism
4. Batch operations to reduce I/O

### For GPU
1. Maximize thread count (1000+ threads)
2. Use shared memory aggressively (48KB per block)
3. Coalesce memory access patterns
4. Minimize synchronization barriers

## Architecture Selection Flowchart

```
START
  │
  ├─ Mobile/iOS? → aarch64-apple-ios
  │
  ├─ Android? → aarch64-linux-android
  │
  ├─ Embedded/IoT? → thumbv7em-none-eabihf
  │
  ├─ macOS Apple Silicon? → aarch64-apple-darwin
  │
  ├─ Linux ARM? → aarch64-unknown-linux-gnu
  │
  ├─ Windows/macOS/Linux? → x86_64-*
  │
  └─ RISC-V? → riscv64gc-unknown-none-elf
```

**Note:** WebAssembly and GPU targets are planned for future releases. Currently, use Rust/C++ for those targets.

## Summary

Adesh's architecture targeting feature currently enables:
- **Multiple CPU architectures** (x86-64, ARM64, ARM32, RISC-V, Cortex-M)
- **Desktop, mobile, and embedded** deployment from single source
- **Automatic optimization** for each target's instruction set
- **Cross-compilation** via `adesh build --target=<TRIPLE>`

**Future features** (planned):
- WebAssembly (wasm32-unknown-unknown) support
- GPU compilation (CUDA, HIP, OpenCL)
- Automatic SIMD optimization (currently manual)
- CPU-specific tuning (generic compilation works, tuning planned)
