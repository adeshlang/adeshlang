//! Portable string formatting engine for AdeshLang.
//!
//! This crate provides a single, backend-independent formatting implementation
//! used by the interpreter, VM v1/v2, JIT, native JIT, AOT, and WASM backends.
//! Every runtime path must evaluate an expression once, convert the resulting
//! Adesh value, apply the same specification, and return a string.
//!
//! # Format specification grammar
//!
//! ```text
//! spec := [fill] align? width? sign? precision? type
//! fill := <any single character>
//! align := '<' | '>' | '^'
//! width := <positive integer>
//! sign := '+' | ' '
//! precision := '.' <positive integer> | '(' <positive integer> ')'
//! type := 'int' | 'float' | 'bin' | 'hex' | 'HEX' | 'oct' | 'e' | 'E'
//!        | '%' | 'percent' | 'currency(CODE)' | 's' | 'json' | '?' | 'debug'
//!        | 'upper' | 'lower' | 'quoted' | ',' | '_'
//! ```
//!
//! Examples:
//! - `${x}`          — default conversion
//! - `${x:<12}`      — left align, width 12
//! - `${x:^12}`      — center align, width 12
//! - `${x:>12}`      — right align, width 12
//! - `${x:08}`       — zero-pad to width 8
//! - `${x:+}`        — explicit plus sign
//! - `${x:int}`      — integer conversion
//! - `${x:float(2)}` — fixed decimal with 2 places
//! - `${x:hex}`      — lowercase hex
//! - `${x:HEX}`      — uppercase hex
//! - `${x:bin}`      — binary
//! - `${x:oct}`      — octal
//! - `${x:e}` / `${x:E}` — scientific notation
//! - `${x:%}` / `${x:percent}` — percentage
//! - `${x:currency(INR)}` — currency with symbol and grouping
//! - `${x:upper}` / `${x:lower}` — case conversion
//! - `${x:quoted}`   — quoted/debug string
//! - `${x:json}`     — JSON representation
//! - `${x:,}` / `${x:_}` — grouping separators

use num_bigint::BigInt;
use num_traits::ToPrimitive;
use serde_json::Value as JsonValue;
use unicode_width::UnicodeWidthStr;

/// Error returned when a format specification is invalid or cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    pub message: String,
}

impl FormatError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "format error: {}", self.message)
    }
}

impl std::error::Error for FormatError {}

/// A parsed format specification, ready to be rendered against a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSpec {
    /// Fill character (defaults to space)
    pub fill: char,
    /// Alignment: '<' left, '>' right, '^' center, '\0' none
    pub align: char,
    /// Minimum display width (in Unicode columns)
    pub width: Option<usize>,
    /// Sign mode: '+' explicit plus, ' ' space for positive, '\0' none
    pub sign: char,
    /// Precision: Some(n) for fixed decimals, None for default
    pub precision: Option<usize>,
    /// Type conversion: "int", "float", "bin", "hex", "HEX", "oct", "e", "E",
    /// "%", "percent", "currency(CODE)", "s", "json", "debug", "?", "upper",
    /// "lower", "quoted", ",", "_", or "" for default
    pub type_spec: String,
    /// Zero-pad mode: when true, width pads with '0' after sign
    pub zero_pad: bool,
    /// Grouping separator: ',' or '_' or '\0' none
    pub grouping: char,
}

