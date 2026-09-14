//! WebAssembly bindings for the Adesh Programming Language.
//!
//! Provides C-ABI WebAssembly exports for:
//! - Full in-browser execution via `Interpreter` with in-memory stdout/stderr capture
//! - Static analysis and type checking via `check_module_in`
//! - Source code formatting via `format_source`
//! - Memory management and version introspection

use std::ffi::CString;
use std::os::raw::c_char;
use std::path::Path;
use std::time::Instant;
use serde::Serialize;

use adeshlang::execution::runtime::{Interpreter, ModuleLoader};
use adeshlang::execution::runtime_core::stdio;
use adeshlang::toolchain::formatter::format_source;
use adeshlang::typesystem::type_system::check_module_in;

#[derive(Serialize)]
pub struct Diagnostic {
    pub severity: String, // "error" | "warning" | "info"
    pub message: String,
    pub line: usize,
    pub column: usize,
    #[serde(rename = "endLine")]
    pub end_line: usize,
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    pub code: Option<String>,
    pub hint: Option<String>,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
pub struct RunResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "durationMs")]
    pub duration_ms: f64,
}

#[derive(Serialize)]
pub struct FormatResult {
    pub ok: bool,
    pub formatted: Option<String>,
    pub error: Option<String>,
}

// Memory Allocation API for JS <-> WASM boundary
#[unsafe(no_mangle)]
pub extern "C" fn adesh_wasm_alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            drop(Vec::from_raw_parts(ptr, 0, size));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

fn string_to_c_ptr(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_else(|_| CString::new("{}").unwrap()).into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_wasm_init() {
    // Global initializations if necessary
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_wasm_version() -> *mut c_char {
    let ver = env!("CARGO_PKG_VERSION");
    string_to_c_ptr(ver.to_string())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_run(
    code_ptr: *const u8,
    code_len: usize,
    file_ptr: *const u8,
    file_len: usize,
) -> *mut c_char {
    let code_slice = unsafe { std::slice::from_raw_parts(code_ptr, code_len) };
    let code = match std::str::from_utf8(code_slice) {
        Ok(s) => s,
        Err(e) => {
            let res = RunResult {
                ok: false,
                stdout: String::new(),
                stderr: format!("Invalid UTF-8 source: {}", e),
                result: None,
                error: Some(format!("Invalid UTF-8 source: {}", e)),
                duration_ms: 0.0,
            };
            return string_to_c_ptr(serde_json::to_string(&res).unwrap_or_default());
        }
    };

    let file_slice = if file_ptr.is_null() || file_len == 0 {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(file_ptr, file_len) })
    };
    let file_name = file_slice
        .and_then(|s| std::str::from_utf8(s).ok())
        .unwrap_or("main.adesh");

    stdio::start_output_capture();
    let start = Instant::now();

    let mut interpreter = Interpreter::new();
    let mut loader = ModuleLoader::new(Path::new("."));
    let exec_res = interpreter.run_module(code, &mut loader, Some(file_name.to_string()));

    // Flush any pending stdout
    stdio::flush_all();
    let stdout = std::mem::take(&mut stdio::finish_output_capture());
    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    let response = match exec_res {
        Ok(()) => {
            RunResult {
                ok: true,
                stdout,
                stderr: String::new(),
                result: None,
                error: None,
                duration_ms,
            }
        }
        Err(err) => {
            RunResult {
                ok: false,
                stdout,
                stderr: err.clone(),
                result: None,
                error: Some(err),
                duration_ms,
            }
        }
    };

    string_to_c_ptr(serde_json::to_string(&response).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_check(
    code_ptr: *const u8,
    code_len: usize,
    file_ptr: *const u8,
    file_len: usize,
) -> *mut c_char {
    let code_slice = unsafe { std::slice::from_raw_parts(code_ptr, code_len) };
    let code = match std::str::from_utf8(code_slice) {
        Ok(s) => s,
        Err(e) => {
            let res = CheckResult {
                ok: false,
                diagnostics: vec![Diagnostic {
                    severity: "error".to_string(),
                    message: format!("Invalid UTF-8: {}", e),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    code: None,
                    hint: None,
                }],
            };
            return string_to_c_ptr(serde_json::to_string(&res).unwrap_or_default());
        }
    };

    let file_slice = if file_ptr.is_null() || file_len == 0 {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(file_ptr, file_len) })
    };
    let file_name = file_slice
        .and_then(|s| std::str::from_utf8(s).ok())
        .unwrap_or("main.adesh");

    let mut diagnostics = Vec::new();
    let check_res = check_module_in(code, Some(file_name));

    if let Err(lang_err) = check_res {
        let start_line = if lang_err.line > 0 { lang_err.line } else { 1 };
        let start_col = if lang_err.col > 0 { lang_err.col } else { 1 };
        let end_line = if lang_err.end_line > 0 { lang_err.end_line } else { start_line };
        let end_col = if lang_err.end_col > 0 { lang_err.end_col } else { start_col + 1 };

        diagnostics.push(Diagnostic {
            severity: "error".to_string(),
            message: lang_err.message,
            line: start_line,
            column: start_col,
            end_line,
            end_column: end_col,
            code: lang_err.code,
            hint: lang_err.hint,
        });
    }

    let result = CheckResult {
        ok: diagnostics.is_empty(),
        diagnostics,
    };

    string_to_c_ptr(serde_json::to_string(&result).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_format(
    code_ptr: *const u8,
    code_len: usize,
) -> *mut c_char {
    let code_slice = unsafe { std::slice::from_raw_parts(code_ptr, code_len) };
    let code = match std::str::from_utf8(code_slice) {
        Ok(s) => s,
        Err(e) => {
            let res = FormatResult {
                ok: false,
                formatted: None,
                error: Some(format!("Invalid UTF-8: {}", e)),
            };
            return string_to_c_ptr(serde_json::to_string(&res).unwrap_or_default());
        }
    };

    let format_res = format_source(code, None);
    let res = match format_res {
        Ok(formatted) => FormatResult {
            ok: true,
            formatted: Some(formatted),
            error: None,
        },
        Err(e) => FormatResult {
            ok: false,
            formatted: None,
            error: Some(e),
        },
    };

    string_to_c_ptr(serde_json::to_string(&res).unwrap_or_default())
}
