//! Error Types and Diagnostics
//!
//! Provides `LangError` with rich, colored diagnostics including file, line,
//! column, snippet, caret markers, related locations, and optional hints/notes.
//! Error kinds cover lexical, parse, runtime, type, IO, compile/lowering, and JIT stages.
//!
//! Location format uses the clickable standard: `path:line:col` (1-based),
//! printed on its own `-->` line for terminal and IDE navigation.

use colored::Colorize;
use std::fmt;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    Lexical,
    Parse,
    Runtime,
    Type,
    Io,
    User,
    Compile,
    Lowering,
    Jit,
    /// Ownership / borrow / memory-safety diagnostics
    Ownership,
}

impl ErrorKind {
    pub fn label(&self) -> &'static str {
        match self {
            ErrorKind::Lexical => "LexicalError",
            ErrorKind::Parse => "ParseError",
            ErrorKind::Runtime => "RuntimeError",
            ErrorKind::Type => "TypeError",
            ErrorKind::Io => "IoError",
            ErrorKind::User => "UserError",
            ErrorKind::Compile => "CompileError",
            ErrorKind::Lowering => "LoweringError",
            ErrorKind::Jit => "JitError",
            ErrorKind::Ownership => "OwnershipError",
        }
    }
}

/// Source location span for better error messages
#[derive(Debug, Clone, Default)]
pub struct SourceSpan {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl SourceSpan {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    pub fn point(line: usize, col: usize) -> Self {
        Self {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
        }
    }
}

/// Secondary location shown as a note (e.g. "value moved here")
#[derive(Debug, Clone)]
pub struct RelatedLocation {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
    pub line_text: String,
    pub label: String,
}

impl RelatedLocation {
    pub fn new(
        file: Option<String>,
        line: usize,
        col: usize,
        line_text: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            file,
            line,
            col,
            line_text: line_text.into(),
            label: label.into(),
        }
    }

    /// Clickable `file:line:col` or `line N col M`
    pub fn clickable(&self) -> String {
        format_clickable_location(self.file.as_deref(), self.line, self.col)
    }
}

/// Build a terminal/IDE-clickable location string (`path:line:col`, 1-based).
pub fn format_clickable_location(file: Option<&str>, line: usize, col: usize) -> String {
    match file {
        Some(f) if !f.is_empty() => {
            if col > 0 {
                format!("{}:{}:{}", f, line.max(1), col)
            } else {
                format!("{}:{}", f, line.max(1))
            }
        }
        _ => {
            if col > 0 {
                format!("line {} col {}", line.max(1), col)
            } else if line > 0 {
                format!("line {}", line)
            } else {
                "unknown location".to_string()
            }
        }
    }
}

/// Build a caret underline for a 1-based column and optional span length.
pub fn format_caret(col: usize, span_len: usize) -> String {
    let col0 = if col > 0 { col - 1 } else { 0 };
    let len = span_len.max(1);
    let mut caret = String::new();
    for _ in 0..col0 {
        caret.push(' ');
    }
    caret.push('^');
    for _ in 1..len {
        caret.push('~');
    }
    caret
}

#[derive(Debug, Clone)]
pub struct LangError {
    pub kind: ErrorKind,
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub line_text: String,
    pub file: Option<String>,
    pub hint: Option<String>,
    pub note: Option<String>,
    /// Optional stable error code (e.g. "E0382")
    pub code: Option<String>,
    /// Extra free-form notes (printed after the primary span)
    pub notes: Vec<String>,
    /// Related source locations (moved-here, borrowed-here, …)
    pub related: Vec<RelatedLocation>,
}

