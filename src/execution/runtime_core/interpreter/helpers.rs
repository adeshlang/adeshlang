//! Helper utilities for the interpreter runtime.
//!
//! Contains utility functions for FFI wrapping and error formatting.

use crate::backends::common::ffi::import::{ForeignFunction, call_foreign};
use crate::parsing::ast::{NativeFn, Value};
use std::sync::Arc;

/// Wrap ForeignFunction as a Value::Function callable.
///
/// This creates a native function value that bridges to FFI calls.
pub(in crate::execution::runtime_core) fn wrap_foreign_function(func: ForeignFunction) -> Value {
    Value::Function(NativeFn(Arc::new(move |_, args| {
        call_foreign(&func, &args)
    })))
}

/// Format input type conversion errors with color and details.
///
/// Creates a formatted error message for input<T>() type conversion failures.
pub(in crate::execution::runtime_core) fn format_input_type_error(
    type_name: &str,
    input_value: &str,
    reason: &str,
) -> String {
    use colored::Colorize;

    let error_header = format!("❌ input<{}> Type Conversion Error", type_name)
        .red()
        .bold();
    let input_line = format!("  Input value: {}", input_value.yellow());
    let reason_line = format!("  Reason: {}", reason);
    let type_hint = format!("  Type: {} (generic parameter)", type_name).cyan();

    format!(
        "{}\n{}\n{}\n{}",
        error_header, input_line, reason_line, type_hint
    )
}
