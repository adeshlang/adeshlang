use crate::ecosystem::version::{SemVer, VersionReq};
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum ManifestValue {
    String(String),
    Number(String),
    Bool(bool),
    Identifier(String),
    Version(VersionReq),
    SemanticVersion(SemVer),
    Path(String),
    Url(String),
    Duration(String),
    Array(Vec<ManifestValue>),
    Object(BTreeMap<String, ManifestValue>),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BindingKind {
    Let,
    Const,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManifestItem {
    Field {
        key: String,
        value: ManifestValue,
    },
    Section(ManifestSection),
    Import(String),
    Binding {
        kind: BindingKind,
        name: String,
        value: ManifestValue,
    },
    Conditional {
        condition: String,
        then_branch: ManifestSection,
        else_branch: Option<ManifestSection>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestSection {
    pub name: String,
    pub items: Vec<ManifestItem>,
}

impl ManifestSection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            items: Vec::new(),
        }
    }

    pub fn field(&self, key: &str) -> Option<&ManifestValue> {
        self.items.iter().find_map(|item| match item {
            ManifestItem::Field {
                key: item_key,
                value,
            } if item_key == key => Some(value),
            _ => None,
        })
    }

    pub fn section(&self, name: &str) -> Option<&ManifestSection> {
        self.items.iter().find_map(|item| match item {
            ManifestItem::Section(section) if section.name == name => Some(section),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub path: Option<PathBuf>,
    pub root: ManifestSection,
}

impl Manifest {
    pub fn empty() -> Self {
        Self {
            path: None,
            root: ManifestSection::new("root"),
        }
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        let mut parser = Parser::new(input);
        Ok(Self {
            path: None,
            root: parser.parse_root()?,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let mut manifest = Self::parse(&source)?;
        manifest.path = Some(path.to_path_buf());
        Ok(manifest)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        fs::write(path, self.to_string()).map_err(|error| error.to_string())
    }

    pub fn sanitize_confidential(&self) -> (Self, BTreeMap<String, String>) {
        let mut confidential = BTreeMap::new();
        let root = sanitize_section(&self.root, "", &mut confidential);
        (
            Self {
                path: self.path.clone(),
                root,
            },
            confidential,
        )
    }

    pub fn section(&self, name: &str) -> Option<&ManifestSection> {
        self.root.section(name)
    }

    pub fn project_name(&self) -> Option<String> {
        self.section("project")
            .and_then(|section| match section.field("name") {
                Some(ManifestValue::String(value)) => Some(value.clone()),
                Some(ManifestValue::Identifier(value)) => Some(value.clone()),
                _ => None,
            })
    }

    pub fn project_version(&self) -> Option<SemVer> {
        self.section("project")
            .and_then(|section| match section.field("version") {
                Some(ManifestValue::SemanticVersion(value)) => Some(value.clone()),
                Some(ManifestValue::Version(req)) => match req {
                    VersionReq::Exact(version) => Some(version.clone()),
                    _ => None,
                },
                _ => None,
            })
    }

    pub fn dependencies(&self) -> Option<&ManifestSection> {
        self.section("dependencies")
    }

    pub fn workspace_members(&self) -> Option<Vec<String>> {
        self.section("workspace")
            .and_then(|section| match section.field("members") {
                Some(ManifestValue::Array(values)) => {
                    let mut members = Vec::new();
                    for val in values {
                        if let ManifestValue::String(s) | ManifestValue::Identifier(s) = val {
                            members.push(s.clone());
                        }
                    }
                    Some(members)
                }
                _ => None,
            })
    }

    pub fn project_libs(&self) -> Option<Vec<String>> {
        self.section("project")
            .and_then(|section| match section.field("libs") {
                Some(ManifestValue::Array(values)) => {
                    let mut libs = Vec::new();
                    for val in values {
                        if let ManifestValue::String(s) | ManifestValue::Identifier(s) = val {
                            libs.push(s.clone());
                        }
                    }
                    Some(libs)
                }
                _ => None,
            })
    }

    pub fn scripts(&self) -> Option<&ManifestSection> {
        self.section("scripts")
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.project_name().is_none() && self.section("workspace").is_none() {
            return Err(
                "manifest validation failed: missing project.name or workspace definition"
                    .to_string(),
            );
        }
        Ok(())
    }
}

impl Display for Manifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_section(f, &self.root, 0)
    }
}

fn write_section(
    f: &mut fmt::Formatter<'_>,
    section: &ManifestSection,
    indent: usize,
) -> fmt::Result {
    let pad = "  ".repeat(indent);
    if section.name != "root" {
        writeln!(f, "{}{} {{", pad, section.name)?;
    }
    let nested_indent = if section.name == "root" {
        indent
    } else {
        indent + 1
    };
    let nested_pad = "  ".repeat(nested_indent);
    for item in &section.items {
        match item {
            ManifestItem::Field { key, value } => writeln!(f, "{}{} = {}", nested_pad, key, value)?,
            ManifestItem::Section(child) => write_section(f, child, nested_indent)?,
            ManifestItem::Import(path) => writeln!(f, "{}import {}", nested_pad, quoted(path))?,
            ManifestItem::Binding { kind, name, value } => {
                let keyword = match kind {
                    BindingKind::Let => "let",
                    BindingKind::Const => "const",
                };
                writeln!(f, "{}{} {} = {}", nested_pad, keyword, name, value)?;
            }
            ManifestItem::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                writeln!(f, "{}if {} {{", nested_pad, condition)?;
                write_section(f, then_branch, nested_indent + 1)?;
                if let Some(else_branch) = else_branch {
                    writeln!(f, "{}}} else {{", nested_pad)?;
                    write_section(f, else_branch, nested_indent + 1)?;
                }
                writeln!(f, "{}}}", nested_pad)?;
            }
        }
    }
    if section.name != "root" {
        writeln!(f, "{}}}", pad)?;
    }
    Ok(())
}

struct Parser<'a> {
    input: &'a str,
    chars: Vec<char>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn parse_root(&mut self) -> Result<ManifestSection, String> {
        let mut root = ManifestSection::new("root");
        self.skip_ws_and_comments();
        while !self.eof() {
            // Support TOML-style [section] headers in addition to brace syntax
            if self.peek_char() == Some('[') {
                self.pos += 1; // consume '['
                let name = self.parse_identifier()?;
                self.skip_ws_and_comments();
                self.expect(']')?;
                let mut section = ManifestSection::new(name);
                self.skip_ws_and_comments();
                while !self.eof() && self.peek_char() != Some('[') {
                    section.items.push(self.parse_item()?);
                    self.skip_ws_and_comments();
                }
                root.items.push(ManifestItem::Section(section));
            } else {
                root.items.push(self.parse_item()?);
                self.skip_ws_and_comments();
            }
        }
        Ok(root)
    }

    fn parse_item(&mut self) -> Result<ManifestItem, String> {
        self.skip_ws_and_comments();
        if self.peek_keyword("import") {
            self.consume_keyword("import");
            return Ok(ManifestItem::Import(self.parse_scalar_string()?));
        }
        if self.peek_keyword("let") || self.peek_keyword("const") {
            let kind = if self.peek_keyword("let") {
                BindingKind::Let
            } else {
                BindingKind::Const
            };
            self.consume_keyword(match kind {
                BindingKind::Let => "let",
                BindingKind::Const => "const",
            });
            let name = self.parse_identifier()?;
            self.expect('=')?;
            let value = self.parse_value()?;
            return Ok(ManifestItem::Binding { kind, name, value });
        }
        if self.peek_keyword("if") {
            self.consume_keyword("if");
            let condition = self.parse_until_block();
            let then_branch = self.parse_braced_section("then")?;
            self.skip_ws_and_comments();
            let else_branch = if self.peek_keyword("else") {
                self.consume_keyword("else");
                Some(self.parse_braced_section("else")?)
            } else {
                None
            };
            return Ok(ManifestItem::Conditional {
                condition,
                then_branch,
                else_branch,
            });
        }

        let name = self.parse_identifier()?;
        self.skip_ws_and_comments();
        if self.peek_char() == Some('{') {
            Ok(ManifestItem::Section(self.parse_named_section(name)?))
        } else {
            self.expect('=')?;
            Ok(ManifestItem::Field {
                key: name,
                value: self.parse_value()?,
            })
        }
    }

    fn parse_named_section(&mut self, name: String) -> Result<ManifestSection, String> {
        self.expect('{')?;
        let mut section = ManifestSection::new(name);
        loop {
            self.skip_ws_and_comments();
            if self.peek_char() == Some('}') {
                self.pos += 1;
                break;
            }
            if self.eof() {
                return Err("unexpected end of manifest while parsing section".to_string());
            }
            section.items.push(self.parse_item()?);
        }
        Ok(section)
    }

    fn parse_braced_section(&mut self, name: &str) -> Result<ManifestSection, String> {
        self.skip_ws_and_comments();
        self.expect('{')?;
        let mut section = ManifestSection::new(name.to_string());
        loop {
            self.skip_ws_and_comments();
            if self.peek_char() == Some('}') {
                self.pos += 1;
                break;
            }
            if self.eof() {
                return Err(
                    "unexpected end of manifest while parsing conditional section".to_string(),
                );
            }
            section.items.push(self.parse_item()?);
        }
        Ok(section)
    }

    fn parse_value(&mut self) -> Result<ManifestValue, String> {
        self.skip_ws_and_comments();
        match self.peek_char() {
            Some('"') => Ok(ManifestValue::String(self.parse_string()?)),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some(ch) if ch.is_ascii_digit() || ch == '-' => self.parse_number_or_version(),
            Some(ch) if matches!(ch, '^' | '~' | '>' | '<' | '=') => {
                let token = self.parse_bare_token();
                VersionReq::parse(&token).map(ManifestValue::Version)
            }
            Some(_) => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Ok(ManifestValue::Bool(true)),
                    "false" => Ok(ManifestValue::Bool(false)),
                    "null" => Ok(ManifestValue::Null),
                    "version" => Ok(ManifestValue::Version(VersionReq::parse(
                        &self.parse_scalar_string()?,
                    )?)),
                    "path" => Ok(ManifestValue::Path(self.parse_scalar_string()?)),
                    "url" => Ok(ManifestValue::Url(self.parse_scalar_string()?)),
                    "duration" => Ok(ManifestValue::Duration(self.parse_scalar_string()?)),
                    _ => {
                        if let Ok(version) = VersionReq::parse(&ident) {
                            Ok(ManifestValue::Version(version))
                        } else if let Ok(version) = SemVer::parse(&ident) {
                            Ok(ManifestValue::SemanticVersion(version))
                        } else {
                            Ok(ManifestValue::Identifier(ident))
                        }
                    }
                }
            }
            None => Err("unexpected end of manifest while parsing value".to_string()),
        }
    }

    fn parse_number_or_version(&mut self) -> Result<ManifestValue, String> {
        let token = self.parse_bare_token();
        if token.contains('.')
            || token.contains('^')
            || token.contains('~')
            || token.contains('>')
            || token.contains('<')
        {
            if let Ok(version) = SemVer::parse(&token) {
                return Ok(ManifestValue::SemanticVersion(version));
            }
            if let Ok(req) = VersionReq::parse(&token) {
                return Ok(ManifestValue::Version(req));
            }
            if let Ok(version) = SemVer::parse(token.trim_start_matches('=')) {
                return Ok(ManifestValue::SemanticVersion(version));
            }
        }
        Ok(ManifestValue::Number(token))
    }

    fn parse_array(&mut self) -> Result<ManifestValue, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek_char() == Some(']') {
                self.pos += 1;
                break;
            }
            values.push(self.parse_value()?);
            self.skip_ws_and_comments();
            if self.peek_char() == Some(',') {
                self.pos += 1;
            }
        }
        Ok(ManifestValue::Array(values))
    }

    fn parse_object(&mut self) -> Result<ManifestValue, String> {
        self.expect('{')?;
        let mut values = BTreeMap::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek_char() == Some('}') {
                self.pos += 1;
                break;
            }
            let key = self.parse_identifier()?;
            self.skip_ws_and_comments();
            if self.peek_char() == Some(':') || self.peek_char() == Some('=') {
                self.pos += 1;
            } else {
                return Err("expected ':' or '=' in object literal".to_string());
            }
            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_ws_and_comments();
            if self.peek_char() == Some(',') {
                self.pos += 1;
            }
        }
        Ok(ManifestValue::Object(values))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.pos += 1;
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let escaped = self
                        .peek_char()
                        .ok_or_else(|| "unterminated string escape".to_string())?;
                    self.pos += 1;
                    out.push(match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        other => other,
                    });
                }
                other => out.push(other),
            }
        }
        Err("unterminated string literal".to_string())
    }

    fn parse_identifier(&mut self) -> Result<String, String> {
        self.skip_ws_and_comments();
        let mut ident = String::new();
        match self.peek_char() {
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {
                ident.push(ch);
                self.pos += 1;
            }
            Some(ch) => return Err(format!("expected identifier, found '{ch}'")),
            None => return Err("unexpected end of manifest while parsing identifier".to_string()),
        }
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ident.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(ident)
    }

    fn parse_bare_token(&mut self) -> String {
        self.skip_ws_and_comments();
        let mut token = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || matches!(ch, ',' | '}' | ']' | ';') {
                break;
            }
            token.push(ch);
            self.pos += 1;
        }
        token
    }

    fn parse_scalar_string(&mut self) -> Result<String, String> {
        self.skip_ws_and_comments();
        match self.peek_char() {
            Some('"') => self.parse_string(),
            Some(_) => Ok(self.parse_bare_token()),
            None => Err("unexpected end of manifest while parsing scalar".to_string()),
        }
    }

    fn parse_until_block(&mut self) -> String {
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '{' {
                break;
            }
            out.push(ch);
            self.pos += 1;
        }
        out.trim().to_string()
    }

    fn peek_keyword(&mut self, keyword: &str) -> bool {
        self.skip_ws_and_comments();
        let remaining = &self.input[self.byte_pos()..];
        remaining.starts_with(keyword)
            && remaining
                .chars()
                .nth(keyword.chars().count())
                .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
                .unwrap_or(true)
    }

    fn consume_keyword(&mut self, keyword: &str) {
        self.skip_ws_and_comments();
        for _ in keyword.chars() {
            self.pos += 1;
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while matches!(self.peek_char(), Some(ch) if ch.is_whitespace()) {
                self.pos += 1;
            }
            if self.peek_char() == Some('/') && self.peek_next_char() == Some('/') {
                self.pos += 2;
                while !self.eof() && self.peek_char() != Some('\n') {
                    self.pos += 1;
                }
                continue;
            }
            if self.peek_char() == Some('#') {
                self.pos += 1;
                while !self.eof() && self.peek_char() != Some('\n') {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        self.skip_ws_and_comments();
        match self.peek_char() {
            Some(ch) if ch == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(ch) => Err(format!("expected '{expected}', found '{ch}'")),
            None => Err(format!("expected '{expected}', found end of input")),
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next_char(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn byte_pos(&self) -> usize {
        self.chars[..self.pos].iter().map(|ch| ch.len_utf8()).sum()
    }
}

impl Display for ManifestValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestValue::String(value) => write!(f, "{}", quoted(value)),
            ManifestValue::Number(value) => write!(f, "{}", value),
            ManifestValue::Bool(value) => write!(f, "{}", value),
            ManifestValue::Identifier(value) => write!(f, "{}", value),
            ManifestValue::Version(value) => write!(f, "{}", value),
            ManifestValue::SemanticVersion(value) => write!(f, "{}", value),
            ManifestValue::Path(value) => write!(f, "path {}", quoted(value)),
            ManifestValue::Url(value) => write!(f, "url {}", quoted(value)),
            ManifestValue::Duration(value) => write!(f, "duration {}", quoted(value)),
            ManifestValue::Array(values) => write!(
                f,
                "[{}]",
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ManifestValue::Object(values) => write!(
                f,
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ManifestValue::Null => write!(f, "null"),
        }
    }
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn sanitize_section(
    section: &ManifestSection,
    prefix: &str,
    confidential: &mut BTreeMap<String, String>,
) -> ManifestSection {
    let mut sanitized = ManifestSection::new(section.name.clone());
    let current_prefix = if section.name == "root" {
        prefix.to_string()
    } else if prefix.is_empty() {
        section.name.clone()
    } else {
        format!("{}.{}", prefix, section.name)
    };

    for item in &section.items {
        match item {
            ManifestItem::Field { key, value } => {
                let full_key = if current_prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", current_prefix, key)
                };
                if is_confidential_key(&current_prefix, key) {
                    confidential.insert(full_key, value.to_plain_text());
                } else {
                    sanitized.items.push(ManifestItem::Field {
                        key: key.clone(),
                        value: value.clone(),
                    });
                }
            }
            ManifestItem::Section(child) => {
                let redacted = sanitize_section(child, &current_prefix, confidential);
                sanitized.items.push(ManifestItem::Section(redacted));
            }
            ManifestItem::Import(path) => sanitized.items.push(ManifestItem::Import(path.clone())),
            ManifestItem::Binding { kind, name, value } => {
                sanitized.items.push(ManifestItem::Binding {
                    kind: kind.clone(),
                    name: name.clone(),
                    value: value.clone(),
                })
            }
            ManifestItem::Conditional {
                condition,
                then_branch,
                else_branch,
            } => sanitized.items.push(ManifestItem::Conditional {
                condition: condition.clone(),
                then_branch: sanitize_section(then_branch, &current_prefix, confidential),
                else_branch: else_branch
                    .as_ref()
                    .map(|branch| sanitize_section(branch, &current_prefix, confidential)),
            }),
        }
    }

    sanitized
}

fn is_confidential_key(section_path: &str, key: &str) -> bool {
    let value = format!("{} {}", section_path, key).to_ascii_lowercase();
    let markers = [
        "token",
        "secret",
        "password",
        "passwd",
        "passphrase",
        "private_key",
        "private-key",
        "api_key",
        "api-key",
        "client_secret",
        "client-secret",
        "auth_token",
        "auth-token",
        "registry_token",
        "registry-token",
        "license_key",
        "license-key",
        "credential",
    ];
    markers.iter().any(|marker| value.contains(marker))
}

impl ManifestValue {
    fn to_plain_text(&self) -> String {
        match self {
            ManifestValue::String(value)
            | ManifestValue::Number(value)
            | ManifestValue::Identifier(value)
            | ManifestValue::Path(value)
            | ManifestValue::Url(value)
            | ManifestValue::Duration(value) => value.clone(),
            ManifestValue::Bool(value) => value.to_string(),
            ManifestValue::Version(value) => value.to_string(),
            ManifestValue::SemanticVersion(value) => value.to_string(),
            ManifestValue::Array(value) => value
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            ManifestValue::Object(value) => value
                .iter()
                .map(|(key, value)| format!("{}: {}", key, value))
                .collect::<Vec<_>>()
                .join(", "),
            ManifestValue::Null => "null".to_string(),
        }
    }
}
