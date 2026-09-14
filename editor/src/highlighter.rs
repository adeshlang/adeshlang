use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashSet;
use std::path::Path;

use crate::config::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Adesh,
    Rust,
    Python,
    C,
    Cpp,
    JavaScript,
    TypeScript,
    Json,
    Html,
    Css,
    Markdown,
    Toml,
    Yaml,
    Shell,
    Generic,
}

impl Language {
    pub fn from_path(path: Option<&Path>) -> Self {
        let ext = path
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "ad" | "adl" | "adesh" => Language::Adesh,
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "c" | "h" => Language::C,
            "cpp" | "cxx" | "cc" | "hpp" => Language::Cpp,
            "js" | "jsx" | "mjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "json" => Language::Json,
            "html" | "htm" => Language::Html,
            "css" | "scss" | "sass" => Language::Css,
            "md" | "markdown" => Language::Markdown,
            "toml" => Language::Toml,
            "yaml" | "yml" => Language::Yaml,
            "sh" | "bash" | "zsh" | "ps1" => Language::Shell,
            _ => Language::Adesh, // Default to Adesh
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Adesh => "Adesh",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Json => "JSON",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Markdown => "Markdown",
            Language::Toml => "TOML",
            Language::Yaml => "YAML",
            Language::Shell => "Shell",
            Language::Generic => "Plain Text",
        }
    }
}

// ── Adesh Keywords ──────────────────────────────────────────────
static ADESH_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "fn",
        "let",
        "mut",
        "const",
        "static",
        "type",
        "struct",
        "enum",
        "trait",
        "impl",
        "class",
        "interface",
        "module",
        "import",
        "use",
        "export",
        "pub",
        "extern",
        "decorator",
        "if",
        "else",
        "for",
        "while",
        "do",
        "loop",
        "return",
        "match",
        "break",
        "continue",
        "try",
        "catch",
        "throw",
        "defer",
        "async",
        "await",
        "as",
        "in",
        "where",
        "ref",
        "new",
        "this",
        "share",
        "alloc",
        "unsafe",
        "move",
        "borrow",
        "own",
        "region",
        "parallel",
        "simd",
        "gpu",
        "wasm",
        "embedded",
    ]
    .into_iter()
    .collect()
});

// ── Rust Keywords ──────────────────────────────────────────────
static RUST_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
    ]
    .into_iter()
    .collect()
});

// ── Python Keywords ──────────────────────────────────────────────
static PYTHON_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "self",
    ]
    .into_iter()
    .collect()
});

// ── JS/TS Keywords ──────────────────────────────────────────────
static JS_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
        "let",
        "static",
        "enum",
        "await",
        "async",
        "interface",
        "type",
        "implements",
        "package",
        "protected",
        "private",
        "public",
        "readonly",
    ]
    .into_iter()
    .collect()
});

// ── C/C++ Keywords ──────────────────────────────────────────────
static C_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "auto",
        "break",
        "case",
        "char",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "float",
        "for",
        "goto",
        "if",
        "int",
        "long",
        "register",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "typedef",
        "union",
        "unsigned",
        "void",
        "volatile",
        "while",
        "class",
        "namespace",
        "template",
        "typename",
        "using",
        "virtual",
        "friend",
        "inline",
        "constexpr",
        "nullptr",
        "bool",
        "true",
        "false",
    ]
    .into_iter()
    .collect()
});

static CONSTANTS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "true",
        "false",
        "null",
        "none",
        "nil",
        "undefined",
        "NaN",
        "Infinity",
        "None",
        "Some",
        "Ok",
        "Err",
    ]
    .into_iter()
    .collect()
});

static TYPES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "int", "uint", "Int", "UInt", "f32", "f64", "float", "Float", "double", "Double", "bool",
        "Bool", "char", "str", "String", "void", "any", "ptr", "Vec", "Map", "Set", "HashMap",
        "BTreeMap", "Array", "Slice", "Box", "Arc", "Rc", "Option", "Result", "Self",
    ]
    .into_iter()
    .collect()
});

