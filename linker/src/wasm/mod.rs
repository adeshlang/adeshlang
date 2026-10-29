//! WebAssembly subsystem.

pub mod link_info;
pub mod object_info;
pub mod reloc;
pub mod relocations;
pub mod sections;
pub mod writer;

pub use link_info::WasmLinkInfo;
pub use relocations::apply as apply_relocations;
pub use sections::WasmReader;
pub use writer::WasmWriter;
