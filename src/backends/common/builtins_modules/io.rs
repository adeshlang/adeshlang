//! I/O builtin functions for print, input, and formatting

use super::RuntimeValue;
use crate::execution::runtime::pretty_print::{PrettyPrintOptions, pretty_print};
use crate::parsing::ast::{ArrayElementType, DynamicArray, NativeFn, Value};
use crate::utils::collections::FastMap;
use colored::Colorize;
use std::sync::Arc;

// Thread-local storage for mock input values
thread_local! {
    static INPUT_MOCK_VALUES: std::cell::RefCell<Option<Vec<RuntimeValue>>> = const { std::cell::RefCell::new(None) };
    static INPUT_MOCK_INDEX: std::cell::RefCell<usize> = const { std::cell::RefCell::new(0) };
    static GENERIC_TYPE_CONTEXT_JIT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static JIT_LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

// Helper to set generic type for JIT context
pub fn set_jit_generic_type(type_name: String) {
    GENERIC_TYPE_CONTEXT_JIT.with(|ctx| {
        let mut types = ctx.borrow_mut();
        types.clear();
        types.push(type_name);
    });
}

// Helper to clear generic type after use
pub fn clear_jit_generic_type() {
    GENERIC_TYPE_CONTEXT_JIT.with(|ctx| {
        ctx.borrow_mut().clear();
    });
}

/// Helper to set mock input values in thread-local storage
#[allow(dead_code)]
pub fn set_thread_mock_input(values: Vec<String>) {
    INPUT_MOCK_VALUES.with(|mock| {
        let vals: Vec<RuntimeValue> = values.into_iter().map(RuntimeValue::String).collect();
        *mock.borrow_mut() = Some(vals);
    });
    INPUT_MOCK_INDEX.with(|idx| {
        *idx.borrow_mut() = 0;
    });
}

/// Helper to get next mock input from thread-local storage
#[allow(dead_code)]
pub fn get_thread_mock_input() -> Option<String> {
    INPUT_MOCK_VALUES.with(|mock| {
        let mock_ref = mock.borrow();
        if let Some(ref values) = *mock_ref {
            INPUT_MOCK_INDEX.with(|idx| {
                let mut idx_ref = idx.borrow_mut();
                if *idx_ref < values.len() {
                    let val = values[*idx_ref].clone();
                    *idx_ref += 1;
                    Some(val.as_string())
                } else {
                    None
                }
            })
        } else {
            None
        }
    })
}

// Record a fatal JIT error so the executor can stop execution
#[allow(dead_code)]
#[allow(dead_code)]
fn set_jit_error(msg: String) {
    JIT_LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(msg);
    });
}

// Retrieve and clear the last JIT error
pub fn take_jit_error() -> Option<String> {
    JIT_LAST_ERROR.with(|e| e.borrow_mut().take())
}

// ============================================================================
// Print Functions
// ============================================================================

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_print(args: &[RuntimeValue]) -> RuntimeValue {
    use std::io::{BufWriter, Write};

    let (
        values,
        sep,
        end,
        color,
        background,
        underline,
        bold,
        italic,
        strikethrough,
        file,
        _flush,
        pretty,
    ) = parse_print_options(args);

    // File output path
    if let Some(path) = file.as_deref() {
        let mut out = String::with_capacity(256);
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                out.push_str(&sep);
            }
            out.push_str(&v.as_string());
        }
        out.push_str(&end);

        // Apply styling for file output
        let styled = if color.is_some()
            || background.is_some()
            || underline
            || bold
            || italic
            || strikethrough
        {
            let mut codes: Vec<String> = Vec::with_capacity(6);
            if bold {
                codes.push("1".to_string());
            }
            if italic {
                codes.push("3".to_string());
            }
            if underline {
                codes.push("4".to_string());
            }
            if strikethrough {
                codes.push("9".to_string());
            }
            if let Some(hex) = color.as_deref() {
                if let Some((r, g, b)) = parse_hex_color(hex) {
                    codes.push(format!("38;2;{};{};{}", r, g, b));
                }
            }
            if let Some(hex) = background.as_deref() {
                if let Some((r, g, b)) = parse_hex_color(hex) {
                    codes.push(format!("48;2;{};{};{}", r, g, b));
                }
            }
            if codes.is_empty() {
                out
            } else {
                format!("\x1b[{}m{}\x1b[0m", codes.join(";"), out)
            }
        } else {
            out
        };

        let _ = write_to_file(path, &styled);
        return RuntimeValue::Null;
    }

    if let Some(mode) = pretty.as_deref() {
        if mode != "none" {
            let pretty_opts = match mode {
                "compact" => PrettyPrintOptions::compact(),
                "simple" => PrettyPrintOptions::simple_color(),
                "no_color" | "plain" => PrettyPrintOptions::no_color(),
                _ => PrettyPrintOptions::default(),
            };

            let mut output = String::with_capacity(256);
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    output.push_str(&sep);
                }
                output.push_str(&pretty_print(&runtime_value_to_value(v), &pretty_opts));
            }

            let stdout = std::io::stdout();
            let mut writer = BufWriter::with_capacity(8192, stdout.lock());

            let needs_styling = color.is_some()
                || background.is_some()
                || underline
                || bold
                || italic
                || strikethrough;

            if needs_styling {
                let mut codes = Vec::with_capacity(6);
                if bold {
                    codes.push("1".to_string());
                }
                if italic {
                    codes.push("3".to_string());
                }
                if underline {
                    codes.push("4".to_string());
                }
                if strikethrough {
                    codes.push("9".to_string());
                }
                if let Some(hex) = color.as_deref() {
                    if let Some((r, g, b)) = parse_hex_color(hex) {
                        codes.push(format!("38;2;{};{};{}", r, g, b));
                    }
                }
                if let Some(hex) = background.as_deref() {
                    if let Some((r, g, b)) = parse_hex_color(hex) {
                        codes.push(format!("48;2;{};{};{}", r, g, b));
                    }
                }
                if !codes.is_empty() {
                    let _ = write!(writer, "\x1b[{}m", codes.join(";"));
                }
                let _ = writer.write_all(output.as_bytes());
                if !codes.is_empty() {
                    let _ = writer.write_all(b"\x1b[0m");
                }
                let _ = writer.write_all(end.as_bytes());
            } else {
                let _ = writer.write_all(output.as_bytes());
                let _ = writer.write_all(end.as_bytes());
            }

            let _ = writer.flush();
            return RuntimeValue::Null;
        }
    }

    // Use BufWriter for high-speed buffered output (8KB buffer)
    let stdout = std::io::stdout();
    let mut writer = BufWriter::with_capacity(8192, stdout.lock());

    let needs_styling =
        color.is_some() || background.is_some() || underline || bold || italic || strikethrough;

    if needs_styling {
        // Styled path: build ANSI codes
        let mut codes = Vec::with_capacity(6);
        if bold {
            codes.push("1".to_string());
        }
        if italic {
            codes.push("3".to_string());
        }
        if underline {
            codes.push("4".to_string());
        }
        if strikethrough {
            codes.push("9".to_string());
        }

        if let Some(hex) = color.as_deref() {
            if let Some((r, g, b)) = parse_hex_color(hex) {
                codes.push(format!("38;2;{};{};{}", r, g, b));
            }
        }

        if let Some(hex) = background.as_deref() {
            if let Some((r, g, b)) = parse_hex_color(hex) {
                codes.push(format!("48;2;{};{};{}", r, g, b));
            }
        }

        // Write ANSI prefix
        if !codes.is_empty() {
            let _ = write!(writer, "\x1b[{}m", codes.join(";"));
        }

        // Write values
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                let _ = writer.write_all(sep.as_bytes());
            }
            let _ = writer.write_all(v.as_string().as_bytes());
        }

        // Write ANSI reset BEFORE newline
        if !codes.is_empty() {
            let _ = writer.write_all(b"\x1b[0m");
        }
        let _ = writer.write_all(end.as_bytes());
    } else {
        // Fast path: no styling, direct write
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                let _ = writer.write_all(sep.as_bytes());
            }
            let _ = writer.write_all(v.as_string().as_bytes());
        }
        let _ = writer.write_all(end.as_bytes());
    }

    let _ = writer.flush();
    RuntimeValue::Null
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_println(args: &[RuntimeValue]) -> RuntimeValue {
    runtime_print(args)
}

