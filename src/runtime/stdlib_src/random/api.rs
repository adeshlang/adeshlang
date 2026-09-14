//! Native Function Callbacks & Random Module Construction
//!
//! Provides native Rust callbacks for all Random standard library functions and builds the `Random` namespace object.

use super::bounded;
use super::collections;
use super::distributions;
use super::prng::{Xoshiro256PlusPlus, auto_seed};
use super::rng_object::make_rng_object;
use super::strings;
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;

use once_cell::sync::Lazy;
use rustc_hash::FxHashMap as HashMap;
use std::sync::{Arc, Mutex};

/// Default global thread-safe PRNG instance for Random convenience functions.
pub(crate) static DEFAULT_GLOBAL_PRNG: Lazy<Mutex<Xoshiro256PlusPlus>> =
    Lazy::new(|| Mutex::new(Xoshiro256PlusPlus::new_auto()));

/// Register all Random functions into the global BuiltinRegistry.
pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "Random.int",
        "random",
        "Generate random integer in range [min, max)",
        builtin_random_int,
    );
    registry.register(
        "Random.intInclusive",
        "random",
        "Generate random integer in inclusive range [min, max]",
        builtin_random_int_inclusive,
    );
    registry.register(
        "Random.intExclusive",
        "random",
        "Generate random integer in exclusive range (min, max)",
        builtin_random_int_exclusive,
    );
    registry.register(
        "Random.u8",
        "random",
        "Generate full-range u8 random integer",
        builtin_random_u8,
    );
    registry.register(
        "Random.u16",
        "random",
        "Generate full-range u16 random integer",
        builtin_random_u16,
    );
    registry.register(
        "Random.u32",
        "random",
        "Generate full-range u32 random integer",
        builtin_random_u32,
    );
    registry.register(
        "Random.u64",
        "random",
        "Generate full-range u64 random integer",
        builtin_random_u64,
    );
    registry.register(
        "Random.u128",
        "random",
        "Generate full-range u128 random integer",
        builtin_random_u128,
    );
    registry.register(
        "Random.i8",
        "random",
        "Generate full-range i8 random integer",
        builtin_random_i8,
    );
    registry.register(
        "Random.i16",
        "random",
        "Generate full-range i16 random integer",
        builtin_random_i16,
    );
    registry.register(
        "Random.i32",
        "random",
        "Generate full-range i32 random integer",
        builtin_random_i32,
    );
    registry.register(
        "Random.i64",
        "random",
        "Generate full-range i64 random integer",
        builtin_random_i64,
    );
    registry.register(
        "Random.i128",
        "random",
        "Generate full-range i128 random integer",
        builtin_random_i128,
    );

    registry.register(
        "Random.float",
        "random",
        "Generate random float in range [0.0, 1.0) or [min, max)",
        builtin_random_float,
    );
    registry.register(
        "Random.floatRange",
        "random",
        "Generate random float in range [min, max)",
        builtin_random_float_range,
    );
    registry.register(
        "Random.floatInclusive",
        "random",
        "Generate random float in range [min, max]",
        builtin_random_float_inclusive,
    );
    registry.register(
        "Random.f32",
        "random",
        "Generate uniform f32 float in range [0.0, 1.0)",
        builtin_random_f32,
    );
    registry.register(
        "Random.f64",
        "random",
        "Generate uniform f64 float in range [0.0, 1.0)",
        builtin_random_f64,
    );

    registry.register(
        "Random.bool",
        "random",
        "Generate random boolean with 50% probability",
        builtin_random_bool,
    );
    registry.register(
        "Random.bernoulli",
        "random",
        "Generate Bernoulli boolean with given probability p",
        builtin_random_bernoulli,
    );

    registry.register(
        "Random.byte",
        "random",
        "Generate single random byte [0..255]",
        builtin_random_byte,
    );
    registry.register(
        "Random.bytes",
        "random",
        "Generate array of random bytes",
        builtin_random_bytes,
    );
    registry.register(
        "Random.fillBytes",
        "random",
        "Fill existing mutable byte array with random bytes",
        builtin_random_fill_bytes,
    );
    registry.register(
        "Random.fillInts",
        "random",
        "Fill array with random integers",
        builtin_random_fill_ints,
    );
    registry.register(
        "Random.fillFloats",
        "random",
        "Fill array with random floats",
        builtin_random_fill_floats,
    );

    registry.register(
        "Random.string",
        "random",
        "Generate random alphanumeric string",
        builtin_random_string,
    );
    registry.register(
        "Random.alphanumeric",
        "random",
        "Generate random alphanumeric string",
        builtin_random_alphanumeric,
    );
    registry.register(
        "Random.ascii",
        "random",
        "Generate random printable ASCII string",
        builtin_random_ascii,
    );
    registry.register(
        "Random.hex",
        "random",
        "Generate random hex string",
        builtin_random_hex,
    );
    registry.register(
        "Random.stringFrom",
        "random",
        "Generate random string from custom charset",
        builtin_random_string_from,
    );
    registry.register(
        "Random.char",
        "random",
        "Generate random valid Unicode scalar character",
        builtin_random_char,
    );
    registry.register(
        "Random.charFrom",
        "random",
        "Generate random character from string charset",
        builtin_random_char_from,
    );

    registry.register(
        "Random.choice",
        "random",
        "Select random element from collection",
        builtin_random_choice,
    );
    registry.register(
        "Random.shuffle",
        "random",
        "Perform Fisher-Yates shuffle on array",
        builtin_random_shuffle,
    );
    registry.register(
        "Random.shuffled",
        "random",
        "Return a new shuffled copy of array",
        builtin_random_shuffled,
    );
    registry.register(
        "Random.sample",
        "random",
        "Sample k elements without replacement",
        builtin_random_sample,
    );
    registry.register(
        "Random.sampleWithReplacement",
        "random",
        "Sample k elements with replacement",
        builtin_random_sample_with_replacement,
    );
    registry.register(
        "Random.weightedChoice",
        "random",
        "Choose element based on weight array or pair list",
        builtin_random_weighted_choice,
    );
    registry.register(
        "Random.reservoirSample",
        "random",
        "Stream sampling k elements in O(k) memory",
        builtin_random_reservoir_sample,
    );

    registry.register(
        "Random.seed",
        "random",
        "Create a new deterministic seeded Rng instance",
        builtin_random_seed,
    );

    registry.register(
        "Random.normal",
        "random",
        "Sample from Gaussian Normal distribution N(mean, stdDev)",
        builtin_random_normal,
    );
    registry.register(
        "Random.uniform",
        "random",
        "Sample from continuous Uniform distribution U(min, max)",
        builtin_random_uniform,
    );
    registry.register(
        "Random.binomial",
        "random",
        "Sample from Binomial distribution Binomial(trials, p)",
        builtin_random_binomial,
    );
    registry.register(
        "Random.exponential",
        "random",
        "Sample from Exponential distribution Exp(lambda)",
        builtin_random_exponential,
    );
    registry.register(
        "Random.poisson",
        "random",
        "Sample from Poisson distribution Poisson(lambda)",
        builtin_random_poisson,
    );
    registry.register(
        "Random.geometric",
        "random",
        "Sample from Geometric distribution Geometric(p)",
        builtin_random_geometric,
    );
    registry.register(
        "Random.gamma",
        "random",
        "Sample from Gamma distribution Gamma(alpha, beta)",
        builtin_random_gamma,
    );
    registry.register(
        "Random.logNormal",
        "random",
        "Sample from LogNormal distribution LogNormal(mean, stdDev)",
        builtin_random_log_normal,
    );

    registry.register(
        "Random.uuid",
        "random",
        "Generate RFC 4122 v4 UUID string",
        builtin_random_uuid,
    );
    registry.register(
        "Random.uuid4",
        "random",
        "Generate RFC 4122 v4 UUID string",
        builtin_random_uuid,
    );
    registry.register(
        "Random.uuidString",
        "random",
        "Generate RFC 4122 v4 UUID string",
        builtin_random_uuid,
    );

    registry.register(
        "Random.token",
        "random",
        "Generate random URL-safe token string",
        builtin_random_token,
    );
    registry.register(
        "Random.hexToken",
        "random",
        "Generate random hex token string",
        builtin_random_hex_token,
    );
    registry.register(
        "Random.base64Token",
        "random",
        "Generate random base64 token string",
        builtin_random_base64_token,
    );

    registry.register(
        "Random.secureBytes",
        "random",
        "Generate cryptographically secure random bytes from OS entropy",
        builtin_random_secure_bytes,
    );
    registry.register(
        "Crypto.secureRandomBytes",
        "crypto",
        "Generate cryptographically secure random bytes from OS entropy",
        builtin_random_secure_bytes,
    );
}

