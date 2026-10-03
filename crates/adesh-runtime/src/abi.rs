//! Adesh Standardized Native Runtime ABI (ADESH_RUNTIME_ABI_V1)
//!
//! Provides ABI-stable, zero-copy, C-compatible memory representations for:
//! - Strings (`AdeshString`)
//! - Slices & Dynamic Arrays (`AdeshSlice`, `AdeshArray`)
//! - Option & Result tagged unions (`AdeshOption`, `AdeshResult`)
//! - Closures & Function Pointers (`AdeshClosure`)
//! - Trait Objects & Fat Pointers (`AdeshFatPointer`)
//! - Memory Allocation, Panics, and Drop Handlers

use super::ffi_guard;
use std::alloc::{Layout, alloc, dealloc, realloc};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr::{null, null_mut};

pub const ADESH_RUNTIME_ABI_VERSION: u32 = 1;

// ============================================================================
// 1. Standardized String Layout
// ============================================================================

/// In-memory representation of an Adesh String.
/// Compatible with standard 64-bit C-ABI:
/// [ ptr (8B) | len (8B) | cap (8B) | flags (4B) | reserved (4B) ] = 32 Bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshString {
    pub ptr: *const u8,
    pub len: usize,
    pub cap: usize,
    pub flags: u32,
    pub reserved: u32,
}

pub const ADESH_STR_FLAG_STATIC: u32 = 1 << 0;
pub const ADESH_STR_FLAG_MUTABLE: u32 = 1 << 1;
pub const ADESH_STR_FLAG_SSO: u32 = 1 << 2;

impl AdeshString {
    pub const fn empty() -> Self {
        Self {
            ptr: null(),
            len: 0,
            cap: 0,
            flags: ADESH_STR_FLAG_STATIC,
            reserved: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let len = bytes.len();
        if len == 0 {
            return Self::empty();
        }

        unsafe {
            let layout = Layout::array::<u8>(len + 1)
                .unwrap_or(Layout::from_size_align_unchecked(len + 1, 1));
            let ptr = alloc(layout);
            if ptr.is_null() {
                return Self::empty();
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
            *ptr.add(len) = 0; // Null-terminator for seamless C-FFI
            Self {
                ptr,
                len,
                cap: len + 1,
                flags: ADESH_STR_FLAG_MUTABLE,
                reserved: 0,
            }
        }
    }

    pub fn from_static_str(s: &'static str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
            cap: s.len(),
            flags: ADESH_STR_FLAG_STATIC,
            reserved: 0,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if self.ptr.is_null() || self.len == 0 {
            return Some("");
        }
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr, self.len);
            std::str::from_utf8(slice).ok()
        }
    }

    pub fn is_static(&self) -> bool {
        (self.flags & ADESH_STR_FLAG_STATIC) != 0
    }

    pub fn free(&mut self) {
        if !self.is_static() && !self.ptr.is_null() && self.cap > 0 {
            unsafe {
                if let Ok(layout) = Layout::array::<u8>(self.cap) {
                    dealloc(self.ptr as *mut u8, layout);
                }
            }
            self.ptr = null();
            self.len = 0;
            self.cap = 0;
        }
    }
}

// ============================================================================
// 2. Standardized Array & Slice Layout
// ============================================================================

/// Standard Read-Only Slice representation.
/// [ data (8B) | len (8B) ] = 16 Bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshSlice<T> {
    pub data: *const T,
    pub len: usize,
}

impl<T> AdeshSlice<T> {
    pub const fn new(data: *const T, len: usize) -> Self {
        Self { data, len }
    }

    pub fn as_slice(&self) -> &[T] {
        if self.data.is_null() || self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.data, self.len) }
        }
    }
}

/// Standard Resizable Dynamic Array Layout.
/// [ data (8B) | len (8B) | cap (8B) | elem_size (8B) ] = 32 Bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshArray {
    pub data: *mut u8,
    pub len: usize,
    pub cap: usize,
    pub elem_size: usize,
}

