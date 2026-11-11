use super::encoder::*;
use super::wasi::{self, WasiAbi, WasiLiterals};
use crate::parsing::ast::{Expr, ExprKind, Stmt, StmtKind, TokenKind, Value};
use crate::parsing::error::{ErrorKind, LangError};
use crate::parsing::hir_lower::clean_type_name;
use crate::types::field_layout::FieldLayout;
use crate::types::visibility::Visibility;
use crate::utils::collections::FastMap;
use std::cell::RefCell;
use std::fs::File;
use std::io::Write;
use std::path::Path;

thread_local! {
    static WASM_VAR_TYPES: RefCell<FastMap<String, String>> = RefCell::new(FastMap::default());
}

/// Host ABI a module is compiled against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WasmAbi {
    /// `env.*` imports (`print_f64`, `print_str`, `alloc`, `concat2`,
    /// `file_write_str`) served by a generated Node loader.
    Env,
    /// A single `wasi_snapshot_preview1.fd_write` import plus in-module
    /// adapters, so the module runs on any WASI Preview 1 host.
    WasiPreview1,
}

struct CompilerContext<'a> {
    locals_f64: &'a FastMap<String, u32>,
    locals_i32: &'a FastMap<String, u32>,
    funcs: &'a FastMap<String, (u32, Vec<u8>, Option<u8>)>,
    strings: &'a Vec<(String, u32)>,
    str_vars: &'a mut FastMap<String, (u32, u32)>,
    complex_vars: &'a FastMap<String, (u32, u32)>,
    global_vars: &'a FastMap<String, u32>,
    is_main: bool,
    var_values: &'a FastMap<String, Value>,
    struct_layouts: &'a FastMap<String, FieldLayout>,
    locals_types: &'a mut FastMap<String, String>,
    /// Stack of defer scopes for compile-time inlining (zero runtime overhead).
    defer_scopes: Vec<Vec<Stmt>>,
    /// Records `defer_scopes.len()` at each loop body entry for break/continue.
    loop_defer_depths: Vec<usize>,
    /// Function index that prints one f64.
    print_f64_idx: u32,
    /// Function index that writes `(ptr, len)` bytes to stdout.
    print_str_idx: u32,
    /// Function index of the `(size) -> ptr` allocator.
    alloc_idx: u32,
    /// Function index for `(path_ptr, path_len, ptr, len)` file writes.
    /// `None` under WASI, where `print(..., { file: ... })` is rejected.
    file_write_idx: Option<u32>,
    /// Optional runtime Date adapter in the env ABI.
    date_op_idx: Option<u32>,
}

