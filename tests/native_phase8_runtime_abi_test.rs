//! Phase 8 — Runtime ABI hardening tests.
//!
//! The C-ABI runtime bridges (`aot_*`, `jit_*`) are the boundary between
//! compiled native code and the Rust runtime. Every entry point must
//! either do the right thing or fail loudly — none may panic, crash on
//! bogus inputs, or silently dereference values that are not pointers.
//!
//! Hardened here and regression-tested:
//! - `adesh_value_to_string`: small integers must stringify without
//!   dereferencing (previously any int > 1000 was dereferenced as a C
//!   string pointer), and embedded NUL bytes must not panic.
//! - The JIT validated-heap pointer ABI: alloc/set/get/free round-trips,
//!   loud errors (pending exception) for invalid pointers, out-of-bounds
//!   stores and double frees — no crashes.
//! - `jit_unwrap_i64`/`jit_unwrap_f64`: numeric handles unwrap; strings
//!   raise a loud pending exception instead of returning garbage.

use adeshlang::backends::aot::runtime_bridge::{
    adesh_value_to_string, aot_get_value, aot_store_value, aot_wrap_ptr,
};
use adeshlang::backends::common::builtins::RuntimeValue;
use adeshlang::backends::jit::native::runtime_bridge::{
    jit_alloc, jit_clear_exception, jit_free, jit_has_exception, jit_ptr_get_index,
    jit_ptr_set_index, jit_unwrap_f64, jit_unwrap_i64, jit_wrap_f64, jit_wrap_i64, jit_wrap_str,
};

fn clear_pending_exception() {
    // Exceptions are thread-local; tests run on their own thread.
    while jit_has_exception() != 0 {
        jit_clear_exception();
    }
}

