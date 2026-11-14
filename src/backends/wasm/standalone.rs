//! Checked, opt-in scalar HIR lowering. The historical emitter is unchanged.
#![forbid(unsafe_code)]

use crate::parsing::hir::{BinOp, HirExpr, HirLiteral, HirModule, HirStmt, HirType, UnaryOp};
use adesh_codegen::machine_ir::{
    ConditionCode as C, MachineInstruction as I, MachineOperand as O, MachineRegister as R,
    VirtualRegister,
};
use adesh_codegen::targets::wasm::standalone::{CallSite, Function, Limits, Module, ValueType};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scalar {
    Bool,
    I32,
    I64,
    F32,
    F64,
}

impl Scalar {
    fn wasm(self) -> ValueType {
        match self {
            Self::Bool | Self::I32 => ValueType::I32,
            Self::I64 => ValueType::I64,
            Self::F32 => ValueType::F32,
            Self::F64 => ValueType::F64,
        }
    }
    fn float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

fn scalar(ty: &HirType) -> Result<Scalar, String> {
    match ty {
        HirType::Bool => Ok(Scalar::Bool),
        HirType::I32 => Ok(Scalar::I32),
        HirType::I64 => Ok(Scalar::I64),
        HirType::F32 => Ok(Scalar::F32),
        HirType::F64 | HirType::Float => Ok(Scalar::F64),
        _ => Err(format!(
            "standalone WASM requires bool/i32/i64/f32/f64, got {ty:?}"
        )),
    }
}

#[derive(Clone)]
struct Signature {
    params: Vec<Scalar>,
    result: Option<Scalar>,
}
#[derive(Clone, Copy)]
struct Value {
    reg: VirtualRegister,
    ty: Scalar,
}
#[derive(Clone, Copy)]
struct Binding {
    value: Value,
    constant: bool,
}
fn reg(v: Value) -> O {
    O::Register(R::Virtual(v.reg))
}

struct Lower<'a> {
    f: Function,
    signatures: &'a BTreeMap<String, Signature>,
    scopes: Vec<BTreeMap<String, Binding>>,
    loops: Vec<(String, String)>,
    block: usize,
    terminated: bool,
    depth: usize,
    nodes: usize,
}

