//! Statement execution implementation for the language interpreter.
//!
//! This module handles all statement kinds including variable declarations,
//! control flow, functions, classes, and more.

use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

use crate::parsing::ast::{
    ExprKind, NativeFn, Stmt, StmtKind, UserClass, UserEnum, UserFn, UserStruct, Value,
};
use crate::utils::memory::OwnershipTracker;

use super::core::Exec;
use crate::execution::runtime_core::flow::ExecFlow;
use crate::execution::runtime_core::format::fmt;
use crate::execution::runtime_core::ops::{equals, num};
use crate::execution::runtime_core::{
    ann_matches_value, call_user, call_user_with_this, coerce_to_fixed_width, err,
};

/// Helper: decide if a runtime Value should be treated as Copy by default
#[inline]
pub fn is_copy_value(v: &Value) -> bool {
    match v {
        // Primitive scalars: default Copy
        Value::Null |
        Value::Bool(_) |
        Value::Number(_) |
        Value::Char(_) |
        // Fixed-width numeric types
        Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) |
        Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) | Value::I128(_) |
        Value::F32(_) | Value::F64(_) |
        // Enum constructors (like Some, None) are copyable
        Value::EnumCtor(_, _) => true,
        // Everything else considered move-only by default
        _ => false,
    }
}

impl Exec {
    pub(in crate::execution::runtime_core) fn exec_stmt(
        &mut self,
        s: &Stmt,
    ) -> Result<ExecFlow, String> {
        match &s.kind {
            StmtKind::LetTuple(names, type_anns, init, _is_export, is_const, is_readonly) => {
                let tuple_val = if let Some(e) = init {
                    self.eval_expr(e)?
                } else {
                    Value::Null
                };

                // Extract tuple elements
                let elements = match &tuple_val {
                    Value::Array(arr) => arr.clone(),
                    Value::Tuple(tup) => tup.clone(),
                    Value::DynArray(da) => da.data.clone(),
                    _ => {
                        return Err(err(
                            "Tuple destructuring requires an array/tuple value".to_string()
                        ));
                    }
                };

                for (i, name) in names.iter().enumerate() {
                    if let Some(prev) = self.envs[self.current].consts.get(name) {
                        if *prev {
                            return Err(err(format!("Cannot redeclare const '{}'", name)));
                        }
                    }
                    if self.envs[self.current].values.contains_key(name) {
                        return Err(err(format!("Cannot redeclare variable '{}'", name)));
                    }

                    let mut val = elements.get(i).cloned().unwrap_or(Value::Null);
                    let type_ann = type_anns.as_ref().and_then(|t| t.get(i).cloned());

                    // Apply type coercion if type annotation is provided
                    if let Some(ref ann) = type_ann {
                        match coerce_to_fixed_width(&val, ann) {
                            Ok(Some(converted)) => val = converted,
                            Ok(None) => {}
                            Err(e) => return Err(err(format!("Type error for '{}': {}", name, e))),
                        }
                        if !ann_matches_value(&type_ann, &val) {
                            return Err(err(format!(
                                "annotated variable '{}' expected {}, found {}",
                                name,
                                ann,
                                fmt(&val)
                            )));
                        }
                    }

                    self.envs[self.current].values.insert(name.clone(), val);
                    self.envs[self.current]
                        .consts
                        .insert(name.clone(), *is_const || *is_readonly);
                    self.envs[self.current]
                        .type_ann
                        .insert(name.clone(), type_ann);
                    // Initialize ownership tracker for non-Copy values
                    if let Some(v) = self.envs[self.current].values.get(name) {
                        if is_copy_value(v) {
                            self.envs[self.current].ownership.remove(name);
                        } else {
                            self.envs[self.current]
                                .ownership
                                .insert(name.clone(), Rc::new(OwnershipTracker::new_unique()));
                        }
                    }
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::ShareDeclaration(decl, _is_export) => {
                let val = self.eval_expr(&decl.expr)?;
                let shared_val = Value::Share(crate::memory::arc::allocate_share(val));
                self.envs[self.current]
                    .values
                    .insert(decl.name.clone(), shared_val);
                Ok(ExecFlow::Next)
            }
            StmtKind::StrongDeclaration(decl, _is_export) => {
                let val = self.eval_expr(&decl.expr)?;
                match &val {
                    Value::Share(sr) => {
                        let new_sr = sr.clone();
                        let shared_val = Value::Share(new_sr);
                        self.envs[self.current]
                            .values
                            .insert(decl.name.clone(), shared_val);
                        Ok(ExecFlow::Next)
                    }
                    _ => Err(err(
                        "Cannot create strong reference from non-shared value".to_string()
                    )),
                }
            }
            StmtKind::WeakDeclaration(decl, _is_export) => {
                let val = self.eval_expr(&decl.expr)?;
                match val {
                    Value::Share(sr) => {
                        let weak_val =
                            Value::Weak(crate::memory::arc::create_weak_from_strong(&sr));
                        self.envs[self.current]
                            .values
                            .insert(decl.name.clone(), weak_val);
                        Ok(ExecFlow::Next)
                    }
                    Value::Weak(wr) => {
                        self.envs[self.current]
                            .values
                            .insert(decl.name.clone(), Value::Weak(wr));
                        Ok(ExecFlow::Next)
                    }
                    _ => Err(err(
                        "Cannot create weak reference from non-shared value".to_string()
                    )),
                }
            }
            StmtKind::Let(name, init, type_ann, _is_export, is_const, is_readonly) => {
                if let Some(prev) = self.envs[self.current].consts.get(name) {
                    if *prev {
                        return Err(err(format!("Cannot redeclare const '{}'", name)));
                    }
                }
                if self.envs[self.current].values.contains_key(name) {
                    return Err(err(format!("Cannot redeclare variable '{}'", name)));
                }
                let mut val = if let Some(e) = init {
                    // Handle move semantics when initializing from a variable
                    if let ExprKind::Variable(src) = &e.kind {
                        let src_val = self.eval_expr(e)?;
                        let is_copy = is_copy_value(&src_val);
                        if !is_copy {
                            // Mark source as moved if it has a tracker
                            if let Some(tr) = self.get_tracker(src) {
                                tr.mark_moved();
                            }
                        }
                        src_val
                    } else {
                        self.eval_expr(e)?
                    }
                } else if let Some(ann) = type_ann.as_ref() {
                    let lowered = ann.trim().to_lowercase();
                    if lowered == "set" || lowered.starts_with("set<") {
                        Value::Set(Vec::new())
                    } else if lowered == "dict"
                        || lowered == "map"
                        || lowered == "object"
                        || lowered == "{}"
                        || lowered.starts_with("dict<")
                        || lowered.starts_with("map<")
                    {
                        Value::Object(std::sync::Arc::new(HashMap::default()))
                    } else if lowered == "array"
                        || lowered == "list"
                        || lowered == "[]"
                        || lowered.starts_with("array<")
                        || lowered.starts_with("list<")
                        || (ann.starts_with('[') && ann.ends_with(']'))
                        || ann.ends_with("[]")
                    {
                        Value::Array(Vec::new())
                    } else if lowered == "tuple" || (ann.starts_with('(') && ann.ends_with(')')) {
                        Value::Tuple(Vec::new())
                    } else {
                        Value::Null
                    }
                } else {
                    Value::Null
                };

                // Apply container coercion if initialized with empty object/array
                if let Some(ann) = type_ann.as_ref() {
                    let lowered = ann.trim().to_lowercase();
                    if lowered == "set" || lowered.starts_with("set<") {
                        match &val {
                            Value::Object(m) if m.is_empty() => {
                                val = Value::Set(Vec::new());
                            }
                            Value::Array(arr) => {
                                let mut unique = Vec::new();
                                for item in arr {
                                    if !unique.iter().any(|x| equals(x, item)) {
                                        unique.push(item.clone());
                                    }
                                }
                                val = Value::Set(unique);
                            }
                            Value::DynArray(da) => {
                                let mut unique = Vec::new();
                                for item in &da.data {
                                    if !unique.iter().any(|x| equals(x, item)) {
                                        unique.push(item.clone());
                                    }
                                }
                                val = Value::Set(unique);
                            }
                            _ => {}
                        }
                    } else if (lowered == "dict"
                        || lowered == "map"
                        || lowered.starts_with("dict<")
                        || lowered.starts_with("map<"))
                        && matches!(&val, Value::Array(a) if a.is_empty())
                    {
                        val = Value::Object(std::sync::Arc::new(HashMap::default()));
                    }
                }

                // Apply type coercion for single-variable let when annotation is present
                if let Some(ann) = type_ann.clone() {
                    match coerce_to_fixed_width(&val, ann.as_str()) {
                        Ok(Some(converted)) => val = converted,
                        Ok(None) => {}
                        Err(e) => return Err(err(format!("Type error for '{}': {}", name, e))),
                    }
                }
                if type_ann.is_some() && !ann_matches_value(type_ann, &val) {
                    return Err(err(format!(
                        "annotated variable '{}' expected {}, found {}",
                        name,
                        type_ann.as_ref().unwrap(),
                        fmt(&val)
                    )));
                }
                self.envs[self.current].values.insert(name.clone(), val);
                self.envs[self.current]
                    .consts
                    .insert(name.clone(), *is_const || *is_readonly);
                self.envs[self.current]
                    .type_ann
                    .insert(name.clone(), type_ann.clone());
                // Initialize ownership tracker for non-Copy values
                if let Some(v) = self.envs[self.current].values.get(name) {
                    if is_copy_value(v) {
                        self.envs[self.current].ownership.remove(name);
                    } else {
                        self.envs[self.current]
                            .ownership
                            .insert(name.clone(), Rc::new(OwnershipTracker::new_unique()));
                    }
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::Struct(s, is_export) => {
                // Create a struct value from the StructDecl
                let mut fields_map = HashMap::default();
                for (fname, ftype) in &s.fields {
                    fields_map.insert(fname.clone(), ftype.clone());
                }
                let us = UserStruct {
                    name: s.name.clone(),
                    fields: s.fields.clone(),
                    methods: HashMap::default(),
                    fields_map,
                };
                let v = Value::Struct(us);
                self.envs[self.current]
                    .values
                    .insert(s.name.clone(), v.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(s.name.clone(), None);
                if *is_export {
                    self.envs[self.current].exports.insert(s.name.clone(), v);
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::Enum(e, is_export) => {
                let mut variants_map = HashMap::default();
                for (vname, payload) in &e.variants {
                    variants_map.insert(vname.clone(), payload.clone());
                }
                let ue = UserEnum {
                    name: e.name.clone(),
                    variants: e.variants.clone(),
                    methods: HashMap::default(),
                    variants_map,
                };
                let v = Value::Enum(ue);
                self.envs[self.current]
                    .values
                    .insert(e.name.clone(), v.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(e.name.clone(), None);
                if *is_export {
                    self.envs[self.current].exports.insert(e.name.clone(), v);
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::ExprStmt(e) => {
                self.eval_expr(e)?;
                Ok(ExecFlow::Next)
            }
            StmtKind::Block(b) => {
                // Rust-like scope management: acquire, execute, release (deterministic drop)
                let parent = self.current;
                let env = self.acquire_scope(Some(parent));
                let prev = self.current;
                self.current = env;
                let block_res = self.exec_block(b);
                self.current = prev;
                // Release scope immediately when block ends (like Rust's RAII)
                self.release_scope(env);
                block_res
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cv = self.eval_expr(cond)?;
                if cv.truthy() {
                    self.exec_stmt(then_branch)
                } else if let Some(el) = else_branch {
                    self.exec_stmt(el)
                } else {
                    Ok(ExecFlow::Next)
                }
            }
            StmtKind::Function(f, is_export) => {
                // For closures, we need to capture the environment at definition time.
                // However, eagerly cloning all values is expensive.
                // Optimization: Only capture if we're nested and need closure semantics.
                // The closure index allows lazy lookup at call time instead of eager capture.

                let fun = Value::UserFunction(UserFn {
                    name: f.name.clone(),
                    type_params: f.type_params.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: self.current,
                    visibility: f.visibility.clone(),
                    ret_type: f.ret_type.clone(),
                    is_async: f.is_async,
                    is_constructor: false,
                    is_static: false,
                    is_abstract: false,
                    is_getter: false,
                    is_setter: false,
                    is_operator: false,
                    operator_symbol: None,
                    captured: None, // Rely on closure index for lookup instead of eager capture
                    defining_class: None,
                    is_unsafe: f.is_unsafe,
                });
                self.envs[self.current]
                    .values
                    .insert(f.name.clone(), fun.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(f.name.clone(), None);
                if *is_export {
                    self.envs[self.current].exports.insert(f.name.clone(), fun);
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::ExportDefaultFunction(f) => {
                let fun = Value::UserFunction(UserFn {
                    name: f.name.clone(),
                    type_params: f.type_params.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: self.current,
                    visibility: f.visibility.clone(),
                    ret_type: f.ret_type.clone(),
                    is_async: f.is_async,
                    is_constructor: false,
                    is_static: false,
                    is_abstract: false,
                    is_getter: false,
                    is_setter: false,
                    is_operator: false,
                    operator_symbol: None,
                    captured: None,
                    defining_class: None,
                    is_unsafe: f.is_unsafe,
                });
                self.envs[self.current]
                    .values
                    .insert(f.name.clone(), fun.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(f.name.clone(), None);
                self.envs[self.current]
                    .exports
                    .insert("default".into(), fun);
                Ok(ExecFlow::Next)
            }
            StmtKind::Class(c, is_export) => {
                let mut methods = HashMap::default();
                let mut static_methods = HashMap::default();
                let mut static_properties = HashMap::default();
                let mut operators = HashMap::default();
                let mut getters = HashMap::default();
                let mut setters = HashMap::default();
                for m in &c.methods {
                    let user_fn = UserFn {
                        type_params: m.type_params.clone(),
                        name: m.name.clone(),
                        params: m.params.clone(),
                        body: m.body.clone(),
                        closure: self.current,
                        visibility: m.visibility.clone(),
                        ret_type: m.ret_type.clone(),
                        is_async: m.is_async,
                        is_static: m.is_static,
                        is_abstract: m.is_abstract,
                        is_constructor: m.is_constructor,
                        is_getter: m.is_getter,
                        is_setter: m.is_setter,
                        is_operator: m.is_operator,
                        operator_symbol: m.operator_symbol.clone(),
                        captured: None,
                        defining_class: Some(c.name.clone()),
                        is_unsafe: m.is_unsafe,
                    };
                    if m.is_operator {
                        if let Some(op_symbol) = &m.operator_symbol {
                            operators.insert(op_symbol.clone(), user_fn);
                        }
                    } else if m.is_getter {
                        getters.insert(m.name.clone(), user_fn);
                    } else if m.is_setter {
                        setters.insert(m.name.clone(), user_fn);
                    } else {
                        methods
                            .entry(m.name.clone())
                            .or_insert_with(Vec::new)
                            .push(user_fn);
                    }
                }
                for m in &c.static_methods {
                    let user_fn = UserFn {
                        type_params: m.type_params.clone(),
                        name: m.name.clone(),
                        params: m.params.clone(),
                        body: m.body.clone(),
                        closure: self.current,
                        visibility: m.visibility.clone(),
                        ret_type: m.ret_type.clone(),
                        is_async: m.is_async,
                        is_static: true,
                        is_abstract: m.is_abstract,
                        is_constructor: m.is_constructor,
                        is_getter: m.is_getter,
                        is_setter: m.is_setter,
                        is_operator: m.is_operator,
                        operator_symbol: m.operator_symbol.clone(),
                        captured: None,
                        defining_class: Some(c.name.clone()),
                        is_unsafe: m.is_unsafe,
                    };
                    static_methods
                        .entry(m.name.clone())
                        .or_insert_with(Vec::new)
                        .push(user_fn);
                }
                for (name, expr, pdecos) in &c.static_properties {
                    let mut value = self.eval_expr(expr)?;
                    if !pdecos.is_empty() {
                        let mut meta: HashMap<String, Value> = HashMap::default();
                        meta.insert("name".into(), Value::Str(name.clone()));
                        meta.insert("type".into(), Value::Str("property".into()));
                        meta.insert("className".into(), Value::Str(c.name.clone()));
                        meta.insert("isStatic".into(), Value::Bool(true));
                        let meta_obj = Value::Object(std::sync::Arc::new(meta));
                        for d in pdecos.iter().rev() {
                            let dv = self.eval_expr(d)?;
                            let nv = match dv.clone() {
                                Value::Function(NativeFn(fwrap)) => {
                                    fwrap(self, vec![value.clone(), meta_obj.clone()])?
                                }
                                Value::UserFunction(uf) => call_user(
                                    uf.clone(),
                                    vec![value.clone(), meta_obj.clone()],
                                    Value::Null,
                                    None,
                                    self.native_side_effects.clone(),
                                )?,
                                Value::BoundMethod(uf, inst) => {
                                    let (val, _i) = call_user_with_this(
                                        uf.clone(),
                                        vec![value.clone(), meta_obj.clone()],
                                        (*inst.as_ref()).clone(),
                                        None,
                                        self.native_side_effects.clone(),
                                    )?;
                                    val
                                }
                                other => other,
                            };
                            value = nv;
                        }
                    }
                    static_properties.insert(name.clone(), value);
                }
                let parent_cls: Option<Box<UserClass>> = if let Some(ext) = &c.extends {
                    match self.get(ext.as_str()) {
                        Some(Value::Class(pc)) => Some(Box::new(pc.clone())),
                        Some(_) => return Err(err(format!("{} is not a class", ext))),
                        None => return Err(err(format!("Unknown superclass '{}'", ext))),
                    }
                } else {
                    None
                };
                if let Some(pc) = &parent_cls {
                    for (k, v) in pc.methods.iter() {
                        if !methods.contains_key(k) {
                            methods.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.static_methods.iter() {
                        if !static_methods.contains_key(k) {
                            static_methods.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.static_properties.iter() {
                        if !static_properties.contains_key(k) {
                            static_properties.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.operators.iter() {
                        if !operators.contains_key(k) {
                            operators.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.getters.iter() {
                        if !getters.contains_key(k) {
                            getters.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.setters.iter() {
                        if !setters.contains_key(k) {
                            setters.insert(k.clone(), v.clone());
                        }
                    }
                }
                for ifn in &c.implements {
                    match self.get(ifn.as_str()) {
                        Some(Value::Interface(ui)) => {
                            for m in &ui.methods {
                                if !methods.contains_key(&m.name) {
                                    return Err(err(format!(
                                        "Class '{}' does not implement method '{}' required by interface '{}'",
                                        c.name, m.name, ifn
                                    )));
                                }
                                let method_overloads = methods.get(&m.name).unwrap();
                                let matches = method_overloads
                                    .iter()
                                    .any(|cm| cm.params.len() == m.params.len());
                                if !matches {
                                    return Err(err(format!(
                                        "Method '{}' on class '{}' does not match arity for interface '{}'",
                                        m.name, c.name, ifn
                                    )));
                                }
                            }
                        }
                        Some(_) => return Err(err(format!("'{}' is not an interface", ifn))),
                        None => return Err(err(format!("Unknown interface '{}'", ifn))),
                    }
                }
                let mut field_visibility = HashMap::default();
                let mut field_owner = HashMap::default();
                let mut field_types = HashMap::default();
                let mut field_initializers = HashMap::default();
                for (fname, fty, fvis, _fdecos, init_expr) in &c.fields {
                    field_visibility.insert(fname.clone(), fvis.clone());
                    field_owner.insert(fname.clone(), c.name.clone());
                    field_types.insert(fname.clone(), fty.clone());
                    if let Some(expr) = init_expr {
                        field_initializers.insert(fname.clone(), expr.clone());
                    }
                }
                if let Some(pc) = &parent_cls {
                    for (k, v) in pc.field_visibility.iter() {
                        if !field_visibility.contains_key(k) {
                            field_visibility.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_owner.iter() {
                        if !field_owner.contains_key(k) {
                            field_owner.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_types.iter() {
                        if !field_types.contains_key(k) {
                            field_types.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_initializers.iter() {
                        if !field_initializers.contains_key(k) {
                            field_initializers.insert(k.clone(), v.clone());
                        }
                    }
                }

                let class = Value::Class(UserClass {
                    name: c.name.clone(),
                    methods,
                    static_methods,
                    static_properties,
                    operators,
                    getters,
                    setters,
                    parent: parent_cls,
                    implements: c.implements.clone(),
                    is_abstract: c.is_abstract,
                    is_sealed: c.is_sealed,
                    field_visibility,
                    field_owner,
                    field_types,
                    field_initializers,
                });
                self.envs[self.current]
                    .values
                    .insert(c.name.clone(), class.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(c.name.clone(), None);
                if *is_export {
                    self.envs[self.current]
                        .exports
                        .insert(c.name.clone(), class);
                }
                Ok(ExecFlow::Next)
            }
            StmtKind::ExportDefaultClass(c) => {
                let mut methods = HashMap::default();
                for m in &c.methods {
                    let user_fn = UserFn {
                        type_params: m.type_params.clone(),
                        name: m.name.clone(),
                        params: m.params.clone(),
                        body: m.body.clone(),
                        closure: self.current,
                        visibility: m.visibility.clone(),
                        ret_type: m.ret_type.clone(),
                        is_async: m.is_async,
                        is_constructor: false,
                        is_static: false,
                        is_abstract: false,
                        is_getter: false,
                        is_setter: false,
                        is_operator: false,
                        operator_symbol: None,
                        captured: None,
                        defining_class: Some(c.name.clone()),
                        is_unsafe: m.is_unsafe,
                    };
                    methods.insert(m.name.clone(), vec![user_fn]);
                }
                let parent_cls: Option<Box<UserClass>> = if let Some(ext) = &c.extends {
                    match self.get(ext.as_str()) {
                        Some(Value::Class(pc)) => Some(Box::new(pc.clone())),
                        Some(_) => return Err(err(format!("{} is not a class", ext))),
                        None => return Err(err(format!("Unknown superclass '{}'", ext))),
                    }
                } else {
                    None
                };
                let mut static_methods = HashMap::default();
                let mut static_properties = HashMap::default();
                let mut operators = HashMap::default();
                let mut getters = HashMap::default();
                let mut setters = HashMap::default();
                if let Some(pc) = &parent_cls {
                    for (k, v) in pc.methods.iter() {
                        if !methods.contains_key(k) {
                            methods.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.static_methods.iter() {
                        static_methods.insert(k.clone(), v.clone());
                    }
                    for (k, v) in pc.static_properties.iter() {
                        static_properties.insert(k.clone(), v.clone());
                    }
                    for (k, v) in pc.operators.iter() {
                        operators.insert(k.clone(), v.clone());
                    }
                    for (k, v) in pc.getters.iter() {
                        getters.insert(k.clone(), v.clone());
                    }
                    for (k, v) in pc.setters.iter() {
                        setters.insert(k.clone(), v.clone());
                    }
                }
                let mut field_visibility = HashMap::default();
                let mut field_owner = HashMap::default();
                let mut field_types = HashMap::default();
                let mut field_initializers = HashMap::default();
                for (fname, fty, fvis, _fdecos, init_expr) in &c.fields {
                    field_visibility.insert(fname.clone(), fvis.clone());
                    field_owner.insert(fname.clone(), c.name.clone());
                    field_types.insert(fname.clone(), fty.clone());
                    if let Some(expr) = init_expr {
                        field_initializers.insert(fname.clone(), expr.clone());
                    }
                }
                if let Some(pc) = &parent_cls {
                    for (k, v) in pc.field_visibility.iter() {
                        if !field_visibility.contains_key(k) {
                            field_visibility.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_owner.iter() {
                        if !field_owner.contains_key(k) {
                            field_owner.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_types.iter() {
                        if !field_types.contains_key(k) {
                            field_types.insert(k.clone(), v.clone());
                        }
                    }
                    for (k, v) in pc.field_initializers.iter() {
                        if !field_initializers.contains_key(k) {
                            field_initializers.insert(k.clone(), v.clone());
                        }
                    }
                }

                let class = Value::Class(UserClass {
                    name: c.name.clone(),
                    methods,
                    parent: parent_cls,
                    implements: c.implements.clone(),
                    is_abstract: c.is_abstract,
                    is_sealed: c.is_sealed,
                    field_visibility,
                    field_owner,
                    field_types,
                    field_initializers,
                    static_methods,
                    static_properties,
                    operators,
                    getters,
                    setters,
                });
                self.envs[self.current]
                    .values
                    .insert(c.name.clone(), class.clone());
                self.envs[self.current]
                    .type_ann
                    .insert(c.name.clone(), None);
                self.envs[self.current]
                    .exports
                    .insert("default".into(), class);
                Ok(ExecFlow::Next)
            }
            StmtKind::Return(v) => {
                let val = if let Some(e) = v {
                    self.eval_expr(e)?
                } else {
                    Value::Null
                };
                Ok(ExecFlow::Return(Some(val)))
            }
            StmtKind::Break => Ok(ExecFlow::Break),
            StmtKind::Continue => Ok(ExecFlow::Continue),
            StmtKind::Jump(expr) => {
                let v = self.eval_expr(expr)?;
                let n = num(v)?;
                Ok(ExecFlow::Jump(n))
            }
            StmtKind::While { cond, body } => {
                let loop_env = self.current;
                loop {
                    self.current = loop_env;
                    if !self.eval_expr(cond)?.truthy() {
                        break;
                    }
                    self.current = loop_env;
                    let exec_res = self.exec_stmt(body)?;
                    match exec_res {
                        ExecFlow::Next => {}
                        ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                        ExecFlow::Break => break,
                        ExecFlow::Continue => continue,
                        ExecFlow::Jump(_n) => continue,
                    }
                }
                self.current = loop_env;
                Ok(ExecFlow::Next)
            }
            StmtKind::ForIn { name, iter, body } => {
                let it = self.eval_expr(iter)?;
                if let Value::Array(xs) = it {
                    for v in xs {
                        self.envs[self.current].values.insert(name.clone(), v);
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::RawArray(_, xs) = it {
                    for v in xs {
                        self.envs[self.current].values.insert(name.clone(), v);
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::DynArray(da) = it {
                    for v in da.data {
                        self.envs[self.current].values.insert(name.clone(), v);
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::Str(s) = it {
                    for ch in s.chars() {
                        self.envs[self.current]
                            .values
                            .insert(name.clone(), Value::Char(ch));
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::Set(st) = it {
                    for v in st {
                        self.envs[self.current].values.insert(name.clone(), v);
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::Tuple(ts) = it {
                    for v in ts {
                        self.envs[self.current].values.insert(name.clone(), v);
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::Object(map) = it {
                    for k in map.keys() {
                        self.envs[self.current]
                            .values
                            .insert(name.clone(), Value::Str(k.clone()));
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                    }
                    Ok(ExecFlow::Next)
                } else if let Value::LazyRange(start, end, step) = it {
                    let mut cur = start;
                    while if step > 0.0 { cur < end } else { cur > end } {
                        self.envs[self.current]
                            .values
                            .insert(name.clone(), Value::Number(cur));
                        match self.exec_stmt(body)? {
                            ExecFlow::Next => {}
                            ExecFlow::Return(v) => return Ok(ExecFlow::Return(v)),
                            ExecFlow::Break => break,
                            ExecFlow::Continue => continue,
                            ExecFlow::Jump(_n) => continue,
                        }
                        cur += step;
                    }
                    Ok(ExecFlow::Next)
                } else {
                    Err(err(
                        "for-in/of requires array, string, set, tuple, object, or range",
                    ))
                }
            }
            StmtKind::Defer(block) => {
                // Register defer block in current scope's defer stack
                self.envs[self.current].defers.push(block.clone());
                Ok(ExecFlow::Next)
            }
            StmtKind::UnsafeBlock(body) => {
                let prev = crate::execution::runtime_core::UNSAFE_DEPTH.with(|c| {
                    let v = c.get();
                    c.set(v.saturating_add(1));
                    v
                });
                let res = self.exec_stmt(body);
                crate::execution::runtime_core::UNSAFE_DEPTH.with(|c| c.set(prev));
                res
            }
            StmtKind::Region { body, .. } => self.exec_stmt(body),
            _ => Ok(ExecFlow::Next),
        }
    }
}
