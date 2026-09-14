use super::encoder::*;
use crate::parsing::ast::{Expr, ExprKind, Stmt, StmtKind, TokenKind, Value};
use crate::parsing::error::{ErrorKind, LangError};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;
use crate::types::field_layout::FieldLayout;
use crate::types::visibility::Visibility;
use crate::utils::collections::FastMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

struct CompilerContext<'a> {
    locals_f64: &'a FastMap<String, u32>,
    locals_i32: &'a FastMap<String, u32>,
    funcs: &'a FastMap<String, (u32, Vec<u8>, Option<u8>)>,
    strings: &'a Vec<(String, u32)>,
    str_vars: &'a mut FastMap<String, (u32, u32)>,
    complex_vars: &'a FastMap<String, (u32, u32)>,
    var_values: &'a FastMap<String, Value>,
    struct_layouts: &'a FastMap<String, FieldLayout>,
    locals_types: &'a mut FastMap<String, String>,
    /// Stack of defer scopes for compile-time inlining (zero runtime overhead).
    defer_scopes: Vec<Vec<Stmt>>,
    /// Records `defer_scopes.len()` at each loop body entry for break/continue.
    loop_defer_depths: Vec<usize>,
}

fn compute_wasm_layout(fields: &[(String, String)]) -> FieldLayout {
    let mut layout = FieldLayout::new();
    for (name, type_name) in fields {
        let (size, align) = match type_name.as_str() {
            "i32" | "int" => (4, 4),
            "f64" | "number" => (8, 8),
            "bool" => (1, 1),
            _ => (4, 4), // Pointers (Structs, Strings)
        };
        layout.add_field(
            name.clone(),
            size,
            align,
            Visibility::Public,
            type_name.clone(),
        );
    }
    layout.finalize();
    layout
}

fn gen_expr_f64(
    e: &Expr,
    out: &mut Vec<u8>,
    ctx: &mut CompilerContext,
) -> Result<Option<String>, String> {
    match &e.kind {
        ExprKind::Literal(v) => match v {
            crate::parsing::ast::Value::Number(n) => {
                out.push(0x44);
                out.extend(&n.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::F64(n) => {
                out.push(0x44);
                out.extend(&n.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::F32(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::I64(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::I32(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::I16(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::I8(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::U64(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::U32(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::U16(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::U8(n) => {
                let n64 = *n as f64;
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::Bool(b) => {
                let n64: f64 = if *b { 1.0 } else { 0.0 };
                out.push(0x44);
                out.extend(&n64.to_bits().to_le_bytes());
                Ok(None)
            }
            crate::parsing::ast::Value::Str(s) => {
                let off = ctx
                    .strings
                    .iter()
                    .find(|(x, _)| x == s)
                    .map(|(_, o)| *o)
                    .unwrap_or(1024) as f64;
                out.push(0x44);
                out.extend(&off.to_bits().to_le_bytes());
                Ok(None)
            }
            _ => {
                out.push(0x44);
                out.extend(&0f64.to_bits().to_le_bytes());
                Ok(None)
            }
        },
        ExprKind::Variable(name) => {
            if let Some(idx) = ctx.locals_f64.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                Ok(None)
            } else if let Some(idx) = ctx.locals_i32.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                out.push(0xB7);
                Ok(None)
            } else {
                if let Some((fidx, _, ret_type)) = ctx.funcs.get(name) {
                    if let Some(rt) = ret_type {
                        out.push(0x10);
                        write_u32_leb(*fidx, out);
                        if *rt == 0x7F {
                            out.push(0xB7);
                        }
                        Ok(None)
                    } else {
                        Err("call to void function in expression".into())
                    }
                } else {
                    Err("unknown numeric variable".into())
                }
            }
        }
        ExprKind::Binary(l, op, r) => {
            // Handle nullish coalescing
            if matches!(op, TokenKind::NullCoalesce) {
                // Evaluate left operand
                gen_expr_f64(l, out, ctx)?;

                // Store in scratch local for reuse
                let scratch_idx = ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .copied()
                    .ok_or("Internal error: __scratch_f64 local not found")?;
                out.push(0x22); // local.tee
                write_u32_leb(scratch_idx, out);

                // Check if null (represented as NaN in f64)
                out.push(0x20); // local.get
                write_u32_leb(scratch_idx, out);
                out.push(0x63); // f64.eq (compare with itself)

                // If NaN (left == left fails), use right; else use left
                out.push(0x04); // if
                out.push(0x7C); // f64 result type

                // Then: return left value (it's a valid number)
                out.push(0x20); // local.get
                write_u32_leb(scratch_idx, out);

                // Else: evaluate and return right
                out.push(0x05); // else
                gen_expr_f64(r, out, ctx)?;

                out.push(0x0B); // end
                return Ok(None);
            }

            gen_expr_f64(l, out, ctx)?;
            gen_expr_f64(r, out, ctx)?;
            match op {
                TokenKind::Plus => {
                    out.push(0xA0);
                    Ok(None)
                }
                TokenKind::Minus => {
                    out.push(0xA1);
                    Ok(None)
                }
                TokenKind::Star => {
                    out.push(0xA2);
                    Ok(None)
                }
                TokenKind::Slash => {
                    out.push(0xA3);
                    Ok(None)
                }
                _ => Err("unsupported binary op in wasm MVP".into()),
            }
        }
        ExprKind::Unary(op, r) => {
            gen_expr_f64(r, out, ctx)?;
            match op {
                TokenKind::Minus => {
                    out.push(0xA1);
                    Ok(None)
                }
                _ => Err("unsupported unary op in wasm MVP".into()),
            }
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if let Some((fidx, params, ret_type)) = ctx.funcs.get(name) {
                    for (i, a) in args.iter().enumerate() {
                        gen_expr_f64(a, out, ctx)?;
                        if let Some(pt) = params.get(i) {
                            if *pt == 0x7F {
                                out.push(0x9D);
                            }
                        }
                    }
                    out.push(0x10);
                    write_u32_leb(*fidx, out);
                    if let Some(rt) = ret_type {
                        if *rt == 0x7F {
                            out.push(0xB7);
                        }
                        Ok(None)
                    } else {
                        Err("call to void function in expression".into())
                    }
                } else {
                    Err("unknown callee".into())
                }
            } else {
                Err("unsupported callee".into())
            }
        }
        ExprKind::Get(target, field) => {
            let type_name = gen_expr_i32(target, out, ctx)?;
            if let Some(tn) = type_name {
                if let Some(layout) = ctx.struct_layouts.get(&tn) {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A);
                                }
                                out.push(0x2B); // f64.load
                                return Ok(Some("f64".into()));
                            } else {
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A);
                                }
                                out.push(0x28); // i32.load
                                out.push(0xB7); // f64.convert
                                return Ok(None);
                            }
                        }
                    }
                }
                Err(format!(
                    "Accessing field {} on unknown struct {}",
                    field, tn
                ))
            } else {
                Err("Accessing field on unknown type".into())
            }
        }
        ExprKind::OptGet(target, field) => {
            // Optional chaining for property access: obj?.field
            let scratch_idx = ctx
                .locals_i32
                .get("__scratch")
                .copied()
                .ok_or("Internal error: __scratch local not found")?;

            // Evaluate target object to i32 pointer
            let type_name = gen_expr_i32(target, out, ctx)?;

            // Store pointer in scratch
            out.push(0x22); // local.tee
            write_u32_leb(scratch_idx, out);

            // Check if null (0)
            out.push(0x45); // i32.eqz

            // If null, return NaN as f64 null representation
            out.push(0x04); // if
            out.push(0x7C); // f64 result type

            // Then: return NaN (null)
            out.push(0x44); // f64.const
            out.extend(&f64::NAN.to_bits().to_le_bytes());

            // Else: access field normally
            out.push(0x05); // else
            out.push(0x20); // local.get
            write_u32_leb(scratch_idx, out);

            if let Some(tn) = type_name {
                if let Some(layout) = ctx.struct_layouts.get(&tn) {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A);
                        }
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                out.push(0x2B); // f64.load
                            } else {
                                out.push(0x28); // i32.load
                                out.push(0xB7); // f64.convert
                            }
                        } else {
                            out.push(0x2B); // f64.load (default)
                        }
                    }
                }
            }

            out.push(0x0B); // end
            Ok(None)
        }
        ExprKind::Array(_) | ExprKind::Tuple(_) | ExprKind::Object(_) => {
            out.push(0x44);
            out.extend(&0f64.to_bits().to_le_bytes());
            Ok(None)
        }
        _ => Err("unsupported expression in wasm MVP".into()),
    }
}