impl AdeshArray {
    pub fn new(elem_size: usize, initial_cap: usize) -> Self {
        let cap = if initial_cap == 0 { 4 } else { initial_cap };
        // Audit fix: the old code computed `elem_size * cap` (overflowing
        // multiplication) and used a bare `.unwrap()` on the layout — both
        // panic paths reachable from the `adesh_arr_new` C export. On any
        // invalid size, return a degenerate (empty, null) array instead;
        // `push` reports failure on such an array rather than touching
        // undefined behavior.
        let total = elem_size.saturating_mul(cap);
        let Ok(layout) = Layout::from_size_align(total, 8) else {
            return Self {
                data: null_mut(),
                len: 0,
                cap: 0,
                elem_size,
            };
        };
        // SAFETY: layout was just validated.
        let data = unsafe { alloc(layout) };
        Self {
            data,
            len: 0,
            cap: if data.is_null() { 0 } else { cap },
            elem_size,
        }
    }

    pub fn push(&mut self, elem_bytes: *const u8) -> bool {
        if self.elem_size == 0 || elem_bytes.is_null() {
            return false;
        }
        if self.len >= self.cap {
            // Audit fix: `realloc` with a null pointer is undefined
            // behavior, and the size multiplications below could overflow
            // into wrapped layouts. Bail out with `false` instead.
            if self.data.is_null() {
                return false;
            }
            let new_cap = if self.cap == 0 {
                4
            } else {
                self.cap.saturating_mul(2)
            };
            let Some(old_total) = self.elem_size.checked_mul(self.cap) else {
                return false;
            };
            let Ok(old_layout) = Layout::from_size_align(old_total, 8) else {
                return false;
            };
            let Some(new_size) = self.elem_size.checked_mul(new_cap) else {
                return false;
            };
            // SAFETY: old_layout matches the allocation of self.data.
            let new_data = unsafe { realloc(self.data, old_layout, new_size) };
            if new_data.is_null() {
                return false;
            }
            self.data = new_data;
            self.cap = new_cap;
        }

        unsafe {
            let dst = self.data.add(self.len * self.elem_size);
            std::ptr::copy_nonoverlapping(elem_bytes, dst, self.elem_size);
            self.len += 1;
        }
        true
    }

    pub fn get(&self, index: usize) -> Option<*mut u8> {
        if index >= self.len || self.data.is_null() {
            None
        } else {
            unsafe { Some(self.data.add(index * self.elem_size)) }
        }
    }

    pub fn free(&mut self) {
        if !self.data.is_null() && self.cap > 0 && self.elem_size > 0 {
            if let Ok(layout) = Layout::from_size_align(self.elem_size * self.cap, 8) {
                unsafe { dealloc(self.data, layout) };
            }
            self.data = null_mut();
            self.len = 0;
            self.cap = 0;
        }
    }
}

// ============================================================================
// 3. Option & Result Tagged Union Layouts
// ============================================================================

pub const ADESH_TAG_NONE: u8 = 0;
pub const ADESH_TAG_SOME: u8 = 1;

pub const ADESH_TAG_OK: u8 = 0;
pub const ADESH_TAG_ERR: u8 = 1;

/// 64-bit Payload Tagged Option.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshOptionU64 {
    pub tag: u8,
    pub _pad: [u8; 7],
    pub value: u64,
}

impl AdeshOptionU64 {
    pub const fn none() -> Self {
        Self {
            tag: ADESH_TAG_NONE,
            _pad: [0; 7],
            value: 0,
        }
    }

    pub const fn some(val: u64) -> Self {
        Self {
            tag: ADESH_TAG_SOME,
            _pad: [0; 7],
            value: val,
        }
    }

    pub fn is_some(&self) -> bool {
        self.tag == ADESH_TAG_SOME
    }
}

/// 64-bit Payload Tagged Result.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshResultU64 {
    pub tag: u8,
    pub _pad: [u8; 7],
    pub payload: u64,
}

impl AdeshResultU64 {
    pub const fn ok(val: u64) -> Self {
        Self {
            tag: ADESH_TAG_OK,
            _pad: [0; 7],
            payload: val,
        }
    }