// ============================================================================
// Print Helper Functions
// ============================================================================

#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) fn parse_print_options(
    args: &[RuntimeValue],
) -> (
    &[RuntimeValue],
    String,
    String,
    Option<String>,
    Option<String>,
    bool,
    bool,
    bool,
    bool,
    Option<String>,
    bool,
    Option<String>,
) {
    if let Some(RuntimeValue::Object(opts)) = args.last() {
        if is_print_options(opts) {
            let values = &args[..args.len() - 1];
            let sep = get_str_opt(opts, "sep").unwrap_or_else(|| " ".to_string());
            let end = get_str_opt(opts, "end").unwrap_or_else(|| "\n".to_string());
            let color = get_str_opt(opts, "color");
            let background = get_str_opt(opts, "background");
            let underline = get_bool_opt(opts, "underline").unwrap_or(false);
            let bold = get_bool_opt(opts, "bold").unwrap_or(false);
            let italic = get_bool_opt(opts, "italic").unwrap_or(false);
            let strikethrough = get_bool_opt(opts, "strikethrough").unwrap_or(false);
            let file = get_str_opt(opts, "file");
            let flush = get_bool_opt(opts, "flush").unwrap_or(false);
            let pretty = get_pretty_opt(opts);
            return (
                values,
                sep,
                end,
                color,
                background,
                underline,
                bold,
                italic,
                strikethrough,
                file,
                flush,
                pretty,
            );
        }
    }
    (
        args,
        " ".to_string(),
        "\n".to_string(),
        None,
        None,
        false,
        false,
        false,
        false,
        None,
        false,
        None,
    )
}

#[allow(dead_code)]
#[allow(dead_code)]
fn is_print_options(opts: &FastMap<String, RuntimeValue>) -> bool {
    opts.contains_key("sep")
        || opts.contains_key("end")
        || opts.contains_key("file")
        || opts.contains_key("color")
        || opts.contains_key("background")
        || opts.contains_key("underline")
        || opts.contains_key("bold")
        || opts.contains_key("italic")
        || opts.contains_key("strikethrough")
        || opts.contains_key("pretty")
}

#[allow(dead_code)]
#[allow(dead_code)]
fn get_str_opt(opts: &FastMap<String, RuntimeValue>, key: &str) -> Option<String> {
    opts.get(key).and_then(|v| match v {
        RuntimeValue::String(s) => Some(s.clone()),
        _ => None,
    })
}

