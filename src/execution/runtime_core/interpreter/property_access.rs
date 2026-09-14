//! Property access and instance creation utilities.
//!
//! Provides utilities for:
//! - Getting properties from values (objects, instances, arrays, etc.)
//! - Setting properties on mutable values
//! - Creating new class instances

use crate::execution::runtime_core::interpreter::{
    err, find_method_in_class_chain, find_method_with_visibility, is_field_accessible,
};
use crate::execution::runtime_core::interpreter_core::call_user_with_this;
use crate::parsing::ast::{BuiltinEnv, NativeFn, UserClass, UserInstance, Value, Visibility};
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap as HashMap;

/// Create a new instance of a class.
///
/// Validates that the class is not abstract and that all abstract methods
/// are implemented before creating the instance.
pub(in crate::execution::runtime_core) fn new_instance(
    c: UserClass,
    _args: Vec<Value>,
) -> Result<Value, String> {
    if c.is_abstract {
        return Err(err(format!(
            "Cannot instantiate abstract class '{}'",
            c.name
        )));
    }
    // Ensure no abstract methods remain unimplemented
    for (name, fns) in &c.methods {
        if !fns.is_empty() && fns.iter().all(|f| f.is_abstract) {
            return Err(err(format!(
                "Cannot instantiate class '{}' because abstract method '{}' is not implemented",
                c.name, name
            )));
        }
    }
    let inst = UserInstance {
        class_name: c.name.clone(),
        fields: std::sync::Arc::new(std::sync::RwLock::new(HashMap::default())),
        class: std::sync::Arc::new(c),
        prop_cache: std::sync::Arc::new(std::sync::RwLock::new(HashMap::default())),
        layout: None,
        raw: None,
    };
    Ok(Value::Instance(inst))
}