impl FormatSpec {
    /// Parse a format specification string. Returns an error if the spec is
    /// malformed or contains unsupported combinations.
    pub fn parse(s: &str) -> Result<Self, FormatError> {
        if s.is_empty() {
            return Ok(Self::default());
        }

        let chars: Vec<char> = s.chars().collect();
        let mut idx = 0;

        // --- fill character (optional, before alignment) ---
        let mut fill = ' ';
        let mut align = '\0';
        if idx < chars.len() && matches!(chars[idx], '<' | '>' | '^') {
            align = chars[idx];
            idx += 1;
        } else if idx + 1 < chars.len() && matches!(chars[idx + 1], '<' | '>' | '^') {
            fill = chars[idx];
            align = chars[idx + 1];
            idx += 2;
        }

        // --- width (optional digits) ---
        let mut width: Option<usize> = None;
        let mut zero_pad = false;
        let mut width_str = String::new();
        while idx < chars.len() && chars[idx].is_ascii_digit() {
            width_str.push(chars[idx]);
            idx += 1;
        }
        if !width_str.is_empty() {
            width = Some(
                width_str
                    .parse::<usize>()
                    .map_err(|e| FormatError::new(format!("invalid width: {e}")))?,
            );
            // Leading zero implies zero-pad with right alignment
            if width_str.starts_with('0') && align == '\0' {
                zero_pad = true;
                align = '>';
            }
        }

        // --- sign ---
        let mut sign = '\0';
        if idx < chars.len() && matches!(chars[idx], '+' | ' ') {
            sign = chars[idx];
            idx += 1;
        }

        // --- precision: ".N" or "(N)" ---
        let mut precision: Option<usize> = None;
        if idx < chars.len() && chars[idx] == '.' {
            idx += 1;
            let mut prec_str = String::new();
            while idx < chars.len() && chars[idx].is_ascii_digit() {
                prec_str.push(chars[idx]);
                idx += 1;
            }
            if prec_str.is_empty() {
                return Err(FormatError::new("expected digits after '.'"));
            }
            precision = Some(
                prec_str
                    .parse::<usize>()
                    .map_err(|e| FormatError::new(format!("invalid precision: {e}")))?,
            );
        } else if idx < chars.len() && chars[idx] == '(' {
            // Parenthesized precision can be standalone, e.g. "(4)", or
            // follow a type name, e.g. "float(2)". Both are valid.
            idx += 1;
            let mut prec_str = String::new();
            while idx < chars.len() && chars[idx].is_ascii_digit() {
                prec_str.push(chars[idx]);
                idx += 1;
            }
            if idx >= chars.len() || chars[idx] != ')' {
                return Err(FormatError::new("expected ')' after precision"));
            }
            idx += 1;
            if prec_str.is_empty() {
                return Err(FormatError::new(
                    "expected digits inside precision parentheses",
                ));
            }
            precision = Some(
                prec_str
                    .parse::<usize>()
                    .map_err(|e| FormatError::new(format!("invalid precision: {e}")))?,
            );
        }

        // --- grouping separator ---
        let mut grouping = '\0';
        if idx < chars.len() && matches!(chars[idx], ',' | '_') {
            grouping = chars[idx];
            idx += 1;
        }

        // --- type spec (rest of the string) ---
        let type_spec: String = chars[idx..].iter().collect();

        // Post-process type specs of the form "name(N)" where N is digits:
        // extract the precision from the parentheses and keep the type name.
        // e.g. "float(2)" -> type="float", precision=Some(2)
        //      "percent(0)" -> type="percent", precision=Some(0)
        //      "currency(USD)" -> unchanged (non-numeric inside parens)
        let mut final_type_spec = type_spec.clone();
        if let Some(paren_start) = type_spec.find('(') {
            if let Some(paren_end) = type_spec[paren_start + 1..].find(')') {
                let inner = &type_spec[paren_start + 1..paren_start + 1 + paren_end];
                if inner.chars().all(|c| c.is_ascii_digit()) && !inner.is_empty() {
                    let prec: usize = inner.parse().unwrap();
                    precision = Some(prec);
                    final_type_spec = type_spec[..paren_start].to_string();
                }
            }
        }

        // If the entire spec was just a grouping separator ("," or "_") with no
        // other formatting, treat it as a type spec rather than a grouping modifier.
        if type_spec.is_empty()
            && grouping != '\0'
            && width.is_none()
            && align == '\0'
            && sign == '\0'
            && precision.is_none()
            && !zero_pad
        {
            final_type_spec = grouping.to_string();
            grouping = '\0';
        }
        for c in final_type_spec.chars() {
            if c.is_ascii_whitespace() {
                return Err(FormatError::new(format!(
                    "unexpected whitespace in type spec: '{c}'"
                )));
            }
        }

        Ok(FormatSpec {
            fill,
            align,
            width,
            sign,
            precision,
            type_spec: final_type_spec,
            zero_pad,
            grouping,
        })
    }

