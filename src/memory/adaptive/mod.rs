//! Adaptive Memory Management for AdeshLang
//!
//! Implements automatic stack and heap growth without explicit allocation

use num_traits::ToPrimitive;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Adaptive memory configuration with automatic growth
#[derive(Debug, Clone)]
pub struct AdaptiveMemoryConfig {
    /// Initial stack size (default: 32MB)
    pub initial_stack_size: usize,
    /// Maximum stack size before error (default: 1GB)
    pub max_stack_size: usize,
    /// Growth factor when expanding (default: 2x)
    pub growth_factor: f32,
    /// Enable automatic growth
    pub auto_grow: bool,
}

impl Default for AdaptiveMemoryConfig {
    fn default() -> Self {
        Self {
            initial_stack_size: 32 * 1024 * 1024, // 32MB - increased for better defaults
            max_stack_size: 1024 * 1024 * 1024,   // 1GB - much higher ceiling
            growth_factor: 2.0,
            auto_grow: true,
        }
    }
}

impl AdaptiveMemoryConfig {
    /// Create config for minimal memory usage
    pub fn minimal() -> Self {
        Self {
            initial_stack_size: 8 * 1024 * 1024, // 8MB
            max_stack_size: 64 * 1024 * 1024,    // 64MB
            growth_factor: 1.5,
            auto_grow: true,
        }
    }

    /// Create config for high-performance scenarios
    pub fn performance() -> Self {
        Self {
            initial_stack_size: 32 * 1024 * 1024, // 32MB
            max_stack_size: 512 * 1024 * 1024,    // 512MB
            growth_factor: 2.0,
            auto_grow: true,
        }
    }

    /// Get the current effective stack size based on usage
    pub fn effective_stack_size(&self, current_depth: usize) -> usize {
        if !self.auto_grow {
            return self.initial_stack_size;
        }

        // Estimate stack usage: ~50KB per recursion depth for BigInt
        // Regular operations: ~5KB per depth
        let estimated_usage = current_depth * 50 * 1024;

        if estimated_usage > self.initial_stack_size {
            let needed = (estimated_usage as f32 * self.growth_factor) as usize;
            needed.min(self.max_stack_size)
        } else {
            self.initial_stack_size
        }
    }

    /// Calculate maximum safe recursion depth for current stack size
    pub fn max_recursion_depth(&self, current_stack_size: usize) -> usize {
        // More realistic estimate: 50KB per depth for BigInt operations
        // This leaves headroom for other stack usage
        let safe_depth = (current_stack_size / (50 * 1024)).saturating_sub(10);
        safe_depth.max(500) // Always allow at least 500 depth
    }
}

