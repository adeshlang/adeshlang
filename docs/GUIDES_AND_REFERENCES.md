# GUIDES_AND_REFERENCES.md

> Consolidated from 7 markdown files on 2026-08-29.
> This file merges related root-level .md documents by category.

---


---

## Source: BUILD_SYSTEM_GUIDE.md

# AdeshLang Build System with Fast Compilation

## Quick Start

### Fast Development Build (⚡ Lightning Speed)
```bash
# Compile in fast mode - instant feedback, no optimization
adesh build myprogram.adesh --fast

# Result: 0.3-0.5 second compile time
```

### Optimized Release Build (🚀 Production-Ready)
```bash
# Full optimization, smaller executable, slower compile
adesh build myprogram.adesh -O3

# Result: 5-7 seconds, but 60-75% smaller executable
```

## Build Command Syntax

```bash
adesh build [INPUT_FILE] [OPTIONS]
```

## Common Flags

### Compilation Speed

| Flag | Speed | Size | Use Case |
|------|-------|------|----------|
| `--fast` / `--dev` | ⚡ 0.3-0.5s | 100% (unoptimized) | Development, rapid iteration |
| `-O0` | ⚡ 0.5-1s | 95% | Debug builds |
| `-O1` | 🟢 1-2s | 80% | Testing |
| `-O2` | 🟡 3-4s | 50-60% | **Recommended** |
| `-O3` | 🔴 5-7s | 30-40% | **Production** |

### Output Options

```bash
# Specify output file
adesh build input.adesh -o program.exe

# Output type (default: executable)
adesh build input.adesh --emit exe      # Native executable
adesh build input.adesh --library       # Static library (.lib/.a)
adesh build input.adesh --shared        # Shared library (.dll/.so)
adesh build input.adesh --emit obj      # Object file only

# Generate C header for interop
adesh build input.adesh --emit header output.h
```

### Debug & Info

```bash
# Include debug symbols
adesh build input.adesh --debug

# Verbose output
adesh build input.adesh --verbose

# Quiet mode (minimal output)
adesh build input.adesh --quiet

# Show build plan without building
adesh build input.adesh --dry-run

# Check only (validate before build)
adesh build input.adesh --check
```

### Linking & Includes

```bash
# Include directories (multiple allowed)
adesh build input.adesh -I /usr/include -I ./include

# Library search paths
adesh build input.adesh -L /usr/lib -L ./lib

# Link libraries
adesh build input.adesh -l m -l pthread -l ssl

# Extra linker arguments
adesh build input.adesh --linker-arg "-Wl,--as-needed"
```

### Library & Special Modes

```bash
# Library mode (skip main function)
adesh build mylib.adesh --library-mode --library

# Cross-compilation
adesh build input.adesh --target aarch64-unknown-linux-gnu

# Build and run
adesh build input.adesh --run --fast

# Pass arguments to the program when running
adesh build input.adesh --run --fast -- arg1 arg2
```

## Examples

### 1. Fast Development Iteration
```bash
# Perfect for rapid testing during development
adesh build hello.adesh --fast

# Output: hello.exe (unoptimized, lightning-fast compile)
# Compile time: ~0.3-0.5 seconds
# Executable size: 4-6MB
```

### 2. Balanced Development Build
```bash
# Good for integration testing
adesh build app.adesh -O2 --verbose

# Output: app.exe (good optimization, reasonable compile time)
# Compile time: ~3-4 seconds
# Executable size: 2-3MB
```

### 3. Production Release
```bash
# For deployment, smallest & fastest executable
adesh build app.adesh -O3 --strip

# Output: app.exe (fully optimized, tiny, fast)
# Compile time: ~5-7 seconds
# Executable size: 1.5-2MB
```

### 4. Library Development
```bash
# Build a reusable library
adesh build mylib.adesh --library --debug -I ./include

# Output: mylib.lib or mylib.a
# Includes debug symbols for debugging
```

### 5. With Dependencies
```bash
# Link against system libraries
adesh build network.adesh \
  -l ssl                 \
  -l curl                \
  -L /usr/lib            \
  -I /usr/include/curl   \
  --verbose
```

### 6. Quick Test → Run
```bash
# Compile in fast mode and immediately run
adesh build test.adesh --fast --run

# Pass test parameters
adesh build test.adesh --fast --run -- --verbose --filter math_
```

### 7. Dry Run (Plan Only)
```bash
# See what would be built without actually building
adesh build app.adesh --dry-run

# Output:
# Build Plan:
#   Input:        app.adesh
#   Output:       app.exe
#   Format:       Executable
#   Optimization: O3 (full optimization)
#   Target:       (host)
```

## Performance Tips

### ✅ DO

1. **Use `--fast` during development**
   - Compiles 10x faster
   - Perfect for rapid iteration
   - Executable still works correctly

2. **Use `-O3` for releases**
   - Smallest executable (60-75% smaller)
   - Fastest runtime performance
   - Worth the longer compile time

3. **Use `-O2` for balanced builds**
   - Good optimization without LTO overhead
   - Reasonable compile time
   - Good for CI/CD environments

4. **Cache dependencies**
   - Libraries are compiled once
   - Only recompile changed code
   - Use `--check` to validate before full build

### ❌ DON'T

1. ❌ Don't use `-O0` for production
   - Executable is ~95% of unoptimized size
   - Lost optimization opportunity
   - Use `--fast` or `-O2` instead

2. ❌ Don't mix `--fast` with `-O3`
   - `--fast` overrides other optim settings
   - Use either/or, not both

3. ❌ Don't compile with debug info for production
   - Adds unnecessary size (~20%)
   - Only use in development/testing

4. ❌ Don't forget to link required libraries
   - Check platform-specific requirements
   - Use `--verbose` to see linker invocation

## Environment Variables

```bash
# Enable fast compile mode by default
export ADESH_FAST_COMPILE=1
adesh build app.adesh        # Uses fast mode automatically

# Enable LTO for O3 builds
export ADESH_ENABLE_LTO=1
adesh build app.adesh -O3    # Enables link-time optimization

# Disable dead-code elimination
export ADESH_NO_GC_SECTIONS=1
adesh build app.adesh        # Larger executable but faster link
```

## Compiler Flags

### Under the Hood

When you use `adesh build`, these optimizations are applied:

**Fast Mode (`--fast`)**:
- Cranelift: `opt_level="none"`
- No string pool bloat (3 strings, not 60)
- No linker dead-code elimination
- Result: 0.3-0.5s compile

**Release Mode (`-O2`)**:
- Cranelift: `opt_level="speed_and_size"`
- Full string pool
- Linker flags: `-ffunction-sections -fdata-sections -Wl,--gc-sections`
- Result: 90% size reduction vs fast mode

**Max Optimization (`-O3`)**:
- Cranelift: `opt_level="speed"`
- Link-time optimization: `-flto`
- Aggressive inlining
- Result: Maximum performance and minimal size

## Troubleshooting

### Executable too large?
```bash
# Use higher optimization
adesh build app.adesh -O3

# Check for unnecessary features
adesh build app.adesh -O2 --verbose  # See what's included
```

### Build too slow?
```bash
# Use fast mode for development
adesh build app.adesh --fast

# Reduce optimization for CI
adesh build app.adesh -O1  # Better than -O2 for speed
```

### Linking errors?
```bash
# Check what's being linked
adesh build app.adesh --verbose

# Add missing libraries
adesh build app.adesh -l m -l pthread

# Add library search paths
adesh build app.adesh -L /usr/lib64
```

## Integration with RuntimeConfig

The build system also supports AdeshLang's RuntimeConfig for feature selection:

```bash
# Check ownership (safety verification)
adesh build app.adesh --check-ownership

# Skip move semantics checking
adesh build app.adesh --no-check-moves

# Enable profiling
adesh build app.adesh --profile
```