impl LangError {
    pub fn new(
        kind: ErrorKind,
        message: String,
        line: usize,
        col: usize,
        line_text: String,
    ) -> Self {
        Self {
            kind,
            message,
            line,
            col,
            end_line: line,
            end_col: col,
            line_text,
            file: None,
            hint: None,
            note: None,
            code: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Create an error with a span
    pub fn with_span(
        kind: ErrorKind,
        message: String,
        span: SourceSpan,
        line_text: String,
    ) -> Self {
        Self {
            kind,
            message,
            line: span.start_line,
            col: span.start_col,
            end_line: span.end_line,
            end_col: span.end_col,
            line_text,
            file: None,
            hint: None,
            note: None,
            code: None,
            notes: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Create a fully located error with file path
    pub fn located(
        kind: ErrorKind,
        message: impl Into<String>,
        file: Option<String>,
        line: usize,
        col: usize,
        line_text: impl Into<String>,
    ) -> Self {
        let mut e = Self::new(kind, message.into(), line, col, line_text.into());
        e.file = file;
        e
    }

    /// Add a hint to help fix the error
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Alias for [`with_hint`]
    pub fn with_help(self, help: impl Into<String>) -> Self {
        self.with_hint(help)
    }

    /// Add an additional note
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn push_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_related(mut self, related: RelatedLocation) -> Self {
        self.related.push(related);
        self
    }

    pub fn with_file(mut self, file: String) -> Self {
        self.file = Some(file);
        self
    }

    /// Optionally attach a file path when present
    pub fn with_file_opt(mut self, file: Option<String>) -> Self {
        if let Some(f) = file {
            if !f.is_empty() {
                self.file = Some(f);
            }
        }
        self
    }

    /// Clickable primary location (`path:line:col`)
    pub fn clickable_location(&self) -> String {
        format_clickable_location(self.file.as_deref(), self.line, self.col)
    }

    /// Attach common parse/type hints based on the message text
    pub fn with_auto_hints(mut self) -> Self {
        if self.hint.is_some() {
            return self;
        }
        let msg = self.message.to_lowercase();
        let hint = if msg.contains("expect expression") || msg.contains("expected expression") {
            Some(
                "an expression is required here (e.g. a value, variable, call, or literal). \
                 check for a missing value after `=` or an extra operator."
                    .to_string(),
            )
        } else if msg.contains("expect") && msg.contains(";") {
            Some("statements usually end with `;`. check for a missing semicolon.".to_string())
        } else if msg.contains("unexpected token") || msg.contains("unexpected eof") {
            Some(
                "check for mismatched braces `{}`, parentheses `()`, brackets `[]`, \
                 or a truncated statement."
                    .to_string(),
            )
        } else if msg.contains("unterminated string") {
            Some("string literals must be closed with a matching quote `\"`.".to_string())
        } else if msg.contains("unknown variable") || msg.contains("undefined") {
            Some(
                "declare the name with `let`/`const` before use, or check the spelling."
                    .to_string(),
            )
        } else if msg.contains("type") && (msg.contains("expected") || msg.contains("mismatch")) {
            Some(
                "ensure the value's type matches the annotation or parameter type; \
                 use an explicit conversion if needed."
                    .to_string(),
            )
        } else {
            None
        };
        if let Some(h) = hint {
            self.hint = Some(h);
        }
        self
    }

    /// Format the error with colored output, caret marker, related spans, hints, and notes
    fn format_error(&self) -> String {
        let kind_str = self.kind.label();
        let header = if let Some(code) = &self.code {
            format!("error[{}]: {}", code, self.message)
                .red()
                .bold()
                .to_string()
        } else {
            format!("{}: {}", kind_str, self.message)
                .red()
                .bold()
                .to_string()
        };

        let loc = self.clickable_location();
        let arrow = format!("  --> {}", loc).cyan().to_string();

        let mut result = format!("{}\n{}", header, arrow);

        // Fetch line text from file if empty but file & line are specified
        let mut source_line = self.line_text.clone();
        if source_line.is_empty() && self.line > 0 {
            if let Some(file_path) = &self.file {
                if let Ok(content) = std::fs::read_to_string(file_path) {
                    if let Some(l) = content.lines().nth(self.line - 1) {
                        source_line = l.to_string();
                    }
                }
            }
        }

        // Primary snippet with Rust-style layout
        if self.line > 0 || !source_line.is_empty() {
            let line_no = self.line.max(1);
            let gutter_width = line_no.to_string().len().max(2);
            let empty_gutter = format!("{:>width$} |", "", width = gutter_width)
                .blue()
                .to_string();
            let line_gutter = format!("{:>width$} | ", line_no, width = gutter_width)
                .blue()
                .to_string();

            let span_len = if self.end_col > self.col && self.end_line == self.line {
                self.end_col - self.col
            } else if !source_line.is_empty() && self.col > 0 {
                1
            } else {
                1
            };
            let caret = format_caret(self.col, span_len);
            let inline_msg = format!(" {}", self.message).red().bold().to_string();

            // Rust-style diagnostic structure:
            //   --> file:line:col
            //    |
            // 14 | source line content
            //    | ^^^^^^^^^^ error message
            //    |
            result.push('\n');
            result.push_str(&empty_gutter);
            result.push('\n');
            result.push_str(&line_gutter);
            result.push_str(&source_line);
            result.push('\n');
            result.push_str(&empty_gutter);
            result.push(' ');
            result.push_str(&caret.red().to_string());
            result.push_str(&inline_msg);
            result.push('\n');
            result.push_str(&empty_gutter);
        }

        // Related locations
        for rel in &self.related {
            let rel_arrow = format!("  --> {}", rel.clickable()).cyan();
            result.push_str(&format!(
                "\n{}\n{} {}",
                format!("note: {}", rel.label).yellow(),
                rel_arrow,
                ""
            ));
            let mut rel_line_text = rel.line_text.clone();
            if rel_line_text.is_empty() && rel.line > 0 {
                if let Some(file_path) = &rel.file {
                    if let Ok(content) = std::fs::read_to_string(file_path) {
                        if let Some(l) = content.lines().nth(rel.line - 1) {
                            rel_line_text = l.to_string();
                        }
                    }
                }
            }
            if !rel_line_text.is_empty() || rel.line > 0 {
                let r_line = rel.line.max(1);
                let r_width = r_line.to_string().len().max(2);
                let r_empty = format!("{:>width$} |", "", width = r_width).blue().to_string();
                let r_gutter = format!("{:>width$} | ", r_line, width = r_width).blue().to_string();
                let caret = format_caret(rel.col, 1);
                let inline_label = if !rel.label.is_empty() {
                    format!(" {}", rel.label).yellow().to_string()
                } else {
                    String::new()
                };

                result.push('\n');
                result.push_str(&r_empty);
                result.push('\n');
                result.push_str(&r_gutter);
                result.push_str(&rel_line_text);
                result.push('\n');
                result.push_str(&r_empty);
                result.push(' ');
                result.push_str(&caret.yellow().to_string());
                result.push_str(&inline_label);
                result.push('\n');
                result.push_str(&r_empty);
            } else {
                result.push('\n');
            }
        }

        // Legacy single note
        if let Some(note) = &self.note {
            result.push_str(&format!(
                "\n{}{} {}",
                "     = ".cyan(),
                "note:".cyan().bold(),
                note
            ));
        }

        for note in &self.notes {
            result.push_str(&format!(
                "\n{}{} {}",
                "     = ".cyan(),
                "note:".cyan().bold(),
                note
            ));
        }

        if let Some(hint) = &self.hint {
            result.push_str(&format!(
                "\n{}{} {}",
                "     = ".cyan(),
                "help:".cyan().bold(),
                hint.green()
            ));
        }

        result
    }
}

impl fmt::Display for LangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_error())
    }
}

impl std::error::Error for LangError {}

// Allow easy conversion to String so existing code using `map_err(|e| format!("... {}", e))` can accept LangError
impl From<LangError> for String {
    fn from(e: LangError) -> String {
        e.to_string()
    }
}

// Allow converting a String message into a basic LangError (runtime errors without source position)
impl From<String> for LangError {
    fn from(msg: String) -> Self {
        LangError::new(ErrorKind::Runtime, msg, 0, 0, String::new())
    }
}

/// Language-appropriate ownership suggestions (AdeshLang uses ARC, not clone).
pub mod ownership_help {
    /// Suggest how to keep using a value that was moved
    pub fn use_after_move(var: &str) -> String {
        format!(
            "`{v}` was moved, so it can no longer be used.\n\
             Fix options:\n\
             (1) Use `{v}` before moving it\n\
             (2) Share ownership with ARC: `share {v} = <value>;` then `strong alias = {v};`\n\
             (3) Use a `weak` reference for non-owning access: `weak w = {v};`\n\
             (4) Restructure so only one owner is needed",
            v = var
        )
    }

