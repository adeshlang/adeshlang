# Target Support Matrix

This is a short orientation table. **[TARGET_MATRIX.md](../TARGET_MATRIX.md) is the
authoritative record** — it carries the per-triple blocker, the date each claim was
measured, and the suite that measures it. Where the two disagree, that file wins.

"Execution verified" means a produced image was *run* and its behavior asserted, not
that its header was decoded. Structural checks are listed as such.

| Target Triple | Object Format | Codegen Status | Linker Status | Execution Verified | External Dependency |
|---------------|---------------|----------------|---------------|-------------------|---------------------|
| `x86_64-unknown-linux-gnu` | ELF64 | Implemented | Implemented | Yes (CI) | None for host builds |
| `x86_64-pc-windows-msvc` | PE32+ | Implemented | Implemented | Yes (CI) | None for host builds |
| `aarch64-unknown-linux-gnu` | ELF64 | Implemented | Implemented | **No** — links a source program (2026-11-04); structure-checked only, no emulator run yet | `gcc-aarch64-linux-gnu` + `libc6-dev-arm64-cross` for the runtime archive |
| `aarch64-apple-darwin` | Mach-O 64 | Implemented | Implemented | **No** — no runtime archive from any job that has run | macOS SDK |
| `riscv64gc-unknown-linux-gnu` | ELF64 | Implemented; reaches the linker from source (2026-11-07) | Implemented | **No** — no target runtime archive exists, so no image to run | `gcc-riscv64-linux-gnu` + `libc6-dev-riscv64-cross` for the runtime archive |
| `riscv32imac-unknown-none-elf` | ELF32 | **Refused** at backend selection (RV64-only) | **Refused** for executables | No | N/A — RV32 lowering not implemented |
| `arm-none-eabi` | ELF32 | Thumb-2 ALU subset | Unit-level only | **No** — no end-to-end firmware link or run | None |
| `wasm32-wasi` | WASM | Implemented | Implemented | Yes (Node + wasmtime in CI) | Node or wasmtime |

Cross-compilation needs a per-target runtime archive under `target/<triple>/`; see
[AOT_COMPILATION.md](AOT_COMPILATION.md) and `scripts/build-runtime-archives.sh`.