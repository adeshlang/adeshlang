# Adesh Linker (`adeshlink`) Integration & Pipeline

## 1. Overview

`adeshlink` is the self-contained native linker for the Adesh ecosystem. It consumes ADOB files, resolves symbol dependencies across modules and archives, applies relocations, and outputs platform-native binaries:

* **Windows**: PE32+ (`.exe`, `.dll`)
* **Linux / BSD**: ELF64 / ELF32 (executables and `.so`)
* **macOS / iOS**: Mach-O (Mach-O 64-bit binaries and `.dylib`)
* **Web**: WebAssembly (`.wasm`)
* **Embedded**: Raw binary images (`.bin`, `.hex`, freestanding ELF)

---

## 2. Linker Ingestion Pipeline

```text
ADOB Input Files (*.adob, *.o)
              │
              ▼
   ADOB Binary Deserializer
 (adesh_object::AdobReader)
              │
              ▼
    Symbol & Relocation Graph
              │
              ▼
   Section Layout & Alignment
              │
              ▼
 Relocation Resolution & Fixups
              │
              ▼
   Platform Executable Writer
  (PE / ELF / Mach-O / WASM)
```

---

## 3. Linker CLI Usage

```bash
# Link multiple ADOB object files
adeshlink main.adob runtime.adob -o app.exe

# Cross-compile for Linux from Windows
adeshlink main.adob runtime.adob --target=x86_64-unknown-linux-gnu -o app
```
