//! Linker execution context and pipeline state.

use crate::archive::Archive;
use crate::config::LinkConfig;
use crate::diagnostics::DiagnosticEngine;
use crate::layout::LayoutEngine;
use crate::object::ObjectFile;
use crate::resolver::SymbolResolver;

/// Context holding shared state during a link invocation.
pub struct LinkContext {
    pub config: LinkConfig,
    pub diagnostics: DiagnosticEngine,
    pub objects: Vec<ObjectFile>,
    pub archives: Vec<Archive>,
    pub resolver: SymbolResolver,
    pub layout: LayoutEngine,
}

impl LinkContext {
    pub fn new(config: LinkConfig) -> Self {
        Self {
            config,
            diagnostics: DiagnosticEngine::new(),
            objects: Vec::new(),
            archives: Vec::new(),
            resolver: SymbolResolver::new(),
            layout: LayoutEngine::new(),
        }
    }
}
