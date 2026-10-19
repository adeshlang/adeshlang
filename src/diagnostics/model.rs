//! Adesh Diagnostics Data Model
//!
//! Shared diagnostic system for AdeshLang compiler, linker, runtime, and CLI.
//! Decouples diagnostic detection from rendering, serialization, and reporting.

use super::registry::{DiagnosticPhase, DiagnosticSeverity, lookup_diagnostic};
use serde::{Deserialize, Serialize};

/// Canonical Source ID / File Identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId {
    pub path: String,
}

impl SourceId {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

/// Source Span with 1-based lines and columns, plus byte offsets when known.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<usize>,
    pub line_text: String,
}

impl Span {
    pub fn new(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        line_text: impl Into<String>,
    ) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
            start_offset: None,
            end_offset: None,
            line_text: line_text.into(),
        }
    }

    pub fn point(line: usize, col: usize, line_text: impl Into<String>) -> Self {
        Self::new(line, col, line, col, line_text)
    }

    pub fn len(&self) -> usize {
        if self.end_line == self.start_line && self.end_col >= self.start_col {
            (self.end_col - self.start_col + 1).max(1)
        } else {
            1
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start_line == 0 && self.start_col == 0
    }
}

/// Secondary or contextual label attached to a source location
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub file: Option<String>,
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            file: None,
            span,
            message: message.into(),
            is_primary: true,
        }
    }

    pub fn secondary(
        file: Option<String>,
        line: usize,
        col: usize,
        line_text: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            file,
            span: Span::point(line, col, line_text),
            message: message.into(),
            is_primary: false,
        }
    }
}

/// Machine-applicable code suggestion
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub span: Span,
    pub replacement: String,
    pub description: String,
}

/// Core structured diagnostic representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: Option<String>,
    pub severity: DiagnosticSeverity,
    pub phase: DiagnosticPhase,
    pub message: String,
    pub file: Option<String>,
    pub primary_span: Option<Span>,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub helps: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    pub fn new(
        severity: DiagnosticSeverity,
        phase: DiagnosticPhase,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: None,
            severity,
            phase,
            message: message.into(),
            file: None,
            primary_span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn error(phase: DiagnosticPhase, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Error, phase, message)
    }

    pub fn warning(phase: DiagnosticPhase, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Warning, phase, message)
    }

    pub fn info(phase: DiagnosticPhase, message: impl Into<String>) -> Self {
        Self::new(DiagnosticSeverity::Info, phase, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        let c = code.into();
        self.code = Some(c);
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        let f = file.into();
        if !f.is_empty() {
            self.file = Some(f);
        }
        self
    }

    pub fn with_file_opt(mut self, file: Option<String>) -> Self {
        if let Some(f) = file {
            if !f.is_empty() {
                self.file = Some(f);
            }
        }
        self
    }

    pub fn with_primary_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.helps.push(help.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }

    pub fn is_warning(&self) -> bool {
        self.severity == DiagnosticSeverity::Warning
    }

    pub fn is_info(&self) -> bool {
        self.severity == DiagnosticSeverity::Info
    }

    /// Automatically append explanation help if a recognized code is set
    pub fn with_explain_hint(mut self) -> Self {
        if let Some(ref code) = self.code {
            if lookup_diagnostic(code).is_some() {
                let hint = format!("run `adesh --explain {}` for more information", code);
                if !self.helps.iter().any(|h| h == &hint) {
                    self.helps.push(hint);
                }
            }
        }
        self
    }
}
