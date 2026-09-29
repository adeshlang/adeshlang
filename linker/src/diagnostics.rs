//! Diagnostic formatting, structured reporting, and linker telemetry.

use crate::error::LinkError;

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

/// A structured diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub notes: Vec<String>,
    pub suggestions: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            code: Some(code.to_string()),
            message: message.into(),
            file: None,
            line: None,
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            code: None,
            message: message.into(),
            file: None,
            line: None,
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn from_link_error(err: &LinkError) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            code: Some(err.code.as_str().to_string()),
            message: err.message.clone(),
            file: err.file.clone(),
            line: None,
            notes: err.notes.clone(),
            suggestions: err.suggestions.clone(),
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        match self.level {
            DiagnosticLevel::Error => {
                if let Some(ref code) = self.code {
                    out.push_str(&format!("error[{}]: {}\n", code, self.message));
                } else {
                    out.push_str(&format!("error: {}\n", self.message));
                }
            }
            DiagnosticLevel::Warning => {
                out.push_str(&format!("warning: {}\n", self.message));
            }
            DiagnosticLevel::Note => {
                out.push_str(&format!("note: {}\n", self.message));
            }
            DiagnosticLevel::Help => {
                out.push_str(&format!("help: {}\n", self.message));
            }
        }

        if let Some(ref file) = self.file {
            out.push_str(&format!("  --> in file: {}\n", file));
        }

        for note in &self.notes {
            out.push_str(&format!("  ::: note: {}\n", note));
        }

        for sug in &self.suggestions {
            out.push_str(&format!("  --> help: {}\n", sug));
        }

        out
    }
}

/// Diagnostics collector and engine.
#[derive(Debug, Default)]
pub struct DiagnosticEngine {
    diagnostics: Vec<Diagnostic>,
    has_errors: bool,
}

impl DiagnosticEngine {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            has_errors: false,
        }
    }

    pub fn emit(&mut self, diag: Diagnostic) {
        if diag.level == DiagnosticLevel::Error {
            self.has_errors = true;
        }
        self.diagnostics.push(diag);
    }

    pub fn emit_error(&mut self, err: &LinkError) {
        self.emit(Diagnostic::from_link_error(err));
    }

    pub fn emit_warning(&mut self, message: impl Into<String>) {
        self.emit(Diagnostic::warning(message));
    }

    pub fn has_errors(&self) -> bool {
        self.has_errors
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn print_all(&self) {
        for diag in &self.diagnostics {
            eprint!("{}", diag.render());
        }
    }
}