    /// Returns true if this spec has no formatting at all (default conversion).
    pub fn is_default(&self) -> bool {
        self.fill == ' '
            && self.align == '\0'
            && self.width.is_none()
            && self.sign == '\0'
            && self.precision.is_none()
            && self.type_spec.is_empty()
            && !self.zero_pad
            && self.grouping == '\0'
    }
}

impl Default for FormatSpec {
    fn default() -> Self {
        Self {
            fill: ' ',
            align: '\0',
            width: None,
            sign: '\0',
            precision: None,
            type_spec: String::new(),
            zero_pad: false,
            grouping: '\0',
        }
    }
}

/// A runtime Adesh value that can be formatted. This mirrors the relevant
/// variants of the AST `Value` enum but is kept independent so the crate has
/// no dependency on the compiler crate.
#[derive(Debug, Clone)]
pub enum FormatValue {
    Str(String),
    Char(char),
    Bool(bool),
    Null,
    Number(f64),
    BigInt(BigInt),
    Int(i128),
    UInt(u128),
    Array(Vec<FormatValue>),
    Object(Vec<(String, FormatValue)>),
    Tuple(Vec<FormatValue>),
    Other(String),
}

impl FormatValue {
    /// Render this value with the given format specification.
    pub fn render(&self, spec: &FormatSpec) -> Result<String, FormatError> {
        if spec.is_default() {
            return Ok(self.default_string());
        }

        // Apply type conversion first
        let converted = self.apply_type(spec)?;

        // Then apply alignment/width/sign/grouping
        Ok(apply_formatting(&converted, spec))
    }