    /// Suggest how to avoid moving when still needed later
    pub fn avoid_move(var: &str) -> String {
        format!(
            "Do not move `{v}` if you still need it later.\n\
             Options:\n\
             (1) Use `{v}` in place without assigning it elsewhere\n\
             (2) Share ownership with ARC: `share {v} = <value>;` then `strong alias = {v};`\n\
             (3) Use `weak` for a non-owning reference: `weak w = {v};`",
            v = var
        )
    }

    /// Suggest for control-flow partial moves
    pub fn branch_move(var: &str) -> String {
        format!(
            "`{v}` may be moved on some control-flow paths.\n\
             Options:\n\
             (1) Ensure every branch leaves `{v}` in a usable state\n\
             (2) Re-assign `{v}` after the branch if it may be consumed\n\
             (3) Share ownership with `share`/`strong` so moves don\\'t invalidate `{v}`",
            v = var
        )
    }

    /// Suggest for borrow conflicts
    pub fn borrow_conflict(var: &str, want_mutable: bool) -> String {
        if want_mutable {
            format!(
                "cannot take an exclusive (mutable) borrow of `{v}` while it is already borrowed. \
                 End or narrow the existing borrow first (smaller scope), then mutate `{v}`.",
                v = var
            )
        } else {
            format!(
                "cannot borrow `{v}` while it is exclusively borrowed. \
                 Finish the exclusive use first, or restructure so shared and exclusive uses do not overlap.",
                v = var
            )
        }
    }