#[allow(dead_code)]
#[allow(dead_code)]
fn get_bool_opt(opts: &FastMap<String, RuntimeValue>, key: &str) -> Option<bool> {
    opts.get(key).and_then(|v| match v {
        RuntimeValue::Bool(b) => Some(*b),
        RuntimeValue::Int(i) => Some(*i != 0),
        RuntimeValue::Float(f) => Some(*f != 0.0),
        RuntimeValue::String(s) => match s.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

#[allow(dead_code)]
#[allow(dead_code)]
fn get_pretty_opt(opts: &FastMap<String, RuntimeValue>) -> Option<String> {
    opts.get("pretty").and_then(|v| match v {
        RuntimeValue::Bool(true) => Some("full".to_string()),
        RuntimeValue::Bool(false) => Some("none".to_string()),
        RuntimeValue::Int(i) => {
            if *i != 0 {
                Some("full".to_string())
            } else {
                Some("none".to_string())
            }
        }
        RuntimeValue::Float(f) => {
            if *f != 0.0 {
                Some("full".to_string())
            } else {
                Some("none".to_string())
            }
        }
        RuntimeValue::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn infer_int_value_type(n: i64) -> Value {
    if n >= 0 {
        let un = n as u64;
        if un <= u8::MAX as u64 {
            Value::U8(un as u8)
        } else if un <= u16::MAX as u64 {
            Value::U16(un as u16)
        } else if un <= u32::MAX as u64 {
            Value::U32(un as u32)
        } else {
            Value::U64(un)
        }
    } else if n >= i8::MIN as i64 {
        Value::I8(n as i8)
    } else if n >= i16::MIN as i64 {
        Value::I16(n as i16)
    } else if n >= i32::MIN as i64 {
        Value::I32(n as i32)
    } else {
        Value::I64(n)
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
fn runtime_value_to_value(v: &RuntimeValue) -> Value {
    match v {
        RuntimeValue::Int(n) => infer_int_value_type(*n),
        RuntimeValue::Float(n) => Value::F64(*n),
        RuntimeValue::Bool(b) => Value::Bool(*b),
        RuntimeValue::Char(c) => Value::Char(*c),
        RuntimeValue::String(s) => Value::Str(s.clone()),
        RuntimeValue::BigInt(bi) => Value::BigInt(bi.clone()),
        RuntimeValue::U8(n) => Value::U8(*n),
        RuntimeValue::U16(n) => Value::U16(*n),
        RuntimeValue::U32(n) => Value::U32(*n),
        RuntimeValue::U64(n) => Value::U64(*n),
        RuntimeValue::U128(n) => Value::U128(*n),
        RuntimeValue::I8(n) => Value::I8(*n),
        RuntimeValue::I16(n) => Value::I16(*n),
        RuntimeValue::I32(n) => Value::I32(*n),
        RuntimeValue::I64(n) => Value::I64(*n),
        RuntimeValue::I128(n) => Value::I128(*n),
        RuntimeValue::F32(n) => Value::F32(*n),
        RuntimeValue::F64(n) => Value::F64(*n),
        RuntimeValue::Array(arr) => Value::Array(arr.iter().map(runtime_value_to_value).collect()),
        RuntimeValue::Set(set_vals) => {
            Value::Set(set_vals.iter().map(runtime_value_to_value).collect())
        }
        RuntimeValue::Tuple(tup) => Value::Tuple(tup.iter().map(runtime_value_to_value).collect()),
        RuntimeValue::Object(obj) => {
            let mut map: rustc_hash::FxHashMap<String, Value> = rustc_hash::FxHashMap::default();
            for (k, v) in obj.iter() {
                map.insert(k.clone(), runtime_value_to_value(v));
            }
            Value::Object(Arc::new(map))
        }
        RuntimeValue::Promise(id) => Value::Promise(*id),
        RuntimeValue::Function(f) => {
            let _ = f;
            Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Null))))
        }
        RuntimeValue::RawArray(elem_ty, values) => Value::RawArray(
            elem_ty.clone(),
            values.iter().map(runtime_value_to_value).collect(),
        ),
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        } => {
            let converted: Vec<Value> = data.iter().map(runtime_value_to_value).collect();
            Value::DynArray(Box::new(DynamicArray {
                data: converted,
                element_type: ArrayElementType::from_type_name(element_type),
                concrete_type: concrete_type.clone(),
                tracked_capacity: tracked_capacity.unwrap_or(data.len()),
            }))
        }
        RuntimeValue::Null => Value::Null,
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim().to_lowercase();
    match s.as_str() {
        "black" => return Some((0, 0, 0)),
        "red" => return Some((255, 0, 0)),
        "green" => return Some((0, 255, 0)),
        "yellow" => return Some((255, 255, 0)),
        "blue" => return Some((0, 0, 255)),
        "magenta" => return Some((255, 0, 255)),
        "cyan" => return Some((0, 255, 255)),
        "white" => return Some((255, 255, 255)),
        "orange" => return Some((255, 165, 0)),
        "purple" => return Some((128, 0, 128)),
        "gray" | "grey" => return Some((128, 128, 128)),
        _ => {}
    }
    let h = s.trim_start_matches('#');
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some((r, g, b))
    } else if h.len() == 3 {
        let cs: Vec<char> = h.chars().collect();
        let r = u8::from_str_radix(&[cs[0], cs[0]].iter().collect::<String>(), 16).ok()?;
        let g = u8::from_str_radix(&[cs[1], cs[1]].iter().collect::<String>(), 16).ok()?;
        let b = u8::from_str_radix(&[cs[2], cs[2]].iter().collect::<String>(), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
fn write_to_file(path: &str, content: &str) -> Result<(), ()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| ())?;
    file.write_all(content.as_bytes()).map_err(|_| ())
}

// ============================================================================
// Format Functions
// ============================================================================

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_format(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return if args.is_empty() {
            RuntimeValue::String(String::new())
        } else {
            RuntimeValue::String(args[0].as_string())
        };
    }

    let value = &args[0];
    let spec = args[1].as_string();

    let formatted = apply_runtime_format(value, &spec);
    RuntimeValue::String(formatted)
}

/// Apply a format specifier to a RuntimeValue
#[allow(dead_code)]
#[allow(dead_code)]
fn apply_runtime_format(value: &RuntimeValue, spec: &str) -> String {
    let base_value = value.as_string();
    let spec = spec.trim();

    if spec.is_empty() {
        return base_value;
    }

    // Parse format spec components
    let mut fill_char = ' ';
    let mut align: Option<char> = None;
    let mut sign_plus = false;
    let mut width: Option<usize> = None;
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
        apply_runtime_type_format(value, ftype)
    } else {
        base_value
    };

    // Apply sign for positive numbers
    if sign_plus {
        if let RuntimeValue::Int(n) = value {
            if *n >= 0 && !result.starts_with('-') && !result.starts_with('+') {
                result = format!("+{}", result);
            }
        } else if let RuntimeValue::Float(n) = value {
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

/// Apply type-specific formatting for RuntimeValue
#[allow(dead_code)]
#[allow(dead_code)]
fn apply_runtime_type_format(value: &RuntimeValue, ftype: &str) -> String {
    let ftype = ftype.trim();

    // currency(CODE)
    if ftype.starts_with("currency(") && ftype.ends_with(")") {
        let code = &ftype[9..ftype.len() - 1];
        return format_runtime_currency(value, code);
    }

    // float(N)
    if ftype.starts_with("float(") && ftype.ends_with(")") {
        let prec_str = &ftype[6..ftype.len() - 1];
        if let Ok(prec) = prec_str.parse::<usize>() {
            if let Some(n) = value.as_float() {
                return format!("{:.prec$}", n, prec = prec);
            }
        }
        return value.as_string();
    }

    match ftype {
        "int" => match value {
            RuntimeValue::Float(n) => format!("{}", *n as i64),
            RuntimeValue::Int(n) => format!("{}", n),
            _ => value.as_string(),
        },
        "bin" => match value {
            RuntimeValue::Int(n) => format!("{:b}", n),
            RuntimeValue::Float(n) => format!("{:b}", *n as i64),
            _ => value.as_string(),
        },
        "hex" => match value {
            RuntimeValue::Int(n) => format!("{:x}", n),
            RuntimeValue::Float(n) => format!("{:x}", *n as i64),
            _ => value.as_string(),
        },
        "HEX" => match value {
            RuntimeValue::Int(n) => format!("{:X}", n),
            RuntimeValue::Float(n) => format!("{:X}", *n as i64),
            _ => value.as_string(),
        },
        "oct" => match value {
            RuntimeValue::Int(n) => format!("{:o}", n),
            RuntimeValue::Float(n) => format!("{:o}", *n as i64),
            _ => value.as_string(),
        },
        _ => {
            // +currency(CODE)
            if ftype.starts_with('+') {
                let rest = &ftype[1..];
                if rest.starts_with("currency(") && rest.ends_with(")") {
                    let code = &rest[9..rest.len() - 1];
                    let mut result = format_runtime_currency(value, code);
                    if let Some(n) = value.as_float() {
                        if n >= 0.0 && !result.starts_with('-') && !result.starts_with('+') {
                            if let Some(pos) = result.find(|c: char| c.is_ascii_digit()) {
                                result.insert(pos, '+');
                            }
                        }
                    }
                    return result;
                }
            }
            value.as_string()
        }
    }
}

/// Format a RuntimeValue as currency with the given currency code
#[allow(dead_code)]
#[allow(dead_code)]
fn format_runtime_currency(value: &RuntimeValue, code: &str) -> String {
    let n = match value {
        RuntimeValue::Int(i) => *i as f64,
        RuntimeValue::Float(f) => *f,
        _ => return value.as_string(),
    };

    let (symbol, decimal_places) = match code.to_uppercase().as_str() {
        "USD" => ("$", 2),
        "EUR" => ("€", 2),
        "GBP" => ("£", 2),
        "JPY" => ("¥", 0),
        "CNY" => ("¥", 2),
        "KRW" => ("₩", 0),
        "RUB" => ("₽", 2),
        "BRL" => ("R$", 2),
        "CAD" => ("C$", 2),
        "AUD" => ("A$", 2),
        "CHF" => ("CHF ", 2),
        "MXN" => ("MX$", 2),
        _ => ("", 2),
    };

    let formatted_number = format_with_commas_runtime(n, decimal_places);
    format!("{}{}", symbol, formatted_number)
}

/// Format a number with comma separators for thousands
#[allow(dead_code)]
#[allow(dead_code)]
fn format_with_commas_runtime(n: f64, decimal_places: usize) -> String {
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

// ============================================================================
// Input Functions
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{InputOptions, prompt_input};

    let prompt = if let Some(p) = args.first() {
        p.as_string()
    } else {
        String::new()
    };

    let mut opts = InputOptions {
        trim: true,
        ..Default::default()
    };

    let mut type_name: Option<String> = None;

    if args.len() >= 2 {
        if let RuntimeValue::Object(map) = &args[1] {
            if let Some(def_val) = map.get("default").or_else(|| map.get("def")) {
                opts.default = Some(def_val.as_string());
            }
            if let Some(RuntimeValue::Bool(b)) = map.get("trim") {
                opts.trim = *b;
            }
            if let Some(RuntimeValue::Bool(b)) = map.get("masked") {
                opts.masked = *b;
            }
            if let Some(n) = map.get("timeout").and_then(|v| v.as_float()) {
                opts.timeout_ms = Some((n.max(0.0) * 1000.0) as u64);
            }
            if let Some(v) = map.get("type") {
                type_name = Some(v.as_string());
            }
            if let Some(RuntimeValue::Array(arr)) = map.get("suggestions") {
                opts.suggestions = arr.iter().map(|v| v.as_string()).collect();
            }
            if let Some(RuntimeValue::Array(arr)) = map.get("allowed") {
                opts.allowed = Some(arr.iter().map(|v| v.as_string()).collect());
            }
            if let Some(RuntimeValue::Array(arr)) = map.get("disallowed") {
                opts.disallowed = Some(arr.iter().map(|v| v.as_string()).collect());
            }
            if let Some(RuntimeValue::Bool(b)) = map.get("notEmpty") {
                opts.not_empty = *b;
            }
        }
    }

    if type_name.is_none() {
        GENERIC_TYPE_CONTEXT_JIT.with(|ctx| {
            let types = ctx.borrow();
            if !types.is_empty() {
                type_name = Some(types[0].clone());
            }
        });
    }

    match prompt_input(&prompt, &opts) {
        Ok(res_str) => {
            if let Some(ref tn) = type_name {
                let val = RuntimeValue::String(res_str);
                match validate_type_conversion(&val, tn) {
                    Ok(converted) => converted,
                    Err(err_msg) => {
                        set_jit_error(err_msg);
                        RuntimeValue::Null
                    }
                }
            } else {
                RuntimeValue::String(res_str)
            }
        }
        Err(e) => {
            if let Some(d) = opts.default {
                RuntimeValue::String(d)
            } else {
                set_jit_error(e);
                RuntimeValue::Null
            }
        }
    }
}

// Input mock function - sets up mock input values for testing
// Accepts both single values and arrays
#[allow(dead_code)]
pub(super) fn runtime_input_mock(args: &[RuntimeValue]) -> RuntimeValue {
    let mock_arg = if args.len() >= 2 {
        &args[1]
    } else if let Some(first) = args.first() {
        first
    } else {
        return RuntimeValue::Null;
    };

    let mock_values = match mock_arg {
        RuntimeValue::Array(arr) => arr.clone(),
        RuntimeValue::DynArray { data, .. } => data.clone(),
        other => vec![other.clone()],
    };

    INPUT_MOCK_VALUES.with(|mock| {
        *mock.borrow_mut() = Some(mock_values.clone());
    });
    INPUT_MOCK_INDEX.with(|idx| {
        *idx.borrow_mut() = 0;
    });

    let str_mocks: Vec<String> = mock_values.iter().map(|v| v.as_string()).collect();
    let mut q = crate::execution::runtime_core::INPUT_PLAYBACK
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()))
        .lock()
        .unwrap();
    q.clear();
    for s in str_mocks {
        q.push_back(s);
    }

    RuntimeValue::Null
}

// ============================================================================
// Input Helper Functions
// ============================================================================

#[allow(dead_code)]
fn format_jit_input_type_error(type_name: &str, value: &RuntimeValue, reason: &str) -> String {
    let value_str = match value {
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Bool(b) => b.to_string(),
        RuntimeValue::Int(n) => n.to_string(),
        RuntimeValue::Float(f) => f.to_string(),
        RuntimeValue::U8(n) => n.to_string(),
        RuntimeValue::U16(n) => n.to_string(),
        RuntimeValue::U32(n) => n.to_string(),
        RuntimeValue::U64(n) => n.to_string(),
        RuntimeValue::U128(n) => n.to_string(),
        RuntimeValue::I8(n) => n.to_string(),
        RuntimeValue::I16(n) => n.to_string(),
        RuntimeValue::I32(n) => n.to_string(),
        RuntimeValue::I64(n) => n.to_string(),
        RuntimeValue::I128(n) => n.to_string(),
        RuntimeValue::F32(n) => n.to_string(),
        RuntimeValue::F64(n) => n.to_string(),
        RuntimeValue::BigInt(bi) => bi.to_string(),
        _ => format!("{:?}", value),
    };

    let header = format!("❌ input<{}> Type Conversion Error", type_name)
        .red()
        .bold();
    let input_line = format!("  Input value: {}", value_str.yellow());
    let reason_line = format!("  Reason: {}", reason);
    let type_hint = format!("  Type: {} (generic parameter)", type_name).cyan();

    format!("{}\n{}\n{}\n{}", header, input_line, reason_line, type_hint)
}

// Helper function to validate and convert values to specific types with bounds checking
#[allow(dead_code)]
fn validate_type_conversion(
    value: &RuntimeValue,
    target_type: &str,
) -> Result<RuntimeValue, String> {
    let make_err = |reason: &str| format_jit_input_type_error(target_type, value, reason);

    match target_type {
        "u8" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (0..=255).contains(&n) {
                Ok(RuntimeValue::U8(n as u8))
            } else {
                Err(make_err("expected unsigned integer in range 0-255"))
            }
        }
        "u16" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (0..=65535).contains(&n) {
                Ok(RuntimeValue::U16(n as u16))
            } else {
                Err(make_err("expected unsigned integer in range 0-65535"))
            }
        }
        "u32" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (0..=4294967295).contains(&n) {
                Ok(RuntimeValue::U32(n as u32))
            } else {
                Err(make_err("invalid u32 value (out of range)"))
            }
        }
        "u64" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if n >= 0 {
                Ok(RuntimeValue::U64(n as u64))
            } else {
                Err(make_err("expected unsigned integer (non-negative)"))
            }
        }
        "u128" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if n >= 0 {
                Ok(RuntimeValue::U128(n as u128))
            } else {
                Err(make_err("expected unsigned integer (non-negative)"))
            }
        }
        "i8" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (-128..=127).contains(&n) {
                Ok(RuntimeValue::I8(n as i8))
            } else {
                Err(make_err("expected signed integer in range -128 to 127"))
            }
        }
        "i16" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (-32768..=32767).contains(&n) {
                Ok(RuntimeValue::I16(n as i16))
            } else {
                Err(make_err("expected signed integer in range -32768 to 32767"))
            }
        }
        "i32" | "int" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            if (-2147483648..=2147483647).contains(&n) {
                Ok(RuntimeValue::I32(n as i32))
            } else {
                Err(make_err("invalid i32 value (out of range)"))
            }
        }
        "i64" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            Ok(RuntimeValue::I64(n))
        }
        "i128" => {
            let n = value
                .as_int()
                .ok_or_else(|| make_err("expected integer-convertible value"))?;
            Ok(RuntimeValue::I128(n as i128))
        }
        "f32" => match value {
            RuntimeValue::Float(f) => Ok(RuntimeValue::F32(*f as f32)),
            RuntimeValue::Int(n) => Ok(RuntimeValue::F32(*n as f32)),
            RuntimeValue::F32(f) => Ok(RuntimeValue::F32(*f)),
            other => {
                if let Some(f) = other.as_float() {
                    Ok(RuntimeValue::F32(f as f32))
                } else {
                    Err(make_err("expected number convertible to f32"))
                }
            }
        },
        "f64" | "float" | "number" => match value {
            RuntimeValue::Float(f) => Ok(RuntimeValue::F64(*f)),
            RuntimeValue::Int(n) => Ok(RuntimeValue::F64(*n as f64)),
            RuntimeValue::F64(f) => Ok(RuntimeValue::F64(*f)),
            other => {
                if let Some(f) = other.as_float() {
                    Ok(RuntimeValue::F64(f))
                } else {
                    Err(make_err("expected number convertible to f64"))
                }
            }
        },
        "bool" | "boolean" => match value {
            RuntimeValue::Bool(b) => Ok(RuntimeValue::Bool(*b)),
            RuntimeValue::String(s) => {
                let low = s.to_lowercase();
                let b = low == "true" || low == "yes" || low == "y" || low == "1";
                Ok(RuntimeValue::Bool(b))
            }
            other => {
                if let Some(b) = other.as_bool() {
                    Ok(RuntimeValue::Bool(b))
                } else {
                    Err(make_err("expected boolean-convertible value"))
                }
            }
        },
        "string" | "str" => Ok(RuntimeValue::String(value.as_string())),
        _ => Err(make_err("unsupported target type")),
    }
}

