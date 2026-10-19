//! Offline API documentation, collected from declaration-adjacent `/** ... */` comments.
mod markup;
mod render;

use crate::parsing::{
    ast::TokenKind,
    lexer::{Lexer, Token},
};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
struct ItemDoc {
    id: String,
    kind: String,
    name: String,
    doc: String,
    file: String,
    line: usize,
    signature: String,
    source: String,
    exported: bool,
}

fn escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            _ => c.to_string(),
        })
        .collect()
}

fn item_id(file: &str, kind: &str, name: &str, line: usize) -> String {
    // Unlike a sequence number, this remains stable when unrelated files are added.
    let key = format!("{file}:{kind}:{name}:{line}");
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    format!(
        "{}-{hash:016x}",
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            })
            .collect::<String>()
    )
}

fn declaration_kind(kind: TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Fn => "function",
        TokenKind::Class => "class",
        TokenKind::Struct => "struct",
        TokenKind::Enum => "enum",
        TokenKind::Interface => "interface",
        TokenKind::Type => "type",
        TokenKind::Const => "constant",
        TokenKind::Let => "variable",
        _ => return None,
    })
}

fn modifier(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Export
            | TokenKind::Default
            | TokenKind::Async
            | TokenKind::Public
            | TokenKind::Private
            | TokenKind::Protected
            | TokenKind::Static
            | TokenKind::Abstract
            | TokenKind::Sealed
            | TokenKind::Readonly
    )
}

fn offset(token: &Token, lines: &[usize], len: usize) -> usize {
    lines
        .get(token.line.saturating_sub(1))
        .copied()
        .unwrap_or(len)
        .saturating_add(token.col.saturating_sub(1))
        .min(len)
}

fn declaration_source(
    src: &str,
    tokens: &[Token],
    start: usize,
    decl: usize,
    lines: &[usize],
) -> (String, String) {
    let begin = offset(&tokens[start], lines, src.len());
    let mut signature_end = None;
    let mut end = begin;
    let (mut parens, mut brackets, mut braces) = (0usize, 0usize, 0usize);
    let body = matches!(
        tokens[decl].kind,
        TokenKind::Fn
            | TokenKind::Class
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Interface
    );
    for (index, token) in tokens[decl..].iter().enumerate() {
        let pos = offset(token, lines, src.len());
        if token.kind == TokenKind::Eof {
            break;
        }
        // Type aliases may omit their final semicolon. Never let a signature
        // absorb the next declaration (or its documentation).
        if index > 0
            && parens == 0
            && brackets == 0
            && braces == 0
            && (token.kind == TokenKind::DocComment
                || declaration_kind(token.kind).is_some()
                || modifier(token.kind))
        {
            break;
        }
        if body && token.kind == TokenKind::LeftBrace && parens == 0 && brackets == 0 && braces == 0
        {
            signature_end = Some(pos);
        }
        match token.kind {
            TokenKind::LeftParen => parens += 1,
            TokenKind::RightParen => parens = parens.saturating_sub(1),
            TokenKind::LeftBracket => brackets += 1,
            TokenKind::RightBracket => brackets = brackets.saturating_sub(1),
            TokenKind::LeftBrace => braces += 1,
            TokenKind::RightBrace => braces = braces.saturating_sub(1),
            _ => {}
        }
        // The lexer normalizes string lexemes. Use the next token's offset for
        // interior text, and only fixed-width delimiters for the final boundary.
        end = (pos + token.lexeme.len()).min(src.len());
        if parens == 0
            && brackets == 0
            && braces == 0
            && (token.kind == TokenKind::Semicolon || (body && token.kind == TokenKind::RightBrace))
        {
            break;
        }
    }
    let source = src.get(begin..end).unwrap_or("").trim().to_string();
    let signature = src
        .get(begin..signature_end.unwrap_or(end))
        .unwrap_or("")
        .trim()
        .to_string();
    (signature, source)
}

