//! Collections built-in bindings for interpreter & execution backends
//!
//! Provides the Collections standard library namespace and all 15 collection types.

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::adesh_alloc::{
    BTreeMap, BTreeSet, BinaryHeap, BitSet, HashMap as AllocHashMap, HashSet,
    OrderedMap as AllocOrderedMap, OrderedSet as AllocOrderedSet, PriorityQueue,
    Queue as AllocQueue, RingBuffer, Stack as AllocStack, Vec, VecDeque,
};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "Collections",
        "collections",
        "Collections standard library namespace",
        |_env: &mut dyn BuiltinEnv, _args: std::vec::Vec<Value>| {
            let mut methods = HashMap::default();

            methods.insert(
                "Vec".to_string(),
                Value::Function(NativeFn(Arc::new(collections_vec_new))),
            );
            methods.insert(
                "Slice".to_string(),
                Value::Function(NativeFn(Arc::new(collections_slice_new))),
            );
            methods.insert(
                "HashMap".to_string(),
                Value::Function(NativeFn(Arc::new(collections_hashmap_new))),
            );
            methods.insert(
                "HashSet".to_string(),
                Value::Function(NativeFn(Arc::new(collections_hashset_new))),
            );
            methods.insert(
                "VecDeque".to_string(),
                Value::Function(NativeFn(Arc::new(collections_vecdeque_new))),
            );
            methods.insert(
                "BTreeMap".to_string(),
                Value::Function(NativeFn(Arc::new(collections_btreemap_new))),
            );
            methods.insert(
                "BTreeSet".to_string(),
                Value::Function(NativeFn(Arc::new(collections_btreeset_new))),
            );
            methods.insert(
                "BinaryHeap".to_string(),
                Value::Function(NativeFn(Arc::new(collections_binaryheap_new))),
            );
            methods.insert(
                "PriorityQueue".to_string(),
                Value::Function(NativeFn(Arc::new(collections_priorityqueue_new))),
            );
            methods.insert(
                "BitSet".to_string(),
                Value::Function(NativeFn(Arc::new(collections_bitset_new))),
            );
            methods.insert(
                "RingBuffer".to_string(),
                Value::Function(NativeFn(Arc::new(collections_ringbuffer_new))),
            );
            methods.insert(
                "Queue".to_string(),
                Value::Function(NativeFn(Arc::new(collections_queue_new))),
            );
            methods.insert(
                "Stack".to_string(),
                Value::Function(NativeFn(Arc::new(collections_stack_new))),
            );
            methods.insert(
                "OrderedMap".to_string(),
                Value::Function(NativeFn(Arc::new(collections_orderedmap_new))),
            );
            methods.insert(
                "OrderedSet".to_string(),
                Value::Function(NativeFn(Arc::new(collections_orderedset_new))),
            );

            Ok(Value::Object(Arc::new(methods)))
        },
    );
}

// Helper to wrap a Mutex'd object in a Value::Object representing methods
fn wrap_instance<T: 'static, F: Fn(&Arc<Mutex<T>>, &mut HashMap<String, Value>)>(
    instance: T,
    init: F,
) -> Value {
    let state = Arc::new(Mutex::new(instance));
    let mut methods = HashMap::default();
    init(&state, &mut methods);
    Value::Object(Arc::new(methods))
}

// Helper structure to allow comparison of generic runtime Value types in sorting / heaps
#[derive(Clone)]
struct SortableValue(Value);