    pub const fn err(err_code: u64) -> Self {
        Self {
            tag: ADESH_TAG_ERR,
            _pad: [0; 7],
            payload: err_code,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.tag == ADESH_TAG_OK
    }
}

// ============================================================================
// 4. Closures & Fat Pointers
// ============================================================================

pub type AdeshDropFn = Option<unsafe extern "C" fn(*mut ())>;

/// Standard Closure Representation: function pointer + environment context pointer.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AdeshClosure {
    pub fn_ptr: *const (),
    pub env_ptr: *mut (),
    pub drop_fn: AdeshDropFn,
}

impl PartialEq for AdeshClosure {
    fn eq(&self, other: &Self) -> bool {
        self.fn_ptr == other.fn_ptr
            && self.env_ptr == other.env_ptr
            && self.drop_fn.map(|f| f as usize) == other.drop_fn.map(|f| f as usize)
    }
}

impl Eq for AdeshClosure {}

impl AdeshClosure {
    pub const fn new(fn_ptr: *const (), env_ptr: *mut (), drop_fn: AdeshDropFn) -> Self {
        Self {
            fn_ptr,
            env_ptr,
            drop_fn,
        }
    }

    pub fn call_nullary<R>(&self) -> Option<R> {
        if self.fn_ptr.is_null() {
            None
        } else {
            unsafe {
                let func: extern "C" fn(*mut ()) -> R = std::mem::transmute(self.fn_ptr);
                Some(func(self.env_ptr))
            }
        }
    }

    pub fn drop_env(&mut self) {
        if let Some(drop) = self.drop_fn {
            if !self.env_ptr.is_null() {
                unsafe { drop(self.env_ptr) };
                self.env_ptr = null_mut();
            }
        }
    }
}

/// Standard Fat Pointer / Interface VTable Object.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshFatPointer {
    pub data_ptr: *mut (),
    pub vtable_ptr: *const (),
}

