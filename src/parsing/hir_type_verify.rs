//! Typed HIR Verification Pass (Phase 19.2.1).
//!
//! Validates structural and type invariants on `HirModule` before lowering to
//! MIR, LIR, or native Machine IR so that malformed, inconsistent, or
//! unresolved types cannot silently reach code generation.

use super::hir::{
    BinOp, HirClass, HirEnum, HirExpr, HirExternFunction, HirFunction, HirLiteral, HirModule,
    HirPattern, HirStmt, HirStruct, HirType, UnaryOp,
};
use std::collections::{HashMap, HashSet};

/// Explicit classification of HIR types to distinguish unknown/unresolved types,
/// dynamic `Any`, `Null`/void-like types, references, and concrete scalars/composites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirTypeClass {
    Unknown,
    DynamicAny,
    NullOrVoid,
    Bool,
    Char,
    SignedInt(u8),
    UnsignedInt(u8),
    Float(u8),
    String,
    PointerOrBorrow { mutable: bool },
    SharedRef,
    WeakRef,
    Promise,
    Composite,
    Function,
    Simd,
}

/// Classify a `HirType` into its semantic category.
pub fn classify_hir_type(ty: &HirType) -> HirTypeClass {
    match ty {
        HirType::Unknown => HirTypeClass::Unknown,
        HirType::Any => HirTypeClass::DynamicAny,
        HirType::Null => HirTypeClass::NullOrVoid,
        HirType::Bool => HirTypeClass::Bool,
        HirType::Char => HirTypeClass::Char,
        HirType::I8 => HirTypeClass::SignedInt(8),
        HirType::I16 => HirTypeClass::SignedInt(16),
        HirType::I32 | HirType::Int => HirTypeClass::SignedInt(32),
        HirType::I64 => HirTypeClass::SignedInt(64),
        HirType::I128 => HirTypeClass::SignedInt(128),
        HirType::U8 => HirTypeClass::UnsignedInt(8),
        HirType::U16 => HirTypeClass::UnsignedInt(16),
        HirType::U32 => HirTypeClass::UnsignedInt(32),
        HirType::U64 => HirTypeClass::UnsignedInt(64),
        HirType::U128 => HirTypeClass::UnsignedInt(128),
        HirType::F32 => HirTypeClass::Float(32),
        HirType::Float | HirType::F64 => HirTypeClass::Float(64),
        HirType::String => HirTypeClass::String,
        HirType::Borrow(_, is_mut) => HirTypeClass::PointerOrBorrow { mutable: *is_mut },
        HirType::BorrowImmut(_) => HirTypeClass::PointerOrBorrow { mutable: false },
        HirType::BorrowMut(_) => HirTypeClass::PointerOrBorrow { mutable: true },
        HirType::Shared(_) => HirTypeClass::SharedRef,
        HirType::Weak(_) => HirTypeClass::WeakRef,
        HirType::Promise(_) => HirTypeClass::Promise,
        HirType::Array(..)
        | HirType::Dict(..)
        | HirType::Set(_)
        | HirType::Tuple(_)
        | HirType::Object
        | HirType::Class(_)
        | HirType::Instance(_) => HirTypeClass::Composite,
        HirType::Function(..) => HirTypeClass::Function,
        HirType::Simd(..) => HirTypeClass::Simd,
    }
}

/// Options controlling how strictly `verify_hir_module_with_options` enforces typing.
#[derive(Debug, Clone, Copy, Default)]
pub struct HirVerifyOptions {
    /// When true, reject any `HirType::Unknown`, missing parameter/return/let type
    /// annotations, or expressions whose static type cannot be resolved.
    pub strict_no_unknown: bool,
}

/// Structured diagnostic emitted by Typed HIR verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirTypeError {
    pub code: &'static str,
    pub function: String,
    pub message: String,
}

impl std::fmt::Display for HirTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.code, self.function, self.message)
    }
}

/// Verify structural and type invariants on an `HirModule` using default (gradual) options.
pub fn verify_hir_module(module: &HirModule) -> Result<(), Vec<HirTypeError>> {
    verify_hir_module_with_options(
        module,
        HirVerifyOptions {
            strict_no_unknown: false,
        },
    )
}

/// Strictly verify that every signature, binding, and expression has resolved concrete types
/// with zero `HirType::Unknown` reaching code generation.
pub fn verify_typed_hir_strict(module: &HirModule) -> Result<(), Vec<HirTypeError>> {
    verify_hir_module_with_options(
        module,
        HirVerifyOptions {
            strict_no_unknown: true,
        },
    )
}

pub fn verify_hir_module_with_options(
    module: &HirModule,
    options: HirVerifyOptions,
) -> Result<(), Vec<HirTypeError>> {
    let mut verifier = HirTypeVerifier::new(module, options);
    verifier.verify_module(module);
    if verifier.errors.is_empty() {
        Ok(())
    } else {
        Err(verifier.errors)
    }
}

struct FnSig {
    min_params: usize,
    max_params: usize,
    is_variadic: bool,
    is_extern: bool,
    param_types: Vec<Option<HirType>>,
    ret_type: Option<HirType>,
}

struct HirTypeVerifier<'a> {
    options: HirVerifyOptions,
    errors: Vec<HirTypeError>,
    structs: HashMap<String, &'a HirStruct>,
    enums: HashMap<String, HashSet<String>>,
    enum_variants: HashMap<String, String>,
    functions: HashMap<String, FnSig>,
    classes: HashSet<String>,
}