/// Get a property from a value.
///
/// Handles property access for all value types including:
/// - Objects and dates
/// - Arrays and strings (with numeric indexing)
/// - Class instances (with visibility checking)
/// - Enums and structs
/// - Native methods and bound functions
pub(in crate::execution::runtime_core) fn get_prop(
    obj: Value,
    key: &str,
    current_context: Option<&str>,
) -> Result<Value, String> {
    use crate::parsing::ast::Value::*;
    // (removed debug instrumentation)
    match obj {
        Value::Ref(inner, _handle) => return get_prop((*inner).clone(), key, current_context),
        Value::Share(sr) => {
            let inner_val = unsafe { &(*sr.ptr).value };
            return get_prop(inner_val.clone(), key, current_context);
        }
        Value::U64(handle) => {
            let mgr = super::arc_bridge::arc_manager();
            if let Ok(guard) = mgr.lock() {
                match key {
                    "strong_count" => {
                        let sc = guard.strong_count(handle).unwrap_or(0);
                        return Ok(Value::U64(sc as u64));
                    }
                    "weak_count" => {
                        let wc = guard.weak_count(handle).unwrap_or(0);
                        return Ok(Value::U64(wc as u64));
                    }
                    "is_alive" => {
                        let sc = guard.strong_count(handle).unwrap_or(0);
                        return Ok(Value::Bool(sc > 0));
                    }
                    _ => {
                        if let Ok(inner) = guard.get_value(handle) {
                            return get_prop(inner, key, current_context);
                        } else {
                            return Ok(Value::Null);
                        }
                    }
                }
            } else {
                return Ok(Value::Null);
            }
        }
        Class(c) => {
            // static property
            if let Some(v) = c.static_properties.get(key) {
                return Ok(v.clone());
            }
            // static method
            if let Some(fns) = c.static_methods.get(key) {
                if let Some(f) = fns.first() {
                    return Ok(UserFunction(f.clone()));
                }
            }
            Ok(Null)
        }
        Object(m) => {
            if let Some(Value::BigInt(ts)) = m.get("__date_ts") {
                match key {
                    "toISOString" => {
                        let ts_clone = ts.clone();
                        return Ok(Value::Function(NativeFn(std::sync::Arc::new(
                            move |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
                                let ms = ts_clone.to_i64().unwrap_or(0);
                                let secs = (ms / 1000) as i64;
                                let nanos = ((ms % 1000) * 1_000_000) as i32;
                                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(
                                    secs,
                                    nanos as u32,
                                )
                                .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                                let iso = dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                                Ok(Value::Str(iso))
                            },
                        ))));
                    }
                    "getTime" => {
                        let ts_clone = ts.clone();
                        return Ok(Value::Function(NativeFn(std::sync::Arc::new(
                            move |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
                                Ok(Value::Number(ts_clone.to_i64().unwrap_or(0) as f64))
                            },
                        ))));
                    }
                    "toString" => {
                        let ts_clone = ts.clone();
                        return Ok(Value::Function(NativeFn(std::sync::Arc::new(
                            move |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
                                let ms = ts_clone.to_i64().unwrap_or(0);
                                let secs = (ms / 1000) as i64;
                                let nanos = ((ms % 1000) * 1_000_000) as i32;
                                let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(
                                    secs,
                                    nanos as u32,
                                )
                                .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
                                Ok(Value::Str(dt.to_string()))
                            },
                        ))));
                    }
                    _ => {}
                }
            }
            Ok(m.get(key).cloned().unwrap_or(Null))
        }
        Struct(_s) => {
            // Struct access on the type returns Null for fields; construction uses `new` syntax instead
            Ok(Null)
        }
        Enum(e) => {
            // Accessing Enum.Variant should return a callable constructor which we represent
            // as EnumCtor that is later handled by Expr::New evaluation when called via `new`.
            if e.variants_map.contains_key(key) {
                return Ok(EnumCtor(Box::new(e.clone()), key.to_string()));
            }
            Ok(Null)
        }
        Array(a) => {
            // numeric index access for arrays
            if let Ok(i) = key.parse::<usize>() {
                return Ok(a.get(i).cloned().unwrap_or(Null));
            }
            // provide bound native methods for arrays
            match key {
                "append" | "extend" | "insert" | "pop" | "remove" | "index" | "count" | "clear"
                | "sort" | "reverse" | "push" | "shift" | "unshift" | "concat" | "indexOf"
                | "includes" | "set_index" | "first" | "last" | "slice" | "map" | "filter"
                | "reduce" | "union" | "intersection" => {
                    Ok(BoundNative(key.to_string(), Box::new(Value::Array(a))))
                }
                "len" | "length" => Ok(Number(a.len() as f64)),
                "capacity" => Ok(Number(a.capacity() as f64)),
                "metadata_size" => Ok(Number(24.0)),
                _ => Ok(Null),
            }
        }
        DynArray(da) => {
            if let Ok(i) = key.parse::<usize>() {
                return Ok(da.data.get(i).cloned().unwrap_or(Null));
            }
            match key {
                "append" | "extend" | "insert" | "pop" | "remove" | "index" | "count" | "clear"
                | "sort" | "reverse" | "push" | "shift" | "unshift" | "concat" | "indexOf"
                | "includes" | "set_index" | "first" | "last" | "slice" | "map" | "filter"
                | "reduce" | "union" | "intersection" => Ok(BoundNative(
                    key.to_string(),
                    Box::new(Value::DynArray(da.clone())),
                )),
                "len" | "length" => Ok(Number(da.data.len() as f64)),
                "capacity" => {
                    let cap = if da.tracked_capacity > 0 {
                        da.tracked_capacity
                    } else {
                        da.data.capacity()
                    };
                    Ok(Number(cap as f64))
                }
                "metadata_size" => Ok(Number(da.element_type.metadata_size() as f64)),
                _ => Ok(Null),
            }
        }
        RawArray(t, a) => {
            if let Ok(i) = key.parse::<usize>() {
                return Ok(a.get(i).cloned().unwrap_or(Null));
            }
            match key {
                "append" | "extend" | "insert" | "pop" | "remove" | "index" | "count" | "clear"
                | "sort" | "reverse" | "push" | "shift" | "unshift" | "concat" | "indexOf"
                | "includes" | "set_index" | "first" | "last" | "slice" | "map" | "filter"
                | "reduce" | "union" | "intersection" => Ok(BoundNative(
                    key.to_string(),
                    Box::new(Value::RawArray(t.clone(), a.clone())),
                )),
                "len" | "length" => Ok(Number(a.len() as f64)),
                "capacity" => Ok(Number(a.len() as f64)),
                "metadata_size" => Ok(Number(0.0)),
                _ => Ok(Null),
            }
        }
        Tuple(t) => {
            if let Ok(i) = key.parse::<usize>() {
                return Ok(t.get(i).cloned().unwrap_or(Null));
            }
            match key {
                "len" | "length" => Ok(Number(t.len() as f64)),
                "capacity" => Ok(Number(t.len() as f64)),
                "metadata_size" => Ok(Number(0.0)),
                _ => Ok(Null),
            }
        }
        Str(s) => {
            // numeric index access for strings (like arrays)
            if let Ok(i) = key.parse::<usize>() {
                if i < s.len() {
                    if let Some(ch) = s.chars().nth(i) {
                        return Ok(Char(ch));
                    }
                }
                return Ok(Null);
            }
            match key {
                "index" => Ok(BoundNative("index".to_string(), Box::new(Str(s.clone())))),
                "mock" => Ok(BoundNative(
                    "input.mock".to_string(),
                    Box::new(Str(s.clone())),
                )),
                "len" | "length" => Ok(Number(s.chars().count() as f64)),
                _ => Ok(Null),
            }
        }
        Promise(id) => match key {
            "then" => Ok(BoundNative(
                "then".to_string(),
                Box::new(Value::Promise(id)),
            )),
            "catch" => Ok(BoundNative(
                "catch".to_string(),
                Box::new(Value::Promise(id)),
            )),
            _ => Ok(Null),
        },
        Set(s) => {
            // provide bound native methods for sets: contains, union, intersection, add
            match key {
                "contains" | "union" | "intersection" | "add" => {
                    Ok(BoundNative(key.to_string(), Box::new(Value::Set(s))))
                }
                _ => Ok(Null),
            }
        }
        Instance(i) => {
            // check instance fields with visibility
            if let Some(v) = i.get_field(key) {
                if let Some(field_visibility) = i.class.field_visibility.get(key).cloned() {
                    let defining_class = i
                        .class
                        .field_owner
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or(&i.class.name);
                    if !is_field_accessible(
                        &Some(field_visibility.clone()),
                        defining_class,
                        &i.class,
                        current_context,
                    ) {
                        let vis_str = match &field_visibility {
                            Visibility::Priv => "private",
                            Visibility::Protected => "protected",
                            Visibility::Pub => "public",
                        };
                        return Err(format!(
                            "Cannot access {} field '{}' of class '{}'",
                            vis_str, key, defining_class
                        ));
                    }
                }
                return Ok(v);
            }

            // getters: check the class's getters map (getters are stored separately from regular methods)
            if let Some(getter) = i.class.getters.get(key) {
                let (val, _updated) =
                    call_user_with_this(getter.clone(), vec![], i.clone(), None, None)?;
                return Ok(val);
            }
            if key == "toString" {
                if let Some(fns) = i.class.methods.get("operatortoString") {
                    if let Some(sel) = fns.first() {
                        return Ok(BoundMethod(sel.clone(), Box::new(i.clone())));
                    }
                }
            }

            // Find method with visibility checking
            match find_method_with_visibility(&i.class, key, current_context) {
                Ok(Some(method)) => {
                    let bm = BoundMethod(method, Box::new(i.clone()));
                    return Ok(bm);
                }
                Ok(None) => {
                    if let Some(v) = i.class.static_properties.get(key) {
                        return Ok(v.clone());
                    }
                    return Ok(Null);
                }
                Err(e) => return Err(e),
            }
        }
        Super(parent_box, inst_box) => {
            if let Some(u) = find_method_in_class_chain(&*parent_box, key) {
                return Ok(BoundMethod(u, Box::new((*inst_box).clone())));
            }
            Ok(Null)
        }
        Function(_nf) => match key {
            "all" => Ok(BoundNative("Promise.all".to_string(), Box::new(Null))),
            "any" => Ok(BoundNative("Promise.any".to_string(), Box::new(Null))),
            "race" => Ok(BoundNative("Promise.race".to_string(), Box::new(Null))),
            "allSettled" => Ok(BoundNative(
                "Promise.allSettled".to_string(),
                Box::new(Null),
            )),
            "resolve" => Ok(BoundNative("Promise.resolve".to_string(), Box::new(Null))),
            "reject" => Ok(BoundNative("Promise.reject".to_string(), Box::new(Null))),
            "now" => Ok(BoundNative("Date.now".to_string(), Box::new(Null))),
            "fromPipe" => Ok(BoundNative("input.fromPipe".to_string(), Box::new(Null))),
            "select" => Ok(BoundNative("input.select".to_string(), Box::new(Null))),
            "selectEnum" => Ok(BoundNative("input.selectEnum".to_string(), Box::new(Null))),
            "selectKey" => Ok(BoundNative("input.selectKey".to_string(), Box::new(Null))),
            "checkbox" => Ok(BoundNative("input.checkbox".to_string(), Box::new(Null))),
            "radio" => Ok(BoundNative("input.radio".to_string(), Box::new(Null))),
            "form" => Ok(BoundNative("input.form".to_string(), Box::new(Null))),
            "voice" => Ok(BoundNative("input.voice".to_string(), Box::new(Null))),
            "record" => Ok(BoundNative("input.record".to_string(), Box::new(Null))),
            "play" => Ok(BoundNative("input.play".to_string(), Box::new(Null))),
            "mock" => Ok(BoundNative("input.mock".to_string(), Box::new(Null))),
            "confirm" => Ok(BoundNative("input.confirm".to_string(), Box::new(Null))),
            "password" => Ok(BoundNative("input.password".to_string(), Box::new(Null))),
            "fuzzy" => Ok(BoundNative("input.fuzzy".to_string(), Box::new(Null))),
            "slider" => Ok(BoundNative("input.slider".to_string(), Box::new(Null))),
            "tree" => Ok(BoundNative("input.tree".to_string(), Box::new(Null))),
            "table" => Ok(BoundNative("input.table".to_string(), Box::new(Null))),
            "datepicker" => Ok(BoundNative("input.datepicker".to_string(), Box::new(Null))),
            "datetimepicker" | "datetime" => Ok(BoundNative(
                "input.datetimepicker".to_string(),
                Box::new(Null),
            )),
            "timepicker" => Ok(BoundNative("input.timepicker".to_string(), Box::new(Null))),
            "color" => Ok(BoundNative("input.color".to_string(), Box::new(Null))),
            "pin" => Ok(BoundNative("input.pin".to_string(), Box::new(Null))),
            "diff" => Ok(BoundNative("input.diff".to_string(), Box::new(Null))),
            "hotkey" => Ok(BoundNative("input.hotkey".to_string(), Box::new(Null))),
            "ai" => Ok(BoundNative("input.ai".to_string(), Box::new(Null))),
            "stream" => Ok(BoundNative("input.stream".to_string(), Box::new(Null))),
            "parse" => Ok(BoundNative("args.parse".to_string(), Box::new(Null))),
            "get" => Ok(BoundNative("args.get".to_string(), Box::new(Null))),
            "has" => Ok(BoundNative("args.has".to_string(), Box::new(Null))),
            _ => Ok(Null),
        },
        _ => Ok(Null),
    }
}

