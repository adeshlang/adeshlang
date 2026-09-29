//! WebAssembly subsystem.

pub mod reloc;
pub mod sections;
pub mod writer;

pub use sections::WasmReader;
pub use writer::WasmWriter;
