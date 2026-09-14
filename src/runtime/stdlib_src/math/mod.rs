//! Math & CMath Stdlib
//!
//! Provides `Math` and `cmath` namespaces with constants and functions.
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;
use std::sync::Mutex;

// Global seedable RNG for deterministic random number generation
static SEEDED_RNG: once_cell::sync::Lazy<Mutex<Option<StdRng>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I128(n) => Some(*n as f64),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U128(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

fn expect_num(v: &Value, name: &str) -> Result<f64, String> {
    if let Some(n) = as_f64(v) {
        Ok(n)
    } else {
        Err(err(format!("{} must be number", name)))
    }
}

fn expect_complex(v: &Value, name: &str) -> Result<(f64, f64), String> {
    match v {
        Value::Complex(r, i) => Ok((*r, *i)),
        _ => {
            if let Some(n) = as_f64(v) {
                Ok((n, 0.0))
            } else {
                Err(err(format!("{} must be complex or number", name)))
            }
        }
    }
}

fn err<T: Into<String>>(m: T) -> String {
    m.into()
}

fn ensure_arity(actual: usize, expected: usize, sig: &str) -> Result<(), String> {
    if actual != expected {
        Err(err(sig))
    } else {
        Ok(())
    }
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a.abs()
}

fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        return 0;
    }
    (a * b).abs() / gcd(a, b)
}

fn factorial(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    let mut res = 1;
    for i in 1..=n {
        res *= i;
    }
    res
}

fn comb(n: i64, k: i64) -> f64 {
    if k < 0 || k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut res = 1.0;
    for i in 1..=k {
        res = res * (n - k + i) as f64 / i as f64;
    }
    res.round()
}

fn perm(n: i64, k: i64) -> f64 {
    if k < 0 || k > n {
        return 0.0;
    }
    let mut res = 1.0;
    for i in (n - k + 1)..=n {
        res *= i as f64;
    }
    res.round()
}

fn isqrt(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    (n as f64).sqrt() as i64
}

fn remainder(x: f64, y: f64) -> f64 {
    if y == 0.0 || x.is_infinite() || y.is_nan() {
        return std::f64::NAN;
    }
    let q = (x / y).round();
    x - q * y
}

fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        (x, 0)
    } else {
        let lg = x.abs().log2().floor() as i32 + 1;
        let mantissa = x / 2.0f64.powi(lg);
        (mantissa, lg)
    }
}

fn nextafter(x: f64, y: f64) -> f64 {
    if x == y {
        return y;
    }
    let bits = x.to_bits();
    if y > x {
        if x >= 0.0 {
            f64::from_bits(bits + 1)
        } else {
            f64::from_bits(bits - 1)
        }
    } else {
        if x > 0.0 {
            f64::from_bits(bits - 1)
        } else if x < 0.0 {
            f64::from_bits(bits + 1)
        } else {
            -f64::from_bits(1)
        }
    }
}

fn ulp(x: f64) -> f64 {
    if !x.is_finite() {
        if x.is_nan() { x } else { std::f64::INFINITY }
    } else {
        let x_abs = x.abs();
        let next = nextafter(x_abs, std::f64::INFINITY);
        next - x_abs
    }
}

fn val_to_vec_f64(v: &Value) -> Result<Vec<f64>, String> {
    match v {
        Value::Array(arr) => {
            let mut res = Vec::new();
            for item in arr.iter() {
                res.push(expect_num(item, "iterable element")?);
            }
            Ok(res)
        }
        _ => Err("Expected array/iterable".to_string()),
    }
}

fn dist(p: &[f64], q: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (pi, qi) in p.iter().zip(q.iter()) {
        sum += (pi - qi) * (pi - qi);
    }
    sum.sqrt()
}

fn fsum(arr: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut c = 0.0;
    for &x in arr {
        let y = x - c;
        let t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    sum
}

fn prod(arr: &[f64], start: f64) -> f64 {
    let mut p = start;
    for &x in arr {
        p *= x;
    }
    p
}

fn sumprod(p: &[f64], q: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (pi, qi) in p.iter().zip(q.iter()) {
        sum += pi * qi;
    }
    sum
}

fn erf(x: f64) -> f64 {
    if x < 0.0 {
        return -erf(-x);
    }
    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    y
}

fn gamma(x: f64) -> f64 {
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x))
    } else {
        let x = x - 1.0;
        let p = [
            0.99999999999980993,
            676.5203681218851,
            -1259.1392167224028,
            771.32342877765313,
            -176.61502916214059,
            12.507343278686905,
            -0.13857109526572012,
            9.9843695780195716e-6,
            1.505632730049959e-7,
        ];
        let mut y = p[0];
        for i in 1..9 {
            y += p[i] / (x + i as f64);
        }
        let t = x + 7.5;
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * y
    }
}

fn complex_sqrt(r: f64, i: f64) -> (f64, f64) {
    let mag = (r * r + i * i).sqrt();
    let phi = i.atan2(r);
    (
        mag.sqrt() * (phi / 2.0).cos(),
        mag.sqrt() * (phi / 2.0).sin(),
    )
}

fn complex_log(r: f64, i: f64) -> (f64, f64) {
    let mag = (r * r + i * i).sqrt();
    let phi = i.atan2(r);
    (mag.ln(), phi)
}

// Math functions implementation wrappers
fn math_random(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    if let Some(ref mut rng) = *guard {
        let val: f64 = rng.gen_range(0.0..1.0);
        Ok(Value::Number(val))
    } else {
        Ok(Value::Number(rand::random::<f64>()))
    }
}

fn math_seed(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.seed(seed)")?;
    let seed = expect_num(&args[0], "seed")?;
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    *guard = Some(StdRng::seed_from_u64(seed as u64));
    Ok(Value::Null)
}

fn math_random_int(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.randomInt(min, max)")?;
    let min = expect_num(&args[0], "min")? as i64;
    let max = expect_num(&args[1], "max")? as i64;
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    let val = if let Some(ref mut rng) = *guard {
        rng.gen_range(min..=max)
    } else {
        rand::thread_rng().gen_range(min..=max)
    };
    Ok(Value::Number(val as f64))
}