fn collect_docs_from_file(path: &Path, file: &str) -> Result<(Vec<ItemDoc>, Vec<String>), String> {
    let src = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let tokens = Lexer::new(&src)
        .tokenize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = vec![0];
    lines.extend(src.match_indices('\n').map(|(i, _)| i + 1));
    let (mut items, mut imports) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == TokenKind::Import {
            // Both `import "..." as x` and `import { x } from "..."`.
            for token in &tokens[i + 1..] {
                if matches!(token.kind, TokenKind::Semicolon | TokenKind::Eof) {
                    break;
                }
                if token.kind == TokenKind::String {
                    imports.push(token.lexeme.trim_matches(['"', '\'']).to_string());
                    break;
                }
            }
        }
        if tokens[i].kind != TokenKind::DocComment {
            i += 1;
            continue;
        }
        let mut comments = vec![tokens[i].lexeme.clone()];
        let mut start = i + 1;
        while start < tokens.len()
            && matches!(
                tokens[start].kind,
                TokenKind::DocComment | TokenKind::Semicolon
            )
        {
            if tokens[start].kind == TokenKind::DocComment {
                comments.push(tokens[start].lexeme.clone());
            }
            start += 1;
        }
        let mut decl = start;
        while decl < tokens.len() && modifier(tokens[decl].kind) {
            decl += 1;
        }
        if let (Some(token), Some(name)) = (tokens.get(decl), tokens.get(decl + 1)) {
            if let Some(kind) = declaration_kind(token.kind) {
                if name.kind == TokenKind::Identifier {
                    let (signature, source) =
                        declaration_source(&src, &tokens, start, decl, &lines);
                    items.push(ItemDoc {
                        id: item_id(file, kind, &name.lexeme, token.line),
                        kind: kind.into(),
                        name: name.lexeme.clone(),
                        doc: comments.join("\n"),
                        file: file.into(),
                        line: token.line,
                        signature,
                        source,
                        exported: tokens[start..decl]
                            .iter()
                            .any(|t| t.kind == TokenKind::Export || t.kind == TokenKind::Public),
                    });
                }
            }
        }
        i = start;
    }
    Ok((items, imports))
}

fn resolve_import(base: &Path, import: &str) -> Option<PathBuf> {
    let path = base.join(import);
    if path.is_file() {
        return Some(path);
    }
    if path.extension().is_none() {
        for extension in ["adesh", "ind"] {
            let candidate = path.with_extension(extension);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn relative_file(path: &Path, root: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        return relative.to_string_lossy().replace('\\', "/");
    }
    // Preserve paths for sibling imports without leaking the machine's absolute path.
    let mut ancestor = root;
    let mut prefix = String::new();
    while let Some(parent) = ancestor.parent() {
        prefix.push_str("../");
        if let Ok(relative) = path.strip_prefix(parent) {
            return format!("{prefix}{}", relative.to_string_lossy().replace('\\', "/"));
        }
        ancestor = parent;
    }
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn generate_docs(entry: &Path, out_dir: &Path) -> Result<(), String> {
    let entry = fs::canonicalize(entry).map_err(|e| format!("{}: {e}", entry.display()))?;
    let root = entry.parent().unwrap_or(Path::new("."));
    let mut pending = vec![entry.clone()];
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let path = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let file = relative_file(&path, root);
        let (docs, imports) = collect_docs_from_file(&path, &file)?;
        files.push(file);
        items.extend(docs);
        for import in imports {
            if let Some(imported) = resolve_import(path.parent().unwrap_or(root), &import) {
                pending.push(imported);
            }
        }
    }
    files.sort();
    items.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
    let project = entry.file_stem().unwrap_or_default().to_string_lossy();
    let html = render::html(&project, &items, &files);
    // Serialize before writing any output. JSON is an external asset, never executable HTML.
    let index = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    for (name, content) in [
        ("index.html", html.as_str()),
        ("style.css", include_str!("style.css")),
        ("app.js", include_str!("app.js")),
        ("search-index.json", index.as_str()),
        ("logo.svg", include_str!("../../../Logo/Logo.svg")),
    ] {
        fs::write(out_dir.join(name), content)
            .map_err(|e| format!("{}: {e}", out_dir.join(name).display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
