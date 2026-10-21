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
/// Apply a format specifier to a value via unified `adesh_format` engine.
pub fn apply_format_spec(v: &Value, spec: &str) -> String {
    let fmt_val = ast_value_to_format_value(v);
    adesh_format::format_value(&fmt_val, spec).unwrap_or_else(|_| fmt(v))
}

pub fn ast_value_to_format_value(v: &Value) -> adesh_format::FormatValue {
    use adesh_format::FormatValue;
    match v {
        Value::Str(s) => FormatValue::Str(s.clone()),
        Value::Char(c) => FormatValue::Char(*c),
        Value::Bool(b) => FormatValue::Bool(*b),
        Value::Null => FormatValue::Null,
        Value::Number(n) => FormatValue::Number(*n),
        Value::BigInt(bi) => FormatValue::BigInt(bi.clone()),
        Value::I8(n) => FormatValue::Int(*n as i128),
        Value::I16(n) => FormatValue::Int(*n as i128),
        Value::I32(n) => FormatValue::Int(*n as i128),
        Value::I64(n) => FormatValue::Int(*n as i128),
        Value::I128(n) => FormatValue::Int(*n),
        Value::U8(n) => FormatValue::UInt(*n as u128),
        Value::U16(n) => FormatValue::UInt(*n as u128),
        Value::U32(n) => FormatValue::UInt(*n as u128),
        Value::U64(n) => FormatValue::UInt(*n as u128),
        Value::U128(n) => FormatValue::UInt(*n),
        Value::F32(n) => FormatValue::Number(*n as f64),
        Value::F64(n) => FormatValue::Number(*n),
        Value::Ref(inner, _) => ast_value_to_format_value(inner),
        Value::Array(items) => {
            FormatValue::Array(items.iter().map(ast_value_to_format_value).collect())
        }
        Value::RawArray(_, items) => {
            FormatValue::Array(items.iter().map(ast_value_to_format_value).collect())
        }
        Value::DynArray(da) => {
            FormatValue::Array(da.data.iter().map(ast_value_to_format_value).collect())
        }
        Value::Tuple(items) => {
            FormatValue::Tuple(items.iter().map(ast_value_to_format_value).collect())
        }
        Value::Object(m) => FormatValue::Object(
            m.iter()
                .map(|(k, val)| (k.clone(), ast_value_to_format_value(val)))
                .collect(),
        ),
        _ => FormatValue::Other(fmt(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::apply_format_spec;
    use crate::parsing::ast::Value;

    #[test]
    fn formats_fixed_width_numbers() {
        assert_eq!(apply_format_spec(&Value::U32(255), "hex"), "ff");
        assert_eq!(apply_format_spec(&Value::I64(10), "08"), "00000010");
        assert_eq!(apply_format_spec(&Value::F32(3.14), "float(1)"), "3.1");
    }

    #[test]
    fn formats_unicode_by_display_width() {
        assert_eq!(apply_format_spec(&Value::Str("₹".into()), "<4"), "₹   ");
    }

    #[test]
    fn combines_sign_currency_and_alignment() {
        assert_eq!(
            apply_format_spec(&Value::U64(1234), ">12+currency(INR)"),
            "  +₹1,234.00"
        );
    }
}