impl<'a> HirTypeVerifier<'a> {
    fn new(module: &'a HirModule, options: HirVerifyOptions) -> Self {
        let mut structs = HashMap::new();
        for s in &module.structs {
            structs.insert(s.name.clone(), s);
        }

        let mut enums = HashMap::new();
        let mut enum_variants = HashMap::new();
        for e in &module.enums {
            let mut vars = HashSet::new();
            for (vname, _) in &e.variants {
                vars.insert(vname.clone());
                enum_variants.insert(vname.clone(), e.name.clone());
                enum_variants.insert(format!("{}::{}", e.name, vname), e.name.clone());
                enum_variants.insert(format!("{}.{}", e.name, vname), e.name.clone());
            }
            enums.insert(e.name.clone(), vars);
        }

        let mut classes = HashSet::new();
        for c in &module.classes {
            classes.insert(c.name.clone());
        }

        let mut functions = HashMap::new();
        for f in &module.functions {
            let min_params = f
                .params
                .iter()
                .take_while(|(_, _, default)| default.is_none())
                .count();
            functions.insert(
                f.name.clone(),
                FnSig {
                    min_params,
                    max_params: f.params.len(),
                    is_variadic: false,
                    is_extern: false,
                    param_types: f.params.iter().map(|(_, ty, _)| ty.clone()).collect(),
                    ret_type: f.ret_type.clone(),
                },
            );
        }
        for ext in &module.externs {
            functions.insert(
                ext.name.clone(),
                FnSig {
                    min_params: ext.params.len(),
                    max_params: ext.params.len(),
                    is_variadic: ext.is_variadic,
                    is_extern: true,
                    param_types: ext.params.iter().map(|(_, ty)| Some(ty.clone())).collect(),
                    ret_type: Some(ext.ret_type.clone()),
                },
            );
        }

        Self {
            options,
            errors: Vec::new(),
            structs,
            enums,
            enum_variants,
            functions,
            classes,
        }
    }

    fn push_err(
        &mut self,
        code: &'static str,
        function: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.errors.push(HirTypeError {
            code,
            function: function.into(),
            message: message.into(),
        });
    }

    fn verify_module(&mut self, module: &HirModule) {
        self.verify_externs(&module.externs);
        self.verify_structs(&module.structs);
        self.verify_enums(&module.enums);

        let mut fn_names = HashSet::new();
        for f in &module.functions {
            if !fn_names.insert(f.name.clone()) {
                self.push_err(
                    "HIR-T001",
                    &f.name,
                    format!("duplicate function definition `{}`", f.name),
                );
            }
            self.verify_function(f);
        }

        for c in &module.classes {
            self.verify_class(c);
        }

        for tls in &module.tls_vars {
            if let Some(ref ty) = tls.ty {
                self.verify_type_wellformed(ty, &format!("tls::{}", tls.name), true);
                if let Some(ref init) = tls.init {
                    let mut env = HashMap::new();
                    self.verify_expr(init, &format!("tls::{}", tls.name), &mut env);
                    if let Some(init_ty) = self.infer_expr_type(init, &env)
                        && !types_compatible(ty, &init_ty)
                    {
                        self.push_err(
                            "HIR-T010",
                            format!("tls::{}", tls.name),
                            format!(
                                "TLS variable `{}` annotated as `{:?}` cannot be initialized with `{:?}`",
                                tls.name, ty, init_ty
                            ),
                        );
                    }
                }
            } else if self.options.strict_no_unknown {
                self.push_err(
                    "HIR-T002",
                    format!("tls::{}", tls.name),
                    format!(
                        "TLS variable `{}` is missing an explicit type in strict mode",
                        tls.name
                    ),
                );
            }
        }

        let mut top_env = HashMap::new();
        for stmt in &module.statements {
            self.verify_stmt(stmt, "<module>", None, &mut top_env);
        }
    }

    fn verify_externs(&mut self, externs: &[HirExternFunction]) {
        let mut seen = HashSet::new();
        for ext in externs {
            if !seen.insert(ext.name.clone()) {
                self.push_err(
                    "HIR-T003",
                    format!("extern::{}", ext.name),
                    format!("duplicate extern declaration `{}`", ext.name),
                );
            }
            let abi_ok = matches!(
                ext.abi.as_str(),
                "C" | "c" | "SysV" | "sysv" | "Win64" | "win64" | "stdcall" | "aapcs"
            );
            if !abi_ok {
                self.push_err(
                    "HIR-T004",
                    format!("extern::{}", ext.name),
                    format!("unsupported extern ABI `{}` on `{}`", ext.abi, ext.name),
                );
            }
            for (pname, pty) in &ext.params {
                if matches!(pty, HirType::Unknown) {
                    self.push_err(
                        "HIR-T005",
                        format!("extern::{}", ext.name),
                        format!("extern parameter `{}` has unresolved `Unknown` type", pname),
                    );
                }
                self.verify_type_wellformed(pty, &format!("extern::{}", ext.name), true);
            }
            if matches!(ext.ret_type, HirType::Unknown) {
                self.push_err(
                    "HIR-T005",
                    format!("extern::{}", ext.name),
                    format!("extern `{}` has unresolved `Unknown` return type", ext.name),
                );
            }
            self.verify_type_wellformed(&ext.ret_type, &format!("extern::{}", ext.name), true);
        }
    }

    fn verify_structs(&mut self, structs: &[HirStruct]) {
        let mut seen = HashSet::new();
        for s in structs {
            if s.name.trim().is_empty() {
                self.push_err("HIR-T006", "<struct>", "struct name cannot be empty");
            }
            if !seen.insert(s.name.clone()) {
                self.push_err(
                    "HIR-T006",
                    format!("struct::{}", s.name),
                    format!("duplicate struct definition `{}`", s.name),
                );
            }
            let mut field_names = HashSet::new();
            for (fname, fty) in &s.fields {
                if !field_names.insert(fname.clone()) {
                    self.push_err(
                        "HIR-T006",
                        format!("struct::{}", s.name),
                        format!("duplicate field `{}` in struct `{}`", fname, s.name),
                    );
                }
                if matches!(fty, HirType::Unknown) {
                    self.push_err(
                        "HIR-T005",
                        format!("struct::{}", s.name),
                        format!(
                            "field `{}` of struct `{}` has `Unknown` type",
                            fname, s.name
                        ),
                    );
                }
                self.verify_type_wellformed(fty, &format!("struct::{}", s.name), true);
            }
        }
    }

    fn verify_enums(&mut self, enums: &[HirEnum]) {
        let mut seen = HashSet::new();
        for e in enums {
            if !seen.insert(e.name.clone()) {
                self.push_err(
                    "HIR-T007",
                    format!("enum::{}", e.name),
                    format!("duplicate enum definition `{}`", e.name),
                );
            }
            let mut variant_names = HashSet::new();
            for (vname, _) in &e.variants {
                if !variant_names.insert(vname.clone()) {
                    self.push_err(
                        "HIR-T007",
                        format!("enum::{}", e.name),
                        format!("duplicate variant `{}` in enum `{}`", vname, e.name),
                    );
                }
            }
        }
    }

