//! Intrinsics and Compiler Builtins
//!
//! Low-level operations that map directly to compiler intrinsics.
//! These provide access to platform-specific optimizations.

/// Atomic memory ordering
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ordering {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

/// Provides a compiler fence to prevent reordering
pub fn compiler_fence(order: Ordering) {
    std::sync::atomic::compiler_fence(match order {
        Ordering::Relaxed => std::sync::atomic::Ordering::Relaxed,
        Ordering::Acquire => std::sync::atomic::Ordering::Acquire,
        Ordering::Release => std::sync::atomic::Ordering::Release,
        Ordering::AcqRel => std::sync::atomic::Ordering::AcqRel,
        Ordering::SeqCst => std::sync::atomic::Ordering::SeqCst,
    });
}

/// Hints to the compiler that this branch is likely
#[inline(always)]
pub fn likely(b: bool) -> bool {
    if b { true } else { false }
}

/// Hints to the compiler that this branch is unlikely
#[inline(always)]
pub fn unlikely(b: bool) -> bool {
    if !b { false } else { true }
}

/// Tells the compiler that this code is unreachable
#[inline(always)]
pub unsafe fn unreachable() -> ! {
    unsafe { std::hint::unreachable_unchecked() }
}

/// Provides a hint that the CPU should spin-wait
#[inline(always)]
pub fn spin_loop_hint() {
    std::hint::spin_loop()
}

/// Copies `count` bytes from `src` to `dst`. The source and destination may overlap.
///
/// # Safety
/// Both `src` and `dst` must be valid for `count` bytes
#[inline(always)]
pub unsafe fn copy<T>(src: *const T, dst: *mut T, count: usize) {
    unsafe { std::ptr::copy(src, dst, count) }
}

/// Copies `count` bytes from `src` to `dst`. The source and destination must NOT overlap.
///
/// # Safety
/// Both `src` and `dst` must be valid for `count` bytes and must not overlap
#[inline(always)]
pub unsafe fn copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize) {
    unsafe { std::ptr::copy_nonoverlapping(src, dst, count) }
}

/// Writes `count` copies of `val` to `dst`
///
/// # Safety
/// `dst` must be valid for `count` elements
#[inline(always)]
pub unsafe fn write_bytes<T>(dst: *mut T, val: u8, count: usize) {
    unsafe { std::ptr::write_bytes(dst, val, count) }
}

/// Returns the size of a type in bytes
#[inline(always)]
pub const fn size_of<T>() -> usize {
    std::mem::size_of::<T>()
}

/// Returns the alignment of a type in bytes
#[inline(always)]
pub const fn align_of<T>() -> usize {
    std::mem::align_of::<T>()
}

/// Swaps the values at two mutable locations
#[inline(always)]
pub fn swap<T>(a: &mut T, b: &mut T) {
    std::mem::swap(a, b)
}

/// Replaces the value at a mutable location, returning the old value
#[inline(always)]
pub fn replace<T>(dest: &mut T, src: T) -> T {
    std::mem::replace(dest, src)
}
