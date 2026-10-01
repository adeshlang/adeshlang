# Universal Target CodeGen & Machine IR

Adesh native code generation transitions High-level Intermediate Representation (HIR) and Low-level Intermediate Representation (LIR) into target machine instructions and relocatable **ADOB** objects without depending on LLVM or Cranelift as canonical object generators.

---

## 🏛️ Pipeline Architecture

```text
Adesh Source
     │
     ▼
  AST → HIR → LIR
     │
     ▼
Target Lowering
     │
     ▼
 Machine IR (Target-Neutral)
     │
 ┌───┴────────────────────────────────────────┐
 │   Optimization & Safety Passes             │
 │   • Peephole Optimizer (Strength Reduction)│
 │   • Dead Code Elimination (DCE)            │
 │   • Array Bounds Check Elimination (BCE)   │
 │   • Stack Canary Guard Instrumentation     │
 │   • Control Flow Integrity (CFI/CET/BTI)   │
 │   • Concurrency & Memory Order Barried     │
 └───┬────────────────────────────────────────┘
     │
 Register Allocation (Linear Scan)
     │
 Instruction Selection & Encoders
     │
 ┌───┴────────────────────────────────────────┐
 │  Target Backends                           │
 │  • x86_64 Backend                          │
 │  • AArch64 Backend                         │
 │  • RISC-V Backend (32-bit & 64-bit)        │
 │  • WASM Backend                            │
 │  • Embedded / MCU Backend                  │
 │  • Accelerators (GPU, NPU, TPU)            │
 └───┬────────────────────────────────────────┘
     │
     ▼
 ADOB Object Container
     │
     ▼
 Adesh Linker (`adeshlink`)
     │
 PE / ELF / Mach-O Native Binary
```

---

## ⚙️ Key Subsystems

### 1. Target Backend Contracts (`CodegenBackend`)
All architecture backends implement the `CodegenBackend` trait:

```rust
pub trait CodegenBackend: Send + Sync {
    fn target(&self) -> &TargetDescriptor;
    fn capabilities(&self) -> TargetCapabilities;
    fn lower_module(&mut self, module: &NativeModule) -> Result<NativeModule, CodegenError>;
    fn generate_function(&mut self, function: &MachineFunction) -> Result<Vec<u8>, CodegenError>;
    fn emit_object(&mut self, module: &NativeModule) -> Result<AdobObject, CodegenError>;
}
```

### 2. Linear Scan Register Allocator
- Target-agnostic linear scan algorithm that calculates live intervals for virtual registers (`VirtualRegister`) and allocates them to physical registers (`PhysicalRegister`) using architecture-specific `RegisterFile` descriptions.
- Automatically handles spill slots on the stack frame when register pressure exceeds capacity.

### 3. Calling Conventions
Architecture calling conventions are modeled via the `CallingConvention` trait:
- **Windows x64:** RCX, RDX, R8, R9 integer register parameters, 32-byte shadow space.
- **System V AMD64 (Linux/macOS):** RDI, RSI, RDX, RCX, R8, R9 integer register parameters, 128-byte red zone.
- **AAPCS64 (ARM64):** X0–X7 integer parameters, X8 indirect result location, SP 16-byte alignment.
- **RISC-V (RV64GC/RV32):** A0–A7 function arguments, RA return address.
- **WASM:** Stack-based parameter passing.

### 4. Binary-Level Optimizations & Passes
- **Peephole Optimizations:**
  - Algebraic simplifications: $x + 0 \to x$, $x \times 0 \to 0$, $x \times 1 \to x$, $x - 0 \to x$.
  - Redundant move elimination: `mov rax, rax` $\to$ `nop`.
  - Strength reduction: $x \times 2^k \to x \ll k$.
- **Dead Code Elimination (DCE):**
  - Removes unreachable basic blocks and dead virtual register definitions.
- **Array Bounds Check Elimination (BCE):**
  - Statically eliminates runtime bounds checks when indexing constant arrays within known bounds.
- **Memory & Runtime Safety:**
  - Stack Canary: Emits canary prologue (`mov rax, [__stack_chk_guard]; mov [rbp - 8], rax`) and epilogue check calling `__stack_chk_fail`.
  - Control Flow Integrity: Emits `endbr64` (Intel CET) on x86_64 and BTI landing pads on AArch64.