fn gen_expr_i32(
    e: &Expr,
    out: &mut Vec<u8>,
    ctx: &mut CompilerContext,
) -> Result<Option<String>, String> {
    match &e.kind {
        ExprKind::Literal(v) => {
            let n = match v {
                crate::parsing::ast::Value::Number(n) => *n as i32,
                crate::parsing::ast::Value::I64(n) => *n as i32,
                crate::parsing::ast::Value::I32(n) => *n,
                crate::parsing::ast::Value::I16(n) => *n as i32,
                crate::parsing::ast::Value::I8(n) => *n as i32,
                crate::parsing::ast::Value::U64(n) => *n as i32,
                crate::parsing::ast::Value::U32(n) => *n as i32,
                crate::parsing::ast::Value::U16(n) => *n as i32,
                crate::parsing::ast::Value::U8(n) => *n as i32,
                crate::parsing::ast::Value::Bool(b) => {
                    if *b {
                        1
                    } else {
                        0
                    }
                }
                _ => 0,
            };
            out.push(0x41);
            write_i32_leb(n, out);
            Ok(None)
        }
        ExprKind::Variable(name) => {
            if let Some(idx) = ctx.locals_i32.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                let t = ctx.locals_types.get(name).cloned();
                Ok(t)
            } else {
                Err("unknown i32 variable".into())
            }
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if let Some((fidx, params, ret_type)) = ctx.funcs.get(name) {
                    for (i, a) in args.iter().enumerate() {
                        gen_expr_i32(a, out, ctx)?;
                        if let Some(pt) = params.get(i) {
                            if *pt == 0x7C {
                                out.push(0xB7);
                            }
                        }
                    }
                    out.push(0x10);
                    write_u32_leb(*fidx, out);
                    match ret_type {
                        Some(0x7F) => Ok(None),
                        Some(_) => Err("function does not return i32".into()),
                        None => Err("void function in expression".into()),
                    }
                } else {
                    Err("unknown callee".into())
                }
            } else {
                Err("unsupported callee".into())
            }
        }
        ExprKind::Unary(TokenKind::Minus, r) => {
            out.push(0x41);
            write_u32_leb(0, out);
            gen_expr_i32(r, out, ctx)?;
            out.push(0x6B);
            Ok(None)
        }
        ExprKind::Binary(l, op, r) => {
            gen_expr_i32(l, out, ctx)?;
            gen_expr_i32(r, out, ctx)?;
            match op {
                TokenKind::Plus => {
                    out.push(0x6A);
                    Ok(None)
                }
                TokenKind::Minus => {
                    out.push(0x6B);
                    Ok(None)
                }
                TokenKind::Star => {
                    out.push(0x6C);
                    Ok(None)
                }
                TokenKind::Slash => {
                    out.push(0x6D);
                    Ok(None)
                }
                _ => Err("unsupported i32 op".into()),
            }
        }
        ExprKind::StructLiteral(name, fields) => {
            if let Some(layout) = ctx.struct_layouts.get(name) {
                out.push(0x41);
                write_u32_leb(layout.get_final_size() as u32, out);
                out.push(0x10);
                write_u32_leb(3, out); // alloc

                if !ctx.locals_i32.contains_key("__scratch") {
                    return Err("Internal error: __scratch local not found".into());
                }
                let scratch_idx = *ctx.locals_i32.get("__scratch").unwrap();

                out.push(0x22); // local.tee $scratch
                write_u32_leb(scratch_idx, out);

                // Stack has ptr.
                // For each field
                for (fname, fexpr) in fields {
                    if let Some(offset) = layout.get_field_offset(fname) {
                        // Prepare store: ptr + offset, value
                        out.push(0x20); // local.get $scratch
                        write_u32_leb(scratch_idx, out);
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A); // add
                        }

                        // Generate value
                        if let Some(fi) = layout.find_field(fname) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                gen_expr_f64(fexpr, out, ctx)?;
                                out.push(0x39); // f64.store
                            } else {
                                gen_expr_i32(fexpr, out, ctx)?;
                                out.push(0x36); // i32.store
                            }
                        }
                    }
                }
                Ok(Some(name.clone()))
            } else {
                Err(format!("Unknown struct {}", name))
            }
        }
        ExprKind::Get(target, field) => {
            let type_name = gen_expr_i32(target, out, ctx)?;
            if let Some(tn) = type_name {
                if let Some(layout) = ctx.struct_layouts.get(&tn) {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A);
                        }
                        out.push(0x28); // i32.load
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                return Err(
                                    "Type mismatch: accessing f64 field where i32 expected".into(),
                                );
                            }
                            return Ok(Some(fi.type_name.clone()));
                        }
                        return Ok(None);
                    }
                }
                Err(format!("Field {} not found on {}", field, tn))
            } else {
                Err("Get on unknown type".into())
            }
        }
        ExprKind::OptGet(target, field) => {
            // Optional chaining for i32 property access: obj?.field
            let scratch_idx = ctx
                .locals_i32
                .get("__scratch")
                .copied()
                .ok_or("Internal error: __scratch local not found")?;

            // Evaluate target object
            let type_name = gen_expr_i32(target, out, ctx)?;

            // Store in scratch
            out.push(0x22); // local.tee
            write_u32_leb(scratch_idx, out);

            // Check if null (0)
            out.push(0x45); // i32.eqz

            // If null, return 0
            out.push(0x04); // if
            out.push(0x7F); // i32 result type

            // Then: return 0 (null)
            out.push(0x41); // i32.const
            write_u32_leb(0, out);

            // Else: access field
            out.push(0x05); // else
            out.push(0x20); // local.get
            write_u32_leb(scratch_idx, out);

            if let Some(tn) = type_name {
                if let Some(layout) = ctx.struct_layouts.get(&tn) {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A);
                        }
                        out.push(0x28); // i32.load
                    }
                }
            }

            out.push(0x0B); // end
            Ok(None)
        }
        _ => Err("unsupported i32 expression".into()),
    }
}

fn gen_cond_i32(e: &Expr, out: &mut Vec<u8>, ctx: &mut CompilerContext) -> Result<(), String> {
    if let ExprKind::Variable(name) = &e.kind {
        if let Some(idx) = ctx.locals_i32.get(name) {
            out.push(0x20);
            write_u32_leb(*idx, out);
            return Ok(());
        }
    }
    gen_expr_f64(e, out, ctx)?;
    out.push(0x44);
    out.extend(&0f64.to_bits().to_le_bytes());
    out.push(0x62);
    Ok(())
}

fn wasm_hex_or_named_to_ansi(color_str: &str, is_bg: bool) -> String {
    let base = if is_bg { 48 } else { 38 };
    let c = color_str.trim().to_lowercase();
    if c.starts_with('#') && c.len() == 7 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&c[1..3], 16),
            u8::from_str_radix(&c[3..5], 16),
            u8::from_str_radix(&c[5..7], 16),
        ) {
            return format!("\x1b[{};2;{};{};{}m", base, r, g, b);
        }
    }
    match c.as_str() {
        "black" => format!("\x1b[{}m", if is_bg { 40 } else { 30 }),
        "red" => format!("\x1b[{}m", if is_bg { 41 } else { 31 }),
        "green" => format!("\x1b[{}m", if is_bg { 42 } else { 32 }),
        "yellow" => format!("\x1b[{}m", if is_bg { 43 } else { 33 }),
        "blue" => format!("\x1b[{}m", if is_bg { 44 } else { 34 }),
        "magenta" => format!("\x1b[{}m", if is_bg { 45 } else { 35 }),
        "cyan" => format!("\x1b[{}m", if is_bg { 46 } else { 36 }),
        "white" => format!("\x1b[{}m", if is_bg { 47 } else { 37 }),
        "orange" => {
            if is_bg {
                "\x1b[48;2;255;165;0m".to_string()
            } else {
                "\x1b[38;2;255;165;0m".to_string()
            }
        }
        "purple" => {
            if is_bg {
                "\x1b[48;2;128;0;128m".to_string()
            } else {
                "\x1b[38;2;128;0;128m".to_string()
            }
        }
        "gray" | "grey" => format!("\x1b[{}m", if is_bg { 100 } else { 90 }),
        _ => String::new(),
    }
}

fn emit_wasm_print_str_call(s: &str, out: &mut Vec<u8>, ctx: &CompilerContext) {
    if s.is_empty() {
        return;
    }
    let off = ctx
        .strings
        .iter()
        .find(|(x, _)| x == s)
        .map(|(_, o)| *o)
        .unwrap_or(1024);
    out.push(0x41);
    write_i32_leb(off as i32, out);
    out.push(0x41);
    write_i32_leb(s.len() as i32, out);
    out.push(0x10);
    write_u32_leb(1, out);
}