static BUILTINS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "print",
        "println",
        "eprint",
        "eprintln",
        "printf",
        "scanf",
        "read",
        "write",
        "typeof",
        "sizeof",
        "type_of",
        "size_of",
        "len",
        "push",
        "pop",
        "insert",
        "remove",
        "contains",
        "sort",
        "reverse",
        "map",
        "filter",
        "reduce",
        "find",
        "abs",
        "min",
        "max",
        "sqrt",
        "pow",
        "assert",
        "assert_eq",
        "panic",
    ]
    .into_iter()
    .collect()
});

const MULTI_OPS: &[&str] = &[
    "...", "..=", "<<=", ">>=", "&&=", "||=", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "->", "=>", "..", "::",
];

fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn match_multi_op(chars: &[char], i: usize) -> usize {
    let remaining = &chars[i..];
    for op in MULTI_OPS {
        let op_chars: Vec<char> = op.chars().collect();
        if remaining.len() >= op_chars.len() && remaining[..op_chars.len()] == op_chars[..] {
            return op_chars.len();
        }
    }
    0
}

fn is_bracket(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}')
}

pub fn highlight_line<'a>(
    line: &'a str,
    theme: &Theme,
    lang: Language,
    rainbow_brackets: bool,
    matching_bracket: Option<usize>, // Column index of matching bracket on this line if any
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut bracket_depth: usize = 0;

    let style_fg = Style::default().fg(theme.fg);
    let style_kw = Style::default()
        .fg(theme.keyword)
        .add_modifier(Modifier::BOLD);
    let style_const = Style::default().fg(theme.number);
    let style_str = Style::default().fg(theme.string);
    let style_num = Style::default().fg(theme.number);
    let style_comment = Style::default().fg(theme.comment);
    let style_doc = Style::default()
        .fg(theme.comment)
        .add_modifier(Modifier::ITALIC);
    let style_type = Style::default().fg(theme.type_color);
    let style_func = Style::default().fg(theme.function);
    let style_op = Style::default().fg(theme.operator);
    let style_punct = Style::default().fg(theme.comment);
    let style_attr = Style::default()
        .fg(theme.type_color)
        .add_modifier(Modifier::BOLD);

    while i < len {
        let ch = chars[i];

        // 1. Comments
        if (ch == '/' && i + 1 < len && (chars[i + 1] == '/' || chars[i + 1] == '*'))
            || (lang == Language::Python
                || lang == Language::Shell
                || lang == Language::Toml
                || lang == Language::Yaml)
                && ch == '#'
        {
            if ch == '#' || (ch == '/' && chars[i + 1] == '/') {
                let is_doc = i + 2 < len && (chars[i + 2] == '/' || chars[i + 2] == '!');
                let rest: String = chars[i..].iter().collect();
                spans.push(Span::styled(
                    rest,
                    if is_doc { style_doc } else { style_comment },
                ));
                break;
            } else if ch == '/' && chars[i + 1] == '*' {
                let start = i;
                i += 2;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                if i + 1 < len {
                    i += 2;
                } else {
                    i = len;
                }
                let comment_text: String = chars[start..i].iter().collect();
                spans.push(Span::styled(comment_text, style_comment));
                continue;
            }
        }

        // 2. Strings & Character literals
        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < len {
                let c = chars[i];
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            spans.push(Span::styled(s, style_str));
            continue;
        }

        // 3. Numbers
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            if ch == '0' && i + 1 < len && matches!(chars[i + 1], 'x' | 'X' | 'b' | 'B' | 'o' | 'O')
            {
                i += 2;
                while i < len && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
                    i += 1;
                }
            } else {
                while i < len
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == '_'
                        || chars[i] == 'e'
                        || chars[i] == 'E')
                {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            spans.push(Span::styled(s, style_num));
            continue;
        }

        // 4. Attributes / Decorators (@name or #[...])
        if ch == '@' || (ch == '#' && i + 1 < len && chars[i + 1] == '[') {
            let start = i;
            i += 1;
            while i < len && is_ident_char(chars[i]) {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            spans.push(Span::styled(s, style_attr));
            continue;
        }

        // 5. Identifiers & Keywords
        if is_ident_start(ch) {
            let start = i;
            while i < len && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();

            let is_kw = match lang {
                Language::Rust => RUST_KEYWORDS.contains(word.as_str()),
                Language::Python => PYTHON_KEYWORDS.contains(word.as_str()),
                Language::JavaScript | Language::TypeScript => JS_KEYWORDS.contains(word.as_str()),
                Language::C | Language::Cpp => C_KEYWORDS.contains(word.as_str()),
                _ => ADESH_KEYWORDS.contains(word.as_str()),
            };

            let style = if is_kw {
                style_kw
            } else if CONSTANTS.contains(word.as_str()) {
                style_const
            } else if TYPES.contains(word.as_str())
                || (word.starts_with(char::is_uppercase) && !word.contains('_'))
            {
                style_type
            } else if BUILTINS.contains(word.as_str()) {
                style_func
            } else if i < len && chars[i] == '(' {
                style_func
            } else {
                style_fg
            };

            spans.push(Span::styled(word, style));
            continue;
        }

        // 6. Rainbow Brackets & Bracket Matching
        if is_bracket(ch) {
            let is_match = matching_bracket == Some(i);
            let style = if is_match {
                Style::default()
                    .fg(theme.matching_bracket)
                    .bg(theme.current_line_bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else if rainbow_brackets {
                let color = theme.rainbow_brackets[bracket_depth % theme.rainbow_brackets.len()];
                if matches!(ch, '(' | '[' | '{') {
                    bracket_depth += 1;
                } else if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                style_punct
            };

            spans.push(Span::styled(ch.to_string(), style));
            i += 1;
            continue;
        }

        // 7. Multi-character operators
        let op_len = match_multi_op(&chars, i);
        if op_len > 0 {
            let s: String = chars[i..i + op_len].iter().collect();
            spans.push(Span::styled(s, style_op));
            i += op_len;
            continue;
        }

        // 8. Single-character operators / punctuation / whitespace
        if "+-*/%=&|^!<>?:~".contains(ch) {
            spans.push(Span::styled(ch.to_string(), style_op));
        } else if ";,.".contains(ch) {
            spans.push(Span::styled(ch.to_string(), style_punct));
        } else {
            spans.push(Span::styled(ch.to_string(), style_fg));
        }
        i += 1;
    }

    Line::from(spans)
}

/// Parse a raw ANSI string (containing standard, bright, 256, and RGB 24-bit TrueColor SGR escape sequences)
/// into a styled Ratatui `Line<'static>`.
pub fn parse_ansi_to_line(raw: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_style = Style::default();
    let mut current_text = String::new();

    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some(&'[') => {
                    chars.next(); // consume '['
                    let mut seq = String::new();
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if (nc >= 'a' && nc <= 'z') || (nc >= 'A' && nc <= 'Z') {
                            if nc == 'm' {
                                // SGR style code
                                if !current_text.is_empty() {
                                    spans.push(Span::styled(current_text.clone(), current_style));
                                    current_text.clear();
                                }
                                apply_ansi_sgr(&seq, &mut current_style);
                            }
                            // Non-SGR CSI codes (cursor, clear) are stripped
                            break;
                        } else {
                            seq.push(nc);
                        }
                    }
                }
                Some(&']') => {
                    chars.next(); // consume ']'
                    // OSC sequence: consume until BEL (\x07) or ST (\x1b\)
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc == '\x07' {
                            break;
                        }
                        if nc == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c == '\r' {
            // Carriage returns handled per line
        } else if c == '\t' {
            current_text.push_str("    ");
        } else if c.is_control() && c != '\n' {
            // Ignore other non-printable control characters
        } else {
            current_text.push(c);
        }
    }

    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }

    if spans.is_empty() {
        Line::from(vec![Span::raw("")])
    } else {
        Line::from(spans)
    }
}