/// Construct the first-class `Random` module object bound when `import Random;` is executed.
pub fn build_random_module_object() -> Value {
    let mut map: HashMap<String, Value> = HashMap::default();

    map.insert(
        "int".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_int))),
    );
    map.insert(
        "intInclusive".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_int_inclusive))),
    );
    map.insert(
        "intExclusive".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_int_exclusive))),
    );
    map.insert(
        "u8".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_u8))),
    );
    map.insert(
        "u16".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_u16))),
    );
    map.insert(
        "u32".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_u32))),
    );
    map.insert(
        "u64".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_u64))),
    );
    map.insert(
        "u128".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_u128))),
    );
    map.insert(
        "i8".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_i8))),
    );
    map.insert(
        "i16".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_i16))),
    );
    map.insert(
        "i32".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_i32))),
    );
    map.insert(
        "i64".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_i64))),
    );
    map.insert(
        "i128".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_i128))),
    );

    map.insert(
        "float".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_float))),
    );
    map.insert(
        "floatRange".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_float_range))),
    );
    map.insert(
        "floatInclusive".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_float_inclusive))),
    );
    map.insert(
        "f32".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_f32))),
    );
    map.insert(
        "f64".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_f64))),
    );

    map.insert(
        "bool".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_bool))),
    );
    map.insert(
        "bernoulli".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_bernoulli))),
    );

    map.insert(
        "byte".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_byte))),
    );
    map.insert(
        "bytes".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_bytes))),
    );
    map.insert(
        "fillBytes".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_fill_bytes))),
    );
    map.insert(
        "fillInts".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_fill_ints))),
    );
    map.insert(
        "fillFloats".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_fill_floats))),
    );

    map.insert(
        "string".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_string))),
    );
    map.insert(
        "alphanumeric".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_alphanumeric))),
    );
    map.insert(
        "ascii".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_ascii))),
    );
    map.insert(
        "hex".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_hex))),
    );
    map.insert(
        "stringFrom".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_string_from))),
    );
    map.insert(
        "char".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_char))),
    );
    map.insert(
        "charFrom".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_char_from))),
    );

    map.insert(
        "choice".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_choice))),
    );
    map.insert(
        "shuffle".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_shuffle))),
    );
    map.insert(
        "shuffled".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_shuffled))),
    );
    map.insert(
        "sample".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_sample))),
    );
    map.insert(
        "sampleWithReplacement".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_sample_with_replacement))),
    );
    map.insert(
        "weightedChoice".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_weighted_choice))),
    );
    map.insert(
        "reservoirSample".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_reservoir_sample))),
    );

    map.insert(
        "seed".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_seed))),
    );

    map.insert(
        "normal".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_normal))),
    );
    map.insert(
        "uniform".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_uniform))),
    );
    map.insert(
        "binomial".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_binomial))),
    );
    map.insert(
        "exponential".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_exponential))),
    );
    map.insert(
        "poisson".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_poisson))),
    );
    map.insert(
        "geometric".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_geometric))),
    );
    map.insert(
        "gamma".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_gamma))),
    );
    map.insert(
        "logNormal".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_log_normal))),
    );

    map.insert(
        "uuid".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_uuid))),
    );
    map.insert(
        "uuid4".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_uuid))),
    );
    map.insert(
        "uuidString".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_uuid))),
    );

    map.insert(
        "token".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_token))),
    );
    map.insert(
        "hexToken".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_hex_token))),
    );
    map.insert(
        "base64Token".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_base64_token))),
    );

    map.insert(
        "secureBytes".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_secure_bytes))),
    );

    // Random.Rng namespace object with constructor functions (.seed(s), .new())
    let mut rng_ns: HashMap<String, Value> = HashMap::default();
    rng_ns.insert(
        "seed".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_random_seed))),
    );
    rng_ns.insert(
        "new".to_string(),
        Value::Function(NativeFn(Arc::new(|_env, _args| {
            let prng = Xoshiro256PlusPlus::new_auto();
            Ok(make_rng_object(prng))
        }))),
    );
    map.insert("Rng".to_string(), Value::Object(Arc::new(rng_ns)));

    // Distribution constructor objects (Normal, Uniform, Exponential, Binomial, Poisson)
    map.insert(
        "Normal".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dist_normal_obj))),
    );
    map.insert(
        "Uniform".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dist_uniform_obj))),
    );
    map.insert(
        "Exponential".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dist_exponential_obj))),
    );

    Value::Object(Arc::new(map))
}

