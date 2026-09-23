use crate::parsing::ast::Value;

/// Format a value with a depth limit to prevent stack overflow on deeply nested structures.
/// When max_depth is reached, nested values are abbreviated as <...>
pub fn fmt_with_depth(v: &Value, max_depth: usize) -> String {
    fmt_with_depth_inner(v, max_depth, 0)
}

fn fmt_with_depth_inner(v: &Value, max_depth: usize, depth: usize) -> String {
    if depth >= max_depth {
        return "<...>".to_string();
    }

    match v {
        Value::Ref(inner, _) => fmt_with_depth_inner(inner, max_depth, depth),
        Value::Str(s) => {
            if depth > 0 {
                format!("\"{}\"", s)
            } else {
                s.clone()
            }
        }
        Value::Char(c) => c.to_string(),
        Value::Error(le) => le.message.clone(),
        Value::Number(n) => format!("{}", n),
        Value::BigInt(bi) => format!("{}", bi),
        Value::Bool(b) => format!("{}", b),
        Value::Null => "null".to_string(),
        Value::Array(a) => {
            if a.is_empty() {
                "[]".to_string()
            } else if depth + 1 >= max_depth {
                "[<...>]".to_string()
            } else {
                let items: Vec<String> = a
                    .iter()
                    .map(|v| fmt_with_depth_inner(v, max_depth, depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
        Value::DynArray(da) => {
            if da.data.is_empty() {
                "[]".to_string()
            } else if depth + 1 >= max_depth {
                "[<...>]".to_string()
            } else {
                let items: Vec<String> = da
                    .data
                    .iter()
                    .map(|v| fmt_with_depth_inner(v, max_depth, depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
        Value::RawArray(_, a) => {
            if a.is_empty() {
                "[]".to_string()
            } else if depth + 1 >= max_depth {
                "[<...>]".to_string()
            } else {
                let items: Vec<String> = a
                    .iter()
                    .map(|v| fmt_with_depth_inner(v, max_depth, depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
        Value::Object(m) => {
            if m.is_empty() {
                "{}".to_string()
            } else if depth + 1 >= max_depth {
                "{<...>}".to_string()
            } else {
                let mut parts = Vec::new();
                for (k, vv) in m.iter() {
                    let vstr = fmt_with_depth_inner(vv, max_depth, depth + 1);
                    parts.push(format!("{}: {}", k, vstr));
                }
                format!("{{{}}}", parts.join(", "))
            }
        }
        Value::Tuple(t) => {
            if t.is_empty() {
                "()".to_string()
            } else if depth + 1 >= max_depth {
                "(<...>)".to_string()
            } else {
                let items: Vec<String> = t
                    .iter()
                    .map(|v| fmt_with_depth_inner(v, max_depth, depth + 1))
                    .collect();
                format!("({})", items.join(", "))
            }
        }
        Value::Set(s) => {
            if s.is_empty() {
                "{}".to_string()
            } else if depth + 1 >= max_depth {
                "{<...>}".to_string()
            } else {
                let items: Vec<String> = s
                    .iter()
                    .map(|v| fmt_with_depth_inner(v, max_depth, depth + 1))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
        }
        Value::Instance(i) => {
            if i.class_name.ends_with("Error") || i.class_name == "Error" {
                match i.get_field("name") {
                    Some(Value::Str(s)) => s,
                    _ => i.class_name.clone(),
                }
            } else {
                format!("<{} instance>", i.class_name)
            }
        }
        Value::LazyRange(s, e, step) => format!("range({}, {}, {})", s, e, step),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        _ => format!("{:?}", v),
    }
}

pub fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as JV;
    match v {
        Value::Null => JV::Null,
        Value::Bool(b) => JV::Bool(*b),
        Value::Number(n) => JV::Number(
            serde_json::Number::from_f64(*n).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        // Fixed-width signed integers
        Value::I8(n) => JV::Number((*n as i64).into()),
        Value::I16(n) => JV::Number((*n as i64).into()),
        Value::I32(n) => JV::Number((*n as i64).into()),
        Value::I64(n) => JV::Number((*n).into()),
        // Fixed-width unsigned integers
        Value::U8(n) => JV::Number((*n as u64).into()),
        Value::U16(n) => JV::Number((*n as u64).into()),
        Value::U32(n) => JV::Number((*n as u64).into()),
        Value::U64(n) => JV::Number((*n).into()),
        // Floating-point
        Value::F32(n) => serde_json::Number::from_f64(*n as f64)
            .map(JV::Number)
            .unwrap_or(JV::Null),
        Value::F64(n) => serde_json::Number::from_f64(*n)
            .map(JV::Number)
            .unwrap_or(JV::Null),
        // BigInt — serialize as string to preserve precision
        Value::BigInt(b) => JV::String(b.to_string()),
        Value::Char(c) => JV::String(c.to_string()),
        Value::Str(s) => JV::String(s.clone()),
        Value::Ref(inner, _) => value_to_json(inner),
        Value::Share(sr) => {
            let inner_val = unsafe { &(*sr.ptr).value };
            value_to_json(inner_val)
        }
        Value::Array(a) => JV::Array(a.iter().map(value_to_json).collect()),
        Value::RawArray(_, raw) => JV::Array(raw.iter().map(value_to_json).collect()),
        Value::DynArray(da) => JV::Array(da.data.iter().map(value_to_json).collect()),
        Value::Tuple(t) => JV::Array(t.iter().map(value_to_json).collect()),
        Value::Set(s) => JV::Array(s.iter().map(value_to_json).collect()),
        Value::U128(n) => JV::Number(
            serde_json::Number::from_f64(*n as f64).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Value::I128(n) => JV::Number(
            serde_json::Number::from_f64(*n as f64).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        Value::Complex(r, i) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "real".to_string(),
                serde_json::Number::from_f64(*r)
                    .map(JV::Number)
                    .unwrap_or(JV::Null),
            );
            obj.insert(
                "imag".to_string(),
                serde_json::Number::from_f64(*i)
                    .map(JV::Number)
                    .unwrap_or(JV::Null),
            );
            JV::Object(obj)
        }
        Value::Instance(inst) => {
            let mut obj = serde_json::Map::new();
            let fields = inst.fields.read().unwrap();
            for (k, vv) in fields.iter() {
                obj.insert(k.clone(), value_to_json(vv));
            }
            JV::Object(obj)
        }
        Value::Object(m) => {
            let mut obj = serde_json::Map::new();
            for (k, vv) in m.iter() {
                obj.insert(k.clone(), value_to_json(vv));
            }
            JV::Object(obj)
        }
        Value::LazyRange(..) => JV::String("range".to_string()),
        _ => JV::Null,
    }
}

pub fn fmt(v: &Value) -> String {
    match v {
        Value::Ref(inner, _) => fmt(inner),
        Value::Str(s) => s.clone(),
        Value::Char(c) => {
            // Fast path: single char to string
            let mut buf = String::with_capacity(4);
            buf.push(*c);
            buf
        }
        Value::Error(le) => le.message.clone(),
        Value::Number(n) => {
            // Optimize common integer cases
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                let int_val = *n as i64;
                itoa::Buffer::new().format(int_val).to_string()
            } else {
                ryu::Buffer::new().format(*n).to_string()
            }
        }
        Value::Bool(b) => {
            // Static strings - no allocation
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Null => "null".to_string(),
        Value::Instance(i) => {
            if i.class_name.ends_with("Error") || i.class_name == "Error" {
                match i.get_field("name") {
                    Some(Value::Str(s)) => s,
                    _ => i.class_name.clone(),
                }
            } else {
                format!("{:?}", v)
            }
        }
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        _ => format!("{:?}", v),
    }
}

/// Apply a format specifier to a value and return the formatted string.
///
/// Supported format specifiers:
/// - `<N` - Left align with width N
/// - `^N` - Center align with width N
/// - `>N` - Right align with width N
/// - `0N` - Zero-pad to width N (for numbers)
/// - `+` - Show sign for positive numbers
/// - `int` - Convert to integer
/// - `float(N)` - Format as float with N decimal places
/// - `bin` - Format as binary
/// - `hex` - Format as hexadecimal
/// - `HEX` - Format as uppercase hexadecimal
/// - `oct` - Format as octal
/// - `currency(CODE)` - Format as currency with given code (USD, EUR, INR, etc.)
/// - Combinations like `>12+currency(INR)` or `0>12currency(USD)`
pub fn apply_format_spec(v: &Value, spec: &str) -> String {
    let base_value = fmt(v);
    let spec = spec.trim();

    if spec.is_empty() {
        return base_value;
    }

    // Parse format spec components
    let mut fill_char = ' ';
    let mut align: Option<char> = None;
    let mut sign_plus = false;
    let mut width: Option<usize> = None;
    let precision: Option<usize> = None;
    let mut format_type: Option<String> = None;

    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;

    // Check for fill character followed by alignment
    if chars.len() >= 2 && (chars[1] == '<' || chars[1] == '^' || chars[1] == '>') {
        fill_char = chars[0];
        align = Some(chars[1]);
        i = 2;
    } else if !chars.is_empty() && (chars[0] == '<' || chars[0] == '^' || chars[0] == '>') {
        align = Some(chars[0]);
        i = 1;
    }

    // Check for sign
    if i < chars.len() && chars[i] == '+' {
        sign_plus = true;
        i += 1;
    }

    // Check for zero-padding (0 followed by digits)
    if i < chars.len() && chars[i] == '0' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
        // Zero-padding: set fill_char to '0' and default to right-align
        fill_char = '0';
        if align.is_none() {
            align = Some('>');
        }
        i += 1;
    }

    // Parse width (digits)
    let mut width_str = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        width_str.push(chars[i]);
        i += 1;
    }
    if !width_str.is_empty() {
        width = width_str.parse().ok();
    }

    // Parse type specifier (rest of the string)
    if i < chars.len() {
        format_type = Some(chars[i..].iter().collect::<String>());
    }

    // Apply format type first
    let mut result = if let Some(ref ftype) = format_type {
        apply_type_format(v, ftype, precision)
    } else {
        base_value
    };

    // Apply sign for positive numbers
    if sign_plus {
        if let Value::Number(n) = v {
            if *n >= 0.0 && !result.starts_with('-') && !result.starts_with('+') {
                result = format!("+{}", result);
            }
        }
    }

    // Apply width and alignment
    if let Some(w) = width {
        if result.len() < w {
            let padding = w - result.len();
            result = match align {
                Some('<') => format!("{}{}", result, fill_char.to_string().repeat(padding)),
                Some('^') => {
                    let left = padding / 2;
                    let right = padding - left;
                    format!(
                        "{}{}{}",
                        fill_char.to_string().repeat(left),
                        result,
                        fill_char.to_string().repeat(right)
                    )
                }
                Some('>') | None => format!("{}{}", fill_char.to_string().repeat(padding), result),
                _ => result,
            };
        }
    }

    result
}

/// Apply type-specific formatting
fn apply_type_format(v: &Value, ftype: &str, _precision: Option<usize>) -> String {
    let ftype = ftype.trim();

    // Handle currency format: currency(CODE)
    if ftype.starts_with("currency(") && ftype.ends_with(")") {
        let code = &ftype[9..ftype.len() - 1];
        return format_currency(v, code);
    }

    // Handle float format: float(N)
    if ftype.starts_with("float(") && ftype.ends_with(")") {
        let prec_str = &ftype[6..ftype.len() - 1];
        if let Ok(prec) = prec_str.parse::<usize>() {
            if let Value::Number(n) = v {
                return format!("{:.prec$}", n, prec = prec);
            }
        }
        return fmt(v);
    }

    // Handle simple type specifiers
    match ftype {
        "int" => match v {
            Value::Number(n) => format!("{}", *n as i64),
            _ => fmt(v),
        },
        "bin" => match v {
            Value::Number(n) => format!("{:b}", *n as i64),
            _ => fmt(v),
        },
        "hex" => match v {
            Value::Number(n) => format!("{:x}", *n as i64),
            _ => fmt(v),
        },
        "HEX" => match v {
            Value::Number(n) => format!("{:X}", *n as i64),
            _ => fmt(v),
        },
        "oct" => match v {
            Value::Number(n) => format!("{:o}", *n as i64),
            _ => fmt(v),
        },
        _ => {
            // Check if this is a combined format like "+currency(INR)"
            if ftype.starts_with("+") {
                let rest = &ftype[1..];
                if rest.starts_with("currency(") && rest.ends_with(")") {
                    let code = &rest[9..rest.len() - 1];
                    let mut result = format_currency(v, code);
                    if let Value::Number(n) = v {
                        if *n >= 0.0 && !result.starts_with('-') && !result.starts_with('+') {
                            // Find where the number starts and insert +
                            if let Some(pos) = result.find(|c: char| c.is_ascii_digit()) {
                                result.insert(pos, '+');
                            }
                        }
                    }
                    return result;
                }
            }
            fmt(v)
        }
    }
}

/// Format a value as currency with the given currency code
fn format_currency(v: &Value, code: &str) -> String {
    let n = match v {
        Value::Number(n) => *n,
        _ => return fmt(v),
    };

    let (symbol, decimal_places, use_comma) = match code.to_uppercase().as_str() {
        "USD" => ("$", 2, true),
        "EUR" => ("€", 2, true),
        "GBP" => ("£", 2, true),
        "JPY" => ("¥", 0, true),
        "INR" => ("₹", 2, true),
        "CNY" => ("¥", 2, true),
        "KRW" => ("₩", 0, true),
        "RUB" => ("₽", 2, true),
        "BRL" => ("R$", 2, true),
        "CAD" => ("C$", 2, true),
        "AUD" => ("A$", 2, true),
        "CHF" => ("CHF ", 2, true),
        "MXN" => ("MX$", 2, true),
        _ => ("", 2, true), // Generic format
    };

    let formatted_number = if use_comma {
        format_with_commas(n, decimal_places)
    } else {
        if decimal_places > 0 {
            format!("{:.prec$}", n, prec = decimal_places)
        } else {
            format!("{}", n as i64)
        }
    };

    format!("{}{}", symbol, formatted_number)
}

/// Format a number with comma separators for thousands
fn format_with_commas(n: f64, decimal_places: usize) -> String {
    let is_negative = n < 0.0;
    let n = n.abs();

    let (int_part, frac_part) = if decimal_places > 0 {
        let formatted = format!("{:.prec$}", n, prec = decimal_places);
        let parts: Vec<&str> = formatted.split('.').collect();
        (
            parts[0].to_string(),
            Some(parts.get(1).unwrap_or(&"").to_string()),
        )
    } else {
        (format!("{}", n as i64), None)
    };

    // Add commas to integer part
    let int_with_commas: String = int_part
        .chars()
        .rev()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 {
                vec![',', c]
            } else {
                vec![c]
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let result = match frac_part {
        Some(frac) => format!("{}.{}", int_with_commas, frac),
        None => int_with_commas,
    };

    if is_negative {
        format!("-{}", result)
    } else {
        result
    }
}
