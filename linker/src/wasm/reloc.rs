//! WebAssembly relocation types and link-time fixups.

pub const R_WASM_FUNCTION_INDEX_LEB: u32 = 0;
pub const R_WASM_TABLE_INDEX_SLEB: u32 = 1;
pub const R_WASM_TABLE_INDEX_I32: u32 = 2;
pub const R_WASM_MEMORY_ADDR_LEB: u32 = 3;
pub const R_WASM_MEMORY_ADDR_SLEB: u32 = 4;
pub const R_WASM_MEMORY_ADDR_I32: u32 = 5;
pub const R_WASM_TYPE_INDEX_LEB: u32 = 6;
pub const R_WASM_GLOBAL_INDEX_LEB: u32 = 7;
pub const R_WASM_FUNCTION_OFFSET_I32: u32 = 8;
pub const R_WASM_SECTION_OFFSET_I32: u32 = 9;
pub const R_WASM_EVENT_INDEX_LEB: u32 = 10;
pub const R_WASM_GLOBAL_INDEX_I32: u32 = 13;
pub const R_WASM_MEMORY_ADDR_LOCREL_I32: u32 = 15;
pub const R_WASM_TABLE_INDEX_REL_SLEB: u32 = 16;
pub const R_WASM_TABLE_INDEX_REL_SLEB64: u32 = 17;
pub const R_WASM_MEMORY_ADDR_LEB64: u32 = 18;
pub const R_WASM_MEMORY_ADDR_SLEB64: u32 = 19;
pub const R_WASM_MEMORY_ADDR_I64: u32 = 20;
pub const R_WASM_FUNCTION_INDEX_I32: u32 = 22;
