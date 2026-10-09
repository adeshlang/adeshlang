//! Native-pipeline ABI surface (`ADESH_RUNTIME_NATIVE_V1`).
//!
//! Symbols required by `src/backends/native/lower.rs` that previously did not
//! exist at all (every program touching them failed at link time):
//! string concat, range construction, `in` membership, dict construction,
//! raw allocation, wall-clock time, the f64 math dispatcher, and the
//! abort-with-message helper.
//!
//! The print-only native entry points live in `native_output.c`, a separate
//! archive member so simple programs do not extract this broader Rust runtime.

use std::os::raw::c_char;

use crate::{RuntimeValue, aot_store_value, get_string_val, unpack_aot_arg};

/// Length of a NUL-terminated byte string used by native runtime metadata.
unsafe fn cstr_bytes(ptr: *const c_char) -> &'static [u8] {
    if ptr.is_null() {
        return &[];
    }
    let start = ptr.cast::<u8>();
    let mut end = start;
    unsafe {
        while *end != 0 {
            end = end.add(1);
        }
    }
    unsafe { core::slice::from_raw_parts(start, end.offset_from(start) as usize) }
}

// ============================================================================
// Time
// ============================================================================

/// Wall-clock time in seconds since the Unix epoch, returned as f64 in XMM0.
/// Raw OS calls: no `std::time` machinery gets linked.
#[unsafe(no_mangle)]
pub extern "C" fn aot_clock() -> f64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            lo: u32,
            hi: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetSystemTimeAsFileTime(lpSystemTimeAsFileTime: *mut FileTime) -> ();
        }
        let mut ft = FileTime::default();
        unsafe { GetSystemTimeAsFileTime(&mut ft) };
        let ticks = ((ft.hi as u64) << 32) | ft.lo as u64;
        // 100ns intervals since 1601-01-01 → seconds since 1970-01-01.
        ((ticks as i64 - 11_644_473_600_000_000) as f64) / 1e7
    }
    #[cfg(not(windows))]
    {
        #[repr(C)]
        struct TimeVal {
            sec: i64,
            usec: i64,
        }
        #[link(name = "c")]
        unsafe extern "C" {
            fn gettimeofday(tv: *mut TimeVal, tz: *mut core::ffi::c_void) -> i32;
        }
        let mut tv = TimeVal { sec: 0, usec: 0 };
        unsafe { gettimeofday(&mut tv, core::ptr::null_mut()) };
        tv.sec as f64 + (tv.usec as f64) / 1e6
    }
}

// ============================================================================
// f64 math dispatcher
// ============================================================================

/// Dispatcher for float builtins: `aot_math_f64(op, x, y) -> f64` (XMM0).
/// Opcodes match `src/backends/native/lower.rs`:
/// 1 sin, 2 cos, 3 tan, 4 asin, 5 acos, 6 atan, 7 sqrt, 8 exp, 9 log,
/// 10 log10, 11 fabs, 12 floor, 13 ceil, 14 round, 16 pow(x, y).
#[unsafe(no_mangle)]
pub extern "C" fn aot_math_f64(op: i64, x: f64, y: f64) -> f64 {
    match op {
        1 => x.sin(),
        2 => x.cos(),
        3 => x.tan(),
        4 => x.asin(),
        5 => x.acos(),
        6 => x.atan(),
        7 => x.sqrt(),
        8 => x.exp(),
        9 => x.ln(),
        10 => x.log10(),
        11 => x.abs(),
        12 => x.floor(),
        13 => x.ceil(),
        14 => x.round(),
        16 => x.powf(y),
        _ => f64::NAN,
    }
}

// ============================================================================
// Strings / ranges / dicts / membership
// ============================================================================

fn value_as_string(handle: u64) -> String {
    get_string_val(handle).unwrap_or_else(|| unpack_aot_arg(handle).as_string())
}

/// Concatenate two values as strings (the `+` operator on strings).
#[unsafe(no_mangle)]
pub extern "C" fn aot_string_concat(a_handle: u64, b_handle: u64) -> u64 {
    let a = value_as_string(a_handle);
    let b = value_as_string(b_handle);
    aot_store_value(RuntimeValue::String(a + &b))
}