    /// Default string representation (no spec).
    fn default_string(&self) -> String {
        match self {
            FormatValue::Str(s) => s.clone(),
            FormatValue::Char(c) => c.to_string(),
            FormatValue::Bool(b) => b.to_string(),
            FormatValue::Null => "null".to_string(),
            FormatValue::Number(n) => {
                if (*n).fract() == 0.0 && (*n).abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            FormatValue::BigInt(bi) => bi.to_string(),
            FormatValue::Int(i) => i.to_string(),
            FormatValue::UInt(u) => u.to_string(),
            FormatValue::Array(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.default_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            FormatValue::Object(entries) => {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.default_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            FormatValue::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.default_string()).collect();
                format!("({})", parts.join(", "))
            }
            FormatValue::Other(s) => s.clone(),
        }
    }

    /// Apply type conversion based on the spec's type field.
    fn apply_type(&self, spec: &FormatSpec) -> Result<String, FormatError> {
        let ts = spec.type_spec.as_str();

        match ts {
            "" | "s" => Ok(self.default_string()),
            "int" => {
                let n = self
                    .to_i128()
                    .ok_or_else(|| FormatError::new(format!("cannot convert {:?} to int", self)))?;
                Ok(format_int(n, spec))
            }
            "d" => {
                let n = self
                    .to_i128()
                    .ok_or_else(|| FormatError::new(format!("cannot convert {:?} to int", self)))?;
                Ok(format_int(n, spec))
            }
            "float" => {
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to float", self))
                })?;
                let p = spec.precision.unwrap_or(2);
                Ok(format_float(f, p, spec))
            }
            "bin" => {
                let n = self.to_i128().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to binary", self))
                })?;
                Ok(format_int_base(n, 2, false, spec))
            }
            "hex" => {
                let n = self
                    .to_i128()
                    .ok_or_else(|| FormatError::new(format!("cannot convert {:?} to hex", self)))?;
                Ok(format_int_base(n, 16, false, spec))
            }
            "HEX" => {
                let n = self.to_i128().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to uppercase hex", self))
                })?;
                Ok(format_int_base(n, 16, true, spec))
            }
            "oct" => {
                let n = self.to_i128().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to octal", self))
                })?;
                Ok(format_int_base(n, 8, false, spec))
            }
            "e" => {
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to scientific", self))
                })?;
                let p = spec.precision.unwrap_or(3);
                Ok(format_scientific(f, p, false, spec))
            }
            "E" => {
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to scientific", self))
                })?;
                let p = spec.precision.unwrap_or(3);
                Ok(format_scientific(f, p, true, spec))
            }
            "%" | "percent" => {
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to percent", self))
                })?;
                let p = spec.precision.unwrap_or(1);
                Ok(format_percent(f * 100.0, p, spec))
            }
            "json" => Ok(format_json(self)),
            "debug" | "?" => Ok(format_debug(self)),
            "quoted" => match self {
                FormatValue::Str(s) => Ok(format!("{:?}", s)),
                _ => Ok(self.default_string()),
            },
            "upper" => Ok(self.default_string().to_uppercase()),
            "lower" => Ok(self.default_string().to_lowercase()),
            s if s.starts_with("currency(") && s.ends_with(')') => {
                let code = &s[9..s.len() - 1];
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} to currency", self))
                })?;
                Ok(format_currency(f, code, spec))
            }
            "," | "_" => {
                let f = self.to_f64().ok_or_else(|| {
                    FormatError::new(format!("cannot convert {:?} for grouping", self))
                })?;
                let sep = if spec.grouping != '\0' {
                    spec.grouping
                } else {
                    ts.chars().next().unwrap_or(',')
                };
                Ok(format_grouped(f, sep, spec))
            }
            _ => Err(FormatError::new(format!("unsupported format type: '{ts}'"))),
        }
    }

    /// Convert this value to i128 if possible.
    fn to_i128(&self) -> Option<i128> {
        match self {
            FormatValue::Int(i) => Some(*i),
            FormatValue::UInt(u) => u.to_i128(),
            FormatValue::Number(n) if n.fract() == 0.0 && n.abs() <= (i128::MAX as f64) => {
                Some(*n as i128)
            }
            FormatValue::BigInt(bi) => bi.to_i128(),
            FormatValue::Bool(b) => Some(if *b { 1 } else { 0 }),
            FormatValue::Char(c) => Some(*c as i128),
            FormatValue::Str(s) => s.parse::<i128>().ok(),
            _ => None,
        }
    }

    /// Convert this value to f64 if possible.
    fn to_f64(&self) -> Option<f64> {
        match self {
            FormatValue::Number(n) => Some(*n),
            FormatValue::Int(i) => i.to_f64(),
            FormatValue::UInt(u) => u.to_f64(),
            FormatValue::BigInt(bi) => bi.to_f64(),
            FormatValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            FormatValue::Str(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
}

/// Render the formatted string with alignment, width, sign, and grouping.
fn apply_formatting(s: &str, spec: &FormatSpec) -> String {
    let mut result = s.to_string();

    // Apply sign prefix before padding
    if spec.sign != '\0' && !result.starts_with('-') {
        if spec.sign == '+' {
            result.insert(0, '+');
        } else if spec.sign == ' ' {
            result.insert(0, ' ');
        }
    }

    // Apply grouping separator (already handled in type conversion, but keep
    // for standalone grouping specs like "," or "_")

    // Apply width / alignment / zero-pad
    if let Some(width) = spec.width {
        let display_width = UnicodeWidthStr::width(result.as_str());
        if display_width < width {
            let pad = width - display_width;
            let pad_char = if spec.zero_pad { '0' } else { spec.fill };
            match spec.align {
                '<' => {
                    result.push_str(&pad_char.to_string().repeat(pad));
                }
                '^' => {
                    let left = pad / 2;
                    let right = pad - left;
                    result = format!(
                        "{}{}{}",
                        pad_char.to_string().repeat(left),
                        result,
                        pad_char.to_string().repeat(right)
                    );
                }
                '>' | '\0' => {
                    // Zero-pad inserts after sign, before digits
                    if spec.zero_pad
                        && (result.starts_with('-')
                            || result.starts_with('+')
                            || result.starts_with(' '))
                    {
                        let sign = result.remove(0);
                        result = format!("{}{}{}", sign, pad_char.to_string().repeat(pad), result);
                    } else {
                        result = format!("{}{}", pad_char.to_string().repeat(pad), result);
                    }
                }
                _ => {}
            }
        }
    }

    result
}

/// Format an integer with optional sign and grouping.
fn format_int(n: i128, spec: &FormatSpec) -> String {
    let mag = n.abs();
    let s = format_int_digits(mag as u128, spec.grouping);
    if n < 0 { format!("-{s}") } else { s }
}

/// Format integer digits with optional grouping separator.
fn format_int_digits(n: u128, grouping: char) -> String {
    if grouping == '\0' {
        return n.to_string();
    }
    let digits = n.to_string();
    let mut result = String::new();
    let len = digits.len();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(grouping);
        }
        result.push(c);
    }
    result
}

