//! Cross-Platform In-Memory Native JIT Loader.
//!
//! Re-exports and integrates `adesh_codegen::jit` directly into the compiler pipeline.
//!
//! All memory management, W^X enforcement, multi-architecture relocation patching,
//! and host symbol resolution are implemented in `adesh_codegen::jit`.

pub use adesh_codegen::jit::{
    HostSymbolResolver, JitLoader, JitLoaderError, JitMemoryBlock, JitMemoryError, LoadedJitModule,
    StandardHostSymbolResolver,
};
