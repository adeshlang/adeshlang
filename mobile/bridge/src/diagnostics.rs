use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "info")]
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn new_error(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            line: line.max(1),
            column: column.max(1),
            length: 1,
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_length(mut self, length: usize) -> Self {
        self.length = length.max(1);
        self
    }
}

/// Parse diagnostic line/column information from compiler/runtime error strings.
pub fn parse_diagnostic_from_error(error_msg: &str) -> Diagnostic {
    let mut line = 1;
    let mut column = 1;
    let mut code: Option<String> = None;

    let lower = error_msg.to_lowercase();

    if let Some(pos) = lower.find("line ") {
        let rest = &lower[pos + 5..];
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(l) = num_str.parse::<usize>() {
            line = l;
            if let Some(col_pos) = rest.find("column ") {
                let col_rest = &rest[col_pos + 7..];
                let col_num: String = col_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(c) = col_num.parse::<usize>() {
                    column = c;
                }
            } else if let Some(col_pos) = rest.find(':') {
                let col_rest = &rest[col_pos + 1..];
                let col_num: String = col_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(c) = col_num.parse::<usize>() {
                    column = c;
                }
            }
        }
    } else if let Some(pos) = lower.find("at line ") {
        let rest = &lower[pos + 8..];
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(l) = num_str.parse::<usize>() {
            line = l;
        }
    } else if lower.contains(':') {
        let parts: Vec<&str> = lower.split(':').collect();
        if parts.len() >= 3 {
            if let Ok(l) = parts[1].trim().parse::<usize>() {
                if let Ok(c) = parts[2].trim().parse::<usize>() {
                    line = l;
                    column = c;
                }
            }
        }
    }

    if lower.contains("type error") || lower.contains("typecheck") {
        code = Some("TypeError".to_string());
    } else if lower.contains("ownership") || lower.contains("borrow") || lower.contains("move") {
        code = Some("OwnershipError".to_string());
    } else if lower.contains("parse") || lower.contains("syntax") || lower.contains("lexer") {
        code = Some("SyntaxError".to_string());
    } else if lower.contains("runtime") || lower.contains("panic") {
        code = Some("RuntimeError".to_string());
    }

    let mut diag = Diagnostic::new_error(error_msg.trim(), line, column);
    if let Some(c) = code {
        diag = diag.with_code(c);
    }
    diag
}
