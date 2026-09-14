//! Mathematical operations builtin functions
//!
//! This module provides runtime implementations for mathematical operations including
//! basic arithmetic, trigonometric functions, logarithms, and random number generation.

use super::RuntimeValue;
use num_bigint::BigInt;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::Mutex;

// Global seedable RNG for deterministic random number generation
pub(crate) static SEEDED_RNG: once_cell::sync::Lazy<Mutex<Option<StdRng>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// Absolute value
pub(crate) fn runtime_abs(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::Int(n.abs()),
        RuntimeValue::Float(n) => RuntimeValue::Float(n.abs()),
        _ => RuntimeValue::Int(0),
    }
}

/// Square root
pub(crate) fn runtime_sqrt(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.sqrt())
}

/// Power function
pub(crate) fn runtime_pow(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Float(0.0);
    }
    let base = args[0].as_float().unwrap_or(0.0);
    let exp = args[1].as_float().unwrap_or(0.0);
    RuntimeValue::Float(base.powf(exp))
}

/// Integer division
pub(crate) fn runtime_int_div(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Int(0);
    }
    let left = args[0].as_int().unwrap_or(0);
    let right = args[1].as_int().unwrap_or(1);
    if right == 0 {
        RuntimeValue::Int(0)
    } else {
        RuntimeValue::Int(left / right)
    }
}

/// Minimum of two values
pub(crate) fn runtime_min(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    let a = args[0].as_float().unwrap_or(f64::MAX);
    let b = args[1].as_float().unwrap_or(f64::MAX);
    RuntimeValue::Float(a.min(b))
}

/// Maximum of two values
pub(crate) fn runtime_max(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return args.first().cloned().unwrap_or(RuntimeValue::Null);
    }
    let a = args[0].as_float().unwrap_or(f64::MIN);
    let b = args[1].as_float().unwrap_or(f64::MIN);
    RuntimeValue::Float(a.max(b))
}

/// Floor function
pub(crate) fn runtime_floor(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.floor())
}

/// Ceiling function
pub(crate) fn runtime_ceil(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.ceil())
}

/// Round function
pub(crate) fn runtime_round(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.round())
}

/// Sine function
pub(crate) fn runtime_sin(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.sin())
}

/// Cosine function
pub(crate) fn runtime_cos(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(1.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.cos())
}

/// Tangent function
pub(crate) fn runtime_tan(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.tan())
}

/// Arcsine function
pub(crate) fn runtime_asin(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.asin())
}

/// Arccosine function
pub(crate) fn runtime_acos(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.acos())
}

/// Arctangent function
pub(crate) fn runtime_atan(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.atan())
}

/// Two-argument arctangent function
pub(crate) fn runtime_atan2(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Float(0.0);
    }
    let y = args[0].as_float().unwrap_or(0.0);
    let x = args[1].as_float().unwrap_or(0.0);
    RuntimeValue::Float(y.atan2(x))
}

/// Exponential function (e^x)
pub(crate) fn runtime_exp(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(1.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.exp())
}

/// Natural logarithm
pub(crate) fn runtime_log(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(f64::NEG_INFINITY);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.ln())
}

/// Base-10 logarithm
pub(crate) fn runtime_log10(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(f64::NEG_INFINITY);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.log10())
}

/// Base-2 logarithm
pub(crate) fn runtime_log2(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(f64::NEG_INFINITY);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.log2())
}

/// Truncate to integer (towards zero)
pub(crate) fn runtime_trunc(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    RuntimeValue::Float(n.trunc())
}

/// Sign function (-1, 0, or 1)
pub(crate) fn runtime_sign(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    let n = args[0].as_float().unwrap_or(0.0);
    let sign = if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        0.0
    };
    RuntimeValue::Float(sign)
}

/// Math constant: PI
pub(crate) fn runtime_math_pi(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::PI)
}

/// Math constant: E (Euler's number)
pub(crate) fn runtime_math_e(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::E)
}

/// Math constant: TAU (2*PI)
pub(crate) fn runtime_math_tau(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::TAU)
}

/// Math constant: SQRT(2)
pub(crate) fn runtime_math_sqrt2(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::SQRT_2)
}

/// Math constant: LN(2)
pub(crate) fn runtime_math_ln2(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::LN_2)
}

