//! A deliberately small, escaped Markdown renderer. Raw HTML is never trusted.
use super::{ItemDoc, escape};
use regex::Regex;
use std::sync::LazyLock;

static CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r#"(?s)/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b[0-9]+(?:\.[0-9]+)?(?:[a-zA-Z][a-zA-Z0-9]*)?|\b[\p{L}_][\p{L}\p{N}_]*"#
).unwrap()
});

pub(super) fn highlight(src: &str) -> String {
    let mut out = String::new();
    let mut end = 0;
    for token in CODE.find_iter(src) {
        out.push_str(&escape(&src[end..token.start()]));
        let value = token.as_str();
        let class = if value.starts_with("//") || value.starts_with("/*") {
            "comment"
        } else if value.starts_with(['"', '\'', '`']) {
            "string"
        } else if value.as_bytes()[0].is_ascii_digit() {
            "number"
        } else if matches!(
            value,
            "fn" | "let"
                | "const"
                | "return"
                | "if"
                | "else"
                | "elif"
                | "do"
                | "while"
                | "for"
                | "in"
                | "of"
                | "class"
                | "new"
                | "this"
                | "self"
                | "extend"
                | "extends"
                | "extern"
                | "private"
                | "protected"
                | "public"
                | "super"
                | "abstract"
                | "sealed"
                | "interface"
                | "implements"
                | "type"
                | "decorator"
                | "on"
                | "import"
                | "as"
                | "export"
                | "from"
                | "default"
                | "try"
                | "catch"
                | "throw"
                | "async"
                | "await"
                | "spawn"
                | "static"
                | "constructor"
                | "break"
                | "continue"
                | "struct"
                | "enum"
                | "match"
                | "readonly"
                | "raw"
                | "region"
                | "unsafe"
                | "share"
                | "strong"
                | "weak"
                | "alloc"
                | "free"
                | "defer"
                | "test"
                | "and"
                | "or"
                | "not"
                | "instanceof"
                | "typeof"
                | "compile"
                | "runtime"
                | "require"
        ) {
            "keyword"
        } else if matches!(value, "true" | "false" | "null") {
            "literal"
        } else if matches!(
            value,
            "number"
                | "string"
                | "bool"
                | "boolean"
                | "void"
                | "any"
                | "int"
                | "uint"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "f32"
                | "f64"
        ) || value.chars().next().is_some_and(char::is_uppercase)
        {
            "type"
        } else if src[token.end()..].trim_start().starts_with('(') {
            "function"
        } else {
            ""
        };
        if class.is_empty() {
            out.push_str(&escape(value));
        } else {
            out.push_str(&format!(
                "<span class=\"tok-{class}\">{}</span>",
                escape(value)
            ));
        }
        end = token.end();
    }
    out.push_str(&escape(&src[end..]));
    out
}

fn symbol_link(name: &str, items: &[ItemDoc]) -> Option<String> {
    let mut matches = items.iter().filter(|item| item.name == name);
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(format!("<a href=\"#{}\">{}</a>", first.id, escape(name)))
}

fn safe_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    !url.chars().any(|c| c.is_control() || c.is_whitespace())
        && (lower.starts_with("https://")
            || lower.starts_with("http://")
            || lower.starts_with("mailto:")
            || lower.starts_with('#'))
}

pub(super) fn inline(src: &str, items: &[ItemDoc]) -> String {
    inline_depth(src, items, 0)
}

fn inline_depth(src: &str, items: &[ItemDoc], depth: usize) -> String {
    if depth > 12 {
        return escape(src);
    }
    let mut out = String::new();
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("{@link ") {
            if let Some(end) = tail.find('}') {
                let name = tail[..end].trim();
                out.push_str(&symbol_link(name, items).unwrap_or_else(|| escape(name)));
                rest = &tail[end + 1..];
                continue;
            }
        }
        let mut consumed = false;
        for (mark, tag) in [("`", "code"), ("**", "strong"), ("*", "em")] {
            if let Some(tail) = rest.strip_prefix(mark) {
                if let Some(end) = tail.find(mark) {
                    let content = &tail[..end];
                    let content = if tag == "code" {
                        symbol_link(content, items).unwrap_or_else(|| escape(content))
                    } else {
                        inline_depth(content, items, depth + 1)
                    };
                    out.push_str(&format!("<{tag}>{content}</{tag}>"));
                    rest = &tail[end + mark.len()..];
                    consumed = true;
                    break;
                }
            }
        }
        if consumed {
            continue;
        }
        if let Some(tail) = rest.strip_prefix('[') {
            if let Some(mid) = tail.find("](") {
                if let Some(end) = tail[mid + 2..].find(')') {
                    let url = &tail[mid + 2..mid + 2 + end];
                    if safe_url(url) {
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape(url),
                            escape(&tail[..mid])
                        ));
                        rest = &tail[mid + 3 + end..];
                        continue;
                    }
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        out.push_str(&escape(&ch.to_string()));
        rest = &rest[ch.len_utf8()..];
    }
    out
}