// ============================================================================
// 5. C-ABI Standard Export Functions (Linkable Runtime Symbols)
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn adesh_abi_version() -> u32 {
    ADESH_RUNTIME_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_str_new(utf8_bytes: *const c_char, len: usize) -> AdeshString {
    if utf8_bytes.is_null() || len == 0 {
        return AdeshString::empty();
    }
    unsafe {
        let slice = std::slice::from_raw_parts(utf8_bytes as *const u8, len);
        if let Ok(s) = std::str::from_utf8(slice) {
            AdeshString::from_str(s)
        } else {
            AdeshString::empty()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_str_from_cstr(c_str_ptr: *const c_char) -> AdeshString {
    if c_str_ptr.is_null() {
        return AdeshString::empty();
    }
    unsafe {
        if let Ok(s) = CStr::from_ptr(c_str_ptr).to_str() {
            AdeshString::from_str(s)
        } else {
            AdeshString::empty()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_str_concat(s1: AdeshString, s2: AdeshString) -> AdeshString {
    let str1 = s1.as_str().unwrap_or("");
    let str2 = s2.as_str().unwrap_or("");
    let combined = format!("{}{}", str1, str2);
    AdeshString::from_str(&combined)
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_str_free(mut s: AdeshString) {
    s.free();
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_arr_new(elem_size: usize, initial_cap: usize) -> AdeshArray {
    AdeshArray::new(elem_size, initial_cap)
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_arr_push(arr: *mut AdeshArray, elem_ptr: *const u8) -> i32 {
    if arr.is_null() || elem_ptr.is_null() {
        return -1;
    }
    unsafe { if (*arr).push(elem_ptr) { 0 } else { -1 } }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_arr_get(arr: *const AdeshArray, index: usize) -> *mut u8 {
    if arr.is_null() {
        return null_mut();
    }
    unsafe { (*arr).get(index).unwrap_or(null_mut()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_arr_free(arr: *mut AdeshArray) {
    if !arr.is_null() {
        unsafe { (*arr).free() };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_mem_alloc(size: usize, align: usize) -> *mut u8 {
    let effective_align = if align == 0 { 8 } else { align };
    if let Ok(layout) = Layout::from_size_align(size, effective_align) {
        unsafe { alloc(layout) }
    } else {
        null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_mem_free(ptr: *mut u8, size: usize, align: usize) {
    if !ptr.is_null() {
        let effective_align = if align == 0 { 8 } else { align };
        if let Ok(layout) = Layout::from_size_align(size, effective_align) {
            unsafe { dealloc(ptr, layout) };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_mem_realloc(
    ptr: *mut u8,
    old_size: usize,
    new_size: usize,
    align: usize,
) -> *mut u8 {
    let effective_align = if align == 0 { 8 } else { align };
    if ptr.is_null() {
        return adesh_mem_alloc(new_size, effective_align);
    }
    if let Ok(old_layout) = Layout::from_size_align(old_size, effective_align) {
        unsafe { realloc(ptr, old_layout, new_size) }
    } else {
        null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_panic_abort(msg: *const c_char, len: usize) -> ! {
    let msg_str = if msg.is_null() || len == 0 {
        "Adesh runtime panic"
    } else {
        unsafe {
            let slice = std::slice::from_raw_parts(msg as *const u8, len);
            std::str::from_utf8(slice).unwrap_or("Invalid UTF-8 in panic message")
        }
    };
    eprintln!("\n🔥 [Adesh Fatal Error]: {}", msg_str);
    std::process::abort();
}

// ============================================================================
// 6. Direct I/O and Printing ABI (Zero-dependency OS Output)
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn adesh_print_str(s: *const c_char) {
    ffi_guard((), || {
        if s.is_null() {
            return;
        }
        unsafe {
            let cstr = CStr::from_ptr(s);
            if let Ok(rust_str) = cstr.to_str() {
                print!("{}", rust_str);
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_print_i64(val: i64) {
    ffi_guard((), || print!("{}", val));
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_print_f64(val: f64) {
    ffi_guard((), || print!("{}", val));
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_print_bool(val: bool) {
    ffi_guard((), || print!("{}", if val { "true" } else { "false" }));
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_print_newline() {
    ffi_guard((), || println!());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adesh_string_abi_lifecycle() {
        let mut s = AdeshString::from_str("Hello, Adesh ABI!");
        assert_eq!(s.as_str(), Some("Hello, Adesh ABI!"));
        assert_eq!(s.len, 17);
        assert!(!s.is_static());

        let static_s = AdeshString::from_static_str("Static data");
        assert!(static_s.is_static());

        let concat = adesh_str_concat(s, static_s);
        assert_eq!(concat.as_str(), Some("Hello, Adesh ABI!Static data"));

        s.free();
    }

    #[test]
    fn test_adesh_array_abi_lifecycle() {
        let mut arr = AdeshArray::new(std::mem::size_of::<i64>(), 2);
        assert_eq!(arr.len, 0);

        let val1: i64 = 100;
        let val2: i64 = 200;
        let val3: i64 = 300;

        assert!(arr.push(&val1 as *const i64 as *const u8));
        assert!(arr.push(&val2 as *const i64 as *const u8));
        assert!(arr.push(&val3 as *const i64 as *const u8)); // Triggers dynamic resize

        assert_eq!(arr.len, 3);
        assert!(arr.cap >= 3);

        unsafe {
            let p0 = arr.get(0).unwrap() as *const i64;
            let p1 = arr.get(1).unwrap() as *const i64;
            let p2 = arr.get(2).unwrap() as *const i64;

            assert_eq!(*p0, 100);
            assert_eq!(*p1, 200);
            assert_eq!(*p2, 300);
        }

        arr.free();
    }

    #[test]
    fn test_tagged_unions_abi() {
        let opt_some = AdeshOptionU64::some(42);
        assert!(opt_some.is_some());
        assert_eq!(opt_some.value, 42);

        let opt_none = AdeshOptionU64::none();
        assert!(!opt_none.is_some());

        let res_ok = AdeshResultU64::ok(1024);
        assert!(res_ok.is_ok());
        assert_eq!(res_ok.payload, 1024);

        let res_err = AdeshResultU64::err(404);
        assert!(!res_err.is_ok());
        assert_eq!(res_err.payload, 404);
    }
}
