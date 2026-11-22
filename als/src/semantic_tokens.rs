//! Semantic Tokens Provider
//!
//! Powered by the lexer tokens and `SemanticIndex` symbol table:
//! - Accurate 0-based UTF-16 token positions
//! - Strictly sorted & deduplicated before delta encoding (guaranteed panic-free!)
//! - Full classification of types, classes, structs, enums, interfaces, enum variants,
//!   functions, methods, fields, parameters, constants, ARC (`share`/`strong`/`weak`),
//!   `unsafe`, `region`, decorators, keywords, literals, and operators.

use crate::analysis::SymbolInfo;
use crate::document::Document;
use crate::type_utils::find_builtin_fn;
use adeshlang::parsing::ast::TokenKind;
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind};
use lsp_types::*;
use std::collections::HashMap;

/// Semantic token types legend for AdeshLang (order matches indices below & VSCode package.json).
pub const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,      // 0
    SemanticTokenType::TYPE,           // 1
    SemanticTokenType::CLASS,          // 2
    SemanticTokenType::ENUM,           // 3
    SemanticTokenType::INTERFACE,      // 4
    SemanticTokenType::STRUCT,         // 5
    SemanticTokenType::TYPE_PARAMETER, // 6
    SemanticTokenType::PARAMETER,      // 7
    SemanticTokenType::VARIABLE,       // 8
    SemanticTokenType::PROPERTY,       // 9
    SemanticTokenType::ENUM_MEMBER,    // 10
    SemanticTokenType::FUNCTION,       // 11
    SemanticTokenType::METHOD,         // 12
    SemanticTokenType::KEYWORD,        // 13
    SemanticTokenType::MODIFIER,       // 14
    SemanticTokenType::COMMENT,        // 15
    SemanticTokenType::STRING,         // 16
    SemanticTokenType::NUMBER,         // 17
    SemanticTokenType::OPERATOR,       // 18
    // Custom types for AdeshLang
    SemanticTokenType::new("borrowedVariable"), // 19
    SemanticTokenType::new("ownedVariable"),    // 20
    SemanticTokenType::new("movedVariable"),    // 21
    SemanticTokenType::new("droppedVariable"),  // 22
    SemanticTokenType::new("unsafeFunction"),   // 23
    SemanticTokenType::new("region"),           // 24
    SemanticTokenType::new("sharedVariable"),   // 25
    SemanticTokenType::new("strongReference"),  // 26
    SemanticTokenType::new("weakReference"),    // 27
    SemanticTokenType::MACRO,                   // 28 (ADL section headers)
];

/// Semantic token modifiers legend for AdeshLang.
pub const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,     // bit 0
    SemanticTokenModifier::DEFINITION,      // bit 1
    SemanticTokenModifier::READONLY,        // bit 2
    SemanticTokenModifier::STATIC,          // bit 3
    SemanticTokenModifier::DEPRECATED,      // bit 4
    SemanticTokenModifier::ABSTRACT,        // bit 5
    SemanticTokenModifier::ASYNC,           // bit 6
    SemanticTokenModifier::MODIFICATION,    // bit 7
    SemanticTokenModifier::DOCUMENTATION,   // bit 8
    SemanticTokenModifier::DEFAULT_LIBRARY, // bit 9
    // Custom modifiers for AdeshLang
    SemanticTokenModifier::new("borrowed"), // bit 10
    SemanticTokenModifier::new("owned"),    // bit 11
    SemanticTokenModifier::new("moved"),    // bit 12
    SemanticTokenModifier::new("mutable"),  // bit 13
    SemanticTokenModifier::new("unsafe"),   // bit 14
    SemanticTokenModifier::new("noalias"),  // bit 15
    SemanticTokenModifier::new("shared"),   // bit 16
    SemanticTokenModifier::new("weak"),     // bit 17
];

const TT_NAMESPACE: u32 = 0;
const TT_TYPE: u32 = 1;
const TT_CLASS: u32 = 2;
const TT_ENUM: u32 = 3;
const TT_INTERFACE: u32 = 4;
const TT_STRUCT: u32 = 5;
const TT_TYPE_PARAM: u32 = 6;
const TT_PARAMETER: u32 = 7;
const TT_VARIABLE: u32 = 8;
const TT_PROPERTY: u32 = 9;
const TT_ENUM_MEMBER: u32 = 10;
const TT_FUNCTION: u32 = 11;
const TT_METHOD: u32 = 12;
const TT_KEYWORD: u32 = 13;
const TT_COMMENT: u32 = 15;
const TT_STRING: u32 = 16;
const TT_NUMBER: u32 = 17;
const TT_REGION: u32 = 24;
const TT_SHARED_VAR: u32 = 25;
const TT_STRONG_REF: u32 = 26;
const TT_WEAK_REF: u32 = 27;

