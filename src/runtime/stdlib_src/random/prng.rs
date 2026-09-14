//! PRNG Core Engines & State Management
//!
//! Implements production-grade, statistically sound PRNG algorithms (Xoshiro256++ and PCG64)
//! with SplitMix64 initialization, state save/restore, cloning, forking, and splitting.

use std::sync::atomic::{AtomicU64, Ordering};

static GLOBAL_COUNTER: AtomicU64 = AtomicU64::new(1000);

/// SplitMix64 generator for expanding a single u64 seed into full state arrays.
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Helper to produce a high-quality entropy seed from system time, atomic counter, and thread id.
pub fn auto_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(123456789);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let thread_id = format!("{:?}", std::thread::current().id());
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in thread_id.as_bytes() {
        hash = (hash ^ (*b as u64)).wrapping_mul(0x100000001b3);
    }
    nanos ^ counter.wrapping_mul(0x9e3779b97f4a7c15) ^ hash
}

/// Xoshiro256++ Pseudo-Random Number Generator.
/// 256-bit state, period 2^256 - 1, passes BigCrush statistical tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xoshiro256PlusPlus {
    s: [u64; 4],
}

impl Xoshiro256PlusPlus {
    /// Create new Xoshiro256++ generator from a 64-bit seed using SplitMix64.
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut sm_state = seed;
        let s0 = splitmix64(&mut sm_state);
        let s1 = splitmix64(&mut sm_state);
        let s2 = splitmix64(&mut sm_state);
        let s3 = splitmix64(&mut sm_state);
        // Ensure non-zero state
        let s = if s0 == 0 && s1 == 0 && s2 == 0 && s3 == 0 {
            [
                0x1234567890abcdef,
                0xfedcba0987654321,
                0x0123456789abcdef,
                0xfedcba9876543210,
            ]
        } else {
            [s0, s1, s2, s3]
        };
        Self { s }
    }

    /// Create new Xoshiro256++ generator from an explicit 4-element u64 state array.
    pub fn from_state(s: [u64; 4]) -> Self {
        if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 {
            Self::seed_from_u64(0)
        } else {
            Self { s }
        }
    }

    /// Generate auto-seeded generator.
    pub fn new_auto() -> Self {
        Self::seed_from_u64(auto_seed())
    }

    /// Generate next 64-bit random integer.
    pub fn next_u64(&mut self) -> u64 {
        let result = (self.s[0].wrapping_add(self.s[3]))
            .rotate_left(23)
            .wrapping_add(self.s[0]);

        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];

        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);

        result
    }

    /// Generate next 32-bit random integer.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Fill slice with random bytes.
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i < dest.len() {
            let val = self.next_u64();
            let bytes = val.to_ne_bytes();
            let chunk_size = (dest.len() - i).min(8);
            dest[i..i + chunk_size].copy_from_slice(&bytes[..chunk_size]);
            i += chunk_size;
        }
    }

    /// Get current state array.
    pub fn get_state(&self) -> [u64; 4] {
        self.s
    }

    /// Restore state array.
    pub fn set_state(&mut self, s: [u64; 4]) {
        if s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 {
            self.s = [1, 0, 0, 0];
        } else {
            self.s = s;
        }
    }

    /// Fork a new generator from current state.
    pub fn fork(&mut self) -> Self {
        let new_seed = self.next_u64();
        Self::seed_from_u64(new_seed)
    }

    /// Split into two independent generators.
    pub fn split(&mut self) -> (Self, Self) {
        let g1 = self.fork();
        let g2 = self.fork();
        (g1, g2)
    }
}
