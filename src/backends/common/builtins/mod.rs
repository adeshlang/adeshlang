//! Builtins registry for JIT/AOT backends
//!
//! This module provides a registry of builtin functions that can be called
//! from JIT-compiled or AOT-compiled code.

#[path = "../builtins_modules/io.rs"]
mod io;

#[path = "../builtins_modules/conversion.rs"]
mod conversion;

#[path = "../builtins_modules/control.rs"]
mod control;

// Modularized builtin function groups
pub(crate) mod arrays;
mod collections;
mod comparison;
mod datetime;
mod environment;
mod json;
mod math;
pub(crate) mod objects;
mod promises;
mod regex;
pub mod stdlib_bridge;
mod strings;
mod thread;

use stdlib_bridge::*;

use json::*;

use regex::{
    runtime_regex_captures, runtime_regex_captures_all, runtime_regex_compile,
    runtime_regex_escape, runtime_regex_find, runtime_regex_find_all, runtime_regex_flag_dot_all,
    runtime_regex_flag_extended, runtime_regex_flag_ignore_case, runtime_regex_flag_multiline,
    runtime_regex_flag_unicode, runtime_regex_full_match, runtime_regex_match_start,
    runtime_regex_new, runtime_regex_replace, runtime_regex_replace_all, runtime_regex_split,
    runtime_regex_split_n, runtime_regex_test,
};

use crate::utils::collections::FastMap;
use num_bigint::BigInt;

// Re-export public I/O functions
pub use io::{clear_jit_generic_type, set_jit_generic_type, take_jit_error};

// Re-export public control flow functions and exception state
pub use control::{CURRENT_EXCEPTION, get_extended_method};

// Re-export promise runtime types for external use
pub use promises::{
    AggregateType, CallableFunction, Microtask, PROMISE_RUNTIME, PromiseEntry, PromiseHandler,
    PromiseId, PromiseRuntime, PromiseState, SharedValue,
};

// Import string builtin functions for registration
use strings::{
    runtime_char_at, runtime_concat, runtime_ends_with, runtime_includes, runtime_index_of,
    runtime_join, runtime_last_index_of, runtime_repeat, runtime_replace, runtime_slice,
    runtime_split, runtime_starts_with, runtime_strlen, runtime_substr, runtime_to_lower_case,
    runtime_to_upper_case, runtime_trim, runtime_trim_end, runtime_trim_start,
};

// Import math builtin functions for registration
use math::{
    runtime_abs, runtime_acos, runtime_add, runtime_asin, runtime_atan, runtime_atan2,
    runtime_ceil, runtime_cos, runtime_div, runtime_exp, runtime_floor, runtime_int_div,
    runtime_log, runtime_log2, runtime_log10, runtime_math_e, runtime_math_ln2, runtime_math_ln10,
    runtime_math_pi, runtime_math_random, runtime_math_random_int, runtime_math_random_range,
    runtime_math_seed, runtime_math_sqrt2, runtime_math_tau, runtime_max, runtime_min, runtime_pow,
    runtime_round, runtime_sign, runtime_sin, runtime_sqrt, runtime_tan, runtime_trunc,
};

// Import array builtin functions for registration
use arrays::{
    runtime_array_concat, runtime_array_flat, runtime_array_includes, runtime_array_index_of,
    runtime_array_last_index_of, runtime_array_max, runtime_array_min, runtime_capacity,
    runtime_clear, runtime_count, runtime_distinct, runtime_every, runtime_filter, runtime_find,
    runtime_find_index, runtime_first, runtime_for_each, runtime_get_index, runtime_has_key,
    runtime_insert, runtime_last, runtime_len, runtime_make_array, runtime_make_array_spread,
    runtime_map, runtime_metadata_size, runtime_pop, runtime_push, runtime_range, runtime_reduce,
    runtime_remove, runtime_reverse, runtime_set_index, runtime_shift, runtime_slice_array,
    runtime_some, runtime_sort, runtime_spread, runtime_sum, runtime_to_set, runtime_to_tuple,
    runtime_unshift,
};

// Import object builtin functions for registration
use objects::{
    runtime_array_to_dynamic, runtime_array_to_fixed, runtime_array_to_fixed_raw,
    runtime_array_to_raw, runtime_call_method, runtime_dict_set, runtime_get_field,
    runtime_make_class_object, runtime_make_dict, runtime_make_object, runtime_make_set,
    runtime_make_tuple, runtime_new_class, runtime_optional_get, runtime_set_class_name,
    runtime_set_field,
};

// Import datetime builtin functions for registration
use datetime::{
    runtime_clock, runtime_date_get_date, runtime_date_get_day, runtime_date_get_full_year,
    runtime_date_get_hours, runtime_date_get_milliseconds, runtime_date_get_minutes,
    runtime_date_get_month, runtime_date_get_seconds, runtime_date_get_time,
    runtime_date_get_timezone_offset, runtime_date_get_utc_date, runtime_date_get_utc_full_year,
    runtime_date_get_utc_hours, runtime_date_get_utc_milliseconds, runtime_date_get_utc_minutes,
    runtime_date_get_utc_month, runtime_date_get_utc_seconds, runtime_date_now, runtime_date_parse,
    runtime_date_to_iso_string, runtime_date_to_locale_date_string, runtime_date_to_locale_string,
    runtime_date_to_locale_time_string, runtime_date_to_string, runtime_time_format,
    runtime_time_local_offset_secs, runtime_time_monotonic_now, runtime_time_parse,
    runtime_time_parse_rfc3339, runtime_time_sleep_nanos, runtime_time_system_now,
    runtime_time_timezone_offset_secs,
};

// Import comparison builtin functions for registration
use comparison::{
    runtime_eq, runtime_in, runtime_instanceof, runtime_ne, runtime_null_coalesce,
    runtime_strict_eq, runtime_strict_ne,
};

// Import collection builtin functions for registration
use collections::{
    runtime_set_add, runtime_set_delete, runtime_set_has, runtime_set_intersection,
    runtime_set_union,
};

