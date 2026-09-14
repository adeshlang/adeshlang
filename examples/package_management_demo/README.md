# Comprehensive Package Management & Import Demo

This example demonstrates how to create a library package, define comprehensive dependencies, and handle various edge cases in the AdeshLang dependency manager.

## Project Structure

- `my_lib/` - The dependency library package.
  - `adesh.adl` - Manifest defining `my_lib` as a library.
  - `src/lib.adesh` - Exports math and welcome helper functions.
- `my_native_lib/` - Native FFI library package.
  - `adesh.adl` - Manifest defining `my_native_lib`.
  - `src/lib.adesh` - Exports math routines that use underlying C library FFI imports (`extern fn sqrt`, `extern fn sin`).
- `my_app/` - The consumer application package.
  - `adesh.adl` - Manifest demonstrating path, git, registry, and optional dependency specifications.
  - `src/main.adesh` - Imports `"my_lib"` and `"my_native_lib"`, calling helper functions and executing native calculations.

---

## Step-by-Step Walkthrough

Follow these steps to run the package management, FFI, and script execution demo:

### Step 1: Navigate to the Application Folder
Move into the `my_app` folder:
```bash
cd examples/package_management_demo/my_app
```

### Step 2: Compile the Custom Native C Library (FFI)
Use the defined `build_c` script in `adesh.adl` to compile the custom math library to a shared DLL:
```bash
adl script build_c
```
This runs the compiler script under the hood, outputting `libcustom_math.dll` into `my_native_lib/lib/`.

### Step 3: Install Package Dependencies
Use the `install` script shortcut or run directly to populate the local `adl_modules/` folder with `my_lib` and `my_native_lib`:
```bash
adl script install
```

This resolves the dependencies, builds the lockfile (`adesh.lock.adl`), and copies the native dynamic library directly into `adl_modules/my_native_lib/lib/libcustom_math.dll`.

### Step 4: Run the Application (Auto FFI Linking)
Execute the application using the `run` script shortcut:
```bash
adl script run
```

At startup, the runtime parses the manifests:
1. It registers `adl_modules/my_native_lib/lib` to the FFI search path.
2. It detects `libs = ["custom_math"]` in `my_native_lib/adesh.adl` and **automatically loads** the library.
3. The interpreter successfully executes the FFI declarations, giving the output:
```text
Welcome demo starting...
Hello, Developer! Welcome to the AdeshLang ecosystem!
Factorial of 5 calculated via my_lib is: 120
Hypotenuse of 3 and 4 calculated via C FFI in my_native_lib: 5
6 * 7 calculated via custom C DLL: 42
Factorial of 10 calculated via custom C DLL: 3628800
```

---

## Manifest Reference (`adesh.adl`)

AdeshLang manifests support multiple dependency sources and configurations:

### 1. Path/Local Dependencies
Used for local development or workspace components:
```toml
my_lib = { version = "^1.2.0", path = "../my_lib" }
```

### 2. Registry Dependencies
Resolved from official or custom registry servers:
```toml
json_parser = { version = "^2.0.0", registry = "official" }
```

### 3. Git Dependencies
Resolved from a remote repository with optional target revision (tag, branch name, or commit SHA):
```toml
network_lib = { version = "^0.4.0", git = "https://github.com/example/network_lib.git", rev = "v0.4.2" }
```

### 4. Advanced Options
- **`optional`**: Marks the dependency as optional (can be omitted in offline mode).
- **`features`**: Selects specific compile-time feature flags to enable in the library.
- **`target`**: Target platform restriction (e.g. `"windows"`, `"linux"`).
- **`profile`**: Custom compilation profile override (e.g. `"release"`, `"debug"`).
```toml
telemetry_lib = { version = "^1.0.0", optional = true, features = ["metrics"], target = "windows", profile = "release" }
```

---

## Ecosystem Design & Edge Cases

The AdeshLang package manager has built-in protection and resolution strategies for key package-management challenges:

### A. Dependency Cycle Detection
If a package dependency chain contains a cycle (e.g., `pkg_a` -> `pkg_b` -> `pkg_a`), the resolver tracks the resolution path history and exits immediately with a clean diagnostic:
```text
dependency cycle detected: main_app -> pkg_a -> pkg_b -> pkg_a
```

### B. Version Unification & Conflict Resolution
- **Version Unification**: If multiple packages request the same dependency with compatible semantic ranges (e.g. `^1.1.0` and `^1.2.0`), the resolver unifies them to the latest compatible version (e.g. `1.2.5`) to avoid compiling multiple copies of the same library.
- **Conflict Resolution**: If ranges are incompatible (e.g. `^1.0.0` and `^2.0.0`), the resolver raises a clean conflict error instead of silently choosing an incorrect version:
```text
dependency conflict for package 'shared': no version satisfies requirements: ^1.0.0, ^2.0.0
```

### C. Lockfile Integrity and Security Signatures
To protect against supply chain attacks, every lockfile contains a computed `signature` hash of its contents:
- Whenever `install` or `run` is executed, the runtime re-calculates the signature and compares it with the lockfile signature.
- If a dependency version, name, or checksum is modified manually in the lockfile, the integrity check fails and halts compilation:
```text
lockfile integrity check failed: signature mismatch (lockfile has been modified)
```

### D. Offline Mode
To run builds when disconnected from the internet, you can use the `--offline` flag. The resolver will only use cached registry packages and local path references, and will skip fetching remote Git repositories or querying remote servers.

---

## Multi-Backend Interoperability & FFI

AdeshLang is designed to support multiple execution strategies (Interpreter, VM, JIT, Native JIT, AOT, WebAssembly, and GPU targets). Interoperability across all these backends for native binary builds (C/Rust compiled libraries) is achieved seamlessly:

### 1. Automatic FFI Search Path Registration
When the compiler/runtime CLI configures the execution environment, it automatically scans for an `adl_modules/` directory in the current directory and its parent tree. 
For every package found in `adl_modules/`, the runtime registers the package root and its common binary/library subdirectories:
- `bin/`
- `lib/`
- `target/release/`
- `target/debug/`
- `build/`
- `src/`

These are added to the global FFI search path, enabling the linker/loader to discover compiled native libraries (such as `.dll` on Windows, `.so` on Linux, or `.dylib` on macOS) without manual setup.

### 2. Integration Across Backends
- **Interpreter & Virtual Machine**: Uses the registered FFI search paths to dynamically locate and bind external C/Rust functions when evaluating code.
- **JIT / Native JIT / AOT**: The Cranelift backends use the registered paths to resolve function prototypes declared via `@cImport("header.h")` or `extern` blocks, compiling direct calls to the resolved library symbols.
- **WebAssembly**: Resolves foreign function mappings inside the WASM linker using the registered FFI search registry.
- **GPU (MLIR/CUDA/Vulkan/Metal)**: Custom GPU kernels compiled from native headers resolve dependency paths using the same search parameters, allowing seamless GPU execution of operations inside installed packages.

