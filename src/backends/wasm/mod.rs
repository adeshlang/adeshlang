pub mod compiler;
pub mod encoder;
pub mod instructions;
pub mod wasi;

pub use compiler::WasmAbi;
pub use compiler::compile_to_file;
pub use compiler::compile_to_file_with_base;
pub use compiler::compile_to_file_with_base_and_abi;
pub use compiler::write_js_loader;
