//! Constant-time utility functions for AdeshLang Crypto.

use subtle::ConstantTimeEq;

pub fn constant_time_equals(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

pub fn constant_time_compare(a: &[u8], b: &[u8]) -> i32 {
    if constant_time_equals(a, b) { 0 } else { 1 }
}
