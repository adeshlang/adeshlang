//! MLIR (Multi-Level Intermediate Representation) dialect and tensor graph linking.
//!
//! Provides self-contained MLIR dialect registration, bytecode container encoding/decoding,
//! polyhedral loop fusion metadata, and tensor graph symbol resolution without LLVM/MLIR C++ dependencies.

pub mod bytecode;
pub mod dialect;

pub use bytecode::{MlirBytecodeReader, MlirBytecodeWriter, MlirModule};
pub use dialect::{DialectKind, DialectOp, DialectRegistry, PolyhedralSchedule, TensorShape};
