//! Code formatting provider
//!
//! Integrates with Adesh's formatter for document formatting, honoring both
//! ALS configuration (`adesh.format.*`) and per-request LSP `FormattingOptions`.

use crate::config::AlsConfig;
use crate::document::Document;
use adeshlang::toolchain::formatter::FormatConfig;
use lsp_types::{FormattingOptions, Position, Range, TextEdit};

/// Format an entire document using ALS configuration and optional LSP formatting options.
pub fn format_document_with_config(
    doc: &Document,
    config: &AlsConfig,
    options: Option<&FormattingOptions>,
) -> Vec<TextEdit> {
    if !config.format.enable {
        return vec![];
    }

    let mut fmt_cfg = FormatConfig::default();
    fmt_cfg.indent_size = options
        .map(|o| o.tab_size as usize)
        .filter(|&n| n > 0)
        .unwrap_or(config.format.indent_size);
    fmt_cfg.use_tabs = options
        .map(|o| !o.insert_spaces)
        .unwrap_or(config.format.use_tabs);

    match adeshlang::utils::formatter::format_source(&doc.content, Some(fmt_cfg)) {
        Ok(formatted) => {
            if formatted != doc.content {
                let last_line = doc.line_count().saturating_sub(1);
                let last_col = doc
                    .get_line(last_line)
                    .map(|l| l.encode_utf16().count() as u32)
                    .unwrap_or(0);
                vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: last_line as u32,
                            character: last_col,
                        },
                    },
                    new_text: formatted,
                }]
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    }
}

/// Format an entire document with default settings.
pub fn format_document(doc: &Document) -> Vec<TextEdit> {
    format_document_with_config(doc, &AlsConfig::default(), None)
}

/// Format a range of a document
#[allow(dead_code)]
pub fn format_range(doc: &Document, _range: Range) -> Vec<TextEdit> {
    format_document(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Url;

    #[test]
    fn test_format_document() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, "let x=1;".to_string(), 1);

        let edits = format_document(&doc);
        assert!(!edits.is_empty());
        assert!(edits[0].new_text.contains("let x = 1;"));
    }

    #[test]
    fn test_format_already_formatted() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, "let x = 1;\n".to_string(), 1);

        let edits = format_document(&doc);
        assert!(edits.is_empty() || edits[0].new_text == doc.content);
    }

    #[test]
    fn test_format_syntax_error() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, "let x = ".to_string(), 1);

        let edits = format_document(&doc);
        assert!(edits.is_empty());
    }

    #[test]
    fn test_format_respects_indent_config() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, "fn f() {\nlet x = 1;\n}\n".to_string(), 1);
        let mut cfg = AlsConfig::default();
        cfg.format.indent_size = 2;

        let edits = format_document_with_config(&doc, &cfg, None);
        assert!(!edits.is_empty());
        assert!(
            edits[0].new_text.contains("  let x = 1;"),
            "got: {:?}",
            edits[0].new_text
        );
    }
}