// ============================================================================
// Callback Implementations
// ============================================================================

fn builtin_random_int(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    if args.is_empty() {
        Ok(Value::I64(g.next_u64() as i64))
    } else if args.len() == 2 {
        let min = as_i64(&args[0])?;
        let max = as_i64(&args[1])?;
        let val = bounded::next_i64_range(&mut g, min, max)?;
        Ok(Value::I64(val))
    } else if args.len() == 1 {
        let max = as_i64(&args[0])?;
        let val = bounded::next_i64_range(&mut g, 0, max)?;
        Ok(Value::I64(val))
    } else {
        Err("Random.int expects 0, 1, or 2 arguments: (), (max), or (min, max)".to_string())
    }
}

fn builtin_random_int_inclusive(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.intInclusive requires 2 arguments: (min, max)".to_string());
    }
    let min = as_i64(&args[0])?;
    let max = as_i64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = bounded::next_i64_range_inclusive(&mut g, min, max)?;
    Ok(Value::I64(val))
}

fn builtin_random_int_exclusive(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.intExclusive requires 2 arguments: (min, max)".to_string());
    }
    let min = as_i64(&args[0])?;
    let max = as_i64(&args[1])?;
    if max - min <= 1 {
        return Err(format!(
            "No integer exists in exclusive range ({}, {})",
            min, max
        ));
    }
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = bounded::next_i64_range(&mut g, min + 1, max)?;
    Ok(Value::I64(val))
}