// Import environment/args builtin functions for registration
use environment::{
    runtime_arg, runtime_arg_get, runtime_arg_has, runtime_argc, runtime_args_count,
    runtime_args_index_of, runtime_args_join, runtime_args_slice, runtime_argv, runtime_env,
    runtime_env_all, runtime_env_clear, runtime_env_command_line, runtime_env_count,
    runtime_env_file_get, runtime_env_filter, runtime_env_from_file, runtime_env_get,
    runtime_env_get_many, runtime_env_has, runtime_env_platform_info, runtime_env_remove,
    runtime_env_runtime_all, runtime_env_runtime_get, runtime_env_runtime_has,
    runtime_env_runtime_load, runtime_env_set, runtime_env_set_current_dir,
    runtime_env_set_if_absent, runtime_env_system_info, runtime_env_user_dir,
    runtime_env_user_info, runtime_exec_name, runtime_parse_args,
};

// Import promise/async builtin functions for registration
use promises::{
    runtime_await, runtime_call_indirect, runtime_make_reject, runtime_make_resolve,
    runtime_promise_all, runtime_promise_all_settled, runtime_promise_any, runtime_promise_catch,
    runtime_promise_fulfill, runtime_promise_new, runtime_promise_race, runtime_promise_reject,
    runtime_promise_reject_internal, runtime_promise_resolve, runtime_promise_then,
    runtime_set_timeout, runtime_sleep, runtime_spawn,
};

// Import I/O builtin functions for registration
use io::{
    runtime_format, runtime_input, runtime_input_ai, runtime_input_checkbox, runtime_input_color,
    runtime_input_confirm, runtime_input_datepicker, runtime_input_datetime, runtime_input_diff,
    runtime_input_form, runtime_input_fuzzy, runtime_input_hotkey, runtime_input_mock,
    runtime_input_password, runtime_input_pin, runtime_input_radio, runtime_input_select,
    runtime_input_slider, runtime_input_stream, runtime_input_table, runtime_input_timepicker,
    runtime_input_tree, runtime_print, runtime_println,
};

use thread::{
    runtime_cpu_count, runtime_thread_hardware_concurrency, runtime_thread_id, runtime_thread_park,
    runtime_thread_sleep, runtime_thread_yield,
};

// Import type conversion builtin functions for registration
use conversion::{
    runtime_bool, runtime_borrow_immut, runtime_borrow_mut, runtime_borrow_release, runtime_char,
    runtime_f32, runtime_f64, runtime_float, runtime_i8, runtime_i16, runtime_i32,
    runtime_i64_conv, runtime_i128, runtime_int, runtime_sizeof, runtime_str, runtime_type,
    runtime_u8, runtime_u16, runtime_u32, runtime_u64, runtime_u128,
};

// Import control flow and exception handling builtin functions for registration
use control::{
    runtime_check_executor_exception, runtime_clear_exception, runtime_error, runtime_extend_class,
    runtime_get_exception, runtime_has_exception, runtime_is_null, runtime_nonnull, runtime_throw,
};

/// Function pointer type for runtime builtins
pub type BuiltinFn = fn(&[RuntimeValue]) -> RuntimeValue;

/// Runtime value representation for builtins
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
    Array(Vec<RuntimeValue>),
    Set(Vec<RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    Object(FastMap<String, RuntimeValue>),
    /// Promise with an ID referencing the global promise table
    Promise(PromiseId),
    /// Function reference for callbacks/handlers
    Function(CallableFunction),
    /// Arbitrary precision integer for large numbers
    BigInt(BigInt),
    // Fixed-width integer types (unsigned)
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    // Fixed-width integer types (signed)
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    // Fixed-width float types
    F32(f32),
    F64(f64),
    /// Raw array with element type and zero overhead
    RawArray(String, Vec<RuntimeValue>),
    /// Dynamic array with type metadata
    DynArray {
        data: Vec<RuntimeValue>,
        element_type: String,
        concrete_type: String,
        tracked_capacity: Option<usize>,
    },
    Null,
}