const MOD_DECLARATION: u32 = 1 << 0;
const MOD_DEFINITION: u32 = 1 << 1;
const MOD_READONLY: u32 = 1 << 2;
const MOD_STATIC: u32 = 1 << 3;
const MOD_ASYNC: u32 = 1 << 6;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 9;
const MOD_MUTABLE: u32 = 1 << 13;
const MOD_SHARED: u32 = 1 << 16;
const MOD_WEAK: u32 = 1 << 17;

#[derive(Debug, Clone, Copy)]
struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

/// Translate ADL's private token legend into the single advertised server legend.
pub(crate) fn get_adl_tokens(content: &str, range: Option<Range>) -> SemanticTokens {
    let mut tokens = crate::adl::get_adl_semantic_tokens(content);
    let mut line = 0;
    let mut col = 0;
    let mut previous_line = 0;
    let mut previous_col = 0;
    tokens.data = tokens
        .data
        .into_iter()
        .filter_map(|mut token| {
            line += token.delta_line;
            col = if token.delta_line == 0 {
                col + token.delta_start
            } else {
                token.delta_start
            };
            if let Some(range) = range {
                if Position::new(line, col) < range.start
                    || Position::new(line, col + token.length) > range.end
                {
                    return None;
                }
            }
            token.token_type = [9, 13, 16, 17, 3, 28, 15, 18, 0]
                .get(token.token_type as usize)
                .copied()
                .unwrap_or(8);
            let mods = token.token_modifiers_bitset;
            token.token_modifiers_bitset = (mods & 1) | ((mods & 2) << 1) | ((mods & 4) << 5);
            token.delta_line = line - previous_line;
            token.delta_start = if token.delta_line == 0 {
                col - previous_col
            } else {
                col
            };
            previous_line = line;
            previous_col = col;
            Some(token)
        })
        .collect();
    tokens
}

/// Generate full-document semantic tokens using the semantic index.
pub fn get_semantic_tokens_semantic(doc: &Document, index: &SemanticIndex) -> SemanticTokens {
    build_semantic_tokens(doc, index, None)
}

/// Generate range semantic tokens using the semantic index.
pub fn get_semantic_tokens_range_semantic(
    doc: &Document,
    index: &SemanticIndex,
    range: Range,
) -> SemanticTokens {
    build_semantic_tokens(doc, index, Some(range))
}