fn value_to_f64_opt(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

impl PartialEq for SortableValue {
    fn eq(&self, other: &Self) -> bool {
        if let (Some(a), Some(b)) = (value_to_f64_opt(&self.0), value_to_f64_opt(&other.0)) {
            return a == b;
        }
        match (&self.0, &other.0) {
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for SortableValue {}

impl PartialOrd for SortableValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if let (Some(a), Some(b)) = (value_to_f64_opt(&self.0), value_to_f64_opt(&other.0)) {
            return a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
        }
        match (&self.0, &other.0) {
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

fn get_generic_type_context() -> std::vec::Vec<String> {
    crate::execution::runtime_core::GENERIC_TYPE_CONTEXT.with(|ctx| ctx.borrow().clone())
}

fn unwrap_ref_val(val: Value) -> Value {
    match val {
        Value::Ref(inner, _) => unwrap_ref_val(*inner),
        Value::Share(sr) => unsafe { unwrap_ref_val((*sr.ptr).value.clone()) },
        other => other,
    }
}

fn unwrap_ref<'a>(mut v: &'a Value) -> &'a Value {
    loop {
        match v {
            Value::Ref(inner, _) => v = inner.as_ref(),
            Value::Share(sr) => unsafe { v = &(*sr.ptr).value },
            _ => return v,
        }
    }
}

fn extract_array(val: &Value) -> Option<std::vec::Vec<Value>> {
    match unwrap_ref(val) {
        Value::Array(arr) => Some(arr.clone()),
        Value::RawArray(_, arr) => Some(arr.clone()),
        Value::DynArray(da) => Some(da.data.clone()),
        Value::Tuple(tup) => Some(tup.clone()),
        _ => None,
    }
}

fn validate_and_coerce_type(val: Value, expected_type: &str) -> Result<Value, String> {
    let val = unwrap_ref_val(val);
    let clean = expected_type.trim();
    if clean.is_empty()
        || clean == "T"
        || clean == "K"
        || clean == "V"
        || clean == "Object"
        || clean == "Value"
    {
        return Ok(val);
    }

    match clean.to_lowercase().as_str() {
        "u8" => match val {
            Value::U8(_) => Ok(val),
            Value::Number(n) if n >= 0.0 && n <= 255.0 && n.fract() == 0.0 => {
                Ok(Value::U8(n as u8))
            }
            Value::I32(n) if n >= 0 && n <= 255 => Ok(Value::U8(n as u8)),
            Value::I64(n) if n >= 0 && n <= 255 => Ok(Value::U8(n as u8)),
            Value::U32(n) if n <= 255 => Ok(Value::U8(n as u8)),
            Value::U64(n) if n <= 255 => Ok(Value::U8(n as u8)),
            Value::I8(n) if n >= 0 => Ok(Value::U8(n as u8)),
            Value::I16(n) if n >= 0 && n <= 255 => Ok(Value::U8(n as u8)),
            Value::U16(n) if n <= 255 => Ok(Value::U8(n as u8)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'u8'",
                val
            )),
        },
        "u16" => match val {
            Value::U16(_) => Ok(val),
            Value::Number(n) if n >= 0.0 && n <= 65535.0 && n.fract() == 0.0 => {
                Ok(Value::U16(n as u16))
            }
            Value::I32(n) if n >= 0 && n <= 65535 => Ok(Value::U16(n as u16)),
            Value::I64(n) if n >= 0 && n <= 65535 => Ok(Value::U16(n as u16)),
            Value::U32(n) if n <= 65535 => Ok(Value::U16(n as u16)),
            Value::U8(n) => Ok(Value::U16(n as u16)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'u16'",
                val
            )),
        },
        "u32" => match val {
            Value::U32(_) => Ok(val),
            Value::Number(n) if n >= 0.0 && n <= 4294967295.0 && n.fract() == 0.0 => {
                Ok(Value::U32(n as u32))
            }
            Value::I32(n) if n >= 0 => Ok(Value::U32(n as u32)),
            Value::I64(n) if n >= 0 && n <= 4294967295 => Ok(Value::U32(n as u32)),
            Value::U8(n) => Ok(Value::U32(n as u32)),
            Value::U16(n) => Ok(Value::U32(n as u32)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'u32'",
                val
            )),
        },
        "u64" => match val {
            Value::U64(_) => Ok(val),
            Value::Number(n) if n >= 0.0 && n.fract() == 0.0 => Ok(Value::U64(n as u64)),
            Value::I32(n) if n >= 0 => Ok(Value::U64(n as u64)),
            Value::I64(n) if n >= 0 => Ok(Value::U64(n as u64)),
            Value::U8(n) => Ok(Value::U64(n as u64)),
            Value::U16(n) => Ok(Value::U64(n as u64)),
            Value::U32(n) => Ok(Value::U64(n as u64)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'u64'",
                val
            )),
        },
        "i8" => match val {
            Value::I8(_) => Ok(val),
            Value::Number(n) if n >= -128.0 && n <= 127.0 && n.fract() == 0.0 => {
                Ok(Value::I8(n as i8))
            }
            Value::I32(n) if n >= -128 && n <= 127 => Ok(Value::I8(n as i8)),
            Value::I64(n) if n >= -128 && n <= 127 => Ok(Value::I8(n as i8)),
            Value::U8(n) if n <= 127 => Ok(Value::I8(n as i8)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'i8'",
                val
            )),
        },
        "i16" => match val {
            Value::I16(_) => Ok(val),
            Value::Number(n) if n >= -32768.0 && n <= 32767.0 && n.fract() == 0.0 => {
                Ok(Value::I16(n as i16))
            }
            Value::I32(n) if n >= -32768 && n <= 32767 => Ok(Value::I16(n as i16)),
            Value::I64(n) if n >= -32768 && n <= 32767 => Ok(Value::I16(n as i16)),
            Value::U8(n) => Ok(Value::I16(n as i16)),
            Value::U16(n) if n <= 32767 => Ok(Value::I16(n as i16)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'i16'",
                val
            )),
        },
        "i32" | "int" => match val {
            Value::I32(_) => Ok(val),
            Value::Number(n) if n.fract() == 0.0 => Ok(Value::I32(n as i32)),
            Value::I64(n) => Ok(Value::I32(n as i32)),
            Value::U8(n) => Ok(Value::I32(n as i32)),
            Value::U16(n) => Ok(Value::I32(n as i32)),
            Value::U32(n) if n <= 2147483647 => Ok(Value::I32(n as i32)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'i32'",
                val
            )),
        },
        "i64" => match val {
            Value::I64(_) => Ok(val),
            Value::Number(n) if n.fract() == 0.0 => Ok(Value::I64(n as i64)),
            Value::I32(n) => Ok(Value::I64(n as i64)),
            Value::U8(n) => Ok(Value::I64(n as i64)),
            Value::U16(n) => Ok(Value::I64(n as i64)),
            Value::U32(n) => Ok(Value::I64(n as i64)),
            Value::U64(n) if n <= 9223372036854775807 => Ok(Value::I64(n as i64)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'i64'",
                val
            )),
        },
        "f32" => match val {
            Value::F32(_) => Ok(val),
            Value::F64(n) => Ok(Value::F32(n as f32)),
            Value::Number(n) => Ok(Value::F32(n as f32)),
            Value::I32(n) => Ok(Value::F32(n as f32)),
            Value::U8(n) => Ok(Value::F32(n as f32)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'f32'",
                val
            )),
        },
        "f64" | "float" | "number" => match val {
            Value::F64(_) | Value::Number(_) => Ok(val),
            Value::F32(n) => Ok(Value::F64(n as f64)),
            Value::I32(n) => Ok(Value::F64(n as f64)),
            Value::I64(n) => Ok(Value::F64(n as f64)),
            Value::U8(n) => Ok(Value::F64(n as f64)),
            Value::U16(n) => Ok(Value::F64(n as f64)),
            Value::U32(n) => Ok(Value::F64(n as f64)),
            Value::U64(n) => Ok(Value::F64(n as f64)),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'f64'",
                val
            )),
        },
        "bool" => match val {
            Value::Bool(_) => Ok(val),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'bool'",
                val
            )),
        },
        "string" | "str" => match val {
            Value::Str(_) => Ok(val),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'string'",
                val
            )),
        },
        "array" => match val {
            Value::Array(_) => Ok(val),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'array'",
                val
            )),
        },
        "object" | "struct" => match val {
            Value::Object(_) | Value::Struct(_) => Ok(val),
            _ => Err(format!(
                "TypeError: Value '{:?}' does not match generic type constraint 'object'",
                val
            )),
        },
        _ => Ok(val),
    }
}

