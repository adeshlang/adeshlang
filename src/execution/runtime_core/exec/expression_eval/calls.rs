//! Function and method call evaluation, including constructor (New) calls

use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::Ordering;

use super::super::core::Exec;
use super::super::stmt::is_copy_value;
use crate::execution::runtime_core::interpreter::CONSTRUCTOR_SLOT;
use crate::execution::runtime_core::interpreter::construction_helpers::err_with_span;
use crate::execution::runtime_core::{
    PROMISE_COUNTER, call_user_with_this_value, err, get_prop, new_instance, ops::equals,
    select_best_overload,
};
use crate::parsing::ast::{
    BuiltinEnv, Expr, ExprKind, NativeEffect, NativeFn, UserInstance, Value,
};
use num_traits::ToPrimitive;

impl Exec {
    pub(super) fn eval_new(&mut self, ctor: &Expr, args: &[Expr]) -> Result<Value, String> {
        let c = self.eval_expr(ctor)?;
        if let Value::Class(cls) = c {
            if cls.is_abstract {
                return Err(err(format!(
                    "Cannot instantiate abstract class '{}'",
                    cls.name
                )));
            }
            let mut inst = UserInstance {
                class_name: cls.name.clone(),
                fields: std::sync::Arc::new(std::sync::RwLock::new(HashMap::default())),
                class: std::sync::Arc::new(cls),
                prop_cache: std::sync::Arc::new(std::sync::RwLock::new(HashMap::default())),
                layout: None,
                raw: None,
            };
            // evaluate field initializers before constructor
            let field_inits: Vec<(String, Expr)> = inst
                .class
                .field_initializers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (fname, init_expr) in field_inits {
                let val = self.eval_expr(&init_expr)?;
                let _ = inst.set_field(&fname, val);
            }
            // evaluate arguments once
            let mut a = Vec::new();
            for e in args {
                a.push(self.eval_expr(e)?);
            }
            // run constructor if present
            if let Some(ctors) = inst.class.methods.get(CONSTRUCTOR_SLOT) {
                if let Some(sel) = ctors
                    .iter()
                    .find(|c| c.matches_signature(a.len()))
                    .or_else(|| ctors.first())
                {
                    let selected = if ctors.len() > 1 {
                        select_best_overload(ctors, &a).unwrap_or(sel.clone())
                    } else {
                        sel.clone()
                    };
                    let (_ret, updated_val) = self._call_user_fn_with_this(
                        &selected,
                        a.clone(),
                        Value::Instance(inst.clone()),
                    )?;
                    if let Value::Instance(up_inst) = updated_val {
                        inst = up_inst;
                    }
                }
            }
            {
                if !a.is_empty() && inst.get_field("name").is_none() {
                    let _ = inst.set_field("name", a[0].clone());
                }
            }
            Ok(Value::Instance(inst))
        } else if let Value::Struct(s) = c {
            let mut obj = HashMap::default();
            for (i, (fname, ftype)) in s.fields.iter().enumerate() {
                let mut val = if let Some(a) = args.get(i) {
                    self.eval_expr(a)?
                } else {
                    Value::Null
                };

                if !ftype.is_empty() {
                    let ann = Some(ftype.clone());
                    match crate::execution::runtime_core::interpreter::coerce_to_fixed_width(
                        &val, ftype,
                    ) {
                        Ok(Some(converted)) => val = converted,
                        Ok(None) => {}
                        Err(e) => return Err(format!("Type error for field '{}': {}", fname, e)),
                    }
                    if !crate::execution::runtime_core::interpreter::type_checking::ann_matches_value(&ann, &val) {
                        return Err(err(&format!("struct field '{}' expected {}, found {}", fname, ftype, crate::execution::format::fmt(&val))));
                    }
                }

                obj.insert(fname.clone(), val);
            }
            obj.insert("__struct".to_string(), Value::Str(s.name.clone()));
            Ok(Value::Object(obj.into()))
        } else if let Value::EnumCtor(ue, variant) = c {
            let mut payload: Option<Value> = None;
            if args.len() == 1 {
                payload = Some(self.eval_expr(&args[0])?);
            } else if args.len() > 1 {
                let mut arr = Vec::new();
                for a in args {
                    arr.push(self.eval_expr(a)?);
                }
                payload = Some(Value::Array(arr));
            }
            // Use default() for FxHashMap - it's already optimized
            let mut m = HashMap::default();
            m.insert("__enum".into(), Value::Str(ue.name.clone()));
            m.insert("tag".into(), Value::Str(variant.clone()));
            if let Some(p) = payload {
                m.insert("value".into(), p);
            }
            Ok(Value::Object(m.into()))
        } else if let Value::Object(default_map) = c {
            let mut new_map = (*default_map).clone();
            if let Some(arg) = args.first() {
                let arg_val = self.eval_expr(arg)?;
                if let Value::Object(override_map) = arg_val {
                    for (k, v) in override_map.iter() {
                        new_map.insert(k.clone(), v.clone());
                    }
                }
            }
            Ok(Value::Object(new_map.into()))
        } else {
            Err(err("'new' needs a class or struct or enum variant"))
        }
    }