fn builtin_random_u8(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = (g.next_u64() & 0xff) as u8;
    Ok(Value::U8(val))
}

fn builtin_random_u16(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = (g.next_u64() & 0xffff) as u16;
    Ok(Value::U16(val))
}

fn builtin_random_u32(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = g.next_u32();
    Ok(Value::U32(val))
}

fn builtin_random_u64(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = g.next_u64();
    Ok(Value::U64(val))
}

fn builtin_random_u128(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let hi = g.next_u64();
    let lo = g.next_u64();
    let val = ((hi as u128) << 64) | (lo as u128);
    Ok(Value::U128(val))
}

fn builtin_random_i8(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = (g.next_u64() & 0xff) as i8;
    Ok(Value::I8(val))
}

fn builtin_random_i16(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = (g.next_u64() & 0xffff) as i16;
    Ok(Value::I16(val))
}

fn builtin_random_i32(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = g.next_u32() as i32;
    Ok(Value::I32(val))
}

fn builtin_random_i64(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = g.next_u64() as i64;
    Ok(Value::I64(val))
}

fn builtin_random_i128(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let hi = g.next_u64();
    let lo = g.next_u64();
    let val = (((hi as u128) << 64) | (lo as u128)) as i128;
    Ok(Value::I128(val))
}

fn builtin_random_float(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    if args.is_empty() {
        Ok(Value::Number(bounded::next_f64(&mut g)))
    } else if args.len() == 2 {
        let min = as_f64(&args[0])?;
        let max = as_f64(&args[1])?;
        let val = bounded::next_f64_range(&mut g, min, max)?;
        Ok(Value::Number(val))
    } else {
        Err("Random.float expects 0 or 2 arguments: () or (min, max)".to_string())
    }
}

