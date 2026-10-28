//! WebAssembly bindings for the Adesh Programming Language.
//!
//! Provides C-ABI WebAssembly exports for the in-browser editor:
//! - Full static analysis (`adesh_wasm_check_full`): ALL errors from the
//!   recovering lexer/parser/type-checker plus unused-symbol warnings,
//!   each with span, code, hint and source line
//! - Full in-browser execution (`adesh_wasm_run`) with in-memory
//!   stdout/stderr capture and positioned runtime diagnostics
//! - Source code formatting via `adesh_wasm_format`
//! - Memory management and version introspection

use serde::Serialize;
use std::ffi::CString;
use std::os::raw::c_char;
use std::path::Path;
use std::time::Instant;

use adeshlang::execution::runtime::{Interpreter, ModuleLoader};
use adeshlang::execution::runtime_core::stdio;
use adeshlang::parsing::error::{DiagnosticSeverity, ErrorKind, LangError};
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use adeshlang::parsing::unused_warnings::UnusedWarningPass;
use adeshlang::toolchain::formatter::format_source;
use adeshlang::toolchain::RuntimeConfig;
use adeshlang::typesystem::type_system::check_module_in;

#[derive(Serialize, Clone)]
pub struct Diagnostic {
    pub severity: String, // "error" | "warning" | "info"
    pub kind: String,    // e.g. "ParseError", "TypeError", "RuntimeError"
    pub message: String,
    pub line: usize,
    pub column: usize,
    #[serde(rename = "endLine")]
    pub end_line: usize,
    #[serde(rename = "endColumn")]
    pub end_column: usize,
    /// The raw source line the diagnostic points at (for Problems panels).
    #[serde(rename = "source")]
    pub source_line: String,
    pub file: Option<String>,
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
    /// Static diagnostics (when the program was refused) or the positioned
    /// runtime error diagnostic (when execution failed).
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
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
    CString::new(s)
        .unwrap_or_else(|_| CString::new("{}").unwrap())
        .into_raw()
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

// ----------------------------------------------------------------------------
// Entropy backend for browser builds
// ----------------------------------------------------------------------------

/// wasm32-unknown-unknown has no OS entropy source, and this crate is loaded
/// through a glue-free C-ABI loader (no wasm-bindgen runtime), so `getrandom`
/// runs on the `custom` backend. This shim keeps `random()`, `uuid()` and
/// other RNG builtins functional inside the browser playground with a
/// deterministic splitmix64 stream (playground semantics, not cryptographic
/// randomness).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod wasm_entropy {
    #[allow(clippy::missing_safety_doc)]
    fn custom_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
        use std::cell::Cell;

        #[inline]
        fn next_u64(state: &Cell<u64>) -> u64 {
            let mut z = state.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
            state.set(z);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        thread_local! {
            static STATE: Cell<u64> = Cell::new(0x853C_49E6_748F_EA9B);
        }

        STATE.with(|state| {
            for chunk in buf.chunks_mut(8) {
                let bytes = next_u64(state).to_le_bytes();
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
        });

        Ok(())
    }

    // Exactly one definition across the whole linked graph: this crate is the
    // final cdylib consumed by the browser loader.
    getrandom::register_custom_getrandom!(custom_getrandom);
}

// ----------------------------------------------------------------------------
// Diagnostics pipeline (shared by check & run)
// ----------------------------------------------------------------------------

/// Convert a `LangError` (flattening any accumulated `Multiple` bag) into the
/// wire `Diagnostic` format with span, code, hint and source line.
fn lang_error_to_diagnostic(err: &LangError) -> Diagnostic {
    let severity = match err.severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Info => "info",
    };
    let line = if err.line > 0 { err.line } else { 1 };
    let col = if err.col > 0 { err.col } else { 1 };
    let end_line = if err.end_line > 0 { err.end_line } else { line };
    let end_col = if err.end_col > 0 { err.end_col } else { col };

    Diagnostic {
        severity: severity.to_string(),
        kind: err.kind.label().to_string(),
        message: err.message.clone(),
        line,
        column: col,
        end_line,
        end_column: end_col,
        source_line: err.line_text.clone(),
        file: err.file.clone(),
        code: err.code.clone(),
        hint: err.hint.clone().or_else(|| {
            err.note.clone().map(|note| format!("note: {}", note))
        }),
    }
}

/// Flatten a `LangError` (possibly an accumulated `Multiple` bag) into
/// individual wire diagnostics.
fn flatten_lang_error(err: &LangError) -> Vec<Diagnostic> {
    match &err.kind {
        ErrorKind::Multiple(inner) => inner.iter().flat_map(flatten_lang_error).collect(),
        _ => vec![lang_error_to_diagnostic(err)],
    }
}

/// Run the full static analysis pipeline: recovering lexer, recovering
/// parser, the canonical type checker, and the unused-symbol warning pass.
/// Returns every error and warning (never collapses to a single message).
fn collect_all_diagnostics(code: &str, file: &str) -> (bool, Vec<Diagnostic>) {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // 1. Canonical check pipeline (lex + parse + type) with recovery.
    match check_module_in(code, Some(file)) {
        Ok(()) => {}
        Err(err) => diagnostics.extend(flatten_lang_error(&err)),
    }

    // A broken parse produces meaningless warnings; only run the unused pass
    // when the program parsed cleanly.
    let parse_broken = diagnostics
        .iter()
        .any(|d| d.severity == "error" && (d.kind == "LexicalError" || d.kind == "ParseError"));

    // 2. Unused-symbol warnings (structured, with line:col).
    if !parse_broken {
        let mut lexer = Lexer::new(code);
        lexer.set_file(file.to_string());
        let (tokens, _) = lexer.tokenize_recovering();
        let mut parser = Parser::new(tokens, Some(file.to_string()));
        let (ast, _) = parser.parse_program_recovering();
        let config = RuntimeConfig::default();
        for (message, line, col) in UnusedWarningPass::collect_warnings(&ast, code, &config) {
            diagnostics.push(Diagnostic {
                severity: "warning".to_string(),
                kind: "UnusedWarning".to_string(),
                message,
                line,
                column: col,
                end_line: line,
                end_column: col,
                source_line: code
                    .lines()
                    .nth(line.saturating_sub(1))
                    .unwrap_or("")
                    .to_string(),
                file: Some(file.to_string()),
                code: Some("unused".to_string()),
                hint: Some("remove it, use it, or prefix the name with `_`".to_string()),
            });
        }
    }

    let has_errors = diagnostics.iter().any(|d| d.severity == "error");
    (has_errors, diagnostics)
}