    pub(super) fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value, String> {
        // Check if this is a method call (object.method(args))
        if let ExprKind::Get(obj, method_name) = &callee.kind {
            if method_name == "mock" {
                let mut av = Vec::with_capacity(args.len());
                for a in args {
                    av.push(self.eval_expr(a)?);
                }
                use crate::execution::interpreter_impl::utilities::value_to_string;
                let strings: Vec<String> = if av.is_empty() {
                    Vec::new()
                } else {
                    match &av[0] {
                        Value::Array(a) => a.iter().map(value_to_string).collect(),
                        Value::DynArray(da) => da.data.iter().map(value_to_string).collect(),
                        other => vec![value_to_string(other)],
                    }
                };
                crate::backends::common::builtins_modules::io::set_thread_mock_input(
                    strings.clone(),
                );
                let mut q = crate::execution::runtime_core::INPUT_PLAYBACK
                    .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()))
                    .lock()
                    .unwrap();
                q.clear();
                for s in strings {
                    q.push_back(s);
                }
                drop(q);

                if matches!(&obj.kind, ExprKind::Variable(v) if v == "input" || v == "Input") {
                    return Ok(Value::Null);
                }
                return self.eval_expr(obj);
            }

            if crate::memory::arc::is_arc_method(method_name) {
                let mut av = Vec::new();
                for a in args {
                    match &a.kind {
                        ExprKind::Spread(inner) => {
                            let spread_val = self.eval_expr(inner)?;
                            match spread_val {
                                Value::Array(arr) => av.extend(arr),
                                Value::DynArray(da) => av.extend(da.data),
                                Value::RawArray(_, raw) => av.extend(raw),
                                Value::Tuple(tup) => av.extend(tup),
                                _ => {
                                    return Err(
                                        "Spread operator in function calls can only be used on arrays or tuples"
                                            .to_string(),
                                    )
                                }
                            }
                        }
                        _ => av.push(self.eval_expr(a)?),
                    }
                }
                if let Some(result) =
                    crate::execution::runtime_core::arc_dispatch::try_invoke_arc_method_on_receiver_expr(
                        &self.envs,
                        self.current,
                        obj,
                        method_name,
                    )
                {
                    return result;
                }
                let obj_val = self.eval_expr(obj)?;
                if matches!(&obj_val, Value::Share(_) | Value::Weak(_)) {
                    return crate::memory::arc::invoke_arc_method(&obj_val, method_name);
                }
            }