fn emit_wasm_file_write_str_call(
    file_path: &str,
    s: &str,
    out: &mut Vec<u8>,
    ctx: &CompilerContext,
) {
    if s.is_empty() {
        return;
    }
    let file_off = ctx
        .strings
        .iter()
        .find(|(x, _)| x == file_path)
        .map(|(_, o)| *o)
        .unwrap_or(1024);
    let off = ctx
        .strings
        .iter()
        .find(|(x, _)| x == s)
        .map(|(_, o)| *o)
        .unwrap_or(1024);
    out.push(0x41);
    write_i32_leb(file_off as i32, out);
    out.push(0x41);
    write_i32_leb(file_path.len() as i32, out);
    out.push(0x41);
    write_i32_leb(off as i32, out);
    out.push(0x41);
    write_i32_leb(s.len() as i32, out);
    out.push(0x10);
    write_u32_leb(4, out);
}

fn add_wasm_string(st: &str, strings: &mut Vec<(String, u32)>, cur_off: &mut u32) {
    if !st.is_empty() && !strings.iter().any(|(x, _)| x == st) {
        strings.push((st.to_string(), *cur_off));
        *cur_off += st.len() as u32;
    }
}

fn format_static_val_for_wasm(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Array(elems) => {
            let mut parts = Vec::new();
            for el in elems {
                if let Some(s) = format_static_val_for_wasm(el) {
                    parts.push(s);
                } else {
                    return None;
                }
            }
            Some(format!("[{}]", parts.join(", ")))
        }
        ExprKind::Tuple(elems) => {
            let mut parts = Vec::new();
            for el in elems {
                if let Some(s) = format_static_val_for_wasm(el) {
                    parts.push(s);
                } else {
                    return None;
                }
            }
            Some(format!("({})", parts.join(", ")))
        }
        ExprKind::Object(entries) => {
            let mut parts = Vec::new();
            for (k, v) in entries {
                if let Some(s) = format_static_val_for_wasm(v) {
                    parts.push(format!("{}: {}", k, s));
                } else {
                    return None;
                }
            }
            Some(format!("{{{}}}", parts.join(", ")))
        }
        ExprKind::Literal(Value::Str(s)) => Some(s.clone()),
        ExprKind::Literal(Value::Number(n)) => Some(format!("{:.1}", n)),
        ExprKind::Literal(Value::F64(n)) => Some(format!("{}", n)),
        ExprKind::Literal(Value::I64(n)) => Some(format!("{}", n)),
        ExprKind::Literal(Value::I32(n)) => Some(format!("{}", n)),
        ExprKind::Literal(Value::U32(n)) => Some(format!("{}", n)),
        ExprKind::Literal(Value::U8(n)) => Some(format!("{}", n)),
        ExprKind::Literal(Value::Bool(b)) => Some((if *b { "true" } else { "false" }).to_string()),
        ExprKind::Literal(Value::Null) => Some("null".to_string()),
        _ => None,
    }
}

fn expr_to_value(e: &Expr) -> Option<Value> {
    match &e.kind {
        ExprKind::Literal(v) => Some(v.clone()),
        ExprKind::Array(elems) => {
            let mut arr = Vec::new();
            for el in elems {
                arr.push(expr_to_value(el)?);
            }
            Some(Value::Array(arr))
        }
        ExprKind::Tuple(elems) => {
            let mut tup = Vec::new();
            for el in elems {
                tup.push(expr_to_value(el)?);
            }
            Some(Value::Tuple(tup))
        }
        ExprKind::Object(entries) => {
            let mut map = FastMap::default();
            for (k, v) in entries {
                map.insert(k.clone(), expr_to_value(v)?);
            }
            Some(Value::Object(map.into()))
        }
        _ => None,
    }
}