impl<'a> Lower<'a> {
    fn new(name: &str, signatures: &'a BTreeMap<String, Signature>) -> Self {
        Self {
            f: Function::new(name),
            signatures,
            scopes: vec![BTreeMap::new()],
            loops: Vec::new(),
            block: 0,
            terminated: false,
            depth: 0,
            nodes: 0,
        }
    }
    fn local(&mut self, ty: Scalar) -> Result<Value, String> {
        if self.f.locals.len() >= Limits::default().locals {
            return Err("WASM local limit exceeded".into());
        }
        Ok(Value {
            reg: self.f.local(ty.wasm()),
            ty,
        })
    }
    fn emit(&mut self, inst: I) {
        self.f.body.blocks[self.block].push(inst);
    }
    fn new_block(&mut self) -> Result<usize, String> {
        let n = self.f.body.blocks.len();
        if n >= Limits::default().blocks {
            return Err("WASM block limit exceeded".into());
        }
        self.f.body.create_block(format!("block{n}"));
        Ok(n)
    }
    fn label(&self, block: usize) -> String {
        self.f.body.blocks[block].label.clone()
    }
    fn select(&mut self, block: usize) {
        self.block = block;
        self.terminated = false;
    }
    fn jump(&mut self, block: usize) {
        if !self.terminated {
            self.emit(I::Branch {
                target: self.label(block),
            });
        }
        self.terminated = true;
    }
    fn tick(&mut self) -> Result<(), String> {
        self.nodes += 1;
        if self.depth >= 256 || self.nodes > 100_000 {
            return Err("standalone WASM lowering depth/node limit exceeded".into());
        }
        Ok(())
    }
    fn binding(&self, name: &str) -> Result<Binding, String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name))
            .copied()
            .ok_or_else(|| {
                format!("unknown local `{name}` (globals and closures are not supported)")
            })
    }
    fn bind(&mut self, name: &str, value: Value, constant: bool) -> Result<(), String> {
        let scope = self.scopes.last_mut().ok_or("missing local scope")?;
        if scope.contains_key(name) {
            return Err(format!("duplicate local `{name}`"));
        }
        scope.insert(name.into(), Binding { value, constant });
        Ok(())
    }
    fn copy(&mut self, value: Value) -> Result<Value, String> {
        let dst = self.local(value.ty)?;
        self.emit(I::Move {
            dst: reg(dst),
            src: reg(value),
        });
        Ok(dst)
    }
    fn assign(&mut self, name: &str, expr: &HirExpr) -> Result<Value, String> {
        let binding = self.binding(name)?;
        if binding.constant {
            return Err(format!("cannot assign const `{name}`"));
        }
        let value = self.expr(expr, Some(binding.value.ty))?;
        self.emit(I::Move {
            dst: reg(binding.value),
            src: reg(value),
        });
        self.copy(value)
    }
    fn expr(&mut self, expr: &HirExpr, expected: Option<Scalar>) -> Result<Value, String> {
        self.tick()?;
        self.depth += 1;
        let value = self.expression(expr, expected);
        self.depth -= 1;
        let value = value?;
        if expected.is_some_and(|ty| ty != value.ty) {
            return Err(format!(
                "standalone WASM type mismatch: expected {expected:?}, got {:?}",
                value.ty
            ));
        }
        Ok(value)
    }
    fn expression(&mut self, expr: &HirExpr, expected: Option<Scalar>) -> Result<Value, String> {
        use HirExpr as E;
        match expr {
            E::Literal(lit) => {
                let (ty, src) = match lit {
                    HirLiteral::Int(n) => (
                        expected
                            .filter(|t| matches!(t, Scalar::I32 | Scalar::I64))
                            .unwrap_or(Scalar::I64),
                        O::Immediate(*n),
                    ),
                    HirLiteral::I32(n) => (Scalar::I32, O::Immediate(i64::from(*n))),
                    HirLiteral::I64(n) => (Scalar::I64, O::Immediate(*n)),
                    HirLiteral::Bool(b) => (Scalar::Bool, O::Immediate(i64::from(*b))),
                    HirLiteral::Float(n) => (
                        expected.filter(|t| t.float()).unwrap_or(Scalar::F64),
                        O::FloatImmediate(*n),
                    ),
                    HirLiteral::F32(n) => (Scalar::F32, O::FloatImmediate(f64::from(*n))),
                    HirLiteral::F64(n) => (Scalar::F64, O::FloatImmediate(*n)),
                    _ => return Err("standalone WASM does not yet support this literal".into()),
                };
                let value = self.local(ty)?;
                self.emit(I::Move {
                    dst: reg(value),
                    src,
                });
                Ok(value)
            }
            // Snapshot reads: a later operand may assign to the same local.
            E::LoadVar(name) => self.copy(self.binding(name)?.value),
            E::StoreVar(name, expr) => self.assign(name, expr),
            E::BinaryOp(lhs, op @ (BinOp::And | BinOp::Or), rhs) => {
                let left = self.expr(lhs, Some(Scalar::Bool))?;
                let result = self.copy(left)?;
                let rhs_block = self.new_block()?;
                let done = self.new_block()?;
                self.emit(I::Compare {
                    lhs: reg(left),
                    rhs: O::Immediate(0),
                });
                self.emit(I::BranchCc {
                    cc: if *op == BinOp::And {
                        C::Equal
                    } else {
                        C::NotEqual
                    },
                    target: self.label(done),
                });
                self.jump(rhs_block);
                self.select(rhs_block);
                let right = self.expr(rhs, Some(Scalar::Bool))?;
                self.emit(I::Move {
                    dst: reg(result),
                    src: reg(right),
                });
                self.jump(done);
                self.select(done);
                Ok(result)
            }
            E::BinaryOp(lhs, op, rhs) => {
                let comparison = matches!(
                    op,
                    BinOp::Eq
                        | BinOp::Ne
                        | BinOp::StrictEq
                        | BinOp::StrictNe
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                );
                let left = self.expr(lhs, if comparison { None } else { expected })?;
                let right = self.expr(rhs, Some(left.ty))?;
                if comparison {
                    let cc = match op {
                        BinOp::Eq | BinOp::StrictEq => C::Equal,
                        BinOp::Ne | BinOp::StrictNe => C::NotEqual,
                        BinOp::Lt => C::LessThan,
                        BinOp::Le => C::LessOrEqual,
                        BinOp::Gt => C::GreaterThan,
                        _ => C::GreaterOrEqual,
                    };
                    let result = self.local(Scalar::Bool)?;
                    self.emit(I::Compare {
                        lhs: reg(left),
                        rhs: reg(right),
                    });
                    self.emit(I::SetCc {
                        dst: reg(result),
                        cc,
                    });
                    return Ok(result);
                }
                if left.ty == Scalar::Bool {
                    return Err("arithmetic on bool is not supported".into());
                }
                let dst = reg(left);
                let src = reg(right);
                let size = if left.ty == Scalar::F32 { 4 } else { 8 };
                let inst = if left.ty.float() {
                    match op {
                        BinOp::Add => I::FAdd { dst, src, size },
                        BinOp::Sub => I::FSub { dst, src, size },
                        BinOp::Mul => I::FMul { dst, src, size },
                        BinOp::Div => I::FDiv { dst, src, size },
                        _ => return Err(format!("unsupported floating operator {op:?}")),
                    }
                } else {
                    match op {
                        BinOp::Add => I::Add { dst, src },
                        BinOp::Sub => I::Sub { dst, src },
                        BinOp::Mul => I::Mul { dst, src },
                        BinOp::BitAnd => I::And { dst, src },
                        BinOp::BitOr => I::Or { dst, src },
                        BinOp::BitXor => I::Xor { dst, src },
                        // Division, modulo and shifts need language-specific rounding/count semantics.
                        _ => return Err(format!("unsupported integer operator {op:?}")),
                    }
                };
                self.emit(inst);
                Ok(left)
            }
            E::UnaryOp(op, expr) => {
                let value = self.expr(
                    expr,
                    if *op == UnaryOp::Not {
                        Some(Scalar::Bool)
                    } else {
                        expected
                    },
                )?;
                let dst = reg(value);
                match op {
                    UnaryOp::Not => {
                        let result = self.local(Scalar::Bool)?;
                        self.emit(I::Compare {
                            lhs: dst,
                            rhs: O::Immediate(0),
                        });
                        self.emit(I::SetCc {
                            dst: reg(result),
                            cc: C::Equal,
                        });
                        return Ok(result);
                    }
                    UnaryOp::Neg if value.ty.float() => self.emit(I::FNeg {
                        dst,
                        size: if value.ty == Scalar::F32 { 4 } else { 8 },
                    }),
                    UnaryOp::Neg if value.ty != Scalar::Bool => self.emit(I::Neg { dst }),
                    UnaryOp::BitNot if matches!(value.ty, Scalar::I32 | Scalar::I64) => {
                        self.emit(I::Not { dst })
                    }
                    _ => return Err("unsupported standalone unary operator".into()),
                }
                Ok(value)
            }
            E::Call(callee, args, type_args) if type_args.is_empty() => self
                .call(callee, args)?
                .ok_or_else(|| "void calls must be used as statements".into()),
            _ => Err("unsupported expression in standalone WASM (no legacy fallback)".into()),
        }
    }
    fn call(&mut self, callee: &HirExpr, args: &[HirExpr]) -> Result<Option<Value>, String> {
        let HirExpr::LoadVar(name) = callee else {
            return Err("standalone WASM requires a direct call".into());
        };
        let sig = self
            .signatures
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unsupported standalone callee `{name}`"))?;
        if self.scopes.iter().any(|s| s.contains_key(name)) {
            return Err("local shadows direct callee".into());
        }
        if args.len() != sig.params.len() {
            return Err(format!("argument count mismatch calling `{name}`"));
        }
        let arguments = args
            .iter()
            .zip(&sig.params)
            .map(|(e, ty)| self.expr(e, Some(*ty)).map(reg))
            .collect::<Result<Vec<_>, _>>()?;
        let result = sig.result.map(|ty| self.local(ty)).transpose()?;
        let block = &self.f.body.blocks[self.block];
        self.f.calls.insert(
            (block.id, block.instructions.len()),
            CallSite {
                arguments,
                result: result.map(|v| v.reg),
            },
        );
        self.emit(I::Call {
            target: O::Symbol(name.clone()),
            num_args: args.len(),
        });
        Ok(result)
    }
    fn stmt(&mut self, stmt: &HirStmt) -> Result<(), String> {
        self.tick()?;
        self.depth += 1;
        let result = self.statement(stmt);
        self.depth -= 1;
        result
    }
    fn scoped(&mut self, stmt: &HirStmt) -> Result<(), String> {
        self.scopes.push(BTreeMap::new());
        let result = self.stmt(stmt);
        self.scopes.pop();
        result
    }
    fn statement(&mut self, stmt: &HirStmt) -> Result<(), String> {
        match stmt {
            HirStmt::Block(stmts) => {
                self.scopes.push(BTreeMap::new());
                let result = self.statements(stmts);
                self.scopes.pop();
                result?;
            }
            HirStmt::Let {
                name,
                ty,
                init: Some(expr),
                is_const,
                is_borrowed: None,
            } => {
                let value = self.expr(expr, ty.as_ref().map(scalar).transpose()?)?;
                self.bind(name, value, *is_const)?;
            }
            HirStmt::Assign {
                target: HirExpr::LoadVar(name),
                value,
                ..
            } => {
                self.assign(name, value)?;
            }
            HirStmt::Expr(HirExpr::Call(callee, args, types)) if types.is_empty() => {
                self.call(callee, args)?;
            }
            HirStmt::Expr(expr) => {
                self.expr(expr, None)?;
            }
            HirStmt::Return(expr) => {
                match (self.f.result, expr) {
                    (Some(r), Some(expr)) => {
                        let ty = self.signatures[&self.f.body.name]
                            .result
                            .ok_or("missing return type")?;
                        let value = self.expr(expr, Some(ty))?;
                        self.emit(I::Move {
                            dst: O::Register(R::Virtual(r)),
                            src: reg(value),
                        });
                    }
                    (None, None) => {}
                    _ => return Err("standalone WASM return type mismatch".into()),
                }
                self.emit(I::Return);
                self.terminated = true;
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let value = self.expr(cond, Some(Scalar::Bool))?;
                let yes = self.new_block()?;
                let no = self.new_block()?;
                let done = self.new_block()?;
                self.emit(I::Compare {
                    lhs: reg(value),
                    rhs: O::Immediate(0),
                });
                self.emit(I::BranchCc {
                    cc: C::Equal,
                    target: self.label(no),
                });
                self.jump(yes);
                self.select(yes);
                self.scoped(then_branch)?;
                let yes_terminated = self.terminated;
                self.jump(done);
                self.select(no);
                if let Some(branch) = else_branch {
                    self.scoped(branch)?;
                }
                let no_terminated = self.terminated;
                self.jump(done);
                self.select(done);
                self.terminated = yes_terminated && no_terminated;
            }
            HirStmt::While { cond, body } => {
                let test = self.new_block()?;
                let work = self.new_block()?;
                let done = self.new_block()?;
                self.jump(test);
                self.select(test);
                let value = self.expr(cond, Some(Scalar::Bool))?;
                self.emit(I::Compare {
                    lhs: reg(value),
                    rhs: O::Immediate(0),
                });
                self.emit(I::BranchCc {
                    cc: C::Equal,
                    target: self.label(done),
                });
                self.jump(work);
                self.select(work);
                self.loops.push((self.label(test), self.label(done)));
                self.scoped(body)?;
                self.loops.pop();
                self.jump(test);
                self.select(done);
            }
            HirStmt::Break | HirStmt::Continue => {
                let (test, done) = self
                    .loops
                    .last()
                    .ok_or("break/continue outside loop")?
                    .clone();
                self.emit(I::Branch {
                    target: if matches!(stmt, HirStmt::Break) {
                        done
                    } else {
                        test
                    },
                });
                self.terminated = true;
            }
            _ => {
                return Err("unsupported statement in standalone WASM (no legacy fallback)".into());
            }
        }
        Ok(())
    }
    fn statements(&mut self, stmts: &[HirStmt]) -> Result<(), String> {
        for stmt in stmts {
            // Reject rather than silently discard unsupported unreachable code.
            if self.terminated {
                return Err("unreachable statement in standalone WASM".into());
            }
            self.stmt(stmt)?;
        }
        Ok(())
    }
}

