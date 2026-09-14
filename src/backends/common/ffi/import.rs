/// FFI (Foreign Function Interface) Import System
///
/// Handles loading and resolving foreign functions from:
/// - C/C++ libraries (via dlopen/GetProcAddress)
/// - Rust FFI imports (same as C ABI)
/// - Go shared libraries (via cgo)
/// - WASM modules (via wasmtime/wasmi)
///
/// All foreign functions are stored in a global registry (Arc<Mutex>) and made available
/// to all execution backends (Interpreter, Bytecode VM, JIT, AOT, WASM).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use libffi::middle::{Arg, Cif, CodePtr, Type};
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use num_traits::ToPrimitive;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use std::ffi::CStr;
use std::ffi::CString;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::parsing::ast::{ExternFunctionDecl, Value};

/// Supported ABI types for extern declarations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiType {
    /// C ABI (SysV on Unix, Windows x64 calling convention)
    C,
    /// Rust FFI (compatible with C, same calling convention)
    Rust,
    /// Go via cgo
    Go,
    /// System V AMD64 ABI (Linux/BSD/macOS x86_64)
    SysV,
    /// Windows x64 calling convention (fastcall)
    Windows,
    /// WASM import (limited to i32, i64, f32, f64)
    Wasm,
    /// Adesh internal ABI (future extension)
    Adesh,
}

impl AbiType {
    /// Parse ABI string from extern declaration
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "c" => Some(AbiType::C),
            "rust" => Some(AbiType::Rust),
            "go" => Some(AbiType::Go),
            "sysv" => Some(AbiType::SysV),
            "windows" => Some(AbiType::Windows),
            "wasm" => Some(AbiType::Wasm),
            "adesh" => Some(AbiType::Adesh),
            _ => None,
        }
    }

    /// Is this ABI valid for the current platform?
    pub fn is_valid_for_platform(&self) -> bool {
        match self {
            AbiType::C | AbiType::Rust => true, // Always valid
            AbiType::SysV => cfg!(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "freebsd"
            )),
            AbiType::Windows => cfg!(target_os = "windows"),
            AbiType::Go => true,   // cgo works on most platforms
            AbiType::Wasm => true, // WASM is platform-independent
            AbiType::Adesh => true,
        }
    }
}

/// Adesh type representation for FFI type mapping
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AdeshType {
    Void,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Bool,
    Ptr(Box<AdeshType>),
    VoidPtr,
    String, // ptr<u8> semantically
}