fn math_random_range(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.randomRange(min, max)")?;
    let min = expect_num(&args[0], "min")?;
    let max = expect_num(&args[1], "max")?;
    let mut guard = SEEDED_RNG.lock().expect("SEEDED_RNG mutex poisoned");
    let val = if let Some(ref mut rng) = *guard {
        rng.gen_range(min..max)
    } else {
        rand::thread_rng().gen_range(min..max)
    };
    Ok(Value::Number(val))
}

fn math_floor(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.floor(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.floor()))
}

fn math_ceil(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.ceil(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.ceil()))
}

fn math_round(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.round(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.round()))
}

fn math_abs(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.abs(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.abs()))
}

fn math_min(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err(err("Math.min(a, b, ...)"));
    }
    let mut m = f64::INFINITY;
    for v in &args {
        let n = expect_num(v, "arg")?;
        if n < m {
            m = n;
        }
    }
    Ok(Value::Number(m))
}

fn math_max(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err(err("Math.max(a, b, ...)"));
    }
    let mut m = f64::NEG_INFINITY;
    for v in &args {
        let n = expect_num(v, "arg")?;
        if n > m {
            m = n;
        }
    }
    Ok(Value::Number(m))
}

fn math_pow(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.pow(x, y)")?;
    let x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    Ok(Value::Number(x.powf(y)))
}

fn math_sqrt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.sqrt(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.sqrt()))
}

fn math_sin(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.sin(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.sin()))
}

fn math_cos(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.cos(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.cos()))
}

fn math_tan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.tan(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.tan()))
}

fn math_asin(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.asin(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.asin()))
}

fn math_acos(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.acos(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.acos()))
}

fn math_atan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.atan(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.atan()))
}

fn math_atan2(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.atan2(y, x)")?;
    let y = expect_num(&args[0], "y")?;
    let x = expect_num(&args[1], "x")?;
    Ok(Value::Number(y.atan2(x)))
}

fn math_exp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.exp(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.exp()))
}

fn math_log(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err(err("Math.log(x, [base])"));
    }
    let n = expect_num(&args[0], "x")?;
    if args.len() > 1 {
        let base = expect_num(&args[1], "base")?;
        Ok(Value::Number(n.log(base)))
    } else {
        Ok(Value::Number(n.ln()))
    }
}

fn math_log10(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.log10(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.log10()))
}

fn math_log2(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.log2(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.log2()))
}

fn math_trunc(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.trunc(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.trunc()))
}

fn math_sign(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.sign(x)")?;
    let n = expect_num(&args[0], "x")?;
    let s = if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        0.0
    };
    Ok(Value::Number(s))
}

fn math_sinh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.sinh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.sinh()))
}

fn math_cosh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.cosh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.cosh()))
}

fn math_tanh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.tanh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.tanh()))
}

fn math_asinh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.asinh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.asinh()))
}

fn math_acosh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.acosh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.acosh()))
}

fn math_atanh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.atanh(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.atanh()))
}

fn math_deg_to_rad(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.degToRad(deg)")?;
    let n = expect_num(&args[0], "deg")?;
    Ok(Value::Number(n * std::f64::consts::PI / 180.0))
}

fn math_rad_to_deg(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.radToDeg(rad)")?;
    let n = expect_num(&args[0], "rad")?;
    Ok(Value::Number(n * 180.0 / std::f64::consts::PI))
}

fn math_clamp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 3, "Math.clamp(val, min, max)")?;
    let val = expect_num(&args[0], "val")?;
    let min = expect_num(&args[1], "min")?;
    let max = expect_num(&args[2], "max")?;
    Ok(Value::Number(val.clamp(min, max)))
}

fn math_lerp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 3, "Math.lerp(a, b, t)")?;
    let a = expect_num(&args[0], "a")?;
    let b = expect_num(&args[1], "b")?;
    let t = expect_num(&args[2], "t")?;
    Ok(Value::Number(a + (b - a) * t))
}

fn math_hypot(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Number(0.0));
    }
    let mut sum = 0.0;
    for v in &args {
        let n = expect_num(v, "coordinate")?;
        sum += n * n;
    }
    Ok(Value::Number(sum.sqrt()))
}

fn math_cbrt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.cbrt(x)")?;
    let n = expect_num(&args[0], "x")?;
    Ok(Value::Number(n.cbrt()))
}

fn math_gcd_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Number(0.0));
    }
    let mut current = expect_num(&args[0], "arg")? as i64;
    for v in &args[1..] {
        let n = expect_num(v, "arg")? as i64;
        current = gcd(current, n);
    }
    Ok(Value::Number(current as f64))
}

fn math_lcm_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Number(1.0));
    }
    let mut current = expect_num(&args[0], "arg")? as i64;
    for v in &args[1..] {
        let n = expect_num(v, "arg")? as i64;
        current = lcm(current, n);
    }
    Ok(Value::Number(current as f64))
}

fn math_factorial_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.factorial(n)")?;
    let n = expect_num(&args[0], "n")? as i64;
    Ok(Value::Number(factorial(n) as f64))
}

fn math_comb_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.comb(n, k)")?;
    let n = expect_num(&args[0], "n")? as i64;
    let k = expect_num(&args[1], "k")? as i64;
    Ok(Value::Number(comb(n, k)))
}

fn math_perm_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.perm(n, k)")?;
    let n = expect_num(&args[0], "n")? as i64;
    let k = expect_num(&args[1], "k")? as i64;
    Ok(Value::Number(perm(n, k)))
}

fn math_isqrt_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.isqrt(n)")?;
    let n = expect_num(&args[0], "n")? as i64;
    Ok(Value::Number(isqrt(n) as f64))
}

fn math_fma(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 3, "Math.fma(x, y, z)")?;
    let x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    let z = expect_num(&args[2], "z")?;
    Ok(Value::Number(x.mul_add(y, z)))
}

