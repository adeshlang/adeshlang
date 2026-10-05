//! Phase 10 — Compiler Bootstrap Boundary Interfaces.
//!
//! Provides stable, host-independent interfaces separating:
//! - SourceManager & DiagnosticEngine
//! - ModuleLoader
//! - TypeContext, HIRContext, MIRContext
//! - OptimizationContext, TargetContext, CodegenContext
//! - ObjectWriter & LinkerInterface

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Severity level of compiler diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Note,
    Warning,
    Error,
    Fatal,
}

/// A structured compiler diagnostic with source location and message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub code: Option<String>,
    pub suggestion: Option<String>,
}

/// Centralized diagnostic collection engine.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticEngine {
    diagnostics: Vec<Diagnostic>,
    error_count: usize,
    warning_count: usize,
}

impl DiagnosticEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, diag: Diagnostic) {
        match diag.severity {
            DiagnosticSeverity::Error | DiagnosticSeverity::Fatal => self.error_count += 1,
            DiagnosticSeverity::Warning => self.warning_count += 1,
            _ => {}
        }
        self.diagnostics.push(diag);
    }

    pub fn error(&mut self, msg: impl Into<String>, file: Option<String>, line: Option<u32>) {
        self.emit(Diagnostic {
            severity: DiagnosticSeverity::Error,
            message: msg.into(),
            file,
            line,
            column: None,
            code: None,
            suggestion: None,
        });
    }

    pub fn warning(&mut self, msg: impl Into<String>, file: Option<String>, line: Option<u32>) {
        self.emit(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            message: msg.into(),
            file,
            line,
            column: None,
            code: None,
            suggestion: None,
        });
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }
}

/// Stable Source Manager tracking source file buffers and coordinate mappings.
#[derive(Debug, Clone, Default)]
pub struct SourceManager {
    files: HashMap<String, String>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }

    pub fn get_file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(|s| s.as_str())
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Abstract Module Loader interface for resolving source files across search paths.
pub trait ModuleLoader: Send + Sync {
    fn load_module(&self, module_name: &str) -> Result<String, String>;
    fn resolve_path(&self, module_name: &str) -> Option<PathBuf>;
}

/// Default filesystem-based ModuleLoader implementation.
pub struct FsModuleLoader {
    search_paths: Vec<PathBuf>,
}

impl FsModuleLoader {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }
}

impl ModuleLoader for FsModuleLoader {
    fn load_module(&self, module_name: &str) -> Result<String, String> {
        if let Some(path) = self.resolve_path(module_name) {
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read module '{}' at {}: {}", module_name, path.display(), e))
        } else {
            Err(format!("Module '{}' not found in search paths", module_name))
        }
    }

    fn resolve_path(&self, module_name: &str) -> Option<PathBuf> {
        for base in &self.search_paths {
            let candidate = base.join(format!("{}.adesh", module_name));
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }
}

/// High-level context interfaces across compilation stages.
#[derive(Debug, Default)]
pub struct TypeContext {
    pub type_map: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct HIRContext {
    pub node_count: usize,
}

#[derive(Debug, Default)]
pub struct MIRContext {
    pub basic_block_count: usize,
}

#[derive(Debug, Default)]
pub struct OptimizationContext {
    pub opt_level: u8,
    pub passes_run: Vec<String>,
}

#[derive(Debug, Default)]
pub struct TargetContext {
    pub target_triple: String,
    pub pointer_width: usize,
}

#[derive(Debug, Default)]
pub struct CodegenContext {
    pub instructions_emitted: usize,
}

/// Abstract Object Writer interface decoupling the backend from specific object formats.
pub trait ObjectWriter {
    fn write_object(&mut self, filename: &Path, bytes: &[u8]) -> Result<(), String>;
}

/// Standard file-based ObjectWriter implementation.
pub struct FsObjectWriter;

impl ObjectWriter for FsObjectWriter {
    fn write_object(&mut self, filename: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(filename, bytes)
            .map_err(|e| format!("Failed to write object {}: {}", filename.display(), e))
    }
}

/// Abstract Linker Interface decoupling driver from underlying linker toolchain.
pub trait LinkerInterface {
    fn link(
        &self,
        objects: &[PathBuf],
        output: &Path,
        libraries: &[String],
    ) -> Result<(), String>;
}
