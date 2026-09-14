use adeshlang::parsing::ast::Value;
use adeshlang::runtime::stdlib_src::random::bounded;
use adeshlang::runtime::stdlib_src::random::collections;
use adeshlang::runtime::stdlib_src::random::distributions;
use adeshlang::runtime::stdlib_src::random::prng::Xoshiro256PlusPlus;
use adeshlang::runtime::stdlib_src::random::strings;

#[test]
fn test_seed_reproducibility() {
    let mut rng1 = Xoshiro256PlusPlus::seed_from_u64(123456789);
    let mut rng2 = Xoshiro256PlusPlus::seed_from_u64(123456789);

    for _ in 0..100 {
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }
}

#[test]
fn test_prng_fork_and_clone() {
    let mut rng1 = Xoshiro256PlusPlus::seed_from_u64(42);
    let mut rng_clone = rng1.clone();
    assert_eq!(rng1.next_u64(), rng_clone.next_u64());

    let mut forked = rng1.fork();
    let val_orig = rng1.next_u64();
    let val_fork = forked.next_u64();
    assert_ne!(val_orig, val_fork);
}

#[test]
fn test_unbiased_integer_range_bounds() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(100);
    for _ in 0..5000 {
        let val = bounded::next_i64_range(&mut rng, 10, 20).unwrap();
        assert!((10..20).contains(&val), "val {} out of range [10, 20)", val);
    }
    for _ in 0..5000 {
        let val = bounded::next_i64_range_inclusive(&mut rng, 10, 20).unwrap();
        assert!(
            (10..=20).contains(&val),
            "val {} out of range [10, 20]",
            val
        );
    }
}

#[test]
fn test_chi_square_uniformity() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(777);
    let k = 10u64; // 10 buckets
    let n = 100_000usize;
    let mut counts = vec![0usize; k as usize];

    for _ in 0..n {
        let bucket = bounded::next_u64_bounded(&mut rng, k) as usize;
        counts[bucket] += 1;
    }

    let expected = n as f64 / k as f64;
    let mut chi_square = 0.0;
    for &observed in &counts {
        let diff = observed as f64 - expected;
        chi_square += (diff * diff) / expected;
    }

    // Critical value for 9 degrees of freedom at p = 0.001 is 27.88
    assert!(
        chi_square < 27.88,
        "Chi-square uniformity test failed: chi2 = {}",
        chi_square
    );
}

#[test]
fn test_normal_distribution_mean_and_stddev() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(2026);
    let mean = 50.0;
    let std_dev = 5.0;
    let n = 50_000;

    let mut sum = 0.0;
    let mut sum_sq = 0.0;

    for _ in 0..n {
        let val = distributions::sample_normal(&mut rng, mean, std_dev).unwrap();
        sum += val;
        sum_sq += val * val;
    }

    let sample_mean = sum / n as f64;
    let sample_var = (sum_sq / n as f64) - (sample_mean * sample_mean);
    let sample_std = sample_var.sqrt();

    assert!(
        (sample_mean - mean).abs() < 0.1,
        "Sample mean error too high: {}",
        sample_mean
    );
    assert!(
        (sample_std - std_dev).abs() < 0.1,
        "Sample std error too high: {}",
        sample_std
    );
}

#[test]
fn test_fisher_yates_shuffle_permutations() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(123);
    let original = vec![Value::I64(1), Value::I64(2), Value::I64(3)];

    let mut counts = std::collections::HashMap::new();
    let n = 60_000;

    for _ in 0..n {
        let mut arr = original.clone();
        collections::shuffle(&mut rng, &mut arr);
        let key: Vec<i64> = arr
            .iter()
            .map(|v| match v {
                Value::I64(i) => *i,
                _ => 0,
            })
            .collect();
        *counts.entry(key).or_insert(0) += 1;
    }

    // 3! = 6 possible permutations
    assert_eq!(counts.len(), 6, "All 6 permutations should occur");
    let expected = n / 6;
    for (perm, &count) in &counts {
        let diff = (count as i64 - expected as i64).abs();
        assert!(
            diff < (expected as i64 / 5),
            "Permutation {:?} count {} deviating from expected {}",
            perm,
            count,
            expected
        );
    }
}

#[test]
fn test_uuid4_formatting() {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(9999);
    let uuid_str = strings::random_uuid4(&mut rng);

    assert_eq!(uuid_str.len(), 36);
    let chars: Vec<char> = uuid_str.chars().collect();
    assert_eq!(chars[8], '-');
    assert_eq!(chars[13], '-');
    assert_eq!(chars[18], '-');
    assert_eq!(chars[23], '-');
    assert_eq!(chars[14], '4'); // Version 4
    assert!(chars[19] == '8' || chars[19] == '9' || chars[19] == 'a' || chars[19] == 'b'); // RFC 4122 variant
}
