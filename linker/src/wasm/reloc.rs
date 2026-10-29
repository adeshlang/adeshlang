//! WebAssembly relocation types.
//!
//! Numbers follow LLVM's `Wasm.h` (`R_WASM_*`), which is what real wasm
//! object files — including the ones emitted by clang/lld — encode in their
//! `reloc.<section>` custom sections. The previous constants in this file
//! disagreed with that numbering above type 13 and were never referenced, so
//! they were corrected rather than kept.

use crate::relocation::RelocationKind;

pub const R_WASM_FUNCTION_INDEX_LEB: u8 = 0;
pub const R_WASM_TABLE_INDEX_SLEB: u8 = 1;
pub const R_WASM_TABLE_INDEX_I32: u8 = 2;
pub const R_WASM_MEMORY_ADDR_LEB: u8 = 3;
pub const R_WASM_MEMORY_ADDR_SLEB: u8 = 4;
pub const R_WASM_MEMORY_ADDR_I32: u8 = 5;
pub const R_WASM_TYPE_INDEX_LEB: u8 = 6;
pub const R_WASM_GLOBAL_INDEX_LEB: u8 = 7;
pub const R_WASM_FUNCTION_OFFSET_I32: u8 = 8;
pub const R_WASM_SECTION_OFFSET_I32: u8 = 9;
pub const R_WASM_EVENT_INDEX_LEB: u8 = 10;
pub const R_WASM_MEMORY_ADDR_REL_SLEB: u8 = 11;
pub const R_WASM_TABLE_INDEX_REL_SLEB: u8 = 12;
pub const R_WASM_GLOBAL_INDEX_I32: u8 = 13;
pub const R_WASM_MEMORY_ADDR_LEB64: u8 = 14;
pub const R_WASM_MEMORY_ADDR_SLEB64: u8 = 15;
pub const R_WASM_MEMORY_ADDR_I64: u8 = 16;
pub const R_WASM_MEMORY_ADDR_REL_SLEB64: u8 = 17;
pub const R_WASM_TABLE_INDEX_SLEB64: u8 = 18;
pub const R_WASM_TABLE_INDEX_I64: u8 = 19;
pub const R_WASM_TABLE_NUMBER_LEB: u8 = 20;
pub const R_WASM_MEMORY_ADDR_TLS_SLEB: u8 = 21;
pub const R_WASM_FUNCTION_OFFSET_I64: u8 = 22;
pub const R_WASM_MEMORY_ADDR_LOCREL_I32: u8 = 23;

/// The linker's relocation kind for a wasm relocation type, or `None` when the
/// type is not implemented. Callers must treat `None` as a hard error: emitting
/// an unrelocated slot silently produces a module that traps or miscomputes.
///
/// Each type maps onto a kind that preserves its on-disk encoding: LEB types
/// patch a padded LEB128 placeholder, while `I32` types patch a fixed 32-bit
/// word. Table-index types are *not* function indices — a table slot only
/// exists relative to an element segment, which this linker does not emit — so
/// they are unimplemented rather than approximated.
pub fn relocation_kind(ty: u8) -> Option<RelocationKind> {
    match ty {
        R_WASM_FUNCTION_INDEX_LEB => Some(RelocationKind::WasmFunctionIndex),
        R_WASM_MEMORY_ADDR_LEB => Some(RelocationKind::WasmMemoryOffset32),
        R_WASM_MEMORY_ADDR_SLEB => Some(RelocationKind::WasmMemoryOffsetSleb),
        R_WASM_MEMORY_ADDR_I32 => Some(RelocationKind::WasmMemoryOffsetI32),
        R_WASM_TYPE_INDEX_LEB => Some(RelocationKind::WasmTypeIndex),
        R_WASM_GLOBAL_INDEX_LEB => Some(RelocationKind::WasmGlobalIndex),
        R_WASM_GLOBAL_INDEX_I32 => Some(RelocationKind::WasmGlobalIndexI32),
        _ => None,
    }
}

/// Whether the relocation entry carries an explicit addend in the object file.
/// Only reachable types need an answer here: an unimplemented type is rejected
/// before its addend would be read.
pub fn has_addend(ty: u8) -> bool {
    matches!(
        ty,
        R_WASM_MEMORY_ADDR_LEB
            | R_WASM_MEMORY_ADDR_SLEB
            | R_WASM_MEMORY_ADDR_I32
            | R_WASM_MEMORY_ADDR_REL_SLEB
            | R_WASM_MEMORY_ADDR_LEB64
            | R_WASM_MEMORY_ADDR_SLEB64
            | R_WASM_MEMORY_ADDR_I64
            | R_WASM_MEMORY_ADDR_REL_SLEB64
            | R_WASM_MEMORY_ADDR_TLS_SLEB
            | R_WASM_MEMORY_ADDR_LOCREL_I32
    )
}