// 1. Vec
fn collections_vec_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let vec: Vec<Value> = Vec::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(vec, move |state, methods| {
        let st_push = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "push".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("push(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_push.lock().unwrap().push(val);
                Ok(Value::Null)
            }))),
        );

        let st_pop = state.clone();
        methods.insert(
            "pop".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop.lock().unwrap().pop().unwrap_or(Value::Null))
            }))),
        );

        let st_get = state.clone();
        methods.insert(
            "get".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("get(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                Ok(st_get
                    .lock()
                    .unwrap()
                    .get(idx)
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_set = state.clone();
        let elem_t2 = elem_t_opt.clone();
        methods.insert(
            "set".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("set(idx, val)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                let val = if let Some(target_t) = elem_t2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), target_t)?
                } else {
                    args[1].clone()
                };
                if let Some(elem) = st_set.lock().unwrap().get_mut(idx) {
                    *elem = val;
                    Ok(Value::Bool(true))
                } else {
                    Ok(Value::Bool(false))
                }
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_cap = state.clone();
        methods.insert(
            "capacity".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_cap.lock().unwrap().capacity() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_insert = state.clone();
        let elem_t3 = elem_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("insert(idx, val)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                let val = if let Some(target_t) = elem_t3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), target_t)?
                } else {
                    args[1].clone()
                };
                st_insert.lock().unwrap().insert(idx, val);
                Ok(Value::Null)
            }))),
        );

        let st_remove = state.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                let mut guard = st_remove.lock().unwrap();
                if idx < guard.len() {
                    Ok(guard.remove(idx))
                } else {
                    Ok(Value::Null)
                }
            }))),
        );

        let st_swap_remove = state.clone();
        methods.insert(
            "swapRemove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("swapRemove(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                let mut guard = st_swap_remove.lock().unwrap();
                if idx < guard.len() {
                    Ok(guard.swap_remove(idx))
                } else {
                    Ok(Value::Null)
                }
            }))),
        );

        let st_first = state.clone();
        methods.insert(
            "first".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_first
                    .lock()
                    .unwrap()
                    .first()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_last = state.clone();
        methods.insert(
            "last".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_last
                    .lock()
                    .unwrap()
                    .last()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_contains = state.clone();
        methods.insert(
            "contains".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("contains(val)".to_string());
                }
                let target = &args[0];
                let guard = st_contains.lock().unwrap();
                let has = guard.as_slice().iter().any(|item: &Value| {
                    SortableValue(item.clone()) == SortableValue(target.clone())
                });
                Ok(Value::Bool(has))
            }))),
        );

        let st_index_of = state.clone();
        methods.insert(
            "indexOf".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("indexOf(val)".to_string());
                }
                let target = &args[0];
                let guard = st_index_of.lock().unwrap();
                for (idx, item) in guard.as_slice().iter().enumerate() {
                    if SortableValue(item.clone()) == SortableValue(target.clone()) {
                        return Ok(Value::Number(idx as f64));
                    }
                }
                Ok(Value::Number(-1.0))
            }))),
        );

        let st_reverse = state.clone();
        methods.insert(
            "reverse".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_reverse.lock().unwrap().reverse();
                Ok(Value::Null)
            }))),
        );

        let st_sort = state.clone();
        methods.insert(
            "sort".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let mut guard = st_sort.lock().unwrap();
                let mut sortable: std::vec::Vec<SortableValue> = guard
                    .as_slice()
                    .iter()
                    .cloned()
                    .map(SortableValue)
                    .collect();
                sortable.sort();
                guard.clear();
                for sv in sortable {
                    guard.push(sv.0);
                }
                Ok(Value::Null)
            }))),
        );

        let st_iter = state.clone();
        let iter_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_iter.lock().unwrap();
            let items: std::vec::Vec<Value> = guard.as_slice().iter().cloned().collect();
            Ok(Value::Array(items))
        })));
        methods.insert("iter".to_string(), iter_fn.clone());
        methods.insert("toArray".to_string(), iter_fn.clone());
        methods.insert("to_array".to_string(), iter_fn);

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("Vec<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 2. Slice
fn collections_slice_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else {
        None
    };

    if args.is_empty() {
        return Err("Slice(array, start?, end?)".to_string());
    }

    let source = match extract_array(&args[0]) {
        Some(arr) => arr,
        None => return Err("Slice requires an array source".to_string()),
    };

    let start = if args.len() > 1 {
        args[1].as_f64().unwrap_or(0.0) as usize
    } else {
        0
    };
    let end = if args.len() > 2 {
        args[2].as_f64().unwrap_or(source.len() as f64) as usize
    } else {
        source.len()
    };

    let slice_data = if start < source.len() && end <= source.len() && start <= end {
        source[start..end].to_vec()
    } else {
        std::vec::Vec::new()
    };

    if let Some(ref target_t) = elem_constraint {
        for item in &slice_data {
            validate_and_coerce_type(item.clone(), target_t)?;
        }
    }

    let slice_state = Arc::new(Mutex::new(slice_data));
    let mut methods = HashMap::default();

    let elem_t_info = Arc::new(Mutex::new(elem_constraint));
    let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
        let t_name = elem_t_info
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "T".to_string());
        Ok(Value::Str(format!("Slice<{}>", t_name)))
    })));
    methods.insert("typeInfo".to_string(), type_info_fn.clone());
    methods.insert("type_info".to_string(), type_info_fn);

    let st_len = slice_state.clone();
    methods.insert(
        "len".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(Value::Number(st_len.lock().unwrap().len() as f64))
        }))),
    );

    let st_get = slice_state.clone();
    methods.insert(
        "get".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("get(idx)".to_string());
            }
            let idx = args[0].as_f64().unwrap_or(0.0) as usize;
            Ok(st_get
                .lock()
                .unwrap()
                .get(idx)
                .cloned()
                .unwrap_or(Value::Null))
        }))),
    );

    let st_first = slice_state.clone();
    methods.insert(
        "first".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(st_first
                .lock()
                .unwrap()
                .first()
                .cloned()
                .unwrap_or(Value::Null))
        }))),
    );

    let st_last = slice_state.clone();
    methods.insert(
        "last".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(st_last
                .lock()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or(Value::Null))
        }))),
    );

    let st_contains = slice_state.clone();
    methods.insert(
        "contains".to_string(),
        Value::Function(NativeFn(Arc::new(move |_env, args| {
            if args.len() != 1 {
                return Err("contains(val)".to_string());
            }
            let target = &args[0];
            let has = st_contains
                .lock()
                .unwrap()
                .iter()
                .any(|item| SortableValue(item.clone()) == SortableValue(target.clone()));
            Ok(Value::Bool(has))
        }))),
    );

    let st_iter = slice_state.clone();
    let iter_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
        let items = st_iter.lock().unwrap().clone();
        Ok(Value::Array(items))
    })));
    methods.insert("iter".to_string(), iter_fn.clone());
    methods.insert("toArray".to_string(), iter_fn);

    Ok(Value::Object(Arc::new(methods)))
}