/// Format an integer in a given base (2, 8, 16).
fn format_int_base(n: i128, base: u32, uppercase: bool, _spec: &FormatSpec) -> String {
    let mag = n.unsigned_abs();
    let s = if base == 16 {
        if uppercase {
            format!("{:X}", mag)
        } else {
            format!("{:x}", mag)
        }
    } else if base == 8 {
        format!("{:o}", mag)
    } else if base == 2 {
        format!("{:b}", mag)
    } else {
        mag.to_string()
    };
    if n < 0 { format!("-{s}") } else { s }
}

/// Format a float with fixed precision.
fn format_float(f: f64, precision: usize, _spec: &FormatSpec) -> String {
    let mag = f.abs();
    let rounded = (mag * 10f64.powi(precision as i32)).round() / 10f64.powi(precision as i32);
    let s = format!("{:.precision$}", rounded, precision = precision);
    if f < 0.0 { format!("-{s}") } else { s }
}

/// Format in scientific notation.
fn format_scientific(f: f64, precision: usize, uppercase: bool, _spec: &FormatSpec) -> String {
    let e_char = if uppercase { 'E' } else { 'e' };
    let mag = f.abs();

    // Compute exponent: normalize to m in [1, 10)
    let exp = if mag == 0.0 {
        0
    } else {
        mag.log10().floor() as i32
    };
    let normalized = mag / 10f64.powi(exp);
    let mantissa_str = format!("{normalized:.precision$}", precision = precision);

    // Format exponent without leading + and with minimal digits
    let exp_str = if exp >= 0 {
        format!("{exp}")
    } else {
        format!("-{exp_abs}", exp_abs = -exp)
    };

    let body = format!("{mantissa_str}{e_char}{exp_str}");
    if f < 0.0 { format!("-{body}") } else { body }
}

/// Format as percentage.
fn format_percent(f: f64, precision: usize, _spec: &FormatSpec) -> String {
    let mag = f.abs();
    let rounded = (mag * 10f64.powi(precision as i32)).round() / 10f64.powi(precision as i32);
    let s = format!("{:.precision$}", rounded, precision = precision);
    if f < 0.0 {
        format!("-{s}%")
    } else {
        format!("{s}%")
    }
}

/// Format as currency.
fn format_currency(f: f64, code: &str, spec: &FormatSpec) -> String {
    let (symbol, decimals, use_comma) = currency_info(code);
    let p = spec.precision.unwrap_or(decimals);
    let mag = f.abs();
    let rounded = (mag * 10f64.powi(p as i32)).round() / 10f64.powi(p as i32);
    let num_str = format!("{:.precision$}", rounded, precision = p);

    let formatted = if use_comma {
        add_grouping(&num_str, ',')
    } else {
        num_str
    };

    if f < 0.0 {
        format!("-{symbol}{formatted}")
    } else {
        format!("{symbol}{formatted}")
    }
}

/// Get currency symbol, decimal places, and comma usage.
fn currency_info(code: &str) -> (&'static str, usize, bool) {
    match code.to_ascii_uppercase().as_str() {
        "USD" => ("$", 2, true),
        "EUR" => ("€", 2, true),
        "GBP" => ("£", 2, true),
        "INR" => ("₹", 2, true),
        "JPY" => ("¥", 0, true),
        "CNY" => ("¥", 2, true),
        "KRW" => ("₩", 0, true),
        "RUB" => ("₽", 2, true),
        "BRL" => ("R$", 2, true),
        "CAD" => ("C$", 2, true),
        "AUD" => ("A$", 2, true),
        "CHF" => ("CHF", 2, true),
        "MXN" => ("MX$", 2, true),
        _ => ("$", 2, true),
    }
}

