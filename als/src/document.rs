//! Document representation for the language server
//!
//! Manages open documents, their content, and parsed state.

use lsp_types::Url;
use std::collections::HashMap;

/// Represents an open document in the editor
#[derive(Debug, Clone)]
pub struct Document {
    /// Document URI
    pub uri: Url,
    /// Document content
    pub content: String,
    /// Document version (incremented on each change)
    pub version: i32,
    /// Cached line offsets for fast position lookups
    line_offsets: Vec<usize>,
}

impl Document {
    /// Create a new document
    pub fn new(uri: Url, content: String, version: i32) -> Self {
        let line_offsets = Self::compute_line_offsets(&content);
        Self {
            uri,
            content,
            version,
            line_offsets,
        }
    }

    /// Update document content
    pub fn update(&mut self, content: String, version: i32) {
        self.content = content;
        self.version = version;
        self.line_offsets = Self::compute_line_offsets(&self.content);
    }

    /// Get the text at a specific line (0-indexed)
    pub fn get_line(&self, line: usize) -> Option<&str> {
        if line >= self.line_offsets.len() {
            return None;
        }

        let start = self.line_offsets[line];
        let end = if line + 1 < self.line_offsets.len() {
            self.line_offsets[line + 1] - 1 // Exclude newline
        } else {
            self.content.len()
        };

        Some(&self.content[start..end.min(self.content.len())])
    }

    /// Convert a UTF-16 code-unit column within a line to a valid UTF-8 byte offset.
    pub fn utf16_col_to_byte_offset(line_text: &str, col: u32) -> usize {
        let mut byte_offset = 0;
        let mut utf16_offset = 0;

        for ch in line_text.chars() {
            if utf16_offset >= col as usize {
                break;
            }
            byte_offset += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        byte_offset
    }

    /// Convert line/column to byte offset
    pub fn position_to_offset(&self, line: u32, col: u32) -> Option<usize> {
        let line = line as usize;
        if line >= self.line_offsets.len() {
            return None;
        }

        let line_start = self.line_offsets[line];
        let line_text = self.get_line(line)?;
        let byte_offset = Self::utf16_col_to_byte_offset(line_text, col);

        Some(line_start + byte_offset)
    }

    /// Convert byte offset to line/column
    pub fn offset_to_position(&self, offset: usize) -> (u32, u32) {
        // Find line
        let line = self
            .line_offsets
            .iter()
            .rposition(|&o| o <= offset)
            .unwrap_or(0);

        let line_start = self.line_offsets[line];
        let col_bytes = offset - line_start;

        // Convert byte column to UTF-16 code units
        let line_text = self.get_line(line).unwrap_or("");
        let mut utf16_col = 0;
        let mut byte_count = 0;

        for ch in line_text.chars() {
            if byte_count >= col_bytes {
                break;
            }
            byte_count += ch.len_utf8();
            utf16_col += ch.len_utf16();
        }

        (line as u32, utf16_col as u32)
    }

    /// Get word at position (line is 0-indexed, col is UTF-16 code units per LSP)
    pub fn get_word_at(&self, line: u32, col: u32) -> Option<String> {
        let line_text = self.get_line(line as usize)?;
        let byte_col = Self::utf16_col_to_byte_offset(line_text, col);

        // Find word boundaries on valid UTF-8 char boundaries
        let start = line_text[..byte_col]
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_alphanumeric() && c != '_')
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);

        let end = line_text[byte_col..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| byte_col + i)
            .unwrap_or(line_text.len());

        if start < end {
            Some(line_text[start..end].to_string())
        } else {
            None
        }
    }

    /// Compute line offsets for the content
    fn compute_line_offsets(content: &str) -> Vec<usize> {
        let mut offsets = vec![0];
        for (i, c) in content.char_indices() {
            if c == '\n' {
                offsets.push(i + 1);
            }
        }
        offsets
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }
}

/// Document store - manages all open documents
pub struct DocumentStore {
    documents: HashMap<Url, Document>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Open a new document
    pub fn open(&mut self, uri: Url, content: String, version: i32) {
        self.documents
            .insert(uri.clone(), Document::new(uri, content, version));
    }

    /// Update an existing document
    pub fn update(&mut self, uri: &Url, content: String, version: i32) {
        if let Some(doc) = self.documents.get_mut(uri) {
            doc.update(content, version);
        }
    }

    /// Close a document
    pub fn close(&mut self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Get a document by URI
    pub fn get(&self, uri: &Url) -> Option<&Document> {
        self.documents.get(uri)
    }

    /// Get all documents
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_lines() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let content = "let x = 1;\nlet y = 2;\nlet z = 3;".to_string();
        let doc = Document::new(uri, content, 1);

        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.get_line(0), Some("let x = 1;"));
        assert_eq!(doc.get_line(1), Some("let y = 2;"));
        assert_eq!(doc.get_line(2), Some("let z = 3;"));
    }

    #[test]
    fn test_position_conversion() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let content = "hello\nworld".to_string();
        let doc = Document::new(uri, content, 1);

        assert_eq!(doc.position_to_offset(0, 0), Some(0));
        assert_eq!(doc.position_to_offset(0, 5), Some(5));
        assert_eq!(doc.position_to_offset(1, 0), Some(6));
        assert_eq!(doc.position_to_offset(1, 5), Some(11));
    }

    #[test]
    fn test_word_at_position() {
        let uri = Url::parse("file:///test.adesh").unwrap();
        let content = "let myVar = 42;".to_string();
        let doc = Document::new(uri, content, 1);

        assert_eq!(doc.get_word_at(0, 0), Some("let".to_string()));
        assert_eq!(doc.get_word_at(0, 4), Some("myVar".to_string()));
        assert_eq!(doc.get_word_at(0, 6), Some("myVar".to_string()));
    }

    #[test]
    fn test_word_at_position_non_ascii_and_multibyte_delimiter() {
        let uri = Url::parse("file:///unicode.adesh").unwrap();
        // Emoji is 2 UTF-16 code units (4 UTF-8 bytes); em-dash is 1 UTF-16 unit (3 UTF-8 bytes).
        let content = "let x = \"😀\" — fooBar;".to_string();
        let doc = Document::new(uri, content, 1);

        // Should never panic for any UTF-16 column across the line.
        for col in 0..30 {
            let _ = doc.get_word_at(0, col);
        }
        // "let x = \"" (9) + "😀" (2) + "\" — " (4) = UTF-16 col 15 -> "fooBar"
        assert_eq!(doc.get_word_at(0, 15), Some("fooBar".to_string()));
        assert_eq!(doc.get_word_at(0, 18), Some("fooBar".to_string()));
    }
}
