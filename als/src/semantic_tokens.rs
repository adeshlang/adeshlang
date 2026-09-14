//! Semantic tokens provider for ownership and borrowing visualization
//!
//! Provides semantic highlighting for AdeshLang's ownership and borrow states.

use crate::analysis::{SymbolInfo, SymbolKind};
use crate::document::Document;
use lsp_types::*;

/// Semantic token types for AdeshLang
pub const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::ENUM,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    // Custom types for AdeshLang
    SemanticTokenType::new("borrowedVariable"),
    SemanticTokenType::new("ownedVariable"),
    SemanticTokenType::new("movedVariable"),
    SemanticTokenType::new("droppedVariable"),
    SemanticTokenType::new("unsafeFunction"),
    SemanticTokenType::new("region"),
];

/// Semantic token modifiers for AdeshLang
pub const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    // Custom modifiers for AdeshLang
    SemanticTokenModifier::new("borrowed"),
    SemanticTokenModifier::new("owned"),
    SemanticTokenModifier::new("moved"),
    SemanticTokenModifier::new("mutable"),
    SemanticTokenModifier::new("unsafe"),
    SemanticTokenModifier::new("noalias"),
];

/// Get semantic token type index
fn get_token_type_index(kind: &SymbolKind) -> u32 {
    match kind {
        SymbolKind::Function => 11, // FUNCTION
        SymbolKind::Class => 2,     // CLASS
        SymbolKind::Variable => 8,  // VARIABLE
        SymbolKind::Constant => 8,  // VARIABLE with READONLY modifier
        SymbolKind::Parameter => 7, // PARAMETER
        SymbolKind::Method => 12,   // METHOD
        SymbolKind::Property => 9,  // PROPERTY
        SymbolKind::Interface => 4, // INTERFACE
        SymbolKind::Enum => 3,      // ENUM
        SymbolKind::Module => 0,    // NAMESPACE (Module is similar)
    }
}

/// Generate semantic tokens for a document
pub fn get_semantic_tokens(doc: &Document, symbols: &[SymbolInfo]) -> SemanticTokens {
    let mut tokens_builder = SemanticTokensBuilder::new();

    // Sort symbols by position
    let mut sorted_symbols = symbols.to_vec();
    sorted_symbols.sort_by_key(|s| (s.line, s.col));

    for symbol in sorted_symbols.iter() {
        let token_type = get_token_type_index(&symbol.kind);
        let mut modifiers = 0u32;

        // Add modifiers based on symbol properties
        if symbol.kind == SymbolKind::Constant {
            modifiers |= 1 << 2; // READONLY
        }

        tokens_builder.push(
            symbol.line as u32,
            symbol.col as u32,
            symbol.name.len() as u32,
            token_type,
            modifiers,
        );
    }

    add_keyword_tokens(doc, &mut tokens_builder);

    tokens_builder.build()
}

/// Semantic tokens builder helper
struct SemanticTokensBuilder {
    data: Vec<SemanticToken>,
    prev_line: u32,
    prev_char: u32,
}

impl SemanticTokensBuilder {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            prev_line: 0,
            prev_char: 0,
        }
    }

    fn push(&mut self, line: u32, char: u32, length: u32, token_type: u32, modifiers: u32) {
        let delta_line = line - self.prev_line;
        let delta_start = if delta_line == 0 {
            char - self.prev_char
        } else {
            char
        };

        self.data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        self.prev_line = line;
        self.prev_char = char;
    }

    fn build(self) -> SemanticTokens {
        SemanticTokens {
            result_id: None,
            data: self.data,
        }
    }
}

/// Get semantic tokens for a range
pub fn get_semantic_tokens_range(
    doc: &Document,
    symbols: &[SymbolInfo],
    range: Range,
) -> SemanticTokens {
    let mut tokens_builder = SemanticTokensBuilder::new();

    // Filter symbols within range
    let filtered_symbols: Vec<_> = symbols
        .iter()
        .filter(|s| {
            let pos = Position::new(s.line as u32, s.col as u32);
            pos >= range.start && pos <= range.end
        })
        .collect();

    for symbol in filtered_symbols.iter() {
        let token_type = get_token_type_index(&symbol.kind);
        let modifiers = 0u32;

        tokens_builder.push(
            symbol.line as u32,
            symbol.col as u32,
            symbol.name.len() as u32,
            token_type,
            modifiers,
        );
    }

    add_keyword_tokens(doc, &mut tokens_builder);

    tokens_builder.build()
}

fn add_keyword_tokens(doc: &Document, builder: &mut SemanticTokensBuilder) {
    const KEYWORD_TEST: &str = "test";
    const ARC_KEYWORDS: [&str; 3] = ["share", "strong", "weak"];
    const ASSERT_BUILTINS: [&str; 3] = ["assert", "assert_eq", "assert_ne"];

    for line_idx in 0..doc.line_count() {
        let Some(line_text) = doc.get_line(line_idx) else {
            continue;
        };
        scan_line_for_word(builder, line_idx as u32, line_text, KEYWORD_TEST, 14); // KEYWORD
        for arc_kw in ARC_KEYWORDS {
            scan_line_for_word(builder, line_idx as u32, line_text, arc_kw, 14);
            // KEYWORD
        }
        for builtin in ASSERT_BUILTINS {
            scan_line_for_word(builder, line_idx as u32, line_text, builtin, 11);
            // FUNCTION
        }
    }
}

fn scan_line_for_word(
    builder: &mut SemanticTokensBuilder,
    line: u32,
    line_text: &str,
    word: &str,
    token_type: u32,
) {
    let mut start = 0;
    while let Some(pos) = line_text[start..].find(word) {
        let idx = start + pos;
        let end = idx + word.len();
        if is_word_boundary(line_text, idx, end) {
            let col = byte_index_to_utf16(line_text, idx) as u32;
            builder.push(line, col, word.len() as u32, token_type, 0);
        }
        start = end;
    }
}

fn is_word_boundary(s: &str, start: usize, end: usize) -> bool {
    let before = s[..start].chars().next_back();
    let after = s[end..].chars().next();
    let before_ok = before.is_none_or(|c| !is_ident_char(c));
    let after_ok = after.is_none_or(|c| !is_ident_char(c));
    before_ok && after_ok
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn byte_index_to_utf16(s: &str, byte_index: usize) -> usize {
    let mut utf16 = 0;
    for (idx, ch) in s.char_indices() {
        if idx >= byte_index {
            break;
        }
        utf16 += ch.len_utf16();
    }
    utf16
}
