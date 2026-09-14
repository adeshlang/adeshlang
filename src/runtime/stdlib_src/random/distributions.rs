//! Statistical Distributions
//!
//! Implements production-grade sampling for continuous and discrete probability distributions:
//! Normal (Gaussian), Uniform, Bernoulli, Binomial, Exponential, Poisson, Geometric, Gamma, LogNormal.

use super::bounded::{next_bernoulli, next_f64, next_f64_range};
use super::prng::Xoshiro256PlusPlus;

/// Standard Normal (Gaussian) sample ~ N(0, 1) using Box-Muller transform.
pub fn sample_standard_normal(rng: &mut Xoshiro256PlusPlus) -> f64 {
    let mut u1 = next_f64(rng);
    while u1 <= 1e-15 {
        u1 = next_f64(rng);
    }
    let u2 = next_f64(rng);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Normal distribution ~ N(mean, std_dev^2).
pub fn sample_normal(rng: &mut Xoshiro256PlusPlus, mean: f64, std_dev: f64) -> Result<f64, String> {
    if std_dev.is_nan() || std_dev < 0.0 {
        return Err(format!(
            "Standard deviation must be non-negative, got {}",
            std_dev
        ));
    }
    if mean.is_nan() {
        return Err("Mean cannot be NaN".to_string());
    }
    Ok(mean + std_dev * sample_standard_normal(rng))
}

/// Uniform continuous distribution ~ U(min, max).
pub fn sample_uniform(rng: &mut Xoshiro256PlusPlus, min: f64, max: f64) -> Result<f64, String> {
    next_f64_range(rng, min, max)
}

/// Exponential distribution ~ Exp(lambda).
pub fn sample_exponential(rng: &mut Xoshiro256PlusPlus, lambda: f64) -> Result<f64, String> {
    if lambda.is_nan() || lambda <= 0.0 {
        return Err(format!(
            "Lambda must be strictly positive (> 0), got {}",
            lambda
        ));
    }
    let mut u = next_f64(rng);
    while u <= 0.0 {
        u = next_f64(rng);
    }
    Ok(-u.ln() / lambda)
}

/// Binomial distribution ~ Binomial(n, p).
pub fn sample_binomial(rng: &mut Xoshiro256PlusPlus, n: u64, p: f64) -> Result<u64, String> {
    if p.is_nan() || !(0.0..=1.0).contains(&p) {
        return Err(format!(
            "Binomial probability p must be in [0.0, 1.0], got {}",
            p
        ));
    }
    if n == 0 || p == 0.0 {
        return Ok(0);
    }
    if (p - 1.0).abs() < f64::EPSILON {
        return Ok(n);
    }
    let mut count = 0u64;
    for _ in 0..n {
        if next_bernoulli(rng, p)? {
            count += 1;
        }
    }
    Ok(count)
}

/// Poisson distribution ~ Poisson(lambda) using Knuth's algorithm.
pub fn sample_poisson(rng: &mut Xoshiro256PlusPlus, lambda: f64) -> Result<u64, String> {
    if lambda.is_nan() || lambda <= 0.0 {
        return Err(format!(
            "Poisson lambda must be strictly positive (> 0), got {}",
            lambda
        ));
    }
    let l_bound = (-lambda).exp();
    let mut k = 0u64;
    let mut p = 1.0;
    loop {
        k += 1;
        p *= next_f64(rng);
        if p <= l_bound {
            break;
        }
    }
    Ok(k - 1)
}

/// Geometric distribution ~ Geometric(p). Number of trials until first success.
pub fn sample_geometric(rng: &mut Xoshiro256PlusPlus, p: f64) -> Result<u64, String> {
    if p.is_nan() || p <= 0.0 || p > 1.0 {
        return Err(format!(
            "Geometric probability p must be in (0.0, 1.0], got {}",
            p
        ));
    }
    if (p - 1.0).abs() < f64::EPSILON {
        return Ok(1);
    }
    let mut u = next_f64(rng);
    while u <= 0.0 {
        u = next_f64(rng);
    }
    let trials = (u.ln() / (1.0 - p).ln()).floor() as u64 + 1;
    Ok(trials)
}

/// Gamma distribution ~ Gamma(shape alpha, scale beta) using Marsaglia & Tsang algorithm.
pub fn sample_gamma(rng: &mut Xoshiro256PlusPlus, alpha: f64, beta: f64) -> Result<f64, String> {
    if alpha.is_nan() || alpha <= 0.0 {
        return Err(format!("Gamma shape (alpha) must be > 0, got {}", alpha));
    }
    if beta.is_nan() || beta <= 0.0 {
        return Err(format!("Gamma scale (beta) must be > 0, got {}", beta));
    }
    if alpha < 1.0 {
        let u = next_f64(rng);
        let g = sample_gamma(rng, alpha + 1.0, beta)?;
        return Ok(g * u.powf(1.0 / alpha));
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let z = sample_standard_normal(rng);
        let v = 1.0 + c * z;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = next_f64(rng);
        if u < 1.0 - 0.0331 * z * z * z * z {
            return Ok(d * v3 * beta);
        }
        if u.ln() < 0.5 * z * z + d * (1.0 - v3 + v3.ln()) {
            return Ok(d * v3 * beta);
        }
    }
}

/// LogNormal distribution ~ LogNormal(mean, std_dev).
pub fn sample_log_normal(
    rng: &mut Xoshiro256PlusPlus,
    mean: f64,
    std_dev: f64,
) -> Result<f64, String> {
    let norm = sample_normal(rng, mean, std_dev)?;
    Ok(norm.exp())
}