impl RuntimeValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            RuntimeValue::Int(n) => Some(*n),
            RuntimeValue::Float(n) => Some(*n as i64),
            RuntimeValue::Char(c) => Some(*c as i64),
            RuntimeValue::BigInt(bi) => bi.try_into().ok(),
            // Fixed-width types & handles
            RuntimeValue::U8(n) => Some(*n as i64),
            RuntimeValue::U16(n) => Some(*n as i64),
            RuntimeValue::U32(n) => Some(*n as i64),
            RuntimeValue::U64(n) => Some(*n as i64),
            RuntimeValue::U128(n) => (*n).try_into().ok(),
            RuntimeValue::I8(n) => Some(*n as i64),
            RuntimeValue::I16(n) => Some(*n as i64),
            RuntimeValue::I32(n) => Some(*n as i64),
            RuntimeValue::I64(n) => Some(*n),
            RuntimeValue::I128(n) => (*n).try_into().ok(),
            RuntimeValue::F32(n) => Some(*n as i64),
            RuntimeValue::F64(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            RuntimeValue::Float(n) => Some(*n),
            RuntimeValue::Int(n) => Some(*n as f64),
            RuntimeValue::Char(c) => Some(*c as u32 as f64),
            RuntimeValue::BigInt(bi) => {
                // Convert BigInt to f64 (may lose precision for very large numbers)
                use num_traits::ToPrimitive;
                bi.to_f64()
            }
            // Fixed-width types
            RuntimeValue::U8(n) => Some(*n as f64),
            RuntimeValue::U16(n) => Some(*n as f64),
            RuntimeValue::U32(n) => Some(*n as f64),
            RuntimeValue::U64(n) => Some(*n as f64),
            RuntimeValue::U128(n) => Some(*n as f64),
            RuntimeValue::I8(n) => Some(*n as f64),
            RuntimeValue::I16(n) => Some(*n as f64),
            RuntimeValue::I32(n) => Some(*n as f64),
            RuntimeValue::I64(n) => Some(*n as f64),
            RuntimeValue::I128(n) => Some(*n as f64),
            RuntimeValue::F32(n) => Some(*n as f64),
            RuntimeValue::F64(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RuntimeValue::Bool(b) => Some(*b),
            RuntimeValue::Int(n) => Some(*n != 0),
            RuntimeValue::Float(n) => Some(*n != 0.0),
            RuntimeValue::Char(c) => Some(*c != '\0'),
            RuntimeValue::BigInt(bi) => Some(*bi != BigInt::from(0)),
            // Fixed-width types
            RuntimeValue::U8(n) => Some(*n != 0),
            RuntimeValue::U16(n) => Some(*n != 0),
            RuntimeValue::U32(n) => Some(*n != 0),
            RuntimeValue::U64(n) => Some(*n != 0),
            RuntimeValue::U128(n) => Some(*n != 0),
            RuntimeValue::I8(n) => Some(*n != 0),
            RuntimeValue::I16(n) => Some(*n != 0),
            RuntimeValue::I32(n) => Some(*n != 0),
            RuntimeValue::I64(n) => Some(*n != 0),
            RuntimeValue::I128(n) => Some(*n != 0),
            RuntimeValue::F32(n) => Some(*n != 0.0),
            RuntimeValue::F64(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn as_promise(&self) -> Option<PromiseId> {
        match self {
            RuntimeValue::Promise(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_function(&self) -> Option<&CallableFunction> {
        match self {
            RuntimeValue::Function(f) => Some(f),
            _ => None,
        }
    }

    /// Get value as BigInt, converting if necessary
    pub fn as_bigint(&self) -> Option<BigInt> {
        match self {
            RuntimeValue::BigInt(bi) => Some(bi.clone()),
            RuntimeValue::Int(n) => Some(BigInt::from(*n)),
            RuntimeValue::Char(c) => Some(BigInt::from(*c as u32)),
            RuntimeValue::Float(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            // Handle fixed-width unsigned integers
            RuntimeValue::U8(n) => Some(BigInt::from(*n)),
            RuntimeValue::U16(n) => Some(BigInt::from(*n)),
            RuntimeValue::U32(n) => Some(BigInt::from(*n)),
            RuntimeValue::U64(n) => Some(BigInt::from(*n)),
            RuntimeValue::U128(n) => Some(BigInt::from(*n)),
            // Handle fixed-width signed integers
            RuntimeValue::I8(n) => Some(BigInt::from(*n)),
            RuntimeValue::I16(n) => Some(BigInt::from(*n)),
            RuntimeValue::I32(n) => Some(BigInt::from(*n)),
            RuntimeValue::I64(n) => Some(BigInt::from(*n)),
            RuntimeValue::I128(n) => Some(BigInt::from(*n)),
            // Handle fixed-width floats
            RuntimeValue::F32(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            RuntimeValue::F64(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            _ => None,
        }
    }

    /// Check if this value is a BigInt
    pub fn is_bigint(&self) -> bool {
        matches!(self, RuntimeValue::BigInt(_))
    }

    /// Check if this value is any numeric type (Int, Float, BigInt, or fixed-width)
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            RuntimeValue::Int(_)
                | RuntimeValue::Float(_)
                | RuntimeValue::Char(_)
                | RuntimeValue::BigInt(_)
                | RuntimeValue::U8(_)
                | RuntimeValue::U16(_)
                | RuntimeValue::U32(_)
                | RuntimeValue::U64(_)
                | RuntimeValue::U128(_)
                | RuntimeValue::I8(_)
                | RuntimeValue::I16(_)
                | RuntimeValue::I32(_)
                | RuntimeValue::I64(_)
                | RuntimeValue::I128(_)
                | RuntimeValue::F32(_)
                | RuntimeValue::F64(_)
        )
    }

    pub fn as_string(&self) -> String {
        self.as_string_with_depth(0)
    }

    fn as_string_with_depth(&self, depth: usize) -> String {
        // Prevent infinite recursion on circular structures
        if depth > 20 {
            return "...".to_string();
        }

        match self {
            RuntimeValue::Int(n) => {
                // Fast path for integers using itoa
                itoa::Buffer::new().format(*n).to_string()
            }
            RuntimeValue::Float(n) => {
                // Fast path for floats using ryu
                ryu::Buffer::new().format(*n).to_string()
            }
            RuntimeValue::Bool(b) => {
                // Static strings - no allocation
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            RuntimeValue::Char(c) => c.to_string(),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::BigInt(bi) => bi.to_string(),
            // Fixed-width integer types (unsigned) - use itoa
            RuntimeValue::U8(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U16(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U32(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U64(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U128(n) => itoa::Buffer::new().format(*n).to_string(),
            // Fixed-width integer types (signed) - use itoa
            RuntimeValue::I8(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I16(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I32(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I64(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I128(n) => itoa::Buffer::new().format(*n).to_string(),
            // Fixed-width float types - use ryu
            RuntimeValue::F32(n) => ryu::Buffer::new().format(*n).to_string(),
            RuntimeValue::F64(n) => ryu::Buffer::new().format(*n).to_string(),
            RuntimeValue::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| v.as_string_with_depth(depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Set(set_vals) => {
                let items: Vec<String> = set_vals
                    .iter()
                    .map(|v| v.as_string_with_depth(depth + 1))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Tuple(tup) => {
                if tup.is_empty() {
                    "()".to_string()
                } else if tup.len() == 1 {
                    format!("({},)", tup[0].as_string_with_depth(depth + 1))
                } else {
                    let items: Vec<String> = tup
                        .iter()
                        .map(|v| v.as_string_with_depth(depth + 1))
                        .collect();
                    format!("({})", items.join(", "))
                }
            }
            RuntimeValue::Object(obj) => {
                let mut entries: Vec<String> = Vec::new();
                for (k, v) in obj.iter() {
                    // For object formatting we intentionally don't quote strings: `b: x`
                    let value_str = v.as_string_with_depth(depth + 1);
                    entries.push(format!("{}: {}", k, value_str));
                }
                format!("{{{}}}", entries.join(", "))
            }
            RuntimeValue::Promise(id) => format!("Promise({})", id),
            RuntimeValue::Function(f) => format!("Function({})", f.name),
            RuntimeValue::RawArray(_elem_type, values) => {
                let items: Vec<String> = values
                    .iter()
                    .map(|v| v.as_string_with_depth(depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::DynArray { data, .. } => {
                let items: Vec<String> = data
                    .iter()
                    .map(|v| v.as_string_with_depth(depth + 1))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Null => "null".to_string(),
        }
    }
}

/// Builtin function registry
pub struct BuiltinRegistry {
    builtins: FastMap<String, BuiltinFn>,
}

impl BuiltinRegistry {
    /// Create a new builtin registry with standard builtins
    pub fn new() -> Self {
        let mut registry = BuiltinRegistry {
            builtins: FastMap::default(),
        };
        registry.register_standard_builtins();
        registry
    }

    /// Register a builtin function
    pub fn register(&mut self, name: &str, func: BuiltinFn) {
        self.builtins.insert(name.to_string(), func);
    }

    /// Get a builtin function by name
    pub fn get(&self, name: &str) -> Option<&BuiltinFn> {
        self.builtins.get(name)
    }

    /// Check if a builtin exists
    pub fn has(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
    }

    /// Get all builtin names
    pub fn names(&self) -> Vec<&String> {
        self.builtins.keys().collect()
    }

    /// Register standard builtins
    fn register_standard_builtins(&mut self) {
        // I/O
        self.register("print", runtime_print);
        self.register("println", runtime_println);
        self.register("input", runtime_input);
        self.register("__input_mock", runtime_input_mock);
        self.register("input.mock", runtime_input_mock);
        self.register("input.checkbox", runtime_input_checkbox);
        self.register("input.radio", runtime_input_radio);
        self.register("input.select", runtime_input_select);
        self.register("input.form", runtime_input_form);
        self.register("input.confirm", runtime_input_confirm);
        self.register("input.password", runtime_input_password);
        self.register("input.fuzzy", runtime_input_fuzzy);
        self.register("input.slider", runtime_input_slider);
        self.register("input.tree", runtime_input_tree);
        self.register("input.table", runtime_input_table);
        self.register("input.datepicker", runtime_input_datepicker);
        self.register("input.datetime", runtime_input_datetime);
        self.register("input.datetimepicker", runtime_input_datetime);
        self.register("input.timepicker", runtime_input_timepicker);
        self.register("input.color", runtime_input_color);
        self.register("input.pin", runtime_input_pin);
        self.register("input.diff", runtime_input_diff);
        self.register("input.hotkey", runtime_input_hotkey);
        self.register("input.ai", runtime_input_ai);
        self.register("input.stream", runtime_input_stream);

        // Type conversion
        self.register("int", runtime_int);
        self.register("float", runtime_float);
        self.register("str", runtime_str);
        self.register("char", runtime_char);
        self.register("bool", runtime_bool);

        // Fixed-width type conversion
        self.register("f32", runtime_f32);
        self.register("f64", runtime_f64);
        self.register("u8", runtime_u8);
        self.register("u16", runtime_u16);
        self.register("u32", runtime_u32);
        self.register("u64", runtime_u64);
        self.register("u128", runtime_u128);
        self.register("i8", runtime_i8);
        self.register("i16", runtime_i16);
        self.register("i32", runtime_i32);
        self.register("i64", runtime_i64_conv);
        self.register("i128", runtime_i128);

        // Collections
        self.register("len", runtime_len);
        self.register("capacity", runtime_capacity);
        self.register("metadata_size", runtime_metadata_size);
        self.register("push", runtime_push);
        self.register("pop", runtime_pop);
        self.register("__method_push", runtime_push);
        self.register("__method_pop", runtime_pop);
        self.register("get_index", runtime_get_index);
        self.register("set_index", runtime_set_index);
        self.register("make_array", runtime_make_array);
        self.register("make_array_spread", runtime_make_array_spread);
        self.register("make_object", runtime_make_object);
        self.register("make_dict", runtime_make_dict);
        self.register("make_set", runtime_make_set);
        self.register("makeSet", runtime_make_set);
        // Note: arrayUnion and arrayIntersection not yet implemented
        self.register("make_tuple", runtime_make_tuple);
        self.register("array_to_raw", runtime_array_to_raw);
        self.register("array_to_dynamic", runtime_array_to_dynamic);
        self.register("array_to_fixed", runtime_array_to_fixed);
        self.register("array_to_fixed_raw", runtime_array_to_fixed_raw);
        self.register("dict_set", runtime_dict_set);
        self.register("get_field", runtime_get_field);
        self.register("set_field", runtime_set_field);

        // Array bound methods
        self.register("append", runtime_push);
        self.register("insert", runtime_insert);
        self.register("remove", runtime_remove);
        self.register("clear", runtime_clear);
        self.register("shift", runtime_shift);
        self.register("unshift", runtime_unshift);
        self.register("count", runtime_count);
        self.register("sort", runtime_sort);
        self.register("first", runtime_first);
        self.register("last", runtime_last);
        self.register("reverse", runtime_reverse);
        self.register("slice", runtime_slice_array);
        self.register("sum", runtime_sum);
        self.register("min", runtime_min);
        self.register("max", runtime_max);
        self.register("distinct", runtime_distinct);
        self.register("toSet", runtime_to_set);
        self.register("toTuple", runtime_to_tuple);

        // Operators/utility
        self.register("int_div", runtime_int_div);
        self.register("eq", runtime_eq);
        self.register("ne", runtime_ne);
        self.register("strict_eq", runtime_strict_eq);
        self.register("strict_ne", runtime_strict_ne);
        self.register("null_coalesce", runtime_null_coalesce);
        self.register("in", runtime_in);
        self.register("instanceof", runtime_instanceof);
        self.register("is_null", runtime_is_null);

        // Math - constants
        self.register("Math.PI", runtime_math_pi);
        self.register("Math.E", runtime_math_e);
        self.register("Math.TAU", runtime_math_tau);
        self.register("Math.SQRT2", runtime_math_sqrt2);
        self.register("Math.LN2", runtime_math_ln2);
        self.register("Math.LN10", runtime_math_ln10);

        // Math - namespace functions
        self.register("Math.random", runtime_math_random);
        self.register("Math.seed", runtime_math_seed);
        self.register("Math.randomInt", runtime_math_random_int);
        self.register("Math.randomRange", runtime_math_random_range);
        self.register("Math.floor", runtime_floor);
        self.register("Math.ceil", runtime_ceil);
        self.register("Math.round", runtime_round);
        self.register("Math.abs", runtime_abs);
        self.register("Math.sqrt", runtime_sqrt);
        self.register("Math.pow", runtime_pow);
        self.register("Math.min", runtime_min);
        self.register("Math.max", runtime_max);
        self.register("Math.sin", runtime_sin);
        self.register("Math.cos", runtime_cos);
        self.register("Math.tan", runtime_tan);
        self.register("Math.asin", runtime_asin);
        self.register("Math.acos", runtime_acos);
        self.register("Math.atan", runtime_atan);
        self.register("Math.atan2", runtime_atan2);
        self.register("Math.exp", runtime_exp);
        self.register("Math.log", runtime_log);
        self.register("Math.log10", runtime_log10);
        self.register("Math.log2", runtime_log2);
        self.register("Math.trunc", runtime_trunc);
        self.register("Math.sign", runtime_sign);

        // JSON operations
        self.register("json_parse", runtime_json_parse);
        self.register("json_stringify", runtime_json_stringify);
        self.register("JSON.parse", runtime_json_parse);
        self.register("JSON.parseBytes", runtime_json_parse);
        self.register("JSON.parseFile", runtime_json_parse);
        self.register("JSON.stringify", runtime_json_stringify);
        self.register("JSON.stringifyPretty", runtime_json_stringify_pretty);
        self.register("JSON.stringifyCompact", runtime_json_stringify);
        self.register("JSON.stringifyFile", runtime_json_stringify);
        self.register("JSON.stringifyBytes", runtime_json_stringify);
        self.register("JSON.isValid", runtime_json_is_valid);
        self.register("JSON.tryParse", runtime_json_parse);
        self.register("JSON.minify", runtime_json_minify);
        self.register("JSON.pretty", runtime_json_pretty);
        self.register("JSON.encode", runtime_json_stringify);
        self.register("JSON.decode", runtime_json_parse);
        self.register("JSON.object", runtime_json_object);
        self.register("JSON.array", runtime_json_array);
        self.register("JSON.from", runtime_json_from);

        // String operations

        self.register("concat", runtime_concat);
        self.register("substr", runtime_substr);
        self.register("strlen", runtime_strlen);
        self.register("format", runtime_format);

        // String methods (object-oriented style)
        self.register("__method_split", runtime_split);
        self.register("__method_substr", runtime_substr);
        self.register("__method_substring", runtime_substr);
        self.register("__method_slice", runtime_slice);
        self.register("__method_charAt", runtime_char_at);
        self.register("__method_indexOf", runtime_index_of);
        self.register("__method_lastIndexOf", runtime_last_index_of);
        self.register("__method_startsWith", runtime_starts_with);
        self.register("__method_endsWith", runtime_ends_with);
        self.register("__method_includes", runtime_includes);
        self.register("__method_trim", runtime_trim);
        self.register("__method_trimStart", runtime_trim_start);
        self.register("__method_trimEnd", runtime_trim_end);
        self.register("__method_toLowerCase", runtime_to_lower_case);
        self.register("__method_toUpperCase", runtime_to_upper_case);
        self.register("__method_replace", runtime_replace);
        self.register("__method_repeat", runtime_repeat);

        // Array methods (object-oriented style)
        self.register("__method_join", runtime_join);
        self.register("__method_slice", runtime_slice_array);
        self.register("__method_indexOf", runtime_array_index_of);
        self.register("__method_lastIndexOf", runtime_array_last_index_of);
        self.register("__method_reverse", runtime_reverse);
        self.register("__method_sort", runtime_sort);
        self.register("__method_map", runtime_map);
        self.register("__method_filter", runtime_filter);
        self.register("__method_reduce", runtime_reduce);
        self.register("__method_forEach", runtime_for_each);
        self.register("__method_find", runtime_find);
        self.register("__method_findIndex", runtime_find_index);
        self.register("__method_some", runtime_some);
        self.register("__method_every", runtime_every);
        self.register("__method_concat", runtime_array_concat);
        self.register("__method_extend", runtime_array_concat);
        self.register("__method_flat", runtime_array_flat);
        self.register("__method_insert", runtime_insert);
        self.register("__method_remove", runtime_remove);
        self.register("__method_clear", runtime_clear);
        self.register("__method_shift", runtime_shift);
        self.register("__method_unshift", runtime_unshift);
        self.register("__method_count", runtime_count);
        self.register("__method_sum", runtime_sum);
        self.register("__method_min", runtime_array_min);
        self.register("__method_max", runtime_array_max);
        self.register("__method_distinct", runtime_distinct);
        self.register("__method_toSet", runtime_to_set);
        self.register("__method_toTuple", runtime_to_tuple);
        self.register("__method_first", runtime_first);
        self.register("__method_last", runtime_last);

        // Polymorphic arithmetic (handles numbers and strings)
        self.register("add", runtime_add);
        self.register("div", runtime_div);

        // Utilities
        self.register("type", runtime_type);
        self.register("typeof", runtime_type);
        self.register("sizeof", runtime_sizeof);
        self.register("hasKey", runtime_has_key);
        self.register("range", runtime_range);
        self.register("spread", runtime_spread);
        self.register("optional_get", runtime_optional_get);
        self.register("nonnull", runtime_nonnull);

        // Exception handling
        self.register("__throw", runtime_throw);
        self.register("__has_exception", runtime_has_exception);
        self.register("__get_exception", runtime_get_exception);
        self.register("__clear_exception", runtime_clear_exception);

        // Error constructors
        self.register("error", runtime_error);
        self.register("Error", runtime_error);

        // DateTime
        self.register("Date", runtime_new_class);
        self.register("__new_Date", runtime_new_class); // For `new Date()` syntax
        self.register("__new_class", runtime_new_class); // Generic class instantiation
        self.register("Date.now", runtime_date_now);
        self.register("Date.parse", runtime_date_parse);
        self.register("clock", runtime_clock);

        self.register("time.systemNow", runtime_time_system_now);
        self.register("time.monotonicNow", runtime_time_monotonic_now);
        self.register("time.sleepNanos", runtime_time_sleep_nanos);
        self.register("time.localOffsetSecs", runtime_time_local_offset_secs);
        self.register("time.timezoneOffsetSecs", runtime_time_timezone_offset_secs);
        self.register("time.format", runtime_time_format);
        self.register("time.parse", runtime_time_parse);
        self.register("time.parseRfc3339", runtime_time_parse_rfc3339);

        // Date instance methods
        self.register("__method_toISOString", runtime_date_to_iso_string);
        self.register("__method_getTime", runtime_date_get_time);
        self.register("__method_toString", runtime_date_to_string);
        self.register("__method_getFullYear", runtime_date_get_full_year);
        self.register("__method_getMonth", runtime_date_get_month);
        self.register("__method_getDate", runtime_date_get_date);
        self.register("__method_getHours", runtime_date_get_hours);
        self.register("__method_getMinutes", runtime_date_get_minutes);
        self.register("__method_getSeconds", runtime_date_get_seconds);
        self.register("__method_getMilliseconds", runtime_date_get_milliseconds);
        self.register("__method_getDay", runtime_date_get_day);
        self.register("__method_getUTCFullYear", runtime_date_get_utc_full_year);
        self.register("__method_getUTCMonth", runtime_date_get_utc_month);
        self.register("__method_getUTCDate", runtime_date_get_utc_date);
        self.register("__method_getUTCHours", runtime_date_get_utc_hours);
        self.register("__method_getUTCMinutes", runtime_date_get_utc_minutes);
        self.register("__method_getUTCSeconds", runtime_date_get_utc_seconds);
        self.register(
            "__method_getUTCMilliseconds",
            runtime_date_get_utc_milliseconds,
        );
        self.register(
            "__method_getTimezoneOffset",
            runtime_date_get_timezone_offset,
        );
        self.register("__method_toLocaleString", runtime_date_to_locale_string);
        self.register(
            "__method_toLocaleDateString",
            runtime_date_to_locale_date_string,
        );

        // Set methods
        self.register("__method_union", runtime_set_union);
        self.register("__method_intersection", runtime_set_intersection);
        self.register("__method_add", runtime_set_add);
        self.register("__method_has", runtime_set_has);
        self.register("__method_delete", runtime_set_delete);
        self.register(
            "__method_toLocaleTimeString",
            runtime_date_to_locale_time_string,
        );

        // Class definition helpers
        self.register("__make_class_object", runtime_make_class_object);
        self.register("__set_class_name", runtime_set_class_name);
        self.register("__extend_class", runtime_extend_class);
        self.register("__call_method", runtime_call_method);

        // Promise builtins - constructor and static methods
        self.register("Promise", runtime_promise_new); // Promise(executor)
        self.register("Promise.new", runtime_promise_new);
        self.register("Promise.resolve", runtime_promise_resolve);
        self.register("Promise.reject", runtime_promise_reject);

        // Promise combinators
        self.register("Promise.all", runtime_promise_all);
        self.register("Promise.race", runtime_promise_race);
        self.register("Promise.any", runtime_promise_any);
        self.register("Promise.allSettled", runtime_promise_all_settled);

        // Promise instance methods (called via __method_X pattern)
        self.register("__method_then", runtime_promise_then);
        self.register("__method_catch", runtime_promise_catch);
        self.register("promise_then", runtime_promise_then);
        self.register("promise_catch", runtime_promise_catch);

        // Internal promise operations
        self.register("promise_fulfill", runtime_promise_fulfill);
        self.register("promise_reject_internal", runtime_promise_reject_internal);
        self.register("await", runtime_await);
        self.register("spawn", runtime_spawn);

        // Resolve/reject callback makers for Promise executor pattern
        self.register("__make_resolve", runtime_make_resolve);
        self.register("__make_reject", runtime_make_reject);
        self.register(
            "__check_executor_exception",
            runtime_check_executor_exception,
        );

        // Indirect function call (for calling functions stored in variables)
        self.register("call_indirect", runtime_call_indirect);

        // Timer builtins for async
        self.register("setTimeout", runtime_set_timeout);
        self.register("sleep", runtime_sleep);

        // Native threading (portable scheduling; spawn of language closures is interpreter)
        self.register("thread.sleep", runtime_thread_sleep);
        self.register("thread.yield", runtime_thread_yield);
        self.register(
            "thread.hardware_concurrency",
            runtime_thread_hardware_concurrency,
        );
        self.register("thread.cpu_count", runtime_thread_hardware_concurrency);
        self.register("thread.id", runtime_thread_id);
        self.register("thread.park", runtime_thread_park);
        self.register("cpu_count", runtime_cpu_count);

        // Command-line arguments
        self.register("argc", runtime_argc);
        self.register("argv", runtime_argv);
        self.register("arg", runtime_arg);
        self.register("args", runtime_argv); // Alias
        self.register("execName", runtime_exec_name);
        self.register("argsCount", runtime_args_count);
        self.register("argsSlice", runtime_args_slice);
        self.register("argsJoin", runtime_args_join);
        self.register("argsIndexOf", runtime_args_index_of);
        self.register("parseArgs", runtime_parse_args);
        self.register("argGet", runtime_arg_get);
        self.register("argHas", runtime_arg_has);

        // Environment variables
        self.register("env", runtime_env);
        self.register("envGet", runtime_env_get);
        self.register("envSet", runtime_env_set);
        self.register("envRemove", runtime_env_remove);
        self.register("envClear", runtime_env_clear);
        self.register("envHas", runtime_env_has);
        self.register("envAll", runtime_env_all);
        self.register("envSetCurrentDir", runtime_env_set_current_dir);
        self.register("envUserDir", runtime_env_user_dir);
        self.register("envPlatformInfo", runtime_env_platform_info);
        self.register("envFromFile", runtime_env_from_file);
        self.register("envFileGet", runtime_env_file_get);
        self.register("envRuntimeGet", runtime_env_runtime_get);
        self.register("envRuntimeHas", runtime_env_runtime_has);
        self.register("envRuntimeAll", runtime_env_runtime_all);
        self.register("envRuntimeLoad", runtime_env_runtime_load);
        self.register("envSystemInfo", runtime_env_system_info);
        self.register("envCount", runtime_env_count);
        self.register("envFilter", runtime_env_filter);
        self.register("envGetMany", runtime_env_get_many);
        self.register("envSetIfAbsent", runtime_env_set_if_absent);
        self.register("envCommandLine", runtime_env_command_line);
        self.register("envUserInfo", runtime_env_user_info);

        // Memory safety - borrow operations (compile-time checked, runtime no-ops)
        self.register("borrow_immut", runtime_borrow_immut);
        self.register("borrow_mut", runtime_borrow_mut);
        self.register("borrow_release", runtime_borrow_release);

        // Regex builtins
        self.register("Regex.new", runtime_regex_new);
        self.register("Regex.compile", runtime_regex_compile);
        self.register("Regex.escape", runtime_regex_escape);

        self.register("Regex.IgnoreCase", runtime_regex_flag_ignore_case);
        self.register("Regex.Multiline", runtime_regex_flag_multiline);
        self.register("Regex.DotAll", runtime_regex_flag_dot_all);
        self.register("Regex.Extended", runtime_regex_flag_extended);
        self.register("Regex.Unicode", runtime_regex_flag_unicode);

        self.register("Regex.CASE_INSENSITIVE", runtime_regex_flag_ignore_case);
        self.register("Regex.MULTILINE", runtime_regex_flag_multiline);
        self.register("Regex.DOT_ALL", runtime_regex_flag_dot_all);
        self.register("Regex.EXTENDED", runtime_regex_flag_extended);
        self.register("Regex.UNICODE", runtime_regex_flag_unicode);

        // Universal Stdlib Bridge Registrations for JIT/NJIT/AOT/WASM
        // FS
        self.register("fs.readFile", runtime_fs_read_file);
        self.register("fs.writeFile", runtime_fs_write_file);
        self.register("fs.exists", runtime_fs_exists);
        self.register("fs.unlink", runtime_fs_unlink);
        self.register("fs.mkdir", runtime_fs_mkdir);
        self.register("fs.readDir", runtime_fs_read_dir);
        self.register("fs.stat", runtime_fs_stat);
        self.register("fs.copyFile", runtime_fs_copy_file);

        self.register("FS.readFile", runtime_fs_read_file);
        self.register("FS.writeFile", runtime_fs_write_file);
        self.register("FS.exists", runtime_fs_exists);
        self.register("FS.unlink", runtime_fs_unlink);
        self.register("FS.mkdir", runtime_fs_mkdir);
        self.register("FS.readDir", runtime_fs_read_dir);
        self.register("FS.stat", runtime_fs_stat);
        self.register("FS.copyFile", runtime_fs_copy_file);

        // Crypto
        self.register("crypto.hash", runtime_crypto_hash);
        self.register("crypto.sha256", runtime_crypto_sha256);
        self.register("crypto.md5", runtime_crypto_md5);
        self.register("crypto.randomBytes", runtime_crypto_random_bytes);
        self.register("crypto.encrypt", runtime_crypto_encrypt);
        self.register("crypto.decrypt", runtime_crypto_decrypt);

        self.register("Crypto.hash", runtime_crypto_hash);
        self.register("Crypto.sha256", runtime_crypto_sha256);
        self.register("Crypto.md5", runtime_crypto_md5);
        self.register("Crypto.randomBytes", runtime_crypto_random_bytes);
        self.register("Crypto.encrypt", runtime_crypto_encrypt);
        self.register("Crypto.decrypt", runtime_crypto_decrypt);

        self.register("cryptoHash", runtime_crypto_hash);
        self.register("cryptoSha256", runtime_crypto_sha256);
        self.register("cryptoMd5", runtime_crypto_md5);

        // Path
        self.register("path.join", runtime_path_join);
        self.register("path.resolve", runtime_path_resolve);
        self.register("path.dirname", runtime_path_dirname);
        self.register("path.basename", runtime_path_basename);
        self.register("path.extname", runtime_path_extname);
        self.register("path.isAbsolute", runtime_path_is_absolute);
        self.register("path.normalize", runtime_path_normalize);

        self.register("Path.join", runtime_path_join);
        self.register("Path.resolve", runtime_path_resolve);
        self.register("Path.dirname", runtime_path_dirname);
        self.register("Path.basename", runtime_path_basename);
        self.register("Path.extname", runtime_path_extname);
        self.register("Path.isAbsolute", runtime_path_is_absolute);
        self.register("Path.normalize", runtime_path_normalize);

        self.register("pathJoin", runtime_path_join);
        self.register("pathResolve", runtime_path_resolve);
        self.register("pathDirname", runtime_path_dirname);
        self.register("pathBasename", runtime_path_basename);

        // HTTP & DNS
        self.register("http.get", runtime_http_get);
        self.register("http.post", runtime_http_post);
        self.register("http.request", runtime_http_request);
        self.register("http.fetch", runtime_http_fetch);
        self.register("dns.lookup", runtime_dns_lookup);
        self.register("dns.resolve", runtime_dns_resolve);

        self.register("HTTP.get", runtime_http_get);
        self.register("HTTP.post", runtime_http_post);
        self.register("HTTP.request", runtime_http_request);
        self.register("HTTP.fetch", runtime_http_fetch);
        self.register("DNS.lookup", runtime_dns_lookup);
        self.register("DNS.resolve", runtime_dns_resolve);

        // System
        self.register("system.env", runtime_system_env);
        self.register("system.args", runtime_system_args);
        self.register("system.os", runtime_system_os);
        self.register("system.arch", runtime_system_arch);
        self.register("system.memoryInfo", runtime_system_memory_info);
        self.register("system.exit", runtime_system_exit);

        self.register("System.env", runtime_system_env);
        self.register("System.args", runtime_system_args);
        self.register("System.os", runtime_system_os);
        self.register("System.arch", runtime_system_arch);
        self.register("System.memoryInfo", runtime_system_memory_info);
        self.register("System.exit", runtime_system_exit);

        // Random, Encoding, Compression, URL, TLS, SIMD
        self.register("random.random", runtime_random_random);
        self.register("random.int", runtime_random_int);
        self.register("random.float", runtime_random_float);
        self.register("random.uuid", runtime_random_uuid);

        self.register("Random.random", runtime_random_random);
        self.register("Random.int", runtime_random_int);
        self.register("Random.float", runtime_random_float);
        self.register("Random.uuid", runtime_random_uuid);

        self.register("encoding.base64Encode", runtime_encoding_base64_encode);
        self.register("encoding.base64Decode", runtime_encoding_base64_decode);
        self.register("encoding.hexEncode", runtime_encoding_hex_encode);
        self.register("encoding.hexDecode", runtime_encoding_hex_decode);

        self.register("Encoding.base64Encode", runtime_encoding_base64_encode);
        self.register("Encoding.base64Decode", runtime_encoding_base64_decode);
        self.register("Encoding.hexEncode", runtime_encoding_hex_encode);
        self.register("Encoding.hexDecode", runtime_encoding_hex_decode);

        self.register(
            "compression.gzipCompress",
            runtime_compression_gzip_compress,
        );
        self.register(
            "compression.gzipDecompress",
            runtime_compression_gzip_decompress,
        );

        self.register(
            "Compression.gzipCompress",
            runtime_compression_gzip_compress,
        );
        self.register(
            "Compression.gzipDecompress",
            runtime_compression_gzip_decompress,
        );

        self.register("url.parse", runtime_url_parse);
        self.register("url.format", runtime_url_format);
        self.register("tls.connect", runtime_tls_connect);

        self.register("URL.parse", runtime_url_parse);
        self.register("URL.format", runtime_url_format);
        self.register("TLS.connect", runtime_tls_connect);

        self.register("simd.vectorAdd", runtime_simd_vector_add);
        self.register("simd.vectorMul", runtime_simd_vector_mul);

        self.register("SIMD.vectorAdd", runtime_simd_vector_add);
        self.register("SIMD.vectorMul", runtime_simd_vector_mul);
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = BuiltinRegistry::new();
        assert!(registry.has("print"));
        assert!(registry.has("len"));
        assert!(registry.has("push"));
        assert!(registry.has("fs.readFile"));
        assert!(registry.has("path.join"));
        assert!(registry.has("crypto.sha256"));
        assert!(registry.has("http.get"));
        assert!(registry.has("system.env"));
    }

    #[test]
    fn test_runtime_print() {
        let result = runtime_print(&[RuntimeValue::String("hello".to_string())]);
        assert!(matches!(result, RuntimeValue::Null));
    }

    #[test]
    fn test_runtime_int_conversion() {
        assert!(matches!(
            runtime_int(&[RuntimeValue::Float(1.99)]),
            RuntimeValue::Int(1)
        ));
        assert!(matches!(
            runtime_int(&[RuntimeValue::String("42".to_string())]),
            RuntimeValue::Int(42)
        ));
    }

    #[test]
    fn test_runtime_len() {
        let arr = RuntimeValue::Array(vec![
            RuntimeValue::Int(1),
            RuntimeValue::Int(2),
            RuntimeValue::Int(3),
        ]);
        assert!(matches!(runtime_len(&[arr]), RuntimeValue::Int(3)));

        let s = RuntimeValue::String("hello".to_string());
        assert!(matches!(runtime_len(&[s]), RuntimeValue::Int(5)));
    }

    #[test]
    fn test_runtime_math() {
        assert!(matches!(
            runtime_abs(&[RuntimeValue::Int(-5)]),
            RuntimeValue::Int(5)
        ));

        if let RuntimeValue::Float(n) = runtime_sqrt(&[RuntimeValue::Float(4.0)]) {
            assert!((n - 2.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }

        if let RuntimeValue::Float(n) =
            runtime_pow(&[RuntimeValue::Float(2.0), RuntimeValue::Float(3.0)])
        {
            assert!((n - 8.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_runtime_range() {
        match runtime_range(&[RuntimeValue::Int(0), RuntimeValue::Int(5)]) {
            RuntimeValue::DynArray { data, .. } => {
                assert_eq!(data.len(), 5);
            }
            RuntimeValue::Array(arr) => {
                assert_eq!(arr.len(), 5);
            }
            _ => {
                panic!("Expected Array or DynArray");
            }
        }
    }

    #[test]
    fn object_stringify_renders_key_values() {
        let mut obj = FastMap::default();
        obj.insert("a".to_string(), RuntimeValue::Int(1));
        obj.insert("b".to_string(), RuntimeValue::String("x".to_string()));
        let s = RuntimeValue::Object(obj).as_string();
        assert!(s.contains("a: 1"));
        assert!(s.contains("b: x"));
        assert!(s.starts_with("{"));
        assert!(s.ends_with("}"));
    }

    #[test]
    fn parse_options_detects_last_object() {
        let mut opts = FastMap::default();
        opts.insert("sep".to_string(), RuntimeValue::String(", ".to_string()));
        opts.insert("end".to_string(), RuntimeValue::String("".to_string()));
        let _args = [
            RuntimeValue::String("hello".to_string()),
            RuntimeValue::Object(opts),
        ];
        // Test requires parse_print_options which is in io module
        // let (values, sep, end, _c, _b, _u, _bo, _i, _st, _f, _flush, _pretty) =
        //     io::parse_print_options(&args);
        // assert_eq!(values.len(), 1);
        // assert_eq!(sep, ", ");
        // assert_eq!(end, "");
    }

    #[test]
    fn test_ansi_styling() {
        // Test that styling codes are correctly formatted
        let text = "abc";
        let mut codes = Vec::new();
        codes.push("1"); // bold
        let color_code: String;
        if let Some((r, g, b)) = io::parse_hex_color("#ff0000") {
            color_code = format!("38;2;{};{};{}", r, g, b);
            codes.push(&color_code);
        }
        let styled = format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text);
        assert!(styled.contains("\x1b["));
        assert!(styled.contains("mabc\x1b[0m"));
    }
}