            let mut obj_val = self.eval_expr(obj)?;
            if let Value::Ref(inner, _handle) = obj_val {
                obj_val = (*inner).clone();
            }
            // CRITICAL FIX: First try to get user-defined methods via get_prop
            let method_val = get_prop(
                obj_val.clone(),
                method_name,
                self.current_class_context.as_deref(),
            )?;
            // If it's a bound method, call it
            if let Value::BoundMethod(func, inst) = method_val {
                let mut av = Vec::new();
                for a in args {
                    match &a.kind {
                        ExprKind::Spread(inner) => {
                            let spread_val = self.eval_expr(inner)?;
                            match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                        }
                        _ => av.push(self.eval_expr(a)?),
                    }
                }
                // Use type-based overload resolution if multiple overloads exist
                let selected = if let Some(all) = (*inst).class.methods.get(&func.name) {
                    if all.len() > 1 {
                        select_best_overload(all, &av)?
                    } else {
                        all.iter()
                            .find(|f| f.matches_signature(av.len()))
                            .cloned()
                            .unwrap_or(func)
                    }
                } else {
                    func
                };
                // Call the bound method
                let (result, _updated) =
                    self._call_user_fn_with_this(&selected, av, Value::Instance((*inst).clone()))?;
                return Ok(result);
            }
            // If it's an enum constructor (Enum.Variant), construct the variant instance
            if let Value::EnumCtor(ue, variant) = method_val {
                let mut av = Vec::new();
                for a in args {
                    match &a.kind {
                        ExprKind::Spread(inner) => {
                            let spread_val = self.eval_expr(inner)?;
                            match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                        }
                        _ => av.push(self.eval_expr(a)?),
                    }
                }
                let mut payload = None;
                if av.len() == 1 {
                    payload = Some(av[0].clone());
                } else if av.len() > 1 {
                    payload = Some(Value::Array(av));
                }
                let mut m = HashMap::default();
                m.insert("__enum".into(), Value::Str(ue.name.clone()));
                m.insert("tag".into(), Value::Str(variant.clone()));
                if let Some(p) = payload {
                    m.insert("value".into(), p);
                }
                return Ok(Value::Object(m.into()));
            }
            // If it's an Object with __enum (enum instance), look up extended methods from method cache
            if let Value::Object(m) = &obj_val {
                if let Some(Value::Str(enum_name)) = m.get("__enum") {
                    let enum_name_interned = crate::utils::interner::intern(enum_name);
                    let method_name_interned = crate::utils::interner::intern(method_name);
                    if let Some(ref cache) = self.method_cache {
                        if let Some(uf) = cache
                            .get(&(enum_name_interned, method_name_interned))
                            .cloned()
                        {
                            let mut av = Vec::new();
                            for a in args {
                                match &a.kind {
                                    ExprKind::Spread(inner) => {
                                        let spread_val = self.eval_expr(inner)?;
                                        match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                                    }
                                    _ => av.push(self.eval_expr(a)?),
                                }
                            }
                            let self_val = Value::Object(m.clone());
                            let (res, _) = call_user_with_this_value(
                                uf.clone(),
                                av,
                                self_val,
                                uf.captured.clone(),
                                self.native_side_effects.clone(),
                                self.method_cache.clone(),
                            )?;
                            return Ok(res);
                        }
                    }
                }
            }
            // Static method call: Class.staticMethod(args)
            if let Value::UserFunction(uf) = &method_val {
                let mut av = Vec::new();
                for a in args {
                    match &a.kind {
                        ExprKind::Spread(inner) => {
                            let spread_val = self.eval_expr(inner)?;
                            match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                        }
                        _ => av.push(self.eval_expr(a)?),
                    }
                }
                return self._call_user_fn(uf, av);
            }
            // If not a bound method, prepare args and try builtin methods
            let mut av = Vec::new();
            for a in args {
                match &a.kind {
                    ExprKind::Spread(inner) => {
                        let spread_val = self.eval_expr(inner)?;
                        match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                    }
                    _ => av.push(self.eval_expr(a)?),
                }
            }
            // Native methods on stdlib objects (thread.sleep, Mutex.with, Condvar.notify_one, ...)
            if let Value::Function(NativeFn(f)) = method_val {
                return (f)(self, av);
            }
            // Dispatch to builtin method
            return self.call_builtin_method(&obj_val, method_name, &av);
        }
        let cal = self.eval_expr(callee)?;
        let mut av = Vec::new();
        for a in args {
            match &a.kind {
                ExprKind::Spread(inner) => {
                    let spread_val = self.eval_expr(inner)?;
                    match spread_val { Value::Array(arr) => av.extend(arr), Value::DynArray(da) => av.extend(da.data), Value::RawArray(_, raw) => av.extend(raw), Value::Tuple(tup) => av.extend(tup), _ => return Err("Spread operator in function calls can only be used on arrays or tuples".to_string()) }
                }
                ExprKind::Variable(src) => {
                    let src_val = self
                        .get_fast(src)
                        .ok_or_else(|| err_with_span(format!("Undefined '{}'", src), &a.span))?;
                    let is_copy = is_copy_value(&src_val);
                    if !is_copy {
                        if let Some(tr) = self.get_tracker(src) {
                            tr.mark_moved();
                        }
                    }
                    av.push(src_val.clone());
                }
                _ => av.push(self.eval_expr(a)?),
            }
        }
        if let Value::Super(parent_box, inst_box) = cal.clone() {
            let target = parent_box
                .methods
                .get(CONSTRUCTOR_SLOT)
                .and_then(|fns| fns.first().cloned());
            if let Some(u) = target {
                let (_ret, updated_val) =
                    self._call_user_fn_with_this(&u, av, Value::Instance((*inst_box).clone()))?;
                if let Value::Instance(up_inst) = updated_val {
                    self.envs[self.current]
                        .values
                        .insert("this".into(), Value::Instance(up_inst.clone()));
                    self.envs[self.current]
                        .values
                        .insert("self".into(), Value::Instance(up_inst));
                }
                return Ok(Value::Null);
            }
            return Ok(Value::Null);
        }
        match cal {
            Value::Function(NativeFn(f)) => (f)(self, av),
            Value::UserFunction(u) => self._call_user_fn(&u, av),
            Value::BoundMethod(u, inst) => {
                // Use type-based overload resolution if multiple overloads exist
                let selected = if let Some(all) = (*inst).class.methods.get(&u.name) {
                    if all.len() > 1 {
                        // Multiple overloads: use type-based selection
                        select_best_overload(all, &av)?
                    } else {
                        // Single overload or fallback to arity matching
                        all.iter()
                            .find(|f| f.matches_signature(av.len()))
                            .cloned()
                            .unwrap_or(u)
                    }
                } else {
                    u
                };
                let (v, _i) = self._call_user_fn_with_this(
                    &selected,
                    av,
                    Value::Instance((*inst.as_ref()).clone()),
                )?;
                Ok(v)
            }
            Value::BoundNative(name, target) => {
                match (name.as_str(), &*target) {
                    ("Promise.resolve", _) => {
                        if av.len() != 1 {
                            return Err(err("Promise.resolve expects 1 argument"));
                        }
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                            q.push(NativeEffect::ResolvePromise(downstream, av[0].clone()));
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("Promise.reject", _) => {
                        if av.len() != 1 {
                            return Err(err("Promise.reject expects 1 argument"));
                        }
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                            q.push(NativeEffect::RejectPromise(downstream, av[0].clone()));
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("Promise.allSettled", _) => {
                        if av.len() != 1 {
                            return Err(err("Promise.allSettled([promises])"));
                        }
                        let arr = match &av[0] {
                            Value::Array(a) => a.clone(),
                            Value::Tuple(t) => t.clone(),
                            Value::DynArray(da) => da.data.clone(),
                            _ => return Err(err("Promise.allSettled expects array")),
                        };
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                        }
                        let n = arr.len();
                        if n == 0 {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::ResolvePromise(
                                downstream,
                                Value::Array(Vec::new()),
                            ));
                            return Ok(Value::Promise(downstream));
                        }
                        let results =
                            std::sync::Arc::new(std::sync::Mutex::new(vec![Value::Null; n]));
                        let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(n));

                        for (i, item) in arr.iter().enumerate() {
                            let on_settle = {
                                let results = results.clone();
                                let remaining = remaining.clone();
                                let ns_clone = ns.clone();
                                move |env: &mut dyn BuiltinEnv, val: Value, fulfilled: bool| {
                                    let mut obj = HashMap::default();
                                    if fulfilled {
                                        obj.insert("status".into(), Value::Str("fulfilled".into()));
                                        obj.insert("value".into(), val);
                                    } else {
                                        obj.insert("status".into(), Value::Str("rejected".into()));
                                        obj.insert("reason".into(), val);
                                    }
                                    let res_val = Value::Object(obj.into());

                                    let mut res = results.lock().unwrap();
                                    res[i] = res_val;
                                    let left =
                                        remaining.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                                    let res_snapshot = res.clone();
                                    drop(res);
                                    if left == 1 {
                                        let handle = env
                                            .get_timer_sender()
                                            .ok_or(err("timer channel unavailable"))?;
                                        let _ = handle;
                                        let mut q = ns_clone.lock().unwrap();
                                        q.push(NativeEffect::ResolvePromise(
                                            downstream,
                                            Value::Array(res_snapshot),
                                        ));
                                    }
                                    Ok(Value::Null)
                                }
                            };

                            match item {
                                Value::Promise(pid) => {
                                    let on_f = Value::Function(NativeFn(std::sync::Arc::new({
                                        let on_settle_clone = on_settle.clone();
                                        move |env, args| {
                                            let val = args.get(0).cloned().unwrap_or(Value::Null);
                                            on_settle_clone(env, val, true)
                                        }
                                    })));
                                    let on_r = Value::Function(NativeFn(std::sync::Arc::new({
                                        let on_settle_clone = on_settle.clone();
                                        move |env, args| {
                                            let val = args.get(0).cloned().unwrap_or(Value::Null);
                                            on_settle_clone(env, val, false)
                                        }
                                    })));
                                    let mut q = ns.lock().unwrap();
                                    q.push(NativeEffect::AttachThen(
                                        *pid,
                                        Some(on_f),
                                        Some(on_r),
                                        0,
                                    ));
                                }
                                other => {
                                    let mut q = ns.lock().unwrap();
                                    let mut obj = HashMap::default();
                                    obj.insert("status".into(), Value::Str("fulfilled".into()));
                                    obj.insert("value".into(), other.clone());
                                    let res_val = Value::Object(obj.into());
                                    {
                                        let mut res = results.lock().unwrap();
                                        res[i] = res_val;
                                    }
                                    let left =
                                        remaining.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                                    if left == 1 {
                                        let res_arr = results.lock().unwrap().clone();
                                        q.push(NativeEffect::ResolvePromise(
                                            downstream,
                                            Value::Array(res_arr),
                                        ));
                                    }
                                }
                            }
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("Promise.all", _) => {
                        if av.len() != 1 {
                            return Err(err("Promise.all([promises])"));
                        }
                        let arr = match &av[0] {
                            Value::Array(a) => a.clone(),
                            Value::Tuple(t) => t.clone(),
                            Value::DynArray(da) => da.data.clone(),
                            _ => return Err(err("Promise.all expects array")),
                        };
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                        }
                        let n = arr.len();
                        if n == 0 {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::ResolvePromise(
                                downstream,
                                Value::Array(Vec::new()),
                            ));
                            return Ok(Value::Promise(downstream));
                        }
                        let results =
                            std::sync::Arc::new(std::sync::Mutex::new(vec![Value::Null; n]));
                        let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(n));
                        for (i, item) in arr.iter().enumerate() {
                            match item {
                                Value::Promise(pid) => {
                                    let on_f = Value::Function(NativeFn(std::sync::Arc::new({
                                        let results = results.clone();
                                        let remaining = remaining.clone();
                                        let idx = i;
                                        let downstream_id = downstream;
                                        move |env: &mut dyn BuiltinEnv, args: Vec<Value>| {
                                            let v = args.get(0).cloned().unwrap_or(Value::Null);
                                            {
                                                let mut res = results.lock().unwrap();
                                                res[idx] = v;
                                            }
                                            let left = remaining
                                                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                                            if left == 1 {
                                                let handle = env
                                                    .get_timer_sender()
                                                    .ok_or(err("timer channel unavailable"))?;
                                                let _ = handle;
                                                let ns = env
                                                    .native_side_effects()
                                                    .ok_or(err("effects unavailable"))?;
                                                let res_arr = results.lock().unwrap().clone();
                                                let mut q = ns.lock().unwrap();
                                                q.push(NativeEffect::ResolvePromise(
                                                    downstream_id,
                                                    Value::Array(res_arr),
                                                ));
                                            }
                                            Ok(Value::Null)
                                        }
                                    })));
                                    let mut q = ns.lock().unwrap();
                                    q.push(NativeEffect::AttachThen(
                                        *pid,
                                        Some(on_f),
                                        None,
                                        downstream,
                                    ));
                                }
                                other => {
                                    {
                                        let mut res = results.lock().unwrap();
                                        res[i] = other.clone();
                                    }
                                    let left =
                                        remaining.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                                    if left == 1 {
                                        let mut q = ns.lock().unwrap();
                                        let res_arr = results.lock().unwrap().clone();
                                        q.push(NativeEffect::ResolvePromise(
                                            downstream,
                                            Value::Array(res_arr),
                                        ));
                                    }
                                }
                            }
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("Date.now", _) => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|_| err("system time error"))?;
                        Ok(Value::Number(now.as_millis() as f64))
                    }
                    ("then", Value::Promise(pid)) => {
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                        }
                        if av.len() < 1 {
                            return Err(err("then expects at least one arg"));
                        }
                        let on_ful = Some(av[0].clone());
                        let on_rej = if av.len() > 1 {
                            Some(av[1].clone())
                        } else {
                            None
                        };
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::AttachThen(*pid, on_ful, on_rej, downstream));
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("catch", Value::Promise(pid)) => {
                        let ns = self
                            .native_side_effects
                            .as_ref()
                            .ok_or(err("Promise not available in this context"))?;
                        let downstream = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::RegisterPromise(downstream));
                        }
                        if av.len() != 1 {
                            return Err(err("catch expects 1 arg"));
                        }
                        let on_rej = Some(av[0].clone());
                        {
                            let mut q = ns.lock().unwrap();
                            q.push(NativeEffect::AttachThen(*pid, None, on_rej, downstream));
                        }
                        Ok(Value::Promise(downstream))
                    }
                    ("date_getTime", Value::BigInt(ts)) => {
                        Ok(Value::Number(ts.to_f64().unwrap_or(0.0)))
                    }
                    ("date_toISOString", Value::BigInt(ts)) => {
                        let ms = ts.to_i64().unwrap_or(0);
                        let secs = (ms / 1000) as i64;
                        let nanos = ((ms % 1000) * 1_000_000) as i32;
                        let dt =
                            chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                                .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                        let iso = dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                        Ok(Value::Str(iso))
                    }
                    ("date_toString", Value::BigInt(ts)) => {
                        use num_traits::ToPrimitive;
                        let ms = ts.to_i64().unwrap_or(0);
                        let secs = (ms / 1000) as i64;
                        let nanos = ((ms % 1000) * 1_000_000) as i32;
                        let dt =
                            chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos as u32)
                                .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                        Ok(Value::Str(dt.to_string()))
                    }
                    ("append", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("append expects 1 arg"));
                        }
                        let mut out = a.clone();
                        out.push(av[0].clone());
                        Ok(Value::Array(out))
                    }
                    ("append", Value::DynArray(da)) => {
                        if av.len() != 1 {
                            return Err(err("append expects 1 arg"));
                        }
                        let mut new_data = da.data.clone();
                        if da.tracked_capacity > 0 && new_data.len() >= da.tracked_capacity {
                            return Err(err(format!(
                                "Cannot append to fixed-capacity array (capacity: {})",
                                da.tracked_capacity
                            )));
                        }
                        new_data.push(av[0].clone());
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray {
                                data: new_data,
                                element_type: da.element_type.clone(),
                                concrete_type: da.concrete_type.clone(),
                                tracked_capacity: da.tracked_capacity,
                            },
                        )))
                    }
                    ("push", Value::DynArray(da)) => {
                        if av.len() != 1 {
                            return Err(err("push expects 1 arg"));
                        }
                        let mut new_data = da.data.clone();
                        if da.tracked_capacity > 0 && new_data.len() >= da.tracked_capacity {
                            return Err(err(format!(
                                "Cannot append to fixed-capacity array (capacity: {})",
                                da.tracked_capacity
                            )));
                        }
                        new_data.push(av[0].clone());
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray {
                                data: new_data,
                                element_type: da.element_type.clone(),
                                concrete_type: da.concrete_type.clone(),
                                tracked_capacity: da.tracked_capacity,
                            },
                        )))
                    }
                    ("append" | "push" | "extend" | "insert", Value::RawArray(_t, _a)) => {
                        Err(err("Cannot append to raw array (fixed size)"))
                    }
                    ("map", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("map(fn)"));
                        }
                        let func = av[0].clone();
                        let mut out: Vec<Value> = Vec::with_capacity(a.len());
                        for v in a {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                                Value::UserFunction(u) => self._call_user_fn(&u, vec![v.clone()]),
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![v.clone()],
                                        Value::Instance((*inst).as_ref().clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("map expects function")),
                            }?;
                            out.push(res);
                        }
                        Ok(Value::Array(out))
                    }
                    ("filter", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("filter(fn)"));
                        }
                        let func = av[0].clone();
                        let mut out: Vec<Value> = Vec::with_capacity(a.len());
                        for v in a {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                                Value::UserFunction(u) => self._call_user_fn(&u, vec![v.clone()]),
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![v.clone()],
                                        Value::Instance((*inst.as_ref()).clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("filter expects function")),
                            }?;
                            if res.truthy() {
                                out.push(v.clone());
                            }
                        }
                        Ok(Value::Array(out))
                    }
                    ("reduce", Value::Array(a)) => {
                        if av.is_empty() || av.len() > 2 {
                            return Err(err("reduce(fn, init?)"));
                        }
                        if a.is_empty() && av.len() == 1 {
                            return Err(err("reduce of empty array with no initial value"));
                        }
                        let func = av[0].clone();
                        let mut acc = if av.len() == 2 {
                            av[1].clone()
                        } else {
                            a[0].clone()
                        };
                        let start_idx = if av.len() == 2 { 0 } else { 1 };

                        // PERFORMANCE NOTE (Phase 3 TODO): Same optimization needed as map()
                        // See comment in "map" case and PHASE3_IMPLEMENTATION_NOTES.md

                        for v in a.iter().skip(start_idx) {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => {
                                    (f)(self, vec![acc.clone(), v.clone()])
                                }
                                Value::UserFunction(u) => {
                                    self._call_user_fn(&u, vec![acc.clone(), v.clone()])
                                }
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![acc.clone(), v.clone()],
                                        Value::Instance((*inst.as_ref()).clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("reduce expects function")),
                            }?;
                            acc = res;
                        }
                        Ok(acc)
                    }
                    ("union", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("union expects 1 arg"));
                        }
                        let other = match &av[0] {
                            Value::Array(o) => o.clone(),
                            Value::Set(o) => o.clone(),
                            Value::DynArray(da) => da.data.clone(),
                            _ => return Err(err("union expects array or set")),
                        };
                        let mut out = Vec::new();
                        for v in a {
                            if !out.iter().any(|x| equals(x, v)) {
                                out.push(v.clone());
                            }
                        }
                        for v in &other {
                            if !out.iter().any(|x| equals(x, v)) {
                                out.push(v.clone());
                            }
                        }
                        Ok(Value::Array(out))
                    }
                    ("intersection", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("intersection expects 1 arg"));
                        }
                        let other = match &av[0] {
                            Value::Array(o) => o.clone(),
                            Value::Set(o) => o.clone(),
                            Value::DynArray(da) => da.data.clone(),
                            _ => return Err(err("intersection expects array or set")),
                        };
                        let mut out = Vec::new();
                        for v in a {
                            if other.iter().any(|x| equals(x, v)) {
                                if !out.iter().any(|x| equals(x, v)) {
                                    out.push(v.clone());
                                }
                            }
                        }
                        Ok(Value::Array(out))
                    }
                    ("map", Value::DynArray(da)) => {
                        if av.len() != 1 {
                            return Err(err("map(fn)"));
                        }
                        let func = av[0].clone();
                        let mut out: Vec<Value> = Vec::with_capacity(da.data.len());
                        for v in da.data.iter() {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                                Value::UserFunction(u) => self._call_user_fn(&u, vec![v.clone()]),
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![v.clone()],
                                        Value::Instance((*inst.as_ref()).clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("map expects function")),
                            }?;
                            out.push(res);
                        }
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray::new(out),
                        )))
                    }
                    ("filter", Value::DynArray(da)) => {
                        if av.len() != 1 {
                            return Err(err("filter(fn)"));
                        }
                        let func = av[0].clone();
                        let mut out: Vec<Value> = Vec::with_capacity(da.data.len());
                        for v in da.data.iter() {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => (f)(self, vec![v.clone()]),
                                Value::UserFunction(u) => self._call_user_fn(&u, vec![v.clone()]),
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![v.clone()],
                                        Value::Instance((*inst.as_ref()).clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("filter expects function")),
                            }?;
                            if res.truthy() {
                                out.push(v.clone());
                            }
                        }
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray::new(out),
                        )))
                    }
                    ("reduce", Value::DynArray(da)) => {
                        if av.is_empty() || av.len() > 2 {
                            return Err(err("reduce(fn, init?)"));
                        }
                        if da.data.is_empty() && av.len() == 1 {
                            return Err(err("reduce of empty array with no initial value"));
                        }
                        let func = av[0].clone();
                        let mut acc = if av.len() == 2 {
                            av[1].clone()
                        } else {
                            da.data[0].clone()
                        };
                        let start_idx = if av.len() == 2 { 0 } else { 1 };
                        for v in da.data.iter().skip(start_idx) {
                            let res = match &func {
                                Value::Function(NativeFn(f)) => {
                                    (f)(self, vec![acc.clone(), v.clone()])
                                }
                                Value::UserFunction(u) => {
                                    self._call_user_fn(&u, vec![acc.clone(), v.clone()])
                                }
                                Value::BoundMethod(u, inst) => self
                                    ._call_user_fn_with_this(
                                        &u,
                                        vec![acc.clone(), v.clone()],
                                        Value::Instance((*inst.as_ref()).clone()),
                                    )
                                    .map(|(v, _)| v),
                                _ => Err(err("reduce expects function")),
                            }?;
                            acc = res;
                        }
                        Ok(acc)
                    }
                    ("pop", Value::Array(a)) => {
                        let mut out = a.clone();
                        if out.is_empty() {
                            return Err(err("Cannot pop from empty array"));
                        }
                        out.pop();
                        Ok(Value::Array(out))
                    }
                    ("pop", Value::DynArray(da)) => {
                        let mut new_data = da.data.clone();
                        if new_data.is_empty() {
                            return Err(err("Cannot pop from empty array"));
                        }
                        new_data.pop();
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray {
                                data: new_data,
                                element_type: da.element_type.clone(),
                                concrete_type: da.concrete_type.clone(),
                                tracked_capacity: da.tracked_capacity,
                            },
                        )))
                    }
                    ("pop" | "shift" | "unshift" | "remove" | "clear", Value::RawArray(_t, _a)) => {
                        Err(err("Cannot pop from raw array (fixed size)"))
                    }
                    ("set_index", Value::Array(a)) => {
                        if av.len() != 2 {
                            return Err(err("set_index(index, value)"));
                        }
                        let idx = match av[0] {
                            Value::Number(n) => {
                                if (n - n.trunc()).abs() > 1e-12 || n < 0.0 {
                                    return Err(err("index must be integer and non-negative"));
                                }
                                n as usize
                            }
                            _ => return Err(err("set_index index must be number")),
                        };
                        let mut new_data = a.clone();
                        if idx >= new_data.len() {
                            return Err(err("index out of bounds"));
                        }
                        new_data[idx] = av[1].clone();
                        Ok(Value::Array(new_data))
                    }
                    ("set_index", Value::DynArray(da)) => {
                        if av.len() != 2 {
                            return Err(err("set_index(index, value)"));
                        }
                        let idx = match av[0] {
                            Value::Number(n) => {
                                if (n - n.trunc()).abs() > 1e-12 || n < 0.0 {
                                    return Err(err("index must be integer and non-negative"));
                                }
                                n as usize
                            }
                            _ => return Err(err("set_index index must be number")),
                        };
                        let mut new_data = da.data.clone();
                        if idx >= new_data.len() {
                            return Err(err("index out of bounds"));
                        }
                        new_data[idx] = av[1].clone();
                        Ok(Value::DynArray(Box::new(
                            crate::parsing::ast::DynamicArray {
                                data: new_data,
                                element_type: da.element_type.clone(),
                                concrete_type: da.concrete_type.clone(),
                                tracked_capacity: da.tracked_capacity,
                            },
                        )))
                    }
                    ("set_index", Value::RawArray(t, a)) => {
                        if av.len() != 2 {
                            return Err(err("set_index(index, value)"));
                        }
                        let idx = match av[0] {
                            Value::Number(n) => {
                                if (n - n.trunc()).abs() > 1e-12 || n < 0.0 {
                                    return Err(err("index must be integer and non-negative"));
                                }
                                n as usize
                            }
                            _ => return Err(err("set_index index must be number")),
                        };
                        let mut new_data = a.clone();
                        if idx >= new_data.len() {
                            return Err(err("index out of bounds"));
                        }
                        new_data[idx] = av[1].clone();
                        Ok(Value::RawArray(t.clone(), new_data))
                    }
                    ("index", Value::Array(a)) => {
                        if av.len() != 1 {
                            return Err(err("index expects 1 arg"));
                        }
                        if let Value::Number(n) = &av[0] {
                            let i = *n as usize;
                            return Ok(a.get(i).cloned().unwrap_or(Value::Null));
                        }
                        Err(err("index expects numeric arg"))
                    }
                    ("index", Value::Str(s)) => {
                        if av.len() != 1 {
                            return Err(err("index expects 1 arg"));
                        }
                        if let Value::Number(n) = &av[0] {
                            let i = *n as usize;
                            if let Some(ch) = s.chars().nth(i) {
                                return Ok(Value::Char(ch));
                            }
                            return Ok(Value::Null);
                        }
                        Err(err("index expects numeric arg"))
                    }
                    ("len", Value::Str(s)) => Ok(Value::Number(s.chars().count() as f64)),
                    ("length", Value::Str(s)) => Ok(Value::Number(s.chars().count() as f64)),
                    ("len", Value::Array(a)) => Ok(Value::Number(a.len() as f64)),
                    ("length", Value::Array(a)) => Ok(Value::Number(a.len() as f64)),
                    ("contains", Value::Set(s)) => {
                        if av.len() != 1 {
                            return Err(err("contains expects 1 arg"));
                        }
                        Ok(Value::Bool(s.iter().any(|x| equals(x, &av[0]))))
                    }
                    ("union", Value::Set(s)) => {
                        if av.len() != 1 {
                            return Err(err("union expects 1 arg"));
                        }
                        let mut out = s.clone();
                        match &av[0] {
                            Value::Set(o) => {
                                for v in o {
                                    if !out.iter().any(|x| equals(x, v)) {
                                        out.push(v.clone());
                                    }
                                }
                                Ok(Value::Set(out))
                            }
                            Value::Array(o) => {
                                for v in o {
                                    if !out.iter().any(|x| equals(x, v)) {
                                        out.push(v.clone());
                                    }
                                }
                                Ok(Value::Set(out))
                            }
                            _ => Err(err("union expects set or array")),
                        }
                    }
                    ("intersection", Value::Set(s)) => {
                        if av.len() != 1 {
                            return Err(err("intersection expects 1 arg"));
                        }
                        match &av[0] {
                            Value::Set(o) => {
                                let mut out = Vec::new();
                                for x in s {
                                    if o.iter().any(|y| equals(x, y)) {
                                        out.push(x.clone());
                                    }
                                }
                                Ok(Value::Set(out))
                            }
                            Value::Array(o) => {
                                let mut out = Vec::new();
                                for x in s {
                                    if o.iter().any(|y| equals(x, y)) {
                                        out.push(x.clone());
                                    }
                                }
                                Ok(Value::Set(out))
                            }
                            _ => Err(err("intersection expects set or array")),
                        }
                    }
                    ("add", Value::Set(s)) => {
                        if av.len() != 1 {
                            return Err(err("add expects 1 arg"));
                        }
                        let mut out = s.clone();
                        if !out.iter().any(|x| equals(x, &av[0])) {
                            out.push(av[0].clone());
                        }
                        Ok(Value::Set(out))
                    }
                    _ => Err(err("unknown native method")),
                }
            }
            Value::Class(c) => new_instance(c, av),
            Value::EnumCtor(ue, variant) => {
                let mut payload = None;
                if av.len() == 1 {
                    payload = Some(av[0].clone());
                } else if av.len() > 1 {
                    let mut arr = Vec::new();
                    for a in av {
                        arr.push(a.clone());
                    }
                    payload = Some(Value::Array(arr));
                }
                let mut m = HashMap::default();
                m.insert("__enum".into(), Value::Str(ue.name.clone()));
                m.insert("tag".into(), Value::Str(variant.clone()));
                if let Some(p) = payload {
                    m.insert("value".into(), p);
                }
                Ok(Value::Object(m.into()))
            }
            _ => Err(err("not callable")),
        }
    }
}