fn apply_ansi_sgr(seq: &str, style: &mut Style) {
    if seq.is_empty() || seq == "0" {
        *style = Style::default();
        return;
    }

    let parts: Vec<u8> = seq
        .split(';')
        .filter_map(|s| s.trim().parse::<u8>().ok())
        .collect();

    let mut i = 0;
    while i < parts.len() {
        let code = parts[i];
        match code {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),

            // Standard FG
            30 => *style = style.fg(Color::Black),
            31 => *style = style.fg(Color::Red),
            32 => *style = style.fg(Color::Green),
            33 => *style = style.fg(Color::Yellow),
            34 => *style = style.fg(Color::Blue),
            35 => *style = style.fg(Color::Magenta),
            36 => *style = style.fg(Color::Cyan),
            37 => *style = style.fg(Color::Gray),
            39 => *style = style.fg(Color::Reset),

            // Standard BG
            40 => *style = style.bg(Color::Black),
            41 => *style = style.bg(Color::Red),
            42 => *style = style.bg(Color::Green),
            43 => *style = style.bg(Color::Yellow),
            44 => *style = style.bg(Color::Blue),
            45 => *style = style.bg(Color::Magenta),
            46 => *style = style.bg(Color::Cyan),
            47 => *style = style.bg(Color::Gray),
            49 => *style = style.bg(Color::Reset),

            // Bright FG
            90 => *style = style.fg(Color::DarkGray),
            91 => *style = style.fg(Color::LightRed),
            92 => *style = style.fg(Color::LightGreen),
            93 => *style = style.fg(Color::LightYellow),
            94 => *style = style.fg(Color::LightBlue),
            95 => *style = style.fg(Color::LightMagenta),
            96 => *style = style.fg(Color::LightCyan),
            97 => *style = style.fg(Color::White),

            // Bright BG
            100 => *style = style.bg(Color::DarkGray),
            101 => *style = style.bg(Color::LightRed),
            102 => *style = style.bg(Color::LightGreen),
            103 => *style = style.bg(Color::LightYellow),
            104 => *style = style.bg(Color::LightBlue),
            105 => *style = style.bg(Color::LightMagenta),
            106 => *style = style.bg(Color::LightCyan),
            107 => *style = style.bg(Color::White),

            // Extended 38 (FG) / 48 (BG)
            38 => {
                if i + 2 < parts.len() && parts[i + 1] == 5 {
                    *style = style.fg(Color::Indexed(parts[i + 2]));
                    i += 2;
                } else if i + 4 < parts.len() && parts[i + 1] == 2 {
                    *style = style.fg(Color::Rgb(parts[i + 2], parts[i + 3], parts[i + 4]));
                    i += 4;
                }
            }
            48 => {
                if i + 2 < parts.len() && parts[i + 1] == 5 {
                    *style = style.bg(Color::Indexed(parts[i + 2]));
                    i += 2;
                } else if i + 4 < parts.len() && parts[i + 1] == 2 {
                    *style = style.bg(Color::Rgb(parts[i + 2], parts[i + 3], parts[i + 4]));
                    i += 4;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_highlight() {
        let theme = Theme::adesh_dark();
        let line = highlight_line("let mut x = 42;", &theme, Language::Adesh, false, None);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_doc_comment_highlight() {
        let theme = Theme::adesh_dark();
        let line = highlight_line(
            "/// documentation comment",
            &theme,
            Language::Adesh,
            false,
            None,
        );
        assert_eq!(line.spans[0].content, "/// documentation comment");
    }

    #[test]
    fn test_multi_op() {
        let theme = Theme::adesh_dark();
        let line = highlight_line("x <= 10 && y >= 20", &theme, Language::Adesh, false, None);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_rainbow_brackets() {
        let theme = Theme::tokyo_night();
        let line = highlight_line(
            "fn foo(a: [i32; 4]) { }",
            &theme,
            Language::Rust,
            true,
            None,
        );
        assert!(!line.spans.is_empty());
    }
}