/// Set a property on a mutable value.
///
/// Handles property setting for:
/// - Objects (with clone-on-write semantics)
/// - Class instances (with visibility checking and setter support)
pub(in crate::execution::runtime_core) fn set_prop(
    obj: &mut Value,
    key: &str,
    val: Value,
    current_context: Option<&str>,
) -> Result<(), String> {
    use crate::parsing::ast::Value::*;
    match obj {
        Share(sr) => unsafe { set_prop(&mut (*sr.ptr).value, key, val, current_context) },
        Object(marc) => {
            // Since property_access.rs does not have access to the interpreter environment easily,
            // we will skip deep schema validation here for now and allow the assignment.
            // Full validation is handled in interpreter_core.rs

            // Use Arc::make_mut to modify in place (clone-on-write if shared)
            std::sync::Arc::make_mut(marc).insert(key.into(), val);
            Ok(())
        }
        Instance(i) => {
            if let Some(field_visibility) = i.class.field_visibility.get(key).cloned() {
                let defining_class = i
                    .class
                    .field_owner
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(&i.class.name);
                if !is_field_accessible(
                    &Some(field_visibility.clone()),
                    defining_class,
                    &i.class,
                    current_context,
                ) {
                    let vis_str = match &field_visibility {
                        Visibility::Priv => "private",
                        Visibility::Protected => "protected",
                        Visibility::Pub => "public",
                    };
                    return Err(format!(
                        "Cannot set {} field '{}' of class '{}'",
                        vis_str, key, defining_class
                    ));
                }
            }
            // Check if there's a setter
            if let Some(setter) = i.class.setters.get(key) {
                call_user_with_this(setter.clone(), vec![val], i.clone(), None, None)?;
                return Ok(());
            }

            // Set the field normally
            i.set_field(key, val)?;
            Ok(())
        }
        _ => Err(err("property set on non-object")),
    }
}

