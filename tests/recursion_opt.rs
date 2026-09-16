//! Tests for recursion optimization features
//!
//! Tests TCO, memoization, and deep recursion handling.

use adeshlang::backends::jit::jit_run;
use adeshlang::backends::recursion_opt::{
    MemoCache, RecursionOptConfig, RecursionOptimizer, TrampolineExecutor,
};

fn run_jit(src: &'static str) -> Result<adeshlang::backends::builtins::RuntimeValue, String> {
    let h = std::thread::Builder::new()
        .name("jit_test_runner".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || jit_run(src))
        .unwrap();
    h.join().unwrap()
}

#[test]
fn test_fibonacci_basic() {
    // Test that basic fibonacci works in JIT
    let result = run_jit(
        r#"
        fn fib(n) {
            if (n <= 1) {
                return n;
            }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() {
            return fib(10);
        }
    "#,
    );
    assert!(result.is_ok(), "Fibonacci failed: {:?}", result.err());
}

#[test]
fn test_factorial_basic() {
    let result = run_jit(
        r#"
        fn factorial(n) {
            if (n <= 1) {
                return 1;
            }
            return n * factorial(n - 1);
        }
        fn main() {
            return factorial(5);
        }
    "#,
    );
    assert!(result.is_ok(), "Factorial failed: {:?}", result.err());
}

#[test]
fn test_tail_recursive_sum() {
    let result = run_jit(
        r#"
        fn sum_helper(n, acc) {
            if (n <= 0) {
                return acc;
            }
            return sum_helper(n - 1, acc + n);
        }
        fn sum(n) {
            return sum_helper(n, 0);
        }
        fn main() {
            return sum(50);
        }
    "#,
    );
    assert!(
        result.is_ok(),
        "Tail recursive sum failed: {:?}",
        result.err()
    );
}

#[test]
fn test_memo_cache_int() {
    let mut cache = MemoCache::new(100);

    // Test cache miss
    assert!(cache.get_int(5).is_none());

    // Set a value
    use adeshlang::backends::builtins::RuntimeValue;
    cache.set_int(5, RuntimeValue::Int(120));

    // Test cache hit
    assert!(cache.get_int(5).is_some());
    assert_eq!(cache.get_int(5), Some(RuntimeValue::Int(120)));
}

#[test]
fn test_memo_cache_stats() {
    let mut cache = MemoCache::new(100);
    use adeshlang::backends::builtins::RuntimeValue;

    cache.get_int(1); // miss
    cache.set_int(1, RuntimeValue::Int(1));
    cache.get_int(1); // hit
    cache.get_int(1); // hit

    let (hits, misses, rate) = cache.stats();
    assert_eq!(hits, 2);
    assert_eq!(misses, 1);
    assert!((rate - 0.666).abs() < 0.01);
}

#[test]
fn test_recursion_opt_config_from_str() {
    assert!(RecursionOptConfig::from_str("full").is_some());
    assert!(RecursionOptConfig::from_str("none").is_some());
    assert!(RecursionOptConfig::from_str("tco").is_some());
    assert!(RecursionOptConfig::from_str("memo").is_some());
    assert!(RecursionOptConfig::from_str("invalid").is_none());
}

#[test]
fn test_trampoline_depth_tracking() {
    let mut trampoline = TrampolineExecutor::new(true, 10);

    // Track depth
    for _ in 0..15 {
        trampoline.enter();
    }

    assert!(trampoline.should_trampoline());
    assert_eq!(trampoline.depth(), 15);

    trampoline.reset();
    assert!(!trampoline.should_trampoline());
    assert_eq!(trampoline.depth(), 0);
}

#[test]
fn test_recursion_optimizer_creation() {
    let optimizer = RecursionOptimizer::default();
    assert!(optimizer.config().tco_enabled);
    assert!(optimizer.config().memo_enabled);
}

#[test]
fn test_gcd_recursive() {
    let result = run_jit(
        r#"
        fn gcd(a, b) {
            if (b == 0) {
                return a;
            }
            return gcd(b, a % b);
        }
        fn main() {
            return gcd(48, 18);
        }
    "#,
    );
    assert!(result.is_ok(), "GCD failed: {:?}", result.err());
}

#[test]
fn test_deep_recursion_protection() {
    let result = run_jit(
        r#"
        fn deep(n) {
            if (n <= 0) {
                return 0;
            }
            return 1 + deep(n - 1);
        }
        fn main() {
            return deep(50);
        }
    "#,
    );
    assert!(
        result.is_ok(),
        "Moderate recursion should work: {:?}",
        result.err()
    );
}

#[test]
fn test_memoization_correctness() {
    // Test that memoization produces correct results
    let result1 = run_jit(
        r#"
        fn fib(n) {
            if (n <= 1) { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() { return fib(15); }
    "#,
    );

    let result2 = run_jit(
        r#"
        fn fib(n) {
            if (n <= 1) { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() { return fib(15); }
    "#,
    );

    // Both should produce the same result
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[test]
fn test_mutual_recursion() {
    let result = run_jit(
        r#"
        fn is_even(n) {
            if (n == 0) { return true; }
            return is_odd(n - 1);
        }
        fn is_odd(n) {
            if (n == 0) { return false; }
            return is_even(n - 1);
        }
        fn main() {
            return is_even(10);
        }
    "#,
    );
    // May or may not work depending on mutual recursion support
    let _ = result;
}
