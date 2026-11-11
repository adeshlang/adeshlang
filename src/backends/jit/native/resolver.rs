//! Unified Host Symbol Resolution for Native JIT.
//!
//! Maps external runtime function names (builtins, ARC, print helpers, C library functions)
//! to their physical host memory addresses.

use std::collections::HashMap;

/// Universal symbol resolver interface for in-memory JIT execution.
pub trait HostSymbolResolver {
    /// Look up an absolute host memory address by symbol name.
    fn resolve(&self, symbol: &str) -> Option<*const u8>;
}

/// Standard Host Symbol Resolver populated with Adesh runtime bridges,
/// ARC FFI, and C runtime symbols.
pub struct StandardHostSymbolResolver {
    symbols: HashMap<String, *const u8>,
}

unsafe impl Send for StandardHostSymbolResolver {}
unsafe impl Sync for StandardHostSymbolResolver {}

impl Default for StandardHostSymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardHostSymbolResolver {
    /// Create a new host symbol resolver with all standard runtime symbols registered.
    pub fn new() -> Self {
        let mut symbols = HashMap::new();

        // 1. Standard C library functions
        symbols.insert("printf".to_string(), libc::printf as *const u8);
        symbols.insert("malloc".to_string(), libc::malloc as *const u8);
        symbols.insert("free".to_string(), libc::free as *const u8);
        symbols.insert("memcpy".to_string(), libc::memcpy as *const u8);
        symbols.insert("puts".to_string(), libc::puts as *const u8);

        // 2. AOT / Runtime bridge print functions
        symbols.insert(
            "aot_print_i64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_i64 as *const u8,
        );
        symbols.insert(
            "aot_print_u8".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u8 as *const u8,
        );
        symbols.insert(
            "aot_print_u16".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u16 as *const u8,
        );
        symbols.insert(
            "aot_print_u32".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u32 as *const u8,
        );
        symbols.insert(
            "aot_print_u64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u64 as *const u8,
        );
        symbols.insert(
            "aot_print_f32".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_f32 as *const u8,
        );
        symbols.insert(
            "aot_print_f64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_f64 as *const u8,
        );
        symbols.insert(
            "aot_print_bool".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_bool as *const u8,
        );
        symbols.insert(
            "aot_print_str".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_str as *const u8,
        );

        // 3. JIT runtime bridge symbols
        for (name, ptr) in crate::backends::jit::native::runtime_bridge::get_runtime_symbols() {
            symbols.insert(name.to_string(), ptr);
        }

        // 4. ARC memory management runtime functions
        use crate::execution::arc_bridge::*;
        symbols.insert("adesh_rt_arc_new".to_string(), adesh_rt_arc_new as *const u8);
        symbols.insert(
            "adesh_rt_arc_clone".to_string(),
            adesh_rt_arc_clone as *const u8,
        );
        symbols.insert(
            "adesh_rt_arc_drop".to_string(),
            adesh_rt_arc_drop as *const u8,
        );
        symbols.insert(
            "adesh_rt_weak_new".to_string(),
            adesh_rt_weak_new as *const u8,
        );
        symbols.insert(
            "adesh_rt_weak_drop".to_string(),
            adesh_rt_weak_drop as *const u8,
        );
        symbols.insert("adesh_rt_arc_get".to_string(), adesh_rt_arc_get as *const u8);
        symbols.insert("adesh_rt_arc_set".to_string(), adesh_rt_arc_set as *const u8);
        symbols.insert(
            "adesh_rt_arc_strong_count".to_string(),
            adesh_rt_arc_strong_count as *const u8,
        );
        symbols.insert(
            "adesh_rt_arc_weak_count".to_string(),
            adesh_rt_arc_weak_count as *const u8,
        );

        Self { symbols }
    }

    /// Register a custom host symbol address.
    pub fn register(&mut self, name: impl Into<String>, ptr: *const u8) {
        self.symbols.insert(name.into(), ptr);
    }
}

impl HostSymbolResolver for StandardHostSymbolResolver {
    fn resolve(&self, symbol: &str) -> Option<*const u8> {
        self.symbols.get(symbol).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_symbol_resolution() {
        let resolver = StandardHostSymbolResolver::new();
        assert!(resolver.resolve("printf").is_some());
        assert!(resolver.resolve("malloc").is_some());
        assert!(resolver.resolve("aot_print_i64").is_some());
        assert!(resolver.resolve("adesh_rt_arc_new").is_some());
        assert!(resolver.resolve("non_existent_symbol_123").is_none());
    }
}
