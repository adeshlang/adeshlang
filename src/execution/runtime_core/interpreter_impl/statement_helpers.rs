//! Statement Execution Helper Functions
//!
//! This module contains helper functions that support statement execution in the interpreter.
//! Extracted from interpreter_core.rs as part of Phase 4F refactoring.
//!
//! # Functions
//!
//! - **Scope Management**: `acquire_scope`, `release_scope` - allocate and recycle scopes
//! - **Variable Definition**: `define_at`, `define_at_const` - create variable bindings with ownership tracking
//! - **Variable Lookup**: `get`, `get_fast`, `get_with_env`, `get_tracker` - resolve variables up scope chain
//! - **Environment Capture**: `capture_env_values`, `capture_function_env` - snapshot environment for closures
//!
//! # Design
//!
//! These functions are stateless helpers that operate on interpreter state through parameters.
//! They are extracted to improve code organization while maintaining 100% behavioral parity.

use crate::execution::runtime_core::interpreter::{Env, is_copy_value};
use crate::parsing::ast::{UserFn, Value};
use crate::utils::memory::OwnershipTracker;
use rustc_hash::FxHashMap as HashMap;
use std::rc::Rc;

/// Acquire a scope from the free list or create a new one
///
/// This implements scope recycling for memory efficiency:
/// - First tries to reuse a scope from the free list
/// - Creates a new scope if free list is empty
/// - Resets the recycled scope to clean state
#[inline]
pub fn acquire_scope(
    envs: &mut Vec<Env>,
    free_envs: &mut Vec<usize>,
    enclosing: Option<usize>,
) -> usize {
    if let Some(idx) = free_envs.pop() {
        // Recycle an existing scope - reset it for reuse
        envs[idx].reset(enclosing);
        idx
    } else {
        // No free scopes - create a new one
        envs.push(Env::new(enclosing));
        envs.len() - 1
    }
}

/// Release a scope back to the free list for recycling
///
/// Ensures memory is properly reclaimed and prevents leaks:
/// - Clears the scope to release any held values immediately
/// - Returns the scope index to the free list
/// - Never releases the global scope
#[inline]
pub fn release_scope(envs: &mut Vec<Env>, free_envs: &mut Vec<usize>, idx: usize, global: usize) {
    // Don't release the global scope
    if idx != global && idx < envs.len() {
        if envs[idx].is_captured {
            return;
        }
        // Clear the scope to release any held values immediately
        envs[idx].reset(None);
        free_envs.push(idx);
    }
}

/// Define a mutable variable in a scope
///
/// Creates a variable binding with:
/// - Value stored in scope's values map
/// - Marked as mutable (not const)
/// - Optional type annotation
/// - Ownership tracker for non-Copy values
pub fn define_at(envs: &mut Vec<Env>, env: usize, name: String, v: Value, ann: Option<String>) {
    let is_copy = is_copy_value(&v);

    // OPTIMIZATION: Move v instead of clone
    envs[env].values.insert(name.clone(), v);

    // Remove from consts (implied mutable), avoids allocation for 'false'
    envs[env].consts.remove(&name);

    // Only insert annotation if present
    if let Some(a) = ann {
        envs[env].type_ann.insert(name.clone(), Some(a));
    } else {
        envs[env].type_ann.remove(&name);
    }

    if is_copy {
        envs[env].ownership.remove(&name);
    } else {
        // Reuse 'name' string ownership here
        envs[env]
            .ownership
            .insert(name, Rc::new(OwnershipTracker::new_unique()));
    }
}

/// Define a const or let variable in a scope
///
/// Creates a variable binding with:
/// - Value stored in scope's values map
/// - Const flag (true for const, false for let)
/// - Optional type annotation
/// - Ownership tracker for non-Copy values
pub fn define_at_const(
    envs: &mut Vec<Env>,
    env: usize,
    name: String,
    v: Value,
    is_const: bool,
    ann: Option<String>,
) {
    let is_copy = is_copy_value(&v);

    envs[env].values.insert(name.clone(), v);

    if is_const {
        envs[env].consts.insert(name.clone(), true);
    } else {
        envs[env].consts.remove(&name);
    }

    if let Some(a) = ann {
        envs[env].type_ann.insert(name.clone(), Some(a));
    } else {
        envs[env].type_ann.remove(&name);
    }

    if is_copy {
        envs[env].ownership.remove(&name);
    } else {
        envs[env]
            .ownership
            .insert(name, Rc::new(OwnershipTracker::new_unique()));
    }
}

