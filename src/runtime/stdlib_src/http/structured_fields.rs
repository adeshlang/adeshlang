use std::collections::HashMap;

use super::errors::{HttpError, HttpErrorKind};

const DEFAULT_MAX_NESTING: usize = 8;
const DEFAULT_MAX_MEMBERS: usize = 256;
const DEFAULT_MAX_STRING_LEN: usize = 8192;
const DEFAULT_MAX_BYTE_SEQUENCE: usize = 8192;

#[derive(Debug, Clone, Copy)]
pub struct StructuredFieldLimits {
    pub max_nesting: usize,
    pub max_members: usize,
    pub max_string_len: usize,
    pub max_byte_sequence_len: usize,
}

impl Default for StructuredFieldLimits {
    fn default() -> Self {
        Self {
            max_nesting: DEFAULT_MAX_NESTING,
            max_members: DEFAULT_MAX_MEMBERS,
            max_string_len: DEFAULT_MAX_STRING_LEN,
            max_byte_sequence_len: DEFAULT_MAX_BYTE_SEQUENCE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StructuredValue {
    Item(StructuredItem),
    InnerList(StructuredInnerList),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StructuredItem {
    Integer(i64),
    Decimal(f64),
    String(String),
    Token(String),
    ByteSequence(Vec<u8>),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredInnerList {
    pub items: Vec<StructuredItem>,
    pub params: StructuredParameters,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredParameters {
    pub params: HashMap<String, StructuredItem>,
}

impl StructuredParameters {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: StructuredItem) {
        self.params.insert(key, value);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredList {
    pub items: Vec<(StructuredValue, StructuredParameters)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuredDictionary {
    pub entries: HashMap<String, (StructuredValue, StructuredParameters)>,
}

pub struct StructuredFields;

impl StructuredFields {
    pub fn parse_item(input: &str) -> Result<(StructuredItem, StructuredParameters), HttpError> {
        let limits = StructuredFieldLimits::default();
        parse_item_internal(input, &limits)
    }

    pub fn parse_list(input: &str) -> Result<StructuredList, HttpError> {
        let limits = StructuredFieldLimits::default();
        parse_list_internal(input, &limits)
    }

    pub fn parse_dictionary(input: &str) -> Result<StructuredDictionary, HttpError> {
        let limits = StructuredFieldLimits::default();
        parse_dictionary_internal(input, &limits)
    }

    pub fn encode_item(item: &StructuredItem) -> String {
        encode_item_internal(item)
    }

    pub fn encode_list(list: &StructuredList) -> String {
        let mut out = Vec::with_capacity(list.items.len());
        for (value, params) in &list.items {
            let mut encoded = match value {
                StructuredValue::Item(item) => encode_item_internal(item),
                StructuredValue::InnerList(inner) => encode_inner_list(inner),
            };
            encoded.push_str(&encode_params(params));
            out.push(encoded);
        }
        out.join(", ")
    }

    pub fn encode_dictionary(dict: &StructuredDictionary) -> String {
        let mut keys: Vec<&String> = dict.entries.keys().collect();
        keys.sort();

        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some((value, params)) = dict.entries.get(key) {
                let mut encoded = String::new();
                encoded.push_str(key);
                match value {
                    StructuredValue::Item(StructuredItem::Boolean(true))
                        if params.params.is_empty() => {}
                    StructuredValue::Item(item) => {
                        encoded.push('=');
                        encoded.push_str(&encode_item_internal(item));
                    }
                    StructuredValue::InnerList(inner) => {
                        encoded.push('=');
                        encoded.push_str(&encode_inner_list(inner));
                    }
                }
                encoded.push_str(&encode_params(params));
                out.push(encoded);
            }
        }

        out.join(", ")
    }
}

fn parse_item_internal(
    input: &str,
    limits: &StructuredFieldLimits,
) -> Result<(StructuredItem, StructuredParameters), HttpError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Empty structured item",
        ));
    }

    let mut parser = SfParser::new(trimmed, *limits);
    let item = parser.parse_item()?;
    let params = parser.parse_parameters()?;
    parser.skip_ows();
    if !parser.is_eof() {
        return Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Unexpected trailing characters in structured item",
        ));
    }
    Ok((item, params))
}

fn parse_list_internal(
    input: &str,
    limits: &StructuredFieldLimits,
) -> Result<StructuredList, HttpError> {
    let mut parser = SfParser::new(input.trim(), *limits);
    let mut items = Vec::new();

    parser.skip_ows();
    if parser.is_eof() {
        return Ok(StructuredList { items });
    }

    loop {
        if items.len() >= limits.max_members {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Structured list exceeds configured member limit",
            ));
        }