fn math_fmod(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.fmod(x, y)")?;
    let x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    Ok(Value::Number(x % y))
}

fn math_modf(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.modf(x)")?;
    let x = expect_num(&args[0], "x")?;
    let integral = x.trunc();
    let fract = x - integral;
    Ok(Value::Tuple(vec![
        Value::Number(fract),
        Value::Number(integral),
    ]))
}

fn math_remainder_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.remainder(x, y)")?;
    let x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    Ok(Value::Number(remainder(x, y)))
}

fn math_copysign(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.copysign(x, y)")?;
    let x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    Ok(Value::Number(x.copysign(y)))
}

fn math_frexp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.frexp(x)")?;
    let x = expect_num(&args[0], "x")?;
    let (m, e) = frexp(x);
    Ok(Value::Tuple(vec![
        Value::Number(m),
        Value::Number(e as f64),
    ]))
}

fn math_ldexp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.ldexp(x, i)")?;
    let x = expect_num(&args[0], "x")?;
    let i = expect_num(&args[1], "i")? as i32;
    Ok(Value::Number(x * 2.0f64.powi(i)))
}

fn math_isclose(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err(err("Math.isclose(a, b, [rel_tol], [abs_tol])"));
    }
    let a = expect_num(&args[0], "a")?;
    let b = expect_num(&args[1], "b")?;
    let rel_tol = if args.len() > 2 {
        expect_num(&args[2], "rel_tol")?
    } else {
        1e-09
    };
    let abs_tol = if args.len() > 3 {
        expect_num(&args[3], "abs_tol")?
    } else {
        0.0
    };
    let close = (a - b).abs() <= f64::max(rel_tol * f64::max(a.abs(), b.abs()), abs_tol);
    Ok(Value::Bool(close))
}

fn math_isfinite(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.isfinite(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Bool(x.is_finite()))
}

fn math_isinf(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.isinf(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Bool(x.is_infinite()))
}

fn math_isnan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.isnan(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Bool(x.is_nan()))
}

fn math_nextafter_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err(err("Math.nextafter(x, y, [steps])"));
    }
    let mut x = expect_num(&args[0], "x")?;
    let y = expect_num(&args[1], "y")?;
    let steps = if args.len() > 2 {
        expect_num(&args[2], "steps")? as i64
    } else {
        1
    };
    for _ in 0..steps {
        x = nextafter(x, y);
    }
    Ok(Value::Number(x))
}

fn math_ulp_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.ulp(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(ulp(x)))
}

fn math_exp2(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.exp2(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(x.exp2()))
}

fn math_expm1(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.expm1(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(x.exp_m1()))
}

fn math_log1p(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.log1p(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(x.ln_1p()))
}

fn math_dist_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.dist(p, q)")?;
    let p = val_to_vec_f64(&args[0])?;
    let q = val_to_vec_f64(&args[1])?;
    Ok(Value::Number(dist(&p, &q)))
}

fn math_fsum_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.fsum(iterable)")?;
    let arr = val_to_vec_f64(&args[0])?;
    Ok(Value::Number(fsum(&arr)))
}

fn math_prod_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err(err("Math.prod(iterable, [start])"));
    }
    let arr = val_to_vec_f64(&args[0])?;
    let start = if args.len() > 1 {
        expect_num(&args[1], "start")?
    } else {
        1.0
    };
    Ok(Value::Number(prod(&arr, start)))
}

fn math_sumprod_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "Math.sumprod(p, q)")?;
    let p = val_to_vec_f64(&args[0])?;
    let q = val_to_vec_f64(&args[1])?;
    Ok(Value::Number(sumprod(&p, &q)))
}

fn math_erf_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.erf(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(erf(x)))
}

fn math_erfc_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.erfc(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(1.0 - erf(x)))
}

fn math_gamma_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.gamma(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(gamma(x)))
}

fn math_lgamma_fn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "Math.lgamma(x)")?;
    let x = expect_num(&args[0], "x")?;
    Ok(Value::Number(gamma(x).abs().ln()))
}

fn math_format_decimal(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err(err("Math.formatDecimal(n, [precision])"));
    }
    let n = expect_num(&args[0], "n")?;
    let precision = if args.len() > 1 {
        expect_num(&args[1], "precision")? as usize
    } else {
        6
    };
    Ok(Value::Str(format!("{:.*}", precision, n)))
}

// CMath Namespace wrappers
fn cmath_cos(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.cos(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Complex(r.cos() * i.cosh(), -r.sin() * i.sinh()))
}

fn cmath_sin(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.sin(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Complex(r.sin() * i.cosh(), r.cos() * i.sinh()))
}

fn cmath_tan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.tan(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let denom = (2.0 * r).cos() + (2.0 * i).cosh();
    if denom == 0.0 {
        return Ok(Value::Complex(std::f64::NAN, std::f64::NAN));
    }
    Ok(Value::Complex(
        (2.0 * r).sin() / denom,
        (2.0 * i).sinh() / denom,
    ))
}

fn cmath_acos(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.acos(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let z_sq_r = r * r - i * i;
    let z_sq_i = 2.0 * r * i;
    let (sr, si) = complex_sqrt(1.0 - z_sq_r, -z_sq_i);
    let (lr, li) = complex_log(r - si, i + sr);
    Ok(Value::Complex(li, -lr))
}

fn cmath_asin(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.asin(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let z_sq_r = r * r - i * i;
    let z_sq_i = 2.0 * r * i;
    let (sr, si) = complex_sqrt(1.0 - z_sq_r, -z_sq_i);
    let (lr, li) = complex_log(-i + sr, r + si);
    Ok(Value::Complex(li, -lr))
}

fn cmath_atan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.atan(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let (lr1, li1) = complex_log(1.0 + i, -r);
    let (lr2, li2) = complex_log(1.0 - i, r);
    Ok(Value::Complex(0.5 * (li2 - li1), 0.5 * (lr1 - lr2)))
}

fn cmath_cosh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.cosh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Complex(r.cosh() * i.cos(), r.sinh() * i.sin()))
}