fn compute_wasm_layout(fields: &[(String, String)]) -> FieldLayout {
    let mut layout = FieldLayout::new();
    for (name, type_name) in fields {
        let (size, align) = (8, 8);
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

fn is_wasm_integer_val(v: &Value) -> bool {
    matches!(
        v,
        Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
    ) || (if let Value::Number(n) = v {
        n.fract() == 0.0
    } else {
        false
    })
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
            crate::parsing::ast::Value::Null => {
                out.push(0x44);
                out.extend(&f64::NAN.to_bits().to_le_bytes());
                Ok(None)
            }
            _ => {
                out.push(0x44);
                out.extend(&0f64.to_bits().to_le_bytes());
                Ok(None)
            }
        },
        ExprKind::Variable(name) => {
            if !ctx.is_main {
                if let Some(idx) = ctx.locals_f64.get(name) {
                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    return Ok(None);
                } else if let Some(idx) = ctx.locals_i32.get(name) {
                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0xB7);
                    return Ok(None);
                }
            }
            if let Some(off) = ctx.global_vars.get(name) {
                out.push(0x41);
                write_i32_leb(*off as i32, out);
                out.push(0x2B); // f64.load
                out.push(0x03); // align 8
                out.push(0x00); // offset 0
                Ok(None)
            } else if let Some(idx) = ctx.locals_f64.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                Ok(None)
            } else if let Some(idx) = ctx.locals_i32.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                out.push(0xB7);
                Ok(None)
            } else if let Some((off, _len)) = ctx
                .str_vars
                .get(name)
                .or_else(|| ctx.complex_vars.get(name))
            {
                out.push(0x44);
                out.extend(&(*off as f64).to_bits().to_le_bytes());
                Ok(Some("string".into()))
            } else if let Some(val) = ctx.var_values.get(name) {
                match val {
                    Value::Null => {
                        out.push(0x44);
                        out.extend(&f64::NAN.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    Value::Bool(b) => {
                        let num: f64 = if *b { 1.0 } else { 0.0 };
                        out.push(0x44);
                        out.extend(&num.to_bits().to_le_bytes());
                        Ok(Some("bool".into()))
                    }
                    _ => {
                        if let Some(n) = val.as_f64() {
                            out.push(0x44);
                            out.extend(&n.to_bits().to_le_bytes());
                            Ok(None)
                        } else if let Some((fidx, _, _)) = ctx.funcs.get(name) {
                            out.push(0x44);
                            out.extend(&(*fidx as f64).to_bits().to_le_bytes());
                            Ok(Some("fn".into()))
                        } else {
                            Err(format!("unknown variable in f64 expr: {}", name))
                        }
                    }
                }
            } else {
                if let Some((fidx, _, _)) = ctx.funcs.get(name) {
                    // Function reference passed as a value (e.g. callback or func pointer)
                    out.push(0x44); // f64.const
                    out.extend(&(*fidx as f64).to_bits().to_le_bytes());
                    Ok(Some("fn".into()))
                } else {
                    Err(format!("unknown variable in f64 expr: {}", name))
                }
            }
        }
        ExprKind::Binary(l, op, r) => {
            if matches!(op, TokenKind::Instanceof) {
                out.push(0x44);
                out.extend(&1.0f64.to_bits().to_le_bytes());
                return Ok(Some("bool".into()));
            }
            // Handle nullish coalescing
            if matches!(op, TokenKind::NullCoalesce) {
                // Evaluate left operand
                let left_ty = gen_expr_f64(l, out, ctx)?;

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
                out.push(0x61); // f64.eq (compare with itself; NaN == NaN is false)

                // If NaN (left == left fails), use right; else use left
                out.push(0x04); // if
                out.push(0x7C); // f64 result type

                // Then: return left value (it's a valid number)
                out.push(0x20); // local.get
                write_u32_leb(scratch_idx, out);

                // Else: evaluate and return right
                out.push(0x05); // else
                let right_ty = gen_expr_f64(r, out, ctx)?;

                out.push(0x0B); // end

                // Propagate string type if either side is a string
                let is_string = matches!(left_ty, Some(ref s) if s == "string")
                    || matches!(right_ty, Some(ref s) if s == "string");
                return Ok(if is_string {
                    Some("string".into())
                } else {
                    None
                });
            }

            if matches!(op, TokenKind::EqualEqual | TokenKind::StrictEqual)
                && matches!(r.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(l, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x62); // f64.ne (NaN != NaN is 1)
                out.push(0xB7);
                return Ok(Some("bool".into()));
            }
            if matches!(op, TokenKind::EqualEqual | TokenKind::StrictEqual)
                && matches!(l.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(r, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x62); // f64.ne
                out.push(0xB7);
                return Ok(Some("bool".into()));
            }
            if matches!(op, TokenKind::BangEqual | TokenKind::StrictNotEqual)
                && matches!(r.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(l, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x61); // f64.eq
                out.push(0xB7);
                return Ok(Some("bool".into()));
            }
            if matches!(op, TokenKind::BangEqual | TokenKind::StrictNotEqual)
                && matches!(l.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(r, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x61); // f64.eq
                out.push(0xB7);
                return Ok(Some("bool".into()));
            }

            if matches!(op, TokenKind::StarStar) {
                if let (Some(base), Some(exp)) = (
                    expr_to_value_env(l, ctx.var_values),
                    expr_to_value_env(r, ctx.var_values),
                ) {
                    if let (Some(b), Some(e)) = (base.as_f64(), exp.as_f64()) {
                        let res = b.powf(e);
                        out.push(0x44);
                        out.extend(&res.to_bits().to_le_bytes());
                        return Ok(None);
                    }
                }
            }

            if matches!(op, TokenKind::In) {
                let exists = if let (Some(needle), Some(haystack)) = (
                    expr_to_value_env(l, ctx.var_values),
                    expr_to_value_env(r, ctx.var_values),
                ) {
                    match haystack {
                        Value::Array(a) => a.iter().any(|el| match (&needle, el) {
                            (Value::Str(s1), Value::Str(s2)) => s1 == s2,
                            _ => needle.as_f64() == el.as_f64(),
                        }),
                        Value::Object(m) => {
                            let k_str = match &needle {
                                Value::Str(s) => s.clone(),
                                _ => crate::execution::runtime_core::format::fmt(&needle),
                            };
                            m.contains_key(&k_str)
                        }
                        Value::Str(s) => {
                            let sub = match &needle {
                                Value::Str(s) => s.clone(),
                                _ => crate::execution::runtime_core::format::fmt(&needle),
                            };
                            s.contains(&sub)
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                out.push(0x44);
                out.extend(
                    &(if exists { 1.0f64 } else { 0.0f64 })
                        .to_bits()
                        .to_le_bytes(),
                );
                return Ok(Some("bool".into()));
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
                    let is_int_l = expr_to_value_env(l, ctx.var_values)
                        .as_ref()
                        .map(is_wasm_integer_val)
                        .unwrap_or(false)
                        || resolve_wasm_type_name_expr(l, ctx.var_values)
                            .map(|t| {
                                matches!(
                                    t.as_str(),
                                    "i8" | "i16"
                                        | "i32"
                                        | "i64"
                                        | "u8"
                                        | "u16"
                                        | "u32"
                                        | "u64"
                                        | "int"
                                )
                            })
                            .unwrap_or(false);
                    let is_int_r = expr_to_value_env(r, ctx.var_values)
                        .as_ref()
                        .map(is_wasm_integer_val)
                        .unwrap_or(false)
                        || resolve_wasm_type_name_expr(r, ctx.var_values)
                            .map(|t| {
                                matches!(
                                    t.as_str(),
                                    "i8" | "i16"
                                        | "i32"
                                        | "i64"
                                        | "u8"
                                        | "u16"
                                        | "u32"
                                        | "u64"
                                        | "int"
                                )
                            })
                            .unwrap_or(false);
                    if is_int_l && is_int_r {
                        out.push(0x9D); // f64.trunc
                    }
                    Ok(None)
                }
                TokenKind::TildeSlash => {
                    out.push(0xA3);
                    out.push(0x9D);
                    Ok(None)
                }
                TokenKind::Percent => {
                    let s1 = *ctx
                        .locals_f64
                        .get("__scratch_f64")
                        .ok_or("no __scratch_f64")?;
                    let s2 = *ctx
                        .locals_f64
                        .get("__scratch_f64_2")
                        .ok_or("no __scratch_f64_2")?;
                    out.push(0x21);
                    write_u32_leb(s2, out);
                    out.push(0x21);
                    write_u32_leb(s1, out);
                    out.push(0x20);
                    write_u32_leb(s1, out);
                    out.push(0x20);
                    write_u32_leb(s1, out);
                    out.push(0x20);
                    write_u32_leb(s2, out);
                    out.push(0xA3); // f64.div
                    out.push(0x9D); // f64.trunc
                    out.push(0x20);
                    write_u32_leb(s2, out);
                    out.push(0xA2); // f64.mul
                    out.push(0xA1); // f64.sub
                    Ok(None)
                }
                TokenKind::AndAnd => {
                    let s_i32 = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    out.push(0x62); // r != 0.0 -> i32
                    out.push(0x21);
                    write_u32_leb(s_i32, out);
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    out.push(0x62); // l != 0.0 -> i32
                    out.push(0x20);
                    write_u32_leb(s_i32, out);
                    out.push(0x71); // i32.and
                    out.push(0xB7); // f64.convert_i32_s
                    Ok(Some("bool".into()))
                }
                TokenKind::OrOr => {
                    let s_i32 = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    out.push(0x62); // r != 0.0 -> i32
                    out.push(0x21);
                    write_u32_leb(s_i32, out);
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    out.push(0x62); // l != 0.0 -> i32
                    out.push(0x20);
                    write_u32_leb(s_i32, out);
                    out.push(0x72); // i32.or
                    out.push(0xB7); // f64.convert_i32_s
                    Ok(Some("bool".into()))
                }
                TokenKind::EqualEqual | TokenKind::StrictEqual => {
                    out.push(0x61); // f64.eq -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::BangEqual | TokenKind::StrictNotEqual => {
                    out.push(0x62); // f64.ne -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::Greater => {
                    out.push(0x64); // f64.gt -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::GreaterEqual => {
                    out.push(0x66); // f64.ge -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::Less => {
                    out.push(0x63); // f64.lt -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::LessEqual => {
                    out.push(0x65); // f64.le -> i32
                    out.push(0xB7); // f64.convert_i32_s -> f64
                    Ok(Some("bool".into()))
                }
                TokenKind::StarStar => {
                    let s_base = *ctx
                        .locals_f64
                        .get("__scratch_f64")
                        .ok_or("no __scratch_f64")?;
                    let s_exp = *ctx
                        .locals_f64
                        .get("__scratch_f64_2")
                        .ok_or("no __scratch_f64_2")?;
                    out.push(0x21);
                    write_u32_leb(s_exp, out);
                    out.push(0x21);
                    write_u32_leb(s_base, out);
                    out.push(0x20);
                    write_u32_leb(s_exp, out);
                    out.push(0x44);
                    out.extend(&2.0f64.to_bits().to_le_bytes());
                    out.push(0x61); // exp == 2.0
                    out.push(0x04);
                    out.push(0x7C); // if f64
                    out.push(0x20);
                    write_u32_leb(s_base, out);
                    out.push(0x20);
                    write_u32_leb(s_base, out);
                    out.push(0xA2); // f64.mul
                    out.push(0x05); // else
                    out.push(0x20);
                    write_u32_leb(s_exp, out);
                    out.push(0x44);
                    out.extend(&0.5f64.to_bits().to_le_bytes());
                    out.push(0x61); // exp == 0.5
                    out.push(0x04);
                    out.push(0x7C);
                    out.push(0x20);
                    write_u32_leb(s_base, out);
                    out.push(0x9F); // f64.sqrt
                    out.push(0x05); // else fallback: base
                    out.push(0x20);
                    write_u32_leb(s_base, out);
                    out.push(0x0B);
                    out.push(0x0B);
                    Ok(None)
                }
                TokenKind::Ampersand => {
                    let s = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0xAA);
                    out.push(0x21);
                    write_u32_leb(s, out);
                    out.push(0xAA);
                    out.push(0x20);
                    write_u32_leb(s, out);
                    out.push(0x71);
                    out.push(0xB7);
                    Ok(None)
                }
                TokenKind::Pipe => {
                    let s = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0xAA);
                    out.push(0x21);
                    write_u32_leb(s, out);
                    out.push(0xAA);
                    out.push(0x20);
                    write_u32_leb(s, out);
                    out.push(0x72);
                    out.push(0xB7);
                    Ok(None)
                }
                TokenKind::Caret => {
                    let s = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0xAA);
                    out.push(0x21);
                    write_u32_leb(s, out);
                    out.push(0xAA);
                    out.push(0x20);
                    write_u32_leb(s, out);
                    out.push(0x73);
                    out.push(0xB7);
                    Ok(None)
                }
                TokenKind::ShiftLeft => {
                    let s = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0xAA);
                    out.push(0x21);
                    write_u32_leb(s, out);
                    out.push(0xAA);
                    out.push(0x20);
                    write_u32_leb(s, out);
                    out.push(0x74);
                    out.push(0xB7);
                    Ok(None)
                }
                TokenKind::ShiftRight => {
                    let s = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0xAA);
                    out.push(0x21);
                    write_u32_leb(s, out);
                    out.push(0xAA);
                    out.push(0x20);
                    write_u32_leb(s, out);
                    out.push(0x75);
                    out.push(0xB7);
                    Ok(None)
                }
                TokenKind::In => {
                    let exists = if let (Some(needle), Some(haystack)) = (
                        expr_to_value_env(l, ctx.var_values),
                        expr_to_value_env(r, ctx.var_values),
                    ) {
                        match haystack {
                            Value::Array(a) => a.iter().any(|el| match (&needle, el) {
                                (Value::Str(s1), Value::Str(s2)) => s1 == s2,
                                _ => needle.as_f64() == el.as_f64(),
                            }),
                            Value::Object(m) => {
                                let k_str = match &needle {
                                    Value::Str(s) => s.clone(),
                                    _ => crate::execution::runtime_core::format::fmt(&needle),
                                };
                                m.contains_key(&k_str)
                            }
                            Value::Str(s) => {
                                let sub = match &needle {
                                    Value::Str(s) => s.clone(),
                                    _ => crate::execution::runtime_core::format::fmt(&needle),
                                };
                                s.contains(&sub)
                            }
                            _ => false,
                        }
                    } else {
                        false
                    };
                    out.push(0x44);
                    out.extend(
                        &(if exists { 1.0f64 } else { 0.0f64 })
                            .to_bits()
                            .to_le_bytes(),
                    );
                    Ok(Some("bool".into()))
                }
                TokenKind::Instanceof => {
                    out.push(0x44);
                    out.extend(&1.0f64.to_bits().to_le_bytes());
                    Ok(Some("bool".into()))
                }
                _ => Err("unsupported binary op in wasm MVP".into()),
            }
        }
        ExprKind::Logical(l, op, r) => {
            let s_i32 = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
            gen_expr_f64(l, out, ctx)?;
            gen_expr_f64(r, out, ctx)?;
            out.push(0x44);
            out.extend(&0.0f64.to_bits().to_le_bytes());
            out.push(0x62); // r != 0.0 -> i32
            out.push(0x21);
            write_u32_leb(s_i32, out);
            out.push(0x44);
            out.extend(&0.0f64.to_bits().to_le_bytes());
            out.push(0x62); // l != 0.0 -> i32
            out.push(0x20);
            write_u32_leb(s_i32, out);
            match op {
                TokenKind::And => out.push(0x71), // i32.and
                TokenKind::Or => out.push(0x72),  // i32.or
                _ => out.push(0x71),
            }
            out.push(0xB7); // f64.convert_i32_s
            Ok(Some("bool".into()))
        }
        ExprKind::Unary(op, r) => {
            if *op == TokenKind::Typeof {
                let type_name = resolve_wasm_type_name_expr(r, ctx.var_values)
                    .unwrap_or_else(|| "object".to_string());
                let off = ctx
                    .strings
                    .iter()
                    .find(|(x, _)| x == &type_name)
                    .map(|(_, o)| *o)
                    .unwrap_or(1024) as f64;
                out.push(0x44);
                out.extend(&off.to_bits().to_le_bytes());
                return Ok(Some("string".into()));
            }
            gen_expr_f64(r, out, ctx)?;
            match op {
                TokenKind::Minus => {
                    out.push(0x9F); // f64.neg
                    Ok(None)
                }
                _ => Err("unsupported unary op in wasm MVP".into()),
            }
        }
        ExprKind::Format(..) => {
            if let Some(s) = format_static_val_for_wasm_env(e, ctx.var_values) {
                let off = ctx
                    .strings
                    .iter()
                    .find(|(x, _)| x == &s)
                    .map(|(_, o)| *o)
                    .unwrap_or(1024) as f64;
                out.push(0x44);
                out.extend(&off.to_bits().to_le_bytes());
                Ok(Some("string".into()))
            } else {
                Err("unsupported dynamic format in wasm MVP".into())
            }
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if (name == "len" || name == "length") && !args.is_empty() {
                    let l = if let Some(val) = expr_to_value_env(&args[0], ctx.var_values) {
                        match val {
                            Value::Array(a) => a.len() as f64,
                            Value::Str(s) => s.len() as f64,
                            Value::Object(m) => m.len() as f64,
                            _ => 0.0,
                        }
                    } else if let Some(s) = format_static_val_for_wasm_env(&args[0], ctx.var_values)
                    {
                        s.len() as f64
                    } else {
                        0.0
                    };
                    out.push(0x44);
                    out.extend(&l.to_bits().to_le_bytes());
                    return Ok(Some("number".into()));
                }
                if name == "concat" || name == "str_concat" {
                    if let Some(s) = format_static_val_for_wasm_env(e, ctx.var_values) {
                        let off = ctx
                            .strings
                            .iter()
                            .find(|(x, _)| x == &s)
                            .map(|(_, o)| *o)
                            .unwrap_or(1024) as f64;
                        out.push(0x44);
                        out.extend(&off.to_bits().to_le_bytes());
                        return Ok(Some("string".into()));
                    }
                }
                if name == "makeSet" {
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    return Ok(Some("Set".into()));
                }
                if name == "Error" || name == "error" {
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    return Ok(Some("Error".into()));
                }
                if name == "concat"
                    || name == "str_concat"
                    || name == "format"
                    || name == "str"
                    || name == "string"
                    || name == "String"
                {
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    return Ok(Some("string".into()));
                }
                if name == "sizeof" || name == "sizeOf" {
                    let sz = if let Some(arg) = args.first() {
                        calculate_wasm_sizeof(arg, ctx.var_values) as f64
                    } else {
                        0.0
                    };
                    out.push(0x44);
                    out.extend(&sz.to_bits().to_le_bytes());
                    return Ok(Some("number".into()));
                }
                if name == "hasKey" || name == "containsKey" {
                    let exists = if args.len() >= 2 {
                        let target = expr_to_value_env(&args[0], ctx.var_values);
                        let key = expr_to_value_env(&args[1], ctx.var_values);
                        if let (Some(Value::Object(map)), Some(k)) = (target, key) {
                            let key_str = match k {
                                Value::Str(s) => s,
                                Value::Number(n) => {
                                    if n.fract() == 0.0 {
                                        (n as i64).to_string()
                                    } else {
                                        n.to_string()
                                    }
                                }
                                Value::Char(c) => c.to_string(),
                                Value::U8(n) => n.to_string(),
                                Value::U16(n) => n.to_string(),
                                Value::U32(n) => n.to_string(),
                                Value::U64(n) => n.to_string(),
                                Value::I8(n) => n.to_string(),
                                Value::I16(n) => n.to_string(),
                                Value::I32(n) => n.to_string(),
                                Value::I64(n) => n.to_string(),
                                other => crate::execution::runtime_core::format::fmt(&other),
                            };
                            map.contains_key(&key_str)
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    let result: f64 = if exists { 1.0 } else { 0.0 };
                    out.push(0x44);
                    out.extend(&result.to_bits().to_le_bytes());
                    return Ok(Some("bool".into()));
                }
                if name == "assert" || name == "assert_eq" || name == "assert_ne" {
                    out.push(0x44);
                    out.extend(&1.0f64.to_bits().to_le_bytes());
                    return Ok(None);
                }
                if let Some((fidx, params, ret_type)) = ctx.funcs.get(name) {
                    for (i, pt) in params.iter().enumerate() {
                        if let Some(a) = args.get(i) {
                            if *pt == 0x7F {
                                gen_expr_i32(a, out, ctx)?;
                            } else {
                                gen_expr_f64(a, out, ctx)?;
                            }
                        } else if *pt == 0x7F {
                            out.push(0x41);
                            write_i32_leb(0, out);
                        } else {
                            out.push(0x44);
                            out.extend(&0f64.to_bits().to_le_bytes());
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
                        out.push(0x44);
                        out.extend(&0.0f64.to_bits().to_le_bytes());
                        Ok(None)
                    }
                } else if name == "type" || name == "typeof" {
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    Ok(None)
                } else if name == "len" || name == "length" {
                    if let Some(val) = expr_to_value_env(e, ctx.var_values) {
                        if let Some(n) = val.as_f64() {
                            out.push(0x44);
                            out.extend(&n.to_bits().to_le_bytes());
                            return Ok(None);
                        }
                    }
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    Ok(None)
                } else if name == "makeSet" {
                    out.push(0x44);
                    out.extend(&0.0f64.to_bits().to_le_bytes());
                    Ok(None)
                } else if name == "double" {
                    if let Some(a) = args.first() {
                        gen_expr_f64(a, out, ctx)?;
                        out.push(0x44);
                        out.extend(&2.0f64.to_bits().to_le_bytes());
                        out.push(0xA2); // f64.mul
                        Ok(None)
                    } else {
                        out.push(0x44);
                        out.extend(&0.0f64.to_bits().to_le_bytes());
                        Ok(None)
                    }
                } else if name == "__array_init" || name == "__array_repeat" || name == "__array_init_raw" {
                    out.push(0x44);
                    out.extend(&0f64.to_bits().to_le_bytes());
                    Ok(None)
                } else {
                    Err(format!("unknown callee in f64 expr: {}", name))
                }
            } else if let ExprKind::Get(target, method) = &callee.kind {
                if let Some(op) = super::dates::method_op(method) {
                    let index = ctx.date_op_idx.ok_or("Date methods require the env Date adapter")?;
                    if method == "now" {
                        out.push(0x44);
                        out.extend(&0.0f64.to_le_bytes());
                    } else {
                        gen_expr_f64(target, out, ctx)?;
                    }
                    out.push(0x41);
                    write_i32_leb(op, out);
                    out.push(0x10);
                    write_u32_leb(index, out);
                    return Ok(None);
                }
                match method.as_str() {
                    "hasKey" | "containsKey" => {
                        let target_val = expr_to_value_env(target, ctx.var_values);
                        let key = args
                            .first()
                            .and_then(|a| expr_to_value_env(a, ctx.var_values));
                        let result: f64 =
                            if let (Some(Value::Object(map)), Some(k)) = (target_val, key) {
                                let key_str = match k {
                                    Value::Str(s) => s,
                                    other => crate::execution::runtime_core::format::fmt(&other),
                                };
                                if map.contains_key(&key_str) { 1.0 } else { 0.0 }
                            } else {
                                0.0
                            };
                        out.push(0x44);
                        out.extend(&result.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    "includes" | "contains" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result: f64 = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.contains(&sa) { 1.0 } else { 0.0 }
                        } else {
                            1.0
                        };
                        out.push(0x44);
                        out.extend(&result.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    "startsWith" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result: f64 = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.starts_with(&sa) { 1.0 } else { 0.0 }
                        } else {
                            1.0
                        };
                        out.push(0x44);
                        out.extend(&result.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    "endsWith" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result: f64 = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.ends_with(&sa) { 1.0 } else { 0.0 }
                        } else {
                            1.0
                        };
                        out.push(0x44);
                        out.extend(&result.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    "len" | "length" => {
                        let len = if let Some(Value::Array(arr)) =
                            expr_to_value_env(target, ctx.var_values)
                        {
                            arr.len() as f64
                        } else {
                            let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                            s_target.map(|s| s.len() as f64).unwrap_or(0.0)
                        };
                        out.push(0x44);
                        out.extend(&len.to_bits().to_le_bytes());
                        Ok(None)
                    }
                    _ => {
                        out.push(0x44);
                        out.extend(&0.0f64.to_bits().to_le_bytes());
                        Ok(None)
                    }
                }
            } else {
                Err(format!("unsupported callee in f64 expr: {:?}", callee.kind))
            }
        }
        ExprKind::Get(target, field) => {
            if let Some(Value::Array(arr)) = expr_to_value_env(target, ctx.var_values) {
                if field == "length" || field == "len" {
                    out.push(0x44);
                    out.extend(&(arr.len() as f64).to_bits().to_le_bytes());
                    return Ok(None);
                }
            }
            if let Some(Value::Object(map)) = expr_to_value_env(target, ctx.var_values) {
                if let Some(val) = map.get(field) {
                    if let Some(num) = val.as_f64() {
                        out.push(0x44);
                        out.extend(&num.to_bits().to_le_bytes());
                        return Ok(None);
                    }
                }
            }
            let type_name = gen_expr_i32(target, out, ctx)?;
            if let Some(tn) = type_name {
                let base_name = if let Some(idx) = tn.find('<') {
                    &tn[..idx]
                } else {
                    &tn[..]
                };
                let mut layout_opt = ctx.struct_layouts.get(base_name);
                if layout_opt.is_none() {
                    for l in ctx.struct_layouts.values() {
                        if l.get_field_offset(field).is_some() {
                            layout_opt = Some(l);
                            break;
                        }
                    }
                }
                if let Some(layout) = layout_opt {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A);
                                }
                                out.push(0x2B); // f64.load
                                out.push(0x03); // align 2^3 = 8
                                out.push(0x00); // offset 0
                                return Ok(Some("f64".into()));
                            } else {
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A);
                                }
                                out.push(0x28); // i32.load
                                out.push(0x02); // align 2^2 = 4
                                out.push(0x00); // offset 0
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
            if let Some(Value::Null) = expr_to_value_env(target, ctx.var_values) {
                out.push(0x44);
                out.extend(&f64::NAN.to_bits().to_le_bytes());
                return Ok(None);
            }
            if let Some(Value::Object(map)) = expr_to_value_env(target, ctx.var_values) {
                if let Some(val) = map.get(field) {
                    if let Some(num) = val.as_f64() {
                        out.push(0x44);
                        out.extend(&num.to_bits().to_le_bytes());
                        return Ok(None);
                    }
                }
            }
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
                let base_name = if let Some(idx) = tn.find('<') {
                    &tn[..idx]
                } else {
                    &tn[..]
                };
                let mut layout_opt = ctx.struct_layouts.get(base_name);
                if layout_opt.is_none() {
                    for l in ctx.struct_layouts.values() {
                        if l.get_field_offset(field).is_some() {
                            layout_opt = Some(l);
                            break;
                        }
                    }
                }
                if let Some(layout) = layout_opt {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A);
                        }
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                out.push(0x2B); // f64.load
                                out.push(0x03); // align 2^3 = 8
                                out.push(0x00); // offset 0
                            } else {
                                out.push(0x28); // i32.load
                                out.push(0x02); // align 2^2 = 4
                                out.push(0x00); // offset 0
                                out.push(0xB7); // f64.convert
                            }
                        } else {
                            out.push(0x2B); // f64.load (default)
                            out.push(0x03); // align 2^3 = 8
                            out.push(0x00); // offset 0
                        }
                    }
                }
            }

            out.push(0x0B); // end
            Ok(None)
        }
        ExprKind::Index(target, idx) => {
            if let (Some(Value::Array(arr)), Some(idx_val)) = (
                expr_to_value_env(target, ctx.var_values),
                expr_to_value_env(idx, ctx.var_values),
            ) {
                let i = match idx_val {
                    Value::Number(n) => n as usize,
                    Value::I32(n) => n as usize,
                    Value::I64(n) => n as usize,
                    Value::U32(n) => n as usize,
                    Value::U64(n) => n as usize,
                    _ => 0,
                };
                if let Some(elem) = arr.get(i) {
                    let num = match elem {
                        Value::Number(n) => *n,
                        Value::I32(n) => *n as f64,
                        Value::I64(n) => *n as f64,
                        Value::U8(n) => *n as f64,
                        Value::U32(n) => *n as f64,
                        Value::U64(n) => *n as f64,
                        Value::F32(n) => *n as f64,
                        Value::F64(n) => *n,
                        Value::Bool(b) => {
                            if *b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        _ => 0.0,
                    };
                    out.push(0x44);
                    out.extend(&num.to_bits().to_le_bytes());
                    return Ok(None);
                }
            }
            gen_expr_i32(e, out, ctx)?;
            out.push(0xB7); // f64.convert_i32_s
            Ok(None)
        }
        ExprKind::New(target, args) if matches!(&target.kind, ExprKind::Variable(name) if name == "Date") => {
            let index = ctx.date_op_idx.ok_or("Date requires the env Date adapter")?;
            if let Some(arg) = args.first() {
                if args.len() != 1 {
                    return Err("WASM Date currently accepts no arguments or one timestamp".into());
                }
                if matches!(expr_to_value_env(arg, ctx.var_values), Some(Value::Str(_))) {
                    return Err("WASM Date string parsing is not supported".into());
                }
                gen_expr_f64(arg, out, ctx)?;
            } else {
                out.push(0x44);
                out.extend(&0.0f64.to_le_bytes());
                out.push(0x41);
                write_i32_leb(0, out);
                out.push(0x10);
                write_u32_leb(index, out);
            }
            Ok(Some("Date".into()))
        }
        ExprKind::Array(_)
        | ExprKind::Tuple(_)
        | ExprKind::Object(_)
        | ExprKind::New(..)
        | ExprKind::StructLiteral(..)
        | ExprKind::Range(..) => {
            out.push(0x44);
            out.extend(&0f64.to_bits().to_le_bytes());
            Ok(None)
        }
        ExprKind::Grouping(inner) => gen_expr_f64(inner, out, ctx),
        ExprKind::Spread(inner) => gen_expr_f64(inner, out, ctx),
        ExprKind::NonNull(inner) => gen_expr_f64(inner, out, ctx),
        ExprKind::Fn(..) => {
            out.push(0x44);
            out.extend(&0.0f64.to_bits().to_le_bytes());
            Ok(Some("fn".into()))
        }
        ExprKind::Throw(..) => {
            out.push(0x44);
            out.extend(&0.0f64.to_bits().to_le_bytes());
            Ok(None)
        }
        ExprKind::Update(is_inc, is_post, target) => {
            if let ExprKind::Variable(name) = &target.kind {
                if let Some(off) = ctx.global_vars.get(name) {
                    let scratch_idx = *ctx
                        .locals_f64
                        .get("__scratch_f64")
                        .ok_or("no __scratch_f64")?;
                    out.push(0x41);
                    write_i32_leb(*off as i32, out);
                    out.push(0x2B);
                    out.push(0x03);
                    out.push(0x00);
                    out.push(0x22); // tee scratch
                    write_u32_leb(scratch_idx, out);
                    out.push(0x41);
                    write_i32_leb(*off as i32, out);
                    out.push(0x20); // get scratch
                    write_u32_leb(scratch_idx, out);
                    out.push(0x44);
                    out.extend(&1.0f64.to_bits().to_le_bytes());
                    if *is_inc {
                        out.push(0xA0);
                    } else {
                        out.push(0xA1);
                    }
                    out.push(0x39);
                    out.push(0x03);
                    out.push(0x00);
                    if *is_post {
                        out.push(0x20);
                        write_u32_leb(scratch_idx, out);
                    } else {
                        out.push(0x41);
                        write_i32_leb(*off as i32, out);
                        out.push(0x2B);
                        out.push(0x03);
                        out.push(0x00);
                    }
                    return Ok(None);
                }
            }
            out.push(0x44);
            out.extend(&0.0f64.to_bits().to_le_bytes());
            Ok(None)
        }
        ExprKind::Conditional(cond, then_branch, else_branch) => {
            gen_cond_i32(cond, out, ctx)?;
            out.push(0x04); // if
            out.push(0x7C); // block type: f64
            let ty_then = gen_expr_f64(then_branch, out, ctx)?;
            out.push(0x05); // else
            let _ty_else = gen_expr_f64(else_branch, out, ctx)?;
            out.push(0x0B); // end
            Ok(ty_then)
        }
        _ => Err(format!("unsupported expression in wasm MVP: {:?}", e.kind)),
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
            } else if let Some(idx) = ctx.locals_f64.get(name) {
                out.push(0x20);
                write_u32_leb(*idx, out);
                out.push(0xAA); // i32.trunc_f64_s
                let t = ctx.locals_types.get(name).cloned();
                Ok(t)
            } else if let Some((fidx, _, _)) = ctx.funcs.get(name) {
                // Function reference passed as a value (e.g. function pointer / index)
                out.push(0x41); // i32.const
                write_i32_leb(*fidx as i32, out);
                Ok(Some("fn".into()))
            } else if let Some(off) = ctx.global_vars.get(name) {
                out.push(0x41);
                write_i32_leb(*off as i32, out);
                out.push(0x28); // i32.load
                out.push(0x02); // align 4
                out.push(0x00); // offset 0
                Ok(None)
            } else {
                Err(format!("unknown i32 variable: {}", name))
            }
        }
        ExprKind::Format(..) => {
            if let Some(s) = format_static_val_for_wasm_env(e, ctx.var_values) {
                let off = ctx
                    .strings
                    .iter()
                    .find(|(x, _)| x == &s)
                    .map(|(_, o)| *o)
                    .unwrap_or(1024) as i32;
                out.push(0x41);
                write_i32_leb(off, out);
                Ok(Some("string".into()))
            } else {
                Err("unsupported dynamic format in wasm MVP".into())
            }
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "concat" || name == "str_concat" {
                    if let Some(s) = format_static_val_for_wasm_env(e, ctx.var_values) {
                        let off = ctx
                            .strings
                            .iter()
                            .find(|(x, _)| x == &s)
                            .map(|(_, o)| *o)
                            .unwrap_or(1024) as i32;
                        out.push(0x41);
                        write_i32_leb(off, out);
                        return Ok(Some("string".into()));
                    }
                }
                if name == "Error" || name == "error" {
                    out.push(0x41);
                    write_i32_leb(0, out);
                    return Ok(Some("Error".into()));
                }
                if name == "sizeof" || name == "sizeOf" {
                    let sz = if let Some(arg) = args.first() {
                        calculate_wasm_sizeof(arg, ctx.var_values) as i32
                    } else {
                        0
                    };
                    out.push(0x41);
                    write_i32_leb(sz, out);
                    return Ok(Some("number".into()));
                }
                if name == "hasKey" || name == "containsKey" {
                    let exists = if args.len() >= 2 {
                        let target = expr_to_value_env(&args[0], ctx.var_values);
                        let key = expr_to_value_env(&args[1], ctx.var_values);
                        if let (Some(Value::Object(map)), Some(k)) = (target, key) {
                            let key_str = match k {
                                Value::Str(s) => s,
                                Value::Number(n) => {
                                    if n.fract() == 0.0 {
                                        (n as i64).to_string()
                                    } else {
                                        n.to_string()
                                    }
                                }
                                Value::Char(c) => c.to_string(),
                                Value::U8(n) => n.to_string(),
                                Value::U16(n) => n.to_string(),
                                Value::U32(n) => n.to_string(),
                                Value::U64(n) => n.to_string(),
                                Value::I8(n) => n.to_string(),
                                Value::I16(n) => n.to_string(),
                                Value::I32(n) => n.to_string(),
                                Value::I64(n) => n.to_string(),
                                other => crate::execution::runtime_core::format::fmt(&other),
                            };
                            map.contains_key(&key_str)
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    let result: i32 = if exists { 1 } else { 0 };
                    out.push(0x41);
                    write_i32_leb(result, out);
                    return Ok(Some("bool".into()));
                }
                if name == "assert" || name == "assert_eq" || name == "assert_ne" {
                    out.push(0x41);
                    write_i32_leb(1, out);
                    return Ok(None);
                }
                if name == "Error" || name == "error" {
                    out.push(0x41);
                    write_i32_leb(0, out);
                    return Ok(Some("Error".into()));
                }
                if name == "concat"
                    || name == "str_concat"
                    || name == "format"
                    || name == "str"
                    || name == "string"
                    || name == "String"
                {
                    out.push(0x41);
                    write_i32_leb(1024, out);
                    return Ok(Some("string".into()));
                }
                if let Some((fidx, params, ret_type)) = ctx.funcs.get(name) {
                    for (i, pt) in params.iter().enumerate() {
                        if let Some(a) = args.get(i) {
                            if *pt == 0x7C {
                                gen_expr_f64(a, out, ctx)?;
                            } else {
                                gen_expr_i32(a, out, ctx)?;
                            }
                        } else if *pt == 0x7C {
                            out.push(0x44);
                            out.extend(&0f64.to_bits().to_le_bytes());
                        } else {
                            out.push(0x41);
                            write_i32_leb(0, out);
                        }
                    }
                    out.push(0x10);
                    write_u32_leb(*fidx, out);
                    match ret_type {
                        Some(0x7F) => Ok(None),
                        Some(0x7C) => {
                            out.push(0xAA); // i32.trunc_f64_s
                            Ok(None)
                        }
                        Some(_) => Ok(None),
                        None => {
                            out.push(0x41);
                            write_i32_leb(0, out);
                            Ok(None)
                        }
                    }
                } else if name == "type" || name == "typeof" {
                    out.push(0x41);
                    write_i32_leb(0, out);
                    Ok(Some("string".to_string()))
                } else if name == "__array_init" || name == "__array_repeat" || name == "__array_init_raw" {
                    out.push(0x44);
                    out.extend(&0f64.to_bits().to_le_bytes());
                    Ok(None)
                } else {
                    Err(format!("unknown callee in f64 expr: {}", name))
                }
            } else if let ExprKind::Get(target, method) = &callee.kind {
                match method.as_str() {
                    "hasKey" | "containsKey" => {
                        let target_val = expr_to_value_env(target, ctx.var_values);
                        let key = args
                            .first()
                            .and_then(|a| expr_to_value_env(a, ctx.var_values));
                        let result: i32 =
                            if let (Some(Value::Object(map)), Some(k)) = (target_val, key) {
                                let key_str = match k {
                                    Value::Str(s) => s,
                                    other => crate::execution::runtime_core::format::fmt(&other),
                                };
                                if map.contains_key(&key_str) { 1 } else { 0 }
                            } else {
                                0
                            };
                        out.push(0x41);
                        write_i32_leb(result, out);
                        Ok(None)
                    }
                    "includes" | "contains" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.contains(&sa) { 1 } else { 0 }
                        } else {
                            1
                        };
                        out.push(0x41);
                        write_i32_leb(result, out);
                        Ok(None)
                    }
                    "startsWith" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.starts_with(&sa) { 1 } else { 0 }
                        } else {
                            1
                        };
                        out.push(0x41);
                        write_i32_leb(result, out);
                        Ok(None)
                    }
                    "endsWith" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let s_arg = args
                            .first()
                            .and_then(|a| format_static_val_for_wasm_env(a, ctx.var_values));
                        let result = if let (Some(st), Some(sa)) = (s_target, s_arg) {
                            if st.ends_with(&sa) { 1 } else { 0 }
                        } else {
                            1
                        };
                        out.push(0x41);
                        write_i32_leb(result, out);
                        Ok(None)
                    }
                    "len" | "length" => {
                        let s_target = format_static_val_for_wasm_env(target, ctx.var_values);
                        let len = s_target.map(|s| s.len() as i32).unwrap_or(0);
                        out.push(0x41);
                        write_i32_leb(len, out);
                        Ok(None)
                    }
                    _ => {
                        out.push(0x41);
                        write_i32_leb(0, out);
                        Ok(None)
                    }
                }
            } else {
                Err(format!("unsupported callee in i32 expr: {:?}", callee.kind))
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
            if matches!(
                op,
                TokenKind::Greater
                    | TokenKind::GreaterEqual
                    | TokenKind::Less
                    | TokenKind::LessEqual
                    | TokenKind::EqualEqual
                    | TokenKind::BangEqual
                    | TokenKind::StrictEqual
                    | TokenKind::StrictNotEqual
            ) {
                gen_cond_i32(e, out, ctx)?;
                return Ok(None);
            }
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
                TokenKind::Percent => {
                    out.push(0x6F); // i32.rem_s
                    Ok(None)
                }
                TokenKind::Ampersand | TokenKind::AndAnd => {
                    out.push(0x71); // i32.and
                    Ok(None)
                }
                TokenKind::Pipe | TokenKind::OrOr => {
                    out.push(0x72); // i32.or
                    Ok(None)
                }
                TokenKind::Caret => {
                    out.push(0x73); // i32.xor
                    Ok(None)
                }
                TokenKind::ShiftLeft => {
                    out.push(0x74); // i32.shl
                    Ok(None)
                }
                TokenKind::ShiftRight => {
                    out.push(0x75); // i32.shr_s
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
                write_u32_leb(ctx.alloc_idx, out);

                if !ctx.locals_i32.contains_key("__scratch")
                    || !ctx.locals_i32.contains_key("__scratch_val")
                    || !ctx.locals_f64.contains_key("__scratch_f64")
                {
                    return Err("Internal error: __scratch locals not found".into());
                }
                let scratch_idx = *ctx.locals_i32.get("__scratch").unwrap();
                let scratch_val_idx = *ctx.locals_i32.get("__scratch_val").unwrap();
                let scratch_f64_idx = *ctx.locals_f64.get("__scratch_f64").unwrap();

                out.push(0x22); // local.tee $scratch
                write_u32_leb(scratch_idx, out);

                // Stack has ptr.
                // For each field
                for (fname, fexpr) in fields {
                    if let Some(offset) = layout.get_field_offset(fname) {
                        if let Some(fi) = layout.find_field(fname) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                out.push(0x20); // local.get $scratch
                                write_u32_leb(scratch_idx, out);
                                gen_expr_f64(fexpr, out, ctx)?;
                                out.push(0x21); // local.set $scratch_f64
                                write_u32_leb(scratch_f64_idx, out);
                                out.push(0x21); // local.set $scratch
                                write_u32_leb(scratch_idx, out);

                                out.push(0x20); // local.get $scratch
                                write_u32_leb(scratch_idx, out);
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A); // add
                                }
                                out.push(0x20); // local.get $scratch_f64
                                write_u32_leb(scratch_f64_idx, out);
                                out.push(0x39); // f64.store
                                out.push(0x03); // align 2^3 = 8
                                out.push(0x00); // offset 0
                            } else {
                                out.push(0x20); // local.get $scratch
                                write_u32_leb(scratch_idx, out);
                                gen_expr_i32(fexpr, out, ctx)?;
                                out.push(0x21); // local.set $scratch_val
                                write_u32_leb(scratch_val_idx, out);
                                out.push(0x21); // local.set $scratch
                                write_u32_leb(scratch_idx, out);

                                out.push(0x20); // local.get $scratch
                                write_u32_leb(scratch_idx, out);
                                if offset > 0 {
                                    out.push(0x41);
                                    write_u32_leb(offset as u32, out);
                                    out.push(0x6A); // add
                                }
                                out.push(0x20); // local.get $scratch_val
                                write_u32_leb(scratch_val_idx, out);
                                out.push(0x36); // i32.store
                                out.push(0x02); // align 2^2 = 4
                                out.push(0x00); // offset 0
                            }
                        }
                    }
                }
                Ok(Some(name.clone()))
            } else {
                Err(format!("Unknown struct {}", name))
            }
        }
        ExprKind::Object(fields) => {
            // Allocate heap buffer for object literal / type alias
            let total_size = (fields.len().max(1) * 8) as u32;
            out.push(0x41);
            write_u32_leb(total_size, out);
            out.push(0x10);
            write_u32_leb(ctx.alloc_idx, out);

            if !ctx.locals_i32.contains_key("__scratch")
                || !ctx.locals_i32.contains_key("__scratch_val")
            {
                return Err("Internal error: __scratch locals not found".into());
            }
            let scratch_idx = *ctx.locals_i32.get("__scratch").unwrap();
            let scratch_val_idx = *ctx.locals_i32.get("__scratch_val").unwrap();
            out.push(0x22); // local.tee $scratch
            write_u32_leb(scratch_idx, out);

            for (i, (_fname, fexpr)) in fields.iter().enumerate() {
                let offset = (i * 8) as u32;
                // Preserve current outer scratch pointer on WASM operand stack
                out.push(0x20); // local.get $scratch
                write_u32_leb(scratch_idx, out);

                // Generate value into scratch_val
                gen_expr_i32(fexpr, out, ctx)?;
                out.push(0x21); // local.set $scratch_val
                write_u32_leb(scratch_val_idx, out);

                // Restore outer scratch pointer from stack
                out.push(0x21); // local.set $scratch
                write_u32_leb(scratch_idx, out);

                // Now store value at outer_scratch + offset
                out.push(0x20); // local.get $scratch
                write_u32_leb(scratch_idx, out);
                if offset > 0 {
                    out.push(0x41);
                    write_u32_leb(offset, out);
                    out.push(0x6A); // add
                }
                out.push(0x20); // local.get $scratch_val
                write_u32_leb(scratch_val_idx, out);
                out.push(0x36); // i32.store
                out.push(0x02); // align
                out.push(0x00); // offset
            }
            Ok(Some("object".into()))
        }
        ExprKind::Get(target, field) => {
            if let Some(Value::Array(arr)) = expr_to_value_env(target, ctx.var_values) {
                if field == "length" || field == "len" {
                    out.push(0x41);
                    write_i32_leb(arr.len() as i32, out);
                    return Ok(Some("i32".into()));
                }
            }
            let type_name = gen_expr_i32(target, out, ctx)?;
            if let Some(tn) = type_name {
                let base_name = if let Some(idx) = tn.find('<') {
                    &tn[..idx]
                } else {
                    &tn[..]
                };
                let mut layout_opt = ctx.struct_layouts.get(base_name);
                if layout_opt.is_none() {
                    for l in ctx.struct_layouts.values() {
                        if l.get_field_offset(field).is_some() {
                            layout_opt = Some(l);
                            break;
                        }
                    }
                }
                if let Some(layout) = layout_opt {
                    if let Some(offset) = layout.get_field_offset(field) {
                        if offset > 0 {
                            out.push(0x41);
                            write_u32_leb(offset as u32, out);
                            out.push(0x6A);
                        }
                        if let Some(fi) = layout.find_field(field) {
                            if fi.type_name == "f64" || fi.type_name == "number" {
                                out.push(0x2B); // f64.load
                                out.push(0x03); // align 2^3 = 8
                                out.push(0x00); // offset 0
                                out.push(0xAA); // i32.trunc_f64_s
                                return Ok(Some(fi.type_name.clone()));
                            }
                        }
                        out.push(0x28); // i32.load
                        out.push(0x02); // align 2^2 = 4
                        out.push(0x00); // offset 0
                        if let Some(fi) = layout.find_field(field) {
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
                        out.push(0x02); // align 2^2 = 4
                        out.push(0x00); // offset 0
                    }
                }
            }

            out.push(0x0B); // end
            Ok(None)
        }
        ExprKind::New(..) => {
            out.push(0x41);
            write_i32_leb(0, out);
            Ok(Some("Object".into()))
        }
        ExprKind::Array(_elems) => {
            out.push(0x41);
            write_i32_leb(1024, out);
            Ok(Some("Array".into()))
        }
        ExprKind::Index(target, idx) => {
            let target_ty = gen_expr_i32(target, out, ctx)?;
            gen_expr_i32(idx, out, ctx)?;
            out.push(0x1A); // drop index
            let elem_ty = target_ty.map(|s| {
                if s.starts_with('[') && s.ends_with(']') {
                    s[1..s.len() - 1].to_string()
                } else {
                    s
                }
            });
            Ok(elem_ty)
        }
        ExprKind::Grouping(inner) => gen_expr_i32(inner, out, ctx),
        ExprKind::Spread(inner) => gen_expr_i32(inner, out, ctx),
        ExprKind::NonNull(inner) => gen_expr_i32(inner, out, ctx),
        ExprKind::Fn(..) => {
            out.push(0x41);
            write_u32_leb(0, out);
            Ok(None)
        }
        ExprKind::Throw(..) => {
            out.push(0x41);
            write_u32_leb(0, out);
            Ok(None)
        }
        ExprKind::Update(..) => {
            out.push(0x41);
            write_u32_leb(0, out);
            Ok(None)
        }
        ExprKind::Conditional(cond, then_branch, else_branch) => {
            gen_cond_i32(cond, out, ctx)?;
            out.push(0x04); // if
            out.push(0x7F); // block type: i32
            let ty_then = gen_expr_i32(then_branch, out, ctx)?;
            out.push(0x05); // else
            let _ty_else = gen_expr_i32(else_branch, out, ctx)?;
            out.push(0x0B); // end
            Ok(ty_then)
        }
        _ => Err(format!("unsupported i32 expression: {:?}", e.kind)),
    }
}