    fn verify_class(&mut self, class: &HirClass) {
        let mut method_names = HashSet::new();
        for m in &class.methods {
            let scope = format!("{}::{}", class.name, m.name);
            if !method_names.insert(m.name.clone()) {
                self.push_err(
                    "HIR-T008",
                    &scope,
                    format!("duplicate method `{}` on class `{}`", m.name, class.name),
                );
            }
            self.verify_callable(
                &scope,
                m.params.iter().map(|(n, ty)| (n.as_str(), ty.as_ref())),
                &[],
                m.ret_type.as_ref(),
                &m.body,
            );
        }
        let mut static_names = HashSet::new();
        for m in &class.static_methods {
            let scope = format!("{}::static::{}", class.name, m.name);
            if !static_names.insert(m.name.clone()) {
                self.push_err(
                    "HIR-T008",
                    &scope,
                    format!(
                        "duplicate static method `{}` on class `{}`",
                        m.name, class.name
                    ),
                );
            }
            self.verify_callable(
                &scope,
                m.params.iter().map(|(n, ty)| (n.as_str(), ty.as_ref())),
                &[],
                m.ret_type.as_ref(),
                &m.body,
            );
        }
    }

    fn verify_function(&mut self, func: &HirFunction) {
        self.verify_callable(
            &func.name,
            func.params
                .iter()
                .map(|(n, ty, _)| (n.as_str(), ty.as_ref())),
            &func.move_params,
            func.ret_type.as_ref(),
            &func.body,
        );
    }