fn cmath_sinh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.sinh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Complex(r.sinh() * i.cos(), r.cosh() * i.sin()))
}

fn cmath_tanh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.tanh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let denom = r.cosh() * 2.0 + i.cos() * 2.0;
    if denom == 0.0 {
        return Ok(Value::Complex(std::f64::NAN, std::f64::NAN));
    }
    Ok(Value::Complex(
        r.sinh() * 2.0 / denom,
        i.sin() * 2.0 / denom,
    ))
}

fn cmath_acosh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.acosh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let z_sq_r = r * r - i * i;
    let z_sq_i = 2.0 * r * i;
    let (sr, si) = complex_sqrt(z_sq_r - 1.0, z_sq_i);
    let (lr, li) = complex_log(r + sr, i + si);
    Ok(Value::Complex(lr, li))
}

fn cmath_asinh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.asinh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let z_sq_r = r * r - i * i;
    let z_sq_i = 2.0 * r * i;
    let (sr, si) = complex_sqrt(z_sq_r + 1.0, z_sq_i);
    let (lr, li) = complex_log(r + sr, i + si);
    Ok(Value::Complex(lr, li))
}

fn cmath_atanh(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.atanh(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let (lr1, li1) = complex_log(1.0 + r, i);
    let (lr2, li2) = complex_log(1.0 - r, -i);
    Ok(Value::Complex(0.5 * (lr1 - lr2), 0.5 * (li1 - li2)))
}

fn cmath_exp(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.exp(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Complex(r.exp() * i.cos(), r.exp() * i.sin()))
}

fn cmath_log(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err(err("cmath.log(z, [base])"));
    }
    let (r, i) = expect_complex(&args[0], "z")?;
    let (mut lr, mut li) = complex_log(r, i);
    if args.len() > 1 {
        let base = expect_num(&args[1], "base")?;
        let denom = base.ln();
        lr /= denom;
        li /= denom;
    }
    Ok(Value::Complex(lr, li))
}

fn cmath_log10(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.log10(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let (mut lr, mut li) = complex_log(r, i);
    let denom = 10.0f64.ln();
    lr /= denom;
    li /= denom;
    Ok(Value::Complex(lr, li))
}

fn cmath_sqrt(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.sqrt(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let (sr, si) = complex_sqrt(r, i);
    Ok(Value::Complex(sr, si))
}

fn cmath_phase(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.phase(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Number(i.atan2(r)))
}

fn cmath_polar(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.polar(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    let rad = (r * r + i * i).sqrt();
    let phi = i.atan2(r);
    Ok(Value::Tuple(vec![Value::Number(rad), Value::Number(phi)]))
}

fn cmath_rect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "cmath.rect(r, phi)")?;
    let r = expect_num(&args[0], "r")?;
    let phi = expect_num(&args[1], "phi")?;
    Ok(Value::Complex(r * phi.cos(), r * phi.sin()))
}

fn cmath_isfinite(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.isfinite(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Bool(r.is_finite() && i.is_finite()))
}

fn cmath_isinf(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.isinf(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Bool(r.is_infinite() || i.is_infinite()))
}

fn cmath_isnan(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "cmath.isnan(z)")?;
    let (r, i) = expect_complex(&args[0], "z")?;
    Ok(Value::Bool(r.is_nan() || i.is_nan()))
}