// 3. HashMap
fn collections_hashmap_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let (key_constraint, val_constraint) = if type_ctx.len() >= 2 {
        (Some(type_ctx[0].clone()), Some(type_ctx[1].clone()))
    } else if type_ctx.len() == 1 {
        (Some(type_ctx[0].clone()), None)
    } else if args.len() >= 2
        && matches!(&args[0], Value::Str(_))
        && matches!(&args[1], Value::Str(_))
    {
        let k = if let Value::Str(s) = &args[0] {
            s.clone()
        } else {
            String::new()
        };
        let v = if let Value::Str(s) = &args[1] {
            s.clone()
        } else {
            String::new()
        };
        (Some(k), Some(v))
    } else {
        (None, None)
    };

    let map: AllocHashMap<String, Value> = AllocHashMap::new();
    let key_t_opt = Arc::new(Mutex::new(key_constraint));
    let val_t_opt = Arc::new(Mutex::new(val_constraint));

    Ok(wrap_instance(map, move |state, methods| {
        let st_insert = state.clone();
        let kt1 = key_t_opt.clone();
        let vt1 = val_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("insert(key, val)".to_string());
                }
                let key_val = if let Some(kt) = kt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let val_val = if let Some(vt) = vt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), vt)?
                } else {
                    args[1].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                st_insert.lock().unwrap().insert(key_str, val_val);
                Ok(Value::Null)
            }))),
        );

        let st_get = state.clone();
        let kt2 = key_t_opt.clone();
        methods.insert(
            "get".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("get(key)".to_string());
                }
                let key_val = if let Some(kt) = kt2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_get
                    .lock()
                    .unwrap()
                    .get(&key_str)
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_contains = state.clone();
        let kt3 = key_t_opt.clone();
        methods.insert(
            "containsKey".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("containsKey(key)".to_string());
                }
                let key_val = if let Some(kt) = kt3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(Value::Bool(
                    st_contains.lock().unwrap().contains_key(&key_str),
                ))
            }))),
        );

        let st_remove = state.clone();
        let kt4 = key_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(key)".to_string());
                }
                let key_val = if let Some(kt) = kt4.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_remove
                    .lock()
                    .unwrap()
                    .remove(&key_str)
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_keys = state.clone();
        methods.insert(
            "keys".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let guard = st_keys.lock().unwrap();
                let keys_arr: std::vec::Vec<Value> =
                    guard.keys().map(|k| Value::Str(k.clone())).collect();
                Ok(Value::Array(keys_arr))
            }))),
        );

        let st_vals = state.clone();
        methods.insert(
            "values".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let guard = st_vals.lock().unwrap();
                let vals_arr: std::vec::Vec<Value> = guard.values().cloned().collect();
                Ok(Value::Array(vals_arr))
            }))),
        );

        let st_entries = state.clone();
        let entries_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_entries.lock().unwrap();
            let entries: std::vec::Vec<Value> = guard
                .iter()
                .map(|(k, v)| Value::Array(vec![Value::Str(k.clone()), v.clone()]))
                .collect();
            Ok(Value::Array(entries))
        })));
        methods.insert("entries".to_string(), entries_fn.clone());
        methods.insert("iter".to_string(), entries_fn);

        let kt_info = key_t_opt.clone();
        let vt_info = val_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let k_name = kt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "K".to_string());
            let v_name = vt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "V".to_string());
            Ok(Value::Str(format!("HashMap<{}, {}>", k_name, v_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 4. HashSet
fn collections_hashset_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let set: HashSet<String> = HashSet::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(set, move |state, methods| {
        let st_insert = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("insert(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let inserted = st_insert.lock().unwrap().insert(key);
                Ok(Value::Bool(inserted))
            }))),
        );

        let st_contains = state.clone();
        let elem_t2 = elem_t_opt.clone();
        methods.insert(
            "contains".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("contains(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let has = st_contains.lock().unwrap().contains(&key);
                Ok(Value::Bool(has))
            }))),
        );

        let st_remove = state.clone();
        let elem_t3 = elem_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let removed = st_remove.lock().unwrap().remove(&key);
                Ok(Value::Bool(removed))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_vals = state.clone();
        let values_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_vals.lock().unwrap();
            let items: std::vec::Vec<Value> = guard.iter().map(|s| Value::Str(s.clone())).collect();
            Ok(Value::Array(items))
        })));
        methods.insert("values".to_string(), values_fn.clone());
        methods.insert("iter".to_string(), values_fn.clone());
        methods.insert("toArray".to_string(), values_fn);

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("HashSet<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 5. VecDeque
fn collections_vecdeque_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let deque: VecDeque<Value> = VecDeque::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(deque, move |state, methods| {
        let st_push_back = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "pushBack".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("pushBack(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_push_back.lock().unwrap().push_back(val);
                Ok(Value::Null)
            }))),
        );

        let st_push_front = state.clone();
        let elem_t2 = elem_t_opt.clone();
        methods.insert(
            "pushFront".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("pushFront(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_push_front.lock().unwrap().push_front(val);
                Ok(Value::Null)
            }))),
        );

        let st_pop_back = state.clone();
        methods.insert(
            "popBack".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop_back
                    .lock()
                    .unwrap()
                    .pop_back()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_pop_front = state.clone();
        methods.insert(
            "popFront".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop_front
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_front = state.clone();
        methods.insert(
            "front".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_front
                    .lock()
                    .unwrap()
                    .front()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_back = state.clone();
        methods.insert(
            "back".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_back
                    .lock()
                    .unwrap()
                    .back()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_get = state.clone();
        methods.insert(
            "get".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("get(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                Ok(st_get
                    .lock()
                    .unwrap()
                    .get(idx)
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_iter = state.clone();
        let iter_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_iter.lock().unwrap();
            let mut items = std::vec::Vec::new();
            for i in 0..guard.len() {
                if let Some(item) = guard.get(i) {
                    items.push(item.clone());
                }
            }
            Ok(Value::Array(items))
        })));
        methods.insert("iter".to_string(), iter_fn.clone());
        methods.insert("toArray".to_string(), iter_fn);

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("VecDeque<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 6. BTreeMap
fn collections_btreemap_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let (key_constraint, val_constraint) = if type_ctx.len() >= 2 {
        (Some(type_ctx[0].clone()), Some(type_ctx[1].clone()))
    } else if type_ctx.len() == 1 {
        (Some(type_ctx[0].clone()), None)
    } else if args.len() >= 2
        && matches!(&args[0], Value::Str(_))
        && matches!(&args[1], Value::Str(_))
    {
        let k = if let Value::Str(s) = &args[0] {
            s.clone()
        } else {
            String::new()
        };
        let v = if let Value::Str(s) = &args[1] {
            s.clone()
        } else {
            String::new()
        };
        (Some(k), Some(v))
    } else {
        (None, None)
    };

    let map: BTreeMap<String, Value> = BTreeMap::new();
    let key_t_opt = Arc::new(Mutex::new(key_constraint));
    let val_t_opt = Arc::new(Mutex::new(val_constraint));

    Ok(wrap_instance(map, move |state, methods| {
        let st_insert = state.clone();
        let kt1 = key_t_opt.clone();
        let vt1 = val_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("insert(key, val)".to_string());
                }
                let key_val = if let Some(kt) = kt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let val_val = if let Some(vt) = vt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), vt)?
                } else {
                    args[1].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                st_insert.lock().unwrap().insert(key_str, val_val);
                Ok(Value::Null)
            }))),
        );

        let st_get = state.clone();
        let kt2 = key_t_opt.clone();
        methods.insert(
            "get".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("get(key)".to_string());
                }
                let key_val = if let Some(kt) = kt2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_get
                    .lock()
                    .unwrap()
                    .get(&key_str)
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_contains = state.clone();
        let kt3 = key_t_opt.clone();
        methods.insert(
            "containsKey".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("containsKey(key)".to_string());
                }
                let key_val = if let Some(kt) = kt3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(Value::Bool(
                    st_contains.lock().unwrap().get(&key_str).is_some(),
                ))
            }))),
        );

        let st_remove = state.clone();
        let kt4 = key_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(key)".to_string());
                }
                let key_val = if let Some(kt) = kt4.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_remove
                    .lock()
                    .unwrap()
                    .remove(&key_str)
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_keys = state.clone();
        methods.insert(
            "keys".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let guard = st_keys.lock().unwrap();
                let keys_arr: std::vec::Vec<Value> =
                    guard.iter().map(|(k, _v)| Value::Str(k.clone())).collect();
                Ok(Value::Array(keys_arr))
            }))),
        );

        let st_vals = state.clone();
        methods.insert(
            "values".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let guard = st_vals.lock().unwrap();
                let vals_arr: std::vec::Vec<Value> =
                    guard.iter().map(|(_k, v)| v.clone()).collect();
                Ok(Value::Array(vals_arr))
            }))),
        );

        let st_entries = state.clone();
        let entries_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_entries.lock().unwrap();
            let entries: std::vec::Vec<Value> = guard
                .iter()
                .map(|(k, v)| Value::Array(vec![Value::Str(k.clone()), v.clone()]))
                .collect();
            Ok(Value::Array(entries))
        })));
        methods.insert("entries".to_string(), entries_fn.clone());
        methods.insert("iter".to_string(), entries_fn);

        let kt_info = key_t_opt.clone();
        let vt_info = val_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let k_name = kt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "K".to_string());
            let v_name = vt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "V".to_string());
            Ok(Value::Str(format!("BTreeMap<{}, {}>", k_name, v_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 7. BTreeSet
fn collections_btreeset_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let set: BTreeSet<String> = BTreeSet::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(set, move |state, methods| {
        let st_insert = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("insert(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let inserted = st_insert.lock().unwrap().insert(key);
                Ok(Value::Bool(inserted))
            }))),
        );

        let st_contains = state.clone();
        let elem_t2 = elem_t_opt.clone();
        methods.insert(
            "contains".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("contains(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let has = st_contains.lock().unwrap().contains(&key);
                Ok(Value::Bool(has))
            }))),
        );

        let st_remove = state.clone();
        let elem_t3 = elem_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let removed = st_remove.lock().unwrap().remove(&key);
                Ok(Value::Bool(removed))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_vals = state.clone();
        let values_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_vals.lock().unwrap();
            let items: std::vec::Vec<Value> = guard.iter().map(|s| Value::Str(s.clone())).collect();
            Ok(Value::Array(items))
        })));
        methods.insert("values".to_string(), values_fn.clone());
        methods.insert("iter".to_string(), values_fn.clone());
        methods.insert("toArray".to_string(), values_fn);

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("BTreeSet<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 8. BinaryHeap (Max Heap)
fn collections_binaryheap_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let heap: BinaryHeap<SortableValue> = BinaryHeap::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(heap, move |state, methods| {
        let st_push = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "push".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("push(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_push.lock().unwrap().push(SortableValue(val));
                Ok(Value::Null)
            }))),
        );

        let st_pop = state.clone();
        methods.insert(
            "pop".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop
                    .lock()
                    .unwrap()
                    .pop()
                    .map(|sv| sv.0)
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_peek = state.clone();
        methods.insert(
            "peek".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_peek
                    .lock()
                    .unwrap()
                    .peek()
                    .map(|sv| sv.0.clone())
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("BinaryHeap<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 9. PriorityQueue
fn collections_priorityqueue_new(
    _env: &mut dyn BuiltinEnv,
    _args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let (elem_constraint, prio_constraint) = if type_ctx.len() >= 2 {
        (Some(type_ctx[0].clone()), Some(type_ctx[1].clone()))
    } else if type_ctx.len() == 1 {
        (Some(type_ctx[0].clone()), None)
    } else {
        (None, None)
    };

    let queue: PriorityQueue<Value, SortableValue> = PriorityQueue::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));
    let prio_t_opt = Arc::new(Mutex::new(prio_constraint));

    Ok(wrap_instance(queue, move |state, methods| {
        let st_push = state.clone();
        let elem_t1 = elem_t_opt.clone();
        let prio_t1 = prio_t_opt.clone();
        methods.insert(
            "push".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("push(val, priority)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let prio = if let Some(target_p) = prio_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), target_p)?
                } else {
                    args[1].clone()
                };
                st_push.lock().unwrap().push(val, SortableValue(prio));
                Ok(Value::Null)
            }))),
        );

        let st_pop = state.clone();
        methods.insert(
            "pop".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop.lock().unwrap().pop().unwrap_or(Value::Null))
            }))),
        );

        let st_peek = state.clone();
        methods.insert(
            "peek".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_peek
                    .lock()
                    .unwrap()
                    .peek()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let elem_t_info = elem_t_opt.clone();
        let prio_t_info = prio_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            let p_name = prio_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "P".to_string());
            Ok(Value::Str(format!("PriorityQueue<{}, {}>", t_name, p_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 10. BitSet
fn collections_bitset_new(
    _env: &mut dyn BuiltinEnv,
    _args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let bitset = BitSet::new();
    Ok(wrap_instance(bitset, |state, methods| {
        let st_set = state.clone();
        methods.insert(
            "set".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("set(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                st_set.lock().unwrap().set(idx);
                Ok(Value::Null)
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("clear(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                st_clear.lock().unwrap().clear(idx);
                Ok(Value::Null)
            }))),
        );

        let st_test = state.clone();
        methods.insert(
            "test".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("test(idx)".to_string());
                }
                let idx = args[0].as_f64().unwrap_or(0.0) as usize;
                let val = st_test.lock().unwrap().test(idx);
                Ok(Value::Bool(val))
            }))),
        );

        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            Ok(Value::Str("BitSet".to_string()))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 11. RingBuffer