// ============================================================================
// Input Checkbox
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input_checkbox(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{CheckboxConfig, prompt_checkbox};

    let (prompt_val, opts_val, cfg_val) = if args.len() >= 3 {
        (&args[0], &args[1], Some(&args[2]))
    } else if args.len() == 2 {
        (&args[0], &args[1], None)
    } else {
        return RuntimeValue::Array(Vec::new());
    };

    let prompt = prompt_val.as_string();
    let options: Vec<String> = match opts_val {
        RuntimeValue::Array(arr) => arr.iter().map(|v| v.as_string()).collect(),
        RuntimeValue::DynArray { data, .. } => data.iter().map(|v| v.as_string()).collect(),
        _ => Vec::new(),
    };

    let mut config = CheckboxConfig::default();
    if let Some(RuntimeValue::Object(map)) = cfg_val {
        if let Some(RuntimeValue::Array(arr)) = map.get("default") {
            config.default = arr.iter().map(|v| v.as_string()).collect();
        }
        if let Some(RuntimeValue::Bool(b)) = map.get("required") {
            config.required = *b;
        }
        if let Some(n) = map.get("limit").and_then(|v| v.as_int()) {
            if n > 0 {
                config.limit = Some(n as usize);
            }
        }
        if let Some(RuntimeValue::Array(arr)) = map.get("disabled") {
            config.disabled = arr.iter().map(|v| v.as_string()).collect();
        }
    }

    match prompt_checkbox(&prompt, &options, &config) {
        Ok(selections) => {
            RuntimeValue::Array(selections.into_iter().map(RuntimeValue::String).collect())
        }
        Err(_) => RuntimeValue::Array(Vec::new()),
    }
}

