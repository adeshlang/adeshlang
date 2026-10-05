# AdeshLang Package Management: `adesh.adl` & `adesh.lock.adl`

This guide explains the curated **ADL** (`adesh.adl`) package manifest and **ADL Lock** (`adesh.lock.adl`) lockfile architecture introduced in Phase 9.

---

## 1. Overview

AdeshLang uses dedicated `.adl` files to declare project dependencies, compiler profiles, feature flags, and build metadata:

- **`adesh.adl`**: The human-readable project manifest declaring dependencies, features, build profiles, and metadata.
- **`adesh.lock.adl`**: The machine-generated deterministic lockfile that locks every direct and transitive dependency to an exact version and cryptographic content checksum.

---

## 2. Manifest Format (`adesh.adl`)

A standard `adesh.adl` file is organized into explicit sections:

```toml
[package]
name = "my-application"
version = "1.0.0"
edition = "2026"
authors = ["Adesh Developer <dev@adesh.dev>"]
description = "A fast, native application built with AdeshLang"
license = "MIT OR Apache-2.0"
entry = "src/main.adesh"

[dependencies]
adesh-math = "^1.2.0"
adesh-simd = { version = "~0.4.0", features = ["avx2", "fma"] }
adesh-net = { version = "0.8.0", optional = true }

[dev_dependencies]
adesh-benchmark = "0.2.0"

[features]
default = ["accelerated"]
accelerated = ["adesh-simd"]
networking = ["adesh-net"]

[profile.dev]
opt_level = 0
debug = true

[profile.release]
opt_level = 3
lto = "fat"
debug = false
panic = "abort"

[workspace]
members = ["packages/*", "crates/*"]
exclude = ["temp"]
```

### Key Sections:
- **`[package]`**: Defines package name, semantic version, language edition, optional author list, and entrypoint source file.
- **`[dependencies]`**: Direct dependencies. Supports simple semantic version strings (`"^1.2.0"`) or detailed tables with `features`, `path` (local workspace path), `optional`, and `default_features`.
- **`[dev_dependencies]`**: Dependencies only required during compilation of benchmarks and test suites.
- **`[features]`**: Named feature gates enabling optional dependencies and conditional compilation flags.
- **`[profile.<name>]`**: Compiler flags per build target, including optimization levels (`0` through `3`), LTO (`"thin"` / `"fat"`), debug symbol inclusion, and panic strategy (`"abort"` / `"unwind"`).
- **`[workspace]`**: Multi-package workspace root defining member patterns.

---

## 3. Lockfile Format (`adesh.lock.adl`)

When dependencies are resolved via `adesh build` or `adl install`, the toolchain generates `adesh.lock.adl`:

```toml
version = 1

[[packages]]
name = "adesh-core"
version = "1.0.0"
source = "registry+https://pkg.adesh.dev"
checksum = "c79ab62688f407dd884ad2733973c52e1858a74ec7846f790c6a51d8b67104b2"
dependencies = []
enabled_features = []

[[packages]]
name = "adesh-math"
version = "1.2.4"
source = "registry+https://pkg.adesh.dev"
checksum = "5d41402abc4b2a76b9719d911017c592b23a7bb3ad93f619e9508d519b49f2b8"
dependencies = [
    "adesh-core",
]
enabled_features = []

[[packages]]
name = "adesh-simd"
version = "0.4.2"
source = "registry+https://pkg.adesh.dev"
checksum = "13fa8959223bb2e1ad15220c3298a002bc09ef784d852a36bca487319c5c2d11"
dependencies = [
    "adesh-core",
]
enabled_features = [
    "avx2",
    "fma",
]
```

### Determinism Guarantees:
1. **Identical Resolution**: The same `adesh.adl` and `adesh.lock.adl` will always resolve to the exact same dependency set across all operating systems.
2. **Cryptographic Integrity**: Package contents are verified against SHA-256 checksums before compilation.
3. **Transitive Transparency**: Every indirect dependency is locked to prevent surprise breaking updates.

---

## 4. Fine-Grained Incremental Compilation

The package system integrates with AdeshLang's cryptographic cache:

```text
               Source Code ───► Source Hash ────┐
                                                │
          Public Signatures ───► Interface Hash ─┼──► ModuleFingerprint
                                                │            │
            Compiler Flags ───► Flags Hash ─────┤            ▼
                                                │     Incremental Cache
          Dependencies Map ───► Dep Hash Map ───┘   (Cache Hit / Rebuild)
```

- **Implementation-only changes**: Modifying a function's private body changes `source_hash`, triggering a rebuild of only that module. Downstream modules skip rebuilding because `interface_hash` is unchanged.
- **Interface changes**: Changing public structs or function signatures changes `interface_hash`, triggering an automatic rebuild of dependent modules.

---

## 5. Running the Example in Debug Mode

A complete executable demo is provided in `examples/adl_package_demo.rs`.

Run it in debug mode using:

```bash
cargo run --example adl_package_demo
```

This runs the demonstration workflow:
1. Dynamically constructs an `adesh.adl` manifest on disk.
2. Resolves direct and transitive dependencies deterministically.
3. Emits and verifies `adesh.lock.adl`.
4. Tests the incremental compiler cache across clean builds, cache hits, and signature modifications.
