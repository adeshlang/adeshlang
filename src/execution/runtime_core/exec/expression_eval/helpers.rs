//! Helper functions shared across expression evaluation modules.

use crate::execution::runtime_core::flow::ExecFlow;
use crate::execution::runtime_core::{PROMISE_COUNTER, err};
use crate::parsing::ast::NativeEffect;
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};

use super::super::core::Exec;

/// Whether `u` is a deterministic, side-effect-free function: no async, no
/// closures, and a body built only from literals/variables/numeric operators/
/// control flow/calls to itself or a whitelist of pure numeric builtins.
/// Shared by the expression-eval fast path and the core interpreter's
/// recursion memoization.
pub(crate) fn is_pure_user_fn(u: &crate::parsing::ast::UserFn) -> bool {
    use crate::parsing::ast::{Expr, ExprKind, Stmt, StmtKind};
    if u.is_async || u.captured.is_some() {
        return false;
    }

    fn check_expr(expr: &Expr, fn_name: &str) -> bool {
        match &expr.kind {
            ExprKind::Literal(_) | ExprKind::Variable(_) => true,
            ExprKind::Binary(l, _, r) | ExprKind::Logical(l, _, r) => {
                check_expr(l, fn_name) && check_expr(r, fn_name)
            }
            ExprKind::Unary(_, inner) | ExprKind::Grouping(inner) => check_expr(inner, fn_name),
            ExprKind::Conditional(cond, then_branch, else_branch) => {
                check_expr(cond, fn_name)
                    && check_expr(then_branch, fn_name)
                    && check_expr(else_branch, fn_name)
            }
            ExprKind::Call(callee, args, _) => {
                if let ExprKind::Variable(name) = &callee.kind {
                    let is_pure_callee = name == fn_name
                        || matches!(
                            name.as_str(),
                            "abs" | "min" | "max" | "sqrt" | "floor" | "ceil" | "round"
                        );
                    if is_pure_callee {
                        return args.iter().all(|a| check_expr(a, fn_name));
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn check_stmt(stmt: &Stmt, fn_name: &str) -> bool {
        match &stmt.kind {
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                check_expr(cond, fn_name)
                    && check_stmt(then_branch, fn_name)
                    && else_branch
                        .as_ref()
                        .map_or(true, |eb| check_stmt(eb, fn_name))
            }
            StmtKind::Return(Some(expr)) => check_expr(expr, fn_name),
            StmtKind::Return(None) => true,
            StmtKind::Block(stmts) => stmts.iter().all(|s| check_stmt(s, fn_name)),
            StmtKind::Let(_, Some(init), _, _, _, _) => check_expr(init, fn_name),
            StmtKind::Let(_, None, _, _, _, _) => true,
            StmtKind::ExprStmt(expr) => check_expr(expr, fn_name),
            _ => false,
        }
    }

    u.body.iter().all(|s| check_stmt(s, &u.name))
}

impl Exec {
    pub(in crate::execution::runtime_core) fn get_fast(&mut self, name: &str) -> Option<Value> {
        let mut c = Some(self.current);
        while let Some(id) = c {
            if let Some(v) = self.envs[id].values.get(name) {
                return Some(v.clone());
            }
            c = self.envs[id].enclosing;
        }
        None
    }

    pub(in crate::execution::runtime_core) fn get_tracker(
        &self,
        name: &str,
    ) -> Option<std::rc::Rc<crate::utils::memory::OwnershipTracker>> {
        let mut c = Some(self.current);
        while let Some(id) = c {
            if self.envs[id].values.contains_key(name) {
                return self.envs[id].ownership.get(name).cloned();
            }
            c = self.envs[id].enclosing;
        }
        None
    }

    pub(in crate::execution::runtime_core) fn get_with_env(
        &self,
        name: &str,
    ) -> Option<(usize, Value)> {
        let mut c = Some(self.current);
        while let Some(id) = c {
            if let Some(v) = self.envs[id].values.get(name) {
                return Some((id, v.clone()));
            }
            c = self.envs[id].enclosing;
        }
        None
    }

    pub(in crate::execution::runtime_core) fn get(&self, name: &str) -> Option<Value> {
        let mut c = Some(self.current);
        while let Some(id) = c {
            if let Some(v) = self.envs[id].values.get(name) {
                return Some(v.clone());
            }
            c = self.envs[id].enclosing;
        }
        None
    }

    pub(in crate::execution::runtime_core) fn assign(&mut self, name: &str, mut v: Value) -> bool {
        let mut c = Some(self.current);
        while let Some(id) = c {
            if self.envs[id].values.contains_key(name) {
                if self.envs[id].consts.get(name).cloned().unwrap_or(false) {
                    return false;
                }
                // enforce runtime annotation if present
                if let Some(ann_opt) = self.envs[id].type_ann.get(name) {
                    if let Some(ann) = ann_opt {
                        match crate::execution::runtime_core::interpreter::coerce_to_fixed_width(
                            &v, ann,
                        ) {
                            Ok(Some(converted)) => {
                                v = converted;
                            }
                            Ok(None) => {}
                            Err(_) => {
                                return false;
                            }
                        }
                        if !crate::execution::runtime_core::interpreter::ann_matches_value(
                            &Some(ann.clone()),
                            &v,
                        ) {
                            return false;
                        }
                    } else {
                        // No type annotation - use smart inference for numeric types
                        v = crate::execution::runtime_core::interpreter::infer_numeric_type(v);
                    }
                } else {
                    v = crate::execution::runtime_core::interpreter::infer_numeric_type(v);
                }
                self.envs[id].values.insert(name.into(), v);
                return true;
            }
            c = self.envs[id].enclosing;
        }
        false
    }

    pub(in crate::execution::runtime_core) fn match_pattern(
        &mut self,
        pat: &crate::parsing::ast::Pattern,
        val: &Value,
    ) -> bool {
        use crate::parsing::ast::Pattern;
        match pat {
            Pattern::Literal(v) => crate::execution::runtime_core::ops::equals(v, val),
            Pattern::Variable(name) => {
                self.envs[self.current]
                    .values
                    .insert(name.clone(), val.clone());
                true
            }
            Pattern::Wildcard => true,
            Pattern::Or(left, right) => {
                self.match_pattern(left, val) || self.match_pattern(right, val)
            }
            Pattern::EnumVariant(name, sub_pats) => {
                // Case 1: Matching against an Enum Instance (Object with __enum, tag, value)
                if let Value::Object(obj) = val {
                    if let Some(Value::Str(_enum_name)) = obj.get("__enum") {
                        if let Some(Value::Str(variant_name)) = obj.get("tag") {
                            if variant_name == name {
                                if sub_pats.is_empty() {
                                    return true;
                                }
                                // Match inner value
                                if let Some(inner) = obj.get("value") {
                                    if sub_pats.len() == 1 {
                                        return self.match_pattern(&sub_pats[0], inner);
                                    } else if let Value::Array(arr) = inner {
                                        // destructure tuple variant
                                        if arr.len() == sub_pats.len() {
                                            for (i, p) in sub_pats.iter().enumerate() {
                                                if !self.match_pattern(p, &arr[i]) {
                                                    return false;
                                                }
                                            }
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Case 2: Matching against a bare Enum Constructor (e.g. `None`)
                if let Value::EnumCtor(_, vname) = val {
                    if vname == name && sub_pats.is_empty() {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub(crate) fn _call_user_fn(
        &mut self,
        u: &crate::parsing::ast::UserFn,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        // Check pure recursion memoization cache for single-argument numerical functions
        let memo_key = if args.len() == 1 && !u.name.is_empty() && u.name != "<anon>" {
            match &args[0] {
                Value::Number(n) if n.fract() == 0.0 => Some(*n as i64),
                Value::I64(n) => Some(*n),
                Value::I32(n) => Some(*n as i64),
                Value::U64(n) if *n <= i64::MAX as u64 => Some(*n as i64),
                Value::U32(n) => Some(*n as i64),
                _ => None,
            }
        } else {
            None
        };

        let is_pure = if let Some(k) = memo_key {
            let pure = *self
                .pure_fn_cache
                .entry(u.name.clone())
                .or_insert_with(|| is_pure_user_fn(u));
            if pure {
                if let Some(cached) = self.recursion_memo.get(&(u.name.clone(), k)) {
                    return Ok(cached.clone());
                }
            }
            pure
        } else {
            false
        };

        // Fast path: inline the function call in the current Exec to avoid:
        // - Cloning the entire global env (HashMap<String, Value>)
        // - Creating a new Exec with ~33 empty allocations
        // - Creating a dummy UserInstance with ~13 empty HashMaps
        // - Cloning UserFn 3 times and UserInstance 3 times
        //
        // Instead, push a new scope, bind args, execute, and pop.
        let saved_current = self.current;
        let fn_env = self.acquire_scope(Some(saved_current));

        // Bind parameters to arguments
        let mut i = 0;
        for param in &u.params {
            let (name, def, _typ) = param;
            if name.starts_with("...") {
                let rest_name = name.trim_start_matches("...").to_string();
                let rest_args: Vec<Value> = args[i..].to_vec();
                self.envs[fn_env]
                    .values
                    .insert(rest_name, Value::Array(rest_args));
                break;
            } else {
                let val = if let Some(v) = args.get(i).cloned() {
                    v
                } else if let Some(dexpr) = def {
                    let prev = self.current;
                    self.current = saved_current;
                    let dv = self.eval_expr(dexpr)?;
                    self.current = prev;
                    dv
                } else {
                    Value::Null
                };
                self.envs[fn_env].values.insert(name.clone(), val);
                i += 1;
            }
        }

        // Only bind function name if not already found in enclosing chain to prevent heavy cloning
        if !u.name.is_empty() && u.name != "<anon>" && self.get_fast(&u.name).is_none() {
            self.envs[fn_env]
                .values
                .insert(u.name.clone(), Value::UserFunction(u.clone()));
        }

        // Execute
        self.current = fn_env;
        let flow = self.exec_block(&u.body)?;
        let result = match flow {
            ExecFlow::Return(Some(v)) => v,
            ExecFlow::Return(None) => Value::Null,
            _ => Value::Null,
        };

        self.release_scope(fn_env);
        self.current = saved_current;

        // Memoize pure result
        if is_pure {
            if let Some(k) = memo_key {
                if self.recursion_memo.len() < 100_000 {
                    self.recursion_memo
                        .insert((u.name.clone(), k), result.clone());
                }
            }
        }

        Ok(result)
    }

    pub(crate) fn _call_user_fn_with_this(
        &mut self,
        u: &crate::parsing::ast::UserFn,
        args: Vec<Value>,
        this_val: Value,
    ) -> Result<(Value, Value), String> {
        let saved_current = self.current;
        let fn_env = self.acquire_scope(Some(u.closure));

        // Bind parameters to arguments
        let mut i = 0;
        for param in &u.params {
            let (name, def, _typ) = param;
            if name.starts_with("...") {
                let rest_name = name.trim_start_matches("...").to_string();
                let rest_args: Vec<Value> = args[i..].to_vec();
                self.envs[fn_env]
                    .values
                    .insert(rest_name, Value::Array(rest_args));
                break;
            } else {
                let val = if let Some(v) = args.get(i).cloned() {
                    v
                } else if let Some(dexpr) = def {
                    let prev = self.current;
                    self.current = saved_current;
                    let dv = self.eval_expr(dexpr)?;
                    self.current = prev;
                    dv
                } else {
                    Value::Null
                };
                self.envs[fn_env].values.insert(name.clone(), val);
                i += 1;
            }
        }

        // Bind `this` and `self`
        self.envs[fn_env]
            .values
            .insert("this".into(), this_val.clone());
        self.envs[fn_env]
            .values
            .insert("self".into(), this_val.clone());

        // Bind `super` if class has parent
        if let Value::Instance(ref inst) = this_val {
            if let Some(parent_box) = inst.class.parent.clone() {
                self.envs[fn_env].values.insert(
                    "super".into(),
                    Value::Super(parent_box, Box::new(inst.clone())),
                );
            }
        }

        // Set class context
        let prev_class_context = self.current_class_context.clone();
        let class_context = u.defining_class.clone(); // Use the function's defining_class!
        self.current_class_context = class_context.clone();
        if let Some(ref cc) = class_context {
            self.envs[fn_env]
                .values
                .insert("__class_context__".to_string(), Value::Str(cc.clone()));
        }

        // Bind function name for recursion
        if !u.name.is_empty() && u.name != "<anon>" {
            self.envs[fn_env]
                .values
                .insert(u.name.clone(), Value::UserFunction(u.clone()));
        }

        // Execute
        self.current = fn_env;
        let flow = self.exec_block(&u.body)?;
        let result = match flow {
            ExecFlow::Return(Some(v)) => v,
            _ => Value::Null,
        };

        // Get updated `this`
        let updated_this = if let Some(t) = self.envs[fn_env].values.get("this").cloned() {
            t
        } else {
            this_val
        };

        self.release_scope(fn_env);
        self.current = saved_current;
        self.current_class_context = prev_class_context;

        Ok((result, updated_this))
    }
}

// Implementations of BuiltinEnv trait methods for Exec
impl BuiltinEnv for Exec {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn native_side_effects(&self) -> Option<Arc<Mutex<Vec<NativeEffect>>>> {
        self.native_side_effects.clone()
    }

    fn create_promise_executor(&mut self, _executor: Value) -> Result<Value, String> {
        let ns = self
            .native_side_effects
            .clone()
            .ok_or(err("Promise not available in this context"))?;
        let promise_id = PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst);
        {
            let mut q = ns.lock().unwrap();
            q.push(NativeEffect::RegisterPromise(promise_id));
        }
        let on_ful: Option<Value>;
        let on_rej: Option<Value>;
        if let Value::Null = _executor {
            on_ful = None;
            on_rej = None;
        } else if let Value::Function(exec) = _executor {
            let resolve_fn = Value::Function(NativeFn(Arc::new({
                let promise_id = promise_id;
                move |env: &mut dyn BuiltinEnv, args: Vec<Value>| {
                    let val = args.get(0).cloned().unwrap_or(Value::Null);
                    if let Some(ns) = env.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::ResolvePromise(promise_id, val));
                    }
                    Ok(Value::Null)
                }
            })));
            let reject_fn = Value::Function(NativeFn(Arc::new({
                let promise_id = promise_id;
                move |env: &mut dyn BuiltinEnv, args: Vec<Value>| {
                    let val = args.get(0).cloned().unwrap_or(Value::Null);
                    if let Some(ns) = env.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::RejectPromise(promise_id, val));
                    }
                    Ok(Value::Null)
                }
            })));
            exec.0(self, vec![resolve_fn, reject_fn])?;
            on_ful = None;
            on_rej = None;
        } else {
            return Err(err("Promise executor must be a function"));
        }
        {
            let mut q = ns.lock().unwrap();
            if on_ful.is_some() || on_rej.is_some() {
                q.push(NativeEffect::AttachThen(
                    promise_id, on_ful, on_rej, promise_id,
                ));
            }
        }
        Ok(Value::Promise(promise_id))
    }

    fn schedule_resolve(&mut self, id: u64, v: Value) {
        if let Some(ns) = &self.native_side_effects {
            let mut q = ns.lock().unwrap();
            q.push(NativeEffect::ResolvePromise(id, v));
        }
    }

    fn schedule_reject(&mut self, id: u64, v: Value) {
        if let Some(ns) = &self.native_side_effects {
            let mut q = ns.lock().unwrap();
            q.push(NativeEffect::RejectPromise(id, v));
        }
    }

    fn alloc_timer_id(&mut self) -> Result<u64, String> {
        Ok(PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    fn get_timer_sender(&self) -> Option<mpsc::Sender<u64>> {
        None
    }

    fn register_timer(
        &mut self,
        id: u64,
        callback: Value,
        cancel_flag: Arc<std::sync::atomic::AtomicBool>,
        is_interval: bool,
        ms: u64,
    ) -> Result<(), String> {
        if let Some(ns) = &self.native_side_effects {
            let mut q = ns.lock().unwrap();
            q.push(NativeEffect::RegisterTimer(
                id,
                callback,
                cancel_flag,
                is_interval,
                ms,
            ));
        }
        Ok(())
    }

    fn cancel_timer(&mut self, id: u64) -> Result<(), String> {
        if let Some(ns) = &self.native_side_effects {
            let mut q = ns.lock().unwrap();
            q.push(NativeEffect::CancelTimer(id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::execution::runtime::{Interpreter, ModuleLoader};

    #[test]
    // Performance-gated: interpreted fib(35) only meets the 500ms budget in
    // optimized builds. In debug it runs for minutes (and previously
    // overflowed the small test-binary stack), so it is skipped there.
    #[cfg_attr(debug_assertions, ignore = "perf-gated: run with cargo test --release")]
    fn test_pure_fib_fast_execution() {
        // Interpreted fib(35) drives deep recursive evaluation; run the test
        // on a dedicated large stack so even optimized test binaries (small
        // default stack) do not overflow mid-recursion.
        let worker = std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                let mut interp = Interpreter::new();
                let mut loader = ModuleLoader::new(std::path::Path::new("."));
                let code = r#"
                    fn fib(n) {
                        if (n <= 1) {
                            return n;
                        }
                        return fib(n - 1) + fib(n - 2);
                    }
                    let res = fib(35);
                "#;
                let start = std::time::Instant::now();
                let r = interp.run_module(code, &mut loader, Some("fib_test".to_string()));
                let elapsed = start.elapsed();
                assert!(r.is_ok(), "fib(35) failed: {:?}", r.err());
                assert!(
                    elapsed.as_millis() < 500,
                    "fib(35) took too long: {:?}",
                    elapsed
                );
            })
            .expect("failed to spawn fib benchmark thread");
        worker.join().expect("fib test panicked");
    }
}