    fn verify_callable<'b>(
        &mut self,
        scope_name: &str,
        params: impl Iterator<Item = (&'b str, Option<&'b HirType>)>,
        move_params: &[usize],
        ret_type: Option<&HirType>,
        body: &[HirStmt],
    ) {
        let mut param_vec = Vec::new();
        let mut param_names = HashSet::new();
        let mut env: HashMap<String, Option<HirType>> = HashMap::new();

        for (pname, pty) in params {
            if !param_names.insert(pname.to_string()) {
                self.push_err(
                    "HIR-T009",
                    scope_name,
                    format!("duplicate parameter `{}`", pname),
                );
            }
            if let Some(ty) = pty {
                self.verify_type_wellformed(ty, scope_name, self.options.strict_no_unknown);
            } else if self.options.strict_no_unknown {
                self.push_err(
                    "HIR-T002",
                    scope_name,
                    format!("parameter `{}` has no resolved type in strict mode", pname),
                );
            }
            env.insert(pname.to_string(), pty.cloned());
            param_vec.push(pname.to_string());
        }

        for &idx in move_params {
            if idx >= param_vec.len() {
                self.push_err(
                    "HIR-T009",
                    scope_name,
                    format!(
                        "move_params index {} out of bounds for {} parameters",
                        idx,
                        param_vec.len()
                    ),
                );
            }
        }

        if let Some(rt) = ret_type {
            self.verify_type_wellformed(rt, scope_name, self.options.strict_no_unknown);
        } else if self.options.strict_no_unknown {
            self.push_err(
                "HIR-T002",
                scope_name,
                "function is missing return type in strict mode",
            );
        }

        for stmt in body {
            self.verify_stmt(stmt, scope_name, ret_type, &mut env);
        }
    }

    fn verify_type_wellformed(&mut self, ty: &HirType, scope: &str, disallow_unknown: bool) {
        match ty {
            HirType::Unknown => {
                if disallow_unknown {
                    self.push_err(
                        "HIR-T005",
                        scope,
                        "unresolved `HirType::Unknown` reached typed HIR verification",
                    );
                }
            }
            HirType::Simd(elem, lanes) => {
                if *lanes == 0 || !lanes.is_power_of_two() || *lanes > 64 {
                    self.push_err(
                        "HIR-T011",
                        scope,
                        format!(
                            "invalid SIMD lane count {} (expected power of two in 1..=64)",
                            lanes
                        ),
                    );
                }
                self.verify_type_wellformed(elem, scope, disallow_unknown);
            }
            HirType::Array(elem, _)
            | HirType::Set(elem)
            | HirType::Promise(elem)
            | HirType::Borrow(elem, _)
            | HirType::BorrowImmut(elem)
            | HirType::BorrowMut(elem)
            | HirType::Shared(elem)
            | HirType::Weak(elem) => self.verify_type_wellformed(elem, scope, disallow_unknown),
            HirType::Dict(k, v) => {
                self.verify_type_wellformed(k, scope, disallow_unknown);
                self.verify_type_wellformed(v, scope, disallow_unknown);
            }
            HirType::Tuple(elems) => {
                for e in elems {
                    self.verify_type_wellformed(e, scope, disallow_unknown);
                }
            }
            HirType::Function(params, ret) => {
                for p in params {
                    self.verify_type_wellformed(p, scope, disallow_unknown);
                }
                self.verify_type_wellformed(ret, scope, disallow_unknown);
            }
            _ => {}
        }
    }

    fn verify_stmt(
        &mut self,
        stmt: &HirStmt,
        scope: &str,
        ret_type: Option<&HirType>,
        env: &mut HashMap<String, Option<HirType>>,
    ) {
        match stmt {
            HirStmt::Let { name, ty, init, .. } => {
                if let Some(t) = ty {
                    self.verify_type_wellformed(t, scope, self.options.strict_no_unknown);
                }
                if let Some(expr) = init {
                    self.verify_expr(expr, scope, env);
                    let inferred = self.infer_expr_type(expr, env);
                    if let (Some(ann), Some(actual)) = (ty, &inferred)
                        && !types_compatible(ann, actual)
                    {
                        self.push_err(
                            "HIR-T010",
                            scope,
                            format!(
                                "cannot assign expression of type `{:?}` to variable `{}` of type `{:?}`",
                                actual, name, ann
                            ),
                        );
                    }
                    if self.options.strict_no_unknown && ty.is_none() && inferred.is_none() {
                        self.push_err(
                            "HIR-T002",
                            scope,
                            format!("variable `{}` has unresolved type in strict mode", name),
                        );
                    }
                    env.insert(name.clone(), ty.clone().or(inferred));
                } else {
                    if self.options.strict_no_unknown && ty.is_none() {
                        self.push_err(
                            "HIR-T002",
                            scope,
                            format!(
                                "uninitialized variable `{}` has no type annotation in strict mode",
                                name
                            ),
                        );
                    }
                    env.insert(name.clone(), ty.clone());
                }
            }
            HirStmt::LetTuple { names, init, .. } => {
                if let Some(expr) = init {
                    self.verify_expr(expr, scope, env);
                }
                for n in names {
                    env.insert(n.clone(), None);
                }
            }
            HirStmt::Assign { target, value, .. } => {
                self.verify_expr(target, scope, env);
                self.verify_expr(value, scope, env);
                if let (Some(target_ty), Some(val_ty)) = (
                    self.infer_expr_type(target, env),
                    self.infer_expr_type(value, env),
                ) && !types_compatible(&target_ty, &val_ty)
                {
                    self.push_err(
                        "HIR-T010",
                        scope,
                        format!(
                            "incompatible assignment: target has type `{:?}`, value has type `{:?}`",
                            target_ty, val_ty
                        ),
                    );
                }
            }
            HirStmt::Expr(expr) | HirStmt::Throw(expr) | HirStmt::Jump(expr) => {
                self.verify_expr(expr, scope, env);
            }
            HirStmt::Return(Some(expr)) => {
                self.verify_expr(expr, scope, env);
                if let (Some(expected), Some(actual)) = (ret_type, self.infer_expr_type(expr, env))
                    && !types_compatible(expected, &actual)
                {
                    self.push_err(
                        "HIR-T012",
                        scope,
                        format!(
                            "return type mismatch: expected `{:?}`, found `{:?}`",
                            expected, actual
                        ),
                    );
                }
            }
            HirStmt::Return(None) => {
                if let Some(expected) = ret_type
                    && !matches!(expected, HirType::Null | HirType::Any | HirType::Unknown)
                {
                    self.push_err(
                        "HIR-T012",
                        scope,
                        format!("bare `return` in function expecting `{:?}`", expected),
                    );
                }
            }
            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.verify_expr(cond, scope, env);
                self.verify_condition_type(cond, scope, env);
                let mut then_env = env.clone();
                self.verify_stmt(then_branch, scope, ret_type, &mut then_env);
                if let Some(eb) = else_branch {
                    let mut else_env = env.clone();
                    self.verify_stmt(eb, scope, ret_type, &mut else_env);
                }
            }
            HirStmt::While { cond, body } => {
                self.verify_expr(cond, scope, env);
                self.verify_condition_type(cond, scope, env);
                let mut loop_env = env.clone();
                self.verify_stmt(body, scope, ret_type, &mut loop_env);
            }
            HirStmt::ForIn { var, iter, body } => {
                self.verify_expr(iter, scope, env);
                let mut loop_env = env.clone();
                let elem_ty = match self.infer_expr_type(iter, env) {
                    Some(HirType::Array(inner, _)) | Some(HirType::Set(inner)) => Some(*inner),
                    _ => None,
                };
                loop_env.insert(var.clone(), elem_ty);
                self.verify_stmt(body, scope, ret_type, &mut loop_env);
            }
            HirStmt::Block(stmts) => {
                let mut inner_env = env.clone();
                for s in stmts {
                    self.verify_stmt(s, scope, ret_type, &mut inner_env);
                }
            }
            HirStmt::FunctionDef {
                name,
                params,
                body,
                ret_type: fn_rt,
                move_params,
                ..
            } => {
                let nested_scope = format!("{}::{}", scope, name);
                self.verify_callable(
                    &nested_scope,
                    params.iter().map(|(n, ty, _)| (n.as_str(), ty.as_ref())),
                    move_params,
                    fn_rt.as_ref(),
                    body,
                );
            }
            HirStmt::TryCatch {
                try_block,
                error_name,
                catch_block,
            } => {
                let mut try_env = env.clone();
                self.verify_stmt(try_block, scope, ret_type, &mut try_env);
                let mut catch_env = env.clone();
                catch_env.insert(error_name.clone(), Some(HirType::Any));
                self.verify_stmt(catch_block, scope, ret_type, &mut catch_env);
            }
            HirStmt::Region { body, .. } | HirStmt::Defer(body) | HirStmt::Unsafe(body) => {
                let mut inner_env = env.clone();
                self.verify_stmt(body, scope, ret_type, &mut inner_env);
            }
            HirStmt::ClassDef(c) => self.verify_class(c),
            HirStmt::Extend { target, methods } => {
                for m in methods {
                    let s = format!("{}::{}", target, m.name);
                    self.verify_callable(
                        &s,
                        m.params.iter().map(|(n, ty, _)| (n.as_str(), ty.as_ref())),
                        &m.move_params,
                        m.ret_type.as_ref(),
                        &m.body,
                    );
                }
            }
            HirStmt::Break
            | HirStmt::Continue
            | HirStmt::Import { .. }
            | HirStmt::ImportDefault { .. }
            | HirStmt::ImportNames { .. } => {}
        }
    }

    fn verify_condition_type(
        &mut self,
        cond: &HirExpr,
        scope: &str,
        env: &HashMap<String, Option<HirType>>,
    ) {
        if let Some(ty) = self.infer_expr_type(cond, env) {
            if matches!(ty, HirType::Unknown) && self.options.strict_no_unknown {
                self.push_err(
                    "HIR-T013",
                    scope,
                    "branch condition has unresolved `Unknown` type in strict mode",
                );
            } else if matches!(ty, HirType::Dict(..) | HirType::Set(_)) {
                self.push_err(
                    "HIR-T013",
                    scope,
                    format!(
                        "branch condition has non-boolean collection type `{:?}`",
                        ty
                    ),
                );
            }
        }
    }

    fn verify_expr(
        &mut self,
        expr: &HirExpr,
        scope: &str,
        env: &mut HashMap<String, Option<HirType>>,
    ) {
        match expr {
            HirExpr::Literal(_) | HirExpr::This | HirExpr::Super => {}
            HirExpr::LoadVar(name) => {
                if self.options.strict_no_unknown
                    && !env.contains_key(name)
                    && !self.functions.contains_key(name)
                    && !self.classes.contains(name)
                    && !self.enums.contains_key(name)
                    && !self.enum_variants.contains_key(name)
                    && !matches!(name.as_str(), "None" | "Some" | "Ok" | "Err")
                {
                    self.push_err(
                        "HIR-T014",
                        scope,
                        format!("unresolved variable reference `{}` in strict mode", name),
                    );
                }
            }
            HirExpr::BinaryOp(l, op, r) => {
                self.verify_expr(l, scope, env);
                self.verify_expr(r, scope, env);
                if matches!(
                    op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::IntDiv
                ) && let (Some(lt), Some(rt)) =
                    (self.infer_expr_type(l, env), self.infer_expr_type(r, env))
                    && !matches!(op, BinOp::Add)
                    && (!is_numeric_like(&lt) || !is_numeric_like(&rt))
                {
                    self.push_err(
                        "HIR-T015",
                        scope,
                        format!(
                            "arithmetic operator `{:?}` requires numeric operands, found `{:?}` and `{:?}`",
                            op, lt, rt
                        ),
                    );
                }
            }
            HirExpr::UnaryOp(op, inner) => {
                self.verify_expr(inner, scope, env);
                if matches!(op, UnaryOp::Neg)
                    && let Some(t) = self.infer_expr_type(inner, env)
                    && !is_numeric_like(&t)
                {
                    self.push_err(
                        "HIR-T015",
                        scope,
                        format!("unary negation requires numeric operand, found `{:?}`", t),
                    );
                }
            }
            HirExpr::Call(callee, args, _) => {
                self.verify_expr(callee, scope, env);
                for a in args {
                    self.verify_expr(a, scope, env);
                }
                if let HirExpr::LoadVar(fn_name) = callee.as_ref()
                    && let Some(sig) = self.functions.get(fn_name)
                {
                    let min_params = sig.min_params;
                    let max_params = sig.max_params;
                    let is_variadic = sig.is_variadic;
                    let is_extern = sig.is_extern;
                    let param_types = sig.param_types.clone();
                    let too_few = args.len() < min_params;
                    let too_many = !is_variadic && args.len() > max_params;
                    if too_few || too_many {
                        self.push_err(
                            "HIR-T016",
                            scope,
                            format!(
                                "call to `{}` passed {} arguments, expected {}{}",
                                fn_name,
                                args.len(),
                                if min_params == max_params {
                                    format!("{}", min_params)
                                } else {
                                    format!("{}..={}", min_params, max_params)
                                },
                                if is_variadic { "+" } else { "" }
                            ),
                        );
                    }
                    let check_count = args.len().min(param_types.len());
                    for i in 0..check_count {
                        if let (Some(expected), Some(actual)) =
                            (param_types[i].as_ref(), self.infer_expr_type(&args[i], env))
                            && !types_compatible(expected, &actual)
                            && !(is_extern
                                && matches!(actual, HirType::Null)
                                && matches!(
                                    expected,
                                    HirType::Borrow(_, _)
                                        | HirType::BorrowImmut(_)
                                        | HirType::BorrowMut(_)
                                ))
                        {
                            self.push_err(
                                "HIR-T016",
                                scope,
                                format!(
                                    "argument {} to `{}` expects `{:?}`, found `{:?}`",
                                    i + 1,
                                    fn_name,
                                    expected,
                                    actual
                                ),
                            );
                        }
                    }
                }
            }
            HirExpr::MethodCall(obj, _, args) => {
                self.verify_expr(obj, scope, env);
                for a in args {
                    self.verify_expr(a, scope, env);
                }
            }
            HirExpr::StructLiteral(name, fields) => {
                for (_, val) in fields {
                    self.verify_expr(val, scope, env);
                }
                if let Some(struct_def) = self.structs.get(name).copied() {
                    let mut provided = HashMap::new();
                    for (fname, fval) in fields {
                        if provided.insert(fname.clone(), fval).is_some() {
                            self.push_err(
                                "HIR-T017",
                                scope,
                                format!("duplicate field `{}` in struct literal `{}`", fname, name),
                            );
                        }
                        if let Some((_, declared_ty)) =
                            struct_def.fields.iter().find(|(n, _)| n == fname)
                        {
                            if let Some(actual_ty) = self.infer_expr_type(fval, env)
                                && !types_compatible(declared_ty, &actual_ty)
                            {
                                self.push_err(
                                    "HIR-T017",
                                    scope,
                                    format!(
                                        "struct `{}` field `{}` expects `{:?}`, found `{:?}`",
                                        name, fname, declared_ty, actual_ty
                                    ),
                                );
                            }
                        } else {
                            self.push_err(
                                "HIR-T017",
                                scope,
                                format!("unknown field `{}` in struct literal `{}`", fname, name),
                            );
                        }
                    }
                    for (declared_name, _) in &struct_def.fields {
                        if !provided.contains_key(declared_name) {
                            self.push_err(
                                "HIR-T017",
                                scope,
                                format!(
                                    "missing required field `{}` in struct literal `{}`",
                                    declared_name, name
                                ),
                            );
                        }
                    }
                }
            }
            HirExpr::Cast(inner, target_ty) => {
                self.verify_expr(inner, scope, env);
                if matches!(target_ty, HirType::Unknown) {
                    self.push_err(
                        "HIR-T018",
                        scope,
                        "cast target cannot be unresolved `HirType::Unknown`",
                    );
                }
                self.verify_type_wellformed(target_ty, scope, true);
                if matches!(
                    target_ty,
                    HirType::Borrow(..)
                        | HirType::BorrowImmut(_)
                        | HirType::BorrowMut(_)
                        | HirType::Shared(_)
                        | HirType::Weak(_)
                        | HirType::Promise(_)
                ) {
                    self.push_err(
                        "HIR-T018",
                        scope,
                        format!(
                            "cast to ownership/handle type `{:?}` is forbidden; use explicit ownership operators",
                            target_ty
                        ),
                    );
                }
                if let Some(src_ty) = self.infer_expr_type(inner, env)
                    && matches!(
                        src_ty,
                        HirType::Array(..) | HirType::Dict(..) | HirType::Set(_)
                    )
                    && is_numeric_like(target_ty)
                {
                    self.push_err(
                        "HIR-T018",
                        scope,
                        format!(
                            "cannot cast collection `{:?}` to numeric type `{:?}`",
                            src_ty, target_ty
                        ),
                    );
                }
            }
            HirExpr::Alloc(ty, count) => {
                self.verify_expr(count, scope, env);
                if matches!(ty.as_ref(), HirType::Unknown | HirType::Null) {
                    self.push_err(
                        "HIR-T019",
                        scope,
                        format!(
                            "cannot allocate `{:?}`: concrete non-null type required",
                            ty
                        ),
                    );
                }
                self.verify_type_wellformed(ty, scope, true);
            }
            HirExpr::Match(scrutinee, arms) => {
                self.verify_expr(scrutinee, scope, env);
                let mut enum_seen: Option<String> = None;
                let mut arm_ty: Option<HirType> = None;
                for (pat, body) in arms {
                    self.verify_pattern(pat, scope, &mut enum_seen);
                    let mut arm_env = env.clone();
                    Self::bind_pattern_vars(pat, &mut arm_env);
                    self.verify_expr(body, scope, &mut arm_env);
                    if let Some(this_ty) = self.infer_expr_type(body, &arm_env) {
                        if let Some(prev) = &arm_ty {
                            if !types_compatible(prev, &this_ty)
                                && !types_compatible(&this_ty, prev)
                            {
                                self.push_err(
                                    "HIR-T020",
                                    scope,
                                    format!(
                                        "match arm types disagree: `{:?}` vs `{:?}`",
                                        prev, this_ty
                                    ),
                                );
                            }
                        } else {
                            arm_ty = Some(this_ty);
                        }
                    }
                }
            }
            HirExpr::ArrayLiteral(elems)
            | HirExpr::SetLiteral(elems)
            | HirExpr::TupleLiteral(elems) => {
                for e in elems {
                    self.verify_expr(e, scope, env);
                }
            }
            HirExpr::DictLiteral(pairs) => {
                for (k, v) in pairs {
                    self.verify_expr(k, scope, env);
                    self.verify_expr(v, scope, env);
                }
            }
            HirExpr::ObjectLiteral(fields) => {
                for (_, v) in fields {
                    self.verify_expr(v, scope, env);
                }
            }
            HirExpr::AssignTuple(_, rhs) | HirExpr::AssignObject(_, rhs) => {
                self.verify_expr(rhs, scope, env);
            }
            HirExpr::Index(obj, idx) => {
                self.verify_expr(obj, scope, env);
                self.verify_expr(idx, scope, env);
            }
            HirExpr::MemberAccess(obj, _)
            | HirExpr::OptionalGet(obj, _)
            | HirExpr::NonNull(obj)
            | HirExpr::Update(obj, _, _)
            | HirExpr::Format(obj, _)
            | HirExpr::Borrow(obj, _)
            | HirExpr::BorrowImmut(obj)
            | HirExpr::BorrowMut(obj)
            | HirExpr::Deref(obj)
            | HirExpr::Share(obj)
            | HirExpr::Downgrade(obj)
            | HirExpr::Free(obj)
            | HirExpr::Move(obj)
            | HirExpr::Await(obj)
            | HirExpr::Spawn(obj)
            | HirExpr::Spread(obj) => {
                self.verify_expr(obj, scope, env);
            }
            HirExpr::SetMember(obj, _, val) | HirExpr::Range(obj, val, _) => {
                self.verify_expr(obj, scope, env);
                self.verify_expr(val, scope, env);
            }
            HirExpr::Conditional(cond, then_e, else_e) => {
                self.verify_expr(cond, scope, env);
                self.verify_condition_type(cond, scope, env);
                self.verify_expr(then_e, scope, env);
                self.verify_expr(else_e, scope, env);
            }
            HirExpr::StoreVar(name, val) => {
                self.verify_expr(val, scope, env);
                if let (Some(Some(var_ty)), Some(val_ty)) =
                    (env.get(name).cloned(), self.infer_expr_type(val, env))
                    && !types_compatible(&var_ty, &val_ty)
                {
                    self.push_err(
                        "HIR-T010",
                        scope,
                        format!(
                            "cannot store `{:?}` into variable `{}` of type `{:?}`",
                            val_ty, name, var_ty
                        ),
                    );
                }
            }
            HirExpr::Lambda(params, body, _) => {
                let lambda_scope = format!("{}::<lambda>", scope);
                self.verify_callable(
                    &lambda_scope,
                    params.iter().map(|(n, ty)| (n.as_str(), ty.as_ref())),
                    &[],
                    None,
                    body,
                );
            }
            HirExpr::NewInstance(_, args) => {
                for a in args {
                    self.verify_expr(a, scope, env);
                }
            }
        }
    }

    fn verify_pattern(&mut self, pat: &HirPattern, scope: &str, enum_seen: &mut Option<String>) {
        match pat {
            HirPattern::Or(a, b) => {
                self.verify_pattern(a, scope, enum_seen);
                self.verify_pattern(b, scope, enum_seen);
            }
            HirPattern::EnumVariant(vname, sub) => {
                if let Some(enum_name) = self.enum_variants.get(vname) {
                    if let Some(prev) = enum_seen {
                        if prev != enum_name {
                            self.push_err(
                                "HIR-T021",
                                scope,
                                format!(
                                    "match mixes variants from enum `{}` and `{}` (`{}`)",
                                    prev, enum_name, vname
                                ),
                            );
                        }
                    } else {
                        *enum_seen = Some(enum_name.clone());
                    }
                } else if !self.enums.is_empty()
                    && !matches!(vname.as_str(), "Some" | "None" | "Ok" | "Err")
                {
                    self.push_err(
                        "HIR-T021",
                        scope,
                        format!("unknown enum variant `{}` in match pattern", vname),
                    );
                }
                for p in sub {
                    self.verify_pattern(p, scope, enum_seen);
                }
            }
            _ => {}
        }
    }

    fn bind_pattern_vars(pat: &HirPattern, env: &mut HashMap<String, Option<HirType>>) {
        match pat {
            HirPattern::Variable(v) => {
                env.insert(v.clone(), None);
            }
            HirPattern::Or(a, b) => {
                Self::bind_pattern_vars(a, env);
                Self::bind_pattern_vars(b, env);
            }
            HirPattern::EnumVariant(_, sub) => {
                for p in sub {
                    Self::bind_pattern_vars(p, env);
                }
            }
            _ => {}
        }
    }

    fn infer_expr_type(
        &self,
        expr: &HirExpr,
        env: &HashMap<String, Option<HirType>>,
    ) -> Option<HirType> {
        match expr {
            HirExpr::Literal(lit) => Some(match lit {
                HirLiteral::Int(_) => HirType::Int,
                HirLiteral::Float(_) => HirType::Float,
                HirLiteral::Bool(_) => HirType::Bool,
                HirLiteral::String(_) => HirType::String,
                HirLiteral::Char(_) => HirType::Char,
                HirLiteral::Null => HirType::Null,
                HirLiteral::BigInt(_) => HirType::I128,
                HirLiteral::U8(_) => HirType::U8,
                HirLiteral::U16(_) => HirType::U16,
                HirLiteral::U32(_) => HirType::U32,
                HirLiteral::U64(_) => HirType::U64,
                HirLiteral::U128(_) => HirType::U128,
                HirLiteral::I8(_) => HirType::I8,
                HirLiteral::I16(_) => HirType::I16,
                HirLiteral::I32(_) => HirType::I32,
                HirLiteral::I64(_) => HirType::I64,
                HirLiteral::I128(_) => HirType::I128,
                HirLiteral::F32(_) => HirType::F32,
                HirLiteral::F64(_) => HirType::F64,
            }),
            HirExpr::LoadVar(name) => env.get(name).and_then(|o| o.clone()),
            HirExpr::Cast(_, target) => Some(target.clone()),
            HirExpr::Call(callee, _, _) => {
                if let HirExpr::LoadVar(name) = callee.as_ref() {
                    self.functions.get(name).and_then(|s| s.ret_type.clone())
                } else {
                    None
                }
            }
            HirExpr::StructLiteral(name, _) => Some(HirType::Class(name.clone())),
            HirExpr::UnaryOp(UnaryOp::Not, _) => Some(HirType::Bool),
            HirExpr::UnaryOp(UnaryOp::Typeof, _) => Some(HirType::String),
            HirExpr::UnaryOp(UnaryOp::Neg, inner) => self.infer_expr_type(inner, env),
            HirExpr::BinaryOp(l, op, r) => match op {
                BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::Eq
                | BinOp::Ne
                | BinOp::StrictEq
                | BinOp::StrictNe
                | BinOp::And
                | BinOp::Or
                | BinOp::In
                | BinOp::Instanceof => Some(HirType::Bool),
                BinOp::Add => {
                    let lt = self.infer_expr_type(l, env);
                    let rt = self.infer_expr_type(r, env);
                    if matches!(lt, Some(HirType::String)) || matches!(rt, Some(HirType::String)) {
                        Some(HirType::String)
                    } else if matches!(lt, Some(HirType::Float | HirType::F64))
                        || matches!(rt, Some(HirType::Float | HirType::F64))
                    {
                        Some(HirType::F64)
                    } else {
                        lt.or(rt)
                    }
                }
                _ => self
                    .infer_expr_type(l, env)
                    .or_else(|| self.infer_expr_type(r, env)),
            },
            HirExpr::Format(_, _) => Some(HirType::String),
            HirExpr::Share(inner) => self
                .infer_expr_type(inner, env)
                .map(|t| HirType::Shared(Box::new(t))),
            HirExpr::Downgrade(inner) => self
                .infer_expr_type(inner, env)
                .map(|t| HirType::Weak(Box::new(t))),
            HirExpr::Borrow(inner, is_mut) => self
                .infer_expr_type(inner, env)
                .map(|t| HirType::Borrow(Box::new(t), *is_mut)),
            HirExpr::BorrowImmut(inner) => self
                .infer_expr_type(inner, env)
                .map(|t| HirType::BorrowImmut(Box::new(t))),
            HirExpr::BorrowMut(inner) => self
                .infer_expr_type(inner, env)
                .map(|t| HirType::BorrowMut(Box::new(t))),
            HirExpr::Deref(inner) => match self.infer_expr_type(inner, env) {
                Some(HirType::Borrow(t, _))
                | Some(HirType::BorrowImmut(t))
                | Some(HirType::BorrowMut(t))
                | Some(HirType::Shared(t)) => Some(*t),
                _ => None,
            },
            HirExpr::Move(inner) => self.infer_expr_type(inner, env),
            HirExpr::Conditional(_, t, _) => self.infer_expr_type(t, env),
            HirExpr::ArrayLiteral(elems) => {
                let elem_ty = elems
                    .first()
                    .and_then(|e| self.infer_expr_type(e, env))
                    .unwrap_or(HirType::Any);
                Some(HirType::Array(
                    Box::new(elem_ty),
                    super::hir::ArrayKind::Dynamic,
                ))
            }
            HirExpr::DictLiteral(_) => Some(HirType::Object),
            HirExpr::SetLiteral(elems) => {
                let elem_ty = elems
                    .first()
                    .and_then(|e| self.infer_expr_type(e, env))
                    .unwrap_or(HirType::Any);
                Some(HirType::Set(Box::new(elem_ty)))
            }
            HirExpr::TupleLiteral(elems) => {
                let mut tys = Vec::with_capacity(elems.len());
                for e in elems {
                    tys.push(self.infer_expr_type(e, env).unwrap_or(HirType::Any));
                }
                Some(HirType::Tuple(tys))
            }
            _ => None,
        }
    }
}