/// Render a diagnostics list as a human-readable multi-line message
/// (used for the stderr text accompanying a refused run).
fn render_diagnostics(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diagnostics {
        let label = if d.severity == "warning" { "warning" } else { "error" };
        let code = d
            .code
            .as_ref()
            .map(|c| format!("[{}]", c))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}{}: {} --> {}:{}:{}\n",
            label, code, d.message, d.file.as_deref().unwrap_or(""), d.line, d.column
        ));
    }
    out
}

/// Parse `[at <file>:<line>:<col>]` context markers embedded by the
/// interpreter in runtime error strings, returning the first match.
fn runtime_error_position(stderr: &str) -> Option<(usize, usize)> {
    let mut search = stderr;
    while let Some(idx) = search.find("[at ") {
        let rest = &search[idx + 4..];
        let Some(end) = rest.find(']') else { break };
        let inner = &rest[..end];
        let parts: Vec<&str> = inner.rsplitn(3, ':').collect();
        // rsplitn yields [col, line, file-or-empty]
        if parts.len() == 3 {
            if let (Ok(col), Ok(line)) = (parts[0].trim().parse(), parts[1].trim().parse()) {
                return Some((line, col));
            }
        } else if parts.len() == 2 {
            if let Ok(line) = parts[0].trim().parse::<usize>() {
                return Some((line, 1));
            }
        }
        search = &rest[end..];
    }
    None
}

fn extract_source_line(code: &str, line: usize) -> String {
    code.lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .to_string()
}

// ----------------------------------------------------------------------------
// Check exports
// ----------------------------------------------------------------------------

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
                    kind: "LexicalError".to_string(),
                    message: format!("Invalid UTF-8: {}", e),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    source_line: String::new(),
                    file: None,
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

    let (has_errors, diagnostics) = collect_all_diagnostics(code, file_name);
    let result = CheckResult {
        ok: !has_errors,
        diagnostics,
    };

    string_to_c_ptr(serde_json::to_string(&result).unwrap_or_default())
}

/// Alias kept for JS discoverability; identical to `adesh_wasm_check`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_check_full(
    code_ptr: *const u8,
    code_len: usize,
    file_ptr: *const u8,
    file_len: usize,
) -> *mut c_char {
    unsafe { adesh_wasm_check(code_ptr, code_len, file_ptr, file_len) }
}

// ----------------------------------------------------------------------------
// Run export (check-first, then execute — mirroring the native CLI)
// ----------------------------------------------------------------------------

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
                diagnostics: Vec::new(),
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

    // 1. Static analysis first (same as the native CLI): refuse to execute
    //    programs that do not type-check, and surface every diagnostic.
    let start = Instant::now();
    let (has_errors, diagnostics) = collect_all_diagnostics(code, file_name);
    if has_errors {
        let res = RunResult {
            ok: false,
            stdout: String::new(),
            stderr: render_diagnostics(&diagnostics),
            result: None,
            error: Some("Program contains errors".to_string()),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            diagnostics,
        };
        return string_to_c_ptr(serde_json::to_string(&res).unwrap_or_default());
    }

    // 2. Execute through the real interpreter with captured stdio.
    stdio::start_output_capture();

    let mut interpreter = Interpreter::new();
    let mut loader = ModuleLoader::new(Path::new("."));
    let exec_res = interpreter.run_module(code, &mut loader, Some(file_name.to_string()));

    // Flush any pending stdout
    stdio::flush_all();
    let stdout = std::mem::take(&mut stdio::finish_output_capture());
    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    let response = match exec_res {
        Ok(()) => RunResult {
            ok: true,
            stdout,
            stderr: String::new(),
            result: None,
            error: None,
            duration_ms,
            diagnostics, // warnings (if any) carried alongside a clean run
        },
        Err(err) => {
            // Position the runtime error using the interpreter's
            // `[at file:line:col]` context marker when present.
            let (line, col) = runtime_error_position(&err).unwrap_or((1, 1));
            let mut runtime_diagnostics = diagnostics; // keep warnings
            runtime_diagnostics.push(Diagnostic {
                severity: "error".to_string(),
                kind: "RuntimeError".to_string(),
                message: err.clone(),
                line,
                column: col,
                end_line: line,
                end_column: col,
                source_line: extract_source_line(code, line),
                file: Some(file_name.to_string()),
                code: None,
                hint: None,
            });
            RunResult {
                ok: false,
                stdout,
                stderr: err.clone(),
                result: None,
                error: Some(err),
                duration_ms,
                diagnostics: runtime_diagnostics,
            }
        }
    };

    string_to_c_ptr(serde_json::to_string(&response).unwrap_or_default())
}

// ----------------------------------------------------------------------------
// Format export
// ----------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_format(code_ptr: *const u8, code_len: usize) -> *mut c_char {
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