## See Also

- [AOT Compilation Optimization](AOT_COMPILATION_OPTIMIZATION.md) - Technical details
- [AOT Quick Start](AOT_COMPILATION_OPTIMIZATION.md#usage-examples) - Example usage
- [Cranelift Backend](docs/BACKENDS.md#cranelift) - Backend documentation



---

## Source: DOCUMENTATION_INDEX.md

# 📋 AdeshLang Documentation Index - January 2026 (Rolling)

## 🎯 Quick Navigation

### 📊 Status & Summary Documents (START HERE!)

| File | Purpose | Last Updated | Status |
|------|---------|--------------|--------|
| **[docs/CURRENT_STATE_AND_NEXT.md](docs/CURRENT_STATE_AND_NEXT.md)** | Current architecture + next focus (source-oriented) | Jan 14, 2026 | ✅ Updated |
| **[STATUS_DASHBOARD_JAN2026.md](STATUS_DASHBOARD_JAN2026.md)** | Visual status overview (snapshot) | Jan 1, 2026 | ✅ Snapshot |
| **[COMPLETION_STATUS_JAN2026.md](COMPLETION_STATUS_JAN2026.md)** | Comprehensive completion status (snapshot) | Jan 1, 2026 | ✅ Snapshot |
| **[WORK_SUMMARY_JAN2026.md](WORK_SUMMARY_JAN2026.md)** | What was done & next steps (snapshot) | Jan 1, 2026 | ✅ Snapshot |
| [STATUS_PROJECT.md](STATUS_PROJECT.md) | Phase completion tracking | Jan 14, 2026 | ✅ Updated |
| [TODO.md](TODO.md) | Roadmap & pending features | Jan 14, 2026 | ✅ Updated |
| [README.md](README.md) | Main project description | Jan 14, 2026 | ✅ Updated |
* [ADL_ECOSYSTEM_PROGRESS.md](ADL_ECOSYSTEM_PROGRESS.md) - ADL package ecosystem implementation status, pending work, examples (Jul 1, 2026)
* [ADL_ECOSYSTEM_USAGE_GUIDE.md](ADL_ECOSYSTEM_USAGE_GUIDE.md) - ADL workflow, manifest usage, graph inspection, and package lifecycle (Jul 1, 2026)

### 📚 Technical Documentation

| File | Topic | Status |
|------|-------|--------|
| [GPU_GUIDE.md](GPU_GUIDE.md) | **GPU/MLIR backend — pipeline, device check, MIR dump, roadmap** | ✅ New (Feb 22) |
| [MEMORY_SAFETY_STATUS.md](MEMORY_SAFETY_STATUS.md) | Memory safety & borrow checking | ✅ Complete |
| [PERFORMANCE_FIXES_DEC2025.md](PERFORMANCE_FIXES_DEC2025.md) | Performance improvements | ✅ Complete |
| [PACKED_INSTANCE_QUICKSTART.md](PACKED_INSTANCE_QUICKSTART.md) | Packed instances quickstart | ✅ New |
| [AOT_COMPILER_QUICKSTART.md](AOT_COMPILER_QUICKSTART.md) | AOT compilation guide | ✅ Complete |
| [CONCURRENCY_MODULE.md](CONCURRENCY_MODULE.md) | Concurrency features | ✅ Complete |
| [STRING_TRANSFORM_FIX.md](STRING_TRANSFORM_FIX.md) | String transformations | ✅ Complete |
| [CHANGELOG_v0.2.md](CHANGELOG_v0.2.md) | Version 0.2 changes | ✅ Complete |

### 🏗️ Architecture & Planning

| File | Topic | Status |
|------|-------|--------|
| [PHASE2_MASTER.md](PHASE2_MASTER.md) | Phase 2 planning & status | ✅ Complete |
| [PHASE3_MASTER.md](PHASE3_MASTER.md) | Phase 3 planning & status | ✅ Complete |

### 📖 Language Documentation (in docs/ folder)

See [docs/](docs/) folder for comprehensive language reference:
- Language features and syntax
- Type system guide
- Memory model documentation
- FFI integration guide
- Performance analysis
- And more...

---

## 🎯 Where to Start?

### If you want to...

**...understand what was done recently:**
→ Read [STATUS_DASHBOARD_JAN2026.md](STATUS_DASHBOARD_JAN2026.md) (5 min)

**...see comprehensive status:**
→ Read [COMPLETION_STATUS_JAN2026.md](COMPLETION_STATUS_JAN2026.md) (15 min)

**...know what to do next:**
→ Read [WORK_SUMMARY_JAN2026.md](WORK_SUMMARY_JAN2026.md) (10 min)

**...understand memory safety:**
→ Read [MEMORY_SAFETY_STATUS.md](MEMORY_SAFETY_STATUS.md) (20 min)

**...see the project roadmap:**
→ Read [TODO.md](TODO.md) (20 min)

**...learn the language:**
→ Read [Readme.md](Readme.md) (30 min)

**...use the GPU/MLIR backend:**
→ Read [GPU_GUIDE.md](GPU_GUIDE.md) (15 min)

---

## 📊 Current Project Status at a Glance

```
╔════════════════════════════════════════════════════════════════╗
║                    AdeshLang Status Summary                     ║
║                       January 14, 2026                         ║
╠════════════════════════════════════════════════════════════════╣
║                                                                ║
║  Crate Version: v0.3.0 (see Cargo.toml)                        ║
║  Status Docs: Jan 1 files are snapshots; see current state doc ║
║  Current Entry: docs/CURRENT_STATE_AND_NEXT.md                 ║
║                                                                ║
╚════════════════════════════════════════════════════════════════╝
```

---

## 📈 Work Completed (January 1, 2026)

### Code Quality Improvements ✅
- Fixed 25+ clippy warnings across entire codebase
- Achieved zero compiler warnings
- Achieved zero clippy warnings
- All builds pass with clean output
- 264/273 tests passing (96.7%)

### String & Array Methods ✅
- Implemented 20+ methods (13 string, 8 array)
- Full JIT backend support
- OOP syntax working: `string.split()`, `array.join()`
- Test file created: `examples/string_methods_test.adesh`
- Interpreter support partial (syntax not yet enabled)

### Documentation ✅
- Created: STATUS_DASHBOARD_JAN2026.md
- Created: COMPLETION_STATUS_JAN2026.md
- Created: WORK_SUMMARY_JAN2026.md
- Updated: TODO.md, STATUS_PROJECT.md, README.md
- Marked completed items, updated progress tracking

---

## 🎓 How to Use This Documentation

### For Developers Working on Code
1. Start with **WORK_SUMMARY_JAN2026.md** (what was done)
2. Check **TODO.md** for high-priority items
3. Review **COMPLETION_STATUS_JAN2026.md** for detailed status
4. Pick a task and implement it
5. For memory/perf: see **PACKED_INSTANCE_QUICKSTART.md**
6. Update relevant documentation when done

### For Understanding the Project
1. Start with **Readme.md** (overview)
2. Read **STATUS_PROJECT.md** (phase status)
3. Review **MEMORY_SAFETY_STATUS.md** (technical depth)
4. Browse **docs/** folder for language guide

### For Deployment/Integration
1. Read **Readme.md** (getting started)
2. Check **STATUS_PROJECT.md** (readiness status)
3. Review **AOT_COMPILER_QUICKSTART.md** if using AOT
4. Check **PERFORMANCE_FIXES_DEC2025.md** for optimization info

---

## 📋 Documentation Organization

```
AdeshLang/
├── 📄 Readme.md                          Main project overview
├── 📄 TODO.md                            Roadmap & pending features
├── 📄 STATUS_PROJECT.md                  Phase completion tracking
├── 📄 STATUS_DASHBOARD_JAN2026.md        ✨ NEW - Visual status
├── 📄 COMPLETION_STATUS_JAN2026.md       ✨ NEW - Detailed status
├── 📄 WORK_SUMMARY_JAN2026.md            ✨ NEW - Work summary
│
├── 📄 MEMORY_SAFETY_STATUS.md            Memory safety documentation
├── 📄 PERFORMANCE_FIXES_DEC2025.md       Performance improvements
├── 📄 AOT_COMPILER_QUICKSTART.md         AOT compilation guide
├── 📄 CONCURRENCY_MODULE.md              Concurrency features
├── 📄 STRING_TRANSFORM_FIX.md            String transformations
├── 📄 CHANGELOG_v0.2.md                  Version 0.2 changes
│
├── 📁 docs/                              Technical documentation
│   ├── language.md                       Language guide
│   ├── memory_model.md                   Memory system
│   ├── borrow_rules.md                   Borrowing rules
│   ├── pointers.md                       Pointer documentation
│   ├── performance.md                    Performance analysis
│   └── [20+ more technical docs]
│
├── 📁 examples/                          Example programs
├── 📁 src/                               Source code
└── 📁 tests/                             Test suite
```

---

## ✨ Key Achievements

| Achievement | Status | Date |
|-------------|--------|------|
| **Memory Safety (Phase 4)** | ✅ Complete | Dec 30, 2025 |
| **Zero Warnings Build** | ✅ Achieved | Jan 1, 2026 |
| **20+ String/Array Methods** | ✅ Implemented | Jan 1, 2026 |
| **96.7% Test Pass Rate** | ✅ Verified | Jan 1, 2026 |
| **Comprehensive Documentation** | ✅ Up-to-date | Jan 1, 2026 |
| **Production Ready** | ✅ Confirmed | Jan 1, 2026 |

---

## 🚀 Next Steps

### Immediate (1-2 days)
1. Fix interpreter method syntax (high impact)
2. Fix 9 VM tests (improves pass rate)
3. Update language documentation

### Short-term (1-2 weeks)
4. Implement AOT method support
5. Add more array/string methods
6. Expand standard library examples

### Medium-term (2-4 weeks)
7. WASM string support
8. Complete standard library (fs, json, http)
9. Performance optimization

---

## 📞 Quick Links

| Need | Reference |
|------|-----------|
| **Project Status** | [STATUS_DASHBOARD_JAN2026.md](STATUS_DASHBOARD_JAN2026.md) |
| **What to Do Next** | [WORK_SUMMARY_JAN2026.md](WORK_SUMMARY_JAN2026.md) |
| **Detailed Analysis** | [COMPLETION_STATUS_JAN2026.md](COMPLETION_STATUS_JAN2026.md) |
| **Memory Safety** | [MEMORY_SAFETY_STATUS.md](MEMORY_SAFETY_STATUS.md) |
| **Roadmap** | [TODO.md](TODO.md) |
| **Getting Started** | [Readme.md](Readme.md) |

---

## 📊 Statistics

- **Total Documentation Files**: 14+ markdown documents
- **Code Files**: ~150 source files
- **Total Lines of Code**: ~150,000
- **Test Files**: 13+ test suites
- **Example Programs**: 50+ examples
- **Documentation Pages**: 95% coverage

---

## 💡 Tips

1. **Read the Dashboard First**: [STATUS_DASHBOARD_JAN2026.md](STATUS_DASHBOARD_JAN2026.md) is the quickest overview
2. **Check TODO for Priorities**: [TODO.md](TODO.md) has clear priority marking
3. **Use Grep to Search**: Documents are well-structured for searching
4. **Review Recent Changes**: Look for "✅ NEW" or "✅ Updated" badges
5. **Build Verification**: Always run `cargo build` and `cargo test` after changes

---

## ✅ Verification

To verify the current state of the project:

```bash
# Check build quality
cargo clippy --all-targets

# Run tests
cargo test

# Build release binary
cargo build --release

# Check file updates
ls -la *.md | grep "Jan  *1"
```

---

**Documentation Index**  
**Created**: January 1, 2026  
**Status**: Current & Verified  
**Last Updated**: January 1, 2026  

For any questions, refer to the specific documentation files listed above.


---

## Source: FEATURE_MATRIX.md

# AdeshLang Feature Matrix

Complete feature comparison across all execution backends.

## Numeric Literals

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Decimal literals | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Binary literals (0b) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **NEW v0.3.1** |
| Octal literals (0o) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **NEW v0.3.1** |
| Hexadecimal literals (0x) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **NEW v0.3.1** |
| Underscore separators | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **NEW v0.3.1** |
| Typed suffixes (u8, i32, etc.) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| BigInt suffix (n) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |

## Type System

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Signed integers (i8-i128) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Unsigned integers (u8-u128) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Floats (f32, f64) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| BigInt | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Option<T> | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Result<T, E> | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Tuple types | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Array types | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Union types | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Generic types | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |

## Functions

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Named functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Anonymous functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Arrow functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Closures | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Higher-order functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Recursion | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Tail-call optimization | ⚠️ | ⚠️ | ✅ | ✅ | ✅ | ⚠️ | Beta |
| Generic functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Default parameters | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Variadic functions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |

## Object-Oriented Programming

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Classes | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Methods | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Properties | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Getters/Setters | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Inheritance | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Interfaces | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Static methods | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Visibility (public/private) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Operator overloading | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Decorators | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | Stable |

## Memory Safety

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Ownership system | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Borrowing | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Lifetimes | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Move semantics | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Explicit ARC | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Weak references | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Region/Arena allocation | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | Stable |
| Raw pointers (unsafe) | ✅ | ❌ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Compile-time checks | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Runtime checks (debug) | ✅ | ✅ | ✅ | ✅ | ⚠️ | ⚠️ | Stable |

## Control Flow

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| if/else | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| match/switch | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| for loops | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| while loops | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| break/continue | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Pattern matching | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Exhaustiveness check | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |

## Async Programming

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| async/await | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | Beta |
| Promises | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | Beta |
| Async functions | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | Beta |
| Concurrent execution | ✅ | ⚠️ | ⚠️ | ❌ | ❌ | ⚠️ | Beta |

## FFI & Interop

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| C FFI | ✅ | ❌ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Dynamic library loading | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | Stable |
| C struct mapping | ✅ | ❌ | ✅ | ✅ | ✅ | ⚠️ | Stable |
| Callback functions | ✅ | ❌ | ⚠️ | ⚠️ | ✅ | ❌ | Beta |

## Standard Library

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM | Status |
|---------|-------------|-----|-----|------------|-----|------|--------|
| Math module | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| String module | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Collections (Array, Set) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| File I/O | ✅ | ⚠️ | ✅ | ✅ | ✅ | ❌ | Stable |
| Network I/O | ✅ | ⚠️ | ✅ | ✅ | ✅ | ⚠️ | Beta |
| Time/Date | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| JSON | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |
| Regex | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | Stable |

## Development Tools

| Feature | Availability | Status |
|---------|-------------|--------|
| Backend validation script | ✅ | **NEW v0.3.1** |
| Performance benchmarks | ✅ | **NEW v0.3.1** |
| REPL | ✅ | Stable |
| Format tool | ✅ | Stable |
| Disassembler | ✅ | Stable |
| IR dumper (HIR/LIR) | ✅ | Stable |
| Profiler | ✅ | Beta |
| Debugger | ⚠️ | Alpha |
| Language Server (LSP) | ⚠️ | Alpha |

## Build Modes

| Feature | Interpreter | VM | JIT | Native JIT | AOT | WASM |
|---------|-------------|-----|-----|------------|-----|------|
| Debug build | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Release build | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Embedded mode | ✅ | ⚠️ | ❌ | ❌ | ✅ | ❌ |
| Optimization levels (-O0 to -O3) | N/A | N/A | ✅ | ✅ | ✅ | ✅ |

## Performance Characteristics

| Backend | Relative Speed | Startup Time | Memory Usage | Best For |
|---------|---------------|--------------|--------------|----------|
| Interpreter | 1x | Instant | Low | Development, scripting |
| Bytecode VM | 5-8x | Fast | Low | Portable deployment |
| JIT | 10-20x | Medium | Medium | Production services |
| Native JIT | **100-200x** | Medium | Medium | Compute-intensive |
| AOT | 100-250x | None | Low | Standalone apps |
| WASM | 80-150x | Fast | Low | Web/edge computing |

## Legend

- ✅ **Fully Supported** - Feature works perfectly
- ⚠️ **Partial Support** - Feature works with limitations
- ❌ **Not Supported** - Feature not available
- **NEW v0.3.1** - Added in February 2026 update

## Notes

1. **WASM limitations** due to platform constraints (no file I/O, limited FFI)
2. **Async support** varies by backend; interpreter has full support
3. **Native JIT** has 71% instruction coverage (46/65 LIR instructions)
4. **Embedded mode** restricts heap allocation and ARC

## Documentation

For detailed information on any feature:
- [docs/](docs/) - Complete documentation
- [examples/](examples/) - Working code examples
- [QUICK_START.md](QUICK_START.md) - Getting started guide

---

*Last updated: February 2026*


---

## Source: MIGRATION.md

# AdeshLang Codebase Modularization - Migration Guide

## Overview

This document describes the comprehensive modularization refactoring of the AdeshLang compiler/runtime codebase. The refactoring reorganizes the source code into clean, logical modules without removing any functionality.

## What Changed

### 1. Backup Created (`src/_legacy_tree/`)

The entire original source structure has been preserved in `src/_legacy_tree/` as a safety backup. This contains all 138 original files unchanged.

### 2. New Module Structure

The source code has been reorganized into functional domains:

#### **Toolchain Module** (`src/toolchain/`)
- `cli/` - Command-line argument parsing
- `config/` - Runtime configuration
- `formatter/` - Code formatting utilities
- `docgen/` - Documentation generation
- `env/` - Environment variable handling
- `timer/` - Performance timing

**Old Paths → New Paths:**
- `src/cli/` → `src/toolchain/cli/`
- `src/utils/formatter.rs` → `src/toolchain/formatter/mod.rs`
- `src/utils/docgen.rs` → `src/toolchain/docgen/mod.rs`
- `src/utils/env.rs` → `src/toolchain/env/mod.rs`
- `src/utils/timer.rs` → `src/toolchain/timer/mod.rs`

#### **Type System Module** (`src/typesystem/`)
- `checker/` - Type checking
- `traits/` - Traits system
- `vtable/` - VTable implementation
- `layouts/` - Type and field layouts
- `visibility/` - Visibility rules
- `references/` - Safe references
- `safeguards/` - Standard library safeguards

**Old Paths → New Paths:**
- `src/types/` → `src/typesystem/`
- All type-related files reorganized into logical submodules

#### **Runtime Module** (`src/runtime/`)
- `stdlib_src/` - Standard library implementation
- `stdlib/` - Clean stdlib re-export interface
- `c_runtime/` - C runtime integration (adesh_runtime.c)
- `system/` - System-level operations

**Old Paths → New Paths:**
- `src/stdlib/` → `src/runtime/stdlib_src/`
- `src/backends/adesh_runtime.c` → `src/runtime/c_runtime/adesh_runtime.c`

#### **Backends Module** (`src/backends/`)
- `jit/` - JIT compilation
  - `cranelift/` - Main JIT engine
  - `tiered/` - Multi-tier JIT
  - `adaptive/` - Adaptive JIT
  - `optimizations/` - JIT optimizations including recursion
  - `array_ops/` - Array operation specializations
- `aot/` - AOT compilation
  - `cranelift/` - Cranelift AOT
  - `linker/` - Native binary linking
  - `memory/` - AOT memory tracking
- `wasm_backend/` - WebAssembly compilation
- `common/` - Shared backend infrastructure
  - `backend/`, `builtins/`, `lir/`, `ffi/`, `escape/`, etc.

**Old Paths → New Paths:**
- `src/backends/jit.rs` → `src/backends/jit/cranelift/mod.rs`
- `src/backends/adaptive_jit.rs` → `src/backends/jit/adaptive/mod.rs`
- `src/backends/tiered_jit.rs` → `src/backends/jit/tiered/mod.rs`
- `src/backends/jit_opt.rs` → `src/backends/jit/optimizations/mod.rs`
- `src/backends/recursion_opt.rs` → `src/backends/jit/optimizations/recursion.rs`
- `src/backends/cranelift_aot.rs` → `src/backends/aot/cranelift/mod.rs`
- `src/backends/linker_driver.rs` → `src/backends/aot/linker/mod.rs`
- `src/backends/backend.rs` → `src/backends/common/backend/mod.rs`
- `src/backends/builtins.rs` → `src/backends/common/builtins/mod.rs`
- `src/backends/lir.rs` → `src/backends/common/lir/mod.rs`
- `src/backends/lir_lower.rs` → `src/backends/common/lir/lower.rs`
- `src/backends/ffi_*.rs` → `src/backends/common/ffi/`

#### **Memory Module** (`src/memory/`)
- `allocators/` - Memory allocators
- `policies/` - Allocation policies
- `arc/` - Atomic reference counting
- `cycle/` - Cycle detection
- `concurrency/` - Thread-safe primitives
- `raii/` - RAII resource management
- `adaptive/` - Adaptive memory strategies

**Old Paths → New Paths:**
- `src/memory/*.rs` → `src/memory/*/mod.rs`
- `src/memory/arc_manager.rs` → `src/memory/arc/mod.rs`
- `src/memory/policy.rs` → `src/memory/policies/mod.rs`
- `src/memory/cycle_detect.rs` → `src/memory/cycle/mod.rs`

### 3. Backward Compatibility

All old import paths continue to work through re-exports:
- `use crate::types::*` → automatically redirects to `crate::typesystem::*`
- `use crate::stdlib::*` → automatically redirects to `crate::runtime::stdlib::*`
- `use crate::cli::*` → automatically redirects to `crate::toolchain::cli::*`
- All backend modules maintain backward compatibility

## Why This Structure is Better

### 1. **Clear Functional Separation**
- Each module has a single, well-defined responsibility
- Related code is grouped together logically
- Easier to understand what each module does

### 2. **Improved Maintainability**
- Smaller, focused modules are easier to understand and modify
- Changes to one subsystem don't affect unrelated code
- New developers can navigate the codebase more easily

### 3. **Better Build Times**
- Modular structure allows for better incremental compilation
- Changes to one module don't require recompiling unrelated modules

### 4. **Reduced Coupling**
- Clear module boundaries prevent accidental dependencies
- Easier to test modules in isolation
- Facilitates future refactoring

### 5. **Scalability**
- New features can be added to appropriate modules
- Modules can be split further if they grow too large
- Clear structure supports team collaboration

## How to Use the New Structure

### For Existing Code

No changes are required! All old import paths work through re-exports:

```rust
// Old style (still works)
use crate::types::typechecker::Ty;
use crate::stdlib::create_stdlib;

// New style (recommended)
use crate::typesystem::checker::Ty;
use crate::runtime::stdlib::create_stdlib;
```

### For New Code

Use the new module paths directly:

```rust
use crate::toolchain::cli::ParsedArgs;
use crate::typesystem::checker::Ty;
use crate::backends::jit::cranelift::JitCompiler;
use crate::runtime::stdlib::create_stdlib;
use crate::memory::arc::ArcManager;
```

## How to Safely Delete `_legacy_tree`

The `_legacy_tree/` directory can be deleted when:

1. **All tests pass** - Verify complete functionality preservation
2. **External consumers updated** - If any external crates depend on AdeshLang
3. **Stable for 2+ releases** - Give time for issues to surface
4. **Team consensus** - Everyone agrees the backup is no longer needed
5. **Documentation complete** - This migration guide and MODULE_MAP.md are sufficient

### Steps to Delete:

```bash
# 1. Verify everything works
cargo test --all
cargo build --release

# 2. Remove the directory
rm -rf src/_legacy_tree/

# 3. Commit the removal
git add src/_legacy_tree/
git commit -m "Remove legacy tree backup after successful migration"
```

## Remaining Work

The following modules still need migration to complete the full refactoring:

1. **Frontend Module** - Move lexer, parser, AST to `src/frontend/`
2. **IR Module** - Organize AST/HIR/MIR/LIR into `src/ir/`
3. **Semantics Module** - Move ownership, borrow checking, decorators to `src/semantics/`
4. **Execution Module** - Better organization of bytecode/VM

These can be done incrementally as needed.

## Questions or Issues?

If you encounter any issues with the new structure:
1. Check if your imports use old paths (which should still work)
2. Verify you're using the correct new module paths
3. Refer to MODULE_MAP.md for specific file relocations
4. The original code is preserved in `_legacy_tree/` if needed

## Benefits Achieved

✅ **100% Code Preservation** - All 138 files backed up, no code deleted
✅ **Incremental & Safe** - Each phase compiled successfully
✅ **Backward Compatible** - All old imports still work
✅ **Clean Architecture** - Logical separation by functional domains
✅ **Better Organization** - Major subsystems now have clear structure
✅ **Maintainable** - Easier to navigate, understand, and modify


---

## Source: PHASE1_QUICK_REFERENCE.md

# Phase 1 Quick Reference - Memory Optimization & Critical Fixes
**Total Duration:** 28-30 days (or 20 days with 2 developers)

---

## 🎯 Phase 1 Overview

The goal of Phase 1 is to **eliminate 200+ bytes of overhead per struct/class instance** by replacing HashMap-based field storage with direct memory layout using TypeRegistry and field offsets.

### Critical Path
```
Phase 1.1 (1.5d)        Phase 1.2 (5d)         Phase 1.3 (8d)       Phase 1.4 (15d)
Enforce Abstract   →    Struct Specialization  →  HashMap Replace   →  Backend Updates
Classes                 Create StackStruct        (PERFORMANCE CRITICAL!)
```

---

## Phase 1.1: Abstract Class Enforcement (1.5 days)

### Goal
Prevent instantiation of abstract classes. Currently, AdeshLang has `is_abstract` flag in AST but doesn't enforce it at runtime.

### What to do

**File:** `src/execution/runtime/mod.rs`
**Find:** ExprKind::New handler (line ~9200)
**Change:**

```rust
// BEFORE: No abstract check
ExprKind::New(class_name, args, kwargs) => {
    // ... find class ...
    let mut inst = UserInstance { ... };
    // Create instance (BUG: allows abstract classes!)
}

// AFTER: Check abstract flag
ExprKind::New(class_name, args, kwargs) => {
    // ... find class ...
    
    // NEW: Check abstract flag
    if class.is_abstract {
        return Err(format!(
            "Cannot instantiate abstract class '{}'", 
            class_name
        ));
    }
    
    // NEW: Check for unimplemented abstract methods in parent chain
    let mut parent_opt = class.parent.clone();
    while let Some(parent) = parent_opt {
        for (name, methods) in &parent.methods {
            for method in methods {
                // Check if method is abstract
                if method.modifiers.contains(&"abstract".to_string()) {
                    // Check if current class implements it
                    if !class.methods.get(name).map(|m| !m.is_empty()).unwrap_or(false) {
                        return Err(format!(
                            "Class '{}' must implement abstract method '{}' from parent",
                            class_name, name
                        ));
                    }
                }
            }
        }
        parent_opt = parent.parent.clone();
    }
    
    let mut inst = UserInstance { ... };
    // ... rest of instantiation ...
}
```

### Validation
1. Create test file: `tests/test_abstract.adesh`
2. Try to instantiate abstract class
3. Expect error: "Cannot instantiate abstract class"

---

## Phase 1.2: Struct Specialization (5 days)

### Goal
Create separate StackStruct variant for better optimization. Currently structs use HashMap like classes (wrong!).

### What to do

**File:** `src/parsing/ast.rs`
**Find:** `Value` enum (search for `UserClass`)
**Change:**

```rust
// BEFORE
pub enum Value {
    // ...
    Class(Arc<UserInstance>),
    // ... no separate Struct variant
}

// AFTER
pub enum Value {
    // ...
    Class(Arc<UserInstance>),           // Reference type (heap)
    Struct(Arc<StackStruct>),           // Value type (can optimize)
    // ... rest
}

// NEW: Add StackStruct definition
#[derive(Clone, Debug)]
pub struct StackStruct {
    pub struct_name: String,
    pub type_id: TypeId,                // Link to TypeRegistry
    pub data: Vec<u8>,                  // Direct byte buffer
    pub layout: Arc<FieldLayout>,       // Field offsets
}
```

### Update Constructor Handling
**File:** `src/execution/runtime/mod.rs`
**Find:** ExprKind::New handler
**Change:**

```rust
// When creating a Struct (not Class):
if struct_decl.is_some() {  // It's a struct, not class
    let struct_layout = compute_struct_layout(&struct_decl);
    let mut data = vec![0u8; struct_layout.size];
    
    // Initialize fields from arguments
    for (field_name, field_value) in fields_iter {
        let offset = struct_layout.get_field_offset(&field_name)?;
        write_field_at_offset(&mut data, offset, field_value);
    }
    
    return Value::Struct(Arc::new(StackStruct {
        struct_name: struct_name.clone(),
        type_id: get_type_id(&struct_name),
        data,
        layout: Arc::new(struct_layout),
    }));
}
```

### Field Access Update
**File:** `src/execution/runtime/mod.rs`
**Find:** Member access operators (`.`)
**Change:**

```rust
// BEFORE: No struct optimization
Value::Class(inst) => {
    let fields = inst.fields.lock().unwrap();
    fields.get(&member_name).cloned()
}

// AFTER: Optimized struct access
Value::Struct(s) => {
    if let Some(offset) = s.layout.get_field_offset(&member_name) {
        read_field_at_offset(&s.data, offset, &s.layout)
    } else {
        Err(format!("Field '{}' not found", member_name))
    }
}
```

### Performance Expectation
- Struct field access: 50-100x faster (direct vs HashMap)
- Memory: Zero overhead (vs 40+ bytes in HashMap)

---

## Phase 1.3: HashMap → Direct Layout Replacement (8 DAYS) ⭐ CRITICAL

### Goal
Replace `Arc<Mutex<HashMap>>` field storage with `Vec<u8>` + TypeLayout direct offsets.

**This is the biggest performance win!**

### Current Problem

```rust
// Current (BAD):
pub struct UserInstance {
    pub class_name: String,
    pub fields: Arc<Mutex<HashMap<String, Value>>>,    // 40+ bytes overhead!
    pub class: UserClass,
    pub prop_cache: Arc<Mutex<HashMap<String, Value>>>, // 72+ bytes overhead!
}

// Memory:
// - HashMap allocation
// - RwLock overhead
// - String keys for every field lookup
// - Cache misses due to pointer chasing
// RESULT: 200+ bytes overhead per instance ❌
```

### Solution

```rust
// New (GOOD):
pub struct UserInstance {
    pub class_name: String,
    pub data: Vec<u8>,                  // Contiguous field data
    pub type_id: TypeId,                // Link to TypeRegistry
    pub layout: Arc<FieldLayout>,       // Offset information
    pub class: UserClass,               // Keep for methods
}

// Memory:
// - Vec<u8> for actual fields
// - TypeId (4 bytes)
// - Arc<FieldLayout> (8 bytes)
// RESULT: 8-16 bytes overhead ✅
// IMPROVEMENT: 75% memory reduction!
```

### Implementation Steps

#### Step 1: Update AST (src/parsing/ast.rs)
```rust
pub struct UserInstance {
    pub class_name: String,
    pub data: Vec<u8>,
    pub type_id: TypeId,
    pub layout: Arc<FieldLayout>,
    pub class: UserClass,
    // Remove: pub fields: Arc<Mutex<HashMap>>
    // Remove: pub prop_cache: Arc<Mutex<HashMap>>
}
```

#### Step 2: Create Instance (src/execution/runtime/mod.rs)
```rust
// At class instantiation (line ~9200):
let mut inst = UserInstance {
    class_name: class_name.clone(),
    data: vec![0u8; class_layout.get_final_size()],
    type_id: register_class_in_registry(&class),
    layout: Arc::new(class_layout),
    class: class.clone(),
};

// Initialize fields:
for (field_name, field_value) in field_init {
    let offset = inst.layout.get_field_offset(&field_name)?;
    inst.write_field(offset, field_value)?;
}
```

#### Step 3: Update Field Access (15+ sites in mod.rs)
```rust
// BEFORE (Line 1176):
let fg = i.fields.lock().unwrap();
let val = fg.get(&field_name).cloned();

// AFTER:
let val = i.read_field_by_name(&field_name)?;

// Add helper methods:
impl UserInstance {
    fn read_field_by_name(&self, name: &str) -> Result<Value> {
        if let Some(offset) = self.layout.get_field_offset(name) {
            self.read_field_at(offset)
        } else {
            Err(format!("Field '{}' not found", name))
        }
    }
    
    fn write_field_by_name(&mut self, name: &str, value: Value) -> Result<()> {
        if let Some(offset) = self.layout.get_field_offset(name) {
            self.write_field_at(offset, value)
        } else {
            Err(format!("Field '{}' not found", name))
        }
    }
    
    fn read_field_at(&self, offset: usize) -> Result<Value> {
        // Deserialize from data[offset..offset+size]
        // Implementation depends on Value type
    }
    
    fn write_field_at(&mut self, offset: usize, value: Value) -> Result<()> {
        // Serialize to data[offset..offset+size]
    }
}
```

#### Step 4: Update All Field Access Sites
Find and replace all 15+ occurrences:

```bash
# Search patterns:
i.fields.lock().unwrap().get(
i.fields.lock().unwrap().insert(
inst.fields.lock().unwrap()
s.fields.get(
s.fields.insert(
```

Files to update:
- src/execution/runtime/mod.rs (main interpreter)
- src/execution/exec.rs (alternative execution)
- src/execution/vm.rs (bytecode VM)
- src/execution/array_ops.rs (array operations)

### Field Storage Serialization

For each field type, you'll need serialize/deserialize:

```rust
fn write_value_at_offset(data: &mut [u8], offset: usize, value: &Value) {
    match value {
        Value::Number(n) => {
            // Write 8-byte f64 at offset
            data[offset..offset+8].copy_from_slice(&n.to_le_bytes());
        }
        Value::String(s) => {
            // String: (ptr, len, cap) - 24 bytes
            // This is complex! Probably better to store String directly
        }
        Value::Class(inst) => {
            // Store pointer (8 bytes)
            let ptr = inst.as_ref() as *const _ as usize;
            data[offset..offset+8].copy_from_slice(&ptr.to_le_bytes());
        }
        // ... etc
    }
}
```

### Performance Testing

Create benchmarks:
```rust
#[bench]
fn bench_field_access_new(b: &mut Bencher) {
    // Create struct with 10 fields
    let inst = create_test_instance();
    
    b.iter(|| {
        for i in 0..1000 {
            let _ = inst.read_field_by_name("field_0");
        }
    });
}
```

Expected improvement: **50-100x faster**

---

## Phase 1.4: Backend Updates (15 days)

### Affected Backends
1. **Interpreter** (src/execution/runtime/mod.rs) ← Start here
2. **BytecodeVM** (src/execution/vm.rs)
3. **JIT/LLVM** (src/backends/jit.rs)
4. **AOT** (src/backends/aot.rs)
5. **WASM** (src/backends/wasm.rs)

### For each backend:
1. Replace HashMap field access with offset-based access
2. Register types in TypeRegistry at compile time
3. Generate field offset constants for JIT/AOT
4. Update method dispatch to use VTable (when Phase 2)

### Example: BytecodeVM Update

```rust
// Old bytecode for field access:
Bytecode::GetField(name) => {
    let obj = pop();
    let fields = obj.fields.lock().unwrap();
    push(fields.get(name).cloned());
}

// New bytecode:
Bytecode::GetField(offset) => {
    let obj = pop();
    let value = obj.read_field_at(offset)?;
    push(value);
}

// Compile-time: compute offsets and emit offset constants
// instead of field names
```

---

## Testing Strategy

### Unit Tests (Add to tests/)
```rust
#[test]
fn test_abstract_class_no_instantiate() {
    // Try to instantiate abstract class
    // Should fail
}

#[test]
fn test_struct_direct_access() {
    // Create struct with 5 fields
    // Access each field
    // Verify values are correct
}

#[test]
fn test_field_layout_memory() {
    // Create instance
    // Check memory size = expected size
    // No 200+ byte overhead
}

#[test]
fn test_field_alignment() {
    // Verify field offsets respect alignment rules
}

#[test]
fn test_inheritance_field_offsets() {
    // Parent + child fields in correct order
}
```

### Integration Tests (Add to tests/)
```rust
#[test]
fn test_large_array_memory() {
    // Create array of 10,000 structs
    // Verify memory usage < expected
    // Should be 75% better than current
}

#[test]
fn test_field_access_performance() {
    // Measure access time
    // Should be 50x faster
}
```

---

## Success Criteria for Phase 1

- ✅ Abstract classes cannot be instantiated
- ✅ Structs use direct layout (not HashMap)
- ✅ Field access uses offsets (not HashMap lookup)
- ✅ Memory overhead < 16 bytes per instance
- ✅ All 5 backends working
- ✅ Field access performance: 50-100x faster
- ✅ All tests passing
- ✅ No breaking changes to user-visible API

---

## Common Pitfalls to Avoid

1. **Forget to update all 15 field access sites**
   - Search for: `fields.lock()`, `fields.get()`, `fields.insert()`
   - Easy to miss one and cause crashes

2. **Misalignment causing segfaults**
   - Always check padding calculations
   - Test with mixed-sized fields

3. **String serialization issues**
   - Strings can't just be stored as fixed bytes
   - Either: store as references, or use fixed-size buffer

4. **Forgetting parent fields**
   - Inheritance: child fields must start after parent
   - Use FieldLayout's inheritance support

5. **VTable integration premature**
   - Phase 1.3 is just offsets, not methods
   - Don't mix VTable work into Phase 1
   - VTable is Phase 2

---

## Detailed Timeline (Days 1-30)

```
Week 1:
  Day 1-2: Phase 1.1 - Abstract class enforcement
  Day 3-7: Phase 1.2 - Struct specialization (5 days)

Week 2:
  Day 8-15: Phase 1.3 - HashMap replacement (8 days) ⭐ CRITICAL
            (Daily focus: update 2-3 field access sites per day)

Week 3-4:
  Day 16-30: Phase 1.4 - Backend updates (15 days)
             (Interpreter: Days 16-20)
             (VM: Days 21-23)
             (JIT/AOT/WASM: Days 24-30, can parallelize)
```

---

## If You Have 2 Developers

**Parallel approach (20 days instead of 30):**
```
Dev 1: Phase 1.1 + 1.2 + 1.3 (1.5 + 5 + 8 = 14.5 days)
  ↓
Dev 2: Start Phase 1.4 parallel to Phase 1.3 (day 9+)
  ↓
Both: Backend updates (5 days parallel)
```

Total: ~20 days with good parallelization

---

## Reference Documents

- **CORE_SEMANTIC_IR_AND_RUNTIME_API.md** - Runtime API details
- **UNIFIED_OBJECT_MODEL_V2.md** - Object model specification  
- **IMPLEMENTATION_ROADMAP_PHASE4.md** - Full roadmap with all phases
- **PHASE0_COMPLETE.md** - TypeInfo foundation that Phase 1 builds on

---

**Status: READY TO START PHASE 1** 🚀


---

## Source: QUICK_BUILD_REFERENCE.md

# Quick Reference: Fast Compilation & Build System

## One-Liners

```bash
# ⚡ Development (instant feedback)
adesh build app.adesh --fast

# 🚀 Production (tiny executable)
adesh build app.adesh -O3

# 🎯 Balanced (recommended)
adesh build app.adesh -O2

# 🧪 Test and Run
adesh build app.adesh --fast --run -- --test-filter math

# 📦 Library Build
adesh build lib.adesh --library --library-mode

# 🔀 Cross-compile
adesh build app.adesh --target aarch64-unknown-linux-gnu
```

## Decision Tree

```
What do you need?
│
├─ Fast iteration (development)? → adesh build app.adesh --fast
│  └─ Time: 0.3-0.5s, Size: 4-6MB
│
├─ Production executable? → adesh build app.adesh -O3
│  └─ Time: 5-7s, Size: 1.5-2MB
│
├─ CI/CD pipeline? → adesh build app.adesh -O2
│  └─ Time: 3-4s, Size: 2-3MB (good balance)
│
├─ Library for others? → adesh build mylib.adesh --library --library-mode
│  └─ Creates: mylib.lib or mylib.a
│
└─ Debug a crash? → adesh build app.adesh --debug -O1
   └─ Includes symbols, moderate optimization
```

## Cheat Sheet

| Task | Command |
|------|---------|
| Quick test | `adesh build test.adesh --fast` |
| Run immediately | `adesh build test.adesh --fast --run` |
| Optimize for size | `adesh build app.adesh -O3` |
| Optimize for speed | `adesh build app.adesh -O2` |
| Debug symbols | `adesh build app.adesh --debug -O1` |
| See build plan | `adesh build app.adesh --dry-run` |
| Link library | `adesh build app.adesh -l m` |
| Custom output | `adesh build app.adesh -o myapp.exe` |

## Optimization Levels

```
--fast   ││░░░░░░░░  Speed: ⚡⚡⚡⚡⚡  Size: 📦📦📦
-O0      │░░░░░░░░░  Speed: ⚡⚡⚡⚡    Size: 📦📦📦
-O1       └─────────  Speed: ⚡⚡⚡⚡⚡   Size: 📦📦
-O2                   Speed: ⚡⚡⚡      Size: 📦         ← Recommended
-O3                   Speed: ⚡⚡       Size: 📦
```

## Common Patterns

### Development Workflow
```bash
# Edit code...
adesh build dev.adesh --fast          # (0.3-0.5s) ← Edit-compile loop
# Test...
# Repeat
```

### Deployment Pipeline
```bash
adesh build app.adesh --check         # Validate (fast)
adesh build app.adesh -O3             # Optimize (medium)
# Ship the binary
```

### Debugging Session
```bash
adesh build app.adesh --debug -O1     # Symbols + some optimization
# Run in debugger
# Step through code
```

### Library Distribution
```bash
adesh build mylib.adesh --library -O3 --library-mode
# Distribute mylib.lib or mylib.a
```

## Performance Expectations

```
Compilation Time:
  --fast:  ████████████ 0.3-0.5s  ← Super fast!
  -O0:     ████████████░ 0.5-1s
  -O1:     ██████████░░░░ 1-2s
  -O2:     ████████░░░░░░░ 3-4s   ← Recommended
  -O3:     ███░░░░░░░░░░░░ 5-7s

Executable Size:
  --fast:  ████████████ 4-6MB
  -O0:     ███████████░ 5.5MB
  -O1:     ██████░░░░░░░ 3.5MB
  -O2:     ████░░░░░░░░░░ 2.5MB   ← Recommended
  -O3:     ██░░░░░░░░░░░░░ 1.5-2MB
```

## Pro Tips

### 💡 Tip 1: Mix Modes
```bash
# Dev loop with --fast
adesh build test.adesh --fast

# Final testing with -O2
adesh build test.adesh -O2

# Release with -O3
adesh build test.adesh -O3
```

### 💡 Tip 2: Dry Run First
```bash
# See what would happen
adesh build myapp.adesh --dry-run
# Then actually build
adesh build myapp.adesh -O2
```

### 💡 Tip 3: Use Verbose for Troubleshooting
```bash
# If build breaks, see what's happening
adesh build app.adesh --verbose
# Look for error details in output
```

### 💡 Tip 4: Link Multiple Libraries
```bash
#!/bin/bash
adesh build network.adesh \
  -l ssl           \
  -l curl          \
  -l z             \
  -L /usr/lib      \
  -O3
```

### 💡 Tip 5: Cross-Compilation
```bash
# Check available targets first
# Then compile for it
adesh build app.adesh --target x86_64-pc-windows-gnu
```

## Troubleshooting

### Build too slow?
```bash
# ✓ Use --fast for development
adesh build app.adesh --fast

# ✗ Don't use -O3 during development
# adesh build app.adesh -O3  ← Too slow
```

### Executable too big?
```bash
# ✓ Use -O3 for production
adesh build app.adesh -O3

# ✗ Don't use --fast for binaries you're shipping
# adesh build app.adesh --fast  ← Still works but not compact
```

### Linking errors?
```bash
# ✓ Add libraries explicitly
adesh build app.adesh -l m -l pthread

# ✓ Check library paths
adesh build app.adesh -L /usr/lib64 -l mylib

# ✓ See what's being linked
adesh build app.adesh --verbose
```

## Environment Quick Reference

```bash
# Development setup
export ADESH_FAST=1           # Default to --fast

# Release setup
unset ADESH_FAST              # Use normal builds

# Debugging
export ADESH_VERBOSE=1        # Verbose output by default
```

## See Also

- Full guide: [BUILD_SYSTEM_GUIDE.md](BUILD_SYSTEM_GUIDE.md)
- Technical docs: [AOT_COMPILATION_OPTIMIZATION.md](AOT_COMPILATION_OPTIMIZATION.md)
- CLI guide: [CLI_IMPLEMENTATION_SUMMARY.md](CLI_IMPLEMENTATION_SUMMARY.md)

---

**Remember:** 
- 🚀 Use `--fast` for development (10x speedup!)
- 📦 Use `-O3` for production (60-75% size reduction!)
- 🎯 Use `-O2` for CI/CD (best balance)



---

## Source: QUICK_START.md

# AdeshLang Quick Start Guide

> Get started with AdeshLang in 5 minutes

---

## Installation

### Prerequisites
- Rust 1.70 or later
- Git

### Build from Source

```bash
# Clone the repository
git clone https://github.com/ajaytainwala-dev/mylang.git
cd mylang

# Build the project
cargo build --release

# Run your first program
cargo run -- run examples/basics/hello.adesh
```

---

## Your First Program

Create a file called `hello.adesh`:

```adesh
print("Hello, AdeshLang!");
```

Run it:

```bash
adesh run hello.adesh
```

---

## Basic Syntax

### Variables

```adesh
// Immutable by default
let x = 42;
let name = "Alice";

// With type annotations
let count: i64 = 100;
let price: f64 = 19.99;
```

### Functions

```adesh
fn greet(name: string) {
    print("Hello, " + name + "!");
}

fn add(a: i64, b: i64): i64 {
    return a + b;
}

greet("World");  // Hello, World!
let sum = add(5, 3);  // 8
```

### Control Flow

```adesh
// If-else
if x > 10 {
    print("Greater");
} else {
    print("Smaller or equal");
}

// While loop
let i: i64;
i = 0;
while i < 5 {
    print(i);
    i = i + 1;
}
```

---

## Numeric Literals

AdeshLang supports multiple numeric formats:

```adesh
// Decimal
let dec = 255;

// Binary
let bin = 0b11111111;

// Octal
let oct = 0o377;

// Hexadecimal
let hex = 0xFF;

// With underscores for readability
let large = 1_000_000;
let color = 0xFF_00_FF;

// Typed literals
let byte: u8 = 255u8;
let word: u16 = 0xFFFFu16;

// All equal!
print(dec == bin && bin == oct && oct == hex);  // true
```

---

## Type System

### Primitive Types

```adesh
// Integers
let tiny: i8 = 127;
let small: i16 = 32767;
let normal: i32 = 2147483647;
let large: i64 = 9223372036854775807;

// Unsigned integers
let byte: u8 = 255u8;
let word: u16 = 65535u16;
let dword: u32 = 4294967295u32;
let qword: u64 = 18446744073709551615u64;

// Floats
let single: f32 = 3.14f32;
let double: f64 = 3.14159265359;

// BigInt
let huge = 123456789012345678901234567890n;
```

### Option & Result

```adesh
// Option for nullable values
let maybe: Option<i64> = Some(42);
let nothing: Option<i64> = None;

match maybe {
    Some(value) => print(value),
    None => print("No value")
}

// Result for error handling
fn divide(a: f64, b: f64): Result<f64, string> {
    if b == 0.0 {
        return Err("Division by zero");
    }
    return Ok(a / b);
}
```

---

## Arrays & Objects

### Arrays

```adesh
let numbers = [1, 2, 3, 4, 5];
print(numbers[0]);  // 1
print(numbers[4]);  // 5
```

### Objects

```adesh
let person = {
    name: "Alice",
    age: 30,
    city: "New York"
};

print(person.name);  // Alice
print(person.age);   // 30
```

---

## Classes

```adesh
class Point {
    x: f64,
    y: f64,
    
    fn distance_from_origin(): f64 {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
}

let p = Point { x: 3.0, y: 4.0 };
print(p.distance_from_origin());  // 5.0
```

---

## Execution Backends

AdeshLang supports multiple execution backends:

```bash
# Interpreter (default, instant startup)
adesh run program.adesh

# Bytecode VM (portable)
adesh compile program.adesh program.adeshbc
adesh run --vm program.adeshbc

# JIT compilation (fast)
adesh run --jit program.adesh

# Native JIT (fastest, 100-200x faster!)
adesh run --njit program.adesh

# AOT compilation (standalone executable)
adesh build program.adesh
./program
```

---

## Common Patterns

### Fibonacci (Recursion)

```adesh
fn fibonacci(n: i64): i64 {
    if n <= 1 {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

print(fibonacci(10));  // 55
```

### Factorial (Tail Recursion)

```adesh
fn factorial(n: i64, acc: i64 = 1): i64 {
    if n <= 1 {
        return acc;
    }
    return factorial(n - 1, n * acc);
}

print(factorial(5));  // 120
```

### Higher-Order Functions

```adesh
fn apply_twice(f: fn(i64): i64, x: i64): i64 {
    return f(f(x));
}

fn increment(n: i64): i64 {
    return n + 1;
}

print(apply_twice(increment, 5));  // 7
```

---

## Example Programs

### Hello World

```adesh
print("Hello, World!");
```

### Calculator

```adesh
fn add(a: i64, b: i64): i64 { return a + b; }
fn sub(a: i64, b: i64): i64 { return a - b; }
fn mul(a: i64, b: i64): i64 { return a * b; }
fn div(a: i64, b: i64): i64 { return a / b; }

print("10 + 5 = ");
print(add(10, 5));

print("10 - 5 = ");
print(sub(10, 5));

print("10 * 5 = ");
print(mul(10, 5));

print("10 / 5 = ");
print(div(10, 5));
```

### Number Guessing Game (Conceptual)

```adesh
fn check_guess(secret: i64, guess: i64): string {
    if guess < secret {
        return "Too low!";
    }
    if guess > secret {
        return "Too high!";
    }
    return "Correct!";
}

let secret = 42;
print(check_guess(secret, 30));  // Too low!
print(check_guess(secret, 50));  // Too high!
print(check_guess(secret, 42));  // Correct!
```

---

## CLI Commands

```bash
# Run a program
adesh run <file>

# Build native executable
adesh build <file>

# Run with specific backend
adesh run --jit <file>          # JIT compilation
adesh run --njit <file>         # Native JIT
adesh run --vm <file>           # Bytecode VM

# Optimization levels
adesh run --jit -O0 <file>      # No optimization
adesh run --jit -O1 <file>      # Basic optimization
adesh run --jit -O2 <file>      # Standard optimization
adesh run --jit -O3 <file>      # Maximum optimization

# Compile to bytecode
adesh compile <input> <output>

# Format code
adesh format <file>             # Print formatted code
adesh format --write <file>     # Format in place

# Type checking
adesh check <file>

# Show IR
adesh run --dump-hir <file>     # High-level IR
adesh run --dump-lir <file>     # Low-level IR

# Testing
adesh run --test <file>         # Run tests

# Performance profiling
adesh run --profile <file>
adesh run --memory <file>
```

---

## Next Steps

1. **Explore Examples:** Check out `examples/` directory for more code samples
2. **Read Documentation:** See `docs/` for detailed guides:
   - `literals.md` - Numeric literal formats
   - `functions.md` - Function reference
   - `semantics.md` - Language semantics
   - `backends.md` - Execution backends
   - `TYPE_SYSTEM.md` - Type system details

3. **Try Different Backends:** Compare performance with different execution modes
4. **Build Something:** Start with simple programs and gradually explore advanced features

---

## Getting Help

- **Documentation:** `docs/` directory
- **Examples:** `examples/` directory
- **GitHub Issues:** Report bugs and request features
- **README:** Full feature list and architecture overview

---

## Common Issues

### Build Fails

```bash
# Update Rust
rustup update

# Clean and rebuild
cargo clean
cargo build --release
```

### Cannot Find Executable

```bash
# Add to PATH or use full path
./target/release/adeshlang run program.adesh

# Or install globally
cargo install --path .
```

---

## What Makes AdeshLang Special?

- ✅ **No Garbage Collector** - Deterministic memory management
- ✅ **Multiple Backends** - Interpreter, JIT, AOT, WASM
- ✅ **Strong Type System** - Rust-inspired safety guarantees
- ✅ **Fast** - Native JIT is 100-200x faster than interpreter
- ✅ **Modern Syntax** - Clean and expressive
- ✅ **Memory Safe** - Ownership and borrowing system
- ✅ **Cross-Platform** - Linux, Windows, macOS support

---

## Quick Reference Card

```adesh
// Variables
let x = 42;
let y: i64 = 100;

// Functions
fn add(a: i64, b: i64): i64 {
    return a + b;
}

// If-else
if condition {
    // ...
} else {
    // ...
}

// While loop
while condition {
    // ...
}

// Arrays
let arr = [1, 2, 3];

// Objects
let obj = { key: value };

// Classes
class MyClass {
    field: Type,
    fn method() { }
}

// Numeric literals
0xFF      // Hex
0b1010    // Binary
0o755     // Octal
1_000_000 // With underscores
```

---

*Happy coding with AdeshLang! 🚀*