// ============================================================================
// Input Radio / Select
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input_radio(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{RadioConfig, prompt_radio};

    let (prompt_val, opts_val, cfg_val) = if args.len() >= 3 {
        (&args[0], &args[1], Some(&args[2]))
    } else if args.len() == 2 {
        (&args[0], &args[1], None)
    } else {
        return RuntimeValue::Null;
    };

    let prompt = prompt_val.as_string();
    let options: Vec<String> = match opts_val {
        RuntimeValue::Array(arr) => arr.iter().map(|v| v.as_string()).collect(),
        RuntimeValue::DynArray { data, .. } => data.iter().map(|v| v.as_string()).collect(),
        _ => Vec::new(),
    };

    let mut config = RadioConfig::default();
    if let Some(RuntimeValue::Object(map)) = cfg_val {
        if let Some(RuntimeValue::String(s)) = map.get("default") {
            config.default = Some(s.clone());
        }
        if let Some(RuntimeValue::Bool(b)) = map.get("required") {
            config.required = *b;
        }
        if let Some(RuntimeValue::Array(arr)) = map.get("disabled") {
            config.disabled = arr.iter().map(|v| v.as_string()).collect();
        }
    }

    match prompt_radio(&prompt, &options, &config) {
        Ok(selected) => RuntimeValue::String(selected),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_select(args: &[RuntimeValue]) -> RuntimeValue {
    runtime_input_radio(args)
}

// ============================================================================
// Input Form
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input_form(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{FormConfig, FormFieldConfig, prompt_form};

    if args.is_empty() {
        return RuntimeValue::Null;
    }

    let fields_map = match &args[0] {
        RuntimeValue::Object(map) => map,
        _ => return RuntimeValue::Null,
    };

    let mut fields: Vec<(String, FormFieldConfig)> = Vec::new();
    for (k, v) in fields_map.iter() {
        let mut field_cfg = FormFieldConfig {
            prompt: k.clone(),
            ..Default::default()
        };
        if let RuntimeValue::Object(m) = v {
            if let Some(p) = m.get("prompt") {
                field_cfg.prompt = p.as_string();
            }
            if let Some(d) = m.get("default") {
                field_cfg.default = Some(d.as_string());
            }
            if let Some(RuntimeValue::Bool(b)) = m.get("masked") {
                field_cfg.masked = *b;
            }
        }
        fields.push((k.clone(), field_cfg));
    }

    let config = FormConfig::default();
    match prompt_form(&fields, &config) {
        Ok(results) => {
            let mut out = FastMap::default();
            for (k, v) in results {
                out.insert(k, RuntimeValue::String(v));
            }
            RuntimeValue::Object(out)
        }
        Err(_) => RuntimeValue::Null,
    }
}

// ============================================================================
// Input Confirm & Password
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input_confirm(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::prompt_confirm;

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Confirm?".into());
    let default = args.get(1).and_then(|v| v.as_bool()).unwrap_or(true);

    match prompt_confirm(&prompt, default) {
        Ok(b) => RuntimeValue::Bool(b),
        Err(_) => RuntimeValue::Bool(default),
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_password(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{PasswordConfig, prompt_password};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Password: ".into());
    let config = PasswordConfig::default();

    match prompt_password(&prompt, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

// ============================================================================
// World-First Primitives: Fuzzy, Slider, Tree, Table, Datepicker, Timepicker,
// Color, Pin, Diff, Hotkey, AI, Stream
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_input_fuzzy(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{FuzzyConfig, prompt_fuzzy};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Search: ".into());
    let options: Vec<String> = match args.get(1) {
        Some(RuntimeValue::Array(arr)) => arr.iter().map(|v| v.as_string()).collect(),
        Some(RuntimeValue::DynArray { data, .. }) => data.iter().map(|v| v.as_string()).collect(),
        _ => Vec::new(),
    };

    let config = FuzzyConfig::default();
    match prompt_fuzzy(&prompt, &options, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_slider(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{SliderConfig, prompt_slider};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Value: ".into());
    let mut min = 0.0;
    let mut max = 100.0;
    let mut step = 1.0;
    let mut default = 50.0;

    if let Some(RuntimeValue::Object(m)) = args.get(1) {
        if let Some(n) = m.get("min").and_then(|v| v.as_float()) {
            min = n;
        }
        if let Some(n) = m.get("max").and_then(|v| v.as_float()) {
            max = n;
        }
        if let Some(n) = m.get("step").and_then(|v| v.as_float()) {
            step = n;
        }
        if let Some(n) = m
            .get("default")
            .or_else(|| m.get("initial"))
            .or_else(|| m.get("value"))
            .and_then(|v| v.as_float())
        {
            default = n;
        } else {
            default = min;
        }
    } else {
        if let Some(n) = args.get(1).and_then(|v| v.as_float()) {
            min = n;
            default = min;
        }
        if let Some(n) = args.get(2).and_then(|v| v.as_float()) {
            max = n;
            default = (min + max) / 2.0;
        }
        if let Some(n) = args.get(3).and_then(|v| v.as_float()) {
            step = n;
        }
        if let Some(n) = args.get(4).and_then(|v| v.as_float()) {
            default = n;
        }
    }

    let config = SliderConfig::default();
    match prompt_slider(&prompt, min, max, step, default, &config) {
        Ok(val) => RuntimeValue::Float(val),
        Err(_) => RuntimeValue::Float(default),
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_tree(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{TreeConfig, TreeNode, prompt_tree};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Select node: ".into());
    let mut nodes = Vec::new();

    if let Some(RuntimeValue::Array(arr)) = args.get(1) {
        for v in arr {
            nodes.push(TreeNode {
                id: v.as_string(),
                label: v.as_string(),
                children: Vec::new(),
                expanded: false,
            });
        }
    }

    let config = TreeConfig::default();
    match prompt_tree(&prompt, &nodes, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_table(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{TableConfig, prompt_table};

    let (prompt, headers_val, rows_val) = if args.len() >= 3 {
        (args[0].as_string(), &args[1], &args[2])
    } else if args.len() == 2 {
        ("Select Table Cell: ".to_string(), &args[0], &args[1])
    } else {
        (
            "Table: ".to_string(),
            &RuntimeValue::Null,
            &RuntimeValue::Null,
        )
    };

    let headers: Vec<String> = match headers_val {
        RuntimeValue::Array(arr) => arr.iter().map(|v| v.as_string()).collect(),
        RuntimeValue::DynArray { data, .. } => data.iter().map(|v| v.as_string()).collect(),
        _ => Vec::new(),
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    let rows_items = match rows_val {
        RuntimeValue::Array(arr) => Some(arr.as_slice()),
        RuntimeValue::DynArray { data, .. } => Some(data.as_slice()),
        _ => None,
    };
    if let Some(items) = rows_items {
        for row_val in items {
            let row: Vec<String> = match row_val {
                RuntimeValue::Array(r) => r.iter().map(|v| v.as_string()).collect(),
                RuntimeValue::DynArray { data, .. } => data.iter().map(|v| v.as_string()).collect(),
                _ => Vec::new(),
            };
            rows.push(row);
        }
    }

    let config = TableConfig::default();
    match prompt_table(&prompt, &headers, &rows, &config) {
        Ok(res) => {
            let mut out = FastMap::default();
            let r_idx = res.selected_row;
            let c_idx = res.selected_col;

            out.insert("row".into(), RuntimeValue::Int(r_idx as i64));
            out.insert("col".into(), RuntimeValue::Int(c_idx as i64));
            out.insert("rowIndex".into(), RuntimeValue::Int(r_idx as i64));
            out.insert("colIndex".into(), RuntimeValue::Int(c_idx as i64));

            let cell_val = rows
                .get(r_idx)
                .and_then(|r| r.get(c_idx))
                .cloned()
                .unwrap_or_default();
            out.insert("value".into(), RuntimeValue::String(cell_val.clone()));
            out.insert("cell".into(), RuntimeValue::String(cell_val));

            let header_name = headers.get(c_idx).cloned().unwrap_or_default();
            out.insert("header".into(), RuntimeValue::String(header_name.clone()));
            out.insert("colName".into(), RuntimeValue::String(header_name.clone()));
            out.insert("columnName".into(), RuntimeValue::String(header_name));

            let selected_row_vec: Vec<RuntimeValue> = rows
                .get(r_idx)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(RuntimeValue::String)
                .collect();
            out.insert(
                "rowData".into(),
                RuntimeValue::Array(selected_row_vec.clone()),
            );
            out.insert(
                "row_data".into(),
                RuntimeValue::Array(selected_row_vec.clone()),
            );
            out.insert("rowValues".into(), RuntimeValue::Array(selected_row_vec));

            let selected_col_vec: Vec<RuntimeValue> = rows
                .iter()
                .filter_map(|r| r.get(c_idx).cloned())
                .map(RuntimeValue::String)
                .collect();
            out.insert(
                "colData".into(),
                RuntimeValue::Array(selected_col_vec.clone()),
            );
            out.insert(
                "col_data".into(),
                RuntimeValue::Array(selected_col_vec.clone()),
            );
            out.insert("colValues".into(), RuntimeValue::Array(selected_col_vec));

            let headers_vec: Vec<RuntimeValue> =
                headers.iter().cloned().map(RuntimeValue::String).collect();
            out.insert("headers".into(), RuntimeValue::Array(headers_vec));

            let mut row_obj_map = FastMap::default();
            if let Some(r) = rows.get(r_idx) {
                for (i, h) in headers.iter().enumerate() {
                    if let Some(v) = r.get(i) {
                        row_obj_map.insert(h.clone(), RuntimeValue::String(v.clone()));
                    }
                }
            }
            out.insert("rowObject".into(), RuntimeValue::Object(row_obj_map));

            let grid_matrix: Vec<RuntimeValue> = rows
                .iter()
                .map(|r| RuntimeValue::Array(r.iter().cloned().map(RuntimeValue::String).collect()))
                .collect();
            out.insert("data".into(), RuntimeValue::Array(grid_matrix.clone()));
            out.insert("grid".into(), RuntimeValue::Array(grid_matrix));

            RuntimeValue::Object(out)
        }
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_datepicker(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{DatepickerConfig, prompt_datepicker};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Date: ".into());
    let config = DatepickerConfig::default();

    match prompt_datepicker(&prompt, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_datetime(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{DatetimeConfig, prompt_datetimepicker};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "DateTime: ".into());
    let config = DatetimeConfig::default();

    match prompt_datetimepicker(&prompt, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_timepicker(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{TimepickerConfig, prompt_timepicker};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Time: ".into());
    let config = TimepickerConfig::default();

    match prompt_timepicker(&prompt, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_color(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{ColorConfig, prompt_color};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Color: ".into());
    let config = ColorConfig::default();

    match prompt_color(&prompt, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_pin(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{PinConfig, prompt_pin};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "PIN: ".into());
    let digits = args.get(1).and_then(|v| v.as_int()).unwrap_or(4) as usize;
    let config = PinConfig::default();

    match prompt_pin(&prompt, digits, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_diff(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{DiffConfig, prompt_diff};

    let (prompt, original) = if args.len() >= 2 {
        let p = args[0].as_string();
        let orig = args[1].as_string();
        (p, orig)
    } else if let Some(first) = args.first() {
        ("Diff: ".to_string(), first.as_string())
    } else {
        ("Diff: ".to_string(), String::new())
    };
    let config = DiffConfig::default();

    match prompt_diff(&prompt, &original, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_hotkey(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{HotkeyConfig, prompt_hotkey};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Hotkey: ".into());
    let config = HotkeyConfig::default();

    match prompt_hotkey(&prompt, &config) {
        Ok(res) => {
            let mut out = FastMap::default();
            out.insert("key".into(), RuntimeValue::String(res.key));
            out.insert("chord".into(), RuntimeValue::String(res.chord));
            RuntimeValue::Object(out)
        }
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_ai(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{AiPredictConfig, prompt_ai};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Prompt: ".into());
    let context: Vec<String> = match args.get(1) {
        Some(RuntimeValue::Array(arr)) => arr.iter().map(|v| v.as_string()).collect(),
        _ => Vec::new(),
    };
    let config = AiPredictConfig::default();

    match prompt_ai(&prompt, &context, &config) {
        Ok(s) => RuntimeValue::String(s),
        Err(_) => RuntimeValue::Null,
    }
}

#[allow(dead_code)]
pub(super) fn runtime_input_stream(args: &[RuntimeValue]) -> RuntimeValue {
    use crate::runtime::tui_input::{StreamConfig, prompt_stream};

    let prompt = args
        .first()
        .map(|v| v.as_string())
        .unwrap_or_else(|| "Stream: ".into());
    let config = StreamConfig::default();

    match prompt_stream(&prompt, &config) {
        Ok(tokens) => RuntimeValue::Array(tokens.into_iter().map(RuntimeValue::String).collect()),
        Err(_) => RuntimeValue::Array(Vec::new()),
    }
}