        let value = if parser.peek_char() == Some('(') {
            StructuredValue::InnerList(parser.parse_inner_list(0)?)
        } else {
            StructuredValue::Item(parser.parse_item()?)
        };

        let params = parser.parse_parameters()?;
        items.push((value, params));

        parser.skip_ows();
        if parser.is_eof() {
            break;
        }

        if parser.consume_if(',') {
            parser.skip_ows();
            if parser.is_eof() {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Trailing comma in structured list",
                ));
            }
            continue;
        }

        return Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Expected comma delimiter in structured list",
        ));
    }

    Ok(StructuredList { items })
}

fn parse_dictionary_internal(
    input: &str,
    limits: &StructuredFieldLimits,
) -> Result<StructuredDictionary, HttpError> {
    let mut parser = SfParser::new(input.trim(), *limits);
    let mut entries = HashMap::new();

    parser.skip_ows();
    if parser.is_eof() {
        return Ok(StructuredDictionary { entries });
    }

    loop {
        if entries.len() >= limits.max_members {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Structured dictionary exceeds configured member limit",
            ));
        }

        let key = parser.parse_key()?;
        let value = if parser.consume_if('=') {
            if parser.peek_char() == Some('(') {
                StructuredValue::InnerList(parser.parse_inner_list(0)?)
            } else {
                StructuredValue::Item(parser.parse_item()?)
            }
        } else {
            StructuredValue::Item(StructuredItem::Boolean(true))
        };

        let params = parser.parse_parameters()?;
        entries.insert(key, (value, params));

        parser.skip_ows();
        if parser.is_eof() {
            break;
        }

        if parser.consume_if(',') {
            parser.skip_ows();
            if parser.is_eof() {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Trailing comma in structured dictionary",
                ));
            }
            continue;
        }

        return Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Expected comma delimiter in structured dictionary",
        ));
    }

    Ok(StructuredDictionary { entries })
}

fn encode_item_internal(item: &StructuredItem) -> String {
    match item {
        StructuredItem::Integer(n) => n.to_string(),
        StructuredItem::Decimal(d) => {
            let mut encoded = format!("{:.3}", d);
            while encoded.contains('.') && encoded.ends_with('0') {
                encoded.pop();
            }
            if encoded.ends_with('.') {
                encoded.push('0');
            }
            encoded
        }
        StructuredItem::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
        StructuredItem::Token(t) => t.clone(),
        StructuredItem::ByteSequence(b) => format!(":{}:", base64_encode(b)),
        StructuredItem::Boolean(b) => {
            if *b {
                "?1".to_string()
            } else {
                "?0".to_string()
            }
        }
    }
}

fn encode_inner_list(inner: &StructuredInnerList) -> String {
    let mut items = Vec::with_capacity(inner.items.len());
    for item in &inner.items {
        items.push(encode_item_internal(item));
    }
    let mut encoded = format!("({})", items.join(" "));
    encoded.push_str(&encode_params(&inner.params));
    encoded
}