/// Fast variable lookup up the scope chain (mutable version)
///
/// Searches from the given environment up through enclosing scopes:
/// - Returns Some(value) if variable is found
/// - Returns None if variable not found in entire chain
#[inline]
pub fn get_fast(envs: &mut Vec<Env>, env: usize, name: &str) -> Option<Value> {
    let mut c = Some(env);
    let mut depth = 0;
    const MAX_LOOKUP_DEPTH: usize = 100;
    while let Some(id) = c {
        if let Some(vref) = envs[id].values.get(name) {
            return Some(vref.clone());
        }
        c = envs[id].enclosing;
        depth += 1;
        if depth > MAX_LOOKUP_DEPTH {
            return None;
        }
    }
    None
}

/// Optimized variable lookup with depth limit for safety
///
/// Searches from the given environment up through enclosing scopes:
/// - Returns Some(value) if variable is found
/// - Returns None if variable not found or depth exceeded
/// - MAX_LOOKUP_DEPTH prevents infinite loops in malformed scope chains
#[inline]
pub fn get(envs: &[Env], env: usize, name: &str) -> Option<Value> {
    let mut c = Some(env);
    let mut depth = 0;
    const MAX_LOOKUP_DEPTH: usize = 100;

    while let Some(id) = c {
        if let Some(v) = envs[id].values.get(name) {
            return Some(v.clone());
        }
        c = envs[id].enclosing;
        depth += 1;
        if depth > MAX_LOOKUP_DEPTH {
            #[cfg(debug_assertions)]
            eprintln!("[Interp] Deep lookup chain for '{}': depth={}", name, depth);
            return None;
        }
    }
    None
}

/// Get ownership tracker for a variable
///
/// Searches from the given environment up through enclosing scopes:
/// - Returns Some(tracker) if variable has an ownership tracker
/// - Returns None if variable not found or has no tracker (Copy type)
#[inline]
pub fn get_tracker(envs: &[Env], env: usize, name: &str) -> Option<Rc<OwnershipTracker>> {
    let mut c = Some(env);
    let mut depth = 0;
    const MAX_LOOKUP_DEPTH: usize = 100;

    while let Some(id) = c {
        if let Some(t) = envs[id].ownership.get(name) {
            return Some(t.clone());
        }
        c = envs[id].enclosing;
        depth += 1;
        if depth > MAX_LOOKUP_DEPTH {
            return None;
        }
    }
    None
}

/// Get variable value along with the environment it's defined in
///
/// Returns tuple (env_index, value) for the first scope containing the variable.
/// Useful when you need to know where a variable is bound (e.g., for assignment).
#[inline]
pub fn get_with_env(envs: &[Env], env: usize, name: &str) -> Option<(usize, Value)> {
    let mut c = Some(env);
    let mut depth = 0;
    const MAX_LOOKUP_DEPTH: usize = 100;

    while let Some(id) = c {
        if let Some(v) = envs[id].values.get(name) {
            return Some((id, v.clone()));
        }
        c = envs[id].enclosing;
        depth += 1;
        if depth > MAX_LOOKUP_DEPTH {
            return None;
        }
    }
    None
}

