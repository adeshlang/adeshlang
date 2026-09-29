# Adesh Portable Relocatable Object Format (ADOB v2)

The ADOB v2 format is a versioned, portable, relocatable object format designed for fast zero-copy parsing, safe bounds checking, and target independence.

---

## 📑 Binary Format Specification

```
+-------------------------------------------------------+
| Magic: "ADOB" (4 bytes)                               |
| Version: 2 (u16)                                      |
| Architecture ID: u8 (0=x86_64, 1=aarch64, 2=riscv64)  |
| ABI ID: u8 (0=sysv, 1=win_msvc, 2=darwin)             |
| Endianness: u8 (0=little, 1=big)                      |
| Pointer Width: u8 (0=64bit, 1=32bit)                  |
| Flags: u32                                            |
| Checksum: u32                                         |
| Section Count: u32                                    |
| Symbol Count: u32                                     |
+-------------------------------------------------------+
| Section Table Entries...                              |
+-------------------------------------------------------+
| Relocation Table Entries...                           |
+-------------------------------------------------------+
| Symbol Table Entries...                               |
+-------------------------------------------------------+
```

All binary parsing is governed by `BinaryReader<'a>` with strict checked bounds and integer arithmetic.