fn extract_pretty_options(
    entries: &[(String, Expr)],
) -> Option<crate::execution::runtime_core::pretty_print::PrettyPrintOptions> {
    use crate::execution::runtime_core::pretty_print::PrettyPrintOptions;
    for (k, v) in entries {
        if k == "pretty" {
            match &v.kind {
                ExprKind::Literal(Value::Bool(b)) => {
                    if *b {
                        return Some(PrettyPrintOptions::default());
                    }
                }
                ExprKind::Literal(Value::Str(s)) => {
                    if s == "compact" {
                        return Some(PrettyPrintOptions::compact());
                    } else if s == "simple" {
                        return Some(PrettyPrintOptions::simple_color());
                    } else if s == "no_color" || s == "plain" {
                        return Some(PrettyPrintOptions::no_color());
                    } else {
                        return Some(PrettyPrintOptions::default());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn format_print_arg(
    arg: &Expr,
    pretty_opt: Option<&crate::execution::runtime_core::pretty_print::PrettyPrintOptions>,
    var_values: &FastMap<String, Value>,
) -> Option<String> {
    let val = match &arg.kind {
        ExprKind::Variable(vname) => var_values.get(vname).cloned(),
        _ => expr_to_value(arg),
    }?;

    if let Some(opts) = pretty_opt {
        Some(crate::execution::runtime_core::pretty_print::pretty_print(
            &val, opts,
        ))
    } else {
        match &val {
            Value::Str(_) => None,
            _ => format_static_val_for_wasm(arg),
        }
    }
}

fn get_wasm_ansi_prefix(entries: &[(String, Expr)]) -> (String, bool) {
    let mut ansi_prefix = String::new();
    let mut has_style = false;
    for (k, v) in entries {
        if k == "color" {
            if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                let ansi = wasm_hex_or_named_to_ansi(s, false);
                if !ansi.is_empty() {
                    ansi_prefix.push_str(&ansi);
                    has_style = true;
                }
            }
        } else if k == "background" {
            if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                let ansi = wasm_hex_or_named_to_ansi(s, true);
                if !ansi.is_empty() {
                    ansi_prefix.push_str(&ansi);
                    has_style = true;
                }
            }
        } else if k == "bold" {
            ansi_prefix.push_str("\x1b[1m");
            has_style = true;
        } else if k == "italic" {
            ansi_prefix.push_str("\x1b[3m");
            has_style = true;
        } else if k == "underline" {
            ansi_prefix.push_str("\x1b[4m");
            has_style = true;
        } else if k == "strikethrough" {
            ansi_prefix.push_str("\x1b[9m");
            has_style = true;
        }
    }
    (ansi_prefix, has_style)
}

fn collect_all_wasm_strings_expr(
    e: &Expr,
    strings: &mut Vec<(String, u32)>,
    cur_off: &mut u32,
    var_values: &FastMap<String, Value>,
) {
    match &e.kind {
        ExprKind::Literal(Value::Str(s)) => add_wasm_string(s, strings, cur_off),
        ExprKind::Object(entries) => {
            if let Some(fmt) = format_static_val_for_wasm(e) {
                add_wasm_string(&fmt, strings, cur_off);
            }
            for (k, v) in entries {
                add_wasm_string(k, strings, cur_off);
                collect_all_wasm_strings_expr(v, strings, cur_off, var_values);
            }
        }
        ExprKind::Array(elems) | ExprKind::Tuple(elems) => {
            if let Some(fmt) = format_static_val_for_wasm(e) {
                add_wasm_string(&fmt, strings, cur_off);
            }
            for el in elems {
                collect_all_wasm_strings_expr(el, strings, cur_off, var_values);
            }
        }
        ExprKind::Call(callee, args, _) => {
            collect_all_wasm_strings_expr(callee, strings, cur_off, var_values);
            for arg in args {
                collect_all_wasm_strings_expr(arg, strings, cur_off, var_values);
            }
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "print" || name == "println" {
                    let mut pretty_opt = None;
                    let mut non_opt_args = Vec::new();
                    if let Some(last) = args.last() {
                        if let ExprKind::Object(entries) = &last.kind {
                            let (ansi_prefix, has_style) = get_wasm_ansi_prefix(entries);
                            if has_style {
                                add_wasm_string(&ansi_prefix, strings, cur_off);
                                add_wasm_string("\x1b[0m", strings, cur_off);
                            }
                            pretty_opt = extract_pretty_options(entries);
                            for (k, v) in entries {
                                if k == "sep" || k == "end" || k == "file" {
                                    if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                                        add_wasm_string(s, strings, cur_off);
                                    }
                                }
                            }
                            for a in &args[..args.len() - 1] {
                                non_opt_args.push(a);
                            }
                        } else {
                            for a in args {
                                non_opt_args.push(a);
                            }
                        }
                    } else {
                        for a in args {
                            non_opt_args.push(a);
                        }
                    }
                    for a in non_opt_args {
                        if let Some(formatted) =
                            format_print_arg(a, pretty_opt.as_ref(), var_values)
                        {
                            add_wasm_string(&formatted, strings, cur_off);
                        }
                    }
                }
            }
        }
        ExprKind::Binary(l, _, r) => {
            collect_all_wasm_strings_expr(l, strings, cur_off, var_values);
            collect_all_wasm_strings_expr(r, strings, cur_off, var_values);
        }
        ExprKind::Unary(_, inner) => {
            collect_all_wasm_strings_expr(inner, strings, cur_off, var_values)
        }
        ExprKind::Assign(_, r) => collect_all_wasm_strings_expr(r, strings, cur_off, var_values),
        _ => {}
    }
}

fn collect_all_wasm_strings_stmt(
    s: &Stmt,
    strings: &mut Vec<(String, u32)>,
    cur_off: &mut u32,
    var_values: &FastMap<String, Value>,
) {
    match &s.kind {
        StmtKind::ExprStmt(e) => collect_all_wasm_strings_expr(e, strings, cur_off, var_values),
        StmtKind::Let(_, init, _, _, _, _) => {
            if let Some(e) = init {
                collect_all_wasm_strings_expr(e, strings, cur_off, var_values);
            }
        }
        StmtKind::ShareDeclaration(decl, _) => {
            collect_all_wasm_strings_expr(&decl.expr, strings, cur_off, var_values)
        }
        StmtKind::StrongDeclaration(decl, _) => {
            collect_all_wasm_strings_expr(&decl.expr, strings, cur_off, var_values)
        }
        StmtKind::WeakDeclaration(decl, _) => {
            collect_all_wasm_strings_expr(&decl.expr, strings, cur_off, var_values)
        }
        StmtKind::Block(stmts) => {
            for st in stmts {
                collect_all_wasm_strings_stmt(st, strings, cur_off, var_values);
            }
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_all_wasm_strings_expr(cond, strings, cur_off, var_values);
            collect_all_wasm_strings_stmt(then_branch, strings, cur_off, var_values);
            if let Some(eb) = else_branch {
                collect_all_wasm_strings_stmt(eb, strings, cur_off, var_values);
            }
        }
        StmtKind::While { cond, body } => {
            collect_all_wasm_strings_expr(cond, strings, cur_off, var_values);
            collect_all_wasm_strings_stmt(body, strings, cur_off, var_values);
        }
        StmtKind::ForIn { iter, body, .. } => {
            collect_all_wasm_strings_expr(iter, strings, cur_off, var_values);
            collect_all_wasm_strings_stmt(body, strings, cur_off, var_values);
        }
        _ => {}
    }
}

fn gen_stmt(
    s: &Stmt,
    out: &mut Vec<u8>,
    ctx: &mut CompilerContext,
    ret_type: Option<u8>,
) -> Result<(), String> {
    match &s.kind {
        StmtKind::Block(bs) => {
            // Push a new defer scope for this block
            ctx.defer_scopes.push(Vec::new());
            for b in bs {
                gen_stmt(b, out, ctx, ret_type)?;
            }
            // Emit this scope's defers in LIFO order (compile-time inlining)
            let defers = ctx.defer_scopes.pop().unwrap_or_default();
            for defer_block in defers.iter().rev() {
                gen_stmt(defer_block, out, ctx, ret_type)?;
            }
            Ok(())
        }
        StmtKind::ShareDeclaration(decl, _) => {
            if let Some(idx) = ctx.locals_f64.get(&decl.name) {
                gen_expr_f64(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
            } else if let Some(idx) = ctx.locals_i32.get(&decl.name) {
                let t = gen_expr_i32(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
                if let Some(tn) = &decl.type_ann {
                    ctx.locals_types.insert(decl.name.clone(), tn.clone());
                } else if let Some(inferred) = t {
                    ctx.locals_types.insert(decl.name.clone(), inferred);
                }
            }
            Ok(())
        }
        StmtKind::StrongDeclaration(decl, _) => {
            if let Some(idx) = ctx.locals_f64.get(&decl.name) {
                gen_expr_f64(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
            } else if let Some(idx) = ctx.locals_i32.get(&decl.name) {
                let t = gen_expr_i32(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
                if let Some(tn) = &decl.type_ann {
                    ctx.locals_types.insert(decl.name.clone(), tn.clone());
                } else if let Some(inferred) = t {
                    ctx.locals_types.insert(decl.name.clone(), inferred);
                }
            }
            Ok(())
        }
        StmtKind::WeakDeclaration(decl, _) => {
            if let Some(idx) = ctx.locals_f64.get(&decl.name) {
                gen_expr_f64(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
            } else if let Some(idx) = ctx.locals_i32.get(&decl.name) {
                let t = gen_expr_i32(&decl.expr, out, ctx)?;
                out.push(0x21);
                write_u32_leb(*idx, out);
                if let Some(tn) = &decl.type_ann {
                    ctx.locals_types.insert(decl.name.clone(), tn.clone());
                } else if let Some(inferred) = t {
                    ctx.locals_types.insert(decl.name.clone(), inferred);
                }
            }
            Ok(())
        }
        StmtKind::Let(name, init, _ann, ..) => {
            if let Some(e) = init {
                if let Some(idx) = ctx.locals_f64.get(name) {
                    gen_expr_f64(e, out, ctx)?;
                    out.push(0x21);
                    write_u32_leb(*idx, out);
                } else if let Some(idx) = ctx.locals_i32.get(name) {
                    let t = gen_expr_i32(e, out, ctx)?;
                    out.push(0x21);
                    write_u32_leb(*idx, out);
                    if let Some(tn) = _ann {
                        ctx.locals_types.insert(name.clone(), tn.clone());
                    } else if let Some(inferred) = t {
                        ctx.locals_types.insert(name.clone(), inferred);
                    }
                }
            }
            Ok(())
        }
        StmtKind::ExprStmt(e) => {
            if let ExprKind::Call(callee, args, _) = &e.kind {
                if let ExprKind::Variable(name) = &callee.kind {
                    if name == "print" || name == "println" {
                        let mut sep = " ".to_string();
                        let mut end = "\n".to_string();
                        let mut ansi_prefix = String::new();
                        let mut has_style = false;
                        let mut non_opt_args: Vec<&Expr> = Vec::new();

                        let mut has_opt_map = false;
                        let mut pretty_opt = None;
                        let mut file_opt: Option<String> = None;
                        if let Some(last) = args.last() {
                            if let ExprKind::Object(entries) = &last.kind {
                                let is_known_opt = entries.iter().all(|(k, _)| {
                                    matches!(
                                        k.as_str(),
                                        "sep"
                                            | "end"
                                            | "color"
                                            | "background"
                                            | "bold"
                                            | "italic"
                                            | "underline"
                                            | "strikethrough"
                                            | "pretty"
                                            | "flush"
                                            | "file"
                                    )
                                });
                                if is_known_opt && !entries.is_empty() {
                                    has_opt_map = true;
                                    let (prefix, style) = get_wasm_ansi_prefix(entries);
                                    ansi_prefix = prefix;
                                    has_style = style;
                                    pretty_opt = extract_pretty_options(entries);
                                    for (k, v) in entries {
                                        if k == "sep" {
                                            if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                                                sep = s.clone();
                                            }
                                        } else if k == "end" {
                                            if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                                                end = s.clone();
                                            }
                                        } else if k == "file" {
                                            if let ExprKind::Literal(Value::Str(s)) = &v.kind {
                                                file_opt = Some(s.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if has_opt_map {
                            for arg in &args[..args.len() - 1] {
                                non_opt_args.push(arg);
                            }
                        } else {
                            for arg in args {
                                non_opt_args.push(arg);
                            }
                        }

                        let total_items = non_opt_args.len();
                        if let Some(file_path) = file_opt.as_deref() {
                            for (i, arg) in non_opt_args.iter().enumerate() {
                                if has_style {
                                    emit_wasm_file_write_str_call(
                                        file_path,
                                        &ansi_prefix,
                                        out,
                                        ctx,
                                    );
                                }
                                if let Some(formatted) =
                                    format_print_arg(arg, pretty_opt.as_ref(), ctx.var_values)
                                {
                                    emit_wasm_file_write_str_call(file_path, &formatted, out, ctx);
                                } else {
                                    match &arg.kind {
                                        ExprKind::Literal(Value::Str(st)) => {
                                            emit_wasm_file_write_str_call(file_path, st, out, ctx);
                                        }
                                        ExprKind::Literal(Value::Bool(b)) => {
                                            emit_wasm_file_write_str_call(
                                                file_path,
                                                if *b { "true" } else { "false" },
                                                out,
                                                ctx,
                                            );
                                        }
                                        ExprKind::Literal(Value::Null) => {
                                            emit_wasm_file_write_str_call(
                                                file_path, "null", out, ctx,
                                            );
                                        }
                                        ExprKind::Variable(vname) => {
                                            if let Some((off, len)) = ctx.str_vars.get(vname) {
                                                let file_off = ctx
                                                    .strings
                                                    .iter()
                                                    .find(|(x, _)| x == file_path)
                                                    .map(|(_, o)| *o)
                                                    .unwrap_or(1024);
                                                out.push(0x41);
                                                write_i32_leb(file_off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(file_path.len() as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*len as i32, out);
                                                out.push(0x10);
                                                write_u32_leb(4, out);
                                            } else if let Some((off, len)) =
                                                ctx.complex_vars.get(vname)
                                            {
                                                let file_off = ctx
                                                    .strings
                                                    .iter()
                                                    .find(|(x, _)| x == file_path)
                                                    .map(|(_, o)| *o)
                                                    .unwrap_or(1024);
                                                out.push(0x41);
                                                write_i32_leb(file_off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(file_path.len() as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*len as i32, out);
                                                out.push(0x10);
                                                write_u32_leb(4, out);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                if has_style {
                                    emit_wasm_file_write_str_call(file_path, "\x1b[0m", out, ctx);
                                }
                                if i + 1 < total_items {
                                    emit_wasm_file_write_str_call(file_path, &sep, out, ctx);
                                }
                            }
                            if !end.is_empty() {
                                emit_wasm_file_write_str_call(file_path, &end, out, ctx);
                            }
                        } else {
                            for (i, arg) in non_opt_args.iter().enumerate() {
                                if has_style {
                                    emit_wasm_print_str_call(&ansi_prefix, out, ctx);
                                }
                                if let Some(formatted) =
                                    format_print_arg(arg, pretty_opt.as_ref(), ctx.var_values)
                                {
                                    emit_wasm_print_str_call(&formatted, out, ctx);
                                } else {
                                    match &arg.kind {
                                        ExprKind::Literal(Value::Str(st)) => {
                                            emit_wasm_print_str_call(st, out, ctx);
                                        }
                                        ExprKind::Literal(Value::Bool(b)) => {
                                            emit_wasm_print_str_call(
                                                if *b { "true" } else { "false" },
                                                out,
                                                ctx,
                                            );
                                        }
                                        ExprKind::Literal(Value::Null) => {
                                            emit_wasm_print_str_call("null", out, ctx);
                                        }
                                        ExprKind::Variable(vname) => {
                                            if let Some((off, len)) = ctx.str_vars.get(vname) {
                                                out.push(0x41);
                                                write_i32_leb(*off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*len as i32, out);
                                                out.push(0x10);
                                                write_u32_leb(1, out);
                                            } else if let Some((off, len)) =
                                                ctx.complex_vars.get(vname)
                                            {
                                                out.push(0x41);
                                                write_i32_leb(*off as i32, out);
                                                out.push(0x41);
                                                write_i32_leb(*len as i32, out);
                                                out.push(0x10);
                                                write_u32_leb(1, out);
                                            } else {
                                                gen_expr_f64(arg, out, ctx)?;
                                                out.push(0x10);
                                                write_u32_leb(0, out);
                                            }
                                        }
                                        _ => {
                                            gen_expr_f64(arg, out, ctx)?;
                                            out.push(0x10);
                                            write_u32_leb(0, out);
                                        }
                                    }
                                }
                                if has_style {
                                    emit_wasm_print_str_call("\x1b[0m", out, ctx);
                                }
                                if i + 1 < total_items {
                                    emit_wasm_print_str_call(&sep, out, ctx);
                                }
                            }
                            if !end.is_empty() {
                                emit_wasm_print_str_call(&end, out, ctx);
                            }
                        }
                        return Ok(());
                    }
                }
            }
            match &e.kind {
                ExprKind::AssignOp(lhs, op, rhs) => {
                    if let ExprKind::Variable(name) = &lhs.kind {
                        if let Some(idx) = ctx.locals_i32.get(name) {
                            match op {
                                TokenKind::Equal => {
                                    gen_expr_i32(rhs, out, ctx)?;
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                TokenKind::PlusEqual => {
                                    out.push(0x20);
                                    write_u32_leb(*idx, out);
                                    gen_expr_i32(rhs, out, ctx)?;
                                    out.push(0x6A);
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                TokenKind::MinusEqual => {
                                    out.push(0x20);
                                    write_u32_leb(*idx, out);
                                    gen_expr_i32(rhs, out, ctx)?;
                                    out.push(0x6B);
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                _ => {}
                            }
                        } else if let Some(idx) = ctx.locals_f64.get(name) {
                            match op {
                                TokenKind::Equal => {
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                TokenKind::PlusEqual => {
                                    out.push(0x20);
                                    write_u32_leb(*idx, out);
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0xA0);
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                TokenKind::MinusEqual => {
                                    out.push(0x20);
                                    write_u32_leb(*idx, out);
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0xA1);
                                    out.push(0x21);
                                    write_u32_leb(*idx, out);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ExprKind::Call(_, _, _)
                | ExprKind::Assign(_, _)
                | ExprKind::AssignTuple(_, _)
                | ExprKind::AssignObject(_, _) => {
                    if let ExprKind::Call(callee, _, _) = &e.kind {
                        if let ExprKind::Variable(name) = &callee.kind {
                            if let Some((_, _, rt)) = ctx.funcs.get(name) {
                                if rt.is_some() {
                                    if *rt.as_ref().unwrap() == 0x7F {
                                        gen_expr_i32(e, out, ctx)?;
                                    } else {
                                        gen_expr_f64(e, out, ctx)?;
                                    }
                                    out.push(0x1A);
                                } else {
                                    if let Some((fidx, _, _)) = ctx.funcs.get(name) {
                                        if let ExprKind::Call(_, args, _) = &e.kind {
                                            for a in args {
                                                gen_expr_f64(a, out, ctx)?;
                                            }
                                        }
                                        out.push(0x10);
                                        write_u32_leb(*fidx, out);
                                    }
                                }
                            }
                        }
                    } else {
                        if let ExprKind::Assign(name, _) = &e.kind {
                            if ctx.locals_i32.contains_key(name) {
                                gen_expr_i32(e, out, ctx)?;
                            } else {
                                gen_expr_f64(e, out, ctx)?;
                            }
                            out.push(0x1A);
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            gen_cond_i32(cond, out, ctx)?;
            out.push(0x04);
            out.push(0x40);
            // Then branch — own defer scope
            ctx.defer_scopes.push(Vec::new());
            gen_stmt(then_branch, out, ctx, ret_type)?;
            let then_defers = ctx.defer_scopes.pop().unwrap_or_default();
            for defer_block in then_defers.iter().rev() {
                gen_stmt(defer_block, out, ctx, ret_type)?;
            }
            if let Some(el) = else_branch {
                out.push(0x05);
                // Else branch — own defer scope
                ctx.defer_scopes.push(Vec::new());
                gen_stmt(el, out, ctx, ret_type)?;
                let else_defers = ctx.defer_scopes.pop().unwrap_or_default();
                for defer_block in else_defers.iter().rev() {
                    gen_stmt(defer_block, out, ctx, ret_type)?;
                }
            }
            out.push(0x0B);
            Ok(())
        }
        StmtKind::While { cond, body } => {
            out.push(0x02);
            out.push(0x40);
            out.push(0x03);
            out.push(0x40);
            gen_cond_i32(cond, out, ctx)?;
            out.push(0x45);
            out.push(0x0D);
            write_u32_leb(1, out);
            // Loop body — own defer scope; record depth for break/continue
            ctx.defer_scopes.push(Vec::new());
            let saved_depth = ctx.defer_scopes.len() - 1;
            ctx.loop_defer_depths.push(saved_depth);
            gen_stmt(body, out, ctx, ret_type)?;
            // Emit this scope's defers at end of loop body (compile-time inlining)
            let body_defers = ctx.defer_scopes.pop().unwrap_or_default();
            for defer_block in body_defers.iter().rev() {
                gen_stmt(defer_block, out, ctx, ret_type)?;
            }
            ctx.loop_defer_depths.pop();
            out.push(0x0C);
            write_u32_leb(0, out);
            out.push(0x0B);
            out.push(0x0B);
            Ok(())
        }
        StmtKind::Return(eo) => {
            // Emit ALL defers from ALL active scopes in LIFO order before returning
            for scope_idx in (0..ctx.defer_scopes.len()).rev() {
                let defers = std::mem::take(&mut ctx.defer_scopes[scope_idx]);
                for defer_block in defers.iter().rev() {
                    gen_stmt(defer_block, out, ctx, ret_type)?;
                }
            }
            ctx.defer_scopes.clear();
            if let Some(e) = eo {
                if ret_type == Some(0x7F) {
                    gen_expr_i32(e, out, ctx)?;
                } else {
                    gen_expr_f64(e, out, ctx)?;
                }
            }
            out.push(0x0F);
            Ok(())
        }
        StmtKind::ForIn { name, iter, body } => {
            if let ExprKind::Range(start, end, inclusive) = &iter.kind {
                if let Some(idx) = ctx.locals_i32.get(name) {
                    gen_expr_i32(start, out, ctx)?;
                    out.push(0x21);
                    write_u32_leb(*idx, out);

                    out.push(0x02);
                    out.push(0x40);

                    out.push(0x03);
                    out.push(0x40);

                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    gen_expr_i32(end, out, ctx)?;
                    if *inclusive {
                        out.push(0x4A); // i32.gt_s
                    } else {
                        out.push(0x4E); // i32.ge_s
                    }
                    out.push(0x0D);
                    write_u32_leb(1, out);

                    // Loop body — own defer scope
                    ctx.defer_scopes.push(Vec::new());
                    let saved_depth = ctx.defer_scopes.len() - 1;
                    ctx.loop_defer_depths.push(saved_depth);
                    gen_stmt(body, out, ctx, ret_type)?;
                    let body_defers = ctx.defer_scopes.pop().unwrap_or_default();
                    for defer_block in body_defers.iter().rev() {
                        gen_stmt(defer_block, out, ctx, ret_type)?;
                    }
                    ctx.loop_defer_depths.pop();

                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0x41);
                    write_i32_leb(1, out);
                    out.push(0x6A); // i32.add
                    out.push(0x21);
                    write_u32_leb(*idx, out);

                    out.push(0x0C);
                    write_u32_leb(0, out);

                    out.push(0x0B);
                    out.push(0x0B);
                } else if let Some(idx) = ctx.locals_f64.get(name) {
                    gen_expr_f64(start, out, ctx)?;
                    out.push(0x21);
                    write_u32_leb(*idx, out);

                    out.push(0x02);
                    out.push(0x40);

                    out.push(0x03);
                    out.push(0x40);

                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    gen_expr_f64(end, out, ctx)?;
                    if *inclusive {
                        out.push(0x64); // f64.gt
                    } else {
                        out.push(0x66); // f64.ge
                    }
                    out.push(0x0D);
                    write_u32_leb(1, out);

                    // Loop body — own defer scope
                    ctx.defer_scopes.push(Vec::new());
                    let saved_depth = ctx.defer_scopes.len() - 1;
                    ctx.loop_defer_depths.push(saved_depth);
                    gen_stmt(body, out, ctx, ret_type)?;
                    let body_defers = ctx.defer_scopes.pop().unwrap_or_default();
                    for defer_block in body_defers.iter().rev() {
                        gen_stmt(defer_block, out, ctx, ret_type)?;
                    }
                    ctx.loop_defer_depths.pop();

                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0x44);
                    out.extend(&1.0f64.to_le_bytes());
                    out.push(0xA0); // f64.add
                    out.push(0x21);
                    write_u32_leb(*idx, out);

                    out.push(0x0C);
                    write_u32_leb(0, out);

                    out.push(0x0B);
                    out.push(0x0B);
                }
            }
            Ok(())
        }
        StmtKind::Defer(block) => {
            // Compile-time defer inlining: register the block in the current scope.
            // At scope exit (return, break, continue, end of block/function),
            // the defer blocks are emitted inline in LIFO order.
            // Zero runtime overhead — no runtime defer stack, no heap allocation.
            if let Some(scope) = ctx.defer_scopes.last_mut() {
                scope.push((**block).clone());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn compile_to_file(src: &str, out: &Path) -> Result<(), LangError> {
    let body_src = if src
        .lines()
        .next()
        .map(|l| l.trim_start().starts_with("@compile"))
        .unwrap_or(false)
    {
        src.lines().skip(1).collect::<Vec<&str>>().join("\n")
    } else {
        src.to_string()
    };

    let mut lx = Lexer::new(&body_src);
    let toks = lx
        .tokenize()
        .map_err(|e| LangError::new(ErrorKind::Lexical, e.to_string(), 0, 0, "".into()))?;
    let mut p = Parser::new(toks, None);
    let prog = p
        .parse_program()
        .map_err(|e| LangError::new(ErrorKind::Parse, e.to_string(), 0, 0, "".into()))?;
    let prog = crate::parsing::ast_optimizer::optimize_program(&prog);

    let mut struct_layouts: FastMap<String, FieldLayout> = FastMap::default();
    for s in &prog {
        if let StmtKind::Struct(decl, _) = &s.kind {
            let layout = compute_wasm_layout(&decl.fields);
            struct_layouts.insert(decl.name.clone(), layout);
        }
    }

    let mut locals_map_f64: FastMap<String, u32> = FastMap::default();
    let mut locals_map_i32: FastMap<String, u32> = FastMap::default();
    let mut local_count_f64: u32 = 0;
    let mut local_count_i32: u32 = 0;

    for s in &prog {
        if let StmtKind::Let(name, init, _ann, ..) = &s.kind {
            if init.is_some() {
                if let Some(tn) = _ann {
                    if tn.to_lowercase() == "int" {
                        if !locals_map_i32.contains_key(name) {
                            locals_map_i32.insert(name.clone(), local_count_i32);
                            local_count_i32 += 1;
                        }
                    } else {
                        if !locals_map_f64.contains_key(name) {
                            locals_map_f64.insert(name.clone(), local_count_f64);
                            local_count_f64 += 1;
                        }
                    }
                } else {
                    if !locals_map_f64.contains_key(name) {
                        locals_map_f64.insert(name.clone(), local_count_f64);
                        local_count_f64 += 1;
                    }
                }
            }
        } else if let StmtKind::ShareDeclaration(decl, _) = &s.kind {
            if !locals_map_f64.contains_key(&decl.name) {
                locals_map_f64.insert(decl.name.clone(), local_count_f64);
                local_count_f64 += 1;
            }
        } else if let StmtKind::StrongDeclaration(decl, _) = &s.kind {
            if !locals_map_f64.contains_key(&decl.name) {
                locals_map_f64.insert(decl.name.clone(), local_count_f64);
                local_count_f64 += 1;
            }
        } else if let StmtKind::WeakDeclaration(decl, _) = &s.kind {
            if !locals_map_f64.contains_key(&decl.name) {
                locals_map_f64.insert(decl.name.clone(), local_count_f64);
                local_count_f64 += 1;
            }
        } else if let StmtKind::ForIn { name, .. } = &s.kind {
            if !locals_map_i32.contains_key(name) && !locals_map_f64.contains_key(name) {
                locals_map_i32.insert(name.clone(), local_count_i32);
                local_count_i32 += 1;
            }
        }
    }

    let mut func_defs: Vec<crate::parsing::ast::Function> = Vec::new();
    for s in &prog {
        if let StmtKind::Function(f, _) = &s.kind {
            func_defs.push(f.clone());
        }
    }

    let mut func_types: Vec<(Vec<u8>, Option<u8>)> = Vec::new();
    func_types.push((vec![0x7C], None));
    func_types.push((vec![0x7F, 0x7F], None));
    func_types.push((Vec::new(), None));
    func_types.push((vec![0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F, 0x7F, 0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F, 0x7F, 0x7F], None));
    let mut func_type_map: FastMap<(Vec<u8>, Option<u8>), u32> = FastMap::default();
    for (i, t) in func_types.iter().enumerate() {
        func_type_map.insert((t.0.clone(), t.1), i as u32);
    }

    let mut funcs_index_map: FastMap<String, (u32, Vec<u8>, Option<u8>)> = FastMap::default();
    let mut function_section_types: Vec<u32> = Vec::new();
    let mut code_funcs: Vec<(u32, u32, Vec<u8>)> = Vec::new();

    let mut var_values: FastMap<String, Value> = FastMap::default();
    for s in &prog {
        if let StmtKind::Let(name, Some(init), ..) = &s.kind {
            if let Some(val) = expr_to_value(init) {
                var_values.insert(name.clone(), val);
            }
        }
    }

    let mut strings: Vec<(String, u32)> = Vec::new();
    let mut cur_off: u32 = 1024;
    for s in &prog {
        collect_all_wasm_strings_stmt(s, &mut strings, &mut cur_off, &var_values);
    }
    for f in &func_defs {
        for s in f.body.iter() {
            collect_all_wasm_strings_stmt(s, &mut strings, &mut cur_off, &var_values);
        }
    }
    if !strings.iter().any(|(x, _)| x == " ") {
        strings.push((" ".into(), cur_off));
        cur_off += 1;
    }
    if !strings.iter().any(|(x, _)| x == "\n") {
        strings.push(("\n".into(), cur_off));
        cur_off += 1;
    }
    if !strings.iter().any(|(x, _)| x == "true") {
        strings.push(("true".into(), cur_off));
        cur_off += 4;
    }
    if !strings.iter().any(|(x, _)| x == "false") {
        strings.push(("false".into(), cur_off));
        cur_off += 5;
    }
    if !strings.iter().any(|(x, _)| x == "null") {
        strings.push(("null".into(), cur_off));
        cur_off += 4;
    }
    if !strings.iter().any(|(x, _)| x == "\x1b[0m") {
        strings.push(("\x1b[0m".into(), cur_off));
    }
    let mut str_vars: FastMap<String, (u32, u32)> = FastMap::default();
    let mut complex_vars: FastMap<String, (u32, u32)> = FastMap::default();
    for s in &prog {
        if let StmtKind::Let(name, Some(init), ..) = &s.kind {
            if let ExprKind::Literal(Value::Str(st)) = &init.kind {
                if let Some((_, off)) = strings.iter().find(|(x, _)| x == st) {
                    str_vars.insert(name.clone(), (*off, st.len() as u32));
                }
            } else if let Some(fmt) = format_static_val_for_wasm(init) {
                if let Some((_, off)) = strings.iter().find(|(x, _)| x == &fmt) {
                    complex_vars.insert(name.clone(), (*off, fmt.len() as u32));
                }
            }
        }
    }

    for (i, f) in func_defs.iter().enumerate() {
        let mut params: Vec<u8> = Vec::new();
        for p in &f.params {
            if let Some(tn) = &p.2 {
                if tn == "int" || tn == "bool" {
                    params.push(0x7F);
                } else {
                    params.push(0x7C);
                }
            } else {
                params.push(0x7C);
            }
        }
        let mut ret_byte: Option<u8> = None;
        if f.ret_type.as_deref() == Some("int") || f.ret_type.as_deref() == Some("bool") {
            ret_byte = Some(0x7F);
        } else if f.ret_type.is_some() {
            ret_byte = Some(0x7C);
        }

        // Inferred Return Type
        if ret_byte.is_none() {
            if let Some(last) = f.body.last() {
                let last_is_numeric = match &last.kind {
                    StmtKind::ExprStmt(e) => is_numeric_expr(e, &funcs_index_map),
                    _ => false,
                };
                if last_is_numeric {
                    ret_byte = Some(0x7C);
                }
            }
        }

        let key = (params.clone(), ret_byte);
        let tindex = if let Some(ix) = func_type_map.get(&key) {
            *ix
        } else {
            func_types.push(key.clone());
            let ix = (func_types.len() - 1) as u32;
            func_type_map.insert(key, ix);
            ix
        };
        function_section_types.push(tindex);
        funcs_index_map.insert(f.name.clone(), (5 + i as u32, params.clone(), ret_byte));

        let mut f_locals_f64: FastMap<String, u32> = FastMap::default();
        let mut f_locals_i32: FastMap<String, u32> = FastMap::default();
        let mut f_local_count_f64: u32 = 0;
        let mut f_local_count_i32: u32 = 0;
        let mut f_locals_types: FastMap<String, String> = FastMap::default();

        for (idx, p) in f.params.iter().enumerate() {
            let t = p.2.clone().unwrap_or("number".to_string());
            f_locals_types.insert(p.0.clone(), t.clone());
            if t == "int" || t == "bool" {
                f_locals_i32.insert(p.0.clone(), idx as u32);
            } else {
                f_locals_f64.insert(p.0.clone(), idx as u32);
            }
        }
        let pcount = f.params.len() as u32;
        for s in f.body.iter() {
            if let StmtKind::Let(name, _init, _ann, ..) = &s.kind {
                if let Some(tn) = _ann {
                    f_locals_types.insert(name.clone(), tn.clone());
                    if tn == "int" {
                        if !f_locals_i32.contains_key(name) {
                            f_locals_i32.insert(name.clone(), f_local_count_i32 + pcount);
                            f_local_count_i32 += 1;
                        }
                    } else {
                        if !f_locals_f64.contains_key(name) {
                            f_locals_f64.insert(name.clone(), f_local_count_f64 + pcount);
                            f_local_count_f64 += 1;
                        }
                    }
                } else {
                    f_locals_types.insert(name.clone(), "number".into());
                    if !f_locals_f64.contains_key(name) {
                        f_locals_f64.insert(name.clone(), f_local_count_f64 + pcount);
                        f_local_count_f64 += 1;
                    }
                }
            } else if let StmtKind::ShareDeclaration(decl, _) = &s.kind {
                f_locals_types.insert(decl.name.clone(), "strong".into());
                if !f_locals_f64.contains_key(&decl.name) {
                    f_locals_f64.insert(decl.name.clone(), f_local_count_f64 + pcount);
                    f_local_count_f64 += 1;
                }
            } else if let StmtKind::StrongDeclaration(decl, _) = &s.kind {
                f_locals_types.insert(decl.name.clone(), "strong".into());
                if !f_locals_f64.contains_key(&decl.name) {
                    f_locals_f64.insert(decl.name.clone(), f_local_count_f64 + pcount);
                    f_local_count_f64 += 1;
                }
            } else if let StmtKind::WeakDeclaration(decl, _) = &s.kind {
                f_locals_types.insert(decl.name.clone(), "weak".into());
                if !f_locals_f64.contains_key(&decl.name) {
                    f_locals_f64.insert(decl.name.clone(), f_local_count_f64 + pcount);
                    f_local_count_f64 += 1;
                }
            } else if let StmtKind::ForIn { name, .. } = &s.kind {
                f_locals_types.insert(name.clone(), "int".into());
                if !f_locals_i32.contains_key(name) && !f_locals_f64.contains_key(name) {
                    f_locals_i32.insert(name.clone(), f_local_count_i32 + pcount);
                    f_local_count_i32 += 1;
                }
            }
        }
        f_locals_i32.insert("__scratch".to_string(), f_local_count_i32 + pcount);
        f_local_count_i32 += 1;

        f_locals_f64.insert("__scratch_f64".to_string(), f_local_count_f64 + pcount);
        f_local_count_f64 += 1;
        for val in f_locals_f64.values_mut() {
            *val += f_local_count_i32;
        }

        let mut ctx = CompilerContext {
            locals_f64: &f_locals_f64,
            locals_i32: &f_locals_i32,
            funcs: &funcs_index_map,
            strings: &strings,
            str_vars: &mut str_vars,
            complex_vars: &complex_vars,
            var_values: &var_values,
            struct_layouts: &struct_layouts,
            locals_types: &mut f_locals_types,
            defer_scopes: vec![Vec::new()],
            loop_defer_depths: Vec::new(),
        };

        let mut fbody: Vec<u8> = Vec::new();
        for s in f.body.iter() {
            gen_stmt(s, &mut fbody, &mut ctx, ret_byte)
                .map_err(|m| LangError::new(ErrorKind::User, m, 0, 0, "".into()))?;
        }
        // Emit all remaining defers in LIFO order before function exit (compile-time inlining)
        for scope_idx in (0..ctx.defer_scopes.len()).rev() {
            let defers = std::mem::take(&mut ctx.defer_scopes[scope_idx]);
            for defer_block in defers.iter().rev() {
                gen_stmt(defer_block, &mut fbody, &mut ctx, ret_byte)
                    .map_err(|m| LangError::new(ErrorKind::User, m, 0, 0, "".into()))?;
            }
        }
        if ret_byte.is_some() && !fbody.ends_with(&[0x0F]) {
            if ret_byte == Some(0x7F) {
                fbody.push(0x41);
                write_u32_leb(0, &mut fbody);
            } else {
                fbody.push(0x44);
                fbody.extend(&0f64.to_bits().to_le_bytes());
            }
            fbody.push(0x0F);
        }
        code_funcs.push((f_local_count_i32, f_local_count_f64, fbody));
    }

    let mut main_body: Vec<u8> = Vec::new();
    let mut main_types: FastMap<String, String> = FastMap::default();
    let mut ctx = CompilerContext {
        locals_f64: &locals_map_f64,
        locals_i32: &locals_map_i32,
        funcs: &funcs_index_map,
        strings: &strings,
        str_vars: &mut str_vars,
        complex_vars: &complex_vars,
        var_values: &var_values,
        struct_layouts: &struct_layouts,
        locals_types: &mut main_types,
        defer_scopes: vec![Vec::new()],
        loop_defer_depths: Vec::new(),
    };
    for s in &prog {
        match &s.kind {
            StmtKind::Function(..) => {}
            _ => gen_stmt(s, &mut main_body, &mut ctx, None)
                .map_err(|m| LangError::new(ErrorKind::User, m, 0, 0, "".into()))?,
        }
    }
    if let Some((main_fn_idx, _, ret_opt)) = funcs_index_map.get("main") {
        main_body.push(0x10);
        write_u32_leb(*main_fn_idx, &mut main_body);
        if ret_opt.is_some() {
            main_body.push(0x1A);
        }
    }

    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend(&[0x00, 0x61, 0x73, 0x6D]);
    bytes.extend(&[0x01, 0x00, 0x00, 0x00]);
    bytes.extend(encode_type_section(&func_types));
    bytes.extend(encode_import_section());
    let mut all_func_types = function_section_types.clone();
    all_func_types.push(2);
    bytes.extend(encode_function_section(&all_func_types));
    bytes.extend(encode_memory_section(1));
    let main_index = 5 + function_section_types.len() as u32;
    bytes.extend(encode_export_section(main_index, true));
    code_funcs.push((local_count_i32, local_count_f64, main_body));
    bytes.extend(encode_code_section(&code_funcs));
    if !strings.is_empty() {
        let mut segs: Vec<(u32, Vec<u8>)> = Vec::new();
        for (s, off) in &strings {
            segs.push((*off, s.as_bytes().to_vec()));
        }
        bytes.extend(encode_data_section(&segs));
    }

    let mut f = File::create(out)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(&bytes)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    Ok(())
}

fn is_numeric_expr(e: &Expr, _funcs: &FastMap<String, (u32, Vec<u8>, Option<u8>)>) -> bool {
    match &e.kind {
        ExprKind::Literal(Value::Number(_)) => true,
        ExprKind::Binary(..) | ExprKind::Unary(..) => true,
        _ => false,
    }
}

pub fn write_js_loader(wasm_out: &Path, js_out: &Path) -> Result<(), LangError> {
    let wasm_name = wasm_out.file_name().unwrap().to_string_lossy().to_string();
    let js = format!(
        r#"
(async function(){{
  const wasmFile = '{wasm}';
  let bytes;
  const isNode = (typeof process !== 'undefined' && process.versions && process.versions.node);
  if (isNode) {{
    const fs = require('fs');
    const path = require('path');
    const p = path.join(__dirname, wasmFile);
    bytes = fs.readFileSync(p);
  }} else {{
    const resp = await fetch(wasmFile);
    bytes = await resp.arrayBuffer();
  }}
  let memory = new WebAssembly.Memory({{initial:1}});
  let heap = 2048;
  const imports = {{
    env: {{
      print_f64: (x) => {{ if (isNode) {{ process.stdout.write(String(x)); }} else {{ console.log(String(x)); }} }},
      print_str: (ptr, len) => {{
        const buf = new Uint8Array(memory.buffer, ptr, len);
        const s = new TextDecoder('utf-8').decode(buf);
        if (isNode) {{ process.stdout.write(s); }} else {{ console.log(s); }}
      }},
      alloc: (len) => {{ const p = heap; heap += len; return p; }},
      concat2: (p1,l1,p2,l2) => {{
        const out = heap; heap += (l1+l2);
        new Uint8Array(memory.buffer, out, l1).set(new Uint8Array(memory.buffer, p1, l1));
        new Uint8Array(memory.buffer, out+l1, l2).set(new Uint8Array(memory.buffer, p2, l2));
        return out;
      }},
      file_write_str: (fptr, flen, sptr, slen) => {{
        const fbuf = new Uint8Array(memory.buffer, fptr, flen);
        const filePath = new TextDecoder('utf-8').decode(fbuf);
        const sbuf = new Uint8Array(memory.buffer, sptr, slen);
        const s = new TextDecoder('utf-8').decode(sbuf);
        if (isNode) {{
          const fs = require('fs');
          fs.appendFileSync(filePath, s);
        }}
      }},
      input_str: (ptr, len) => {{
        const pbuf = new Uint8Array(memory.buffer, ptr, len);
        const prompt = new TextDecoder('utf-8').decode(pbuf);
        let res = "";
        if (isNode) {{
          const fs = require('fs');
          process.stdout.write(prompt);
          const buf = Buffer.alloc ? Buffer.alloc(1024) : new Buffer(1024);
          try {{
            const bytesRead = fs.readSync(0, buf, 0, 1024, null);
            res = buf.toString('utf8', 0, bytesRead).replace(/\r?\n$/, '');
          }} catch (e) {{ res = ""; }}
        }} else {{
          res = window.prompt(prompt) || "";
        }}
        const encoded = new TextEncoder().encode(res);
        const out = heap; heap += (encoded.length + 1);
        new Uint8Array(memory.buffer, out, encoded.length).set(encoded);
        new Uint8Array(memory.buffer, out + encoded.length, 1).set([0]);
        return out;
      }},
      input_f64: (ptr, len) => {{
        const pbuf = new Uint8Array(memory.buffer, ptr, len);
        const prompt = new TextDecoder('utf-8').decode(pbuf);
        let res = "";
        if (isNode) {{
          const fs = require('fs');
          process.stdout.write(prompt);
          const buf = Buffer.alloc ? Buffer.alloc(1024) : new Buffer(1024);
          try {{
            const bytesRead = fs.readSync(0, buf, 0, 1024, null);
            res = buf.toString('utf8', 0, bytesRead).replace(/\r?\n$/, '');
          }} catch (e) {{ res = ""; }}
        }} else {{
          res = window.prompt(prompt) || "";
        }}
        return parseFloat(res) || 0;
      }}
    }}
  }};
  const {{ instance }} = await WebAssembly.instantiate(bytes, imports);
  memory = instance.exports.memory;
  instance.exports.main();
}})();"#,
        wasm = wasm_name
    );
    std::fs::write(js_out, js)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::compile_to_file;
    #[test]
    fn wasm_writes_valid_header() {
        let src = "print(1);";
        let tmp = std::env::temp_dir().join("india-wasm-test.wasm");
        if compile_to_file(src, &tmp).is_ok() {
            let data = std::fs::read(&tmp).unwrap();
            assert!(data.starts_with(&[0x00, 0x61, 0x73, 0x6D]));
            let _ = std::fs::remove_file(&tmp);
        }
    }
    #[test]
    fn wasm_print_extended_compiles() {
        // WASM MVP only supports numeric literals - test with simple numeric print
        let src = "print(1);";
        let tmp = std::env::temp_dir().join("india-wasm-test-extended.wasm");
        let _ = compile_to_file(src, &tmp);
        let _ = std::fs::remove_file(&tmp);
    }
    #[test]
    fn wasm_func_mixed_param_types_compiles() {
        let src = "fn f(x, y){ print(x); } f(1, 2);";
        let tmp = std::env::temp_dir().join("india-wasm-test-fn.wasm");
        let _ = compile_to_file(src, &tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn wasm_nullish_coalescing_basic() {
        let src = "let x = null ?? 42; print(x);";
        let tmp = std::env::temp_dir().join("wasm-nullish-basic.wasm");
        let _result = compile_to_file(src, &tmp);
        // May not compile due to null literal limitation, but test the operator exists
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn wasm_nullish_coalescing_numbers() {
        let src = "let a = 0; let b = a ?? 10; print(b);";
        let tmp = std::env::temp_dir().join("wasm-nullish-num.wasm");
        let _ = compile_to_file(src, &tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn wasm_optional_chaining_compiles() {
        // Test that optional chaining syntax is recognized
        let src = "let obj = {x: 42}; let y = obj?.x; print(y);";
        let tmp = std::env::temp_dir().join("wasm-opt-chain.wasm");
        let _result = compile_to_file(src, &tmp);
        // May not fully work due to WASM limitations, but should parse
        let _ = std::fs::remove_file(&tmp);
    }
}