pub(in crate::execution::runtime_core) fn get_prop_exec(
    obj: Value,
    key: &str,
    current_context: Option<&str>,
    exec: &mut crate::execution::runtime_core::exec::core::Exec,
) -> Result<Value, String> {
    use crate::parsing::ast::Value::*;
    match obj {
        Value::Ref(inner, _handle) => {
            return get_prop_exec((*inner).clone(), key, current_context, exec);
        }
        Value::Share(sr) => {
            let inner_val = unsafe { &(*sr.ptr).value };
            return get_prop_exec(inner_val.clone(), key, current_context, exec);
        }
        Instance(i) => {
            // check instance fields with visibility
            if let Some(v) = i.get_field(key) {
                if let Some(field_visibility) = i.class.field_visibility.get(key).cloned() {
                    let defining_class = i
                        .class
                        .field_owner
                        .get(key)
                        .map(String::as_str)
                        .unwrap_or(&i.class.name);
                    if !is_field_accessible(
                        &Some(field_visibility.clone()),
                        defining_class,
                        &i.class,
                        current_context,
                    ) {
                        let vis_str = match &field_visibility {
                            Visibility::Priv => "private",
                            Visibility::Protected => "protected",
                            Visibility::Pub => "public",
                        };
                        return Err(format!(
                            "Cannot access {} field '{}' of class '{}'",
                            vis_str, key, defining_class
                        ));
                    }
                }
                return Ok(v);
            }

            // getters: check the class's getters map (getters are stored separately from regular methods)
            if let Some(getter) = i.class.getters.get(key) {
                let (val, _) =
                    exec._call_user_fn_with_this(getter, vec![], Value::Instance(i.clone()))?;
                return Ok(val);
            }
            if key == "toString" {
                if let Some(fns) = i.class.methods.get("operatortoString") {
                    if let Some(sel) = fns.first() {
                        return Ok(BoundMethod(sel.clone(), Box::new(i.clone())));
                    }
                }
            }

            // Find method with visibility checking
            match find_method_with_visibility(&i.class, key, current_context) {
                Ok(Some(method)) => {
                    let bm = BoundMethod(method, Box::new(i.clone()));
                    return Ok(bm);
                }
                Ok(None) => {
                    if let Some(v) = i.class.static_properties.get(key) {
                        return Ok(v.clone());
                    }
                    return Ok(Null);
                }
                Err(e) => return Err(e),
            }
        }
        _ => get_prop(obj, key, current_context),
    }
}