/// Math constant: LN(10)
pub(crate) fn runtime_math_ln10(_args: &[RuntimeValue]) -> RuntimeValue {
    RuntimeValue::Float(std::f64::consts::LN_10)
}

/// Returns a random floating point number between 0.0 and 1.0
pub(crate) fn runtime_math_random(_args: &[RuntimeValue]) -> RuntimeValue {
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    if let Some(ref mut rng) = *guard {
        let val: f64 = rng.gen_range(0.0..1.0);
        RuntimeValue::Float(val)
    } else {
        let mut rng = rand::thread_rng();
        RuntimeValue::Float(rng.gen_range(0.0..1.0))
    }
}

/// Seed the random number generator for reproducible results
pub(crate) fn runtime_math_seed(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    let seed = args[0].as_int().unwrap_or(0) as u64;
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    *guard = Some(StdRng::seed_from_u64(seed));
    RuntimeValue::Null
}

/// Random integer in range [min, max]
pub(crate) fn runtime_math_random_int(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let min = args[0].as_int().unwrap_or(0);
    let max = args[1].as_int().unwrap_or(0);

    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    let val = if let Some(ref mut rng) = *guard {
        rng.gen_range(min..=max)
    } else {
        rand::thread_rng().gen_range(min..=max)
    };
    RuntimeValue::Int(val)
}

/// Random float in range [min, max)
pub(crate) fn runtime_math_random_range(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let min = args[0].as_float().unwrap_or(0.0);
    let max = args[1].as_float().unwrap_or(0.0);

    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    let val = if let Some(ref mut rng) = *guard {
        rng.gen_range(min..max)
    } else {
        rand::thread_rng().gen_range(min..max)
    };
    RuntimeValue::Float(val)
}

/// Polymorphic addition (handles numbers and strings)
pub(crate) fn runtime_add(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let left = &args[0];
    let right = &args[1];

    // If either operand is a string, do string concatenation
    match (left, right) {
        (RuntimeValue::String(a), _) => RuntimeValue::String(format!("{}{}", a, right.as_string())),
        (_, RuntimeValue::String(b)) => RuntimeValue::String(format!("{}{}", left.as_string(), b)),
        // Handle BigInt addition
        (RuntimeValue::BigInt(a), RuntimeValue::BigInt(b)) => RuntimeValue::BigInt(a + b),
        (RuntimeValue::BigInt(a), RuntimeValue::Int(b)) => {
            RuntimeValue::BigInt(a + BigInt::from(*b))
        }
        (RuntimeValue::Int(a), RuntimeValue::BigInt(b)) => {
            RuntimeValue::BigInt(BigInt::from(*a) + b)
        }
        // Regular integer addition
        (RuntimeValue::Int(a), RuntimeValue::Int(b)) => RuntimeValue::Int(a + b),
        (RuntimeValue::Float(a), RuntimeValue::Float(b)) => RuntimeValue::Float(a + b),
        (RuntimeValue::Int(a), RuntimeValue::Float(b)) => RuntimeValue::Float(*a as f64 + b),
        (RuntimeValue::Float(a), RuntimeValue::Int(b)) => RuntimeValue::Float(a + *b as f64),
        _ => {
            // For any other numeric types (U8, U16, I32, F32, F64, etc.),
            // convert to i64/f64 and add
            if let (Some(a), Some(b)) = (left.as_int(), right.as_int()) {
                RuntimeValue::Int(a + b)
            } else if let (Some(a), Some(b)) = (left.as_float(), right.as_float()) {
                RuntimeValue::Float(a + b)
            } else {
                // True fallback: convert to strings and concatenate
                RuntimeValue::String(format!("{}{}", left.as_string(), right.as_string()))
            }
        }
    }
}

/// Division operation (always returns float)
pub(crate) fn runtime_div(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Null;
    }
    let left = &args[0];
    let right = &args[1];

    // Division always returns float
    // Handle all numeric types by converting to float
    let left_f = left.as_float().unwrap_or(0.0);
    let right_f = right.as_float().unwrap_or(1.0);

    if right_f == 0.0 {
        RuntimeValue::Float(f64::INFINITY)
    } else {
        RuntimeValue::Float(left_f / right_f)
    }
}