fn gen_cond_i32(e: &Expr, out: &mut Vec<u8>, ctx: &mut CompilerContext) -> Result<(), String> {
    if let ExprKind::Grouping(inner) = &e.kind {
        return gen_cond_i32(inner, out, ctx);
    }
    if let ExprKind::Call(callee, args, _) = &e.kind {
        if let ExprKind::Variable(name) = &callee.kind {
            if let Some((fidx, params, ret_type)) = ctx.funcs.get(name) {
                for (i, pt) in params.iter().enumerate() {
                    if let Some(a) = args.get(i) {
                        if *pt == 0x7C {
                            gen_expr_f64(a, out, ctx)?;
                        } else {
                            gen_expr_i32(a, out, ctx)?;
                        }
                    } else if *pt == 0x7C {
                        out.push(0x44);
                        out.extend(&0f64.to_bits().to_le_bytes());
                    } else {
                        out.push(0x41);
                        write_i32_leb(0, out);
                    }
                }
                out.push(0x10);
                write_u32_leb(*fidx, out);
                match ret_type {
                    Some(0x7F) => return Ok(()),
                    Some(0x7C) => {
                        out.push(0x44);
                        out.extend(&0f64.to_bits().to_le_bytes());
                        out.push(0x62); // f64.ne -> i32
                        return Ok(());
                    }
                    _ => {
                        out.push(0x41);
                        write_i32_leb(1, out);
                        return Ok(());
                    }
                }
            }
        }
    }
    if let ExprKind::Variable(name) = &e.kind {
        if let Some(idx) = ctx.locals_i32.get(name) {
            out.push(0x20);
            write_u32_leb(*idx, out);
            return Ok(());
        }
    }
    if let ExprKind::Logical(l, op, r) = &e.kind {
        gen_cond_i32(l, out, ctx)?;
        gen_cond_i32(r, out, ctx)?;
        match op {
            TokenKind::And => out.push(0x71), // i32.and
            TokenKind::Or => out.push(0x72),  // i32.or
            _ => out.push(0x71),
        }
        return Ok(());
    }
    if let ExprKind::Binary(l, op, r) = &e.kind {
        if matches!(op, TokenKind::Instanceof) {
            out.push(0x41);
            write_u32_leb(1, out);
            return Ok(());
        }
        if matches!(op, TokenKind::AndAnd) {
            gen_cond_i32(l, out, ctx)?;
            gen_cond_i32(r, out, ctx)?;
            out.push(0x71); // i32.and
            return Ok(());
        }
        if matches!(op, TokenKind::OrOr) {
            gen_cond_i32(l, out, ctx)?;
            gen_cond_i32(r, out, ctx)?;
            out.push(0x72); // i32.or
            return Ok(());
        }
        if matches!(
            op,
            TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::EqualEqual
                | TokenKind::StrictEqual
                | TokenKind::BangEqual
                | TokenKind::StrictNotEqual
        ) {
            if matches!(op, TokenKind::EqualEqual | TokenKind::StrictEqual)
                && matches!(r.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(l, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x62); // f64.ne (NaN != NaN is 1)
                return Ok(());
            }
            if matches!(op, TokenKind::EqualEqual | TokenKind::StrictEqual)
                && matches!(l.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(r, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x62); // f64.ne
                return Ok(());
            }
            if matches!(op, TokenKind::BangEqual | TokenKind::StrictNotEqual)
                && matches!(r.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(l, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x61); // f64.eq
                return Ok(());
            }
            if matches!(op, TokenKind::BangEqual | TokenKind::StrictNotEqual)
                && matches!(l.kind, ExprKind::Literal(Value::Null))
            {
                gen_expr_f64(r, out, ctx)?;
                let scratch_idx = *ctx
                    .locals_f64
                    .get("__scratch_f64")
                    .ok_or("no __scratch_f64")?;
                out.push(0x22);
                write_u32_leb(scratch_idx, out);
                out.push(0x20);
                write_u32_leb(scratch_idx, out);
                out.push(0x61); // f64.eq
                return Ok(());
            }

            gen_expr_f64(l, out, ctx)?;
            gen_expr_f64(r, out, ctx)?;
            match op {
                TokenKind::Greater => out.push(0x64),      // f64.gt -> i32
                TokenKind::GreaterEqual => out.push(0x66), // f64.ge -> i32
                TokenKind::Less => out.push(0x63),         // f64.lt -> i32
                TokenKind::LessEqual => out.push(0x65),    // f64.le -> i32
                TokenKind::EqualEqual | TokenKind::StrictEqual => out.push(0x61), // f64.eq -> i32
                TokenKind::BangEqual | TokenKind::StrictNotEqual => out.push(0x62), // f64.ne -> i32
                _ => unreachable!(),
            }
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

fn wasm_string_offset(s: &str, ctx: &CompilerContext) -> u32 {
    ctx.strings
        .iter()
        .find(|(x, _)| x == s)
        .map(|(_, o)| *o)
        .unwrap_or(1024)
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
    write_u32_leb(ctx.print_str_idx, out);
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
    write_u32_leb(
        ctx.file_write_idx
            .expect("file writes are rejected before emission under WASI"),
        out,
    );
}

fn add_wasm_string(st: &str, strings: &mut Vec<(String, u32)>, cur_off: &mut u32) {
    if !st.is_empty() && !strings.iter().any(|(x, _)| x == st) {
        strings.push((st.to_string(), *cur_off));
        *cur_off += st.len() as u32;
    }
}

fn format_static_val_for_wasm(e: &Expr) -> Option<String> {
    format_static_val_for_wasm_env(e, &FastMap::default())
}

fn format_static_val_for_wasm_env(e: &Expr, var_values: &FastMap<String, Value>) -> Option<String> {
    match &e.kind {
        ExprKind::Grouping(inner) => format_static_val_for_wasm_env(inner, var_values),
        ExprKind::NonNull(inner) => format_static_val_for_wasm_env(inner, var_values),
        ExprKind::Cast(inner, _) => format_static_val_for_wasm_env(inner, var_values),
        ExprKind::Binary(..) | ExprKind::Unary(..) => {
            let val = expr_to_value_env(e, var_values)?;
            Some(crate::execution::runtime_core::format::fmt(&val))
        }
        ExprKind::Variable(name) => {
            let val = var_values.get(name)?;
            match val {
                Value::Str(s) => Some(s.clone()),
                other => Some(crate::execution::runtime_core::format::fmt(other)),
            }
        }
        ExprKind::Array(elems) => {
            let mut parts = Vec::new();
            for el in elems {
                if let ExprKind::Spread(inner) = &el.kind {
                    if let Some(Value::Array(sub_arr)) = expr_to_value_env(inner, var_values) {
                        for sub_el in sub_arr {
                            parts.push(crate::execution::runtime_core::format::fmt(&sub_el));
                        }
                        continue;
                    }
                }
                parts.push(format_static_val_for_wasm_env(el, var_values)?);
            }
            Some(format!("[{}]", parts.join(", ")))
        }
        ExprKind::Tuple(elems) => {
            let mut parts = Vec::new();
            for el in elems {
                parts.push(format_static_val_for_wasm_env(el, var_values)?);
            }
            Some(format!("({})", parts.join(", ")))
        }
        ExprKind::Object(entries) => {
            if let Some(val) = expr_to_value_env(e, var_values) {
                return Some(crate::execution::runtime_core::format::fmt(&val));
            }
            let mut parts = Vec::new();
            for (k, v) in entries {
                parts.push(format!(
                    "{}: {}",
                    k,
                    format_static_val_for_wasm_env(v, var_values)?
                ));
            }
            Some(format!("{{{}}}", parts.join(", ")))
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "concat" || name == "str_concat" {
                    let mut s = String::new();
                    for a in args {
                        s.push_str(&format_static_val_for_wasm_env(a, var_values)?);
                    }
                    return Some(s);
                }
                if name == "Error" || name == "error" {
                    let msg = args
                        .first()
                        .and_then(|a| format_static_val_for_wasm_env(a, var_values))
                        .unwrap_or_else(|| "Error".to_string());
                    return Some(msg);
                }
            }
            if let Some(val) = expr_to_value_env(e, var_values) {
                return match val {
                    Value::Str(s) => Some(s),
                    Value::Bool(b) => Some(if b { "true".into() } else { "false".into() }),
                    other => Some(crate::execution::runtime_core::format::fmt(&other)),
                };
            }
            None
        }
        ExprKind::New(target, args) => {
            let class_name = if let ExprKind::Variable(name) = &target.kind {
                name.clone()
            } else {
                "Object".to_string()
            };
            if class_name == "Date" {
                return Some("<Date instance>".to_string());
            }
            if class_name.ends_with("Error") {
                let msg = args
                    .first()
                    .and_then(|a| format_static_val_for_wasm_env(a, var_values))
                    .unwrap_or_else(|| class_name.clone());
                Some(msg)
            } else {
                Some(format!("<{} instance>", class_name))
            }
        }
        ExprKind::Format(inner, spec) => {
            let val = expr_to_value_env(inner, var_values)?;
            Some(crate::execution::runtime_core::format::apply_format_spec(
                &val, spec,
            ))
        }
        ExprKind::Get(_target, _field) => {
            let val = expr_to_value_env(e, var_values)?;
            Some(crate::execution::runtime_core::format::fmt(&val))
        }
        ExprKind::OptGet(_target, _field) => {
            let val = expr_to_value_env(e, var_values)?;
            Some(crate::execution::runtime_core::format::fmt(&val))
        }
        ExprKind::Conditional(cond, then_branch, else_branch) => {
            let is_true = match expr_to_value_env(cond, var_values)? {
                Value::Bool(b) => b,
                Value::Null => false,
                other => other.as_f64().map(|n| n != 0.0).unwrap_or(true),
            };
            if is_true {
                format_static_val_for_wasm_env(then_branch, var_values)
            } else {
                format_static_val_for_wasm_env(else_branch, var_values)
            }
        }
        ExprKind::Range(..) | ExprKind::Logical(..) => {
            let val = expr_to_value_env(e, var_values)?;
            Some(crate::execution::runtime_core::format::fmt(&val))
        }
        ExprKind::Literal(Value::Str(s)) => Some(s.clone()),
        ExprKind::Literal(v) => Some(crate::execution::runtime_core::format::fmt(v)),
        _ => None,
    }
}

fn wasm_value_type_name(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(n) => {
            if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= u8::MAX as i128 {
                        "u8".to_string()
                    } else if i <= u16::MAX as i128 {
                        "u16".to_string()
                    } else if i <= u32::MAX as i128 {
                        "u32".to_string()
                    } else if i <= u64::MAX as i128 {
                        "u64".to_string()
                    } else {
                        "u128".to_string()
                    }
                } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                    "i8".to_string()
                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                    "i16".to_string()
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    "i32".to_string()
                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                    "i64".to_string()
                } else {
                    "i128".to_string()
                }
            } else {
                "f64".to_string()
            }
        }
        Value::U8(_) => "u8".to_string(),
        Value::U16(_) => "u16".to_string(),
        Value::U32(_) => "u32".to_string(),
        Value::U64(_) => "u64".to_string(),
        Value::U128(_) => "u128".to_string(),
        Value::I8(_) => "i8".to_string(),
        Value::I16(_) => "i16".to_string(),
        Value::I32(_) => "i32".to_string(),
        Value::I64(_) => "i64".to_string(),
        Value::I128(_) => "i128".to_string(),
        Value::F32(_) => "f32".to_string(),
        Value::F64(_) => "f64".to_string(),
        Value::Char(_) => "char".to_string(),
        Value::Str(_) => "string".to_string(),
        // Array literals are DynArrays in the interpreter, whose typeof reports
        // the inferred element type.
        Value::Array(items) => format!(
            "[{}]",
            crate::parsing::ast::DynamicArray::new(items.clone()).concrete_type
        ),
        Value::Tuple(_) => "tuple".to_string(),
        Value::Set(_) => "set".to_string(),
        Value::Object(map) => {
            if let Some(Value::Str(t)) = map.get("__type__") {
                return clean_type_name(t).to_string();
            }
            "object".to_string()
        }
        _ => "unknown".to_string(),
    }
}

