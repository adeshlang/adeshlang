# Relocation Engine (`relocation`)

The Relocation Engine resolves symbolic addresses into absolute or relative offsets within the final binary image.

---

## 1. Relocation Formula Reference

| Relocation Kind | Calculation | Size | Description |
|---|---|---|---|
| **Absolute 64** | `S + A` | 8 bytes | Direct 64-bit absolute address |
| **Absolute 32** | `(S + A) as u32` | 4 bytes | Direct 32-bit absolute address (zero-extended) |
| **Absolute 16** | `(S + A) as u16` | 2 bytes | Direct 16-bit absolute address |
| **Absolute 8**  | `(S + A) as u8`  | 1 byte  | Direct 8-bit absolute address |
| **PC-Relative 32** | `S + A - P` | 4 bytes | 32-bit signed offset relative to the program counter `P` |
| **PC-Relative 64** | `S + A - P` | 8 bytes | 64-bit signed offset relative to `P` |
| **PLT Relative 32** | `L + A - P` | 4 bytes | 32-bit relative offset to Procedure Linkage Table entry |
| **GOT Relative 32** | `G + A - P` | 4 bytes | 32-bit relative offset to Global Offset Table entry |
| **Section Relative 32** | `S + A - SecBase` | 4 bytes | Offset from section start (PE RVA, DWARF) |
| **AArch64 CALL26** | `(S + A - P) >> 2` | 4 bytes | 26-bit immediate branch encoded into instruction `BL` |
| **AArch64 ADRP** | `Page(S+A) - Page(P)` | 4 bytes | 21-bit page relative offset |
| **RISC-V CALL** | `S + A - P` | 8 bytes | `AUIPC` + `JALR` pair |
| **RISC-V BRANCH** | `S + A - P` | 4 bytes | 12-bit signed branch |

*Notation:*
- `S`: Value (Virtual Address) of the referenced symbol.
- `A`: Addend associated with the relocation record.
- `P`: Place (Virtual Address) where the relocation is being applied.
- `G`: Address of the GOT entry for the symbol.
- `L`: Address of the PLT entry for the symbol.
- `SecBase`: Virtual address of the containing output section.
