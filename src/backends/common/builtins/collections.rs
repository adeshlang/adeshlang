//! Collection operations for runtime builtins
//!
//! This module provides builtin functions for working with collections,
//! particularly set operations on arrays. These functions are used to
//! implement set-like behavior in the AdeshLang runtime.
//!
//! ## Set Operations
//!
//! Sets are represented as arrays with unique elements. The following operations
//! are provided:
//!
//! - `runtime_set_union`: Combines two sets, removing duplicates
//! - `runtime_set_intersection`: Returns elements present in both sets
//! - `runtime_set_add`: Adds an element to a set if not already present
//! - `runtime_set_has`: Checks if an element exists in a set
//! - `runtime_set_delete`: Removes an element from a set
//!
//! All operations use value equality comparison via `runtime_values_equal`,
//! which handles type coercion (e.g., Int and Float comparison).

use super::RuntimeValue;

/// Compares two runtime values for equality, with type coercion support
///
/// This function handles comparison between different numeric types (Int/Float),
/// and performs exact equality checks for other types.
fn runtime_values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => (*x as f64) == *y,
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => *x == (*y as f64),
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::String(x), RuntimeValue::String(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => false,
    }
}

/// Computes the union of two sets
///
/// Returns a new array containing all elements from both input sets,
/// with duplicates removed. Elements are compared using value equality.
///
/// # Arguments
///
/// * `args[0]` - First set (Array)
/// * `args[1]` - Second set (Array)
///
/// # Returns
///
/// An array containing all unique elements from both sets. If either argument
/// is missing or not an array, returns an empty array.
pub(crate) fn runtime_set_union(args: &[RuntimeValue]) -> RuntimeValue {
    // args[0] is self (set/array), args[1] is other set
    if args.len() < 2 {
        return RuntimeValue::Array(vec![]);
    }

    let set1 = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let set2 = match &args[1] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    // Create union: all elements from both, no duplicates
    let mut result = set1.clone();
    for elem in set2 {
        let already_in = result.iter().any(|e| runtime_values_equal(e, &elem));
        if !already_in {
            result.push(elem);
        }
    }

    RuntimeValue::Array(result)
}

/// Computes the intersection of two sets
///
/// Returns a new array containing only elements present in both input sets.
///
/// # Arguments
///
/// * `args[0]` - First set (Array)
/// * `args[1]` - Second set (Array)
///
/// # Returns
///
/// An array containing elements that appear in both sets. If either argument
/// is missing or not an array, returns an empty array.
pub(crate) fn runtime_set_intersection(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Array(vec![]);
    }

    let set1 = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let set2 = match &args[1] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    // Create intersection: only elements in both
    let result: Vec<RuntimeValue> = set1
        .into_iter()
        .filter(|e| set2.iter().any(|e2| runtime_values_equal(e, e2)))
        .collect();

    RuntimeValue::Array(result)
}

/// Adds an element to a set
///
/// Returns a new array with the element added if it's not already present.
/// If the element already exists, returns the original set unchanged.
///
/// # Arguments
///
/// * `args[0]` - The set (Array)
/// * `args[1]` - The element to add
///
/// # Returns
///
/// An array with the element added (if not already present). If the first
/// argument is missing or not an array, returns an empty array.
pub(crate) fn runtime_set_add(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Array(vec![]));
    }

    let mut set = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let elem = &args[1];

    // Add if not already in set
    let already_in = set.iter().any(|e| runtime_values_equal(e, elem));
    if !already_in {
        set.push(elem.clone());
    }

    RuntimeValue::Array(set)
}

/// Checks if an element exists in a set
///
/// # Arguments
///
/// * `args[0]` - The set (Array)
/// * `args[1]` - The element to check for
///
/// # Returns
///
/// A boolean indicating whether the element is present in the set.
/// Returns false if arguments are missing or the first argument is not an array.
pub(crate) fn runtime_set_has(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }

    let set = match &args[0] {
        RuntimeValue::Array(arr) => arr,
        _ => return RuntimeValue::Bool(false),
    };

    let elem = &args[1];

    RuntimeValue::Bool(set.iter().any(|e| runtime_values_equal(e, elem)))
}

/// Removes an element from a set
///
/// Returns a new array with all occurrences of the element removed.
///
/// # Arguments
///
/// * `args[0]` - The set (Array)
/// * `args[1]` - The element to remove
///
/// # Returns
///
/// An array with the element removed. If the first argument is missing or
/// not an array, returns an empty array.
pub(crate) fn runtime_set_delete(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Array(vec![]));
    }

    let set = match &args[0] {
        RuntimeValue::Array(arr) => arr.clone(),
        _ => return RuntimeValue::Array(vec![]),
    };

    let elem = &args[1];

    let result: Vec<RuntimeValue> = set
        .into_iter()
        .filter(|e| !runtime_values_equal(e, elem))
        .collect();

    RuntimeValue::Array(result)
}
