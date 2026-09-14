use super::Interpreter;
use super::ops;
use crate::parsing::ast::Value;
use crate::typesystem::layouts::field_layout::{FieldLayout, LayoutComputer};
use crate::typesystem::visibility::Visibility;

fn parse_visibility(s: &str) -> Visibility {
    match s {
        "private" | "priv" => Visibility::Private,
        "protected" | "prot" => Visibility::Protected,
        _ => Visibility::Public,
    }
}

pub(super) fn install_range(i: &mut Interpreter) {
    i.define_builtin("range", |_ctx, args| {
        if args.is_empty() {
            return Err("range() requires at least 1 argument".to_string());
        }

        let start: f64;
        let end: f64;
        let step: f64;

        match args.len() {
            1 => {
                // range(end) -> 0..end with step 1
                start = 0.0;
                end = ops::num(args[0].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                step = 1.0;
            }
            2 => {
                // range(start, end) -> start..end with step 1
                start = ops::num(args[0].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                end = ops::num(args[1].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                step = 1.0;
            }
            3 => {
                // range(start, end, step)
                start = ops::num(args[0].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                end = ops::num(args[1].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                step = ops::num(args[2].clone())
                    .map_err(|_| "range() arguments must be numbers".to_string())?;
                if step == 0.0 {
                    return Err("range() step cannot be zero".to_string());
                }
            }
            _ => return Err("range() takes at most 3 arguments".to_string()),
        }

        Ok(Value::LazyRange(start, end, step))
    });
}

pub(super) fn install_concat(i: &mut Interpreter) {
    i.define_builtin("concat", |_ctx, args| {
        let mut out = String::new();
        for a in &args {
            out.push_str(&crate::execution::runtime::format::fmt(a));
        }
        Ok(Value::Str(out))
    });
}

pub(super) fn install_print(i: &mut Interpreter) {
    i.define_builtin("print", |_ctx, args| {
        // Fast path: Check for styling options first
        let mut sep = " ";
        let mut end = "\n";
        let mut file: Option<String> = None;
        let mut color_hex: Option<&str> = None;
        let mut background_hex: Option<&str> = None;
        let mut bold = false;
        let mut italic = false;
        let mut underline = false;
        let mut strikethrough = false;
        let mut _flush = false;

        let mut values_end = args.len();

        // Parse options from last argument if it's an object with print options
        if let Some(Value::Object(opts)) = args.last() {
            let is_options = opts.contains_key("sep")
                || opts.contains_key("end")
                || opts.contains_key("file")
                || opts.contains_key("color")
                || opts.contains_key("background")
                || opts.contains_key("bold")
                || opts.contains_key("italic")
                || opts.contains_key("underline")
                || opts.contains_key("strikethrough")
                || opts.contains_key("flush");

            if is_options {
                values_end -= 1;
                if let Some(Value::Str(s)) = opts.get("sep") {
                    sep = s.as_str();
                }
                if let Some(Value::Str(s)) = opts.get("end") {
                    end = s.as_str();
                }
                if let Some(Value::Str(s)) = opts.get("file") {
                    file = Some(s.clone());
                }
                if let Some(Value::Str(s)) = opts.get("color") {
                    color_hex = Some(s.as_str());
                }
                if let Some(Value::Str(s)) = opts.get("background") {
                    background_hex = Some(s.as_str());
                }
                if let Some(Value::Bool(b)) = opts.get("bold") {
                    bold = *b;
                }
                if let Some(Value::Bool(b)) = opts.get("italic") {
                    italic = *b;
                }
                if let Some(Value::Bool(b)) = opts.get("underline") {
                    underline = *b;
                }
                if let Some(Value::Bool(b)) = opts.get("strikethrough") {
                    strikethrough = *b;
                }
                if let Some(Value::Bool(b)) = opts.get("flush") {
                    _flush = *b;
                }
            }
        }

        // File output path - keep as is (less performance critical)
        if let Some(path) = file {
            use std::fs::OpenOptions;
            use std::io::Write;
            let mut out = String::with_capacity(256);
            for (i, a) in args[..values_end].iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                out.push_str(&crate::execution::runtime::format::fmt(a));
            }
            out.push_str(end);
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(out.as_bytes());
            }
            return Ok(Value::Null);
        }

        let needs_styling = color_hex.is_some()
            || background_hex.is_some()
            || bold
            || italic
            || underline
            || strikethrough;

        if needs_styling {
            // Styled output: Use stdio for ANSI support
            use crate::execution::runtime_core::stdio;

            // Build ANSI codes
            let mut style_buf = String::with_capacity(32);
            let mut codes = Vec::with_capacity(6);
            if bold {
                codes.push("1");
            }
            if italic {
                codes.push("3");
            }
            if underline {
                codes.push("4");
            }
            if strikethrough {
                codes.push("9");
            }

            let color_code = if let Some(hex) = color_hex {
                if let Some((r, g, b)) = parse_print_hex_color(hex) {
                    format!("38;2;{};{};{}", r, g, b)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            if !color_code.is_empty() {
                codes.push(&color_code);
            }

            let bg_code = if let Some(hex) = background_hex {
                if let Some((r, g, b)) = parse_print_hex_color(hex) {
                    format!("48;2;{};{};{}", r, g, b)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            if !bg_code.is_empty() {
                codes.push(&bg_code);
            }

            if !codes.is_empty() {
                style_buf.push_str("\x1b[");
                style_buf.push_str(&codes.join(";"));
                style_buf.push_str("m");
                stdio::write_stdout(style_buf.as_bytes());
            }

            // Write values with styling
            for (i, a) in args[..values_end].iter().enumerate() {
                if i > 0 {
                    stdio::write_stdout(sep.as_bytes());
                }
                stdio::write_stdout(crate::execution::runtime::format::fmt(a).as_bytes());
            }
            stdio::write_stdout(end.as_bytes());

            if !codes.is_empty() {
                stdio::write_stdout(b"\x1b[0m");
            }

            if _flush {
                stdio::flush_stdout();
            }
        } else {
            // ======================================================================
            // ULTRA-FAST PATH: No styling, use fast_print module
            // ======================================================================
            use crate::execution::runtime_core::fast_print;

            // Use the optimized multi-value print path
            fast_print::print_values(&args[..values_end], sep, end);

            if _flush {
                fast_print::flush_fast_buffer();
            }
        }
        Ok(Value::Null)
    });
}

pub(super) fn install_opt_layout(i: &mut Interpreter) {
    // with_layout(instance, [ { name: string, type: string, visibility?: string }, ... ]) -> Instance
    // Returns a NEW instance with packed storage initialized and existing primitive fields migrated.
    i.define_builtin("with_layout", |_ctx, args| {
        if args.len() != 2 {
            return Err("with_layout(instance, spec) expects 2 arguments".to_string());
        }
        let mut inst = match &args[0] {
            Value::Instance(i0) => i0.clone(),
            _ => return Err("with_layout: first argument must be an instance".to_string()),
        };
        let spec = match &args[1] {
            Value::Array(a) => a,
            _ => {
                return Err(
                    "with_layout: second argument must be an array of field specs".to_string(),
                );
            }
        };

        // Build field layout
        let mut fl = FieldLayout::new();
        for v in spec.iter() {
            if let Value::Object(m) = v {
                let name = match m.get("name") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => return Err("with_layout: each field needs a string 'name'".to_string()),
                };
                let ty = match m.get("type") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => return Err("with_layout: each field needs a string 'type'".to_string()),
                };
                let vis = m
                    .get("visibility")
                    .and_then(|vv| {
                        if let Value::Str(s) = vv {
                            Some(parse_visibility(s))
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Visibility::Public);
                let (sz, al) = LayoutComputer::compute_primitive(&ty)
                    .or_else(|| {
                        if ty.starts_with("ref ") || ty.starts_with("mut ") {
                            Some(LayoutComputer::compute_pointer())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(LayoutComputer::compute_pointer);
                fl.add_field(name, sz, al, vis, ty);
            } else {
                return Err(
                    "with_layout: spec items must be objects {name, type[, visibility]}"
                        .to_string(),
                );
            }
        }
        fl.finalize();
        let fl = std::sync::Arc::new(fl);

        // Initialize packed storage
        inst.init_layout(fl.clone());

        // Migrate existing primitive fields into packed storage when present
        // Only remove from map when packing succeeds.
        if let Ok(mut map) = inst.fields.write() {
            // collect keys to avoid borrow issues
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                if let Some(v) = map.get(&k).cloned() {
                    // Try packing only (avoid fallback write to map)
                    if inst.try_pack_only(&k, &v) {
                        map.remove(&k);
                    }
                }
            }
        }

        Ok(Value::Instance(inst))
    });
}

pub(super) fn install_test_assertions(i: &mut Interpreter) {
    use super::interpreter_impl::builtins::testing::*;

    i.define_builtin("assert", |_ctx, args| builtin_assert(args));

    i.define_builtin("assert_eq", |_ctx, args| builtin_assert_eq(args));

    i.define_builtin("assert_ne", |_ctx, args| builtin_assert_ne(args));
}

#[allow(dead_code)]
fn apply_print_styles(
    text: &str,
    color: Option<&str>,
    background: Option<&str>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
) -> String {
    let mut codes: Vec<String> = Vec::new();

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

    if let Some(hex) = color {
        if let Some((r, g, b)) = parse_print_hex_color(hex) {
            codes.push(format!("38;2;{};{};{}", r, g, b));
        }
    }

    if let Some(hex) = background {
        if let Some((r, g, b)) = parse_print_hex_color(hex) {
            codes.push(format!("48;2;{};{};{}", r, g, b));
        }
    }

    if codes.is_empty() {
        text.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

fn parse_print_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some((r, g, b))
    } else if h.len() == 3 {
        let cs: Vec<char> = h.chars().collect();
        let r = u8::from_str_radix(&format!("{}{}", cs[0], cs[0]), 16).ok()?;
        let g = u8::from_str_radix(&format!("{}{}", cs[1], cs[1]), 16).ok()?;
        let b = u8::from_str_radix(&format!("{}{}", cs[2], cs[2]), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}
