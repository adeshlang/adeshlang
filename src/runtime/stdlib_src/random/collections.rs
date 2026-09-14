//! Collection Randomization (Choice, Shuffle, Sample, Weighted Choice, Reservoir Sampling)
//!
//! Implements Fisher-Yates shuffle, unbiased collection sampling, weighted selection, and reservoir sampling.

use super::bounded::next_u64_bounded;
use super::prng::Xoshiro256PlusPlus;
use crate::parsing::ast::Value;

/// Select a random element from an array.
pub fn choice<'a>(rng: &mut Xoshiro256PlusPlus, items: &'a [Value]) -> Result<&'a Value, String> {
    if items.is_empty() {
        return Err("Cannot choose from an empty collection".to_string());
    }
    let idx = next_u64_bounded(rng, items.len() as u64) as usize;
    Ok(&items[idx])
}

/// Fisher-Yates in-place shuffle for a mutable slice of Values.
pub fn shuffle(rng: &mut Xoshiro256PlusPlus, items: &mut [Value]) {
    let len = items.len();
    if len <= 1 {
        return;
    }
    for i in (1..len).rev() {
        let j = next_u64_bounded(rng, (i + 1) as u64) as usize;
        items.swap(i, j);
    }
}

/// Sample k unique elements without replacement from an array using Fisher-Yates selection.
pub fn sample(
    rng: &mut Xoshiro256PlusPlus,
    items: &[Value],
    count: usize,
) -> Result<Vec<Value>, String> {
    if count > items.len() {
        return Err(format!(
            "Sample count ({}) cannot exceed collection length ({})",
            count,
            items.len()
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut pool: Vec<Value> = items.to_vec();
    let len = pool.len();
    for i in 0..count {
        let j = i + next_u64_bounded(rng, (len - i) as u64) as usize;
        pool.swap(i, j);
    }
    pool.truncate(count);
    Ok(pool)
}

/// Sample k elements with replacement from an array.
pub fn sample_with_replacement(
    rng: &mut Xoshiro256PlusPlus,
    items: &[Value],
    count: usize,
) -> Result<Vec<Value>, String> {
    if items.is_empty() {
        return Err("Cannot sample from an empty collection".to_string());
    }
    let mut result = Vec::with_capacity(count);
    let len = items.len() as u64;
    for _ in 0..count {
        let idx = next_u64_bounded(rng, len) as usize;
        result.push(items[idx].clone());
    }
    Ok(result)
}

/// Weighted choice from a slice of (item, weight) pairs or item slice + weight slice.
pub fn weighted_choice(
    rng: &mut Xoshiro256PlusPlus,
    items: &[Value],
    weights: &[f64],
) -> Result<Value, String> {
    if items.is_empty() {
        return Err("Cannot perform weighted choice on empty items collection".to_string());
    }
    if items.len() != weights.len() {
        return Err(format!(
            "Items length ({}) does not match weights length ({})",
            items.len(),
            weights.len()
        ));
    }

    let mut total_weight = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        if w.is_nan() || w < 0.0 || w.is_infinite() {
            return Err(format!("Invalid weight at index {}: {}", i, w));
        }
        total_weight += w;
    }

    if total_weight <= 0.0 {
        return Err("Total weight must be strictly positive (> 0)".to_string());
    }

    let r = super::bounded::next_f64(rng) * total_weight;
    let mut accum = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        accum += w;
        if r < accum || i == weights.len() - 1 {
            return Ok(items[i].clone());
        }
    }

    Ok(items[items.len() - 1].clone())
}

/// Reservoir sampling for selecting k items from a stream / collection in O(k) memory.
pub fn reservoir_sample(
    rng: &mut Xoshiro256PlusPlus,
    items: &[Value],
    k: usize,
) -> Result<Vec<Value>, String> {
    if k == 0 {
        return Ok(Vec::new());
    }
    if items.len() < k {
        return Err(format!(
            "Reservoir sample size ({}) cannot be greater than item stream length ({})",
            k,
            items.len()
        ));
    }

    let mut reservoir: Vec<Value> = items[..k].to_vec();
    for i in k..items.len() {
        let j = next_u64_bounded(rng, (i + 1) as u64) as usize;
        if j < k {
            reservoir[j] = items[i].clone();
        }
    }

    Ok(reservoir)
}