fn cmath_isclose(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err(err("cmath.isclose(a, b, [rel_tol], [abs_tol])"));
    }
    let (ar, ai) = expect_complex(&args[0], "a")?;
    let (br, bi) = expect_complex(&args[1], "b")?;
    let rel_tol = if args.len() > 2 {
        expect_num(&args[2], "rel_tol")?
    } else {
        1e-09
    };
    let abs_tol = if args.len() > 3 {
        expect_num(&args[3], "abs_tol")?
    } else {
        0.0
    };
    let diff_sq = (ar - br) * (ar - br) + (ai - bi) * (ai - bi);
    let a_mag = (ar * ar + ai * ai).sqrt();
    let b_mag = (br * br + bi * bi).sqrt();
    let limit = rel_tol * f64::max(a_mag, b_mag);
    let limit = f64::max(limit, abs_tol);
    Ok(Value::Bool(diff_sq.sqrt() <= limit))
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    // Math Module registration
    registry.register(
        "Math",
        "math",
        "Math namespace for numeric utilities",
        |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
            let mut methods = HashMap::default();

            // Constants
            methods.insert("PI".to_string(), Value::Number(std::f64::consts::PI));
            methods.insert("pi".to_string(), Value::Number(std::f64::consts::PI));
            methods.insert("E".to_string(), Value::Number(std::f64::consts::E));
            methods.insert("e".to_string(), Value::Number(std::f64::consts::E));
            methods.insert("TAU".to_string(), Value::Number(std::f64::consts::TAU));
            methods.insert("tau".to_string(), Value::Number(std::f64::consts::TAU));
            methods.insert("SQRT2".to_string(), Value::Number(std::f64::consts::SQRT_2));
            methods.insert(
                "SQRT1_2".to_string(),
                Value::Number(std::f64::consts::FRAC_1_SQRT_2),
            );
            methods.insert("LN2".to_string(), Value::Number(std::f64::consts::LN_2));
            methods.insert("LN10".to_string(), Value::Number(std::f64::consts::LN_10));
            methods.insert("inf".to_string(), Value::Number(std::f64::INFINITY));
            methods.insert("nan".to_string(), Value::Number(std::f64::NAN));

            // Functions
            methods.insert(
                "random".to_string(),
                Value::Function(NativeFn(Arc::new(math_random))),
            );
            methods.insert(
                "seed".to_string(),
                Value::Function(NativeFn(Arc::new(math_seed))),
            );
            methods.insert(
                "randomInt".to_string(),
                Value::Function(NativeFn(Arc::new(math_random_int))),
            );
            methods.insert(
                "randomRange".to_string(),
                Value::Function(NativeFn(Arc::new(math_random_range))),
            );
            methods.insert(
                "floor".to_string(),
                Value::Function(NativeFn(Arc::new(math_floor))),
            );
            methods.insert(
                "ceil".to_string(),
                Value::Function(NativeFn(Arc::new(math_ceil))),
            );
            methods.insert(
                "round".to_string(),
                Value::Function(NativeFn(Arc::new(math_round))),
            );
            methods.insert(
                "abs".to_string(),
                Value::Function(NativeFn(Arc::new(math_abs))),
            );
            methods.insert(
                "fabs".to_string(),
                Value::Function(NativeFn(Arc::new(math_abs))),
            );
            methods.insert(
                "min".to_string(),
                Value::Function(NativeFn(Arc::new(math_min))),
            );
            methods.insert(
                "max".to_string(),
                Value::Function(NativeFn(Arc::new(math_max))),
            );
            methods.insert(
                "pow".to_string(),
                Value::Function(NativeFn(Arc::new(math_pow))),
            );
            methods.insert(
                "sqrt".to_string(),
                Value::Function(NativeFn(Arc::new(math_sqrt))),
            );
            methods.insert(
                "sin".to_string(),
                Value::Function(NativeFn(Arc::new(math_sin))),
            );
            methods.insert(
                "cos".to_string(),
                Value::Function(NativeFn(Arc::new(math_cos))),
            );
            methods.insert(
                "tan".to_string(),
                Value::Function(NativeFn(Arc::new(math_tan))),
            );
            methods.insert(
                "asin".to_string(),
                Value::Function(NativeFn(Arc::new(math_asin))),
            );
            methods.insert(
                "acos".to_string(),
                Value::Function(NativeFn(Arc::new(math_acos))),
            );
            methods.insert(
                "atan".to_string(),
                Value::Function(NativeFn(Arc::new(math_atan))),
            );
            methods.insert(
                "atan2".to_string(),
                Value::Function(NativeFn(Arc::new(math_atan2))),
            );
            methods.insert(
                "exp".to_string(),
                Value::Function(NativeFn(Arc::new(math_exp))),
            );
            methods.insert(
                "log".to_string(),
                Value::Function(NativeFn(Arc::new(math_log))),
            );
            methods.insert(
                "log10".to_string(),
                Value::Function(NativeFn(Arc::new(math_log10))),
            );
            methods.insert(
                "log2".to_string(),
                Value::Function(NativeFn(Arc::new(math_log2))),
            );
            methods.insert(
                "trunc".to_string(),
                Value::Function(NativeFn(Arc::new(math_trunc))),
            );
            methods.insert(
                "sign".to_string(),
                Value::Function(NativeFn(Arc::new(math_sign))),
            );
            methods.insert(
                "sinh".to_string(),
                Value::Function(NativeFn(Arc::new(math_sinh))),
            );
            methods.insert(
                "cosh".to_string(),
                Value::Function(NativeFn(Arc::new(math_cosh))),
            );
            methods.insert(
                "tanh".to_string(),
                Value::Function(NativeFn(Arc::new(math_tanh))),
            );
            methods.insert(
                "asinh".to_string(),
                Value::Function(NativeFn(Arc::new(math_asinh))),
            );
            methods.insert(
                "acosh".to_string(),
                Value::Function(NativeFn(Arc::new(math_acosh))),
            );
            methods.insert(
                "atanh".to_string(),
                Value::Function(NativeFn(Arc::new(math_atanh))),
            );
            methods.insert(
                "degToRad".to_string(),
                Value::Function(NativeFn(Arc::new(math_deg_to_rad))),
            );
            methods.insert(
                "radToDeg".to_string(),
                Value::Function(NativeFn(Arc::new(math_rad_to_deg))),
            );
            methods.insert(
                "degrees".to_string(),
                Value::Function(NativeFn(Arc::new(math_rad_to_deg))),
            );
            methods.insert(
                "radians".to_string(),
                Value::Function(NativeFn(Arc::new(math_deg_to_rad))),
            );
            methods.insert(
                "clamp".to_string(),
                Value::Function(NativeFn(Arc::new(math_clamp))),
            );
            methods.insert(
                "lerp".to_string(),
                Value::Function(NativeFn(Arc::new(math_lerp))),
            );
            methods.insert(
                "hypot".to_string(),
                Value::Function(NativeFn(Arc::new(math_hypot))),
            );
            methods.insert(
                "cbrt".to_string(),
                Value::Function(NativeFn(Arc::new(math_cbrt))),
            );
            methods.insert(
                "gcd".to_string(),
                Value::Function(NativeFn(Arc::new(math_gcd_fn))),
            );
            methods.insert(
                "lcm".to_string(),
                Value::Function(NativeFn(Arc::new(math_lcm_fn))),
            );
            methods.insert(
                "factorial".to_string(),
                Value::Function(NativeFn(Arc::new(math_factorial_fn))),
            );
            methods.insert(
                "comb".to_string(),
                Value::Function(NativeFn(Arc::new(math_comb_fn))),
            );
            methods.insert(
                "perm".to_string(),
                Value::Function(NativeFn(Arc::new(math_perm_fn))),
            );
            methods.insert(
                "isqrt".to_string(),
                Value::Function(NativeFn(Arc::new(math_isqrt_fn))),
            );
            methods.insert(
                "fma".to_string(),
                Value::Function(NativeFn(Arc::new(math_fma))),
            );
            methods.insert(
                "fmod".to_string(),
                Value::Function(NativeFn(Arc::new(math_fmod))),
            );
            methods.insert(
                "modf".to_string(),
                Value::Function(NativeFn(Arc::new(math_modf))),
            );
            methods.insert(
                "remainder".to_string(),
                Value::Function(NativeFn(Arc::new(math_remainder_fn))),
            );
            methods.insert(
                "copysign".to_string(),
                Value::Function(NativeFn(Arc::new(math_copysign))),
            );
            methods.insert(
                "frexp".to_string(),
                Value::Function(NativeFn(Arc::new(math_frexp))),
            );
            methods.insert(
                "ldexp".to_string(),
                Value::Function(NativeFn(Arc::new(math_ldexp))),
            );
            methods.insert(
                "isclose".to_string(),
                Value::Function(NativeFn(Arc::new(math_isclose))),
            );
            methods.insert(
                "isfinite".to_string(),
                Value::Function(NativeFn(Arc::new(math_isfinite))),
            );
            methods.insert(
                "isFinite".to_string(),
                Value::Function(NativeFn(Arc::new(math_isfinite))),
            );
            methods.insert(
                "isinf".to_string(),
                Value::Function(NativeFn(Arc::new(math_isinf))),
            );
            methods.insert(
                "isInf".to_string(),
                Value::Function(NativeFn(Arc::new(math_isinf))),
            );
            methods.insert(
                "isnan".to_string(),
                Value::Function(NativeFn(Arc::new(math_isnan))),
            );
            methods.insert(
                "isNaN".to_string(),
                Value::Function(NativeFn(Arc::new(math_isnan))),
            );
            methods.insert(
                "nextafter".to_string(),
                Value::Function(NativeFn(Arc::new(math_nextafter_fn))),
            );
            methods.insert(
                "ulp".to_string(),
                Value::Function(NativeFn(Arc::new(math_ulp_fn))),
            );
            methods.insert(
                "exp2".to_string(),
                Value::Function(NativeFn(Arc::new(math_exp2))),
            );
            methods.insert(
                "expm1".to_string(),
                Value::Function(NativeFn(Arc::new(math_expm1))),
            );
            methods.insert(
                "log1p".to_string(),
                Value::Function(NativeFn(Arc::new(math_log1p))),
            );
            methods.insert(
                "dist".to_string(),
                Value::Function(NativeFn(Arc::new(math_dist_fn))),
            );
            methods.insert(
                "fsum".to_string(),
                Value::Function(NativeFn(Arc::new(math_fsum_fn))),
            );
            methods.insert(
                "prod".to_string(),
                Value::Function(NativeFn(Arc::new(math_prod_fn))),
            );
            methods.insert(
                "sumprod".to_string(),
                Value::Function(NativeFn(Arc::new(math_sumprod_fn))),
            );
            methods.insert(
                "erf".to_string(),
                Value::Function(NativeFn(Arc::new(math_erf_fn))),
            );
            methods.insert(
                "erfc".to_string(),
                Value::Function(NativeFn(Arc::new(math_erfc_fn))),
            );
            methods.insert(
                "gamma".to_string(),
                Value::Function(NativeFn(Arc::new(math_gamma_fn))),
            );
            methods.insert(
                "lgamma".to_string(),
                Value::Function(NativeFn(Arc::new(math_lgamma_fn))),
            );
            methods.insert(
                "formatDecimal".to_string(),
                Value::Function(NativeFn(Arc::new(math_format_decimal))),
            );

            Ok(Value::Object(Arc::new(methods)))
        },
    );

    // CMath Module registration
    registry.register(
        "cmath",
        "math",
        "Complex Math namespace for complex number operations",
        |_env: &mut dyn BuiltinEnv, _args: Vec<Value>| {
            let mut methods = HashMap::default();

            // Constants
            methods.insert("pi".to_string(), Value::Number(std::f64::consts::PI));
            methods.insert("e".to_string(), Value::Number(std::f64::consts::E));
            methods.insert("tau".to_string(), Value::Number(std::f64::consts::TAU));
            methods.insert("inf".to_string(), Value::Number(std::f64::INFINITY));
            methods.insert("nan".to_string(), Value::Number(std::f64::NAN));
            methods.insert("infj".to_string(), Value::Complex(0.0, std::f64::INFINITY));
            methods.insert("nanj".to_string(), Value::Complex(0.0, std::f64::NAN));

            // Functions
            methods.insert(
                "cos".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_cos))),
            );
            methods.insert(
                "sin".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_sin))),
            );
            methods.insert(
                "tan".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_tan))),
            );
            methods.insert(
                "acos".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_acos))),
            );
            methods.insert(
                "asin".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_asin))),
            );
            methods.insert(
                "atan".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_atan))),
            );
            methods.insert(
                "cosh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_cosh))),
            );
            methods.insert(
                "sinh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_sinh))),
            );
            methods.insert(
                "tanh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_tanh))),
            );
            methods.insert(
                "acosh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_acosh))),
            );
            methods.insert(
                "asinh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_asinh))),
            );
            methods.insert(
                "atanh".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_atanh))),
            );
            methods.insert(
                "exp".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_exp))),
            );
            methods.insert(
                "log".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_log))),
            );
            methods.insert(
                "log10".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_log10))),
            );
            methods.insert(
                "sqrt".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_sqrt))),
            );
            methods.insert(
                "phase".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_phase))),
            );
            methods.insert(
                "polar".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_polar))),
            );
            methods.insert(
                "rect".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_rect))),
            );
            methods.insert(
                "isfinite".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isfinite))),
            );
            methods.insert(
                "isFinite".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isfinite))),
            );
            methods.insert(
                "isinf".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isinf))),
            );
            methods.insert(
                "isInf".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isinf))),
            );
            methods.insert(
                "isnan".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isnan))),
            );
            methods.insert(
                "isNaN".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isnan))),
            );
            methods.insert(
                "isclose".to_string(),
                Value::Function(NativeFn(Arc::new(cmath_isclose))),
            );

            Ok(Value::Object(Arc::new(methods)))
        },
    );

    // Keep individual JIT/AOT registrations for Math
    registry.register("Math.PI", "math", "Pi constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::PI))
    });
    registry.register("Math.pi", "math", "Pi constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::PI))
    });
    registry.register("Math.E", "math", "Euler's number", |_env, _| {
        Ok(Value::Number(std::f64::consts::E))
    });
    registry.register("Math.e", "math", "Euler's number", |_env, _| {
        Ok(Value::Number(std::f64::consts::E))
    });
    registry.register("Math.TAU", "math", "Tau constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::TAU))
    });
    registry.register("Math.tau", "math", "Tau constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::TAU))
    });
    registry.register("Math.SQRT2", "math", "Square root of 2", |_env, _| {
        Ok(Value::Number(std::f64::consts::SQRT_2))
    });
    registry.register("Math.SQRT1_2", "math", "1/sqrt(2) constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::FRAC_1_SQRT_2))
    });
    registry.register("Math.LN2", "math", "Natural log of 2", |_env, _| {
        Ok(Value::Number(std::f64::consts::LN_2))
    });
    registry.register("Math.LN10", "math", "Natural log of 10", |_env, _| {
        Ok(Value::Number(std::f64::consts::LN_10))
    });
    registry.register("Math.inf", "math", "Infinity", |_env, _| {
        Ok(Value::Number(std::f64::INFINITY))
    });
    registry.register("Math.nan", "math", "NaN", |_env, _| {
        Ok(Value::Number(std::f64::NAN))
    });

    registry.register("Math.random", "math", "Random number", |env, args| {
        math_random(env, args)
    });
    registry.register("Math.seed", "math", "Seed RNG", |env, args| {
        math_seed(env, args)
    });
    registry.register("Math.randomInt", "math", "Random integer", |env, args| {
        math_random_int(env, args)
    });
    registry.register("Math.randomRange", "math", "Random range", |env, args| {
        math_random_range(env, args)
    });
    registry.register("Math.floor", "math", "Floor", |env, args| {
        math_floor(env, args)
    });
    registry.register("Math.ceil", "math", "Ceil", |env, args| {
        math_ceil(env, args)
    });
    registry.register("Math.round", "math", "Round", |env, args| {
        math_round(env, args)
    });
    registry.register("Math.abs", "math", "Absolute value", |env, args| {
        math_abs(env, args)
    });
    registry.register("Math.fabs", "math", "Float absolute value", |env, args| {
        math_abs(env, args)
    });
    registry.register("Math.min", "math", "Minimum", |env, args| {
        math_min(env, args)
    });
    registry.register("Math.max", "math", "Maximum", |env, args| {
        math_max(env, args)
    });
    registry.register("Math.pow", "math", "Power", |env, args| math_pow(env, args));
    registry.register("Math.sqrt", "math", "Square root", |env, args| {
        math_sqrt(env, args)
    });
    registry.register("Math.sin", "math", "Sine", |env, args| math_sin(env, args));
    registry.register("Math.cos", "math", "Cosine", |env, args| {
        math_cos(env, args)
    });
    registry.register("Math.tan", "math", "Tangent", |env, args| {
        math_tan(env, args)
    });
    registry.register("Math.asin", "math", "Arcsine", |env, args| {
        math_asin(env, args)
    });
    registry.register("Math.acos", "math", "Arccosine", |env, args| {
        math_acos(env, args)
    });
    registry.register("Math.atan", "math", "Arctangent", |env, args| {
        math_atan(env, args)
    });
    registry.register("Math.atan2", "math", "atan2(y, x)", |env, args| {
        math_atan2(env, args)
    });
    registry.register("Math.exp", "math", "e^x", |env, args| math_exp(env, args));
    registry.register("Math.log", "math", "logarithm", |env, args| {
        math_log(env, args)
    });
    registry.register("Math.log10", "math", "log base 10", |env, args| {
        math_log10(env, args)
    });
    registry.register("Math.log2", "math", "log base 2", |env, args| {
        math_log2(env, args)
    });
    registry.register("Math.trunc", "math", "truncate", |env, args| {
        math_trunc(env, args)
    });
    registry.register("Math.sign", "math", "sign", |env, args| {
        math_sign(env, args)
    });
    registry.register("Math.sinh", "math", "sinh", |env, args| {
        math_sinh(env, args)
    });
    registry.register("Math.cosh", "math", "cosh", |env, args| {
        math_cosh(env, args)
    });
    registry.register("Math.tanh", "math", "tanh", |env, args| {
        math_tanh(env, args)
    });
    registry.register("Math.asinh", "math", "asinh", |env, args| {
        math_asinh(env, args)
    });
    registry.register("Math.acosh", "math", "acosh", |env, args| {
        math_acosh(env, args)
    });
    registry.register("Math.atanh", "math", "atanh", |env, args| {
        math_atanh(env, args)
    });
    registry.register("Math.degToRad", "math", "degToRad", |env, args| {
        math_deg_to_rad(env, args)
    });
    registry.register("Math.radToDeg", "math", "radToDeg", |env, args| {
        math_rad_to_deg(env, args)
    });
    registry.register("Math.degrees", "math", "degrees", |env, args| {
        math_rad_to_deg(env, args)
    });
    registry.register("Math.radians", "math", "radians", |env, args| {
        math_deg_to_rad(env, args)
    });
    registry.register("Math.clamp", "math", "clamp", |env, args| {
        math_clamp(env, args)
    });
    registry.register("Math.lerp", "math", "lerp", |env, args| {
        math_lerp(env, args)
    });
    registry.register("Math.hypot", "math", "hypot", |env, args| {
        math_hypot(env, args)
    });
    registry.register("Math.cbrt", "math", "cbrt", |env, args| {
        math_cbrt(env, args)
    });
    registry.register("Math.gcd", "math", "gcd", |env, args| {
        math_gcd_fn(env, args)
    });
    registry.register("Math.lcm", "math", "lcm", |env, args| {
        math_lcm_fn(env, args)
    });
    registry.register("Math.factorial", "math", "factorial", |env, args| {
        math_factorial_fn(env, args)
    });
    registry.register("Math.comb", "math", "comb", |env, args| {
        math_comb_fn(env, args)
    });
    registry.register("Math.perm", "math", "perm", |env, args| {
        math_perm_fn(env, args)
    });
    registry.register("Math.isqrt", "math", "isqrt", |env, args| {
        math_isqrt_fn(env, args)
    });
    registry.register("Math.fma", "math", "fma", |env, args| math_fma(env, args));
    registry.register("Math.fmod", "math", "fmod", |env, args| {
        math_fmod(env, args)
    });
    registry.register("Math.modf", "math", "modf", |env, args| {
        math_modf(env, args)
    });
    registry.register("Math.remainder", "math", "remainder", |env, args| {
        math_remainder_fn(env, args)
    });
    registry.register("Math.copysign", "math", "copysign", |env, args| {
        math_copysign(env, args)
    });
    registry.register("Math.frexp", "math", "frexp", |env, args| {
        math_frexp(env, args)
    });
    registry.register("Math.ldexp", "math", "ldexp", |env, args| {
        math_ldexp(env, args)
    });
    registry.register("Math.isclose", "math", "isclose", |env, args| {
        math_isclose(env, args)
    });
    registry.register("Math.isfinite", "math", "isfinite", |env, args| {
        math_isfinite(env, args)
    });
    registry.register("Math.isinf", "math", "isinf", |env, args| {
        math_isinf(env, args)
    });
    registry.register("Math.isnan", "math", "isnan", |env, args| {
        math_isnan(env, args)
    });
    registry.register("Math.nextafter", "math", "nextafter", |env, args| {
        math_nextafter_fn(env, args)
    });
    registry.register("Math.ulp", "math", "ulp", |env, args| {
        math_ulp_fn(env, args)
    });
    registry.register("Math.exp2", "math", "exp2", |env, args| {
        math_exp2(env, args)
    });
    registry.register("Math.expm1", "math", "expm1", |env, args| {
        math_expm1(env, args)
    });
    registry.register("Math.log1p", "math", "log1p", |env, args| {
        math_log1p(env, args)
    });
    registry.register("Math.dist", "math", "dist", |env, args| {
        math_dist_fn(env, args)
    });
    registry.register("Math.fsum", "math", "fsum", |env, args| {
        math_fsum_fn(env, args)
    });
    registry.register("Math.prod", "math", "prod", |env, args| {
        math_prod_fn(env, args)
    });
    registry.register("Math.sumprod", "math", "sumprod", |env, args| {
        math_sumprod_fn(env, args)
    });
    registry.register("Math.erf", "math", "erf", |env, args| {
        math_erf_fn(env, args)
    });
    registry.register("Math.erfc", "math", "erfc", |env, args| {
        math_erfc_fn(env, args)
    });
    registry.register("Math.gamma", "math", "gamma", |env, args| {
        math_gamma_fn(env, args)
    });
    registry.register("Math.lgamma", "math", "lgamma", |env, args| {
        math_lgamma_fn(env, args)
    });
    registry.register(
        "Math.formatDecimal",
        "math",
        "formatDecimal",
        |env, args| math_format_decimal(env, args),
    );

    // Keep individual JIT/AOT registrations for cmath
    registry.register("cmath.pi", "math", "Pi constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::PI))
    });
    registry.register("cmath.e", "math", "Euler's number", |_env, _| {
        Ok(Value::Number(std::f64::consts::E))
    });
    registry.register("cmath.tau", "math", "Tau constant", |_env, _| {
        Ok(Value::Number(std::f64::consts::TAU))
    });
    registry.register("cmath.inf", "math", "Infinity", |_env, _| {
        Ok(Value::Number(std::f64::INFINITY))
    });
    registry.register("cmath.nan", "math", "NaN", |_env, _| {
        Ok(Value::Number(std::f64::NAN))
    });
    registry.register("cmath.infj", "math", "Complex infinity", |_env, _| {
        Ok(Value::Complex(0.0, std::f64::INFINITY))
    });
    registry.register("cmath.nanj", "math", "Complex NaN", |_env, _| {
        Ok(Value::Complex(0.0, std::f64::NAN))
    });

    registry.register("cmath.cos", "math", "cos", |env, args| cmath_cos(env, args));
    registry.register("cmath.sin", "math", "sin", |env, args| cmath_sin(env, args));
    registry.register("cmath.tan", "math", "tan", |env, args| cmath_tan(env, args));
    registry.register("cmath.acos", "math", "acos", |env, args| {
        cmath_acos(env, args)
    });
    registry.register("cmath.asin", "math", "asin", |env, args| {
        cmath_asin(env, args)
    });
    registry.register("cmath.atan", "math", "atan", |env, args| {
        cmath_atan(env, args)
    });
    registry.register("cmath.cosh", "math", "cosh", |env, args| {
        cmath_cosh(env, args)
    });
    registry.register("cmath.sinh", "math", "sinh", |env, args| {
        cmath_sinh(env, args)
    });
    registry.register("cmath.tanh", "math", "tanh", |env, args| {
        cmath_tanh(env, args)
    });
    registry.register("cmath.acosh", "math", "acosh", |env, args| {
        cmath_acosh(env, args)
    });
    registry.register("cmath.asinh", "math", "asinh", |env, args| {
        cmath_asinh(env, args)
    });
    registry.register("cmath.atanh", "math", "atanh", |env, args| {
        cmath_atanh(env, args)
    });
    registry.register("cmath.exp", "math", "exp", |env, args| cmath_exp(env, args));
    registry.register("cmath.log", "math", "log", |env, args| cmath_log(env, args));
    registry.register("cmath.log10", "math", "log10", |env, args| {
        cmath_log10(env, args)
    });
    registry.register("cmath.sqrt", "math", "sqrt", |env, args| {
        cmath_sqrt(env, args)
    });
    registry.register("cmath.phase", "math", "phase", |env, args| {
        cmath_phase(env, args)
    });
    registry.register("cmath.polar", "math", "polar", |env, args| {
        cmath_polar(env, args)
    });
    registry.register("cmath.rect", "math", "rect", |env, args| {
        cmath_rect(env, args)
    });
    registry.register("cmath.isfinite", "math", "isfinite", |env, args| {
        cmath_isfinite(env, args)
    });
    registry.register("cmath.isinf", "math", "isinf", |env, args| {
        cmath_isinf(env, args)
    });
    registry.register("cmath.isnan", "math", "isnan", |env, args| {
        cmath_isnan(env, args)
    });
    registry.register("cmath.isclose", "math", "isclose", |env, args| {
        cmath_isclose(env, args)
    });
}