pub(in crate::execution::runtime_core) fn set_prop_exec(
    obj: &mut Value,
    key: &str,
    val: Value,
    current_context: Option<&str>,
    exec: &mut crate::execution::runtime_core::exec::core::Exec,
) -> Result<(), String> {
    use crate::parsing::ast::Value::*;
    match obj {
        Share(sr) => unsafe {
            set_prop_exec(&mut (*sr.ptr).value, key, val, current_context, exec)
        },
        Instance(i) => {
            // Check if there's a setter
            if let Some(setter) = i.class.setters.get(key) {
                let (_ret, updated) =
                    exec._call_user_fn_with_this(setter, vec![val], Value::Instance(i.clone()))?;
                if let Value::Instance(up_inst) = updated {
                    *i = up_inst;
                }
                return Ok(());
            }

            if let Some(field_visibility) = i.class.field_visibility.get(key).cloned() {
                let defining_class = i
                    .class
                    .field_owner
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or(&i.class.name);
                if !is_field_accessible(
                    &Some(field_visibility.clone()),
                    defining_class,
                    &i.class,
                    current_context,
                ) {
                    let vis_str = match &field_visibility {
                        Visibility::Priv => "private",
                        Visibility::Protected => "protected",
                        Visibility::Pub => "public",
                    };
                    return Err(format!(
                        "Cannot set {} field '{}' of class '{}'",
                        vis_str, key, defining_class
                    ));
                }
            }

            // Set the field normally
            i.set_field(key, val)?;
            Ok(())
        }
        _ => set_prop(obj, key, val, current_context),
    }
}