/// Typed fixed-width scalars only. Dynamic `int`, heap values and implicit
/// signatures are refused rather than assigned a different representation.
pub fn lower(hir: &HirModule) -> Result<Module, String> {
    if !hir.classes.is_empty() || !hir.enums.is_empty() || !hir.statements.is_empty() {
        return Err("standalone WASM currently requires a function-only module".into());
    }
    if hir.functions.len() > Limits::default().functions {
        return Err("WASM function limit exceeded".into());
    }
    let mut signatures = BTreeMap::new();
    for f in &hir.functions {
        if f.is_async || f.is_unsafe || !f.decorators.is_empty() || !f.move_params.is_empty() {
            return Err(format!(
                "unsupported standalone function attributes on `{}`",
                f.name
            ));
        }
        let params = f
            .params
            .iter()
            .map(|(_, ty, default)| {
                if default.is_some() {
                    return Err("standalone WASM does not yet support default parameters".into());
                }
                scalar(
                    ty.as_ref()
                        .ok_or("standalone WASM requires explicit parameter types")?,
                )
            })
            .collect::<Result<_, String>>()?;
        let result = match &f.ret_type {
            Some(HirType::Null) => None,
            Some(ty) => Some(scalar(ty)?),
            None => return Err("standalone WASM requires an explicit return type".into()),
        };
        if signatures
            .insert(f.name.clone(), Signature { params, result })
            .is_some()
        {
            return Err(format!("duplicate standalone function `{}`", f.name));
        }
    }
    let mut module = Module::default();
    for f in &hir.functions {
        let sig = &signatures[&f.name];
        let mut lower = Lower::new(&f.name, &signatures);
        lower.f.body.is_exported = f.is_exported;
        for ((name, _, _), ty) in f.params.iter().zip(&sig.params) {
            let value = lower.local(*ty)?;
            lower.f.parameters.push(value.reg);
            lower.bind(name, value, false)?;
        }
        lower.f.result = sig
            .result
            .map(|ty| lower.local(ty).map(|v| v.reg))
            .transpose()?;
        lower
            .statements(&f.body)
            .map_err(|e| format!("standalone WASM `{}`: {e}", f.name))?;
        if !lower.terminated {
            if sig.result.is_some() {
                return Err(format!(
                    "standalone WASM `{}` may fall through without returning",
                    f.name
                ));
            }
            lower.emit(I::Return);
        }
        module.functions.push(lower.f);
    }
    Ok(module)
}

/// Use adeshlink's object reader, relocation planner and writer in memory.
/// No temporary object, external linker, or third-party WASM emitter.
pub fn compile(hir: &HirModule, object_only: bool) -> Result<Vec<u8>, String> {
    let bytes = lower(hir)?.emit_object(Limits::default())?;
    if object_only {
        return Ok(bytes);
    }
    use adesh_linker::wasm::{WasmLinkInfo, WasmReader, WasmWriter, apply_relocations};
    let mut objects =
        vec![WasmReader::read(&bytes, Path::new("standalone.o"), 0).map_err(|e| e.to_string())?];
    drop(bytes);
    let info = WasmLinkInfo::build(&objects, None, &[], false).map_err(|e| e.to_string())?;
    apply_relocations(&mut objects, &info).map_err(|e| e.to_string())?;
    WasmWriter::encode_binary(&objects, &info).map_err(|e| e.to_string())
}