/// Capture an environment snapshot (values) for closure binding
///
/// Walks up the enclosing chain from `env` and collects all variable bindings.
/// The nearest scope shadows outer scopes; we insert from outer -> inner so inner wins.
///
/// Used to provide closure bindings to function execution (defaults evaluation, etc.).
pub fn capture_env_values(envs: &[Env], env: usize) -> HashMap<String, Value> {
    let mut out: HashMap<String, Value> = HashMap::default();
    let mut c = Some(env);
    let mut depth = 0;
    const MAX_CHAIN_DEPTH: usize = 100;

    // Build chain with pre-allocated capacity (most chains are shallow)
    let mut chain = Vec::with_capacity(8);
    while let Some(id) = c {
        chain.push(id);
        c = envs[id].enclosing;
        depth += 1;
        if depth > MAX_CHAIN_DEPTH {
            #[cfg(debug_assertions)]
            eprintln!("[Interp] Warning: deep environment chain ({})", depth);
            break;
        }
    }

    // Reserve capacity based on total size to avoid reallocations
    let total_size: usize = chain.iter().map(|id| envs[*id].values.len()).sum();
    out.reserve(total_size);

    // walk outermost first so inner scopes overwrite
    for id in chain.iter().rev() {
        for (k, v) in &envs[*id].values {
            out.insert(k.clone(), clone_value_for_capture(v));
        }
    }
    out
}

/// Clone a value for capture, stripping nested closures
///
/// When capturing environment values for closures, we need to avoid capturing
/// nested function closures (which would create circular references).
#[inline]
fn clone_value_for_capture(v: &Value) -> Value {
    match v {
        Value::UserFunction(u) => Value::UserFunction(UserFn {
            name: u.name.clone(),
            type_params: u.type_params.clone(),
            params: u.params.clone(),
            body: u.body.clone(),
            closure: u.closure,
            captured: None,
            visibility: u.visibility.clone(),
            ret_type: u.ret_type.clone(),
            is_async: u.is_async,
            is_static: u.is_static,
            is_abstract: u.is_abstract,
            is_constructor: u.is_constructor,
            is_getter: u.is_getter,
            is_setter: u.is_setter,
            is_operator: u.is_operator,
            operator_symbol: u.operator_symbol.clone(),
            defining_class: u.defining_class.clone(),
            is_unsafe: u.is_unsafe,
        }),
        Value::BoundMethod(u, inst) => {
            let u2 = UserFn {
                name: u.name.clone(),
                type_params: u.type_params.clone(),
                params: u.params.clone(),
                body: u.body.clone(),
                closure: u.closure,
                captured: None,
                visibility: u.visibility.clone(),
                ret_type: u.ret_type.clone(),
                is_async: u.is_async,
                is_static: u.is_static,
                is_abstract: u.is_abstract,
                is_constructor: u.is_constructor,
                is_getter: u.is_getter,
                is_setter: u.is_setter,
                is_operator: u.is_operator,
                operator_symbol: u.operator_symbol.clone(),
                defining_class: u.defining_class.clone(),
                is_unsafe: u.is_unsafe,
            };
            Value::BoundMethod(u2, inst.clone())
        }
        Value::Class(uc) => {
            let mut uc2 = uc.clone();
            for methods in uc2.methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            for methods in uc2.static_methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            for op in uc2.operators.values_mut() {
                op.captured = None;
            }
            for getter in uc2.getters.values_mut() {
                getter.captured = None;
            }
            for setter in uc2.setters.values_mut() {
                setter.captured = None;
            }
            Value::Class(uc2)
        }
        Value::Instance(inst) => {
            let mut inst2 = inst.clone();
            let class_mut = std::sync::Arc::make_mut(&mut inst2.class);
            for methods in class_mut.methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            for methods in class_mut.static_methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            for op in class_mut.operators.values_mut() {
                op.captured = None;
            }
            for getter in class_mut.getters.values_mut() {
                getter.captured = None;
            }
            for setter in class_mut.setters.values_mut() {
                setter.captured = None;
            }
            Value::Instance(inst2)
        }
        Value::Struct(us) => {
            let mut us2 = us.clone();
            for methods in us2.methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            Value::Struct(us2)
        }
        Value::Enum(ue) => {
            let mut ue2 = ue.clone();
            for methods in ue2.methods.values_mut() {
                for m in methods {
                    m.captured = None;
                }
            }
            Value::Enum(ue2)
        }
        other => other.clone(),
    }
}

/// Capture environment for function definitions (only for nested closures)
#[inline]
pub fn capture_function_env(envs: &[Env], env: usize) -> HashMap<String, Value> {
    capture_env_values(envs, env)
}