fn builtin_random_float_range(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.floatRange requires 2 arguments: (min, max)".to_string());
    }
    let min = as_f64(&args[0])?;
    let max = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = bounded::next_f64_range(&mut g, min, max)?;
    Ok(Value::Number(val))
}

fn builtin_random_float_inclusive(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.floatInclusive requires 2 arguments: (min, max)".to_string());
    }
    let min = as_f64(&args[0])?;
    let max = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = bounded::next_f64_range_inclusive(&mut g, min, max)?;
    Ok(Value::Number(val))
}

fn builtin_random_f32(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::F32(bounded::next_f32(&mut g)))
}

fn builtin_random_f64(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::F64(bounded::next_f64(&mut g)))
}

fn builtin_random_bool(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    if args.is_empty() {
        Ok(Value::Bool(bounded::next_bool(&mut g)))
    } else if args.len() == 1 {
        let p = as_f64(&args[0])?;
        let val = bounded::next_bernoulli(&mut g, p)?;
        Ok(Value::Bool(val))
    } else {
        Err("Random.bool expects 0 or 1 argument".to_string())
    }
}

fn builtin_random_bernoulli(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.bernoulli requires 1 probability argument (p)".to_string());
    }
    let p = as_f64(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = bounded::next_bernoulli(&mut g, p)?;
    Ok(Value::Bool(val))
}

fn builtin_random_byte(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let b = bounded::next_u64_bounded(&mut g, 256) as u8;
    Ok(Value::U8(b))
}

fn builtin_random_bytes(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.bytes requires 1 count argument".to_string());
    }
    let count = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let mut buf = vec![0u8; count];
    g.fill_bytes(&mut buf);
    let vals: Vec<Value> = buf.into_iter().map(Value::U8).collect();
    Ok(Value::Array(vals))
}

fn builtin_random_fill_bytes(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.fillBytes requires 1 array argument".to_string());
    }
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    match &args[0] {
        Value::Array(arr) => {
            let mut filled = Vec::with_capacity(arr.len());
            for _ in 0..arr.len() {
                let b = bounded::next_u64_bounded(&mut g, 256) as u8;
                filled.push(Value::U8(b));
            }
            Ok(Value::Array(filled))
        }
        _ => Err("Random.fillBytes argument must be a mutable array".to_string()),
    }
}

fn builtin_random_fill_ints(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 || args.len() > 3 {
        return Err("Random.fillInts expects (count) or (count, min, max)".to_string());
    }
    let count = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let mut filled = Vec::with_capacity(count);
    if args.len() == 3 {
        let min = as_i64(&args[1])?;
        let max = as_i64(&args[2])?;
        for _ in 0..count {
            let val = bounded::next_i64_range(&mut g, min, max)?;
            filled.push(Value::I64(val));
        }
    } else {
        for _ in 0..count {
            filled.push(Value::I64(g.next_u64() as i64));
        }
    }
    Ok(Value::Array(filled))
}

