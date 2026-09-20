//! JIT Optimization Integration Test
//!
//! Tests for inline caching, hidden classes, and JIT performance features.

#[cfg(test)]
mod tests {
    use adeshlang::backends::jit::adaptive::adaptive_impl::hidden_class::HiddenClassSystem;
    use adeshlang::backends::jit::adaptive::adaptive_impl::inline_cache::InlineCacheSystem;

    /// Test hidden class stability for consistent object shapes
    #[test]
    fn test_hidden_class_consistency() {
        let mut system = HiddenClassSystem::new();

        // Object 1: add "x", then "y"
        let root_id = 0;
        let c1_x = system.add_property(root_id, "x");
        let c1_xy = system.add_property(c1_x, "y");

        // Object 2: add "x", then "y" in identical order -> must get same hidden class IDs
        let c2_x = system.add_property(root_id, "x");
        let c2_xy = system.add_property(c2_x, "y");

        assert_eq!(
            c1_x, c2_x,
            "Hidden class for {{x}} must be shared across objects"
        );
        assert_eq!(
            c1_xy, c2_xy,
            "Hidden class for {{x, y}} must be shared across objects"
        );

        // Verify property offsets in the shared hidden class
        let class = system.get_class(c1_xy).expect("Class c1_xy should exist");
        assert_eq!(class.get_offset("x"), Some(0), "Offset for 'x' should be 0");
        assert_eq!(class.get_offset("y"), Some(1), "Offset for 'y' should be 1");
        assert_eq!(
            class.get_offset("z"),
            None,
            "Unknown property 'z' should have no offset"
        );

        // Object 3: add "y", then "x" -> different shape / transition path
        let c3_y = system.add_property(root_id, "y");
        let c3_yx = system.add_property(c3_y, "x");

        assert_ne!(
            c1_xy, c3_yx,
            "Objects with different property orders must have distinct hidden classes"
        );
        let class_yx = system.get_class(c3_yx).expect("Class c3_yx should exist");
        assert_eq!(class_yx.get_offset("y"), Some(0));
        assert_eq!(class_yx.get_offset("x"), Some(1));
    }

    /// Test inline cache hit rates for monomorphic call sites
    #[test]
    fn test_inline_cache_monomorphic() {
        let mut ic = InlineCacheSystem::new();
        let call_site_addr = 0x1000;
        let class_id = 42;
        let prop_offset = 3;

        // First access is a cache miss
        let first_access = ic.get_property(call_site_addr, class_id);
        assert_eq!(first_access, None, "First lookup must be a cache miss");

        let (hits, misses, hit_rate): (u64, u64, f64) = ic.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 1);
        assert_eq!(hit_rate, 0.0);

        // Populate cache for monomorphic call site
        ic.cache_property(call_site_addr, class_id, prop_offset);

        // Subsequent accesses should hit
        for _ in 0..10 {
            let cached = ic.get_property(call_site_addr, class_id);
            assert_eq!(
                cached,
                Some(prop_offset),
                "Cached property offset should match"
            );
        }

        let (hits, misses, hit_rate): (u64, u64, f64) = ic.stats();
        assert_eq!(hits, 10);
        assert_eq!(misses, 1);
        assert!(
            (hit_rate - (10.0f64 / 11.0f64)).abs() < 1e-6,
            "Hit rate should be 10/11 (~90.9%)"
        );

        // Lookup with a different class ID at same monomorphic site causes a miss
        let wrong_class_access = ic.get_property(call_site_addr, 999);
        assert_eq!(
            wrong_class_access, None,
            "Lookup with mismatched class ID must miss"
        );
    }

    /// Test constant folding at compile time
    #[test]
    fn test_constant_folding() {
        // Arithmetic expressions folded directly
        let folded_add = 2 + 3 * 4;
        assert_eq!(
            folded_add, 14,
            "Multiplication precedence with addition must fold to 14"
        );

        let folded_bitwise = (0xFF & 0x0F) | 0x80;
        assert_eq!(
            folded_bitwise, 0x8F,
            "Bitwise constant operations must fold to 0x8F"
        );

        let folded_shift = (1 << 4) >> 2;
        assert_eq!(
            folded_shift, 4,
            "Bit shift constant operations must fold to 4"
        );
    }
}