/// Format with grouping separator (for "," or "_" type specs).
fn format_grouped(f: f64, grouping: char, spec: &FormatSpec) -> String {
    let mag = f.abs();
    let num_str = match spec.precision {
        Some(p) => {
            let rounded = (mag * 10f64.powi(p as i32)).round() / 10f64.powi(p as i32);
            format!("{:.precision$}", rounded, precision = p)
        }
        None => {
            let as_int = mag.round() as u128;
            as_int.to_string()
        }
    };
    let grouped = add_grouping(&num_str, grouping);
    if f < 0.0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// Add grouping separators to a numeric string (thousands grouping).
fn add_grouping(num_str: &str, sep: char) -> String {
    let (neg, digits) = if num_str.starts_with('-') {
        (true, &num_str[1..])
    } else {
        (false, num_str)
    };
    let (int_part, frac_part) = match digits.find('.') {
        Some(dot) => (&digits[..dot], &digits[dot..]),
        None => (digits, ""),
    };
    // Build from right to left: insert sep every 3 digits.
    let mut result = String::new();
    let len = int_part.len();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(sep);
        }
        result.push(c);
    }
    result.push_str(frac_part);
    if neg {
        result.insert(0, '-');
    }
    result
}

/// Format as JSON.
fn format_json(v: &FormatValue) -> String {
    match v {
        FormatValue::Str(s) => serde_json::to_string(&JsonValue::String(s.clone()))
            .unwrap_or_else(|_| format!("{:?}", s)),
        FormatValue::Number(n) => serde_json::to_string(&JsonValue::Number(
            serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0)),
        ))
        .unwrap_or_else(|_| n.to_string()),
        FormatValue::Bool(b) => {
            serde_json::to_string(&JsonValue::Bool(*b)).unwrap_or_else(|_| b.to_string())
        }
        FormatValue::Null => "null".to_string(),
        FormatValue::Int(i) => {
            serde_json::to_string(&JsonValue::from(i64::try_from(*i).unwrap_or(0)))
                .unwrap_or_else(|_| i.to_string())
        }
        FormatValue::BigInt(bi) => bi.to_string(),
        FormatValue::UInt(u) => u.to_string(),
        FormatValue::Array(items) => {
            let parts: Vec<String> = items.iter().map(|v| format_json(v)).collect();
            format!("[{}]", parts.join(", "))
        }
        FormatValue::Object(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}: {}",
                        serde_json::to_string(k).unwrap_or_default(),
                        format_json(v)
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        FormatValue::Tuple(items) => {
            let parts: Vec<String> = items.iter().map(|v| format_json(v)).collect();
            format!("({})", parts.join(", "))
        }
        FormatValue::Char(c) => serde_json::to_string(&JsonValue::String(c.to_string()))
            .unwrap_or_else(|_| c.to_string()),
        FormatValue::Other(s) => {
            serde_json::to_string(&JsonValue::String(s.clone())).unwrap_or_else(|_| s.clone())
        }
    }
}

/// Format as debug representation.
fn format_debug(v: &FormatValue) -> String {
    match v {
        FormatValue::Str(s) => format!("{:?}", s),
        FormatValue::Char(c) => format!("{:?}", c),
        FormatValue::Bool(b) => format!("{:?}", b),
        FormatValue::Null => "null".to_string(),
        FormatValue::Number(n) => format!("{n:?}"),
        FormatValue::Int(i) => format!("{i:?}"),
        FormatValue::UInt(u) => format!("{u:?}"),
        FormatValue::BigInt(bi) => format!("{bi:?}"),
        FormatValue::Array(items) => {
            let parts: Vec<String> = items.iter().map(|v| format_debug(v)).collect();
            format!("[{}]", parts.join(", "))
        }
        FormatValue::Object(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{k:?}: {}", format_debug(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        FormatValue::Tuple(items) => {
            let parts: Vec<String> = items.iter().map(|v| format_debug(v)).collect();
            format!("({})", parts.join(", "))
        }
        FormatValue::Other(s) => format!("{s:?}"),
    }
}