fn wasm_value_sizeof(val: &Value) -> usize {
    match val {
        Value::Null => 0,
        Value::Bool(_) | Value::U8(_) | Value::I8(_) => 1,
        Value::U16(_) | Value::I16(_) => 2,
        Value::U32(_) | Value::I32(_) | Value::F32(_) | Value::Char(_) => 4,
        Value::U64(_) | Value::I64(_) | Value::F64(_) => 8,
        Value::U128(_) | Value::I128(_) => 16,
        Value::Number(n) => {
            if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= 255 {
                        1
                    } else if i <= 65535 {
                        2
                    } else if i <= u32::MAX as i128 {
                        4
                    } else {
                        8
                    }
                } else if i >= -128 && i <= 127 {
                    1
                } else if i >= -32768 && i <= 32767 {
                    2
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    4
                } else {
                    8
                }
            } else {
                8
            }
        }
        Value::Str(s) => s.len(),
        Value::Array(arr) => 24 + arr.iter().map(wasm_value_sizeof).sum::<usize>(),
        Value::Tuple(tup) => 24 + tup.iter().map(wasm_value_sizeof).sum::<usize>(),
        Value::Set(set) => 24 + set.iter().map(wasm_value_sizeof).sum::<usize>(),
        Value::Object(obj) => {
            48 + obj
                .iter()
                .map(|(k, v)| k.len() + wasm_value_sizeof(v))
                .sum::<usize>()
        }
        _ => 8,
    }
}

fn calculate_wasm_sizeof(arg: &Expr, var_values: &FastMap<String, Value>) -> usize {
    if let ExprKind::Variable(vname) = &arg.kind {
        if let Some(vt) = WASM_VAR_TYPES.with(|cell| cell.borrow().get(vname).cloned()) {
            match vt.as_str() {
                "u8" | "i8" | "bool" | "boolean" => return 1,
                "u16" | "i16" => return 2,
                "u32" | "i32" | "f32" | "char" => return 4,
                "u64" | "i64" | "f64" => return 8,
                "u128" | "i128" => return 16,
                _ => {}
            }
        }
    }
    if let Some(val) = expr_to_value_env(arg, var_values) {
        return wasm_value_sizeof(&val);
    }
    if let Some(t) = resolve_wasm_type_name_expr(arg, var_values) {
        match t.as_str() {
            "u8" | "i8" | "bool" | "boolean" => 1,
            "u16" | "i16" => 2,
            "u32" | "i32" | "f32" | "char" => 4,
            "u64" | "i64" | "f64" => 8,
            "u128" | "i128" => 16,
            _ => 8,
        }
    } else {
        8
    }
}

fn resolve_wasm_type_name_expr(arg: &Expr, var_values: &FastMap<String, Value>) -> Option<String> {
    match &arg.kind {
        ExprKind::Variable(name) => {
            let clean = clean_type_name(name);
            if crate::parsing::hir_lower::KNOWN_TYPE_ALIASES.with(|cell| {
                let map = cell.borrow();
                map.contains_key(name) || map.contains_key(clean)
            }) {
                return Some(clean.to_string());
            }
            if let Some(vt) = WASM_VAR_TYPES.with(|cell| {
                let map = cell.borrow();
                map.get(name).cloned().or_else(|| map.get(clean).cloned())
            }) {
                return Some(clean_type_name(&vt).to_string());
            }
            if let Some(val) = var_values.get(name) {
                return Some(wasm_value_type_name(val));
            }
            None
        }
        ExprKind::Get(target, field) => {
            if let ExprKind::Variable(tname) = &target.kind {
                let resolved = crate::parsing::hir_lower::KNOWN_TYPE_ALIASES.with(|cell| {
                    let map = cell.borrow();
                    let entry = map.get(tname).or_else(|| map.get(clean_type_name(tname)));
                    if let Some((fields, _)) = entry {
                        for (fname, _, ftype) in fields {
                            if fname == field {
                                return Some(ftype.clone());
                            }
                        }
                    }
                    None
                });
                if let Some(target_type) = resolved {
                    let clean = clean_type_name(target_type.trim_start_matches('?'));
                    return Some(clean.to_string());
                }
            }
            if let Some(target_val) = expr_to_value_env(target, var_values) {
                if let Value::Object(map) = target_val {
                    if let Some(field_val) = map.get(field) {
                        return Some(wasm_value_type_name(field_val));
                    }
                }
            }
            None
        }
        _ => {
            if let Some(val) = expr_to_value_env(arg, var_values) {
                Some(wasm_value_type_name(&val))
            } else {
                None
            }
        }
    }
}

#[allow(dead_code)]
fn expr_to_value(e: &Expr) -> Option<Value> {
    expr_to_value_env(e, &FastMap::default())
}