pub(super) fn code_block(code: &str, language: &str) -> String {
    let rendered = if matches!(language, "" | "adesh" | "adeshlang" | "ind") {
        highlight(code)
    } else {
        escape(code)
    };
    let label = if language.is_empty() {
        "adesh"
    } else {
        language
    };
    format!(
        "<div class=\"code-block\"><div class=\"code-toolbar\"><span>{}</span><button type=\"button\" class=\"copy-code\" aria-label=\"Copy code\">Copy</button></div><pre><code>{rendered}</code></pre></div>",
        escape(label)
    )
}

fn table_cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

pub(super) fn markdown(src: &str, items: &[ItemDoc]) -> String {
    let lines: Vec<_> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }
        if let Some(language) = line.strip_prefix("```") {
            i += 1;
            let mut code = Vec::new();
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            out.push_str(&code_block(&code.join("\n"), language.trim()));
            i += usize::from(i < lines.len());
            continue;
        }
        if i + 1 < lines.len()
            && line.contains('|')
            && lines[i + 1].contains('-')
            && lines[i + 1]
                .chars()
                .all(|c| matches!(c, '|' | ':' | '-' | ' ' | '\t'))
        {
            out.push_str("<div class=\"table-wrap\"><table><thead><tr>");
            for cell in table_cells(line) {
                out.push_str(&format!("<th scope=\"col\">{}</th>", inline(cell, items)));
            }
            out.push_str("</tr></thead><tbody>");
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                out.push_str("<tr>");
                for cell in table_cells(lines[i]) {
                    out.push_str(&format!("<td>{}</td>", inline(cell, items)));
                }
                out.push_str("</tr>");
                i += 1;
            }
            out.push_str("</tbody></table></div>");
            continue;
        }
        if line.starts_with('#') {
            let title = line.trim_start_matches('#').trim();
            out.push_str(&format!("<h4>{}</h4>", inline(title, items)));
            i += 1;
            continue;
        }
        if let Some(quote) = line.strip_prefix("> ") {
            out.push_str(&format!(
                "<blockquote>{}</blockquote>",
                inline(quote, items)
            ));
            i += 1;
            continue;
        }
        let ordered = line
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if line.starts_with("- ") || line.starts_with("* ") || ordered {
            let tag = if ordered { "ol" } else { "ul" };
            out.push_str(&format!("<{tag}>"));
            while i < lines.len() {
                let l = lines[i].trim();
                let content = if ordered {
                    l.split_once(". ")
                        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                        .map(|(_, c)| c)
                } else {
                    l.strip_prefix("- ").or_else(|| l.strip_prefix("* "))
                };
                let Some(content) = content else {
                    break;
                };
                out.push_str(&format!("<li>{}</li>", inline(content, items)));
                i += 1;
            }
            out.push_str(&format!("</{tag}>"));
            continue;
        }
        // Soft line breaks stay in the paragraph; explicit blocks start a new one.
        let mut paragraph = vec![line];
        i += 1;
        while i < lines.len() {
            let next = lines[i].trim();
            if next.is_empty()
                || next.starts_with(['#', '>', '|'])
                || next.starts_with("```")
                || next.starts_with("- ")
                || next.starts_with("* ")
                || next
                    .split_once(". ")
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                || (i + 1 < lines.len()
                    && next.contains('|')
                    && lines[i + 1].trim().starts_with('|'))
            {
                break;
            }
            paragraph.push(next);
            i += 1;
        }
        out.push_str(&format!("<p>{}</p>", inline(&paragraph.join(" "), items)));
    }
    out
}

#[derive(Default)]
pub(super) struct Comment {
    pub description: String,
    pub tags: Vec<(String, String)>,
}

pub(super) fn comment(src: &str) -> Comment {
    let mut result = Comment::default();
    let mut fenced = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && trimmed.starts_with('@') {
            let (name, value) = trimmed[1..]
                .split_once(char::is_whitespace)
                .unwrap_or((&trimmed[1..], ""));
            result
                .tags
                .push((name.to_ascii_lowercase(), value.trim().into()));
        } else if let Some((_, value)) = result.tags.last_mut() {
            value.push('\n');
            value.push_str(line);
        } else {
            result.description.push_str(line);
            result.description.push('\n');
        }
    }
    result
}

pub(super) fn typed_value(value: &str) -> (&str, &str) {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('{') {
        if let Some(end) = rest.find('}') {
            return (&rest[..end], rest[end + 1..].trim());
        }
    }
    ("", value)
}