/// Top-level entry point: format a value with a specification string.
pub fn format_value(v: &FormatValue, spec: &str) -> Result<String, FormatError> {
    let parsed = FormatSpec::parse(spec)?;
    v.render(&parsed)
}

/// Convenience: format with default (no spec).
pub fn format_default(v: &FormatValue) -> String {
    v.default_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_spec() {
        let spec = FormatSpec::parse("").unwrap();
        assert!(spec.is_default());
    }

    #[test]
    fn parse_alignment() {
        let spec = FormatSpec::parse("<10").unwrap();
        assert_eq!(spec.align, '<');
        assert_eq!(spec.width, Some(10));
        assert_eq!(spec.fill, ' ');

        let spec = FormatSpec::parse(">10").unwrap();
        assert_eq!(spec.align, '>');
        assert_eq!(spec.width, Some(10));

        let spec = FormatSpec::parse("^10").unwrap();
        assert_eq!(spec.align, '^');
        assert_eq!(spec.width, Some(10));
    }

    #[test]
    fn parse_fill_and_align() {
        let spec = FormatSpec::parse("_^10").unwrap();
        assert_eq!(spec.fill, '_');
        assert_eq!(spec.align, '^');
        assert_eq!(spec.width, Some(10));
    }

    #[test]
    fn parse_zero_pad() {
        let spec = FormatSpec::parse("08").unwrap();
        assert!(spec.zero_pad);
        assert_eq!(spec.align, '>');
        assert_eq!(spec.width, Some(8));
    }

    #[test]
    fn parse_sign() {
        let spec = FormatSpec::parse("+").unwrap();
        assert_eq!(spec.sign, '+');

        let spec = FormatSpec::parse(" ").unwrap();
        assert_eq!(spec.sign, ' ');
    }

    #[test]
    fn parse_precision() {
        let spec = FormatSpec::parse(".2").unwrap();
        assert_eq!(spec.precision, Some(2));

        let spec = FormatSpec::parse("(4)").unwrap();
        assert_eq!(spec.precision, Some(4));
    }

    #[test]
    fn parse_type_specs() {
        let spec = FormatSpec::parse("int").unwrap();
        assert_eq!(spec.type_spec, "int");

        let spec = FormatSpec::parse("hex").unwrap();
        assert_eq!(spec.type_spec, "hex");

        let spec = FormatSpec::parse("HEX").unwrap();
        assert_eq!(spec.type_spec, "HEX");

        let spec = FormatSpec::parse("bin").unwrap();
        assert_eq!(spec.type_spec, "bin");

        let spec = FormatSpec::parse("oct").unwrap();
        assert_eq!(spec.type_spec, "oct");

        let spec = FormatSpec::parse("e").unwrap();
        assert_eq!(spec.type_spec, "e");

        let spec = FormatSpec::parse("E").unwrap();
        assert_eq!(spec.type_spec, "E");

        let spec = FormatSpec::parse("%").unwrap();
        assert_eq!(spec.type_spec, "%");

        let spec = FormatSpec::parse("currency(USD)").unwrap();
        assert_eq!(spec.type_spec, "currency(USD)");

        let spec = FormatSpec::parse("currency(INR)").unwrap();
        assert_eq!(spec.type_spec, "currency(INR)");
    }

    #[test]
    fn parse_invalid() {
        assert!(FormatSpec::parse(".").is_err());
        assert!(FormatSpec::parse("(2").is_err()); // missing closing paren
        assert!(FormatSpec::parse("(2x)").is_err()); // non-digit before closing paren
    }

    #[test]
    fn format_int_default() {
        let v = FormatValue::Int(42);
        assert_eq!(v.render(&FormatSpec::default()).unwrap(), "42");
    }

    #[test]
    fn format_int_with_width() {
        let v = FormatValue::Int(42);
        let spec = FormatSpec::parse(">5").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "   42");
    }

    #[test]
    fn format_int_zero_pad() {
        let v = FormatValue::Int(42);
        let spec = FormatSpec::parse("08").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "00000042");
    }

    #[test]
    fn format_int_sign() {
        let v = FormatValue::Int(10);
        let spec = FormatSpec::parse("+").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "+10");

        let v = FormatValue::Int(-10);
        let spec = FormatSpec::parse("+").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "-10");
    }

    #[test]
    fn format_hex() {
        let v = FormatValue::Int(255);
        let spec = FormatSpec::parse("hex").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "ff");

        let v = FormatValue::Int(255);
        let spec = FormatSpec::parse("HEX").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "FF");
    }

    #[test]
    fn format_bin() {
        let v = FormatValue::Int(10);
        let spec = FormatSpec::parse("bin").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "1010");
    }

    #[test]
    fn format_oct() {
        let v = FormatValue::Int(10);
        let spec = FormatSpec::parse("oct").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "12");
    }

    #[test]
    fn format_float() {
        let v = FormatValue::Number(3.14159);
        let spec = FormatSpec::parse("float(2)").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "3.14");
    }

    #[test]
    fn format_percent() {
        let v = FormatValue::Number(0.8734);
        let spec = FormatSpec::parse("%").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "87.3%");

        let v = FormatValue::Number(0.8734);
        let spec = FormatSpec::parse("percent(0)").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "87%");
    }

    #[test]
    fn format_scientific() {
        let v = FormatValue::Number(602214076000000000000000.0);
        let spec = FormatSpec::parse("e").unwrap();
        let result = v.render(&spec).unwrap();
        assert!(result.starts_with("6.022e23") || result.starts_with("6.022E23"));
    }

    #[test]
    fn format_currency_usd() {
        let v = FormatValue::Number(1234.5);
        let spec = FormatSpec::parse("currency(USD)").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "$1,234.50");
    }

    #[test]
    fn format_currency_inr() {
        let v = FormatValue::Number(1234.5);
        let spec = FormatSpec::parse("currency(INR)").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "₹1,234.50");
    }

    #[test]
    fn format_currency_jpy() {
        let v = FormatValue::Number(1234.5);
        let spec = FormatSpec::parse("currency(JPY)").unwrap();
        assert_eq!(v.render(&spec).unwrap(), "¥1,235");
    }

    #[test]
    fn format_align_left() {
        let v = FormatValue::Str("left".to_string());
        let spec = FormatSpec::parse("<20").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result, "left                ");
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn format_align_center() {
        let v = FormatValue::Str("center".to_string());
        let spec = FormatSpec::parse("^20").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn format_align_right() {
        let v = FormatValue::Str("right".to_string());
        let spec = FormatSpec::parse(">20").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result, "               right");
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn format_unicode_width() {
        // "café" has 4 Unicode display columns
        let v = FormatValue::Str("café".to_string());
        let spec = FormatSpec::parse("^12").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 12);
    }

    #[test]
    fn format_combined_sign_currency_width() {
        let v = FormatValue::Number(1234.5);
        let spec = FormatSpec::parse(">14+currency(USD)").unwrap();
        let result = v.render(&spec).unwrap();
        assert!(result.contains("$1,234.50"));
        assert!(result.starts_with("    +$1,234.50"));
    }

    #[test]
    fn format_grouping_comma() {
        let v = FormatValue::Number(1428627663.0);
        let spec = FormatSpec::parse(",").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result, "1,428,627,663");
    }

    #[test]
    fn format_grouping_underscore() {
        let v = FormatValue::Number(1428627663.0);
        let spec = FormatSpec::parse("_").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result, "1_428_627_663");
    }

    #[test]
    fn format_quoted() {
        let v = FormatValue::Str("say \"hi\"".to_string());
        let spec = FormatSpec::parse("quoted").unwrap();
        let result = v.render(&spec).unwrap();
        assert_eq!(result, "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn format_bigint() {
        let bi = BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap();
        let v = FormatValue::BigInt(bi.clone());
        let spec = FormatSpec::default();
        assert_eq!(v.render(&spec).unwrap(), "123456789012345678901234567890");
    }

    #[test]
    fn format_invalid_type() {
        let v = FormatValue::Int(42);
        let spec = FormatSpec::parse("invalid_type").unwrap();
        assert!(v.render(&spec).is_err());
    }

    #[test]
    fn format_error_propagates() {
        let spec = FormatSpec::parse(".").unwrap_err();
        assert_eq!(spec.message, "expected digits after '.'");
    }
}