fn build_semantic_tokens(
    doc: &Document,
    index: &SemanticIndex,
    range_filter: Option<Range>,
) -> SemanticTokens {
    let mut raw: Vec<RawToken> = Vec::new();

    // Index declarations by name and exact (line, col) for O(1) lookup
    let mut decl_by_name: HashMap<&str, (u32, u32)> = HashMap::new();
    let mut decl_positions: HashMap<(usize, usize), (u32, u32)> = HashMap::new();

    // Collect ARC variable names from AST (`share`, `strong`, `weak`)
    let mut arc_vars: HashMap<String, (u32, u32)> = HashMap::new();
    for stmt in &index.statements {
        match &stmt.kind {
            adeshlang::parsing::ast::StmtKind::ShareDeclaration(d, _) => {
                arc_vars.insert(d.name.clone(), (TT_SHARED_VAR, MOD_SHARED | MOD_READONLY));
            }
            adeshlang::parsing::ast::StmtKind::StrongDeclaration(d, _) => {
                arc_vars.insert(d.name.clone(), (TT_STRONG_REF, MOD_SHARED));
            }
            adeshlang::parsing::ast::StmtKind::WeakDeclaration(d, _) => {
                arc_vars.insert(d.name.clone(), (TT_WEAK_REF, MOD_WEAK));
            }
            _ => {}
        }
    }

    for sym in &index.symbols {
        let (tt, mut mods) = if let Some(&(arc_tt, arc_mods)) = arc_vars.get(&sym.name) {
            (arc_tt, arc_mods)
        } else {
            symbol_kind_to_token(sym.kind)
        };
        if sym.kind == SemanticSymbolKind::Constant
            || !sym.mutable && sym.kind == SemanticSymbolKind::Variable
        {
            mods |= MOD_READONLY;
        } else if sym.mutable && sym.kind == SemanticSymbolKind::Variable {
            mods |= MOD_MUTABLE;
        }
        if sym.is_static {
            mods |= MOD_STATIC;
        }
        if sym.is_async {
            mods |= MOD_ASYNC;
        }

        if sym.line > 0 && sym.col > 0 {
            decl_positions.insert(
                (sym.line, sym.col),
                (tt, mods | MOD_DECLARATION | MOD_DEFINITION),
            );
        }
        decl_by_name.entry(sym.name.as_str()).or_insert((tt, mods));
    }

    // Also map enum variants & type names from `index.types`
    let mut variant_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (tname, td) in &index.types {
        let (tt, mods) = symbol_kind_to_token(td.kind);
        decl_by_name.insert(tname.as_str(), (tt, mods));
        for (vname, _) in &td.variants {
            variant_names.insert(vname.as_str());
        }
    }

    // Classify every lexer token
    let tokens = &index.tokens;
    for (i, tok) in tokens.iter().enumerate() {
        if tok.line == 0 || tok.col == 0 || tok.lexeme.is_empty() {
            continue;
        }
        let line_0 = (tok.line - 1) as u32;
        if let Some(r) = range_filter {
            if line_0 < r.start.line || line_0 > r.end.line {
                continue;
            }
        }
        // Compiler lexer columns are UTF-8 byte offsets, LSP columns are UTF-16.
        let Some(line_text) = doc.get_line(line_0 as usize) else {
            continue;
        };
        let byte_col = tok.col.saturating_sub(1).min(line_text.len());
        let Some(before) = line_text.get(..byte_col) else {
            continue;
        };
        let start_0 = before.encode_utf16().count() as u32;
        let len_u16 = tok.lexeme.encode_utf16().count() as u32;
        if len_u16 == 0 {
            continue;
        }

        // Skip multi-line tokens (LSP semantic tokens without multiline support must be single-line)
        if tok.lexeme.contains('\n') {
            continue;
        }

        let prev_kind = i
            .checked_sub(1)
            .and_then(|k| tokens.get(k))
            .map(|t| &t.kind);
        let next_kind = tokens.get(i + 1).map(|t| &t.kind);

        let classified: Option<(u32, u32)> = match tok.kind {
            TokenKind::Identifier => {
                // Exact declaration site
                if let Some(&(tt, mods)) = decl_positions.get(&(tok.line, tok.col)) {
                    Some((tt, mods))
                } else if matches!(prev_kind, Some(TokenKind::Region)) {
                    Some((TT_REGION, MOD_DECLARATION))
                } else if matches!(
                    prev_kind,
                    Some(TokenKind::Dot) | Some(TokenKind::QuestionDot)
                ) {
                    if matches!(next_kind, Some(TokenKind::LeftParen)) {
                        Some((TT_METHOD, 0))
                    } else if variant_names.contains(tok.lexeme.as_str()) {
                        Some((TT_ENUM_MEMBER, MOD_READONLY))
                    } else {
                        Some((TT_PROPERTY, 0))
                    }
                } else if matches!(
                    tok.lexeme.as_str(),
                    "Math"
                        | "time"
                        | "JSON"
                        | "IO"
                        | "HTTP"
                        | "Env"
                        | "Path"
                        | "Crypto"
                        | "Random"
                        | "Process"
                        | "Encoding"
                        | "Compression"
                ) {
                    Some((TT_NAMESPACE, MOD_DEFAULT_LIBRARY))
                } else if is_builtin_type_name(&tok.lexeme) {
                    Some((TT_TYPE, MOD_DEFAULT_LIBRARY))
                } else if find_builtin_fn(&tok.lexeme).is_some() {
                    Some((TT_FUNCTION, MOD_DEFAULT_LIBRARY))
                } else if let Some(&(tt, mods)) = decl_by_name.get(tok.lexeme.as_str()) {
                    Some((tt, mods))
                } else if matches!(next_kind, Some(TokenKind::LeftParen)) {
                    Some((TT_FUNCTION, 0))
                } else if tok
                    .lexeme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase())
                {
                    Some((TT_TYPE_PARAM, 0))
                } else {
                    Some((TT_VARIABLE, 0))
                }
            }
            TokenKind::String | TokenKind::Template | TokenKind::CharLit => Some((TT_STRING, 0)),
            TokenKind::Number
            | TokenKind::BigIntLit
            | TokenKind::ComplexLit
            | TokenKind::U8Lit
            | TokenKind::U16Lit
            | TokenKind::U32Lit
            | TokenKind::U64Lit
            | TokenKind::U128Lit
            | TokenKind::I8Lit
            | TokenKind::I16Lit
            | TokenKind::I32Lit
            | TokenKind::I64Lit
            | TokenKind::I128Lit
            | TokenKind::F32Lit
            | TokenKind::F64Lit => Some((TT_NUMBER, 0)),
            TokenKind::DocComment => Some((TT_COMMENT, 1 << 8)), // DOCUMENTATION
            TokenKind::Share
            | TokenKind::Strong
            | TokenKind::Weak
            | TokenKind::Test
            | TokenKind::Region
            | TokenKind::Unsafe => Some((TT_KEYWORD, 0)),
            _ if crate::type_utils::is_reserved_keyword(&tok.lexeme) => Some((TT_KEYWORD, 0)),
            _ => None,
        };

        if let Some((token_type, modifiers)) = classified {
            // Ensure token does not exceed line bounds
            if let Some(line_text) = doc.get_line(line_0 as usize) {
                let line_len = line_text.encode_utf16().count() as u32;
                if start_0 < line_len {
                    let clamped_len = len_u16.min(line_len - start_0);
                    if clamped_len > 0 {
                        raw.push(RawToken {
                            line: line_0,
                            start: start_0,
                            length: clamped_len,
                            token_type,
                            modifiers,
                        });
                    }
                }
            }
        }
    }

    if let Some(range) = range_filter {
        raw.retain(|token| {
            Position::new(token.line, token.start) >= range.start
                && Position::new(token.line, token.start + token.length) <= range.end
        });
    }

    // Sort strictly by (line, start) and deduplicate overlapping start positions
    raw.sort_by_key(|t| (t.line, t.start));
    raw.dedup_by(|a, b| a.line == b.line && a.start == b.start);

    // Encode delta positions safely
    let mut data = Vec::with_capacity(raw.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for tok in raw {
        let delta_line = tok.line.saturating_sub(prev_line);
        let delta_start = if delta_line == 0 {
            tok.start.saturating_sub(prev_start)
        } else {
            tok.start
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: tok.length,
            token_type: tok.token_type,
            token_modifiers_bitset: tok.modifiers,
        });
        prev_line = tok.line;
        prev_start = tok.start;
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

fn symbol_kind_to_token(kind: SemanticSymbolKind) -> (u32, u32) {
    match kind {
        SemanticSymbolKind::Module | SemanticSymbolKind::Import => (TT_NAMESPACE, 0),
        SemanticSymbolKind::Class => (TT_CLASS, 0),
        SemanticSymbolKind::Struct => (TT_STRUCT, 0),
        SemanticSymbolKind::Enum => (TT_ENUM, 0),
        SemanticSymbolKind::EnumVariant => (TT_ENUM_MEMBER, MOD_READONLY),
        SemanticSymbolKind::Interface => (TT_INTERFACE, 0),
        SemanticSymbolKind::TypeAlias => (TT_TYPE, 0),
        SemanticSymbolKind::Parameter => (TT_PARAMETER, 0),
        SemanticSymbolKind::Variable => (TT_VARIABLE, 0),
        SemanticSymbolKind::Constant => (TT_VARIABLE, MOD_READONLY),
        SemanticSymbolKind::Field | SemanticSymbolKind::Property => (TT_PROPERTY, 0),
        SemanticSymbolKind::Function => (TT_FUNCTION, 0),
        SemanticSymbolKind::Method => (TT_METHOD, 0),
        SemanticSymbolKind::StaticMethod => (TT_METHOD, MOD_STATIC),
    }
}

fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "Int"
            | "uint"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "f32"
            | "f64"
            | "float"
            | "number"
            | "bigint"
            | "bool"
            | "string"
            | "str"
            | "char"
            | "void"
            | "any"
            | "never"
            | "Array"
            | "Map"
            | "Set"
            | "Option"
            | "Result"
            | "Promise"
    )
}

/// Legacy entry point kept for backward compatibility.
#[allow(dead_code)]
pub fn get_semantic_tokens(doc: &Document, _symbols: &[SymbolInfo]) -> SemanticTokens {
    let index = adeshlang::semantics::index_source(&doc.content);
    get_semantic_tokens_semantic(doc, &index)
}

/// Legacy range entry point kept for backward compatibility.
#[allow(dead_code)]
pub fn get_semantic_tokens_range(
    doc: &Document,
    _symbols: &[SymbolInfo],
    range: Range,
) -> SemanticTokens {
    let index = adeshlang::semantics::index_source(&doc.content);
    get_semantic_tokens_range_semantic(doc, &index, range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tokens_never_panics_with_keywords_and_symbols() {
        let src = "share config = 1;\nfn main() {\n    let x = 42;\n    assert(x == 42);\n}\n";
        let uri = Url::parse("file:///tokens.adesh").unwrap();
        let doc = Document::new(uri, src.to_string(), 1);
        let index = adeshlang::semantics::index_source(src);
        let tokens = get_semantic_tokens_semantic(&doc, &index);
        assert!(!tokens.data.is_empty());
    }
}