/// Materialize `start..end` (inclusive == 0) or `start..=end` (inclusive
/// != 0) as an array of integers, matching the interpreter/Cranelift backend
/// range semantics. Descending ranges count down.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_range(start: i64, end: i64, inclusive: i64) -> u64 {
    let mut items = Vec::new();
    if start <= end {
        let stop = if inclusive != 0 { end + 1 } else { end };
        let mut i = start;
        while i < stop {
            items.push(RuntimeValue::Int(i));
            i += 1;
        }
    } else {
        let stop = if inclusive != 0 { end - 1 } else { end };
        let mut i = start;
        while i > stop {
            items.push(RuntimeValue::Int(i));
            i -= 1;
        }
    }
    aot_store_value(RuntimeValue::Array(items))
}

/// Create an empty dict; entries are added via `aot_set_index`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_dict() -> u64 {
    aot_store_value(RuntimeValue::Object(std::collections::BTreeMap::new()))
}

/// `item in container` membership test. Returns a raw 0/1 (never a handle).
/// Arrays/tuples/sets test element membership, strings test substring
/// containment, objects/dicts test key presence.
#[unsafe(no_mangle)]
pub extern "C" fn aot_contains(container_handle: u64, item_handle: u64) -> i64 {
    let container =
        crate::aot_get_value(container_handle).unwrap_or_else(|| unpack_aot_arg(container_handle));
    let item = crate::aot_get_value(item_handle).unwrap_or_else(|| unpack_aot_arg(item_handle));
    match container {
        RuntimeValue::Array(v) => v.contains(&item) as i64,
        RuntimeValue::Tuple(v) => v.contains(&item) as i64,
        RuntimeValue::Set(v) => v.contains(&item) as i64,
        RuntimeValue::String(s) => s.contains(&item.as_string()) as i64,
        RuntimeValue::Object(obj) => obj.contains_key(&item.as_string()) as i64,
        RuntimeValue::BTreeMap(map) => map.contains_key(&item.as_string()) as i64,
        RuntimeValue::HashSet(set) => set.contains(&item.as_string()) as i64,
        _ => 0,
    }
}

// ============================================================================
// Raw allocation
// ============================================================================

/// Raw byte allocation for `alloc<T>(size)`. Returns a heap pointer.
#[unsafe(no_mangle)]
pub extern "C" fn aot_alloc(size: i64) -> *mut core::ffi::c_void {
    if size <= 0 {
        return core::ptr::null_mut();
    }
    unsafe { libc::malloc(size as usize) as *mut core::ffi::c_void }
}

/// Free a pointer previously returned by `aot_alloc`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_free(ptr: *mut core::ffi::c_void) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut core::ffi::c_void) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_roundtrip_across_sizes() {
        for size in [1i64, 8, 24, 256, 4096, 1 << 20] {
            let p = aot_alloc(size) as *mut u8;
            assert!(!p.is_null(), "size {size}");
            unsafe {
                p.write(0xAB);
                p.add(size as usize - 1).write(0xCD);
            }
            aot_free(p as *mut core::ffi::c_void);
        }
        assert!(aot_alloc(0).is_null());
        assert!(aot_alloc(-5).is_null());
        aot_free(core::ptr::null_mut());
    }
}

// ============================================================================
// PGO profile dump (Phase 4, `--pgo=generate`)
// ============================================================================

/// One `__pgo_table` entry: `{name_rva, counter_rva}` image RVAs. The table
/// is zero-terminated and emitted by the backend's `emit_object` whenever
/// `pgo_inc` instrumentation is present.
#[repr(C)]
pub struct PgoTableEntry {
    name_rva: u32,
    counter_rva: u32,
}

/// Per-function counters aggregated from the table: the entry-block count
/// (`entry_count`) and per-block execution counts keyed by block id.
#[derive(Default)]
struct PgoFnProfile {
    entry_count: u64,
    blocks: std::collections::BTreeMap<u32, u64>,
}

/// Called by the `__adesh_windows_start` stub after `main` returns, before
/// `ExitProcess`: walks the `__pgo_table` and writes
/// `adesh_pgo_profile.json` (in the process working directory) in exactly
/// the `ProfileData` JSON shape the compiler consumes via
/// `--pgo=use=<path>`.
///
/// Best-effort and panic-isolated (unwinding through the FFI boundary from
/// the synthesized startup stub would be undefined behaviour); failures go
/// to stderr and never alter the program's exit code.
#[unsafe(no_mangle)]
pub extern "C" fn aot_pgo_dump(table_ptr: *const PgoTableEntry) {
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pgo_dump_impl(table_ptr)));
    if result.is_err() {
        eprintln!("adesh pgo: profile dump failed (instrumented process panicked while dumping)");
    }
}

