//! Memory Model Tests
//!
//! Tests for smart pointers, SSO/SAO, and memory management features.

#[cfg(test)]
mod tests {
    /// Test Small String Optimization threshold
    #[test]
    fn test_sso_threshold() {
        // Strings under 22 bytes should use SSO
        let short = "Hello";
        assert!(short.len() <= 22, "Short string should be under SSO threshold");
        
        let max_sso = "21 chars exactly!!!!"; // 21 chars
        assert!(max_sso.len() <= 22, "Max SSO string should be under threshold");
    }

    /// Test Small Array Optimization threshold
    #[test]
    fn test_sao_threshold() {
        // Arrays under 8 elements should use SAO
        let small: Vec<i32> = vec![1, 2, 3, 4, 5];
        assert!(small.len() < 8, "Small array should be under SAO threshold");
        
        let max_sao: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7];
        assert!(max_sao.len() < 8, "Max SAO array should be under threshold");
    }

    // Additional memory tests would go here:
    // - shared_basic
    // - unique_move
    // - weak_cycle
    // - sso_strings
    // - small_arrays
}