/// Read a malloc'd C string returned by the AOT bridge.
/// The small backing allocation is intentionally leaked per call —
/// `libc` is not a dev-dependency, and a handful of bytes in a test
/// process is harmless.
unsafe fn take_c_string(ptr: *mut std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[test]
fn test_aot_value_to_string_small_int_is_not_dereferenced() {
    // Any integer at or below 0x10000 is a plain number: stringifying it
    // must never dereference it as a pointer (crash) — regression for the
    // old `> 1000` heuristic.
    for v in [0u64, 7, 999, 1000, 1001, 65535, 65536] {
        let ptr = adesh_value_to_string(v);
        let s = unsafe { take_c_string(ptr) }.unwrap_or_default();
        assert_eq!(s, v.to_string(), "integer {} must stringify", v);
    }
}

#[test]
fn test_aot_value_to_string_stored_string_handle() {
    let handle = aot_store_value(RuntimeValue::String("hello abi".into()));
    let ptr = adesh_value_to_string(handle);
    let s = unsafe { take_c_string(ptr) }.unwrap_or_default();
    assert_eq!(s, "hello abi");
}

#[test]
fn test_aot_value_to_string_embedded_nul_does_not_panic() {
    // CString cannot hold interior NULs; the bridge must escape them
    // instead of panicking.
    let handle = aot_store_value(RuntimeValue::String("a\0b".into()));
    let ptr = adesh_value_to_string(handle);
    let s = unsafe { take_c_string(ptr) }.unwrap_or_default();
    assert!(s.contains('a') && s.contains("\\0"), "got: {:?}", s);
}

#[test]
fn test_aot_wrap_ptr_null_and_small_int_never_dereference() {
    // Null wraps to the null value.
    let null_handle = aot_wrap_ptr(0);
    assert!(matches!(
        aot_get_value(null_handle),
        Some(RuntimeValue::Null)
    ));

    // Small ints wrap as integers, not dereferenced.
    let int_handle = aot_wrap_ptr(42);
    match aot_get_value(int_handle) {
        Some(RuntimeValue::Int(n)) => assert_eq!(n, 42),
        other => panic!("expected Int(42), got {:?}", other),
    }
}

#[test]
fn test_jit_pointer_abi_round_trip() {
    clear_pending_exception();

    let ptr = jit_alloc(4);
    assert_ne!(ptr, 0, "jit_alloc must return a valid heap handle");

    for i in 0..4i64 {
        let res = jit_ptr_set_index(ptr, i, (i + 1) as u64);
        assert_eq!(res, ptr, "set_index must return the pointer");
    }
    assert_eq!(jit_has_exception(), 0, "no violation may be pending");

    for i in 0..4i64 {
        let handle = jit_ptr_get_index(ptr, i);
        match adeshlang::backends::jit::native::runtime_bridge::get_runtime_value(handle) {
            Some(RuntimeValue::Int(v)) => assert_eq!(v, i + 1),
            other => panic!("load at {} got {:?}", i, other),
        }
    }

    assert_eq!(jit_free(ptr), 0);
    assert_eq!(jit_has_exception(), 0);
}

#[test]
fn test_jit_pointer_abi_double_free_is_loud() {
    clear_pending_exception();

    let ptr = jit_alloc(8);
    assert_eq!(jit_free(ptr), 0);

    // Second free must fail loudly: nonzero return + pending exception.
    let ret = jit_free(ptr);
    assert_ne!(ret, 0, "double free must return an error");
    assert_ne!(
        jit_has_exception(),
        0,
        "double free must raise a pending exception"
    );
    clear_pending_exception();
}

#[test]
fn test_jit_pointer_abi_invalid_pointer_is_loud_not_crashing() {
    clear_pending_exception();

    // A foreign pointer (never allocated on the validated heap) must be
    // rejected loudly, never dereferenced.
    let handle = jit_ptr_get_index(0xdead_beef, 0);
    match adeshlang::backends::jit::native::runtime_bridge::get_runtime_value(handle) {
        Some(RuntimeValue::Null) | None => {}
        other => panic!("expected null result, got {:?}", other),
    }
    assert_ne!(
        jit_has_exception(),
        0,
        "invalid pointer must raise an error"
    );
    clear_pending_exception();

    let ret = jit_free(0xdead_beef);
    assert_ne!(ret, 0, "free of a foreign pointer must fail");
    clear_pending_exception();
}

#[test]
fn test_jit_pointer_abi_out_of_bounds_store_is_loud() {
    clear_pending_exception();

    let ptr = jit_alloc(2);
    let ret = jit_ptr_set_index(ptr, 99, 1);
    assert_eq!(ret, ptr);
    assert_ne!(
        jit_has_exception(),
        0,
        "out-of-bounds store must raise a pending exception"
    );

    // Byte-range violation: storing 300 into a byte element.
    clear_pending_exception();
    let ret = jit_ptr_set_index(ptr, 0, 300);
    assert_eq!(ret, ptr);
    assert_ne!(
        jit_has_exception(),
        0,
        "value out of byte range must raise a pending exception"
    );

    let _ = jit_free(ptr);
    clear_pending_exception();
}

#[test]
fn test_jit_unwrap_numeric_and_loud_non_numeric() {
    clear_pending_exception();

    // Numeric handles unwrap cleanly.
    let int_handle = jit_wrap_i64(-5);
    assert_eq!(jit_unwrap_i64(int_handle), -5);
    assert_eq!(jit_has_exception(), 0);

    let float_handle = jit_wrap_f64(2.5);
    let f = jit_unwrap_f64(float_handle);
    assert!((f - 2.5).abs() < 1e-12);
    assert_eq!(jit_has_exception(), 0);

    // A string handle must NOT unwrap to garbage bits: loud error instead.
    let cstr = std::ffi::CString::new("not a number").unwrap();
    let str_handle = jit_wrap_str(cstr.as_ptr() as i64);
    assert_eq!(
        jit_unwrap_i64(str_handle),
        0,
        "non-numeric unwrap must return 0"
    );
    assert_ne!(
        jit_has_exception(),
        0,
        "non-numeric unwrap must raise a pending exception"
    );

    clear_pending_exception();
    assert_eq!(jit_unwrap_f64(str_handle), 0.0);
    assert_ne!(
        jit_has_exception(),
        0,
        "float unwrap of string must be loud"
    );

    clear_pending_exception();
}
