//! Phase 10 Cross-Platform Classification E2E Test Suite.
//!
//! Validates:
//! - Target specifications across Windows x64, Linux x64, macOS ARM64, and RISC-V 64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::target_spec::TargetSpec;
use adesh_object::PointerWidth;

#[test]
fn test_cross_platform_target_specs() {
    let win = TargetSpec::windows_x64();
    assert_eq!(win.descriptor.pointer_width, PointerWidth::U64);
    assert_eq!(win.is_windows(), true);

    let linux = TargetSpec::linux_x64();
    assert_eq!(linux.descriptor.pointer_width, PointerWidth::U64);
    assert_eq!(linux.is_windows(), false);

    let macos = TargetSpec::macos_arm64();
    assert_eq!(macos.descriptor.pointer_width, PointerWidth::U64);

    let riscv = TargetSpec::riscv64();
    assert_eq!(riscv.descriptor.pointer_width, PointerWidth::U64);
}
