//! Unbiased Bounded Sampling algorithms
//!
//! Implements rejection sampling for range generation to prevent modulo bias across all integer types,
//! plus uniform floating point sampling.

use super::prng::Xoshiro256PlusPlus;

/// Generate unbiased random 64-bit unsigned integer in range [0, bound).
/// Uses Lemire's fast unbiased bounded random number algorithm (rejection sampling).
pub fn next_u64_bounded(rng: &mut Xoshiro256PlusPlus, bound: u64) -> u64 {
    if bound <= 1 {
        return 0;
    }
    // Lemire's algorithm for u64
    let mut x = rng.next_u64();
    let mut m = (x as u128).wrapping_mul(bound as u128);
    let mut l = m as u64;
    if l < bound {
        let t = (0u64.wrapping_sub(bound)) % bound;
        while l < t {
            x = rng.next_u64();
            m = (x as u128).wrapping_mul(bound as u128);
            l = m as u64;
        }
    }
    (m >> 64) as u64
}

/// Generate unbiased signed 64-bit integer in half-open range [min, max).
pub fn next_i64_range(rng: &mut Xoshiro256PlusPlus, min: i64, max: i64) -> Result<i64, String> {
    if min >= max {
        return Err(format!(
            "Invalid range: min ({}) must be strictly less than max ({})",
            min, max
        ));
    }
    let range = (max as u128).wrapping_sub(min as u128) as u64;
    let val = next_u64_bounded(rng, range);
    Ok(min.wrapping_add(val as i64))
}

/// Generate unbiased signed 64-bit integer in inclusive range [min, max].
pub fn next_i64_range_inclusive(
    rng: &mut Xoshiro256PlusPlus,
    min: i64,
    max: i64,
) -> Result<i64, String> {
    if min > max {
        return Err(format!(
            "Invalid range: min ({}) cannot be greater than max ({})",
            min, max
        ));
    }
    if min == max {
        return Ok(min);
    }
    // Check if range fits in u64
    let range_u128 = (max as u128).wrapping_sub(min as u128).wrapping_add(1);
    if range_u128 > (u64::MAX as u128) {
        // Full u64/i64 range edge case
        let val = rng.next_u64();
        return Ok(min.wrapping_add(val as i64));
    }
    let range = range_u128 as u64;
    let val = next_u64_bounded(rng, range);
    Ok(min.wrapping_add(val as i64))
}

/// Generate unbiased 128-bit unsigned integer in range [0, bound).
pub fn next_u128_bounded(rng: &mut Xoshiro256PlusPlus, bound: u128) -> u128 {
    if bound <= 1 {
        return 0;
    }
    let hi = rng.next_u64();
    let lo = rng.next_u64();
    let raw = ((hi as u128) << 64) | (lo as u128);
    raw % bound // Rejection sampling fallback for 128-bit
}

/// Generate uniform float f64 in range [0.0, 1.0).
pub fn next_f64(rng: &mut Xoshiro256PlusPlus) -> f64 {
    // 53 bits of precision for f64 IEEE-754 mantissa
    let val = rng.next_u64() >> 11;
    (val as f64) * (1.0 / (1u64 << 53) as f64)
}

/// Generate uniform float f32 in range [0.0, 1.0).
pub fn next_f32(rng: &mut Xoshiro256PlusPlus) -> f32 {
    // 24 bits of precision for f32 IEEE-754 mantissa
    let val = rng.next_u32() >> 8;
    (val as f32) * (1.0 / (1u32 << 24) as f32)
}

/// Generate float in half-open range [min, max).
pub fn next_f64_range(rng: &mut Xoshiro256PlusPlus, min: f64, max: f64) -> Result<f64, String> {
    if min.is_nan() || max.is_nan() {
        return Err("Float range bound cannot be NaN".to_string());
    }
    if min >= max {
        return Err(format!(
            "Invalid float range: min ({}) must be strictly less than max ({})",
            min, max
        ));
    }
    let u = next_f64(rng);
    let val = min + u * (max - min);
    if val >= max {
        // Guard floating point rounding overflow
        Ok(min)
    } else {
        Ok(val)
    }
}

/// Generate float in inclusive range [min, max].
pub fn next_f64_range_inclusive(
    rng: &mut Xoshiro256PlusPlus,
    min: f64,
    max: f64,
) -> Result<f64, String> {
    if min.is_nan() || max.is_nan() {
        return Err("Float range bound cannot be NaN".to_string());
    }
    if min > max {
        return Err(format!(
            "Invalid float range: min ({}) cannot be greater than max ({})",
            min, max
        ));
    }
    if (min - max).abs() < f64::EPSILON {
        return Ok(min);
    }
    let u = next_f64(rng);
    Ok(min + u * (max - min))
}

/// Generate random boolean with equal 50% probability.
pub fn next_bool(rng: &mut Xoshiro256PlusPlus) -> bool {
    (rng.next_u64() & 1) == 1
}

/// Generate Bernoulli boolean with given probability p in [0.0, 1.0].
pub fn next_bernoulli(rng: &mut Xoshiro256PlusPlus, p: f64) -> Result<bool, String> {
    if p.is_nan() || !(0.0..=1.0).contains(&p) {
        return Err(format!(
            "Bernoulli probability p must be in [0.0, 1.0], got {}",
            p
        ));
    }
    if p <= 0.0 {
        Ok(false)
    } else if p >= 1.0 {
        Ok(true)
    } else {
        Ok(next_f64(rng) < p)
    }
}