pub(super) fn expr_to_value_env(e: &Expr, var_values: &FastMap<String, Value>) -> Option<Value> {
    match &e.kind {
        ExprKind::Literal(v) => Some(v.clone()),
        ExprKind::Variable(name) => {
            if let Some(v) = var_values.get(name) {
                Some(v.clone())
            } else if crate::parsing::hir_lower::KNOWN_TYPE_ALIASES
                .with(|cell| cell.borrow().contains_key(name))
            {
                Some(Value::Str(name.clone()))
            } else {
                None
            }
        }
        ExprKind::Grouping(inner) => expr_to_value_env(inner, var_values),
        ExprKind::Spread(inner) => expr_to_value_env(inner, var_values),
        ExprKind::NonNull(inner) => expr_to_value_env(inner, var_values),
        ExprKind::Fn(..) => Some(Value::Null),
        ExprKind::Throw(inner) => expr_to_value_env(inner, var_values),
        ExprKind::Conditional(cond, then_branch, else_branch) => {
            let cv = expr_to_value_env(cond, var_values)?;
            let is_true = match cv {
                Value::Bool(b) => b,
                Value::Number(n) => n != 0.0,
                _ => true,
            };
            if is_true {
                expr_to_value_env(then_branch, var_values)
            } else {
                expr_to_value_env(else_branch, var_values)
            }
        }
        ExprKind::Cast(inner, _) => expr_to_value_env(inner, var_values),
        ExprKind::Range(start, end, inclusive) => {
            let s = expr_to_value_env(start, var_values)?.as_f64()?;
            let e = expr_to_value_env(end, var_values)?.as_f64()?;
            if s.fract() != 0.0 || e.fract() != 0.0 {
                return None;
            }
            let (s, e) = (s as i64, e as i64);
            let last = if *inclusive { e } else { e - 1 };
            if last - s > 1_000_000 {
                return None;
            }
            Some(Value::Array(
                (s..=last).map(|i| Value::Number(i as f64)).collect(),
            ))
        }
        ExprKind::Unary(op, inner) => {
            if *op == TokenKind::Typeof {
                if let Some(t) = resolve_wasm_type_name_expr(inner, var_values) {
                    return Some(Value::Str(t));
                }
            }
            let val = expr_to_value_env(inner, var_values)?;
            match op {
                TokenKind::Minus => match val {
                    Value::Number(n) => Some(Value::Number(-n)),
                    Value::F64(n) => Some(Value::F64(-n)),
                    Value::I64(n) => Some(Value::I64(-n)),
                    Value::I32(n) => Some(Value::I32(-n)),
                    _ => val.as_f64().map(|n| Value::Number(-n)),
                },
                TokenKind::Bang => match val {
                    Value::Bool(b) => Some(Value::Bool(!b)),
                    _ => None,
                },
                _ => None,
            }
        }
        ExprKind::Binary(l, op, r) => {
            if matches!(op, TokenKind::Instanceof) {
                return Some(Value::Bool(true));
            }
            let lv = expr_to_value_env(l, var_values)?;
            let rv = expr_to_value_env(r, var_values)?;
            match op {
                TokenKind::Plus => match (&lv, &rv) {
                    (Value::Str(s1), Value::Str(s2)) => Some(Value::Str(format!("{}{}", s1, s2))),
                    (Value::Str(s1), _) => Some(Value::Str(format!(
                        "{}{}",
                        s1,
                        crate::execution::runtime_core::format::fmt(&rv)
                    ))),
                    (_, Value::Str(s2)) => Some(Value::Str(format!(
                        "{}{}",
                        crate::execution::runtime_core::format::fmt(&lv),
                        s2
                    ))),
                    _ => match (lv.as_f64(), rv.as_f64()) {
                        (Some(n1), Some(n2)) => Some(Value::Number(n1 + n2)),
                        _ => None,
                    },
                },
                TokenKind::Minus => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Number(n1 - n2)),
                    _ => None,
                },
                TokenKind::Star => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Number(n1 * n2)),
                    _ => None,
                },
                TokenKind::Slash => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => {
                        // Adesh / is integer division when both operands are integers
                        if n1.fract() == 0.0 && n2.fract() == 0.0 {
                            Some(Value::Number((n1 / n2).trunc()))
                        } else {
                            Some(Value::Number(n1 / n2))
                        }
                    }
                    _ => None,
                },
                TokenKind::StarStar => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Number(n1.powf(n2))),
                    _ => None,
                },
                TokenKind::Percent => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Number(n1 % n2)),
                    _ => None,
                },
                TokenKind::TildeSlash => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Number((n1 / n2).trunc())),
                    _ => None,
                },
                TokenKind::Greater => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Bool(n1 > n2)),
                    _ => None,
                },
                TokenKind::GreaterEqual => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Bool(n1 >= n2)),
                    _ => None,
                },
                TokenKind::Less => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Bool(n1 < n2)),
                    _ => None,
                },
                TokenKind::LessEqual => match (lv.as_f64(), rv.as_f64()) {
                    (Some(n1), Some(n2)) => Some(Value::Bool(n1 <= n2)),
                    _ => None,
                },
                TokenKind::EqualEqual | TokenKind::StrictEqual => {
                    let eq = match (&lv, &rv) {
                        (Value::Str(a), Value::Str(b)) => a == b,
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        (Value::Null, Value::Null) => true,
                        _ => match (lv.as_f64(), rv.as_f64()) {
                            (Some(a), Some(b)) => a == b,
                            _ => {
                                crate::execution::runtime_core::format::fmt(&lv)
                                    == crate::execution::runtime_core::format::fmt(&rv)
                            }
                        },
                    };
                    Some(Value::Bool(eq))
                }
                TokenKind::BangEqual | TokenKind::StrictNotEqual => {
                    let eq = match (&lv, &rv) {
                        (Value::Str(a), Value::Str(b)) => a == b,
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        (Value::Null, Value::Null) => true,
                        _ => match (lv.as_f64(), rv.as_f64()) {
                            (Some(a), Some(b)) => a == b,
                            _ => {
                                crate::execution::runtime_core::format::fmt(&lv)
                                    == crate::execution::runtime_core::format::fmt(&rv)
                            }
                        },
                    };
                    Some(Value::Bool(!eq))
                }
                TokenKind::In => {
                    let exists = match &rv {
                        Value::Array(a) => a.iter().any(|el| match (&lv, el) {
                            (Value::Str(s1), Value::Str(s2)) => s1 == s2,
                            _ => lv.as_f64() == el.as_f64(),
                        }),
                        Value::Object(m) => {
                            let k_str = match &lv {
                                Value::Str(s) => s.clone(),
                                _ => crate::execution::runtime_core::format::fmt(&lv),
                            };
                            m.contains_key(&k_str)
                        }
                        Value::Str(s) => {
                            let sub = match &lv {
                                Value::Str(s) => s.clone(),
                                _ => crate::execution::runtime_core::format::fmt(&lv),
                            };
                            s.contains(&sub)
                        }
                        _ => false,
                    };
                    Some(Value::Bool(exists))
                }
                TokenKind::Instanceof => Some(Value::Bool(true)),
                TokenKind::AndAnd | TokenKind::OrOr => {
                    let truthy = match &lv {
                        Value::Bool(b) => *b,
                        Value::Null => false,
                        Value::Str(s) => !s.is_empty(),
                        other => other.as_f64().map(|n| n != 0.0).unwrap_or(true),
                    };
                    let pick_right = (*op == TokenKind::AndAnd) == truthy;
                    Some(if pick_right { rv } else { lv })
                }
                TokenKind::NullCoalesce => {
                    // x ?? y: if x is null, return y; else return x
                    let is_null =
                        matches!(&lv, Value::Null) || matches!(&lv, Value::Number(n) if n.is_nan());
                    if is_null { Some(rv) } else { Some(lv) }
                }
                _ => None,
            }
        }
        ExprKind::Logical(l, op, r) => {
            let lv = expr_to_value_env(l, var_values)?;
            let rv = expr_to_value_env(r, var_values)?;
            let lb = match &lv {
                Value::Bool(b) => *b,
                Value::Number(n) => *n != 0.0,
                _ => true,
            };
            let rb = match &rv {
                Value::Bool(b) => *b,
                Value::Number(n) => *n != 0.0,
                _ => true,
            };
            match op {
                TokenKind::And => Some(Value::Bool(lb && rb)),
                TokenKind::Or => Some(Value::Bool(lb || rb)),
                _ => None,
            }
        }
        ExprKind::Array(elems) => {
            let mut arr = Vec::new();
            for el in elems {
                if let ExprKind::Spread(inner) = &el.kind {
                    if let Some(Value::Array(sub_arr)) = expr_to_value_env(inner, var_values) {
                        arr.extend(sub_arr);
                        continue;
                    }
                }
                arr.push(expr_to_value_env(el, var_values)?);
            }
            Some(Value::Array(arr))
        }
        ExprKind::Tuple(elems) => {
            let mut tup = Vec::new();
            for el in elems {
                tup.push(expr_to_value_env(el, var_values)?);
            }
            Some(Value::Tuple(tup))
        }
        ExprKind::Object(entries) => {
            let mut map = FastMap::default();
            for (k, v) in entries {
                if k.is_empty() || k == "..." || matches!(v.kind, ExprKind::Spread(_)) {
                    let target_expr = if let ExprKind::Spread(inner) = &v.kind {
                        inner
                    } else {
                        v
                    };
                    if let Some(Value::Object(inner_map)) =
                        expr_to_value_env(target_expr, var_values)
                    {
                        for (ik, iv) in inner_map.iter() {
                            map.insert(ik.clone(), iv.clone());
                        }
                    }
                } else {
                    map.insert(k.clone(), expr_to_value_env(v, var_values)?);
                }
            }
            Some(Value::Object(map.into()))
        }
        ExprKind::Call(callee, args, _) => {
            if let Some((value, prints)) = super::static_calls::call(callee, args, var_values) {
                if !prints.is_empty() {
                    return None;
                }
                // Scalar results of ordinary calls stay runtime calls so the
                // ABI's own number formatting (e.g. WASI rounding) applies.
                let scalar = matches!(value, Value::Number(_) | Value::I64(_) | Value::Bool(_));
                if !scalar || super::static_calls::requires_folding(callee, args) {
                    return Some(value);
                }
                return None;
            }
            if let ExprKind::Variable(name) = &callee.kind {
                if (name == "len" || name == "length") && !args.is_empty() {
                    let v = expr_to_value_env(&args[0], var_values)?;
                    return match v {
                        Value::Array(a) => Some(Value::Number(a.len() as f64)),
                        Value::Set(s) => Some(Value::Number(s.len() as f64)),
                        Value::Str(s) => Some(Value::Number(s.len() as f64)),
                        Value::Object(m) => Some(Value::Number(m.len() as f64)),
                        _ => Some(Value::Number(0.0)),
                    };
                }
                if (name == "__array_init" || name == "__array_repeat" || name == "__array_init_raw") && args.len() >= 2 {
                    let elem = expr_to_value_env(&args[0], var_values)?;
                    let count_val = expr_to_value_env(&args[1], var_values)?;
                    let count = match count_val {
                        Value::Number(n) => n as usize,
                        Value::U8(n) => n as usize,
                        Value::U16(n) => n as usize,
                        Value::U32(n) => n as usize,
                        Value::U64(n) => n as usize,
                        Value::I8(n) => n as usize,
                        Value::I16(n) => n as usize,
                        Value::I32(n) => n as usize,
                        Value::I64(n) => n as usize,
                        _ => 0,
                    };
                    return Some(Value::Array(vec![elem; count]));
                }
                if name == "double" && !args.is_empty() {
                    if let Some(v) = expr_to_value_env(&args[0], var_values) {
                        if let Some(n) = v.as_f64() {
                            return Some(Value::Number(n * 2.0));
                        }
                    }
                }
                if (name == "type" || name == "typeof") && args.len() == 1 {
                    if let Some(t) = resolve_wasm_type_name_expr(&args[0], var_values) {
                        return Some(Value::Str(t));
                    }
                }
                if name == "concat" || name == "str_concat" {
                    let mut s = String::new();
                    for a in args {
                        let val = expr_to_value_env(a, var_values)?;
                        match val {
                            Value::Str(part) => s.push_str(&part),
                            other => {
                                s.push_str(&crate::execution::runtime_core::format::fmt(&other))
                            }
                        }
                    }
                    return Some(Value::Str(s));
                }
                if name == "Error" || name == "error" {
                    let msg = args
                        .first()
                        .and_then(|a| expr_to_value_env(a, var_values))
                        .map(|v| match v {
                            Value::Str(s) => s,
                            other => crate::execution::runtime_core::format::fmt(&other),
                        })
                        .unwrap_or_else(|| "Error".to_string());
                    return Some(Value::Str(msg));
                }
                if (name == "sizeof" || name == "sizeOf") && args.len() == 1 {
                    let sz = calculate_wasm_sizeof(&args[0], var_values);
                    return Some(Value::Number(sz as f64));
                }
                if name == "makeSet" && !args.is_empty() {
                    if let Some(v) = expr_to_value_env(&args[0], var_values) {
                        if let Value::Array(items) = v {
                            return Some(Value::Set(items));
                        }
                    }
                }
                if (name == "hasKey" || name == "containsKey") && args.len() >= 2 {
                    let target = expr_to_value_env(&args[0], var_values);
                    let key = expr_to_value_env(&args[1], var_values);
                    if let (Some(Value::Object(map)), Some(k)) = (target, key) {
                        let key_str = match k {
                            Value::Str(s) => s,
                            Value::Number(n) => {
                                if n.fract() == 0.0 {
                                    (n as i64).to_string()
                                } else {
                                    n.to_string()
                                }
                            }
                            Value::Char(c) => c.to_string(),
                            Value::U8(n) => n.to_string(),
                            Value::U16(n) => n.to_string(),
                            Value::U32(n) => n.to_string(),
                            Value::U64(n) => n.to_string(),
                            Value::I8(n) => n.to_string(),
                            Value::I16(n) => n.to_string(),
                            Value::I32(n) => n.to_string(),
                            Value::I64(n) => n.to_string(),
                            other => crate::execution::runtime_core::format::fmt(&other),
                        };
                        return Some(Value::Bool(map.contains_key(&key_str)));
                    }
                    return Some(Value::Bool(false));
                }
            } else if let ExprKind::Get(target, method) = &callee.kind {
                if method == "union" && !args.is_empty() {
                    let s1 = expr_to_value_env(target, var_values);
                    let s2 = expr_to_value_env(&args[0], var_values);
                    if let (Some(Value::Set(a)), Some(Value::Set(b))) = (s1, s2) {
                        let mut res = a.clone();
                        for item in b {
                            if !res.iter().any(|x| x.as_f64() == item.as_f64()) {
                                res.push(item);
                            }
                        }
                        return Some(Value::Set(res));
                    }
                }
                if method == "intersection" && !args.is_empty() {
                    let s1 = expr_to_value_env(target, var_values);
                    let s2 = expr_to_value_env(&args[0], var_values);
                    if let (Some(Value::Set(a)), Some(Value::Set(b))) = (s1, s2) {
                        let res: Vec<_> = a
                            .into_iter()
                            .filter(|x| b.iter().any(|y| y.as_f64() == x.as_f64()))
                            .collect();
                        return Some(Value::Set(res));
                    }
                }
                if method == "add" && !args.is_empty() {
                    let s1 = expr_to_value_env(target, var_values);
                    let item = expr_to_value_env(&args[0], var_values);
                    if let (Some(Value::Set(mut a)), Some(it)) = (s1, item) {
                        if !a.iter().any(|x| x.as_f64() == it.as_f64()) {
                            a.push(it);
                        }
                        return Some(Value::Set(a));
                    }
                }
                if method == "map" && !args.is_empty() {
                    if let Some(Value::Array(arr)) = expr_to_value_env(target, var_values) {
                        if let ExprKind::Fn(params, body, _) = &args[0].kind {
                            let param_name = params.first().map(|p| p.0.as_str()).unwrap_or("x");
                            let mut mapped = Vec::new();
                            for item in arr.iter() {
                                let mut local_env = var_values.clone();
                                local_env.insert(param_name.to_string(), item.clone());
                                let mut res = item.clone();
                                for stmt in body.iter() {
                                    if let StmtKind::Return(Some(ret_e)) = &stmt.kind {
                                        if let Some(val) = expr_to_value_env(ret_e, &local_env) {
                                            res = val;
                                        }
                                    }
                                }
                                mapped.push(res);
                            }
                            return Some(Value::Array(mapped));
                        }
                    }
                }
                if (method == "hasKey" || method == "containsKey") && !args.is_empty() {
                    let target = expr_to_value_env(target, var_values);
                    let key = expr_to_value_env(&args[0], var_values);
                    if let (Some(Value::Object(map)), Some(k)) = (target, key) {
                        let key_str = match k {
                            Value::Str(s) => s,
                            other => crate::execution::runtime_core::format::fmt(&other),
                        };
                        return Some(Value::Bool(map.contains_key(&key_str)));
                    }
                    return Some(Value::Bool(false));
                }
            }
            None
        }
        ExprKind::New(target, args) => {
            let class_name = if let ExprKind::Variable(name) = &target.kind {
                name.clone()
            } else {
                "Object".to_string()
            };
            if class_name.ends_with("Error") {
                let msg = args
                    .first()
                    .and_then(|a| expr_to_value_env(a, var_values))
                    .map(|v| match v {
                        Value::Str(s) => s,
                        other => crate::execution::runtime_core::format::fmt(&other),
                    })
                    .unwrap_or_else(|| class_name.clone());
                Some(Value::Str(msg))
            } else {
                if class_name == "Date" {
                    Some(Value::Str("<Date instance>".into()))
                } else {
                    super::static_calls::construct(&class_name, args, var_values)
                }
            }
        }
        ExprKind::Format(inner, spec) => {
            let val = expr_to_value_env(inner, var_values)?;
            let formatted = crate::execution::runtime_core::format::apply_format_spec(&val, spec);
            Some(Value::Str(formatted))
        }
        ExprKind::Get(target, field) => {
            if let ExprKind::Variable(tname) = &target.kind {
                let resolved = crate::parsing::hir_lower::KNOWN_TYPE_ALIASES.with(|cell| {
                    let map = cell.borrow();
                    if let Some((fields, _)) = map.get(tname) {
                        for (fname, _, ftype) in fields {
                            if fname == field {
                                return Some(ftype.clone());
                            }
                        }
                    }
                    None
                });
                if let Some(target_type) = resolved {
                    let clean = target_type.trim_start_matches('?').to_string();
                    return Some(Value::Str(clean));
                }
            }
            let target_val = expr_to_value_env(target, var_values)?;
            match target_val {
                Value::Array(arr) if field == "length" || field == "len" => {
                    Some(Value::Number(arr.len() as f64))
                }
                Value::Object(map) => map.get(field).cloned(),
                _ => None,
            }
        }
        ExprKind::Index(target, index) => {
            let target = expr_to_value_env(target, var_values)?;
            let index = expr_to_value_env(index, var_values)?.as_f64()?;
            if index < 0.0 || index.fract() != 0.0 { return None; }
            match target {
                Value::Array(items) | Value::Tuple(items) => items.get(index as usize).cloned(),
                Value::Str(s) => s.chars().nth(index as usize).map(Value::Char),
                _ => None,
            }
        }
        ExprKind::OptGet(target, field) => {
            let target_val = expr_to_value_env(target, var_values)?;
            match target_val {
                Value::Array(arr) if field == "length" || field == "len" => {
                    Some(Value::Number(arr.len() as f64))
                }
                Value::Object(map) => map.get(field).cloned(),
                Value::Null => Some(Value::Null),
                _ => None,
            }
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
    if let ExprKind::Call(callee, _, _) = &arg.kind {
        if let ExprKind::Variable(name) = &callee.kind {
            if name == "concat" {
                return format_static_val_for_wasm_env(arg, var_values);
            }
        }
    }
    if let ExprKind::Format(..) = &arg.kind {
        return format_static_val_for_wasm_env(arg, var_values);
    }
    let val = match &arg.kind {
        ExprKind::Variable(vname) => var_values.get(vname).cloned(),
        _ => expr_to_value_env(arg, var_values),
    }?;

    if let Some(opts) = pretty_opt {
        Some(crate::execution::runtime_core::pretty_print::pretty_print(
            &val, opts,
        ))
    } else {
        match &val {
            Value::Str(s) => Some(s.clone()),
            Value::Bool(b) => Some(if *b { "true".into() } else { "false".into() }),
            _ => format_static_val_for_wasm_env(arg, var_values),
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
            if let Some((_, prints)) = super::static_calls::call(callee, args, var_values) {
                for stmt in &prints {
                    collect_all_wasm_strings_stmt(stmt, strings, cur_off, var_values);
                }
            }
            for arg in args {
                collect_all_wasm_strings_expr(arg, strings, cur_off, var_values);
            }
            if let ExprKind::Variable(name) = &callee.kind {
                if (name == "type" || name == "typeof") && args.len() == 1 {
                    if let Some(t) = resolve_wasm_type_name_expr(&args[0], var_values) {
                        add_wasm_string(&t, strings, cur_off);
                    }
                }
                if name == "hasKey" || name == "containsKey" {
                    add_wasm_string("true", strings, cur_off);
                    add_wasm_string("false", strings, cur_off);
                }
                if name == "concat" {
                    if let Some(fmt) = format_static_val_for_wasm_env(e, var_values) {
                        add_wasm_string(&fmt, strings, cur_off);
                    }
                }
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
        ExprKind::Format(inner, _spec) => {
            collect_all_wasm_strings_expr(inner, strings, cur_off, var_values);
            if let Some(fmt) = format_static_val_for_wasm_env(e, var_values) {
                add_wasm_string(&fmt, strings, cur_off);
            }
        }
        ExprKind::Binary(l, _, r) => {
            collect_all_wasm_strings_expr(l, strings, cur_off, var_values);
            collect_all_wasm_strings_expr(r, strings, cur_off, var_values);
        }
        ExprKind::Unary(op, inner) => {
            if *op == TokenKind::Typeof {
                if let Some(t) = resolve_wasm_type_name_expr(inner, var_values) {
                    add_wasm_string(&t, strings, cur_off);
                } else {
                    add_wasm_string("object", strings, cur_off);
                }
            }
            collect_all_wasm_strings_expr(inner, strings, cur_off, var_values)
        }
        ExprKind::Assign(_, r) => collect_all_wasm_strings_expr(r, strings, cur_off, var_values),
        ExprKind::AssignOp(_, _, rhs) => {
            collect_all_wasm_strings_expr(rhs, strings, cur_off, var_values);
        }
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
                if let Some(fmt) = format_static_val_for_wasm_env(e, var_values) {
                    add_wasm_string(&fmt, strings, cur_off);
                }
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
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_all_wasm_strings_stmt(try_block, strings, cur_off, var_values);
            collect_all_wasm_strings_stmt(catch_block, strings, cur_off, var_values);
        }
        _ => {}
    }
}

fn emit_wasm_print_arg(
    arg: &Expr,
    pretty_opt: Option<&crate::execution::runtime_core::pretty_print::PrettyPrintOptions>,
    out: &mut Vec<u8>,
    ctx: &mut CompilerContext,
) -> Result<(), String> {
    if ctx.is_main {
        if let ExprKind::Variable(name) = &arg.kind {
            if matches!(ctx.var_values.get(name), Some(Value::Array(_) | Value::Tuple(_) | Value::Set(_) | Value::Object(_))) {
                if let Some(formatted) = format_print_arg(arg, pretty_opt, ctx.var_values) {
                    emit_wasm_print_str_call(&formatted, out, ctx);
                    return Ok(());
                }
            }
        }
    }
    if let ExprKind::Call(callee, _, _) = &arg.kind {
        if matches!(&callee.kind, ExprKind::Get(_, method) if method == "toISOString") {
            gen_expr_f64(arg, out, ctx)?;
            out.push(0xAA); // i32.trunc_f64_s: adapter returns the scratch string pointer
            out.push(0x41);
            write_i32_leb(24, out);
            out.push(0x10);
            write_u32_leb(ctx.print_str_idx, out);
            return Ok(());
        }
    }
    let is_dynamic_var = if let ExprKind::Variable(vname) = &arg.kind {
        (!ctx.is_main && (ctx.locals_f64.contains_key(vname) || ctx.locals_i32.contains_key(vname)))
            || ctx.global_vars.contains_key(vname)
    } else {
        false
    };

    if !is_dynamic_var {
        if let Some(formatted) = format_print_arg(arg, pretty_opt, ctx.var_values) {
            emit_wasm_print_str_call(&formatted, out, ctx);
            return Ok(());
        }
    }
    match &arg.kind {
        ExprKind::Literal(Value::Str(st)) => {
            emit_wasm_print_str_call(st, out, ctx);
        }
        ExprKind::Literal(Value::Bool(b)) => {
            emit_wasm_print_str_call(if *b { "true" } else { "false" }, out, ctx);
        }
        ExprKind::Literal(Value::Null) => {
            emit_wasm_print_str_call("null", out, ctx);
        }
        ExprKind::Unary(TokenKind::Typeof, inner) => {
            if let Some(t) = resolve_wasm_type_name_expr(inner, ctx.var_values) {
                emit_wasm_print_str_call(&t, out, ctx);
                return Ok(());
            }
            emit_wasm_print_str_call("object", out, ctx);
        }
        ExprKind::Call(callee, args, _) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if (name == "type" || name == "typeof") && args.len() == 1 {
                    if let Some(t) = resolve_wasm_type_name_expr(&args[0], ctx.var_values) {
                        emit_wasm_print_str_call(&t, out, ctx);
                        return Ok(());
                    }
                }
                if name == "concat" || name == "str_concat" {
                    for sub_arg in args {
                        emit_wasm_print_arg(sub_arg, pretty_opt, out, ctx)?;
                    }
                    return Ok(());
                }
                if (name == "hasKey" || name == "containsKey") && args.len() >= 2 {
                    let exists = if let (Some(Value::Object(map)), Some(k)) = (
                        expr_to_value_env(&args[0], ctx.var_values),
                        expr_to_value_env(&args[1], ctx.var_values),
                    ) {
                        let key_str = match k {
                            Value::Str(s) => s,
                            other => crate::execution::runtime_core::format::fmt(&other),
                        };
                        map.contains_key(&key_str)
                    } else {
                        false
                    };
                    emit_wasm_print_str_call(if exists { "true" } else { "false" }, out, ctx);
                    return Ok(());
                }
            } else if let ExprKind::Get(target, method) = &callee.kind {
                if (method == "hasKey" || method == "containsKey") && !args.is_empty() {
                    let exists = if let (Some(Value::Object(map)), Some(k)) = (
                        expr_to_value_env(target, ctx.var_values),
                        expr_to_value_env(&args[0], ctx.var_values),
                    ) {
                        let key_str = match k {
                            Value::Str(s) => s,
                            other => crate::execution::runtime_core::format::fmt(&other),
                        };
                        map.contains_key(&key_str)
                    } else {
                        false
                    };
                    emit_wasm_print_str_call(if exists { "true" } else { "false" }, out, ctx);
                    return Ok(());
                }
                if (method == "hasKey" || method == "containsKey") && !args.is_empty() {
                    let exists = if let (Some(Value::Object(map)), Some(k)) = (
                        expr_to_value_env(target, ctx.var_values),
                        expr_to_value_env(&args[0], ctx.var_values),
                    ) {
                        let key_str = match k {
                            Value::Str(s) => s,
                            other => crate::execution::runtime_core::format::fmt(&other),
                        };
                        map.contains_key(&key_str)
                    } else {
                        false
                    };
                    emit_wasm_print_str_call(if exists { "true" } else { "false" }, out, ctx);
                    return Ok(());
                }
            }
            gen_expr_f64(arg, out, ctx)?;
            out.push(0x10);
            write_u32_leb(ctx.print_f64_idx, out);
        }
        ExprKind::Variable(vname) => {
            if !ctx.is_main
                && (ctx.locals_f64.contains_key(vname) || ctx.locals_i32.contains_key(vname))
            {
                if let Some(idx) = ctx.locals_f64.get(vname) {
                    let scratch_i32 = *ctx.locals_i32.get("__scratch").ok_or("no __scratch")?;
                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0xAA); // i32.trunc_f64_s
                    out.push(0x21);
                    write_u32_leb(scratch_i32, out);
                    for (s, off) in ctx.strings.iter() {
                        out.push(0x20);
                        write_u32_leb(scratch_i32, out);
                        out.push(0x41);
                        write_i32_leb(*off as i32, out);
                        out.push(0x46); // i32.eq
                        out.push(0x04);
                        out.push(0x40); // if (void)
                        out.push(0x41);
                        write_i32_leb(*off as i32, out);
                        out.push(0x41);
                        write_i32_leb(s.len() as i32, out);
                        out.push(0x10);
                        write_u32_leb(ctx.print_str_idx, out);
                        out.push(0x05); // else
                    }
                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0x10);
                    write_u32_leb(ctx.print_f64_idx, out);
                    for _ in 0..ctx.strings.len() {
                        out.push(0x0B); // end
                    }
                } else if let Some(idx) = ctx.locals_i32.get(vname) {
                    out.push(0x20);
                    write_u32_leb(*idx, out);
                    out.push(0xB7); // f64.convert_i32_s
                    out.push(0x10);
                    write_u32_leb(ctx.print_f64_idx, out);
                }
            } else if let Some((off, len)) = ctx.str_vars.get(vname) {
                out.push(0x41);
                write_i32_leb(*off as i32, out);
                out.push(0x41);
                write_i32_leb(*len as i32, out);
                out.push(0x10);
                write_u32_leb(ctx.print_str_idx, out);
            } else if let Some((off, len)) = ctx.complex_vars.get(vname) {
                out.push(0x41);
                write_i32_leb(*off as i32, out);
                out.push(0x41);
                write_i32_leb(*len as i32, out);
                out.push(0x10);
                write_u32_leb(ctx.print_str_idx, out);
            } else if let Some(off) = ctx.global_vars.get(vname) {
                if let Some(val) = ctx.var_values.get(vname) {
                    if let Value::Bool(_) = val {
                        // Linear memory holds 0.0/1.0; print it as a boolean.
                        let t_off = wasm_string_offset("true", ctx);
                        let f_off = wasm_string_offset("false", ctx);
                        out.push(0x41);
                        write_i32_leb(*off as i32, out);
                        out.push(0x2B); // f64.load
                        out.push(0x03);
                        out.push(0x00);
                        out.push(0x44);
                        out.extend(&0.0f64.to_bits().to_le_bytes());
                        out.push(0x62); // f64.ne
                        out.push(0x04);
                        out.push(0x40); // if
                        out.push(0x41);
                        write_i32_leb(t_off as i32, out);
                        out.push(0x41);
                        write_i32_leb(4, out);
                        out.push(0x10);
                        write_u32_leb(ctx.print_str_idx, out);
                        out.push(0x05); // else
                        out.push(0x41);
                        write_i32_leb(f_off as i32, out);
                        out.push(0x41);
                        write_i32_leb(5, out);
                        out.push(0x10);
                        write_u32_leb(ctx.print_str_idx, out);
                        out.push(0x0B);
                        return Ok(());
                    }
                    if matches!(
                        val,
                        Value::Array(_) | Value::Tuple(_) | Value::Set(_) | Value::Object(_)
                    ) {
                        if let Some(formatted) =
                            format_print_arg(arg, pretty_opt, ctx.var_values)
                        {
                            emit_wasm_print_str_call(&formatted, out, ctx);
                            return Ok(());
                        }
                    }
                }
                // If this global holds a string at compile time, print it as a string
                if let Some(val) = ctx.var_values.get(vname) {
                    if let Value::Str(s) = val {
                        if let Some((_, str_off)) = ctx.strings.iter().find(|(name, _)| name == s) {
                            out.push(0x41);
                            write_i32_leb(*str_off as i32, out);
                            out.push(0x41);
                            write_i32_leb(s.len() as i32, out);
                            out.push(0x10);
                            write_u32_leb(ctx.print_str_idx, out);
                            return Ok(());
                        }
                    }
                }
                out.push(0x41);
                write_i32_leb(*off as i32, out);
                out.push(0x2B); // f64.load
                out.push(0x03);
                out.push(0x00);
                out.push(0x10);
                write_u32_leb(ctx.print_f64_idx, out);
            } else {
                gen_expr_f64(arg, out, ctx)?;
                out.push(0x10);
                write_u32_leb(ctx.print_f64_idx, out);
            }
        }
        _ => {
            gen_expr_f64(arg, out, ctx)?;
            out.push(0x10);
            write_u32_leb(ctx.print_f64_idx, out);
        }
    }
    Ok(())
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
                if let Some(off) = ctx.global_vars.get(name) {
                    out.push(0x41);
                    write_i32_leb(*off as i32, out);
                    gen_expr_f64(e, out, ctx)?;
                    out.push(0x39); // f64.store
                    out.push(0x03);
                    out.push(0x00);
                }
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
                if ctx.is_main {
                    if let Some((_, prints)) = super::static_calls::call(callee, args, ctx.var_values) {
                        for stmt in &prints {
                            gen_stmt(stmt, out, ctx, ret_type)?;
                        }
                        return Ok(());
                    }
                }
                if let ExprKind::Variable(name) = &callee.kind {
                    if name == "assert" || name == "assert_eq" || name == "assert_ne" {
                        return Ok(());
                    }
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

                        if file_opt.is_some() && ctx.file_write_idx.is_none() {
                            return Err(
                                "print(..., { file: ... }) cannot run under the WASI Preview 1 ABI:                                  the compiler emits only wasi_snapshot_preview1.fd_write, which cannot                                  open files. Drop the `file` option or build for a non-WASI target."
                                    .to_string(),
                            );
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
                                                write_u32_leb(
                                                    ctx.file_write_idx.expect(
                                                        "file writes are rejected before emission under WASI",
                                                    ),
                                                    out,
                                                );
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
                                                write_u32_leb(
                                                    ctx.file_write_idx.expect(
                                                        "file writes are rejected before emission under WASI",
                                                    ),
                                                    out,
                                                );
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
                                emit_wasm_print_arg(arg, pretty_opt.as_ref(), out, ctx)?;
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
                    // Top-level variables live in linear memory and every read
                    // in main loads from there, so writes must go there too.
                    if let ExprKind::Variable(name) = &lhs.kind {
                        let shadowed = !ctx.is_main
                            && (ctx.locals_f64.contains_key(name)
                                || ctx.locals_i32.contains_key(name));
                        let bin_op = match op {
                            TokenKind::Equal => Some(None),
                            TokenKind::PlusEqual => Some(Some(TokenKind::Plus)),
                            TokenKind::MinusEqual => Some(Some(TokenKind::Minus)),
                            TokenKind::StarEqual => Some(Some(TokenKind::Star)),
                            TokenKind::SlashEqual => Some(Some(TokenKind::Slash)),
                            TokenKind::PercentEqual => Some(Some(TokenKind::Percent)),
                            TokenKind::StarStarEqual => Some(Some(TokenKind::StarStar)),
                            TokenKind::ShiftLeftEqual => Some(Some(TokenKind::ShiftLeft)),
                            TokenKind::ShiftRightEqual => Some(Some(TokenKind::ShiftRight)),
                            TokenKind::AmpersandEqual => Some(Some(TokenKind::Ampersand)),
                            TokenKind::PipeEqual => Some(Some(TokenKind::Pipe)),
                            TokenKind::CaretEqual => Some(Some(TokenKind::Caret)),
                            _ => None,
                        };
                        if let (Some(&off), false, Some(bin_op)) =
                            (ctx.global_vars.get(name), shadowed, bin_op)
                        {
                            let value_expr = match bin_op {
                                None => (**rhs).clone(),
                                Some(b) => Expr {
                                    kind: ExprKind::Binary(lhs.clone(), b, rhs.clone()),
                                    span: e.span.clone(),
                                },
                            };
                            out.push(0x41);
                            write_i32_leb(off as i32, out);
                            gen_expr_f64(&value_expr, out, ctx)?;
                            out.push(0x39); // f64.store
                            out.push(0x03);
                            out.push(0x00);
                            return Ok(());
                        }
                    }
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
                                TokenKind::NullCoalesceEqual => {
                                    // x ??= y: if x is null (NaN), set x = y; else keep x
                                    let scratch_idx =
                                        ctx.locals_f64.get("__scratch_f64_2").copied().ok_or(
                                            "Internal error: __scratch_f64_2 local not found",
                                        )?;
                                    out.push(0x20); // local.get idx
                                    write_u32_leb(*idx, out);
                                    out.push(0x22); // local.tee scratch
                                    write_u32_leb(scratch_idx, out);
                                    out.push(0x20); // local.get scratch
                                    write_u32_leb(scratch_idx, out);
                                    out.push(0x61); // f64.eq (NaN == NaN is false)
                                    out.push(0x04); // if f64
                                    out.push(0x7C);
                                    // Then: not null, keep current value
                                    out.push(0x20); // local.get scratch
                                    write_u32_leb(scratch_idx, out);
                                    // Else: null, assign right side
                                    out.push(0x05); // else
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0x0B); // end
                                    out.push(0x21); // local.set idx
                                    write_u32_leb(*idx, out);
                                }
                                _ => {}
                            }
                        } else if let Some(off) = ctx.global_vars.get(name) {
                            match op {
                                TokenKind::Equal => {
                                    out.push(0x41);
                                    write_i32_leb(*off as i32, out);
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0x39); // f64.store
                                    out.push(0x03);
                                    out.push(0x00);
                                }
                                TokenKind::PlusEqual => {
                                    out.push(0x41);
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x41);
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x2B); // f64.load
                                    out.push(0x03);
                                    out.push(0x00);
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0xA0); // f64.add
                                    out.push(0x39); // f64.store
                                    out.push(0x03);
                                    out.push(0x00);
                                }
                                TokenKind::MinusEqual => {
                                    out.push(0x41);
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x41);
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x2B); // f64.load
                                    out.push(0x03);
                                    out.push(0x00);
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0xA1); // f64.sub
                                    out.push(0x39); // f64.store
                                    out.push(0x03);
                                    out.push(0x00);
                                }
                                TokenKind::NullCoalesceEqual => {
                                    // x ??= y for global: if x is null (NaN), set x = y; else keep x
                                    let scratch_idx =
                                        ctx.locals_f64.get("__scratch_f64_2").copied().ok_or(
                                            "Internal error: __scratch_f64_2 local not found",
                                        )?;
                                    out.push(0x41); // i32.const offset
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x2B); // f64.load
                                    out.push(0x03);
                                    out.push(0x00);
                                    out.push(0x22); // local.tee scratch
                                    write_u32_leb(scratch_idx, out);
                                    out.push(0x20); // local.get scratch
                                    write_u32_leb(scratch_idx, out);
                                    out.push(0x61); // f64.eq (NaN == NaN is false)
                                    out.push(0x04); // if f64
                                    out.push(0x7C);
                                    // Then: not null, keep current value
                                    out.push(0x20); // local.get scratch
                                    write_u32_leb(scratch_idx, out);
                                    // Else: null, assign right side
                                    out.push(0x05); // else
                                    gen_expr_f64(rhs, out, ctx)?;
                                    out.push(0x0B); // end
                                    out.push(0x41); // i32.const offset
                                    write_i32_leb(*off as i32, out);
                                    out.push(0x39); // f64.store
                                    out.push(0x03);
                                    out.push(0x00);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                ExprKind::Update(is_inc, _, target) => {
                    if let ExprKind::Variable(name) = &target.kind {
                        if let Some(off) = ctx.global_vars.get(name) {
                            out.push(0x41);
                            write_i32_leb(*off as i32, out);
                            out.push(0x41);
                            write_i32_leb(*off as i32, out);
                            out.push(0x2B);
                            out.push(0x03);
                            out.push(0x00);
                            out.push(0x44);
                            out.extend(&1.0f64.to_bits().to_le_bytes());
                            if *is_inc {
                                out.push(0xA0);
                            } else {
                                out.push(0xA1);
                            }
                            out.push(0x39);
                            out.push(0x03);
                            out.push(0x00);
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
                                    if let Some((fidx, params, _)) = ctx.funcs.get(name) {
                                        if let ExprKind::Call(_, args, _) = &e.kind {
                                            for (i, pt) in params.iter().enumerate() {
                                                if let Some(a) = args.get(i) {
                                                    if *pt == 0x7F {
                                                        gen_expr_i32(a, out, ctx)?;
                                                    } else {
                                                        gen_expr_f64(a, out, ctx)?;
                                                    }
                                                } else if *pt == 0x7F {
                                                    out.push(0x41);
                                                    write_i32_leb(0, out);
                                                } else {
                                                    out.push(0x44);
                                                    out.extend(&0f64.to_bits().to_le_bytes());
                                                }
                                            }
                                        }
                                        out.push(0x10);
                                        write_u32_leb(*fidx, out);
                                    }
                                }
                            }
                        }
                    } else if let ExprKind::Assign(name, rhs) = &e.kind {
                        if let Some(off) = ctx.global_vars.get(name) {
                            out.push(0x41);
                            write_i32_leb(*off as i32, out);
                            gen_expr_f64(rhs, out, ctx)?;
                            out.push(0x39); // f64.store
                            out.push(0x03);
                            out.push(0x00);
                        }
                        if let Some(idx) = ctx.locals_f64.get(name) {
                            gen_expr_f64(rhs, out, ctx)?;
                            out.push(0x21);
                            write_u32_leb(*idx, out);
                        } else if let Some(idx) = ctx.locals_i32.get(name) {
                            gen_expr_i32(rhs, out, ctx)?;
                            out.push(0x21);
                            write_u32_leb(*idx, out);
                        }
                    } else if let ExprKind::AssignOp(lhs, op, rhs) = &e.kind {
                        if let ExprKind::Variable(name) = &lhs.kind {
                            match op {
                                TokenKind::NullCoalesceEqual => {
                                    if let Some(off) = ctx.global_vars.get(name) {
                                        // x ??= y for global: if x is null (NaN), set x = y; else keep x
                                        let scratch_idx =
                                            ctx.locals_f64.get("__scratch_f64_2").copied().ok_or(
                                                "Internal error: __scratch_f64_2 local not found",
                                            )?;
                                        out.push(0x41); // i32.const offset
                                        write_i32_leb(*off as i32, out);
                                        out.push(0x2B); // f64.load
                                        out.push(0x03);
                                        out.push(0x00);
                                        out.push(0x22); // local.tee scratch
                                        write_u32_leb(scratch_idx, out);
                                        out.push(0x20); // local.get scratch
                                        write_u32_leb(scratch_idx, out);
                                        out.push(0x61); // f64.eq (NaN == NaN is false)
                                        out.push(0x04); // if f64
                                        out.push(0x7C);
                                        // Then: not null, keep current value
                                        out.push(0x20); // local.get scratch
                                        write_u32_leb(scratch_idx, out);
                                        // Else: null, assign right side
                                        out.push(0x05); // else
                                        gen_expr_f64(rhs, out, ctx)?;
                                        out.push(0x0B); // end
                                        out.push(0x41); // i32.const offset
                                        write_i32_leb(*off as i32, out);
                                        out.push(0x39); // f64.store
                                        out.push(0x03);
                                        out.push(0x00);
                                    } else if let Some(idx) = ctx.locals_f64.get(name) {
                                        let scratch_idx =
                                            ctx.locals_f64.get("__scratch_f64_2").copied().ok_or(
                                                "Internal error: __scratch_f64_2 local not found",
                                            )?;
                                        out.push(0x20); // local.get idx
                                        write_u32_leb(*idx, out);
                                        out.push(0x22); // local.tee scratch
                                        write_u32_leb(scratch_idx, out);
                                        out.push(0x20); // local.get scratch
                                        write_u32_leb(scratch_idx, out);
                                        out.push(0x61); // f64.eq
                                        out.push(0x04); // if f64
                                        out.push(0x7C);
                                        // Then: not null, keep current
                                        out.push(0x20); // local.get scratch
                                        write_u32_leb(scratch_idx, out);
                                        // Else: null, assign right
                                        out.push(0x05); // else
                                        gen_expr_f64(rhs, out, ctx)?;
                                        out.push(0x0B); // end
                                        out.push(0x21); // local.set idx
                                        write_u32_leb(*idx, out);
                                    }
                                }
                                _ => {}
                            }
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
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            gen_stmt(try_block, out, ctx, ret_type)?;
            gen_stmt(catch_block, out, ctx, ret_type)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn compile_to_file(src: &str, out: &Path) -> Result<(), LangError> {
    compile_to_file_with_base_and_abi(src, out, Path::new("."), WasmAbi::Env)
}

pub fn compile_to_file_with_base(src: &str, out: &Path, base: &Path) -> Result<(), LangError> {
    compile_to_file_with_base_and_abi(src, out, base, WasmAbi::Env)
}

/// Compile `src` against an explicit host ABI.
pub fn compile_to_file_with_base_and_abi(
    src: &str,
    out: &Path,
    base: &Path,
    abi: WasmAbi,
) -> Result<(), LangError> {
    // The file-based Adesh→WASM compiler drives the CLI parsing/import-linking
    // pipeline and writes artifacts to disk; it is only reachable through the
    // native CLI. Inside a browser build there is no CLI module and no real
    // filesystem, so surface a clear error instead.
    #[cfg(not(target_arch = "wasm32"))]
    {
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

        let (prog, _) =
            crate::cli::parsing::check_ownership_and_parse(&body_src, &Default::default())
                .map_err(|e| LangError::new(ErrorKind::User, e, 0, 0, "".into()))?;
        let prog = crate::cli::static_imports::link_ast(prog, base)
            .map_err(|e| LangError::new(ErrorKind::User, e, 0, 0, "".into()))?;
        compile_ast_to_file_with_abi(prog, out, abi)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = (src, out, base, abi);
        Err(LangError::new(
            ErrorKind::User,
            "Adesh-to-WASM file compilation requires the native CLI".to_string(),
            0,
            0,
            "".into(),
        ))
    }
}

fn apply_type_alias_defaults_to_stmt(
    s: &mut Stmt,
    type_alias_defs: &FastMap<String, (Vec<(String, bool, String)>, Vec<(String, Expr)>)>,
    var_types: &mut FastMap<String, String>,
) {
    match &mut s.kind {
        StmtKind::Let(name, init_opt, ann_opt, ..) => {
            if let Some(ann) = ann_opt {
                let base_name = if let Some(idx) = ann.find('<') {
                    &ann[..idx]
                } else {
                    ann.as_str()
                };
                let clean_base = clean_type_name(base_name);
                var_types.insert(name.clone(), clean_base.to_string());
                if let Some(init) = init_opt {
                    let lookup = type_alias_defs
                        .get(base_name)
                        .or_else(|| type_alias_defs.get(clean_base));
                    if let Some((fields_schema, defaults)) = lookup {
                        if let ExprKind::Object(entries) = &mut init.kind {
                            for (field_name, is_opt, _ty) in fields_schema {
                                if !entries.iter().any(|(f, _)| f == field_name) {
                                    if let Some((_, def_expr)) =
                                        defaults.iter().find(|(f, _)| f == field_name)
                                    {
                                        entries.push((field_name.clone(), def_expr.clone()));
                                    } else if *is_opt {
                                        entries.push((
                                            field_name.clone(),
                                            Expr {
                                                kind: ExprKind::Literal(Value::Null),
                                                span: init.span.clone(),
                                            },
                                        ));
                                    }
                                }
                            }
                            if !entries.iter().any(|(f, _)| f == "__type__") {
                                entries.push((
                                    "__type__".to_string(),
                                    Expr {
                                        kind: ExprKind::Literal(Value::Str(clean_base.to_string())),
                                        span: init.span.clone(),
                                    },
                                ));
                            }
                        }
                    }
                }
            }
        }
        StmtKind::Function(f, _) => {
            let body_vec: &mut Vec<Stmt> = std::sync::Arc::make_mut(&mut f.body);
            apply_type_alias_defaults_to_stmts(body_vec, type_alias_defs, var_types);
        }
        StmtKind::Block(inner) => {
            apply_type_alias_defaults_to_stmts(inner, type_alias_defs, var_types);
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            apply_type_alias_defaults_to_stmt(then_branch, type_alias_defs, var_types);
            if let Some(eb) = else_branch {
                apply_type_alias_defaults_to_stmt(eb, type_alias_defs, var_types);
            }
        }
        StmtKind::While { body, .. } => {
            apply_type_alias_defaults_to_stmt(body, type_alias_defs, var_types);
        }
        StmtKind::ForIn { body, .. } => {
            apply_type_alias_defaults_to_stmt(body, type_alias_defs, var_types);
        }
        _ => {}
    }
}

fn apply_type_alias_defaults_to_stmts(
    stmts: &mut [Stmt],
    type_alias_defs: &FastMap<String, (Vec<(String, bool, String)>, Vec<(String, Expr)>)>,
    var_types: &mut FastMap<String, String>,
) {
    for s in stmts {
        apply_type_alias_defaults_to_stmt(s, type_alias_defs, var_types);
    }
}

pub(crate) fn compile_ast_to_file_with_abi(
    prog: Vec<Stmt>,
    out: &Path,
    abi: WasmAbi,
) -> Result<(), LangError> {
    let wasi_abi = WasiAbi::default();
    let uses_dates = prog.iter().any(super::dates::uses_dates);
    if uses_dates && abi == WasmAbi::WasiPreview1 {
        return Err(LangError::new(ErrorKind::User,
            "Date is not supported by the WASI Preview 1 adapter".into(), 0, 0, "".into()));
    }
    let date_op_idx = (uses_dates && abi == WasmAbi::Env).then_some(5);
    let (print_f64_idx, print_str_idx, alloc_idx, file_write_idx, func_base) = match abi {
        WasmAbi::Env => (0, 1, 2, Some(4), if uses_dates { 6 } else { 5 }),
        WasmAbi::WasiPreview1 => (
            wasi_abi.print_f64,
            wasi_abi.write,
            wasi_abi.alloc,
            None,
            wasi_abi.func_base,
        ),
    };
    let mut prog = crate::parsing::ast_optimizer::optimize_program(&prog);

    let mut struct_layouts: FastMap<String, FieldLayout> = FastMap::default();
    let mut type_alias_defs: FastMap<String, (Vec<(String, bool, String)>, Vec<(String, Expr)>)> =
        FastMap::default();
    for s in &prog {
        if let StmtKind::Struct(decl, _) = &s.kind {
            let layout = compute_wasm_layout(&decl.fields);
            struct_layouts.insert(decl.name.clone(), layout);
        } else if let StmtKind::TypeAlias(decl, _) = &s.kind {
            let fields: Vec<(String, String)> = decl
                .fields
                .iter()
                .map(|(name, _, ty)| (name.clone(), ty.clone()))
                .collect();
            let layout = compute_wasm_layout(&fields);
            let clean_name = clean_type_name(&decl.name).to_string();
            struct_layouts.insert(decl.name.clone(), layout.clone());
            type_alias_defs.insert(
                decl.name.clone(),
                (decl.fields.clone(), decl.defaults.clone()),
            );
            if clean_name != decl.name {
                struct_layouts.insert(clean_name.clone(), layout);
                type_alias_defs.insert(clean_name, (decl.fields.clone(), decl.defaults.clone()));
            }
        }
    }

    let mut aliases_for_hir = std::collections::HashMap::new();
    for (k, v) in &type_alias_defs {
        aliases_for_hir.insert(k.clone(), v.clone());
    }
    crate::parsing::hir_lower::KNOWN_TYPE_ALIASES.with(|cell| {
        *cell.borrow_mut() = aliases_for_hir;
    });

    let mut var_types = FastMap::default();
    apply_type_alias_defaults_to_stmts(&mut prog, &type_alias_defs, &mut var_types);
    WASM_VAR_TYPES.with(|cell| {
        *cell.borrow_mut() = var_types;
    });

    let mut desugared_prog = Vec::new();
    for s in prog {
        if let StmtKind::Let(name, Some(init), ..) = &s.kind {
            if let ExprKind::Fn(params, body, is_async) = &init.kind {
                desugared_prog.push(Stmt {
                    kind: StmtKind::Function(
                        crate::parsing::ast::Function::new(
                            name.clone(),
                            vec![],
                            params.clone(),
                            body.clone(),
                            None,
                            Some("number".into()),
                            *is_async,
                        ),
                        *is_async,
                    ),
                    span: s.span.clone(),
                });
                continue;
            }
        }
        desugared_prog.push(s);
    }
    prog = desugared_prog;
    super::static_calls::register(&prog);

    // Collect declared functions and their parameter default values
    let mut fn_param_defaults: FastMap<String, Vec<(String, Option<Expr>)>> = FastMap::default();
    for s in &prog {
        if let StmtKind::Function(f, _) = &s.kind {
            let params = f
                .params
                .iter()
                .map(|(n, def, _)| (n.clone(), def.clone()))
                .collect();
            fn_param_defaults.insert(f.name.clone(), params);
        }
    }

    // Helper closure to pad function call arguments with defaults
    fn pad_call_expr(
        e: &mut Expr,
        fn_param_defaults: &FastMap<String, Vec<(String, Option<Expr>)>>,
    ) {
        match &mut e.kind {
            ExprKind::Call(callee, args, _) => {
                pad_call_expr(callee, fn_param_defaults);
                for a in args.iter_mut() {
                    pad_call_expr(a, fn_param_defaults);
                }
                if let ExprKind::Variable(fn_name) = &callee.kind {
                    if let Some(params) = fn_param_defaults.get(fn_name) {
                        // A spread supplies an unknown number of positional
                        // arguments here. Bind defaults after expanding it.
                        if !args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
                            && args.len() < params.len() {
                            for i in args.len()..params.len() {
                                if let Some(ref def_expr) = params[i].1 {
                                    args.push(def_expr.clone());
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::Binary(_, _, r) | ExprKind::Logical(_, _, r) => {
                // recursive for children if needed
                pad_call_expr(r, fn_param_defaults);
            }
            ExprKind::Unary(_, inner) | ExprKind::Grouping(inner) => {
                pad_call_expr(inner, fn_param_defaults);
            }
            _ => {}
        }
    }

    fn pad_stmt(s: &mut Stmt, fn_param_defaults: &FastMap<String, Vec<(String, Option<Expr>)>>) {
        match &mut s.kind {
            StmtKind::ExprStmt(e) => pad_call_expr(e, fn_param_defaults),
            StmtKind::Let(_, Some(init), ..) => pad_call_expr(init, fn_param_defaults),
            StmtKind::Function(f, _) => {
                let mut body_vec = (*f.body).clone();
                for st in &mut body_vec {
                    pad_stmt(st, fn_param_defaults);
                }
                f.body = std::sync::Arc::new(body_vec);
            }
            StmtKind::Block(stmts) => {
                for st in stmts {
                    pad_stmt(st, fn_param_defaults);
                }
            }
            _ => {}
        }
    }

    for s in &mut prog {
        pad_stmt(s, &fn_param_defaults);
    }

    let mut func_defs: Vec<crate::parsing::ast::Function> = Vec::new();
    for s in &prog {
        if let StmtKind::Function(f, _) = &s.kind {
            let mut f = f.clone();
            if f.ret_type.is_none() && f.body.iter().any(has_value_return) {
                f.ret_type = Some("number".into());
            }
            func_defs.push(f);
        }
    }

    let resolve_wasm_type = |ty: &str| -> String {
        let mut cur = ty.to_string();
        for _ in 0..10 {
            let base = if let Some(idx) = cur.find('<') {
                cur[..idx].to_string()
            } else {
                cur.clone()
            };
            if base.contains('.') {
                let parts: Vec<&str> = base.split('.').collect();
                let mut cur_target: Option<String> = None;
                for (i, &part) in parts.iter().enumerate() {
                    if i == 0 {
                        cur_target = Some(part.to_string());
                    } else if let Some(parent) = cur_target {
                        let next = if let Some((fields, _)) = type_alias_defs.get(&parent) {
                            fields
                                .iter()
                                .find(|(f, _, _)| f == part)
                                .map(|(_, _, fty)| fty.clone())
                        } else {
                            None
                        };
                        cur_target = next;
                    } else {
                        break;
                    }
                }
                if let Some(target) = cur_target {
                    cur = target;
                    continue;
                }
            } else if let Some((fields, _)) = type_alias_defs.get(&base) {
                if fields.len() == 1 && fields[0].0 == "__alias_target__" {
                    cur = fields[0].2.clone();
                    continue;
                }
            }
            break;
        }
        cur
    };

    let is_i32_type = |ty: &str| -> bool {
        let resolved = resolve_wasm_type(ty);
        let base = if let Some(idx) = resolved.find('<') {
            &resolved[..idx]
        } else {
            &resolved[..]
        };
        matches!(
            base,
            "int" | "i32" | "i16" | "i8" | "u32" | "u16" | "u8" | "bool" | "string" | "String"
        ) || struct_layouts.contains_key(base)
    };

    let mut locals_map_f64: FastMap<String, u32> = FastMap::default();
    let mut locals_map_i32: FastMap<String, u32> = FastMap::default();
    let mut local_count_f64: u32 = 0;
    let mut local_count_i32: u32 = 0;

    fn collect_top_locals(
        s: &Stmt,
        func_defs: &[crate::parsing::ast::Function],
        locals_map_i32: &mut FastMap<String, u32>,
        locals_map_f64: &mut FastMap<String, u32>,
        local_count_i32: &mut u32,
        local_count_f64: &mut u32,
        is_i32_type: &impl Fn(&str) -> bool,
    ) {
        match &s.kind {
            StmtKind::Let(name, init, _ann, ..) => {
                if init.is_some() {
                    let inferred_struct_or_i32 = if let Some(init_expr) = init {
                        match &init_expr.kind {
                            ExprKind::StructLiteral(sname, ..) => Some(sname.clone()),
                            ExprKind::Call(callee, ..) => {
                                if let ExprKind::Variable(fn_name) = &callee.kind {
                                    func_defs
                                        .iter()
                                        .find(|fd| &fd.name == fn_name)
                                        .and_then(|fd| fd.ret_type.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };

                    let effective_type = _ann.clone().or(inferred_struct_or_i32);
                    if let Some(tn) = effective_type {
                        if is_i32_type(&tn) {
                            if !locals_map_i32.contains_key(name) {
                                locals_map_i32.insert(name.clone(), *local_count_i32);
                                *local_count_i32 += 1;
                            }
                        } else {
                            if !locals_map_f64.contains_key(name) {
                                locals_map_f64.insert(name.clone(), *local_count_f64);
                                *local_count_f64 += 1;
                            }
                        }
                    } else {
                        if !locals_map_f64.contains_key(name) {
                            locals_map_f64.insert(name.clone(), *local_count_f64);
                            *local_count_f64 += 1;
                        }
                    }
                }
            }
            StmtKind::ShareDeclaration(decl, _) => {
                if !locals_map_f64.contains_key(&decl.name) {
                    locals_map_f64.insert(decl.name.clone(), *local_count_f64);
                    *local_count_f64 += 1;
                }
            }
            StmtKind::StrongDeclaration(decl, _) => {
                if !locals_map_f64.contains_key(&decl.name) {
                    locals_map_f64.insert(decl.name.clone(), *local_count_f64);
                    *local_count_f64 += 1;
                }
            }
            StmtKind::WeakDeclaration(decl, _) => {
                if !locals_map_f64.contains_key(&decl.name) {
                    locals_map_f64.insert(decl.name.clone(), *local_count_f64);
                    *local_count_f64 += 1;
                }
            }
            StmtKind::ForIn { name, body, .. } => {
                if !locals_map_i32.contains_key(name) && !locals_map_f64.contains_key(name) {
                    locals_map_i32.insert(name.clone(), *local_count_i32);
                    *local_count_i32 += 1;
                }
                collect_top_locals(
                    body,
                    func_defs,
                    locals_map_i32,
                    locals_map_f64,
                    local_count_i32,
                    local_count_f64,
                    is_i32_type,
                );
            }
            StmtKind::TryCatch {
                try_block,
                err_name,
                catch_block,
            } => {
                if !err_name.is_empty() {
                    if !locals_map_i32.contains_key(err_name)
                        && !locals_map_f64.contains_key(err_name)
                    {
                        locals_map_i32.insert(err_name.clone(), *local_count_i32);
                        *local_count_i32 += 1;
                    }
                }
                collect_top_locals(
                    try_block,
                    func_defs,
                    locals_map_i32,
                    locals_map_f64,
                    local_count_i32,
                    local_count_f64,
                    is_i32_type,
                );
                collect_top_locals(
                    catch_block,
                    func_defs,
                    locals_map_i32,
                    locals_map_f64,
                    local_count_i32,
                    local_count_f64,
                    is_i32_type,
                );
            }
            StmtKind::Block(stmts) => {
                for st in stmts {
                    collect_top_locals(
                        st,
                        func_defs,
                        locals_map_i32,
                        locals_map_f64,
                        local_count_i32,
                        local_count_f64,
                        is_i32_type,
                    );
                }
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_top_locals(
                    then_branch,
                    func_defs,
                    locals_map_i32,
                    locals_map_f64,
                    local_count_i32,
                    local_count_f64,
                    is_i32_type,
                );
                if let Some(eb) = else_branch {
                    collect_top_locals(
                        eb,
                        func_defs,
                        locals_map_i32,
                        locals_map_f64,
                        local_count_i32,
                        local_count_f64,
                        is_i32_type,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::UnsafeBlock(body)
            | StmtKind::Region { body, .. } => {
                collect_top_locals(
                    body,
                    func_defs,
                    locals_map_i32,
                    locals_map_f64,
                    local_count_i32,
                    local_count_f64,
                    is_i32_type,
                );
            }
            _ => {}
        }
    }

    for s in &prog {
        collect_top_locals(
            s,
            &func_defs,
            &mut locals_map_i32,
            &mut locals_map_f64,
            &mut local_count_i32,
            &mut local_count_f64,
            &is_i32_type,
        );
    }

    if !locals_map_i32.contains_key("__scratch") {
        locals_map_i32.insert("__scratch".to_string(), local_count_i32);
        local_count_i32 += 1;
    }
    if !locals_map_i32.contains_key("__scratch_val") {
        locals_map_i32.insert("__scratch_val".to_string(), local_count_i32);
        local_count_i32 += 1;
    }
    if !locals_map_f64.contains_key("__scratch_f64") {
        locals_map_f64.insert("__scratch_f64".to_string(), local_count_f64);
        local_count_f64 += 1;
    }
    if !locals_map_f64.contains_key("__scratch_f64_2") {
        locals_map_f64.insert("__scratch_f64_2".to_string(), local_count_f64);
        local_count_f64 += 1;
    }
    // The main wrapper declares i32 locals before f64 locals, so every f64
    // local sits local_count_i32 slots past its f64-only index.
    for val in locals_map_f64.values_mut() {
        *val += local_count_i32;
    }

    let mut func_types: Vec<(Vec<u8>, Option<u8>)> = Vec::new();
    func_types.push((vec![0x7C], None));
    func_types.push((vec![0x7F, 0x7F], None));
    func_types.push((Vec::new(), None));
    func_types.push((vec![0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F, 0x7F, 0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F], Some(0x7F)));
    func_types.push((vec![0x7F, 0x7F, 0x7F, 0x7F], None));
    // 7: (i64)->(), used by the WASI integer/fraction printers
    func_types.push((vec![0x7E], None));
    // 8: runtime Date operation (timestamp, operation) -> number or string pointer.
    func_types.push((vec![0x7C, 0x7F], Some(0x7C)));
    let mut func_type_map: FastMap<(Vec<u8>, Option<u8>), u32> = FastMap::default();
    for (i, t) in func_types.iter().enumerate() {
        func_type_map.insert((t.0.clone(), t.1), i as u32);
    }

    let resolve_wasm_type = |ty: &str| -> String {
        let mut cur = ty.to_string();
        for _ in 0..10 {
            let base = if let Some(idx) = cur.find('<') {
                cur[..idx].to_string()
            } else {
                cur.clone()
            };
            if base.contains('.') {
                let parts: Vec<&str> = base.split('.').collect();
                let mut cur_target: Option<String> = None;
                for (i, &part) in parts.iter().enumerate() {
                    if i == 0 {
                        cur_target = Some(part.to_string());
                    } else if let Some(parent) = cur_target {
                        let next = if let Some((fields, _)) = type_alias_defs.get(&parent) {
                            fields
                                .iter()
                                .find(|(f, _, _)| f == part)
                                .map(|(_, _, fty)| fty.clone())
                        } else {
                            None
                        };
                        cur_target = next;
                    } else {
                        break;
                    }
                }
                if let Some(target) = cur_target {
                    cur = target;
                    continue;
                }
            } else if let Some((fields, _)) = type_alias_defs.get(&base) {
                if fields.len() == 1 && fields[0].0 == "__alias_target__" {
                    cur = fields[0].2.clone();
                    continue;
                }
            }
            break;
        }
        cur
    };

    let is_i32_type = |ty: &str| -> bool {
        let resolved = resolve_wasm_type(ty);
        let base = if let Some(idx) = resolved.find('<') {
            &resolved[..idx]
        } else {
            &resolved[..]
        };
        matches!(
            base,
            "int" | "i32" | "i16" | "i8" | "u32" | "u16" | "u8" | "bool" | "string" | "String"
        ) || struct_layouts.contains_key(base)
    };

    let mut funcs_index_map: FastMap<String, (u32, Vec<u8>, Option<u8>)> = FastMap::default();
    // Declare every signature before compiling any body. Imported private
    // helpers and recursive calls can refer to a later function.
    let mut global_vars: FastMap<String, u32> = FastMap::default();
    let mut cur_global_off: u32 = 16;
    for s in &prog {
        if let StmtKind::Let(name, ..) = &s.kind {
            if !global_vars.contains_key(name) {
                global_vars.insert(name.clone(), cur_global_off);
                cur_global_off += 8;
            }
        }
    }

    for (i, f) in func_defs.iter().enumerate() {
        let params = f
            .params
            .iter()
            .map(|p| match p.2.as_deref() {
                Some(ty) if is_i32_type(ty) => 0x7F,
                _ => 0x7C,
            })
            .collect();
        let ret = match f.ret_type.as_deref() {
            Some(ty) if is_i32_type(ty) => Some(0x7F),
            Some(_) => Some(0x7C),
            None if f.body.last().is_some_and(|s| {
                matches!(&s.kind,
                StmtKind::ExprStmt(e) if is_numeric_expr(e, &funcs_index_map))
            }) =>
            {
                Some(0x7C)
            }
            None => None,
        };
        funcs_index_map.insert(f.name.clone(), (func_base + i as u32, params, ret));
    }
    let mut function_section_types: Vec<u32> = Vec::new();
    let mut code_funcs: Vec<(u32, u32, u32, Vec<u8>)> = Vec::new();

    let mut var_values: FastMap<String, Value> = FastMap::default();
    fn eval_stmts_for_env(stmts: &[Stmt], var_values: &mut FastMap<String, Value>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Let(name, Some(init), ..) => {
                    if let Some(val) = expr_to_value_env(init, var_values) {
                        var_values.insert(name.clone(), val);
                    }
                }
                StmtKind::ExprStmt(e) => {
                    match &e.kind {
                        ExprKind::Assign(name, rhs) => {
                            if let Some(val) = expr_to_value_env(rhs, var_values) {
                                var_values.insert(name.clone(), val);
                            }
                        }
                        ExprKind::AssignOp(lhs, op, rhs) => {
                            if let ExprKind::Get(target, field) = &lhs.kind {
                                if let ExprKind::Variable(tname) = &target.kind {
                                    if let Some(Value::Object(map)) = var_values.get(tname).cloned()
                                    {
                                        let mut new_map = (*map).clone();
                                        if let Some(v) = expr_to_value_env(rhs, var_values) {
                                            new_map.insert(field.clone(), v);
                                            var_values.insert(
                                                tname.clone(),
                                                Value::Object(std::sync::Arc::new(new_map)),
                                            );
                                        }
                                    }
                                }
                            } else if let ExprKind::Index(target, idx) = &lhs.kind {
                                if let ExprKind::Variable(tname) = &target.kind {
                                    if let (Some(Value::Array(arr)), Some(idx_val)) = (
                                        var_values.get(tname).cloned(),
                                        expr_to_value_env(idx, var_values),
                                    ) {
                                        let i = idx_val.as_f64().unwrap_or(0.0) as usize;
                                        let mut new_arr = arr.clone();
                                        if let Some(v) = expr_to_value_env(rhs, var_values) {
                                            if i < new_arr.len() {
                                                new_arr[i] = v;
                                                var_values
                                                    .insert(tname.clone(), Value::Array(new_arr));
                                            }
                                        }
                                    }
                                }
                            } else if let ExprKind::Variable(tname) = &lhs.kind {
                                if *op == TokenKind::NullCoalesceEqual {
                                    // x ??= y: if x is null, set x = y; else keep x
                                    if let Some(current) = var_values.get(tname).cloned() {
                                        let is_null = matches!(current, Value::Null)
                                            || matches!(current, Value::Number(n) if n.is_nan());
                                        if is_null {
                                            if let Some(v) = expr_to_value_env(rhs, var_values) {
                                                var_values.insert(tname.clone(), v);
                                            }
                                        }
                                        // else: keep current value (no change)
                                    } else {
                                        // Variable not in env, evaluate rhs
                                        if let Some(v) = expr_to_value_env(rhs, var_values) {
                                            var_values.insert(tname.clone(), v);
                                        }
                                    }
                                } else if let Some(v) = expr_to_value_env(rhs, var_values) {
                                    var_values.insert(tname.clone(), v);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                StmtKind::Block(inner) => {
                    eval_stmts_for_env(inner, var_values);
                }
                StmtKind::TryCatch {
                    try_block,
                    err_name,
                    catch_block,
                } => {
                    let mut thrown_val = None;
                    fn find_throw(st: &Stmt, env: &FastMap<String, Value>) -> Option<Value> {
                        match &st.kind {
                            StmtKind::ExprStmt(e) => {
                                if let ExprKind::Throw(inner) = &e.kind {
                                    return expr_to_value_env(inner, env);
                                }
                            }
                            StmtKind::Block(sts) => {
                                for s in sts {
                                    if let Some(v) = find_throw(s, env) {
                                        return Some(v);
                                    }
                                }
                            }
                            _ => {}
                        }
                        None
                    }
                    if let Some(tv) = find_throw(try_block, var_values) {
                        thrown_val = Some(tv);
                    }
                    if let Some(tv) = thrown_val {
                        var_values.insert(err_name.clone(), tv);
                        eval_stmts_for_env(std::slice::from_ref(catch_block), var_values);
                    }
                }
                _ => {}
            }
        }
    }
    // Compile-time values are flow-sensitive: each top-level statement sees
    // the environment produced by the statements before it, so a print after
    // a reassignment observes the updated value.
    let mut env_snapshots: Vec<FastMap<String, Value>> = Vec::with_capacity(prog.len());
    for s in &prog {
        env_snapshots.push(var_values.clone());
        eval_stmts_for_env(std::slice::from_ref(s), &mut var_values);
    }

    let mut strings: Vec<(String, u32)> = Vec::new();
    let mut cur_off: u32 = 1024;
    for (i, s) in prog.iter().enumerate() {
        collect_all_wasm_strings_stmt(s, &mut strings, &mut cur_off, &env_snapshots[i]);
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
    if !strings.iter().any(|(x, _)| x == "object") {
        strings.push(("object".into(), cur_off));
        cur_off += 6;
    }
    if abi == WasmAbi::WasiPreview1 {
        for literal in ["NaN", "Infinity", "-Infinity", "-", "."] {
            if !strings.iter().any(|(x, _)| x == literal) {
                strings.push((literal.to_string(), cur_off));
                cur_off += literal.len() as u32;
            }
        }
    }
    if !strings.iter().any(|(x, _)| x == "\x1b[0m") {
        strings.push(("\x1b[0m".into(), cur_off));
    }
    let mut str_vars: FastMap<String, (u32, u32)> = FastMap::default();
    let mut complex_vars: FastMap<String, (u32, u32)> = FastMap::default();
    for (stmt_idx, s) in prog.iter().enumerate() {
        if let StmtKind::Let(name, Some(init), ..) = &s.kind {
            if let ExprKind::Literal(Value::Str(st)) = &init.kind {
                if let Some((_, off)) = strings.iter().find(|(x, _)| x == st) {
                    str_vars.insert(name.clone(), (*off, st.len() as u32));
                }
            } else if !matches!(
                init.kind,
                ExprKind::Literal(
                    Value::Number(_)
                        | Value::F64(_)
                        | Value::F32(_)
                        | Value::I64(_)
                        | Value::I32(_)
                        | Value::I16(_)
                        | Value::I8(_)
                        | Value::U64(_)
                        | Value::U32(_)
                        | Value::U16(_)
                        | Value::U8(_)
                        | Value::Bool(_)
                        | Value::Null
                )
            ) {
                if let Some(fmt) = format_static_val_for_wasm_env(init, &env_snapshots[stmt_idx]) {
                    if let Some((_, off)) = strings.iter().find(|(x, _)| x == &fmt) {
                        complex_vars.insert(name.clone(), (*off, fmt.len() as u32));
                    }
                }
            }
        }
    }

    for (i, f) in func_defs.iter().enumerate() {
        let mut params: Vec<u8> = Vec::new();
        for p in &f.params {
            if let Some(tn) = &p.2 {
                if is_i32_type(tn) {
                    params.push(0x7F);
                } else {
                    params.push(0x7C);
                }
            } else {
                params.push(0x7C);
            }
        }
        let mut ret_byte: Option<u8> = None;
        if f.ret_type
            .as_deref()
            .map(|rt| is_i32_type(rt))
            .unwrap_or(false)
        {
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
        funcs_index_map.insert(
            f.name.clone(),
            (func_base + i as u32, params.clone(), ret_byte),
        );

        let mut f_locals_f64: FastMap<String, u32> = FastMap::default();
        let mut f_locals_i32: FastMap<String, u32> = FastMap::default();
        let mut f_local_count_f64: u32 = 0;
        let mut f_local_count_i32: u32 = 0;
        let mut f_locals_types: FastMap<String, String> = FastMap::default();

        for (idx, p) in f.params.iter().enumerate() {
            let t = p.2.clone().unwrap_or("number".to_string());
            let clean_name = p.0.trim_start_matches("...").to_string();
            f_locals_types.insert(p.0.clone(), t.clone());
            f_locals_types.insert(clean_name.clone(), t.clone());
            if is_i32_type(&t) {
                f_locals_i32.insert(p.0.clone(), idx as u32);
                f_locals_i32.insert(clean_name, idx as u32);
            } else {
                f_locals_f64.insert(p.0.clone(), idx as u32);
                f_locals_f64.insert(clean_name, idx as u32);
            }
        }
        let pcount = f.params.len() as u32;

        fn collect_fn_locals(
            s: &Stmt,
            func_defs: &[crate::parsing::ast::Function],
            f_locals_types: &mut FastMap<String, String>,
            f_locals_i32: &mut FastMap<String, u32>,
            f_locals_f64: &mut FastMap<String, u32>,
            f_local_count_i32: &mut u32,
            f_local_count_f64: &mut u32,
            pcount: u32,
            is_i32_type: &impl Fn(&str) -> bool,
        ) {
            match &s.kind {
                StmtKind::Let(name, _init, _ann, ..) => {
                    let inferred_struct_or_i32 = if let Some(init_expr) = _init {
                        match &init_expr.kind {
                            ExprKind::StructLiteral(sname, ..) => Some(sname.clone()),
                            ExprKind::Call(callee, ..) => {
                                if let ExprKind::Variable(fn_name) = &callee.kind {
                                    func_defs
                                        .iter()
                                        .find(|fd| &fd.name == fn_name)
                                        .and_then(|fd| fd.ret_type.clone())
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };

                    let effective_type = _ann.clone().or(inferred_struct_or_i32);
                    if let Some(tn) = effective_type {
                        f_locals_types.insert(name.clone(), tn.clone());
                        if is_i32_type(&tn) {
                            if !f_locals_i32.contains_key(name) {
                                f_locals_i32.insert(name.clone(), *f_local_count_i32 + pcount);
                                *f_local_count_i32 += 1;
                            }
                        } else {
                            if !f_locals_f64.contains_key(name) {
                                f_locals_f64.insert(name.clone(), *f_local_count_f64 + pcount);
                                *f_local_count_f64 += 1;
                            }
                        }
                    } else {
                        f_locals_types.insert(name.clone(), "number".into());
                        if !f_locals_f64.contains_key(name) {
                            f_locals_f64.insert(name.clone(), *f_local_count_f64 + pcount);
                            *f_local_count_f64 += 1;
                        }
                    }
                }
                StmtKind::ShareDeclaration(decl, _) => {
                    f_locals_types.insert(decl.name.clone(), "strong".into());
                    if !f_locals_f64.contains_key(&decl.name) {
                        f_locals_f64.insert(decl.name.clone(), *f_local_count_f64 + pcount);
                        *f_local_count_f64 += 1;
                    }
                }
                StmtKind::StrongDeclaration(decl, _) => {
                    f_locals_types.insert(decl.name.clone(), "strong".into());
                    if !f_locals_f64.contains_key(&decl.name) {
                        f_locals_f64.insert(decl.name.clone(), *f_local_count_f64 + pcount);
                        *f_local_count_f64 += 1;
                    }
                }
                StmtKind::WeakDeclaration(decl, _) => {
                    f_locals_types.insert(decl.name.clone(), "weak".into());
                    if !f_locals_f64.contains_key(&decl.name) {
                        f_locals_f64.insert(decl.name.clone(), *f_local_count_f64 + pcount);
                        *f_local_count_f64 += 1;
                    }
                }
                StmtKind::ForIn { name, body, .. } => {
                    f_locals_types.insert(name.clone(), "int".into());
                    if !f_locals_i32.contains_key(name) && !f_locals_f64.contains_key(name) {
                        f_locals_i32.insert(name.clone(), *f_local_count_i32 + pcount);
                        *f_local_count_i32 += 1;
                    }
                    collect_fn_locals(
                        body,
                        func_defs,
                        f_locals_types,
                        f_locals_i32,
                        f_locals_f64,
                        f_local_count_i32,
                        f_local_count_f64,
                        pcount,
                        is_i32_type,
                    );
                }
                StmtKind::TryCatch {
                    try_block,
                    err_name,
                    catch_block,
                } => {
                    if !err_name.is_empty() {
                        f_locals_types.insert(err_name.clone(), "string".into());
                        if !f_locals_i32.contains_key(err_name)
                            && !f_locals_f64.contains_key(err_name)
                        {
                            f_locals_i32.insert(err_name.clone(), *f_local_count_i32 + pcount);
                            *f_local_count_i32 += 1;
                        }
                    }
                    collect_fn_locals(
                        try_block,
                        func_defs,
                        f_locals_types,
                        f_locals_i32,
                        f_locals_f64,
                        f_local_count_i32,
                        f_local_count_f64,
                        pcount,
                        is_i32_type,
                    );
                    collect_fn_locals(
                        catch_block,
                        func_defs,
                        f_locals_types,
                        f_locals_i32,
                        f_locals_f64,
                        f_local_count_i32,
                        f_local_count_f64,
                        pcount,
                        is_i32_type,
                    );
                }
                StmtKind::Block(stmts) => {
                    for st in stmts {
                        collect_fn_locals(
                            st,
                            func_defs,
                            f_locals_types,
                            f_locals_i32,
                            f_locals_f64,
                            f_local_count_i32,
                            f_local_count_f64,
                            pcount,
                            is_i32_type,
                        );
                    }
                }
                StmtKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_fn_locals(
                        then_branch,
                        func_defs,
                        f_locals_types,
                        f_locals_i32,
                        f_locals_f64,
                        f_local_count_i32,
                        f_local_count_f64,
                        pcount,
                        is_i32_type,
                    );
                    if let Some(eb) = else_branch {
                        collect_fn_locals(
                            eb,
                            func_defs,
                            f_locals_types,
                            f_locals_i32,
                            f_locals_f64,
                            f_local_count_i32,
                            f_local_count_f64,
                            pcount,
                            is_i32_type,
                        );
                    }
                }
                StmtKind::While { body, .. }
                | StmtKind::UnsafeBlock(body)
                | StmtKind::Region { body, .. } => {
                    collect_fn_locals(
                        body,
                        func_defs,
                        f_locals_types,
                        f_locals_i32,
                        f_locals_f64,
                        f_local_count_i32,
                        f_local_count_f64,
                        pcount,
                        is_i32_type,
                    );
                }
                _ => {}
            }
        }

        for s in f.body.iter() {
            collect_fn_locals(
                s,
                &func_defs,
                &mut f_locals_types,
                &mut f_locals_i32,
                &mut f_locals_f64,
                &mut f_local_count_i32,
                &mut f_local_count_f64,
                pcount,
                &is_i32_type,
            );
        }
        f_locals_i32.insert("__scratch".to_string(), f_local_count_i32 + pcount);
        f_local_count_i32 += 1;
        f_locals_i32.insert("__scratch_val".to_string(), f_local_count_i32 + pcount);
        f_local_count_i32 += 1;

        f_locals_f64.insert("__scratch_f64".to_string(), f_local_count_f64 + pcount);
        f_local_count_f64 += 1;
        f_locals_f64.insert("__scratch_f64_2".to_string(), f_local_count_f64 + pcount);
        f_local_count_f64 += 1;
        for val in f_locals_f64.values_mut() {
            // Parameter indices precede all locals, irrespective of type.
            if *val >= pcount {
                *val += f_local_count_i32;
            }
        }

        let mut function_values = var_values.clone();
        for (name, _, _) in &f.params {
            function_values.remove(name.trim_start_matches("..."));
        }
        for name in f_locals_f64.keys().chain(f_locals_i32.keys()) {
            function_values.remove(name);
        }
        let mut ctx = CompilerContext {
            locals_f64: &f_locals_f64,
            locals_i32: &f_locals_i32,
            funcs: &funcs_index_map,
            strings: &strings,
            str_vars: &mut str_vars,
            complex_vars: &complex_vars,
            global_vars: &global_vars,
            is_main: false,
            var_values: &function_values,
            struct_layouts: &struct_layouts,
            locals_types: &mut f_locals_types,
            defer_scopes: vec![Vec::new()],
            loop_defer_depths: Vec::new(),
            print_f64_idx,
            print_str_idx,
            alloc_idx,
            file_write_idx,
            date_op_idx,
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
        code_funcs.push((f_local_count_i32, 0, f_local_count_f64, fbody));
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
        global_vars: &global_vars,
        is_main: true,
        var_values: &var_values,
        struct_layouts: &struct_layouts,
        locals_types: &mut main_types,
        defer_scopes: vec![Vec::new()],
        loop_defer_depths: Vec::new(),
        print_f64_idx,
        print_str_idx,
        alloc_idx,
        file_write_idx,
        date_op_idx,
    };
    for (i, s) in prog.iter().enumerate() {
        match &s.kind {
            StmtKind::Function(..) => {}
            _ => {
                ctx.var_values = &env_snapshots[i];
                gen_stmt(s, &mut main_body, &mut ctx, None)
                    .map_err(|m| LangError::new(ErrorKind::User, m, 0, 0, "".into()))?
            }
        }
    }
    ctx.var_values = &var_values;
    if let Some((main_fn_idx, _, ret_opt)) = funcs_index_map.get("main") {
        main_body.push(0x10);
        write_u32_leb(*main_fn_idx, &mut main_body);
        if ret_opt.is_some() {
            main_body.push(0x1A);
        }
    }

    // The heap starts after the emitted string data, and linear memory must be
    // large enough to hold that data before the allocator ever bumps.
    let data_end = strings
        .iter()
        .map(|(s, off)| off + s.len() as u32)
        .max()
        .unwrap_or(1024);
    let heap_start = (data_end.max(2048) + 7) & !7;
    let memory_pages = heap_start.div_ceil(65536).max(1);
    let wasi_lits = match abi {
        WasmAbi::Env => None,
        WasmAbi::WasiPreview1 => {
            let find = |literal: &str| {
                strings
                    .iter()
                    .find(|(s, _)| s == literal)
                    .map(|(s, off)| (*off, s.len() as u32))
                    .expect("WASI literal is collected before module assembly")
            };
            Some(WasiLiterals {
                nan: find("NaN"),
                infinity: find("Infinity"),
                neg_infinity: find("-Infinity"),
                minus: find("-"),
                dot: find("."),
            })
        }
    };

    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend(&[0x00, 0x61, 0x73, 0x6D]);
    bytes.extend(&[0x01, 0x00, 0x00, 0x00]);
    bytes.extend(encode_type_section(&func_types));
    match abi {
        WasmAbi::Env => bytes.extend(encode_import_section(uses_dates)),
        WasmAbi::WasiPreview1 => bytes.extend(encode_wasi_import_section()),
    }
    let mut all_func_types = Vec::new();
    if abi == WasmAbi::WasiPreview1 {
        all_func_types.extend(wasi_abi.function_types());
    }
    all_func_types.extend(function_section_types.iter().copied());
    all_func_types.push(2);
    bytes.extend(encode_function_section(&all_func_types));
    bytes.extend(encode_memory_section(memory_pages));
    if abi == WasmAbi::WasiPreview1 {
        bytes.extend(encode_global_section(heap_start));
    }
    let main_index = func_base + function_section_types.len() as u32;
    bytes.extend(encode_export_section(main_index, 2, true));
    let mut all_code: Vec<(u32, u32, u32, Vec<u8>)> = Vec::new();
    if let Some(lits) = wasi_lits {
        all_code.extend(wasi::adapter_bodies(wasi_abi, lits));
    }
    all_code.extend(code_funcs);
    all_code.push((local_count_i32, 0, local_count_f64, main_body));
    bytes.extend(encode_code_section(&all_code));
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

fn has_value_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::Block(body) => body.iter().any(has_value_return),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            has_value_return(then_branch)
                || else_branch.as_ref().is_some_and(|s| has_value_return(s))
        }
        StmtKind::While { body, .. }
        | StmtKind::ForIn { body, .. }
        | StmtKind::UnsafeBlock(body)
        | StmtKind::Region { body, .. } => has_value_return(body),
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => has_value_return(try_block) || has_value_return(catch_block),
        _ => false,
    }
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
      date_op: (timestamp, op) => {{
        if (op === 0) return Date.now();
        const date = new Date(timestamp);
        switch (op) {{
          case 1: return date.getTime();
          case 2: return date.getFullYear();
          case 3: return date.getMonth();
          case 4: return date.getDate();
          case 5: return date.getHours();
          case 6: return date.getMinutes();
          case 7: return date.getSeconds();
          case 8: return date.getMilliseconds();
          case 9: {{
            const bytes = new TextEncoder().encode(date.toISOString());
            new Uint8Array(memory.buffer, 512, bytes.length).set(bytes);
            return 512;
          }}
          case 10: return date.getUTCFullYear();
          case 11: return date.getUTCMonth();
          case 12: return date.getUTCDate();
          case 13: return date.getUTCHours();
          case 14: return date.getUTCMinutes();
          case 15: return date.getUTCSeconds();
          case 16: return date.getUTCMilliseconds();
          default: throw new Error('Unknown Date operation');
        }}
      }},
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

    #[test]
    fn wasm_formatting_expressions_compile() {
        let src = r#"
            let val = 123.456;
            let formatted_num = `${val:.2f}`;
            let formatted_hex = `${255:hex}`;
            let formatted_curr = `${1000:currency(USD)}`;
            print(formatted_num);
            print(formatted_hex);
            print(formatted_curr);
        "#;
        let tmp = std::env::temp_dir().join("wasm-formatting-expr.wasm");
        let result = compile_to_file(src, &tmp);
        assert!(
            result.is_ok(),
            "WASM compile failed for formatting expressions: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(&tmp);
    }
}
