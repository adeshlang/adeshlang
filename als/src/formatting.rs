//! Code formatting provider
//!
//! Integrates with Adesh's formatter for document formatting.

use crate::document::Document;
use lsp_types::{Position, Range, TextEdit};

/// Format an entire document
pub fn format_document(doc: &Document) -> Vec<TextEdit> {
    // Use Adesh's formatter
    match adeshlang::utils::formatter::format_source(&doc.content, None) {
        Ok(formatted) => {
            // If formatting succeeded and changed the content, return the edit
            if formatted != doc.content {
                vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: doc.line_count() as u32,
                            character: 0,
                        },
                    },
                    new_text: formatted,
                }]
            } else {
                vec![]
            }
        }
        Err(_) => {
            // If formatting failed (e.g., syntax error), return empty edits
            vec![]
        }
    }
}

/// Format a range of a document
#[allow(dead_code)]
pub fn format_range(doc: &Document, _range: Range) -> Vec<TextEdit> {
    // For simplicity, format the entire document for now
    // A more sophisticated implementation would extract and format just the range
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
        // Should return empty if already formatted
        assert!(edits.is_empty() || edits[0].new_text == doc.content);
    }

    #[test]
    fn test_format_syntax_error() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, "let x = ".to_string(), 1);

        let edits = format_document(&doc);
        // Should return empty on syntax error
        assert!(edits.is_empty());
    }
}