fn encode_params(params: &StructuredParameters) -> String {
    if params.params.is_empty() {
        return String::new();
    }

    let mut keys: Vec<&String> = params.params.keys().collect();
    keys.sort();

    let mut out = String::new();
    for key in keys {
        if let Some(value) = params.params.get(key) {
            out.push(';');
            out.push_str(key);
            if value != &StructuredItem::Boolean(true) {
                out.push('=');
                out.push_str(&encode_item_internal(value));
            }
        }
    }
    out
}

struct SfParser<'a> {
    data: &'a str,
    pos: usize,
    limits: StructuredFieldLimits,
}

impl<'a> SfParser<'a> {
    fn new(data: &'a str, limits: StructuredFieldLimits) -> Self {
        Self {
            data,
            pos: 0,
            limits,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.data[self.pos..].chars().next()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn consume_if(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.next_char();
            true
        } else {
            false
        }
    }

    fn skip_ows(&mut self) {
        while matches!(self.peek_char(), Some(' ' | '\t')) {
            self.next_char();
        }
    }

    fn parse_item(&mut self) -> Result<StructuredItem, HttpError> {
        match self.peek_char() {
            Some('"') => self.parse_string(),
            Some(':') => self.parse_byte_sequence(),
            Some('?') => self.parse_boolean(),
            Some('-' | '0'..='9') => self.parse_number(),
            Some(_) => self.parse_token(),
            None => Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected structured item, found end of input",
            )),
        }
    }

    fn parse_number(&mut self) -> Result<StructuredItem, HttpError> {
        let start = self.pos;
        if self.consume_if('-') && !matches!(self.peek_char(), Some('0'..='9')) {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Malformed number in structured field",
            ));
        }

        while matches!(self.peek_char(), Some('0'..='9')) {
            self.next_char();
        }

        let mut is_decimal = false;
        if self.consume_if('.') {
            is_decimal = true;
            let mut frac = 0;
            while matches!(self.peek_char(), Some('0'..='9')) {
                self.next_char();
                frac += 1;
                if frac > 3 {
                    return Err(HttpError::new(
                        HttpErrorKind::ParseError,
                        "Structured decimal supports at most 3 fractional digits",
                    ));
                }
            }
            if frac == 0 {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Malformed decimal in structured field",
                ));
            }
        }

        let raw = &self.data[start..self.pos];
        if is_decimal {
            let v = raw.parse::<f64>().map_err(|_| {
                HttpError::new(
                    HttpErrorKind::ParseError,
                    "Invalid decimal in structured field",
                )
            })?;
            Ok(StructuredItem::Decimal(v))
        } else {
            let v = raw.parse::<i64>().map_err(|_| {
                HttpError::new(
                    HttpErrorKind::ParseError,
                    "Integer overflow in structured field",
                )
            })?;
            Ok(StructuredItem::Integer(v))
        }
    }

    fn parse_string(&mut self) -> Result<StructuredItem, HttpError> {
        if !self.consume_if('"') {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected opening quote for structured string",
            ));
        }

        let mut out = String::new();
        while let Some(ch) = self.next_char() {
            match ch {
                '"' => {
                    if out.len() > self.limits.max_string_len {
                        return Err(HttpError::new(
                            HttpErrorKind::ParseError,
                            "Structured string exceeds configured length limit",
                        ));
                    }
                    return Ok(StructuredItem::String(out));
                }
                '\\' => {
                    let escaped = self.next_char().ok_or_else(|| {
                        HttpError::new(
                            HttpErrorKind::ParseError,
                            "Malformed escape sequence in structured string",
                        )
                    })?;
                    if escaped != '\\' && escaped != '"' {
                        return Err(HttpError::new(
                            HttpErrorKind::ParseError,
                            "Invalid escape in structured string",
                        ));
                    }
                    out.push(escaped);
                }
                c if c.is_control() => {
                    return Err(HttpError::new(
                        HttpErrorKind::ParseError,
                        "Control character is not allowed in structured string",
                    ));
                }
                c => {
                    out.push(c);
                    if out.len() > self.limits.max_string_len {
                        return Err(HttpError::new(
                            HttpErrorKind::ParseError,
                            "Structured string exceeds configured length limit",
                        ));
                    }
                }
            }
        }

        Err(HttpError::new(
            HttpErrorKind::ParseError,
            "Unterminated structured string",
        ))
    }

    fn parse_byte_sequence(&mut self) -> Result<StructuredItem, HttpError> {
        if !self.consume_if(':') {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected ':' for structured byte sequence",
            ));
        }

        let start = self.pos;
        while !self.is_eof() && self.peek_char() != Some(':') {
            self.next_char();
        }

        if !self.consume_if(':') {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Unterminated structured byte sequence",
            ));
        }

        let raw = &self.data[start..self.pos - 1];
        let bytes = base64_decode(raw)?;
        if bytes.len() > self.limits.max_byte_sequence_len {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Structured byte sequence exceeds configured size limit",
            ));
        }
        Ok(StructuredItem::ByteSequence(bytes))
    }

    fn parse_boolean(&mut self) -> Result<StructuredItem, HttpError> {
        if !self.consume_if('?') {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected '?' for structured boolean",
            ));
        }
        match self.next_char() {
            Some('1') => Ok(StructuredItem::Boolean(true)),
            Some('0') => Ok(StructuredItem::Boolean(false)),
            _ => Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Invalid structured boolean value",
            )),
        }
    }

    fn parse_token(&mut self) -> Result<StructuredItem, HttpError> {
        let token = self.parse_bare_token()?;
        Ok(StructuredItem::Token(token))
    }

    fn parse_bare_token(&mut self) -> Result<String, HttpError> {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if is_token_char(ch) {
                self.next_char();
            } else {
                break;
            }
        }

        if self.pos == start {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected token in structured field",
            ));
        }

        Ok(self.data[start..self.pos].to_string())
    }

    fn parse_parameters(&mut self) -> Result<StructuredParameters, HttpError> {
        let mut params = StructuredParameters::new();

        loop {
            self.skip_ows();
            if !self.consume_if(';') {
                break;
            }

            let key = self.parse_key()?;
            let value = if self.consume_if('=') {
                self.parse_item()?
            } else {
                StructuredItem::Boolean(true)
            };
            params.insert(key, value);

            if params.params.len() > self.limits.max_members {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Structured parameters exceed configured member limit",
                ));
            }
        }

        Ok(params)
    }

    fn parse_key(&mut self) -> Result<String, HttpError> {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if is_key_char(ch) {
                self.next_char();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected key in structured field",
            ));
        }
        Ok(self.data[start..self.pos].to_string())
    }

    fn parse_inner_list(&mut self, depth: usize) -> Result<StructuredInnerList, HttpError> {
        if depth >= self.limits.max_nesting {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Structured inner-list nesting limit exceeded",
            ));
        }

        if !self.consume_if('(') {
            return Err(HttpError::new(
                HttpErrorKind::ParseError,
                "Expected '(' for structured inner-list",
            ));
        }

        let mut items = Vec::new();
        loop {
            self.skip_ows();
            if self.consume_if(')') {
                break;
            }
            let item = self.parse_item()?;
            items.push(item);
            if items.len() > self.limits.max_members {
                return Err(HttpError::new(
                    HttpErrorKind::ParseError,
                    "Structured inner-list exceeds configured member limit",
                ));
            }
            self.skip_ows();
            if self.peek_char() == Some(')') {
                self.next_char();
                break;
            }
        }

        let params = self.parse_parameters()?;
        Ok(StructuredInnerList { items, params })
    }
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '*'
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/' | '*')
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(data: &str) -> Result<Vec<u8>, HttpError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| {
            HttpError::new(
                HttpErrorKind::ParseError,
                format!("Invalid base64 in byte sequence: {}", e),
            )
        })
}
