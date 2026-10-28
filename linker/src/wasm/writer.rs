//! WebAssembly Binary Writer.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::section::MergedSection;
use crate::symbol::Symbol;
use crate::target::Target;
use std::fs;
use std::path::Path;

pub struct WasmWriter;

impl WasmWriter {
    pub fn write_binary(
        path: &Path,
        _target: &Target,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<()> {
        let bytes = Self::encode_binary(merged_sections, symbols)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn encode_binary(
        _merged_sections: &[MergedSection],
        _symbols: &[Symbol],
    ) -> LinkResult<Vec<u8>> {
        Err(LinkError::new(
            ErrorCode::UnsupportedFormat,
            "the WebAssembly linker writer is not implemented: it would emit a \
             single hard-coded `() -> i64` function with no import section, no \
             type/function tables and no applied relocations",
        )
        .with_suggestion(
            "WebAssembly modules must be produced by the WASM compiler backend, not \
             by `adeshlink`; a real linker writer needs an import section, memory \
             layout, data segments with applied relocations and a function table.",
        ))
    }
}