impl AdeshType {
    /// Parse type from string annotation
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "void" => Some(AdeshType::Void),
            "i8" => Some(AdeshType::I8),
            "i16" => Some(AdeshType::I16),
            "i32" => Some(AdeshType::I32),
            "i64" => Some(AdeshType::I64),
            "i128" => Some(AdeshType::I128),
            "u8" => Some(AdeshType::U8),
            "u16" => Some(AdeshType::U16),
            "u32" => Some(AdeshType::U32),
            "u64" | "usize" => Some(AdeshType::U64), // usize maps to u64 on 64-bit systems
            "u128" => Some(AdeshType::U128),
            "f32" => Some(AdeshType::F32),
            "f64" => Some(AdeshType::F64),
            "bool" => Some(AdeshType::Bool),
            "void*" | "ptr<void>" => Some(AdeshType::VoidPtr),
            "string" | "ptr<u8>" | "char*" => Some(AdeshType::String),
            s if s.starts_with("ptr<") && s.ends_with(">") => {
                let inner = &s[4..s.len() - 1];
                AdeshType::from_str(inner).map(|t| AdeshType::Ptr(Box::new(t)))
            }
            _ => None,
        }
    }

    /// Get size in bytes for this type
    pub fn size_bytes(&self) -> usize {
        match self {
            AdeshType::Void => 0,
            AdeshType::I8 | AdeshType::U8 | AdeshType::Bool => 1,
            AdeshType::I16 | AdeshType::U16 => 2,
            AdeshType::I32 | AdeshType::U32 | AdeshType::F32 => 4,
            AdeshType::I64 | AdeshType::U64 | AdeshType::F64 => 8,
            AdeshType::I128 | AdeshType::U128 => 16,
            AdeshType::Ptr(_) | AdeshType::VoidPtr | AdeshType::String => 8, // 64-bit pointers
        }
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
/// Concrete storage for libffi arguments
#[derive(Debug)]
enum ArgValue {
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(i32),
    Ptr(*mut c_void),
    CString { _storage: CString, ptr: *mut c_void },
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
impl ArgValue {
    fn as_arg(&self) -> Arg {
        match self {
            ArgValue::I8(v) => Arg::new(v),
            ArgValue::U8(v) => Arg::new(v),
            ArgValue::I16(v) => Arg::new(v),
            ArgValue::U16(v) => Arg::new(v),
            ArgValue::I32(v) => Arg::new(v),
            ArgValue::U32(v) => Arg::new(v),
            ArgValue::I64(v) => Arg::new(v),
            ArgValue::U64(v) => Arg::new(v),
            ArgValue::F32(v) => Arg::new(v),
            ArgValue::F64(v) => Arg::new(v),
            ArgValue::Bool(v) => Arg::new(v),
            ArgValue::Ptr(p) => Arg::new(p),
            ArgValue::CString { ptr, .. } => Arg::new(ptr),
        }
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn to_ffi_type(vt: &AdeshType) -> Result<Type, String> {
    match vt {
        AdeshType::Void => Ok(Type::void()),
        AdeshType::Bool => Ok(Type::i32()),
        AdeshType::I8 => Ok(Type::i8()),
        AdeshType::I16 => Ok(Type::i16()),
        AdeshType::I32 => Ok(Type::i32()),
        AdeshType::I64 => Ok(Type::i64()),
        AdeshType::U8 => Ok(Type::u8()),
        AdeshType::U16 => Ok(Type::u16()),
        AdeshType::U32 => Ok(Type::u32()),
        AdeshType::U64 => Ok(Type::u64()),
        AdeshType::F32 => Ok(Type::f32()),
        AdeshType::F64 => Ok(Type::f64()),
        AdeshType::Ptr(_) | AdeshType::VoidPtr | AdeshType::String => Ok(Type::pointer()),
        AdeshType::I128 | AdeshType::U128 => {
            Err("128-bit integers are not supported in FFI calls".to_string())
        }
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn value_as_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::F64(n) => Ok(*n),
        Value::F32(n) => Ok(*n as f64),
        Value::I128(n) => Ok(*n as f64),
        Value::I64(n) => Ok(*n as f64),
        Value::I32(n) => Ok(*n as f64),
        Value::I16(n) => Ok(*n as f64),
        Value::I8(n) => Ok(*n as f64),
        Value::U128(n) => Ok(*n as f64),
        Value::U64(n) => Ok(*n as f64),
        Value::U32(n) => Ok(*n as f64),
        Value::U16(n) => Ok(*n as f64),
        Value::U8(n) => Ok(*n as f64),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::BigInt(bi) => bi
            .to_f64()
            .ok_or_else(|| "BigInt value out of f64 range".to_string()),
        _ => Err("Expected numeric value".to_string()),
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn value_as_i128(v: &Value, ty: &str, min: i128, max: i128) -> Result<i128, String> {
    let n = match v {
        Value::Number(n) => {
            if (n.fract()).abs() > 1e-9 {
                return Err(format!("{} requires an integer", ty));
            }
            *n as i128
        }
        Value::I128(n) => *n,
        Value::I64(n) => *n as i128,
        Value::I32(n) => *n as i128,
        Value::I16(n) => *n as i128,
        Value::I8(n) => *n as i128,
        Value::U128(n) => *n as i128,
        Value::U64(n) => *n as i128,
        Value::U32(n) => *n as i128,
        Value::U16(n) => *n as i128,
        Value::U8(n) => *n as i128,
        Value::BigInt(bi) => bi.to_i128().ok_or_else(|| format!("{} out of range", ty))?,
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        _ => return Err(format!("{} expects numeric value", ty)),
    };
    if n < min || n > max {
        return Err(format!("{} out of range ({}..={})", n, min, max));
    }
    Ok(n)
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn value_as_u128(v: &Value, ty: &str, max: u128) -> Result<u128, String> {
    let n = match v {
        Value::Number(n) => {
            if (n.fract()).abs() > 1e-9 {
                return Err(format!("{} requires an integer", ty));
            }
            if *n < 0.0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::U128(n) => *n,
        Value::U64(n) => *n as u128,
        Value::U32(n) => *n as u128,
        Value::U16(n) => *n as u128,
        Value::U8(n) => *n as u128,
        Value::BigInt(bi) => bi
            .to_u128()
            .ok_or_else(|| format!("{} cannot be negative or out of range", ty))?,
        Value::I128(n) => {
            if *n < 0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::I64(n) => {
            if *n < 0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::I32(n) => {
            if *n < 0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::I16(n) => {
            if *n < 0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::I8(n) => {
            if *n < 0 {
                return Err(format!("{} cannot be negative", ty));
            }
            *n as u128
        }
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        _ => return Err(format!("{} expects numeric value", ty)),
    };
    if n > max {
        return Err(format!("{} out of range (0..={})", n, max));
    }
    Ok(n)
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn value_as_ptr(v: &Value) -> Result<*mut c_void, String> {
    match v {
        Value::Null => Ok(std::ptr::null_mut()),
        Value::Number(n) => Ok(*n as usize as *mut c_void),
        Value::I64(n) => Ok(*n as usize as *mut c_void),
        Value::U64(n) => {
            // Validate pointer handle is alive before passing across FFI
            crate::backends::unsafe_heap::size_of_checked(*n)
                .map_err(|e| format!("FFI pointer validation failed: {}", e))?;
            Ok(*n as usize as *mut c_void)
        }
        Value::I32(n) => Ok(*n as usize as *mut c_void),
        Value::U32(n) => Ok(*n as usize as *mut c_void),
        Value::I16(n) => Ok(*n as usize as *mut c_void),
        Value::U16(n) => Ok(*n as usize as *mut c_void),
        Value::I8(n) => Ok(*n as usize as *mut c_void),
        Value::U8(n) => Ok(*n as usize as *mut c_void),
        Value::BigInt(bi) => bi
            .to_usize()
            .map(|n| n as *mut c_void)
            .ok_or_else(|| "Pointer value out of range".to_string()),
        _ => Err("Expected pointer-compatible value (number or null)".to_string()),
    }
}

fn default_system_libs() -> Vec<String> {
    if cfg!(target_os = "windows") {
        vec!["msvcrt.dll".to_string(), "ucrtbase.dll".to_string()]
    } else if cfg!(any(target_os = "linux", target_os = "android")) {
        vec!["libc.so.6".to_string(), "libm.so.6".to_string()]
    } else if cfg!(target_os = "macos") {
        vec!["libSystem.B.dylib".to_string()]
    } else {
        Vec::new()
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn make_arg_value(v: &Value, ty: &AdeshType) -> Result<ArgValue, String> {
    Ok(match ty {
        AdeshType::Bool => ArgValue::Bool(if v.truthy() { 1 } else { 0 }),
        AdeshType::I8 => {
            ArgValue::I8(value_as_i128(v, "i8", i8::MIN as i128, i8::MAX as i128)? as i8)
        }
        AdeshType::I16 => {
            ArgValue::I16(value_as_i128(v, "i16", i16::MIN as i128, i16::MAX as i128)? as i16)
        }
        AdeshType::I32 => {
            ArgValue::I32(value_as_i128(v, "i32", i32::MIN as i128, i32::MAX as i128)? as i32)
        }
        AdeshType::I64 => {
            ArgValue::I64(value_as_i128(v, "i64", i64::MIN as i128, i64::MAX as i128)? as i64)
        }
        AdeshType::U8 => ArgValue::U8(value_as_u128(v, "u8", u8::MAX as u128)? as u8),
        AdeshType::U16 => ArgValue::U16(value_as_u128(v, "u16", u16::MAX as u128)? as u16),
        AdeshType::U32 => ArgValue::U32(value_as_u128(v, "u32", u32::MAX as u128)? as u32),
        AdeshType::U64 => ArgValue::U64(value_as_u128(v, "u64", u64::MAX as u128)? as u64),
        AdeshType::F32 => ArgValue::F32(value_as_f64(v)? as f32),
        AdeshType::F64 => ArgValue::F64(value_as_f64(v)?),
        AdeshType::Ptr(_) | AdeshType::VoidPtr => ArgValue::Ptr(value_as_ptr(v)?),
        AdeshType::String => match v {
            Value::Str(s) => {
                let c = CString::new(s.as_str())
                    .map_err(|_| "String arguments cannot contain NUL bytes".to_string())?;
                let ptr = c.as_ptr() as *mut c_void;
                ArgValue::CString { _storage: c, ptr }
            }
            Value::Null => ArgValue::Ptr(std::ptr::null_mut()),
            _ => return Err("String parameters expect Adesh string".to_string()),
        },
        AdeshType::Void => ArgValue::I32(0),
        AdeshType::I128 | AdeshType::U128 => {
            return Err("128-bit integers are not supported in FFI calls".to_string());
        }
    })
}

/// Type signature for a foreign function
#[derive(Debug, Clone)]
pub struct ForeignFunctionSignature {
    /// Parameter types in order
    pub params: Vec<AdeshType>,
    /// Return type
    pub ret_type: AdeshType,
}

impl ForeignFunctionSignature {
    /// Create a new signature
    pub fn new(params: Vec<AdeshType>, ret_type: AdeshType) -> Self {
        ForeignFunctionSignature { params, ret_type }
    }
}

/// Loaded foreign function entry
#[derive(Clone)]
pub struct ForeignFunction {
    /// Function name (symbol)
    pub name: String,
    /// ABI type
    pub abi: AbiType,
    /// Function pointer (as raw void*)
    /// On Windows: FARPROC from GetProcAddress
    /// On Unix: result from dlsym
    /// On WASM: exported function index or handle
    pub ptr: *const std::ffi::c_void,
    /// Function signature
    pub signature: ForeignFunctionSignature,
}

/// SAFETY: Raw pointers are safe to send across threads if the underlying
/// library handles are properly managed (e.g., dlopen keeps libraries loaded)
unsafe impl Send for ForeignFunction {}
unsafe impl Sync for ForeignFunction {}

/// Global FFI registry (lazily initialized, Arc-based for thread-safe sharing)
pub struct FfiRegistry {
    functions: FxHashMap<String, ForeignFunction>,
    /// Library handles for dlopen/LoadLibrary (to keep libraries loaded)
    library_handles: Vec<*mut std::ffi::c_void>,
    /// Search paths for libraries
    search_paths: Vec<PathBuf>,
    /// Default libraries to search (-l)
    link_libs: Vec<String>,
    /// Enable verbose FFI logging
    ffi_debug: bool,
}

/// SAFETY: Raw pointers in library_handles are safe to send across threads
/// because they are managed by dlopen/LoadLibrary and kept loaded via Box::leak.
unsafe impl Send for FfiRegistry {}
unsafe impl Sync for FfiRegistry {}

impl FfiRegistry {
    /// Create a new registry with FxHashMap for fast lookups
    pub fn new() -> Self {
        FfiRegistry {
            functions: FxHashMap::default(),
            library_handles: Vec::new(),
            search_paths: Vec::new(),
            link_libs: Vec::new(),
            ffi_debug: false,
        }
    }

    /// Register a foreign function
    pub fn register(&mut self, func: ForeignFunction) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Look up a foreign function by name
    pub fn lookup(&self, name: &str) -> Option<ForeignFunction> {
        self.functions.get(name).cloned()
    }

    /// Add a library search path
    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Add a default library name (-l)
    pub fn add_link_lib(&mut self, lib: String) {
        if !self.link_libs.contains(&lib) {
            self.link_libs.push(lib);
        }
    }

    /// Enable or disable debug logging
    pub fn set_debug(&mut self, enabled: bool) {
        self.ffi_debug = enabled;
    }

    /// Get all search paths
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Load a shared library and register functions from it
    #[cfg(target_os = "windows")]
    pub fn load_library(&mut self, lib_name: &str) -> Result<(), String> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        unsafe {
            unsafe extern "system" {
                fn LoadLibraryW(lpLibFileName: *const u16) -> *mut std::ffi::c_void;
            }

            let wide: Vec<u16> = OsStr::new(lib_name)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let handle = LoadLibraryW(wide.as_ptr());

            if handle.is_null() {
                return Err(format!("Failed to load library: {}", lib_name));
            }

            self.library_handles.push(handle);
            Ok(())
        }
    }

    /// Load a shared library on Unix (Linux/macOS)
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn load_library(&mut self, lib_name: &str) -> Result<(), String> {
        use libloading::Library;

        // SAFETY: Loading a library is unsafe as it can execute arbitrary code
        // We trust the library path provided by the user
        let lib = unsafe { Library::new(lib_name) }
            .map_err(|e| format!("Failed to load library '{}': {}", lib_name, e))?;

        // Keep the library handle loaded
        let handle = Box::leak(Box::new(lib));
        self.library_handles
            .push(handle as *mut _ as *mut std::ffi::c_void);

        Ok(())
    }

    /// Resolve a function symbol from a loaded library
    #[allow(unused_variables)]
    pub fn resolve_symbol(
        &mut self,
        lib_name: &str,
        symbol: &str,
    ) -> Result<*const std::ffi::c_void, String> {
        let mut candidates: Vec<String> = Vec::new();

        // helper to append platform extension variants
        let push_variants = |base: &str, list: &mut Vec<String>| {
            list.push(base.to_string());
            if base.contains(std::path::MAIN_SEPARATOR) || base.contains('/') {
                return;
            }
            let has_ext = base.contains('.');
            if cfg!(target_os = "windows") {
                if !has_ext {
                    list.push(format!("{}.dll", base));
                    list.push(format!("lib{}.dll", base));
                }
            } else if cfg!(target_os = "macos") {
                if !has_ext {
                    list.push(format!("lib{}.dylib", base));
                }
            } else {
                if !has_ext {
                    list.push(format!("lib{}.so", base));
                }
            }
        };

        push_variants(lib_name, &mut candidates);
        // Prepend search paths
        let mut with_paths: Vec<String> = Vec::new();
        for cand in &candidates {
            let p = PathBuf::from(cand);
            if p.is_absolute() || cand.contains(std::path::MAIN_SEPARATOR) {
                with_paths.push(cand.clone());
                continue;
            }
            for sp in &self.search_paths {
                with_paths.push(sp.join(cand).to_string_lossy().to_string());
            }
            with_paths.push(cand.clone());
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        let mut ordered: Vec<String> = Vec::new();
        for c in with_paths {
            if seen.insert(c.clone()) {
                ordered.push(c);
            }
        }

        #[cfg(target_os = "windows")]
        {
            unsafe {
                unsafe extern "system" {
                    fn LoadLibraryA(lpLibFileName: *const u8) -> *mut std::ffi::c_void;
                    fn GetProcAddress(
                        hModule: *mut std::ffi::c_void,
                        lpProcName: *const u8,
                    ) -> *const std::ffi::c_void;
                }

                let sym_cstr =
                    CString::new(symbol).map_err(|_| "Invalid symbol name".to_string())?;
                let mut last_err: Option<String> = None;

                for cand in ordered {
                    if self.ffi_debug {
                        eprintln!("[ffi] resolving '{}' in '{}'", symbol, cand);
                    }
                    match CString::new(cand.clone()) {
                        Ok(lib_cstr) => {
                            let handle = LoadLibraryA(lib_cstr.as_ptr() as *const u8);
                            if handle.is_null() {
                                last_err = Some(format!("Could not load library: {}", cand));
                                continue;
                            }
                            self.library_handles.push(handle);
                            let ptr = GetProcAddress(handle, sym_cstr.as_ptr() as *const u8);
                            if !ptr.is_null() {
                                return Ok(ptr);
                            }
                            last_err = Some(format!(
                                "Symbol '{}' not found in library '{}'",
                                symbol, cand
                            ));
                        }
                        Err(_) => {
                            last_err = Some("Invalid library name".to_string());
                        }
                    }
                }
                Err(last_err.unwrap_or_else(|| "Symbol resolution failed".to_string()))
            }
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            unsafe {
                unsafe extern "C" {
                    fn dlopen(filename: *const u8, flags: i32) -> *mut std::ffi::c_void;
                    fn dlsym(
                        handle: *mut std::ffi::c_void,
                        symbol: *const u8,
                    ) -> *const std::ffi::c_void;
                }

                const RTLD_LAZY: i32 = 1;

                let sym_cstr =
                    CString::new(symbol).map_err(|_| "Invalid symbol name".to_string())?;
                let mut last_err: Option<String> = None;

                for cand in ordered {
                    if self.ffi_debug {
                        eprintln!("[ffi] resolving '{}' in '{}'", symbol, cand);
                    }
                    match CString::new(cand.clone()) {
                        Ok(lib_cstr) => {
                            let handle = dlopen(lib_cstr.as_ptr() as *const u8, RTLD_LAZY);
                            if handle.is_null() {
                                last_err = Some(format!("Could not load library: {}", cand));
                                continue;
                            }
                            let ptr = dlsym(handle, sym_cstr.as_ptr() as *const u8);
                            if !ptr.is_null() {
                                self.library_handles.push(handle);
                                return Ok(ptr);
                            }
                            last_err = Some(format!(
                                "Symbol '{}' not found in library '{}'",
                                symbol, cand
                            ));
                        }
                        Err(_) => {
                            last_err = Some("Invalid library name".to_string());
                        }
                    }
                }

                Err(last_err.unwrap_or_else(|| "Symbol resolution failed".to_string()))
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err("Library loading not supported on this platform".to_string())
        }
    }

    /// Resolve a symbol from the current process (statically linked/embedded)
    #[allow(unused_variables)]
    pub fn resolve_current_process(&self, symbol: &str) -> Result<*const std::ffi::c_void, String> {
        let sym_cstr = CString::new(symbol).map_err(|_| "Invalid symbol name".to_string())?;

        #[cfg(target_os = "windows")]
        unsafe {
            let lib = libloading::os::windows::Library::this()
                .map_err(|e| format!("Could not access current process library: {}", e))?;
            let sym: libloading::os::windows::Symbol<*const c_void> = lib
                .get(sym_cstr.as_bytes_with_nul())
                .map_err(|e| format!("Symbol '{}' not found in current process: {}", symbol, e))?;
            Ok(*sym)
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        unsafe {
            // SAFETY: Accessing the current process library is safe when done correctly
            // The libloading API has changed; use open_self() instead of this()
            let _lib = libloading::os::unix::Library::open(None::<&str>, 0x1 | 0x2) // RTLD_LAZY | RTLD_GLOBAL
                .map_err(|e| format!("Could not access current process library: {}", e))?;
            let lib = libloading::os::unix::Library::this();
            let sym: libloading::os::unix::Symbol<*const c_void> = lib
                .get(sym_cstr.as_bytes_with_nul())
                .map_err(|e| format!("Symbol '{}' not found in current process: {}", symbol, e))?;
            Ok(*sym)
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err("Current-process symbol resolution not supported on this platform".to_string())
        }
    }

    /// Resolve a symbol by trying explicit libraries
    pub fn resolve_with_libs(
        &mut self,
        libs: &[String],
        symbol: &str,
    ) -> Result<*const std::ffi::c_void, String> {
        let mut last_err: Option<String> = None;

        let lib_list: Vec<String> = if libs.is_empty() {
            self.link_libs.clone()
        } else {
            libs.to_vec()
        };

        for lib in &lib_list {
            match self.resolve_symbol(lib, symbol) {
                Ok(ptr) => return Ok(ptr),
                Err(e) => last_err = Some(e),
            }
        }

        match self.resolve_current_process(symbol) {
            Ok(ptr) => Ok(ptr),
            Err(e) => Err(last_err.unwrap_or(e)),
        }
    }
}

/// Global FFI registry (thread-safe singleton)
static REGISTRY: Lazy<Arc<Mutex<FfiRegistry>>> =
    Lazy::new(|| Arc::new(Mutex::new(FfiRegistry::new())));

/// Get the global FFI registry
pub fn get_registry() -> Arc<Mutex<FfiRegistry>> {
    REGISTRY.clone()
}

/// Register a foreign function globally
pub fn register_function(func: ForeignFunction) {
    let registry = get_registry();
    registry.lock().unwrap().register(func);
}

/// Look up a foreign function globally
pub fn lookup_function(name: &str) -> Option<ForeignFunction> {
    let registry = get_registry();
    registry.lock().unwrap().lookup(name)
}

/// Lookup with detailed error message if not found
pub fn lookup_function_or_error(name: &str) -> Result<ForeignFunction, String> {
    match lookup_function(name) {
        Some(func) => Ok(func),
        None => {
            let registry = get_registry();
            let reg = registry.lock().unwrap();
            let registered: Vec<String> = reg.functions.keys().cloned().collect();

            let mut error = format!("FFI function '{}' not found.\n", name);

            if registered.is_empty() {
                error.push_str("No extern functions have been registered.\n");
                error.push_str("Make sure you have declared the function with 'extern \"C\" fn ...' before calling it.\n");
            } else {
                error.push_str(&format!(
                    "Registered functions: {}\n",
                    registered.join(", ")
                ));

                // Suggest similar names (simple Levenshtein-like heuristic)
                let similar: Vec<&String> = registered
                    .iter()
                    .filter(|&n| {
                        n.contains(&name[..name.len().min(3)])
                            || name.contains(&n[..n.len().min(3)])
                    })
                    .collect();
                if !similar.is_empty() {
                    error.push_str(&format!(
                        "Did you mean: {}?\n",
                        similar
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }

            if reg.library_handles.is_empty() {
                error.push_str(
                    "No libraries loaded. Use --import <path> or -l <name> to load libraries.\n",
                );
            } else {
                error.push_str(&format!(
                    "Loaded libraries: {} library handles\n",
                    reg.library_handles.len()
                ));
            }

            if !reg.search_paths.is_empty() {
                error.push_str(&format!(
                    "Search paths: {}\n",
                    reg.search_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            error.push_str("Troubleshooting:\n");
            error.push_str("  - Add library path with: -L /path/to/lib\n");
            error.push_str(
                "  - Link library with: -l mylib (finds libmylib.so/mylib.dll/libmylib.dylib)\n",
            );
            error.push_str("  - Load explicit library: --import /full/path/to/library.so\n");
            error.push_str("  - Enable debug output: --ffi-debug\n");

            Err(error)
        }
    }
}

/// Add a library search path
pub fn add_search_path(path: PathBuf) {
    let registry = get_registry();
    registry.lock().unwrap().add_search_path(path);
}

/// Add a default link library (-l)
pub fn add_link_lib(lib: String) {
    let registry = get_registry();
    registry.lock().unwrap().add_link_lib(lib);
}

/// Enable or disable FFI debug logging globally
pub fn set_debug(enabled: bool) {
    let registry = get_registry();
    registry.lock().unwrap().set_debug(enabled);
}

/// Register an extern function declaration with the global registry, resolving its symbol immediately
pub fn register_extern_function(
    decl: &ExternFunctionDecl,
    libs: &[String],
) -> Result<ForeignFunction, String> {
    let abi = AbiType::from_str(&decl.abi).ok_or_else(|| format!("Unknown ABI '{}'", decl.abi))?;
    if !abi.is_valid_for_platform() {
        return Err(format!(
            "ABI '{}' is not supported on this platform",
            decl.abi
        ));
    }

    let params: Vec<AdeshType> = decl
        .params
        .iter()
        .map(|(_, t)| {
            AdeshType::from_str(t).ok_or_else(|| format!("Unknown parameter type '{}'", t))
        })
        .collect::<Result<_, _>>()?;
    let ret_type = AdeshType::from_str(&decl.ret_type)
        .ok_or_else(|| format!("Unknown return type '{}'", decl.ret_type))?;

    let registry = get_registry();
    let mut reg = registry.lock().unwrap();

    if reg.ffi_debug {
        eprintln!("[ffi] registering extern '{}' ABI {}", decl.name, decl.abi);
    }

    let primary = reg.resolve_with_libs(libs, &decl.name);

    let ptr = match primary {
        Ok(p) => p,
        Err(e) => {
            if libs.is_empty() {
                let fallbacks = default_system_libs();
                if !fallbacks.is_empty() {
                    reg.resolve_with_libs(&fallbacks, &decl.name)
                        .map_err(|_| e)?
                } else {
                    return Err(format!("Failed to resolve symbol '{}': {}", decl.name, e));
                }
            } else {
                return Err(format!("Failed to resolve symbol '{}': {}", decl.name, e));
            }
        }
    };

    let func = ForeignFunction {
        name: decl.name.clone(),
        abi,
        ptr,
        signature: ForeignFunctionSignature::new(params, ret_type),
    };

    reg.register(func.clone());
    Ok(func)
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
/// Call a foreign function using libffi and marshal to Adesh `Value`
pub fn call_foreign(func: &ForeignFunction, args: &[Value]) -> Result<Value, String> {
    if args.len() != func.signature.params.len() {
        return Err(format!(
            "FFI argument count mismatch for '{}': expected {} arguments, got {}.\nExpected signature: {}({}) -> {:?}",
            func.name,
            func.signature.params.len(),
            args.len(),
            func.name,
            func.signature
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| format!("arg{}: {:?}", i, t))
                .collect::<Vec<_>>()
                .join(", "),
            func.signature.ret_type
        ));
    }

    let arg_types: Vec<Type> = func
        .signature
        .params
        .iter()
        .map(to_ffi_type)
        .collect::<Result<_, _>>()?;
    let ret_type = to_ffi_type(&func.signature.ret_type)?;

    let mut prepared_args: Vec<ArgValue> = Vec::with_capacity(args.len());
    for (i, (value, ty)) in args.iter().zip(func.signature.params.iter()).enumerate() {
        prepared_args.push(make_arg_value(value, ty).map_err(|e| {
            format!(
                "FFI argument type mismatch at position {}: {}\nExpected: {:?}, Got: {:?}",
                i, e, ty, value
            )
        })?);
    }

    let arg_refs: Vec<Arg> = prepared_args.iter().map(|a| a.as_arg()).collect();
    let cif = Cif::new(arg_types, ret_type);
    let code_ptr = CodePtr::from_ptr(func.ptr as *mut c_void);

    unsafe {
        match func.signature.ret_type {
            AdeshType::Void => {
                cif.call::<()>(code_ptr, &arg_refs);
                Ok(Value::Null)
            }
            AdeshType::Bool => {
                let r: i32 = cif.call(code_ptr, &arg_refs);
                Ok(Value::Bool(r != 0))
            }
            AdeshType::I8 => {
                let r: i8 = cif.call(code_ptr, &arg_refs);
                Ok(Value::I8(r))
            }
            AdeshType::I16 => {
                let r: i16 = cif.call(code_ptr, &arg_refs);
                Ok(Value::I16(r))
            }
            AdeshType::I32 => {
                let r: i32 = cif.call(code_ptr, &arg_refs);
                Ok(Value::I32(r))
            }
            AdeshType::I64 => {
                let r: i64 = cif.call(code_ptr, &arg_refs);
                Ok(Value::I64(r))
            }
            AdeshType::U8 => {
                let r: u8 = cif.call(code_ptr, &arg_refs);
                Ok(Value::U8(r))
            }
            AdeshType::U16 => {
                let r: u16 = cif.call(code_ptr, &arg_refs);
                Ok(Value::U16(r))
            }
            AdeshType::U32 => {
                let r: u32 = cif.call(code_ptr, &arg_refs);
                Ok(Value::U32(r))
            }
            AdeshType::U64 => {
                let r: u64 = cif.call(code_ptr, &arg_refs);
                Ok(Value::U64(r))
            }
            AdeshType::F32 => {
                let r: f32 = cif.call(code_ptr, &arg_refs);
                Ok(Value::F32(r))
            }
            AdeshType::F64 => {
                let r: f64 = cif.call(code_ptr, &arg_refs);
                Ok(Value::F64(r))
            }
            AdeshType::Ptr(_) | AdeshType::VoidPtr => {
                let r: *mut c_void = cif.call(code_ptr, &arg_refs);
                if r.is_null() {
                    Ok(Value::Null)
                } else {
                    Ok(Value::U64(r as u64))
                }
            }
            AdeshType::String => {
                let r: *mut c_void = cif.call(code_ptr, &arg_refs);
                if r.is_null() {
                    return Ok(Value::Null);
                }
                let cstr = CStr::from_ptr(r as *const c_char);
                let s = cstr.to_string_lossy().into_owned();
                Ok(Value::Str(s))
            }
            AdeshType::I128 | AdeshType::U128 => {
                Err("128-bit integers are not supported in FFI calls".to_string())
            }
        }
    }
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
/// Call a foreign function using libffi and marshal to Adesh `Value` (fallback on Android and WebAssembly)
pub fn call_foreign(_func: &ForeignFunction, _args: &[Value]) -> Result<Value, String> {
    Err("Foreign function calls (FFI) are not supported in the WebAssembly browser environment".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_type_parsing() {
        assert_eq!(AbiType::from_str("C"), Some(AbiType::C));
        assert_eq!(AbiType::from_str("Rust"), Some(AbiType::Rust));
        assert_eq!(AbiType::from_str("Go"), Some(AbiType::Go));
        assert_eq!(AbiType::from_str("Wasm"), Some(AbiType::Wasm));
        assert_eq!(AbiType::from_str("invalid"), None);
    }

    #[test]
    fn test_adesh_type_parsing() {
        assert_eq!(AdeshType::from_str("i32"), Some(AdeshType::I32));
        assert_eq!(AdeshType::from_str("u64"), Some(AdeshType::U64));
        assert_eq!(AdeshType::from_str("f64"), Some(AdeshType::F64));
        assert_eq!(AdeshType::from_str("void"), Some(AdeshType::Void));
        assert_eq!(AdeshType::from_str("ptr<void>"), Some(AdeshType::VoidPtr));
        assert_eq!(AdeshType::from_str("ptr<u8>"), Some(AdeshType::String));
    }

    #[test]
    fn test_adesh_type_size() {
        assert_eq!(AdeshType::I8.size_bytes(), 1);
        assert_eq!(AdeshType::I32.size_bytes(), 4);
        assert_eq!(AdeshType::I64.size_bytes(), 8);
        assert_eq!(AdeshType::I128.size_bytes(), 16);
        assert_eq!(AdeshType::VoidPtr.size_bytes(), 8);
    }

    #[test]
    fn test_registry_singleton() {
        let reg1 = get_registry();
        let reg2 = get_registry();
        // Both should point to the same underlying registry
        assert!(Arc::ptr_eq(&reg1, &reg2));
    }
}
