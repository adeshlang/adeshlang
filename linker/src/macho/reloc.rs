//! Mach-O 64-bit relocations definitions.

// ─── x86_64 (symbol num / section based, `relocation_info`) ─────────────────
pub const X86_64_RELOC_UNSIGNED: u32 = 0;
pub const X86_64_RELOC_SIGNED: u32 = 1;
pub const X86_64_RELOC_BRANCH: u32 = 2;
pub const X86_64_RELOC_GOT_LOAD: u32 = 3;
pub const X86_64_RELOC_GOT: u32 = 4;
pub const X86_64_RELOC_SUBTRACTOR: u32 = 5;
pub const X86_64_RELOC_SIGNED_1: u32 = 6;
pub const X86_64_RELOC_SIGNED_2: u32 = 7;
pub const X86_64_RELOC_SIGNED_4: u32 = 8;
/// Thread-local access: the 4-byte field is the RIP-relative displacement of
/// a `movq desc(%rip), %reg` and the linker must turn that load into a `leaq`
/// so the register receives the *address* of the TLV descriptor.
pub const X86_64_RELOC_TLV: u32 = 9;

// ─── AArch64 ────────────────────────────────────────────────────────────────
pub const ARM64_RELOC_UNSIGNED: u32 = 0;
pub const ARM64_RELOC_SUBTRACTOR: u32 = 1;
pub const ARM64_RELOC_BRANCH26: u32 = 2;
pub const ARM64_RELOC_PAGE21: u32 = 3;
pub const ARM64_RELOC_PAGEOFF12: u32 = 4;
pub const ARM64_RELOC_GOT_LOAD_PAGE21: u32 = 5;
pub const ARM64_RELOC_GOT_LOAD_PAGEOFF12: u32 = 6;
pub const ARM64_RELOC_POINTER_TO_GOT: u32 = 7;
/// `adrp` half of a TLV descriptor address computation.
pub const ARM64_RELOC_TLVP_LOAD_PAGE21: u32 = 8;
/// `ldr` half of the same pair: the linker rewrites the load into an `add` so
/// the register holds the descriptor address, not `*descriptor`.
pub const ARM64_RELOC_TLVP_LOAD_PAGEOFF12: u32 = 9;
/// Carries a 24-bit addend for the relocation entry that follows it.
pub const ARM64_RELOC_ADDEND: u32 = 10;