fn collections_ringbuffer_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("RingBuffer(capacity)".to_string());
    }
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else {
        None
    };

    let cap = args[0].as_f64().unwrap_or(4.0) as usize;
    let ring = RingBuffer::new(cap);
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(ring, move |state, methods| {
        let st_push = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "push".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("push(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let overw = st_push.lock().unwrap().push(val);
                Ok(overw.unwrap_or(Value::Null))
            }))),
        );

        let st_pop = state.clone();
        methods.insert(
            "pop".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop.lock().unwrap().pop().unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_cap = state.clone();
        methods.insert(
            "capacity".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_cap.lock().unwrap().capacity() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_full = state.clone();
        methods.insert(
            "isFull".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_full.lock().unwrap().is_full()))
            }))),
        );

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("RingBuffer<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 12. Queue
fn collections_queue_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let queue: AllocQueue<Value> = AllocQueue::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(queue, move |state, methods| {
        let st_enqueue = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "enqueue".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("enqueue(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_enqueue.lock().unwrap().enqueue(val);
                Ok(Value::Null)
            }))),
        );

        let st_dequeue = state.clone();
        methods.insert(
            "dequeue".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_dequeue.lock().unwrap().dequeue().unwrap_or(Value::Null))
            }))),
        );

        let st_front = state.clone();
        methods.insert(
            "front".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_front
                    .lock()
                    .unwrap()
                    .front()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_back = state.clone();
        methods.insert(
            "back".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_back
                    .lock()
                    .unwrap()
                    .back()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("Queue<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 13. Stack
fn collections_stack_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let stack: AllocStack<Value> = AllocStack::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(stack, move |state, methods| {
        let st_push = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "push".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("push(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                st_push.lock().unwrap().push(val);
                Ok(Value::Null)
            }))),
        );

        let st_pop = state.clone();
        methods.insert(
            "pop".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_pop.lock().unwrap().pop().unwrap_or(Value::Null))
            }))),
        );

        let st_peek = state.clone();
        methods.insert(
            "peek".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(st_peek
                    .lock()
                    .unwrap()
                    .peek()
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("Stack<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 14. OrderedMap
fn collections_orderedmap_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let (key_constraint, val_constraint) = if type_ctx.len() >= 2 {
        (Some(type_ctx[0].clone()), Some(type_ctx[1].clone()))
    } else if type_ctx.len() == 1 {
        (Some(type_ctx[0].clone()), None)
    } else if args.len() >= 2
        && matches!(&args[0], Value::Str(_))
        && matches!(&args[1], Value::Str(_))
    {
        let k = if let Value::Str(s) = &args[0] {
            s.clone()
        } else {
            String::new()
        };
        let v = if let Value::Str(s) = &args[1] {
            s.clone()
        } else {
            String::new()
        };
        (Some(k), Some(v))
    } else {
        (None, None)
    };

    let map: AllocOrderedMap<String, Value> = AllocOrderedMap::new();
    let key_t_opt = Arc::new(Mutex::new(key_constraint));
    let val_t_opt = Arc::new(Mutex::new(val_constraint));

    Ok(wrap_instance(map, move |state, methods| {
        let st_insert = state.clone();
        let kt1 = key_t_opt.clone();
        let vt1 = val_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 2 {
                    return Err("insert(key, val)".to_string());
                }
                let key_val = if let Some(kt) = kt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let val_val = if let Some(vt) = vt1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[1].clone(), vt)?
                } else {
                    args[1].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                st_insert.lock().unwrap().insert(key_str, val_val);
                Ok(Value::Null)
            }))),
        );

        let st_get = state.clone();
        let kt2 = key_t_opt.clone();
        methods.insert(
            "get".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("get(key)".to_string());
                }
                let key_val = if let Some(kt) = kt2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_get
                    .lock()
                    .unwrap()
                    .get(&key_str)
                    .cloned()
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_contains = state.clone();
        let kt3 = key_t_opt.clone();
        methods.insert(
            "containsKey".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("containsKey(key)".to_string());
                }
                let key_val = if let Some(kt) = kt3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(Value::Bool(
                    st_contains.lock().unwrap().contains_key(&key_str),
                ))
            }))),
        );

        let st_remove = state.clone();
        let kt4 = key_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(key)".to_string());
                }
                let key_val = if let Some(kt) = kt4.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), kt)?
                } else {
                    args[0].clone()
                };
                let key_str = match &key_val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&key_val),
                };
                Ok(st_remove
                    .lock()
                    .unwrap()
                    .remove(&key_str)
                    .unwrap_or(Value::Null))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_keys = state.clone();
        methods.insert(
            "keys".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                let guard = st_keys.lock().unwrap();
                let keys_arr: std::vec::Vec<Value> =
                    guard.keys().iter().map(|k| Value::Str(k.clone())).collect();
                Ok(Value::Array(keys_arr))
            }))),
        );

        let kt_info = key_t_opt.clone();
        let vt_info = val_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let k_name = kt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "K".to_string());
            let v_name = vt_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "V".to_string());
            Ok(Value::Str(format!("OrderedMap<{}, {}>", k_name, v_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}

// 15. OrderedSet
fn collections_orderedset_new(
    _env: &mut dyn BuiltinEnv,
    args: std::vec::Vec<Value>,
) -> Result<Value, String> {
    let type_ctx = get_generic_type_context();
    let elem_constraint = if !type_ctx.is_empty() {
        Some(type_ctx[0].clone())
    } else if !args.is_empty() && matches!(&args[0], Value::Str(_)) {
        if let Value::Str(s) = &args[0] {
            Some(s.clone())
        } else {
            None
        }
    } else {
        None
    };

    let set: AllocOrderedSet<String> = AllocOrderedSet::new();
    let elem_t_opt = Arc::new(Mutex::new(elem_constraint));

    Ok(wrap_instance(set, move |state, methods| {
        let st_insert = state.clone();
        let elem_t1 = elem_t_opt.clone();
        methods.insert(
            "insert".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("insert(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t1.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let inserted = st_insert.lock().unwrap().insert(key);
                Ok(Value::Bool(inserted))
            }))),
        );

        let st_contains = state.clone();
        let elem_t2 = elem_t_opt.clone();
        methods.insert(
            "contains".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("contains(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t2.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let has = st_contains.lock().unwrap().contains(&key);
                Ok(Value::Bool(has))
            }))),
        );

        let st_remove = state.clone();
        let elem_t3 = elem_t_opt.clone();
        methods.insert(
            "remove".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, args| {
                if args.len() != 1 {
                    return Err("remove(val)".to_string());
                }
                let val = if let Some(target_t) = elem_t3.lock().unwrap().as_deref() {
                    validate_and_coerce_type(args[0].clone(), target_t)?
                } else {
                    args[0].clone()
                };
                let key = match &val {
                    Value::Str(s) => s.clone(),
                    Value::U8(n) => n.to_string(),
                    Value::U16(n) => n.to_string(),
                    Value::U32(n) => n.to_string(),
                    Value::U64(n) => n.to_string(),
                    Value::I8(n) => n.to_string(),
                    Value::I16(n) => n.to_string(),
                    Value::I32(n) => n.to_string(),
                    Value::I64(n) => n.to_string(),
                    Value::F32(n) => n.to_string(),
                    Value::F64(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => crate::execution::runtime::format::fmt(&val),
                };
                let removed = st_remove.lock().unwrap().remove(&key);
                Ok(Value::Bool(removed))
            }))),
        );

        let st_len = state.clone();
        methods.insert(
            "len".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Number(st_len.lock().unwrap().len() as f64))
            }))),
        );

        let st_empty = state.clone();
        methods.insert(
            "isEmpty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                Ok(Value::Bool(st_empty.lock().unwrap().is_empty()))
            }))),
        );

        let st_clear = state.clone();
        methods.insert(
            "clear".to_string(),
            Value::Function(NativeFn(Arc::new(move |_env, _args| {
                st_clear.lock().unwrap().clear();
                Ok(Value::Null)
            }))),
        );

        let st_vals = state.clone();
        let values_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let guard = st_vals.lock().unwrap();
            let items: std::vec::Vec<Value> = guard
                .elements()
                .iter()
                .map(|s| Value::Str(s.clone()))
                .collect();
            Ok(Value::Array(items))
        })));
        methods.insert("values".to_string(), values_fn.clone());
        methods.insert("iter".to_string(), values_fn);

        let elem_t_info = elem_t_opt.clone();
        let type_info_fn = Value::Function(NativeFn(Arc::new(move |_env, _args| {
            let t_name = elem_t_info
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "T".to_string());
            Ok(Value::Str(format!("OrderedSet<{}>", t_name)))
        })));
        methods.insert("typeInfo".to_string(), type_info_fn.clone());
        methods.insert("type_info".to_string(), type_info_fn);
    }))
}
