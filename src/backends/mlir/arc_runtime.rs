//! ARC Runtime Interface for MLIR
//!
//! Provides extern declarations for ARC (Automatic Reference Counting) runtime functions
//! that are used by VIR ARC operations when lowered to MLIR.

/// Generate MLIR extern declarations for ARC runtime functions
pub fn generate_arc_runtime_decls() -> String {
    let mut decls = String::new();

    decls.push_str("  // ARC Runtime Functions\n");

    // arc_retain: Atomically increment reference count
    decls.push_str("  func.func private @arc_retain(!llvm.ptr)\n");

    // arc_release: Atomically decrement reference count and free if zero
    decls.push_str("  func.func private @arc_release(!llvm.ptr)\n");

    // arc_clone: Clone with reference count increment
    decls.push_str("  func.func private @arc_clone(!llvm.ptr) -> !llvm.ptr\n");

    decls.push('\n');

    decls
}

/// Generate MLIR extern declarations for libc memory functions
pub fn generate_libc_decls() -> String {
    let mut decls = String::new();

    decls.push_str("  // libc Memory Functions\n");

    // free: Deallocate memory
    decls.push_str("  func.func private @free(!llvm.ptr)\n");

    // malloc: Allocate memory
    decls.push_str("  func.func private @malloc(i64) -> !llvm.ptr\n");

    // memcpy: Copy memory
    decls.push_str("  func.func private @memcpy(!llvm.ptr, !llvm.ptr, i64)\n");

    // memmove: Move memory (overlap-safe)
    decls.push_str("  func.func private @memmove(!llvm.ptr, !llvm.ptr, i64)\n");

    // memset: Set memory
    decls.push_str("  func.func private @memset(!llvm.ptr, i32, i64)\n");

    decls.push('\n');

    decls
}

/// Generate all runtime function declarations
pub fn generate_runtime_decls() -> String {
    let mut decls = String::new();
    decls.push_str(&generate_arc_runtime_decls());
    decls.push_str(&generate_libc_decls());
    decls
}
