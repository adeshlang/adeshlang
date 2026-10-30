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
use adeshlang::toolchain::RuntimeConfig;
use adeshlang::toolchain::formatter::format_source;
use adeshlang::typesystem::type_system::check_module_in;

#[derive(Serialize, Clone)]
pub struct Diagnostic {
    pub severity: String, // "error" | "warning" | "info"
    pub kind: String,     // e.g. "ParseError", "TypeError", "RuntimeError"
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

// Memory Allocation API for JS <-> WASM boundary.
//
// The pair must be layout-sound: `alloc` and `free` are called with the very
// same `size` from JS, so the dealloc layout always matches the alloc layout.
// (A previous implementation used `Vec::with_capacity`, whose real allocation
// can be larger than requested, producing a layout mismatch on free that
// corrupted the wasm allocator and trapped with spurious
// "memory access out of bounds" errors.)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_alloc(size: usize) -> *mut u8 {
    unsafe {
        if size == 0 {
            return std::ptr::null_mut();
        }
        match std::alloc::Layout::from_size_align(size, 8) {
            Ok(layout) => std::alloc::alloc(layout),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            if let Ok(layout) = std::alloc::Layout::from_size_align(size, 8) {
                std::alloc::dealloc(ptr, layout);
            }
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
// Virtual module store (browser workspace)
//
// wasm32 has no filesystem, so `import "..."` statements could never resolve.
// The embedder registers every workspace file here before run/check; the
// interpreter resolves imports from this store before touching the disk.
// ----------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_wasm_set_module(
    path_ptr: *const u8,
    path_len: usize,
    src_ptr: *const u8,
    src_len: usize,
) {
    if path_ptr.is_null() || src_ptr.is_null() {
        return;
    }
    unsafe {
        let path_slice = std::slice::from_raw_parts(path_ptr, path_len);
        let src_slice = std::slice::from_raw_parts(src_ptr, src_len);
        if let (Ok(path), Ok(src)) = (
            std::str::from_utf8(path_slice),
            std::str::from_utf8(src_slice),
        ) {
            adeshlang::execution::runtime_core::vfs::register(path, src.to_string());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_wasm_clear_modules() {
    adeshlang::execution::runtime_core::vfs::clear();
}

// ----------------------------------------------------------------------------
// Memory statistics
// ----------------------------------------------------------------------------

/// Shadow-stack reservation, kept in sync with the linker flag
/// `-zstack-size=16777216` in `.cargo/config.toml`. The wasm32 stack lives at
/// the bottom of linear memory ([0, stack)), static data follows it
/// ([stack, __data_end)), and the dlmalloc heap grows upward from
/// `__heap_base`. wasm-ld lays the regions out in exactly this order.
const WASM_STACK_SIZE: usize = 16 * 1024 * 1024;

unsafe extern "C" {
    #[link_name = "__data_end"]
    static DATA_END: u8;
    #[link_name = "__heap_base"]
    static HEAP_BASE: u8;
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct MemStats {
    /// Reserved interpreter shadow stack (lower 16 MiB of linear memory).
    stackSizeBytes: usize,
    /// Static data segment (engine code tables, stdlib sources, ...).
    dataSizeBytes: usize,
    /// Byte offset where the heap starts; the JS side derives live heap usage
    /// as `memory.buffer.byteLength - heapBaseBytes`.
    heapBaseBytes: usize,
}

/// Break the linear-memory layout into its parts so the UI can report the
/// *actual* heap a program allocated instead of the full reservation.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_wasm_mem_stats() -> *mut c_char {
    let data_end = unsafe { std::ptr::addr_of!(DATA_END) as usize };
    let heap_base = unsafe { std::ptr::addr_of!(HEAP_BASE) as usize };
    let stats = MemStats {
        stackSizeBytes: WASM_STACK_SIZE,
        dataSizeBytes: data_end.saturating_sub(WASM_STACK_SIZE),
        heapBaseBytes: heap_base,
    };
    string_to_c_ptr(serde_json::to_string(&stats).unwrap_or_default())
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
        hint: err
            .hint
            .clone()
            .or_else(|| err.note.clone().map(|note| format!("note: {}", note))),
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
        let label = if d.severity == "warning" {
            "warning"
        } else {
            "error"
        };
        let code = d
            .code
            .as_ref()
            .map(|c| format!("[{}]", c))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}{}: {} --> {}:{}:{}\n",
            label,
            code,
            d.message,
            d.file.as_deref().unwrap_or(""),
            d.line,
            d.column
        ));
    }
    out
}

/// Structured view of an interpreter runtime error string.
///
/// The interpreter returns `Err(String)`; errors constructed with
/// `err_with_span` embed a rendered `LangError` (kind label, `-->` position,
/// source snippet, caret), while older call sites embed `[at file:line:col]`
/// markers or no position at all. This parser recovers the best position and
/// a clean one-line message from every variant.
struct RuntimeParse {
    kind: String,
    message: String,
    line: Option<usize>,
    col: Option<usize>,
    hint: Option<String>,
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip ESC [ ... letter sequences
            if chars.next() == Some('[') {
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_usize(s: &str) -> Option<usize> {
    s.trim().parse::<usize>().ok().filter(|&n| n > 0)
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
        } else if parts.len() == 2
            && let Ok(line) = parts[0].trim().parse::<usize>()
        {
            return Some((line, 1));
        }
        search = &rest[end..];
    }
    None
}

fn parse_runtime_error(raw: &str) -> RuntimeParse {
    let text = strip_ansi(raw);

    // Position from a rendered LangError's `-->` marker:
    //   "--> line 3 col 11" | "--> main.adesh:3:11" | "--> main.adesh:3"
    let mut line = None;
    let mut col = None;
    if let Some(marker) = text.find("--> ") {
        let marker_line: &str = text[marker + 4..].lines().next().unwrap_or("");
        let loc = marker_line.split_whitespace().last().unwrap_or("");
        let nums: Vec<&str> = loc.rsplitn(3, ':').collect();
        match nums.len() {
            3 => {
                col = parse_usize(nums[0]);
                line = parse_usize(nums[1]);
            }
            2 => {
                if let Some(n) = parse_usize(nums[0]) {
                    line = Some(n);
                } else {
                    line = parse_usize(nums[1]);
                }
            }
            _ => {}
        }
        if line.is_none() {
            // "--> line 3 col 11"
            let words: Vec<&str> = marker_line.split_whitespace().collect();
            let mut it = words.iter();
            while let Some(w) = it.next() {
                if w == &"line" {
                    line = parse_usize(it.next().copied().unwrap_or(""));
                } else if w == &"col" {
                    col = parse_usize(it.next().copied().unwrap_or(""));
                }
            }
        }
    }

    // Fall back to `[at file:line:col]` markers from the interpreter's
    // run-module context when no rendered position exists.
    if line.is_none()
        && let Some((l, c)) = runtime_error_position(&text)
    {
        line = Some(l);
        col = Some(c);
    }

    // Headline: the first line of the rendered error ("KindLabel: message"),
    // or the message minus any `[at ...]` marker.
    let mut message = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("-->"))
        .unwrap_or("runtime error")
        .trim()
        .to_string();
    if let Some(at) = message.find("[at ") {
        let trimmed = message[..at].trim_end();
        if !trimmed.is_empty() {
            message = trimmed.to_string();
        }
    }