fn builtin_random_fill_floats(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 1 || args.len() > 3 {
        return Err("Random.fillFloats expects (count) or (count, min, max)".to_string());
    }
    let count = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let mut filled = Vec::with_capacity(count);
    if args.len() == 3 {
        let min = as_f64(&args[1])?;
        let max = as_f64(&args[2])?;
        for _ in 0..count {
            let val = bounded::next_f64_range(&mut g, min, max)?;
            filled.push(Value::Number(val));
        }
    } else {
        for _ in 0..count {
            filled.push(Value::Number(bounded::next_f64(&mut g)));
        }
    }
    Ok(Value::Array(filled))
}

fn builtin_random_string(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.string requires 1 length argument".to_string());
    }
    let len = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_alphanumeric(&mut g, len)))
}

fn builtin_random_alphanumeric(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.alphanumeric requires 1 length argument".to_string());
    }
    let len = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_alphanumeric(&mut g, len)))
}

fn builtin_random_ascii(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.ascii requires 1 length argument".to_string());
    }
    let len = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_ascii(&mut g, len)))
}

fn builtin_random_hex(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.hex requires 1 length argument".to_string());
    }
    let len = as_usize(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_hex(&mut g, len)))
}

fn builtin_random_string_from(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.stringFrom requires 2 arguments: (charset, length)".to_string());
    }
    let charset = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("Random.stringFrom first argument must be a charset string".to_string()),
    };
    let len = as_usize(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let s = strings::random_string_from(&mut g, charset, len)?;
    Ok(Value::Str(s))
}

fn builtin_random_char(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let c = strings::random_char(&mut g);
    Ok(Value::Str(c.to_string()))
}

fn builtin_random_char_from(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.charFrom requires 1 charset argument".to_string());
    }
    let charset = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("Random.charFrom argument must be a charset string".to_string()),
    };
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let s = strings::random_string_from(&mut g, charset, 1)?;
    Ok(Value::Str(s))
}

fn builtin_random_choice(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.choice requires 1 collection argument".to_string());
    }
    let items = as_array_slice(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = collections::choice(&mut g, &items)?;
    Ok(val.clone())
}

fn builtin_random_shuffle(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.shuffle requires 1 array argument".to_string());
    }
    let mut items = as_array_slice(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    collections::shuffle(&mut g, &mut items);
    Ok(Value::Array(items))
}

fn builtin_random_shuffled(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.shuffled requires 1 array argument".to_string());
    }
    let mut items = as_array_slice(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    collections::shuffle(&mut g, &mut items);
    Ok(Value::Array(items))
}

fn builtin_random_sample(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.sample requires 2 arguments: (collection, count)".to_string());
    }
    let items = as_array_slice(&args[0])?;
    let count = as_usize(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let result = collections::sample(&mut g, &items, count)?;
    Ok(Value::Array(result))
}

fn builtin_random_sample_with_replacement(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err(
            "Random.sampleWithReplacement requires 2 arguments: (collection, count)".to_string(),
        );
    }
    let items = as_array_slice(&args[0])?;
    let count = as_usize(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let result = collections::sample_with_replacement(&mut g, &items, count)?;
    Ok(Value::Array(result))
}

fn builtin_random_weighted_choice(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() == 1 {
        // Handle array of tuples / pairs: [("a", 10), ("b", 90)]
        let pairs = as_array_slice(&args[0])?;
        let mut items = Vec::with_capacity(pairs.len());
        let mut weights = Vec::with_capacity(pairs.len());
        for pair in pairs {
            let arr = as_array_slice(&pair)?;
            if arr.len() == 2 {
                items.push(arr[0].clone());
                weights.push(as_f64(&arr[1])?);
            } else {
                return Err(
                    "Random.weightedChoice pair must be an array of length 2 [item, weight]"
                        .to_string(),
                );
            }
        }
        let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
        collections::weighted_choice(&mut g, &items, &weights)
    } else if args.len() == 2 {
        let items = as_array_slice(&args[0])?;
        let weight_vals = as_array_slice(&args[1])?;
        let mut weights = Vec::with_capacity(weight_vals.len());
        for w in weight_vals {
            weights.push(as_f64(&w)?);
        }
        let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
        collections::weighted_choice(&mut g, &items, &weights)
    } else {
        Err("Random.weightedChoice expects 1 or 2 arguments: ([ (item, weight)... ]) or (items, weights)".to_string())
    }
}