fn is_numeric_like(ty: &HirType) -> bool {
    matches!(
        classify_hir_type(ty),
        HirTypeClass::SignedInt(_)
            | HirTypeClass::UnsignedInt(_)
            | HirTypeClass::Float(_)
            | HirTypeClass::DynamicAny
            | HirTypeClass::Unknown
    )
}

/// Check whether a value of `actual` type is assignable to `expected` type in HIR.
pub fn types_compatible(expected: &HirType, actual: &HirType) -> bool {
    if expected == actual {
        return true;
    }
    if matches!(expected, HirType::Any | HirType::Unknown | HirType::Object)
        || matches!(actual, HirType::Any | HirType::Unknown)
    {
        return true;
    }
    match (classify_hir_type(expected), classify_hir_type(actual)) {
        // Signed/unsigned integers can initialize fixed-width integer slots from unsuffixed Int literals.
        (HirTypeClass::SignedInt(_), HirTypeClass::SignedInt(_))
        | (HirTypeClass::UnsignedInt(_), HirTypeClass::SignedInt(32))
        | (HirTypeClass::UnsignedInt(_), HirTypeClass::UnsignedInt(_)) => true,
        // Floats accept float or integer promotions in gradual AdeshLang typing.
        (HirTypeClass::Float(_), HirTypeClass::Float(_))
        | (HirTypeClass::Float(_), HirTypeClass::SignedInt(_))
        | (HirTypeClass::Float(_), HirTypeClass::UnsignedInt(_)) => true,
        // Nullable / class / instance / string nullability
        (
            HirTypeClass::Composite | HirTypeClass::String | HirTypeClass::NullOrVoid,
            HirTypeClass::NullOrVoid,
        ) => true,
        (
            HirTypeClass::PointerOrBorrow { mutable: exp_mut },
            HirTypeClass::PointerOrBorrow { mutable: act_mut },
        ) => !exp_mut || act_mut,
        _ => match (expected, actual) {
            (HirType::Class(a), HirType::Instance(b))
            | (HirType::Instance(a), HirType::Class(b))
            | (HirType::Class(a), HirType::Class(b))
            | (HirType::Instance(a), HirType::Instance(b)) => a == b,
            (HirType::Array(e1, _), HirType::Array(e2, _)) => types_compatible(e1, e2),
            (HirType::Shared(a), HirType::Shared(b)) => types_compatible(a, b),
            (HirType::Weak(a), HirType::Weak(b)) => types_compatible(a, b),
            (HirType::Tuple(t1), HirType::Tuple(t2)) if t1.len() == t2.len() => t1
                .iter()
                .zip(t2.iter())
                .all(|(a, b)| types_compatible(a, b)),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_fn(
        name: &str,
        params: Vec<(&str, Option<HirType>)>,
        ret: Option<HirType>,
        stmts: Vec<HirStmt>,
    ) -> HirFunction {
        HirFunction {
            name: name.to_string(),
            params: params
                .into_iter()
                .map(|(n, t)| (n.to_string(), t, None))
                .collect(),
            body: Arc::new(stmts),
            ret_type: ret,
            is_async: false,
            decorators: Vec::new(),
            is_exported: false,
            move_params: Vec::new(),
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        }
    }

    #[test]
    fn rejects_cast_to_unknown_and_ownership_types() {
        let mut m = HirModule::new();
        m.functions.push(make_fn(
            "bad_cast",
            vec![("x", Some(HirType::I64))],
            Some(HirType::I64),
            vec![HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("x".into())),
                HirType::Unknown,
            )))],
        ));
        let err = verify_hir_module(&m).expect_err("cast to Unknown must fail");
        assert!(err.iter().any(|e| e.code == "HIR-T018"));

        let mut m2 = HirModule::new();
        m2.functions.push(make_fn(
            "bad_own_cast",
            vec![("x", Some(HirType::I64))],
            Some(HirType::I64),
            vec![HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("x".into())),
                HirType::Shared(Box::new(HirType::I64)),
            )))],
        ));
        let err2 = verify_hir_module(&m2).expect_err("cast to Shared<T> must fail");
        assert!(err2.iter().any(|e| e.code == "HIR-T018"));
    }

    #[test]
    fn rejects_concrete_let_and_return_mismatches() {
        let mut m = HirModule::new();
        m.functions.push(make_fn(
            "mismatch",
            vec![],
            Some(HirType::I64),
            vec![
                HirStmt::Let {
                    name: "s".into(),
                    ty: Some(HirType::I64),
                    init: Some(HirExpr::Literal(HirLiteral::String("oops".into()))),
                    is_const: false,
                    is_borrowed: None,
                    is_atomic: false,
                },
                HirStmt::Return(Some(HirExpr::Literal(HirLiteral::String("bad".into())))),
            ],
        ));
        let errs = verify_hir_module(&m).expect_err("type mismatches must fail");
        assert!(errs.iter().any(|e| e.code == "HIR-T010"));
        assert!(errs.iter().any(|e| e.code == "HIR-T012"));
    }

    #[test]
    fn rejects_extern_unknown_and_struct_field_mismatch() {
        let mut m = HirModule::new();
        m.externs.push(HirExternFunction {
            name: "bad_ext".into(),
            abi: "C".into(),
            params: vec![("p".into(), HirType::Unknown)],
            ret_type: HirType::I32,
            is_variadic: false,
        });
        m.structs.push(HirStruct {
            name: "Pair".into(),
            fields: vec![("a".into(), HirType::I64), ("b".into(), HirType::Bool)],
        });
        m.functions.push(make_fn(
            "make_pair",
            vec![],
            Some(HirType::Class("Pair".into())),
            vec![HirStmt::Return(Some(HirExpr::StructLiteral(
                "Pair".into(),
                vec![
                    (
                        "a".into(),
                        HirExpr::Literal(HirLiteral::String("no".into())),
                    ),
                    ("bogus".into(), HirExpr::Literal(HirLiteral::Int(1))),
                ],
            )))],
        ));
        let errs = verify_hir_module(&m).expect_err("extern unknown + struct mismatch must fail");
        assert!(errs.iter().any(|e| e.code == "HIR-T005"));
        assert!(errs.iter().any(|e| e.code == "HIR-T017"));
    }

    #[test]
    fn strict_mode_rejects_missing_annotations_and_unknown() {
        let mut m = HirModule::new();
        m.functions.push(make_fn(
            "unannotated",
            vec![("x", None)],
            Some(HirType::I64),
            vec![HirStmt::Return(Some(HirExpr::LoadVar("x".into())))],
        ));
        assert!(verify_hir_module(&m).is_ok());
        let strict_errs =
            verify_typed_hir_strict(&m).expect_err("strict mode requires param types");
        assert!(strict_errs.iter().any(|e| e.code == "HIR-T002"));
    }

    #[test]
    fn rejects_invalid_simd_lanes_and_out_of_bounds_move_params() {
        let mut f = make_fn(
            "bad_sig",
            vec![("v", Some(HirType::Simd(Box::new(HirType::F32), 0)))],
            Some(HirType::Null),
            vec![],
        );
        f.move_params = vec![3];
        let mut m = HirModule::new();
        m.functions.push(f);
        let errs = verify_hir_module(&m).expect_err("invalid simd + move_params must fail");
        assert!(errs.iter().any(|e| e.code == "HIR-T011"));
        assert!(errs.iter().any(|e| e.code == "HIR-T009"));
    }
}