    /// Suggest for free-while-borrowed
    pub fn free_while_borrowed(var: &str) -> String {
        format!(
            "cannot free `{v}` while borrows of it are still active. \
             Drop or end all borrows before freeing, or free after the borrow scopes end.",
            v = var
        )
    }

    /// Suggest for shared ownership / cycles
    pub fn use_weak_for_cycle() -> String {
        "to break a reference cycle, keep one side as `weak` instead of `strong`/`share` only. \
         Upgrade with `.upgrade()` when needed, and check for null."
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clickable_location_format() {
        assert_eq!(
            format_clickable_location(Some("src/main.adesh"), 10, 4),
            "src/main.adesh:10:4"
        );
        assert_eq!(format_clickable_location(None, 3, 2), "line 3 col 2");
    }

    #[test]
    fn display_includes_arrow_and_file_line_col() {
        let e = LangError::new(
            ErrorKind::Parse,
            "Expect expression".into(),
            2,
            11,
            "  let x = ;".into(),
        )
        .with_file("examples/test_file.adesh".into())
        .with_auto_hints();
        let s = e.to_string();
        // Strip ANSI for assertions
        let re = regex::Regex::new("\\x1B\\[[0-9;]*m").unwrap();
        let clean = re.replace_all(&s, "");
        assert!(clean.contains("examples/test_file.adesh:2:11"));
        assert!(clean.contains("-->"));
        assert!(clean.contains("Expect expression"));
        assert!(clean.contains("let x = ;"));
        assert!(clean.contains("^"));
        assert!(clean.contains("help:"));
    }

    #[test]
    fn ownership_help_never_mentions_clone() {
        let s = ownership_help::use_after_move("x");
        assert!(s.contains("share"));
        // Should not suggest using .clone() as a fix
        assert!(!s.contains("use `.clone()`)"));
        assert!(!s.contains("try `.clone()`"));
        assert!(!s.contains("with `.clone()`"));

        let s2 = ownership_help::avoid_move("y");
        assert!(s2.contains("share"));

        let s3 = ownership_help::branch_move("z");
        assert!(s3.contains("share"));

        let s4 = ownership_help::borrow_conflict("a", true);
        assert!(s4.contains("exclusive"));

        let s5 = ownership_help::free_while_borrowed("b");
        assert!(s5.contains("free"));

        let s6 = ownership_help::use_weak_for_cycle();
        assert!(s6.contains("weak"));
    }
}