fn builtin_random_reservoir_sample(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.reservoirSample requires 2 arguments: (collection, k)".to_string());
    }
    let items = as_array_slice(&args[0])?;
    let k = as_usize(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let result = collections::reservoir_sample(&mut g, &items, k)?;
    Ok(Value::Array(result))
}

fn builtin_random_seed(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let seed_val = if args.is_empty() {
        auto_seed()
    } else if args.len() == 1 {
        as_u64(&args[0])?
    } else {
        return Err("Random.seed expects 0 or 1 seed argument".to_string());
    };
    let prng = Xoshiro256PlusPlus::seed_from_u64(seed_val);
    Ok(make_rng_object(prng))
}

fn builtin_random_normal(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.normal requires 2 arguments: (mean, stdDev)".to_string());
    }
    let mean = as_f64(&args[0])?;
    let std_dev = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_normal(&mut g, mean, std_dev)?;
    Ok(Value::Number(val))
}

fn builtin_random_uniform(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.uniform requires 2 arguments: (min, max)".to_string());
    }
    let min = as_f64(&args[0])?;
    let max = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_uniform(&mut g, min, max)?;
    Ok(Value::Number(val))
}

fn builtin_random_binomial(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.binomial requires 2 arguments: (trials, p)".to_string());
    }
    let n = as_usize(&args[0])? as u64;
    let p = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_binomial(&mut g, n, p)?;
    Ok(Value::I64(val as i64))
}

fn builtin_random_exponential(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.exponential requires 1 argument: (lambda)".to_string());
    }
    let lambda = as_f64(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_exponential(&mut g, lambda)?;
    Ok(Value::Number(val))
}

fn builtin_random_poisson(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.poisson requires 1 argument: (lambda)".to_string());
    }
    let lambda = as_f64(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_poisson(&mut g, lambda)?;
    Ok(Value::I64(val as i64))
}

fn builtin_random_geometric(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.geometric requires 1 argument: (p)".to_string());
    }
    let p = as_f64(&args[0])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_geometric(&mut g, p)?;
    Ok(Value::I64(val as i64))
}

fn builtin_random_gamma(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.gamma requires 2 arguments: (alpha, beta)".to_string());
    }
    let alpha = as_f64(&args[0])?;
    let beta = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_gamma(&mut g, alpha, beta)?;
    Ok(Value::Number(val))
}

fn builtin_random_log_normal(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.logNormal requires 2 arguments: (mean, stdDev)".to_string());
    }
    let mean = as_f64(&args[0])?;
    let std_dev = as_f64(&args[1])?;
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    let val = distributions::sample_log_normal(&mut g, mean, std_dev)?;
    Ok(Value::Number(val))
}

fn builtin_random_uuid(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_uuid4(&mut g)))
}

fn builtin_random_token(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let len = if args.is_empty() {
        32
    } else {
        as_usize(&args[0])?
    };
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_alphanumeric(&mut g, len)))
}

fn builtin_random_hex_token(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let len = if args.is_empty() {
        32
    } else {
        as_usize(&args[0])?
    };
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_hex(&mut g, len)))
}

fn builtin_random_base64_token(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let len = if args.is_empty() {
        32
    } else {
        as_usize(&args[0])?
    };
    let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
    Ok(Value::Str(strings::random_base64_token(&mut g, len)))
}

fn builtin_random_secure_bytes(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    use rand::RngCore;
    let count = if args.is_empty() {
        32
    } else {
        as_usize(&args[0])?
    };
    let mut buf = vec![0u8; count];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    let vals: Vec<Value> = buf.into_iter().map(Value::U8).collect();
    Ok(Value::Array(vals))
}