    // Split a recognized kind label off the headline.
    let mut kind = "RuntimeError".to_string();
    for label in [
        "RuntimeError",
        "TypeError",
        "ParseError",
        "LexicalError",
        "CompileError",
    ] {
        let prefix = format!("{}: ", label);
        if let Some(rest) = message.strip_prefix(&prefix) {
            kind = label.to_string();
            message = rest.to_string();
            break;
        }
    }

    // Hint: the rendered error may carry `help: ...` lines.
    let hint = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("help: "))
        .map(|h| h.to_string());

    RuntimeParse {
        kind,
        message,
        line,
        col,
        hint,
    }
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
    #[cfg(not(target_arch = "wasm32"))]
    let start = Instant::now();
    let (has_errors, diagnostics) = collect_all_diagnostics(code, file_name);
    if has_errors {
        let res = RunResult {
            ok: false,
            stdout: String::new(),
            stderr: render_diagnostics(&diagnostics),
            result: None,
            error: Some("Program contains errors".to_string()),
            #[cfg(not(target_arch = "wasm32"))]
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            // wasm32-unknown-unknown has no clock: the JS caller measures
            // wall-clock duration itself.
            #[cfg(target_arch = "wasm32")]
            duration_ms: 0.0,
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
    #[cfg(not(target_arch = "wasm32"))]
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    #[cfg(target_arch = "wasm32")]
    let duration_ms = 0.0;

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
            // Recover position, kind, and a clean one-line message from the
            // interpreter's rendered runtime error.
            let parsed = parse_runtime_error(&err);
            let line = parsed.line.unwrap_or(1);
            let col = parsed.col.unwrap_or(1);
            let mut runtime_diagnostics = diagnostics; // keep warnings
            runtime_diagnostics.push(Diagnostic {
                severity: "error".to_string(),
                kind: parsed.kind.clone(),
                message: parsed.message.clone(),
                line,
                column: col,
                end_line: line,
                end_column: col + 1,
                source_line: extract_source_line(code, line),
                file: Some(file_name.to_string()),
                code: None,
                hint: parsed.hint,
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
