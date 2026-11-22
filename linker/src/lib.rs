//! # Adesh Linker (`adesh_linker`)
//!
//! A self-contained, multi-format, multi-architecture native linker for the Adesh ecosystem.
//!
//! Inspired by the design and philosophy of Go's `cmd/link`, `adesh_linker` links native
//! object files into final executable binaries, dynamic libraries, WebAssembly artifacts,
//! GPU fatbins, AI accelerator payloads, and Quantum QIR packages without requiring external
//! toolchains or system linkers (LLVM/lld, GNU ld, MSVC link.exe).

pub mod abi;
pub mod accelerators;
pub mod arch;
pub mod archive;
pub mod cache;
pub mod codegen;
pub mod config;
pub mod context;
pub mod debug;
pub mod diagnostics;
pub mod elf;
pub mod embedded;
pub mod error;
pub mod gc;
pub mod hash;
pub mod icf;
pub mod incremental;
pub mod intrinsics;
pub mod layout;
pub mod linker;
pub mod macho;
pub mod map;
pub mod metadata;
pub mod mlir;
pub mod object;
pub mod os;
pub mod os_router;
pub mod pe;
pub mod quantum;
pub mod relocation;
pub mod resolver;
pub mod section;
pub mod symbol;
pub mod target;
pub mod tls;
pub mod unwind;
pub mod wasm;
pub mod xcoff;

pub use config::{BuildIdStyle, IcfMode, LinkConfig, MapFormat};
pub use diagnostics::{Diagnostic, DiagnosticEngine, DiagnosticLevel};
pub use error::{ErrorCode, LinkError, LinkResult};
pub use intrinsics::IntrinsicsEngine;
pub use linker::Linker;
pub use object::ObjectFile;
pub use relocation::{Relocation, RelocationHandler, RelocationKind};
pub use section::{Section, SectionKind};
pub use symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
pub use target::{
    Arch, Endianness, ObjectFormat, Os, PointerWidth, RelocationModel, Target, TargetTier,
};
pub use unwind::{EhFrameHdrGenerator, RaiiDropTable, WindowsPdataGenerator};

use std::path::{Path, PathBuf};

/// High-level API to link object files with custom configuration.
pub fn link_with_config(inputs: &[PathBuf], config: LinkConfig) -> LinkResult<()> {
    Linker::link(inputs, config)
}

/// High-level API to link object files into an executable at `output_path` using default settings.
pub fn link(
    inputs: &[impl AsRef<Path>],
    output_path: impl AsRef<Path>,
    target_triple: Option<&str>,
) -> LinkResult<()> {
    let target = if let Some(triple) = target_triple {
        Target::from_triple(triple)?
    } else {
        Target::host()
    };

    let input_paths: Vec<PathBuf> = inputs.iter().map(|p| p.as_ref().to_path_buf()).collect();
    let config = LinkConfig::new(output_path.as_ref().to_path_buf(), target);

    Linker::link(&input_paths, config)
}
