//! Lexer
//!
//! Tokenizes source text into `Token`s with precise line/column and full line
//! snippets for diagnostics. Supports rich operators (optional chaining,
//! null-coalescing, exponentiation), doc-comments, strings/templates, BigInt,
//! and identifiers/keywords mapped to `TokenKind`.
use super::ast::TokenKind;
use crate::parsing::error::{ErrorKind, LangError};

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    #[allow(dead_code)]
    pub line: usize,
    // 1-based column number
    pub col: usize,
    // the full source line text where this token appears (no trailing newline)
    pub line_text: String,
}

#[derive(Clone)]
pub struct Lexer<'a> {
    src: &'a [u8],
    start: usize,
    current: usize,
    line: usize,
    /// Optional source file path for clickable diagnostics
    file: Option<String>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let bytes = input.as_bytes();
        let start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
        Self {
            src: bytes,
            start,
            current: start,
            line: 1,
            file: None,
        }
    }

    /// Attach a file path so lexical errors print as `file:line:col`
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        let f = file.into();
        if !f.is_empty() {
            self.file = Some(f);
        }
        self
    }

    pub fn set_file(&mut self, file: impl Into<String>) {
        let f = file.into();
        if !f.is_empty() {
            self.file = Some(f);
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LangError> {
        let mut tokens = Vec::new();
        while !self.is_end() {
            self.start = self.current;
            if let Some(tok) = self.scan()? {
                tokens.push(tok);
            }
        }
        tokens.push(self.build_token(TokenKind::Eof, String::new()));
        Ok(tokens)
    }

    fn is_end(&self) -> bool {
        self.current >= self.src.len()
    }

    fn adv(&mut self) -> u8 {
        let c = self.src[self.current];
        self.current += 1;
        c
    }

    fn peek(&self) -> u8 {
        if self.is_end() {
            0
        } else {
            self.src[self.current]
        }
    }

    fn peek2(&self) -> u8 {
        if self.current + 1 >= self.src.len() {
            0
        } else {
            self.src[self.current + 1]
        }
    }

    fn matchc(&mut self, b: u8) -> bool {
        if self.is_end() || self.src[self.current] != b {
            false
        } else {
            self.current += 1;
            true
        }
    }

    fn match_str(&mut self, s: &str) -> bool {
        let bytes = s.as_bytes();
        if self.current + bytes.len() > self.src.len() {
            return false;
        }
        for (i, &b) in bytes.iter().enumerate() {
            if self.src[self.current + i] != b {
                return false;
            }
        }
        // Ensure it's not part of a larger identifier
        let next_pos = self.current + bytes.len();
        if next_pos < self.src.len() {
            let next_char = self.src[next_pos] as char;
            if next_char.is_alphanumeric() || next_char == '_' {
                return false;
            }
        }
        self.current += bytes.len();
        true
    }

    fn make(&self, k: TokenKind) -> Token {
        let lex = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
        self.build_token(k, lex)
    }

    fn make_error(&self, msg: &str) -> LangError {
        // compute a best-effort column and the full line text
        let last_nl = self.src[..self.start].iter().rposition(|&b| b == b'\n');
        let line_start = match last_nl {
            Some(p) => p + 1,
            None => 0,
        };
        let col = self.start - line_start + 1; // 1-based
        let next_nl = self.src[self.current..].iter().position(|&b| b == b'\n');
        let line_end = match next_nl {
            Some(p) => self.current + p,
            None => self.src.len(),
        };
        let line_text =
            String::from_utf8(self.src[line_start..line_end].to_vec()).unwrap_or_default();
        let mut err = LangError::new(
            ErrorKind::Lexical,
            msg.to_string(),
            self.line,
            col,
            line_text,
        )
        .with_auto_hints();
        if let Some(f) = &self.file {
            err = err.with_file(f.clone());
        }
        err
    }

    fn build_token(&self, k: TokenKind, lexeme: String) -> Token {
        // compute column and the full line text for nicer diagnostics
        let last_nl = self.src[..self.start].iter().rposition(|&b| b == b'\n');
        let line_start = match last_nl {
            Some(p) => p + 1,
            None => 0,
        };
        let col = self.start - line_start + 1; // 1-based
        let next_nl = self.src[self.current..].iter().position(|&b| b == b'\n');
        let line_end = match next_nl {
            Some(p) => self.current + p,
            None => self.src.len(),
        };
        let line_text = String::from_utf8(self.src[line_start..line_end].to_vec()).unwrap();
        Token {
            kind: k,
            lexeme,
            line: self.line,
            col,
            line_text,
        }
    }

    fn scan(&mut self) -> Result<Option<Token>, LangError> {
        let c = self.adv();
        match c as char {
            '(' => Ok(Some(self.make(TokenKind::LeftParen))),
            ')' => Ok(Some(self.make(TokenKind::RightParen))),
            '{' => Ok(Some(self.make(TokenKind::LeftBrace))),
            '}' => Ok(Some(self.make(TokenKind::RightBrace))),
            '[' => Ok(Some(self.make(TokenKind::LeftBracket))),
            ']' => Ok(Some(self.make(TokenKind::RightBracket))),
            ',' => Ok(Some(self.make(TokenKind::Comma))),
            '.' => {
                // Handle .. and ... operators
                if self.matchc(b'.') {
                    if self.matchc(b'.') {
                        Ok(Some(self.make(TokenKind::DotDotDot))) // ... (spread/rest)
                    } else {
                        Ok(Some(self.make(TokenKind::DotDot))) // .. (range)
                    }
                } else {
                    Ok(Some(self.make(TokenKind::Dot)))
                }
            }
            ';' => Ok(Some(self.make(TokenKind::Semicolon))),
            ':' => Ok(Some(self.make(TokenKind::Colon))),
            '@' => Ok(Some(self.make(TokenKind::At))),
            '?' => {
                if self.matchc(b'.') {
                    Ok(Some(self.make(TokenKind::QuestionDot)))
                } else if self.matchc(b'?') {
                    if self.matchc(b'=') {
                        Ok(Some(self.make(TokenKind::NullCoalesceEqual)))
                    } else {
                        Ok(Some(self.make(TokenKind::NullCoalesce)))
                    }
                } else {
                    Ok(Some(self.make(TokenKind::Question)))
                }
            }
            '+' => {
                if self.matchc(b'+') {
                    Ok(Some(self.make(TokenKind::PlusPlus)))
                } else if self.matchc(b'=') {
                    Ok(Some(self.make(TokenKind::PlusEqual)))
                } else {
                    Ok(Some(self.make(TokenKind::Plus)))
                }
            }
            '-' => {
                if self.matchc(b'-') {
                    Ok(Some(self.make(TokenKind::MinusMinus)))
                } else if self.matchc(b'=') {
                    Ok(Some(self.make(TokenKind::MinusEqual)))
                } else {
                    Ok(Some(self.make(TokenKind::Minus)))
                }
            }
            '*' => {
                if self.matchc(b'*') {
                    if self.matchc(b'=') {
                        Ok(Some(self.make(TokenKind::StarStarEqual)))
                    } else {
                        Ok(Some(self.make(TokenKind::StarStar)))
                    }
                } else if self.matchc(b'=') {
                    Ok(Some(self.make(TokenKind::StarEqual)))
                } else {
                    Ok(Some(self.make(TokenKind::Star)))
                }
            }
            '/' => {
                if self.matchc(b'/') {
                    while self.peek() != b'\n' && !self.is_end() {
                        self.adv();
                    }
                    Ok(None)
                } else if self.matchc(b'*') {
                    let is_doc = self.matchc(b'*');
                    let mut content: Vec<u8> = Vec::new();
                    loop {
                        if self.is_end() {
                            return Err(self.make_error("Unterminated block comment"));
                        }
                        if self.peek() == b'\n' {
                            self.line += 1;
                        }
                        if self.peek() == b'*' && self.peek2() == b'/' {
                            self.adv();
                            self.adv();
                            break;
                        } else {
                            let b = self.adv();
                            if is_doc {
                                content.push(b);
                            }
                        }
                    }
                    if is_doc {
                        let s = String::from_utf8(content).unwrap_or_default();
                        let cleaned = s
                            .lines()
                            .map(|l| {
                                let t = l.trim_start();
                                if t.starts_with('*') {
                                    t[1..].trim_start().to_string()
                                } else {
                                    t.to_string()
                                }
                            })
                            .collect::<Vec<String>>()
                            .join("\n");
                        return Ok(Some(self.build_token(TokenKind::DocComment, cleaned)));
                    }
                    Ok(None)
                } else if self.matchc(b'=') {
                    Ok(Some(self.make(TokenKind::SlashEqual)))
                } else {
                    Ok(Some(self.make(TokenKind::Slash)))
                }
            }
            '%' => Ok(Some(self.make(TokenKind::Percent))),
            '!' => {
                let kind = if self.matchc(b'=') {
                    if self.matchc(b'=') {
                        TokenKind::StrictNotEqual
                    } else {
                        TokenKind::BangEqual
                    }
                } else {
                    TokenKind::Bang
                };
                Ok(Some(self.make(kind)))
            }
            '=' => {
                // support '=>' arrow for concise fn expressions, '==' equality, or '=' assignment
                let kind = if self.matchc(b'>') {
                    TokenKind::Arrow
                } else if self.matchc(b'=') {
                    if self.matchc(b'=') {
                        TokenKind::StrictEqual
                    } else {
                        TokenKind::EqualEqual
                    }
                } else {
                    TokenKind::Equal
                };
                Ok(Some(self.make(kind)))
            }
            '>' => {
                let kind = if self.matchc(b'>') {
                    if self.matchc(b'=') {
                        TokenKind::ShiftRightEqual
                    } else {
                        TokenKind::ShiftRight
                    }
                } else if self.matchc(b'=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                };
                Ok(Some(self.make(kind)))
            }
            '<' => {
                let kind = if self.matchc(b'<') {
                    if self.matchc(b'=') {
                        TokenKind::ShiftLeftEqual
                    } else {
                        TokenKind::ShiftLeft
                    }
                } else if self.matchc(b'=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                };
                Ok(Some(self.make(kind)))
            }
            '&' => {
                let kind = if self.matchc(b'&') {
                    TokenKind::AndAnd
                } else if self.matchc(b'=') {
                    TokenKind::AmpersandEqual
                } else {
                    TokenKind::Ampersand
                };
                Ok(Some(self.make(kind)))
            }
            '|' => {
                let kind = if self.matchc(b'|') {
                    TokenKind::OrOr
                } else if self.matchc(b'=') {
                    TokenKind::PipeEqual
                } else {
                    TokenKind::Pipe
                };
                Ok(Some(self.make(kind)))
            }
            '^' => {
                let kind = if self.matchc(b'=') {
                    TokenKind::CaretEqual
                } else {
                    TokenKind::Caret
                };
                Ok(Some(self.make(kind)))
            }
            '~' => {
                if self.matchc(b'/') {
                    Ok(Some(self.make(TokenKind::TildeSlash)))
                } else {
                    Ok(Some(self.make(TokenKind::Tilde)))
                }
            }
            '#' => {
                // Comment - skip to end of line
                while self.peek() != b'\n' && !self.is_end() {
                    self.adv();
                }
                Ok(None)
            }
            '$' => {
                // Check if this is $cImport directive
                if self.match_str("cImport") {
                    // Return a special token for $cImport
                    return Ok(Some(self.make(TokenKind::CImport)));
                }
                Err(self.make_error(&format!("Unexpected '$' at line {}", self.line)))
            }
            ' ' | '\r' | '\t' => Ok(None),
            '\n' => {
                self.line += 1;
                Ok(None)
            }
            '"' => self.string(),
            '`' => self.template(),
            '\'' => self.char_lit(),
            'r' | 'R' if self.peek() == b'"' || self.peek() == b'#' => self.raw_string(),
            c if c.is_ascii_digit() => self.number(),
            c if is_ident_start(c) => self.ident(),
            _ => Err(self.make_error(&format!("Unexpected '{}' at line {}", c as char, self.line))),
        }
    }

    fn string(&mut self) -> Result<Option<Token>, LangError> {
        let mut out: String = String::new();
        loop {
            if self.is_end() {
                return Err(self.make_error(&format!("Unterminated string line {}", self.line)));
            }
            if self.peek() == b'"' {
                self.adv();
                break;
            }
            if self.peek() == b'\\' {
                self.adv();
                if self.is_end() {
                    return Err(self.make_error("Unterminated escape in string"));
                }
                let e = self.adv() as char;
                match e {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'a' => out.push('\x07'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0C'),
                    'v' => out.push('\x0B'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '0' => out.push('\0'),
                    'x' => {
                        let h1 = self.peek();
                        let h2 = self.peek2();
                        if !((h1 as char).is_ascii_hexdigit() && (h2 as char).is_ascii_hexdigit()) {
                            return Err(self.make_error("Invalid \\x escape"));
                        }
                        self.adv();
                        self.adv();
                        let s = String::from_utf8(vec![h1, h2]).unwrap();
                        let val = u8::from_str_radix(&s, 16).unwrap();
                        out.push(val as char);
                    }
                    'u' => {
                        if self.peek() == b'{' {
                            self.adv();
                            let mut hex_bytes: Vec<u8> = Vec::new();
                            while !self.is_end() && self.peek() != b'}' {
                                let hb = self.adv();
                                hex_bytes.push(hb);
                            }
                            if self.is_end() || self.peek() != b'}' {
                                return Err(self.make_error("Invalid \\u{...} escape"));
                            }
                            self.adv();
                            let hs = String::from_utf8(hex_bytes).unwrap();
                            if hs.is_empty() || hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                                return Err(self.make_error("Invalid \\u{...} escape"));
                            }
                            let val = u32::from_str_radix(&hs, 16).unwrap();
                            if let Some(ch) = std::char::from_u32(val) {
                                out.push(ch);
                            } else {
                                return Err(self.make_error("Invalid Unicode code point"));
                            }
                        } else {
                            let mut hex: Vec<u8> = Vec::new();
                            for _ in 0..4 {
                                if !self.is_end() {
                                    let hb = self.adv();
                                    hex.push(hb);
                                } else {
                                    return Err(self.make_error("Invalid \\u escape"));
                                }
                            }
                            let hs = String::from_utf8(hex).unwrap();
                            if hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                                return Err(self.make_error("Invalid \\u escape"));
                            }
                            let val = u32::from_str_radix(&hs, 16).unwrap();
                            if let Some(ch) = std::char::from_u32(val) {
                                out.push(ch);
                            } else {
                                return Err(self.make_error("Invalid Unicode code point"));
                            }
                        }
                    }
                    other => {
                        out.push(other);
                    }
                }
            } else {
                if self.peek() == b'$' && self.peek2() == b'{' {
                    return Err(self.make_error(
                        "Template interpolation '${...}' only allowed in backtick strings",
                    ));
                }
                let rem = std::str::from_utf8(&self.src[self.current..])
                    .map_err(|_| self.make_error("Invalid UTF-8 in string"))?;
                if let Some(ch) = rem.chars().next() {
                    out.push(ch);
                    self.current += ch.len_utf8();
                    if ch == '\n' {
                        self.line += 1;
                    }
                } else {
                    return Err(self.make_error("Invalid UTF-8 in string"));
                }
            }
        }
        Ok(Some(self.build_token(TokenKind::String, out)))
    }

    fn raw_string(&mut self) -> Result<Option<Token>, LangError> {
        let mut hashes = 0;
        while self.peek() == b'#' {
            self.adv();
            hashes += 1;
        }
        if self.peek() != b'"' {
            return Err(self.make_error("Expect '\"' after raw string prefix"));
        }
        self.adv(); // Consume '"'
        let mut out = String::new();
        loop {
            if self.is_end() {
                return Err(self.make_error(&format!("Unterminated raw string line {}", self.line)));
            }
            if self.peek() == b'"' {
                let start_idx = self.current;
                self.adv(); // consume '"'
                let mut match_hashes = 0;
                while match_hashes < hashes && self.peek() == b'#' {
                    self.adv();
                    match_hashes += 1;
                }
                if match_hashes == hashes {
                    break;
                } else {
                    self.current = start_idx + 1;
                    out.push('"');
                }
            } else {
                let c = self.adv();
                if c == b'\n' {
                    self.line += 1;
                }
                out.push(c as char);
            }
        }
        Ok(Some(self.build_token(TokenKind::String, out)))
    }

    fn template(&mut self) -> Result<Option<Token>, LangError> {
        let mut out: String = String::new();
        loop {
            if self.is_end() {
                return Err(
                    self.make_error(&format!("Unterminated template string line {}", self.line))
                );
            }
            if self.peek() == b'`' {
                self.adv();
                break;
            }
            if self.peek() == b'\\' {
                self.adv();
                if self.is_end() {
                    return Err(self.make_error("Unterminated escape in template"));
                }
                let e = self.adv() as char;
                match e {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'a' => out.push('\x07'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0C'),
                    'v' => out.push('\x0B'),
                    '\\' => out.push('\\'),
                    '`' => out.push('`'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '0' => out.push('\0'),
                    'x' => {
                        let h1 = self.peek();
                        let h2 = self.peek2();
                        if !((h1 as char).is_ascii_hexdigit() && (h2 as char).is_ascii_hexdigit()) {
                            return Err(self.make_error("Invalid \\x escape"));
                        }
                        self.adv();
                        self.adv();
                        let s = String::from_utf8(vec![h1, h2]).unwrap();
                        let val = u8::from_str_radix(&s, 16).unwrap();
                        out.push(val as char);
                    }
                    'u' => {
                        if self.peek() == b'{' {
                            self.adv();
                            let mut hex_bytes: Vec<u8> = Vec::new();
                            while !self.is_end() && self.peek() != b'}' {
                                let hb = self.adv();
                                hex_bytes.push(hb);
                            }
                            if self.is_end() || self.peek() != b'}' {
                                return Err(self.make_error("Invalid \\u{...} escape"));
                            }
                            self.adv();
                            let hs = String::from_utf8(hex_bytes).unwrap();
                            if hs.is_empty() || hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                                return Err(self.make_error("Invalid \\u{...} escape"));
                            }
                            let val = u32::from_str_radix(&hs, 16).unwrap();
                            if let Some(ch) = std::char::from_u32(val) {
                                out.push(ch);
                            } else {
                                return Err(self.make_error("Invalid Unicode code point"));
                            }
                        } else {
                            let mut hex: Vec<u8> = Vec::new();
                            for _ in 0..4 {
                                if !self.is_end() {
                                    let hb = self.adv();
                                    hex.push(hb);
                                } else {
                                    return Err(self.make_error("Invalid \\u escape"));
                                }
                            }
                            let hs = String::from_utf8(hex).unwrap();
                            if hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                                return Err(self.make_error("Invalid \\u escape"));
                            }
                            let val = u32::from_str_radix(&hs, 16).unwrap();
                            if let Some(ch) = std::char::from_u32(val) {
                                out.push(ch);
                            } else {
                                return Err(self.make_error("Invalid Unicode code point"));
                            }
                        }
                    }
                    other => {
                        out.push(other);
                    }
                }
            } else {
                let rem = std::str::from_utf8(&self.src[self.current..])
                    .map_err(|_| self.make_error("Invalid UTF-8 in template"))?;
                if let Some(ch) = rem.chars().next() {
                    out.push(ch);
                    self.current += ch.len_utf8();
                    if ch == '\n' {
                        self.line += 1;
                    }
                } else {
                    return Err(self.make_error("Invalid UTF-8 in template"));
                }
            }
        }
        Ok(Some(self.build_token(TokenKind::Template, out)))
    }

    fn char_lit(&mut self) -> Result<Option<Token>, LangError> {
        if self.is_end() {
            return Err(self.make_error("Unterminated char"));
        }
        let value_char = if self.peek() == b'\\' {
            self.adv();
            let esc = self.adv() as char;
            match esc {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'x' => {
                    let h1 = self.peek();
                    let h2 = self.peek2();
                    if !((h1 as char).is_ascii_hexdigit() && (h2 as char).is_ascii_hexdigit()) {
                        return Err(self.make_error("Invalid \\x escape"));
                    }
                    self.adv();
                    self.adv();
                    let s = String::from_utf8(vec![h1, h2]).unwrap();
                    let val = u8::from_str_radix(&s, 16).unwrap();
                    val as char
                }
                'u' => {
                    if self.peek() == b'{' {
                        self.adv();
                        let mut hex_bytes: Vec<u8> = Vec::new();
                        while !self.is_end() && self.peek() != b'}' {
                            let hb = self.adv();
                            hex_bytes.push(hb);
                        }
                        if self.is_end() || self.peek() != b'}' {
                            return Err(self.make_error("Invalid \\u{...} escape"));
                        }
                        self.adv();
                        let hs = String::from_utf8(hex_bytes).unwrap();
                        if hs.is_empty() || hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                            return Err(self.make_error("Invalid \\u{...} escape"));
                        }
                        let val = u32::from_str_radix(&hs, 16).unwrap();
                        std::char::from_u32(val)
                            .ok_or_else(|| self.make_error("Invalid Unicode code point"))?
                    } else {
                        let mut hex: Vec<u8> = Vec::new();
                        for _ in 0..4 {
                            if !self.is_end() {
                                let hb = self.adv();
                                hex.push(hb);
                            } else {
                                return Err(self.make_error("Invalid \\u escape"));
                            }
                        }
                        let hs = String::from_utf8(hex).unwrap();
                        if hs.chars().any(|c| !c.is_ascii_hexdigit()) {
                            return Err(self.make_error("Invalid \\u escape"));
                        }
                        let val = u32::from_str_radix(&hs, 16).unwrap();
                        std::char::from_u32(val)
                            .ok_or_else(|| self.make_error("Invalid Unicode code point"))?
                    }
                }
                _ => esc,
            }
        } else {
            let rem = std::str::from_utf8(&self.src[self.current..])
                .map_err(|_| self.make_error("Invalid UTF-8 in char"))?;
            if let Some(ch) = rem.chars().next() {
                self.current += ch.len_utf8();
                ch
            } else {
                return Err(self.make_error("Invalid UTF-8 in char"));
            }
        };
        if self.peek() != b'\'' {
            return Err(self.make_error("Unterminated char"));
        }
        self.adv();
        Ok(Some(
            self.build_token(TokenKind::CharLit, value_char.to_string()),
        ))
    }

    fn number(&mut self) -> Result<Option<Token>, LangError> {
        // Check for special numeric literal formats: 0b (binary), 0o (octal), 0x (hex)
        let first_char = self.src[self.start];
        if first_char == b'0' && !self.is_end() {
            let next = self.peek();
            match next {
                b'b' | b'B' => {
                    // Binary literal: 0b[01_]+
                    self.adv(); // consume 'b'
                    let start_pos = self.current;
                    while matches!(self.peek(), b'0' | b'1' | b'_') {
                        self.adv();
                    }
                    if self.current == start_pos {
                        return Err(self.make_error("Invalid binary literal: no digits after 0b"));
                    }
                    // Check for invalid suffix (type suffixes allowed)
                    let s = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
                    return self.handle_numeric_suffix(s, false);
                }
                b'o' | b'O' => {
                    // Octal literal: 0o[0-7_]+
                    self.adv(); // consume 'o'
                    let start_pos = self.current;
                    while matches!(self.peek(), b'0'..=b'7' | b'_') {
                        self.adv();
                    }
                    if self.current == start_pos {
                        return Err(self.make_error("Invalid octal literal: no digits after 0o"));
                    }
                    // Validate no invalid octal digits (8 or 9)
                    let s = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
                    if s[2..].chars().any(|c| matches!(c, '8' | '9')) {
                        return Err(self.make_error("Invalid octal literal: contains digit 8 or 9"));
                    }
                    return self.handle_numeric_suffix(s, false);
                }
                b'x' | b'X' => {
                    // Hexadecimal literal: 0x[0-9a-fA-F_]+
                    self.adv(); // consume 'x'
                    let start_pos = self.current;
                    while matches!(self.peek(), b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' | b'_') {
                        self.adv();
                    }
                    if self.current == start_pos {
                        return Err(
                            self.make_error("Invalid hexadecimal literal: no digits after 0x")
                        );
                    }
                    let s = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
                    return self.handle_numeric_suffix(s, false);
                }
                _ => {
                    // Regular decimal number starting with 0
                }
            }
        }

        // Regular decimal number with optional underscore separators
        let mut has_dot = false;
        while matches!(self.peek(), b'0'..=b'9' | b'_') {
            self.adv();
        }
        if self.peek() == b'.' && self.peek2().is_ascii_digit() {
            has_dot = true;
            self.adv();
            while matches!(self.peek(), b'0'..=b'9' | b'_') {
                self.adv();
            }
        }

        // Handle scientific exponent notation: [eE][+-]?[0-9_]+
        if matches!(self.peek(), b'e' | b'E') {
            let next2 = self.peek2();
            let has_sign = matches!(next2, b'+' | b'-');
            let next3 = if has_sign {
                if self.current + 2 >= self.src.len() {
                    0
                } else {
                    self.src[self.current + 2]
                }
            } else {
                next2
            };
            if next3.is_ascii_digit() {
                has_dot = true; // Exponent implies a float
                self.adv(); // consume 'e'/'E'
                if has_sign {
                    self.adv(); // consume '+' or '-'
                }
                while matches!(self.peek(), b'0'..=b'9' | b'_') {
                    self.adv();
                }
            }
        }

        let s = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
        self.handle_numeric_suffix(s, has_dot)
    }

    fn handle_numeric_suffix(
        &mut self,
        num_str: String,
        has_dot: bool,
    ) -> Result<Option<Token>, LangError> {
        // Check for typed numeric literal suffixes
        let suffix_start = self.current;
        let peek_char = self.peek() as char;

        // Check for BigInt suffix 'n' (only for integers)
        if !has_dot && peek_char == 'n' && !is_ident_continue(self.peek2() as char) {
            self.adv();
            let s = format!("{}n", num_str);
            return Ok(Some(self.build_token(TokenKind::BigIntLit, s)));
        }

        // Imaginary suffix: `5j` or `2.5j`.  Keep it as one token so the
        // parser can construct a complex value without treating `j` as a
        // variable.
        if peek_char == 'j' && !is_ident_continue(self.peek2() as char) {
            self.adv();
            return Ok(Some(self.build_token(
                TokenKind::ComplexLit,
                format!("{}j", num_str),
            )));
        }

        // Check for typed integer suffixes: u8, u16, u32, u64, u128, i8, i16, i32, i64, i128
        // Check for typed float suffixes: f32, f64
        if peek_char == 'u' || peek_char == 'i' || peek_char == 'f' {
            // Collect potential suffix
            let mut suffix_chars: Vec<u8> = Vec::new();
            while is_ident_continue(self.peek() as char) {
                suffix_chars.push(self.adv());
            }
            let suffix = String::from_utf8(suffix_chars.clone()).unwrap();

            // Match known suffixes
            let token_kind = match suffix.as_str() {
                "u8" if !has_dot => Some(TokenKind::U8Lit),
                "u16" if !has_dot => Some(TokenKind::U16Lit),
                "u32" if !has_dot => Some(TokenKind::U32Lit),
                "u64" if !has_dot => Some(TokenKind::U64Lit),
                "u128" if !has_dot => Some(TokenKind::U128Lit),
                "i8" if !has_dot => Some(TokenKind::I8Lit),
                "i16" if !has_dot => Some(TokenKind::I16Lit),
                "i32" if !has_dot => Some(TokenKind::I32Lit),
                "i64" if !has_dot => Some(TokenKind::I64Lit),
                "i128" if !has_dot => Some(TokenKind::I128Lit),
                "f32" => Some(TokenKind::F32Lit),
                "f64" => Some(TokenKind::F64Lit),
                _ => None,
            };

            if let Some(kind) = token_kind {
                // Return the numeric value without the suffix in the lexeme
                return Ok(Some(self.build_token(kind, num_str)));
            } else {
                // Not a valid numeric suffix - rewind and treat as regular number
                self.current = suffix_start;
            }
        }

        Ok(Some(self.build_token(TokenKind::Number, num_str)))
    }

    fn ident(&mut self) -> Result<Option<Token>, LangError> {
        while is_ident_continue(self.peek() as char) {
            self.adv();
        }
        let s = String::from_utf8(self.src[self.start..self.current].to_vec()).unwrap();
        let kind = match s.as_str() {
            "type" => TokenKind::Type,
            "abstract" => TokenKind::Abstract,
            "sealed" => TokenKind::Sealed,
            "interface" => TokenKind::Interface,
            "implements" => TokenKind::Implements,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "elif" => TokenKind::Elif,
            "do" => TokenKind::Do,
            "while" => TokenKind::While,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "jump" => TokenKind::Jump,
            "extend" => TokenKind::Extend,
            "extends" => TokenKind::Extends,
            "extern" => TokenKind::Extern,
            "private" => TokenKind::Private,
            "protected" => TokenKind::Protected,
            "public" => TokenKind::Public,
            "super" => TokenKind::Super,
            "on" => TokenKind::On,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "of" => TokenKind::Of,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "instanceof" => TokenKind::Instanceof,
            "typeof" => TokenKind::Typeof,
            "class" => TokenKind::Class,
            "new" => TokenKind::New,
            "this" => TokenKind::This,
            "self" => TokenKind::SelfKeyword,
            "uint" => TokenKind::Uint,
            "int" => TokenKind::Int,
            "Int" => TokenKind::Int,
            "static" => TokenKind::Static,
            "constructor" => TokenKind::Constructor,
            "get" => TokenKind::Get,
            "set" => TokenKind::Set,
            "operator" => TokenKind::Operator,
            "import" => TokenKind::Import,
            "as" => TokenKind::As,
            "export" => TokenKind::Export,
            "from" => TokenKind::From,
            "default" => TokenKind::Default,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "throw" => TokenKind::Throw,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "spawn" => TokenKind::Spawn,
            "match" => TokenKind::Match,
            "decorator" => TokenKind::Decorator,
            "readonly" => TokenKind::Readonly,
            "raw" => TokenKind::Raw,
            "vec" => TokenKind::VecType,
            "region" => TokenKind::Region,
            "defer" => TokenKind::Defer,
            "unsafe" => TokenKind::Unsafe,
            "share" => TokenKind::Share,
            "strong" => TokenKind::Strong,
            "weak" => TokenKind::Weak,
            "alloc" => TokenKind::Alloc,
            "free" => TokenKind::Free,
            "compile" => TokenKind::Compile,
            "runtime" => TokenKind::Runtime,
            "typecheck" => TokenKind::Typecheck,
            "emit" => TokenKind::Emit,
            "require" => TokenKind::Require,
            "proceed" => TokenKind::Proceed,
            "test" => TokenKind::Test,
            "ignore" => TokenKind::Ignore,
            "expect_fail" => TokenKind::ExpectFail,
            "_" => TokenKind::Underscore,
            _ => TokenKind::Identifier,
        };
        Ok(Some(self.build_token(kind, s)))
    }

    // support '@' decorator symbol
    fn _punct(&mut self, c: u8) -> Result<Option<Token>, LangError> {
        match c as char {
            '@' => Ok(Some(self.make(TokenKind::Identifier))), // will treat '@' followed by ident as separate tokens
            _ => Ok(None),
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// ✅ Export both Token and Lexer publicly for use in parser/runtime

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_numbers_and_idents() {
        let src = "let x = 42; fn f() { return x + 1; }";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        // find some token kinds and lexemes
        let lexemes: Vec<_> = toks.iter().map(|t| t.lexeme.clone()).collect();
        assert!(lexemes.contains(&"let".to_string()));
        assert!(lexemes.contains(&"x".to_string()));
        assert!(lexemes.contains(&"42".to_string()));
        assert!(lexemes.contains(&"fn".to_string()));
    }

    #[test]
    fn tokenize_new_operators() {
        let src = "a?.b ?? c; x++ ; y-- ; 2 ** 3; 5 ~/ 2; a && b || c; a |= 1;";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::QuestionDot))
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::NullCoalesce))
        );
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::PlusPlus)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::MinusMinus)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::StarStar)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::TildeSlash)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::AndAnd)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::OrOr)));
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::PipeEqual)));
    }

    #[test]
    fn tokenize_string_literal() {
        let src = "print(\"Hello\")";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        // expect a string token with lexeme Hello
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::String) && t.lexeme == "Hello")
        );
    }

    #[test]
    fn tokenize_keywords() {
        let src = "if else while for fn let class return";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        let kinds: Vec<_> = toks.iter().map(|t| t.kind.clone()).collect();
        assert!(kinds.contains(&TokenKind::If));
        assert!(kinds.contains(&TokenKind::Else));
        assert!(kinds.contains(&TokenKind::While));
        assert!(kinds.contains(&TokenKind::For));
        assert!(kinds.contains(&TokenKind::Fn));
        assert!(kinds.contains(&TokenKind::Let));
        assert!(kinds.contains(&TokenKind::Class));
        assert!(kinds.contains(&TokenKind::Return));
    }

    #[test]
    fn tokenize_typed_numeric_literals() {
        // Test unsigned integer suffixes
        let src = "10u8 255u16 1024u32 65536u64 1u128";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U8Lit) && t.lexeme == "10")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U16Lit) && t.lexeme == "255")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U32Lit) && t.lexeme == "1024")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U64Lit) && t.lexeme == "65536")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U128Lit) && t.lexeme == "1")
        );

        // Test signed integer suffixes
        let src2 = "10i8 255i16 1024i32 65536i64 1i128";
        let mut lx2 = Lexer::new(src2);
        let toks2 = lx2.tokenize().expect("tokenize failed");
        assert!(
            toks2
                .iter()
                .any(|t| matches!(t.kind, TokenKind::I8Lit) && t.lexeme == "10")
        );
        assert!(
            toks2
                .iter()
                .any(|t| matches!(t.kind, TokenKind::I16Lit) && t.lexeme == "255")
        );
        assert!(
            toks2
                .iter()
                .any(|t| matches!(t.kind, TokenKind::I32Lit) && t.lexeme == "1024")
        );
        assert!(
            toks2
                .iter()
                .any(|t| matches!(t.kind, TokenKind::I64Lit) && t.lexeme == "65536")
        );
        assert!(
            toks2
                .iter()
                .any(|t| matches!(t.kind, TokenKind::I128Lit) && t.lexeme == "1")
        );

        // Test float suffixes
        let src3 = "3.14f32 2.718f64 42f32 100f64";
        let mut lx3 = Lexer::new(src3);
        let toks3 = lx3.tokenize().expect("tokenize failed");
        assert!(
            toks3
                .iter()
                .any(|t| matches!(t.kind, TokenKind::F32Lit) && t.lexeme == "3.14")
        );
        assert!(
            toks3
                .iter()
                .any(|t| matches!(t.kind, TokenKind::F64Lit) && t.lexeme == "2.718")
        );
        assert!(
            toks3
                .iter()
                .any(|t| matches!(t.kind, TokenKind::F32Lit) && t.lexeme == "42")
        );
        assert!(
            toks3
                .iter()
                .any(|t| matches!(t.kind, TokenKind::F64Lit) && t.lexeme == "100")
        );
    }

    #[test]
    fn tokenize_bigint_still_works() {
        let src = "100n";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::BigIntLit)));
    }
}