fn pgo_dump_impl(table_ptr: *const PgoTableEntry) {
    if table_ptr.is_null() {
        return;
    }
    let base = match image_base() {
        Some(b) => b,
        None => {
            eprintln!("adesh pgo: image base unavailable; profile not dumped");
            return;
        }
    };

    let mut funcs: std::collections::BTreeMap<String, PgoFnProfile> =
        std::collections::BTreeMap::new();
    // Sanity bound: a corrupt table must never walk off the mapping.
    const MAX_ENTRIES: usize = 1 << 20;
    unsafe {
        for i in 0..MAX_ENTRIES {
            let entry = &*table_ptr.add(i);
            if entry.name_rva == 0 && entry.counter_rva == 0 {
                break;
            }
            let name = cstr_bytes((base + entry.name_rva as usize) as *const c_char);
            let count = ((base + entry.counter_rva as usize) as *const u64).read_unaligned();
            pgo_record(&mut funcs, name, count);
        }
    }
    pgo_write_profile(&funcs);
}

/// Map one counter observation onto the aggregate. Counter names follow the
/// instrumentation pass grammar: `__pgo_entry_<fn>_<block_id>` (the entry
/// block) and `__pgo_counter_<fn>_<block_id>`; the block id is the last
/// `_`-separated component so function names may contain underscores.
fn pgo_record(
    funcs: &mut std::collections::BTreeMap<String, PgoFnProfile>,
    name: &[u8],
    count: u64,
) {
    let (is_entry, rest) = if let Some(r) = name.strip_prefix(b"__pgo_entry_") {
        (true, r)
    } else if let Some(r) = name.strip_prefix(b"__pgo_counter_") {
        (false, r)
    } else {
        eprintln!(
            "adesh pgo: unrecognized counter name `{}`, skipping",
            String::from_utf8_lossy(name)
        );
        return;
    };
    let rest = String::from_utf8_lossy(rest);
    let Some((fn_name, id_str)) = rest.rsplit_once('_') else {
        eprintln!("adesh pgo: malformed counter name `{}`, skipping", rest);
        return;
    };
    let Ok(block_id) = id_str.parse::<u32>() else {
        eprintln!(
            "adesh pgo: counter `{}` has a non-numeric block id, skipping",
            rest
        );
        return;
    };
    let prof = funcs.entry(fn_name.to_string()).or_default();
    if is_entry {
        prof.entry_count = prof.entry_count.saturating_add(count);
    }
    prof.blocks.insert(block_id, count);
}

/// Minimal JSON string escaping (identifiers only need `"` and `\`).
fn pgo_json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn pgo_write_profile(funcs: &std::collections::BTreeMap<String, PgoFnProfile>) {
    if funcs.is_empty() {
        return;
    }
    // Hand-rolled JSON in the exact `ProfileData` serde shape (all fields
    // are required on deserialize); keeps serde_json out of every linked
    // program's binary.
    let mut json = String::from("{\n  \"functions\": {");
    for (i, (fname, prof)) in funcs.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let esc = pgo_json_escape(fname);
        json.push_str(&format!(
            "\n    \"{esc}\": {{\n      \"name\": \"{esc}\",\n      \"entry_count\": {},\n      \"block_profiles\": {{",
            prof.entry_count
        ));
        for (j, (block_id, count)) in prof.blocks.iter().enumerate() {
            if j > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "\n        \"{block_id}\": {{\"execution_count\": {count}}}"
            ));
        }
        json.push_str("\n      },\n      \"edge_profiles\": {}\n    }");
    }
    json.push_str("\n  }\n}");

    if let Err(e) = std::fs::write("adesh_pgo_profile.json", json) {
        eprintln!("adesh pgo: failed to write adesh_pgo_profile.json: {e}");
    }
}

/// Image base of the main module (needed to turn table RVAs into VAs).
#[cfg(windows)]
fn image_base() -> Option<usize> {
    unsafe {
        let handle = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(core::ptr::null());
        if handle == 0 {
            None
        } else {
            Some(handle as usize)
        }
    }
}

/// Non-Windows hosts never link an instrumented PE (the backend rejects
/// `--pgo=generate` for non-PE targets loudly), so there is no image base
/// to query.
#[cfg(not(windows))]
fn image_base() -> Option<usize> {
    None
}