fn builtin_dist_normal_obj(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.Normal constructor requires 2 arguments: (mean, stdDev)".to_string());
    }
    let mean = as_f64(&args[0])?;
    let std_dev = as_f64(&args[1])?;
    let mut map: HashMap<String, Value> = HashMap::default();
    map.insert(
        "sample".to_string(),
        Value::Function(NativeFn(Arc::new(move |_e, _a| {
            let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
            let val = distributions::sample_normal(&mut g, mean, std_dev)?;
            Ok(Value::Number(val))
        }))),
    );
    Ok(Value::Object(Arc::new(map)))
}

fn builtin_dist_uniform_obj(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Random.Uniform constructor requires 2 arguments: (min, max)".to_string());
    }
    let min = as_f64(&args[0])?;
    let max = as_f64(&args[1])?;
    let mut map: HashMap<String, Value> = HashMap::default();
    map.insert(
        "sample".to_string(),
        Value::Function(NativeFn(Arc::new(move |_e, _a| {
            let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
            let val = distributions::sample_uniform(&mut g, min, max)?;
            Ok(Value::Number(val))
        }))),
    );
    Ok(Value::Object(Arc::new(map)))
}

fn builtin_dist_exponential_obj(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Random.Exponential constructor requires 1 argument: (lambda)".to_string());
    }
    let lambda = as_f64(&args[0])?;
    let mut map: HashMap<String, Value> = HashMap::default();
    map.insert(
        "sample".to_string(),
        Value::Function(NativeFn(Arc::new(move |_e, _a| {
            let mut g = DEFAULT_GLOBAL_PRNG.lock().unwrap();
            let val = distributions::sample_exponential(&mut g, lambda)?;
            Ok(Value::Number(val))
        }))),
    );
    Ok(Value::Object(Arc::new(map)))
}

fn as_i64(v: &Value) -> Result<i64, String> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::I32(n) => Ok(*n as i64),
        Value::I16(n) => Ok(*n as i64),
        Value::I8(n) => Ok(*n as i64),
        Value::U64(n) => Ok(*n as i64),
        Value::U32(n) => Ok(*n as i64),
        Value::U16(n) => Ok(*n as i64),
        Value::U8(n) => Ok(*n as i64),
        Value::Number(n) => Ok(*n as i64),
        _ => Err(format!("Expected integer, got {:?}", v)),
    }
}

fn as_u64(v: &Value) -> Result<u64, String> {
    match v {
        Value::U64(n) => Ok(*n),
        Value::U32(n) => Ok(*n as u64),
        Value::U16(n) => Ok(*n as u64),
        Value::U8(n) => Ok(*n as u64),
        Value::I64(n) => Ok(*n as u64),
        Value::I32(n) => Ok(*n as u64),
        Value::Number(n) => Ok(*n as u64),
        _ => Err(format!("Expected u64 integer, got {:?}", v)),
    }
}

fn as_usize(v: &Value) -> Result<usize, String> {
    let n = as_i64(v)?;
    if n < 0 {
        Err(format!("Count/length cannot be negative, got {}", n))
    } else {
        Ok(n as usize)
    }
}

fn as_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::F64(n) => Ok(*n),
        Value::F32(n) => Ok(*n as f64),
        Value::I64(n) => Ok(*n as f64),
        Value::I32(n) => Ok(*n as f64),
        Value::U64(n) => Ok(*n as f64),
        Value::U32(n) => Ok(*n as f64),
        _ => Err(format!("Expected float number, got {:?}", v)),
    }
}

fn as_array_slice(v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(arr) => Ok(arr.clone()),
        Value::RawArray(_, arr) => Ok(arr.clone()),
        Value::Tuple(arr) => Ok(arr.clone()),
        Value::Set(arr) => Ok(arr.clone()),
        Value::DynArray(da) => Ok(da.data.clone()),
        _ => Err(format!("Expected array collection, got {:?}", v)),
    }
}
