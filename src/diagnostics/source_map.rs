//! Source Map and Efficient Line/Column Indexing
//!
//! Provides fast, reusable line-offset tables for source files, converting byte offsets
//! to (line, col) and visual columns without repeated full-text rescans. Handles UTF-8,
//! Unicode display width, tabs, CRLF, and empty files gracefully.

use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub struct SourceMap {
    pub file_name: Option<String>,
    pub source: String,
    line_starts: Vec<usize>,
}

impl SourceMap {
    pub fn new(source: String, file_name: Option<String>) -> Self {
        let mut line_starts = vec![0];
        let bytes = source.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            file_name,
            source,
            line_starts,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Convert 0-based byte offset to 1-based (line, col).
    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        if self.source.is_empty() {
            return (1, 1);
        }
        let clamped = offset.min(self.source.len());
        // Binary search line_starts
        let line_idx = match self.line_starts.binary_search(&clamped) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx];
        let col_byte_offset = clamped.saturating_sub(line_start);

        // UTF-8 character column count
        let line_sub = &self.source[line_start..clamped];
        let col = line_sub.chars().count() + 1;
        (line_idx + 1, col)
    }

    /// Get line content without trailing newline (CRLF or LF).
    pub fn get_line(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let start = self.line_starts[line - 1];
        let end = if line < self.line_starts.len() {
            self.line_starts[line]
        } else {
            self.source.len()
        };
        let slice = &self.source[start..end];
        Some(slice.trim_end_matches(['\r', '\n']))
    }

    /// Calculate visual column width taking tabs and unicode width into account.
    pub fn visual_column(line_text: &str, char_col: usize, tab_width: usize) -> usize {
        if char_col <= 1 {
            return 1;
        }
        let mut visual = 1;
        for (i, ch) in line_text.chars().enumerate() {
            if i + 1 >= char_col {
                break;
            }
            if ch == '\t' {
                let rem = (visual - 1) % tab_width;
                visual += tab_width - rem;
            } else {
                visual += ch.width().unwrap_or(1);
            }
        }
        visual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sourcemap_ascii() {
        let src = "let x = 1;\nlet y = 2;\nlet z = 3;";
        let sm = SourceMap::new(src.to_string(), Some("test.adesh".to_string()));
        assert_eq!(sm.line_count(), 3);
        assert_eq!(sm.get_line(1), Some("let x = 1;"));
        assert_eq!(sm.get_line(2), Some("let y = 2;"));
        assert_eq!(sm.get_line(3), Some("let z = 3;"));

        let (l, c) = sm.offset_to_line_col(11); // start of line 2
        assert_eq!(l, 2);
        assert_eq!(c, 1);
    }

    #[test]
    fn test_sourcemap_unicode_and_crlf() {
        let src = "let 🦀 = \"crab\";\r\nlet y = 42;";
        let sm = SourceMap::new(src.to_string(), None);
        assert_eq!(sm.get_line(1), Some("let 🦀 = \"crab\";"));
        assert_eq!(sm.get_line(2), Some("let y = 42;"));

        // Position after crab
        let (l, c) = sm.offset_to_line_col("let 🦀".len());
        assert_eq!(l, 1);
        assert_eq!(c, 6);
    }
}
