# AdeshLang AOT Compilation Examples

This directory contains examples of all available AOT (Ahead-of-Time) compilation options for AdeshLang.

## Generated Files

### Executables (Different Optimization Levels)
- `aot_test_O0.exe` - No optimization (fastest compilation)
- `aot_test_O1.exe` - Basic optimization
- `aot_test_O2.exe` - Standard optimization (default)
- `aot_test_O3.exe` - Maximum optimization (slowest compilation)

### Special Executables
- `aot_test_debug.exe` - Includes debug information
- `aot_test_win64.exe` - Explicit Windows x86_64 target
- `comprehensive_release.exe` - Full-featured test with maximum optimization

### Libraries
- `aot_test_shared.dll` - Shared/dynamic library (--shared)
- `aot_test_dll.dll` - Dynamic library (--dll)
- `aot_test_static.lib` - Static library (--static)
- `aot_test_lib.lib` - Static library (--lib)

### Object Files
- `aot_test_object.obj` - Object file only (--object)
- `aot_test_obj.obj` - Object file only (--obj)

## Compilation Commands Used

```bash
# Optimization levels
adesh compile-aot aot_test.adesh lib\aot_test_O0.exe -O0
adesh compile-aot aot_test.adesh lib\aot_test_O1.exe -O1
adesh compile-aot aot_test.adesh lib\aot_test_O2.exe -O2
adesh compile-aot aot_test.adesh lib\aot_test_O3.exe -O3

# With debug info
adesh compile-aot aot_test.adesh lib\aot_test_debug.exe --debug

# Libraries
adesh compile-aot aot_test.adesh lib\aot_test_shared.dll --shared
adesh compile-aot aot_test.adesh lib\aot_test_dll.dll --dll
adesh compile-aot aot_test.adesh lib\aot_test_static.lib --static
adesh compile-aot aot_test.adesh lib\aot_test_lib.lib --lib

# Object files
adesh compile-aot aot_test.adesh lib\aot_test_object.obj --object
adesh compile-aot aot_test.adesh lib\aot_test_obj.obj --obj

# Cross-compilation (Windows x86_64)
adesh compile-aot --target=x86_64-pc-windows-msvc aot_test.adesh lib\aot_test_win64.exe

# Comprehensive example
adesh compile-aot comprehensive_test.adesh lib\comprehensive_release.exe -O3
```

## Cross-Compilation

AdeshLang supports cross-compilation to multiple platforms. The compiler generates object files for the target platform, and linking requires appropriate cross-compilation toolchains.

### Supported Targets

| Target Triple | Platform | Architecture |
|---------------|----------|--------------|
| `x86_64-unknown-linux-gnu` | Linux | x86_64 (glibc) |
| `x86_64-unknown-linux-musl` | Linux | x86_64 (musl, static) |
| `aarch64-unknown-linux-gnu` | Linux | ARM64 |
| `arm-unknown-linux-gnueabihf` | Linux | ARM32 (hard float) |
| `x86_64-pc-windows-gnu` | Windows | x86_64 (MinGW) |
| `x86_64-pc-windows-msvc` | Windows | x86_64 (MSVC) |
| `x86_64-apple-darwin` | macOS | x86_64 |
| `aarch64-apple-darwin` | macOS | ARM64 (Apple Silicon) |
| `aarch64-linux-android` | Android | ARM64 |
| `armv7-linux-androideabi` | Android | ARM32 |
| `aarch64-apple-ios` | iOS | ARM64 |

### Cross-Compilation Examples

```bash
# Generate Linux x86_64 executable (from Windows/macOS)
adesh compile-aot --target=x86_64-unknown-linux-gnu program.adesh app

# Generate Linux ARM64 executable
adesh compile-aot --target=aarch64-unknown-linux-gnu program.adesh app

# Generate Windows executable (from Linux/macOS)
adesh compile-aot --target=x86_64-pc-windows-gnu program.adesh app.exe

# Generate macOS Apple Silicon executable
adesh compile-aot --target=aarch64-apple-darwin program.adesh app

# Generate Android ARM64 executable
adesh compile-aot --target=aarch64-linux-android program.adesh app

# Generate object file only (no linking required)
adesh compile-aot --target=x86_64-unknown-linux-gnu program.adesh app.o --object
```

### Toolchain Requirements

For cross-compilation linking, you need the appropriate toolchain installed:

| Target Platform | Required Toolchain |
|-----------------|-------------------|
| Linux (from Windows/macOS) | Cross-GCC (e.g., `x86_64-linux-gnu-gcc`) or Clang |
| Windows (from Linux/macOS) | MinGW-w64 (`x86_64-w64-mingw32-gcc`) |
| macOS (from other platforms) | osxcross or macOS development environment |
| Android | Android NDK |
| iOS | Xcode (macOS only) |

**Note:** If linking fails due to missing toolchain, the compiler will generate an object file (.o) that can be linked manually using the appropriate cross-linker.

## File Sizes
- Executables: ~65KB (optimized)
- Object files: ~1.3KB
- Libraries: ~65KB

## Test Results
All executables run successfully and produce expected output.

## Notes
- Cross-compilation generates object files for any target platform
- Linking requires appropriate cross-compilation toolchain
- Use `--object` flag to skip linking and generate object files only
- Template literals and some array operations may have display issues in current version
- All optimization levels and library types compile successfully