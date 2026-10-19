//! Terminal Diagnostics Renderer
//!
//! Produces high-quality Rust-inspired error tables with correct line-number gutters,
//! accurate multi-column/multi-line caret underlines, secondary labels, notes,
//! actionable help messages, and strict NO_COLOR / color override handling.

use super::model::Diagnostic;
use super::registry::DiagnosticSeverity;
use colored::Colorize;
use std::env;

pub struct TerminalRenderer {
    pub color_enabled: bool,
    pub tab_width: usize,
}

impl Default for TerminalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalRenderer {
    pub fn new() -> Self {
        let no_color =
            env::var("NO_COLOR").is_ok() || env::var("CLICOLOR").map(|v| v == "0").unwrap_or(false);
        Self {
            color_enabled: !no_color,
            tab_width: 4,
        }
    }

    pub fn plain() -> Self {
        Self {
            color_enabled: false,
            tab_width: 4,
        }
    }

    pub fn with_color(mut self, enabled: bool) -> Self {
        self.color_enabled = enabled;
        self
    }

    /// Render a single diagnostic into a terminal-formatted string.
    pub fn render(&self, diag: &Diagnostic) -> String {
        let mut out = String::new();

        // 1. Header (e.g. error[E0308]: mismatched types)
        let sev_label = diag.severity.as_str();
        let header_text = if let Some(ref code) = diag.code {
            format!("{}[{}]: {}", sev_label, code, diag.message)
        } else {
            format!("{}: {}", sev_label, diag.message)
        };

        if self.color_enabled {
            match diag.severity {
                DiagnosticSeverity::Error => out.push_str(&header_text.red().bold().to_string()),
                DiagnosticSeverity::Warning => {
                    out.push_str(&header_text.yellow().bold().to_string())
                }
                DiagnosticSeverity::Info => out.push_str(&header_text.cyan().bold().to_string()),
            }
        } else {
            out.push_str(&header_text);
        }

        // 2. Primary location arrow: `  --> path:line:col`
        let (file_name, line, col) = if let Some(ref span) = diag.primary_span {
            (
                diag.file.as_deref().unwrap_or("unknown"),
                span.start_line,
                span.start_col,
            )
        } else if let Some(ref f) = diag.file {
            (f.as_str(), 1, 1)
        } else {
            ("", 0, 0)
        };

        if line > 0 || !file_name.is_empty() {
            let loc_str = if col > 0 {
                format!("{}:{}:{}", file_name, line.max(1), col)
            } else {
                format!("{}:{}", file_name, line.max(1))
            };
            out.push('\n');
            let arrow_prefix = "  --> ";
            if self.color_enabled {
                out.push_str(&arrow_prefix.blue().bold().to_string());
                out.push_str(&loc_str);
            } else {
                out.push_str(arrow_prefix);
                out.push_str(&loc_str);
            }
        }

        // 3. Source table with gutter & caret
        if let Some(ref span) = diag.primary_span {
            if span.start_line > 0 || !span.line_text.is_empty() {
                let line_no = span.start_line.max(1);
                let gutter_width = line_no.to_string().len().max(2);
                let empty_gutter = format!("{:>width$} |", "", width = gutter_width);
                let line_gutter = format!("{:>width$} | ", line_no, width = gutter_width);

                out.push('\n');
                if self.color_enabled {
                    out.push_str(&empty_gutter.blue().bold().to_string());
                    out.push('\n');
                    out.push_str(&line_gutter.blue().bold().to_string());
                    out.push_str(&span.line_text);
                    out.push('\n');
                    out.push_str(&empty_gutter.blue().bold().to_string());
                } else {
                    out.push_str(&empty_gutter);
                    out.push('\n');
                    out.push_str(&line_gutter);
                    out.push_str(&span.line_text);
                    out.push('\n');
                    out.push_str(&empty_gutter);
                }

                // Caret underline
                let col0 = if span.start_col > 0 {
                    span.start_col - 1
                } else {
                    0
                };
                let len = span.len();
                let mut underline = String::new();
                for _ in 0..col0 {
                    underline.push(' ');
                }
                underline.push('^');
                for _ in 1..len {
                    underline.push('^');
                }

                out.push(' ');
                if self.color_enabled {
                    match diag.severity {
                        DiagnosticSeverity::Error => {
                            out.push_str(&underline.red().bold().to_string())
                        }
                        DiagnosticSeverity::Warning => {
                            out.push_str(&underline.yellow().bold().to_string())
                        }
                        DiagnosticSeverity::Info => {
                            out.push_str(&underline.cyan().bold().to_string())
                        }
                    }
                } else {
                    out.push_str(&underline);
                }

                // Optional primary inline message if defined
                if let Some(lbl) = diag.labels.iter().find(|l| l.is_primary) {
                    out.push(' ');
                    if self.color_enabled {
                        out.push_str(&lbl.message.red().bold().to_string());
                    } else {
                        out.push_str(&lbl.message);
                    }
                }

                out.push('\n');
                if self.color_enabled {
                    out.push_str(&empty_gutter.blue().bold().to_string());
                } else {
                    out.push_str(&empty_gutter);
                }
            }
        }

        // 4. Secondary labels / related locations
        for lbl in diag.labels.iter().filter(|l| !l.is_primary) {
            let lbl_line = lbl.span.start_line.max(1);
            let lbl_col = lbl.span.start_col.max(1);
            let lbl_file = lbl.file.as_deref().unwrap_or(file_name);
            let loc_str = format!("{}:{}:{}", lbl_file, lbl_line, lbl_col);

            out.push('\n');
            let note_hdr = format!("note: {}", lbl.message);
            if self.color_enabled {
                out.push_str(&note_hdr.yellow().to_string());
                out.push_str(&format!("\n  --> {}\n", loc_str).blue().bold().to_string());
            } else {
                out.push_str(&note_hdr);
                out.push_str(&format!("\n  --> {}\n", loc_str));
            }

            if !lbl.span.line_text.is_empty() {
                let gutter_width = lbl_line.to_string().len().max(2);
                let empty_gutter = format!("{:>width$} |", "", width = gutter_width);
                let line_gutter = format!("{:>width$} | ", lbl_line, width = gutter_width);
                let col0 = if lbl_col > 0 { lbl_col - 1 } else { 0 };
                let mut underline = String::new();
                for _ in 0..col0 {
                    underline.push(' ');
                }
                underline.push('^');

                if self.color_enabled {
                    out.push_str(&empty_gutter.blue().bold().to_string());
                    out.push('\n');
                    out.push_str(&line_gutter.blue().bold().to_string());
                    out.push_str(&lbl.span.line_text);
                    out.push('\n');
                    out.push_str(&empty_gutter.blue().bold().to_string());
                    out.push(' ');
                    out.push_str(&underline.yellow().to_string());
                } else {
                    out.push_str(&empty_gutter);
                    out.push('\n');
                    out.push_str(&line_gutter);
                    out.push_str(&lbl.span.line_text);
                    out.push('\n');
                    out.push_str(&empty_gutter);
                    out.push(' ');
                    out.push_str(&underline);
                }
            }
        }

        // 5. Notes
        for note in &diag.notes {
            out.push('\n');
            if self.color_enabled {
                out.push_str(&format!("   = {}: {}", "note".cyan().bold(), note));
            } else {
                out.push_str(&format!("   = note: {}", note));
            }
        }

        // 6. Helps
        for help in &diag.helps {
            out.push('\n');
            if self.color_enabled {
                out.push_str(&format!("   = {}: {}", "help".cyan().bold(), help.green()));
            } else {
                out.push_str(&format!("   = help: {}", help));
            }
        }

        // 7. Automatic explanation hint when error code is present
        if let Some(ref code) = diag.code {
            let explain_msg = format!("run `adesh --explain {}` for more information", code);
            if !diag.helps.iter().any(|h| h.contains(&explain_msg)) {
                out.push('\n');
                if self.color_enabled {
                    out.push_str(&format!(
                        "   = {}: {}",
                        "help".cyan().bold(),
                        explain_msg.green()
                    ));
                } else {
                    out.push_str(&format!("   = help: {}", explain_msg));
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::model::Span;
    use crate::diagnostics::registry::DiagnosticPhase;

    #[test]
    fn test_renderer_plain_text() {
        let renderer = TerminalRenderer::new().with_color(false);
        let diag = Diagnostic::error(DiagnosticPhase::TypeCheck, "mismatched types")
            .with_code("E0308")
            .with_file("examples/types.adesh")
            .with_primary_span(Span::new(4, 18, 4, 25, "let count: i32 = \"seven\";"))
            .with_note("this declaration requires an integer value")
            .with_help("provide an integer or change the declared type")
            .with_explain_hint();

        let rendered = renderer.render(&diag);
        assert!(rendered.contains("error[E0308]: mismatched types"));
        assert!(rendered.contains("--> examples/types.adesh:4:18"));
        assert!(rendered.contains("4 | let count: i32 = \"seven\";"));
        assert!(rendered.contains("^^^^^^^^"));
        assert!(rendered.contains("= note: this declaration requires an integer value"));
        assert!(rendered.contains("= help: provide an integer or change the declared type"));
        assert!(rendered.contains("= help: run `adesh --explain E0308` for more information"));
    }
}
