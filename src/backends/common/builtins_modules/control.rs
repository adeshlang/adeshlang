//! Exception handling and control flow builtins

use super::{PROMISE_RUNTIME, RuntimeValue};
use crate::utils::collections::FastMap;
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// Exception Handling Runtime
// ============================================================================

thread_local! {
    pub static CURRENT_EXCEPTION: RefCell<Option<RuntimeValue>> = const { RefCell::new(None) };
    // Registry for extended methods: class_name -> method_name -> function_name
    static EXTENDED_METHODS: RefCell<HashMap<String, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

/// Register an extended method for a class
/// args[0] = class_name, args[1] = method_name
#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_extend_class(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let class_name = args[0].as_string();
    let method_name = args[1].as_string();

    EXTENDED_METHODS.with(|methods| {
        let mut methods = methods.borrow_mut();
        let class_methods = methods.entry(class_name).or_insert_with(HashMap::new);
        // Store the method name - the function is already registered in LIR
        class_methods.insert(method_name.clone(), method_name);
    });

    RuntimeValue::Null
}

/// Get an extended method for a class if it exists
pub fn get_extended_method(class_name: &str, method_name: &str) -> Option<String> {
    EXTENDED_METHODS.with(|methods| {
        methods
            .borrow()
            .get(class_name)
            .and_then(|m| m.get(method_name).cloned())
    })
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_nonnull(args: &[RuntimeValue]) -> RuntimeValue {
    // Non-null assertion: returns the value if non-null, otherwise throws (returns null for now)
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let val = &args[0];
    if matches!(val, RuntimeValue::Null) {
        // In a real implementation, this would throw an error
        // For now, just return null
        RuntimeValue::Null
    } else {
        val.clone()
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_throw(args: &[RuntimeValue]) -> RuntimeValue {
    if let Some(val) = args.first() {
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(val.clone());
        });
    }
    RuntimeValue::Null
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_has_exception(_args: &[RuntimeValue]) -> RuntimeValue {
    CURRENT_EXCEPTION.with(|exc| RuntimeValue::Bool(exc.borrow().is_some()))
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_get_exception(_args: &[RuntimeValue]) -> RuntimeValue {
    CURRENT_EXCEPTION.with(|exc| exc.borrow().clone().unwrap_or(RuntimeValue::Null))
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_clear_exception(_args: &[RuntimeValue]) -> RuntimeValue {
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = None;
    });
    RuntimeValue::Null
}

#[allow(dead_code)]
pub(super) fn runtime_assert(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(
                "assert() requires at least 1 argument".to_string(),
            ));
        });
        return RuntimeValue::Null;
    }
    let condition = args[0].as_bool().unwrap_or(false);
    if !condition {
        let message = if args.len() >= 2 {
            args[1].as_string()
        } else {
            "Assertion failed: condition is false".to_string()
        };
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(format!("Test failed: {}", message)));
        });
    }
    RuntimeValue::Null
}

#[allow(dead_code)]
pub(super) fn runtime_assert_eq(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() != 2 {
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(
                "assert_eq() requires exactly 2 arguments".to_string(),
            ));
        });
        return RuntimeValue::Null;
    }
    let eq = match (&args[0], &args[1]) {
        (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a == b,
        (RuntimeValue::Float(a), RuntimeValue::Float(b)) => (a - b).abs() < f64::EPSILON,
        (RuntimeValue::Int(a), RuntimeValue::Float(b)) => (*a as f64 - b).abs() < f64::EPSILON,
        (RuntimeValue::Float(a), RuntimeValue::Int(b)) => (a - *b as f64).abs() < f64::EPSILON,
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
        (RuntimeValue::Char(a), RuntimeValue::Char(b)) => a == b,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        (a, b) => a == b,
    };
    if !eq {
        let left = format!("{:?}", args[0]);
        let right = format!("{:?}", args[1]);
        let message = format!(
            "Test failed: Assertion failed: left != right\n  left: {}\n right: {}",
            left, right
        );
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(message));
        });
    }
    RuntimeValue::Null
}

#[allow(dead_code)]
pub(super) fn runtime_assert_ne(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() != 2 {
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(
                "assert_ne() requires exactly 2 arguments".to_string(),
            ));
        });
        return RuntimeValue::Null;
    }
    let eq = match (&args[0], &args[1]) {
        (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a == b,
        (RuntimeValue::Float(a), RuntimeValue::Float(b)) => (a - b).abs() < f64::EPSILON,
        (RuntimeValue::Int(a), RuntimeValue::Float(b)) => (*a as f64 - b).abs() < f64::EPSILON,
        (RuntimeValue::Float(a), RuntimeValue::Int(b)) => (a - *b as f64).abs() < f64::EPSILON,
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
        (RuntimeValue::Char(a), RuntimeValue::Char(b)) => a == b,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        (a, b) => a == b,
    };
    if eq {
        let left = format!("{:?}", args[0]);
        let message = format!(
            "Test failed: Assertion failed: left == right (expected not equal)\n  both values: {}",
            left
        );
        CURRENT_EXCEPTION.with(|exc| {
            *exc.borrow_mut() = Some(RuntimeValue::String(message));
        });
    }
    RuntimeValue::Null
}

/// Create a user-defined error object
#[inline]
#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_error(args: &[RuntimeValue]) -> RuntimeValue {
    let message = if args.is_empty() {
        "Error".to_string()
    } else {
        match &args[0] {
            RuntimeValue::String(s) => s.clone(),
            _ => format!("{:?}", args[0]),
        }
    };

    let mut obj = FastMap::default();
    obj.insert(
        "__class__".to_string(),
        RuntimeValue::String("UserError".to_string()),
    );
    obj.insert("message".to_string(), RuntimeValue::String(message));
    RuntimeValue::Object(obj)
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_is_null(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Bool(true);
    }
    RuntimeValue::Bool(matches!(&args[0], RuntimeValue::Null))
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_check_executor_exception(args: &[RuntimeValue]) -> RuntimeValue {
    let promise_id = match args.first() {
        Some(RuntimeValue::Promise(id)) => *id,
        _ => return RuntimeValue::Null,
    };

    // Check if there's a pending exception
    let exception = CURRENT_EXCEPTION.with(|exc| exc.borrow().clone());
    if let Some(exc_value) = exception {
        // Clear the exception so it doesn't pollute other code
        CURRENT_EXCEPTION.with(|exc| *exc.borrow_mut() = None);
        // Reject the promise with the exception
        PROMISE_RUNTIME.reject_value(promise_id, exc_value);
    }

    RuntimeValue::Null
}