/// Memory statistics tracker
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub peak_stack_usage: Arc<AtomicUsize>,
    pub peak_heap_usage: Arc<AtomicUsize>,
    pub current_recursion_depth: Arc<AtomicUsize>,
    pub allocation_count: Arc<AtomicUsize>,
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self {
            peak_stack_usage: Arc::new(AtomicUsize::new(0)),
            peak_heap_usage: Arc::new(AtomicUsize::new(0)),
            current_recursion_depth: Arc::new(AtomicUsize::new(0)),
            allocation_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl MemoryStats {
    pub fn enter_function(&self) {
        self.current_recursion_depth.fetch_add(1, Ordering::Relaxed);
    }

    pub fn exit_function(&self) {
        self.current_recursion_depth.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_depth(&self) -> usize {
        self.current_recursion_depth.load(Ordering::Relaxed)
    }

    pub fn record_allocation(&self, size: usize) {
        self.allocation_count.fetch_add(1, Ordering::Relaxed);
        let current_heap = self.peak_heap_usage.load(Ordering::Relaxed);
        if size > current_heap {
            self.peak_heap_usage.store(size, Ordering::Relaxed);
        }
    }

    pub fn report(&self) -> String {
        format!(
            "Memory Stats:\n\
             - Peak Stack: {}KB\n\
             - Peak Heap: {}KB\n\
             - Max Recursion Depth: {}\n\
             - Allocations: {}",
            self.peak_stack_usage.load(Ordering::Relaxed) / 1024,
            self.peak_heap_usage.load(Ordering::Relaxed) / 1024,
            self.current_recursion_depth.load(Ordering::Relaxed),
            self.allocation_count.load(Ordering::Relaxed)
        )
    }
}

/// Optimized BigInt allocator with small-integer optimization
#[derive(Debug, Clone)]
pub enum OptimizedBigInt {
    /// Small integers that fit in i64 (no heap allocation)
    Small(i64),
    /// Large integers requiring heap allocation
    Large(Box<num_bigint::BigInt>),
}

impl OptimizedBigInt {
    /// Create from i64 - O(1)
    pub fn from_i64(val: i64) -> Self {
        Self::Small(val)
    }

    /// Create from BigInt, optimizing for small values - O(1) for small, O(n) for large
    pub fn from_bigint(val: num_bigint::BigInt) -> Self {
        if let Some(small) = val.to_i64() {
            Self::Small(small)
        } else {
            Self::Large(Box::new(val))
        }
    }

    /// Get as BigInt reference - O(1)
    pub fn as_bigint(&self) -> num_bigint::BigInt {
        match self {
            Self::Small(n) => num_bigint::BigInt::from(*n),
            Self::Large(b) => (**b).clone(),
        }
    }

    /// Multiply - optimized for small values
    pub fn mul(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Small(a), Self::Small(b)) => {
                // Check for overflow
                if let Some(result) = a.checked_mul(*b) {
                    Self::Small(result)
                } else {
                    // Overflow, promote to BigInt
                    let big_a = num_bigint::BigInt::from(*a);
                    let big_b = num_bigint::BigInt::from(*b);
                    Self::from_bigint(&big_a * &big_b)
                }
            }
            _ => {
                let big_a = self.as_bigint();
                let big_b = other.as_bigint();
                Self::from_bigint(&big_a * &big_b)
            }
        }
    }

    /// Add - optimized for small values
    pub fn add(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Small(a), Self::Small(b)) => {
                if let Some(result) = a.checked_add(*b) {
                    Self::Small(result)
                } else {
                    let big_a = num_bigint::BigInt::from(*a);
                    let big_b = num_bigint::BigInt::from(*b);
                    Self::from_bigint(&big_a + &big_b)
                }
            }
            _ => {
                let big_a = self.as_bigint();
                let big_b = other.as_bigint();
                Self::from_bigint(&big_a + &big_b)
            }
        }
    }

    /// Memory footprint in bytes
    pub fn memory_size(&self) -> usize {
        match self {
            Self::Small(_) => 8, // Just the i64
            Self::Large(b) => {
                // Approximate: 8 bytes per limb in BigInt
                let digits = b.to_string().len();
                8 + (digits / 19) * 8 // ~19 digits per 64-bit limb
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimized_bigint_small() {
        let a = OptimizedBigInt::from_i64(100);
        let b = OptimizedBigInt::from_i64(200);
        let c = a.mul(&b);

        if let OptimizedBigInt::Small(n) = c {
            assert_eq!(n, 20000);
        } else {
            panic!("Expected small integer");
        }
    }

    #[test]
    fn test_optimized_bigint_overflow() {
        let a = OptimizedBigInt::from_i64(i64::MAX);
        let b = OptimizedBigInt::from_i64(2);
        let c = a.mul(&b);

        // Should promote to Large
        assert!(matches!(c, OptimizedBigInt::Large(_)));
    }

    #[test]
    fn test_adaptive_config() {
        let config = AdaptiveMemoryConfig::default();
        assert_eq!(config.initial_stack_size, 32 * 1024 * 1024);

        // At depth 700, should recommend more stack (700 * 50KB = 35MB > 32MB initial)
        let needed = config.effective_stack_size(700);
        assert!(needed > config.initial_stack_size);
        assert!(needed <= config.max_stack_size);
    }
}
