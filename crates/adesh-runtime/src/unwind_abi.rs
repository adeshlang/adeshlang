//! Adesh Native Exception ABI & Two-Phase DWARF Personality (`__adesh_personality_v0`).
//!
//! ABI version: `ALSD` v1 (Adesh Language Specific Data) + Itanium/GCC `_Unwind_*`
//! exception header (`AdeshUnwindException`, class `"ADESH001"`).
//!
//! Two-phase unwinding contract on ELF x86-64:
//! 1. Search phase (`_UA_SEARCH_PHASE`): `__adesh_personality_v0` inspects the
//!    per-function `ALSD` v1 LSDA via `_Unwind_GetLanguageSpecificData` and
//!    `_Unwind_GetIPInfo` / `_Unwind_GetRegionStart`. If the call-site IP falls
//!    inside a `Catch` region (and unwinding is not forced for a foreign class),
//!    it returns `_URC_HANDLER_FOUND`.
//! 2. Cleanup/handler phase (`_UA_CLEANUP_PHASE`):
//!    - On the handler frame (`_UA_HANDLER_FRAME`), it sets `RAX` (DWARF reg 0)
//!      to the `_Unwind_Exception*`, `RDX` (DWARF reg 1) to `1`, sets IP to the
//!      `Catch` landing pad, and returns `_URC_INSTALL_CONTEXT`.
//!    - On intermediate frames with a matching `Cleanup` region, it sets `RAX`
//!      to the `_Unwind_Exception*`, `RDX` (DWARF reg 1) to `2`, sets IP to the
//!      `Cleanup` landing pad, and returns `_URC_INSTALL_CONTEXT`.
//!
//! Double-exception during cleanup policy:
//! Entering a `Cleanup` landing pad increments the per-thread `UNWIND_CLEANUP_DEPTH`
//! counter via `adesh_rt_unwind_cleanup_enter`, and leaving via `adesh_rt_unwind_resume`
//! decrements it. Throwing while `UNWIND_CLEANUP_DEPTH > 0` prints a diagnostic
//! and aborts the process (`std::process::abort`) rather than corrupting active
//! unwind state.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::raw::{c_int, c_void};

use crate::{RuntimeMutex, RuntimeValue, aot_get_exception, aot_store_value, thread_key, unpack_aot_arg};

/// 8-byte Itanium exception class identifying Adesh exceptions (`"ADESH001"`).
pub const ADESH_EXCEPTION_CLASS: u64 = u64::from_be_bytes(*b"ADESH001");

/// Magic header for Adesh compact LSDA tables in `.gcc_except_table` (`"ALSD"`).
pub const ADESH_LSDA_MAGIC: [u8; 4] = *b"ALSD";

/// Version 1 of the Adesh compact LSDA table format.
pub const ADESH_LSDA_VERSION: u32 = 1;

/// Landing-pad entry kinds inside `ALSD` v1.
pub const ADESH_EH_KIND_CATCH: u8 = 1;
pub const ADESH_EH_KIND_CLEANUP: u8 = 2;

// Standard GCC/Itanium `_Unwind_Action` bitflags (verified against <unwind.h>).
const _UA_SEARCH_PHASE: c_int = 1;
const _UA_CLEANUP_PHASE: c_int = 2;
const _UA_HANDLER_FRAME: c_int = 4;
const _UA_FORCE_UNWIND: c_int = 8;

// Standard GCC/Itanium `_Unwind_Reason_Code` values (verified against <unwind.h>).
const _URC_FATAL_PHASE2_ERROR: c_int = 2;
const _URC_FATAL_PHASE1_ERROR: c_int = 3;
const _URC_HANDLER_FOUND: c_int = 6;
const _URC_INSTALL_CONTEXT: c_int = 7;
const _URC_CONTINUE_UNWIND: c_int = 8;

/// Standard Itanium/GCC `struct _Unwind_Exception` header (32 bytes, 16-byte aligned).
#[repr(C, align(16))]
pub struct UnwindExceptionHeader {
    pub exception_class: u64,
    pub exception_cleanup: Option<unsafe extern "C" fn(reason: c_int, exc: *mut UnwindExceptionHeader)>,
    pub private_1: usize,
    pub private_2: usize,
}

/// Adesh native unwind exception object passed through `_Unwind_RaiseException`.
#[repr(C, align(16))]
pub struct AdeshUnwindException {
    pub header: UnwindExceptionHeader,
    pub payload_handle: u64,
    pub exception_tag: u64,
    pub rethrown: u8,
}

/// Per-thread active cleanup depth (without `thread_local!` for standalone Mach-O/static safety).
static UNWIND_CLEANUP_DEPTH: RuntimeMutex<BTreeMap<u64, u32>> = RuntimeMutex::new(BTreeMap::new());

fn current_cleanup_depth() -> u32 {
    let key = thread_key();
    UNWIND_CLEANUP_DEPTH
        .lock()
        .get(&key)
        .copied()
        .unwrap_or(0)
}

fn push_cleanup_depth() {
    let key = thread_key();
    let mut map = UNWIND_CLEANUP_DEPTH.lock();
    let entry = map.entry(key).or_insert(0);
    *entry = entry.saturating_add(1);
}

fn pop_cleanup_depth() {
    let key = thread_key();
    let mut map = UNWIND_CLEANUP_DEPTH.lock();
    if let Some(val) = map.get_mut(&key) {
        if *val <= 1 {
            map.remove(&key);
        } else {
            *val -= 1;
        }
    }
}

unsafe extern "C" fn adesh_exception_cleanup(_reason: c_int, exc: *mut UnwindExceptionHeader) {
    if !exc.is_null() {
        unsafe {
            drop(Box::from_raw(exc as *mut AdeshUnwindException));
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C-unwind" {
    fn _Unwind_RaiseException(exc: *mut UnwindExceptionHeader) -> c_int;
    fn _Unwind_Resume(exc: *mut UnwindExceptionHeader) -> !;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" {
    fn _Unwind_DeleteException(exc: *mut UnwindExceptionHeader);
    fn _Unwind_GetIPInfo(context: *mut c_void, ip_before_insn: *mut c_int) -> u64;
    fn _Unwind_SetGR(context: *mut c_void, index: c_int, new_value: u64);
    fn _Unwind_SetIP(context: *mut c_void, new_value: u64);
    fn _Unwind_GetLanguageSpecificData(context: *mut c_void) -> *const u8;
    fn _Unwind_GetRegionStart(context: *mut c_void) -> u64;
}

/// Decoded `ALSD` v1 entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlsdEntry {
    pub start_offset: u32,
    pub length: u32,
    pub landing_pad_offset: u32,
    pub kind: u8,
}

/// Parse and validate an `ALSD` v1 table from raw bytes (bounded, deterministic).
///
/// Header (12 bytes):
/// - `0..4`: `"ALSD"`
/// - `4..8`: `version` (`1u32.to_le_bytes()`)
/// - `8..12`: `entry_count` (`u32`)
/// Followed by `entry_count` * 16-byte entries:
/// - `0..4`: `start_offset: u32`
/// - `4..8`: `length: u32`
/// - `8..12`: `landing_pad_offset: u32`
/// - `12`: `kind: u8` (`1 = Catch`, `2 = Cleanup`)
/// - `13..16`: reserved `[u8; 3]`
pub fn parse_alsd_entries(bytes: &[u8]) -> Option<Vec<AlsdEntry>> {
    if bytes.len() < 12 || bytes[0..4] != ADESH_LSDA_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    if version != ADESH_LSDA_VERSION {
        return None;
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    let needed = 12usize.checked_add(count.checked_mul(16)?)?;
    if bytes.len() < needed {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = 12 + i * 16;
        let start_offset = u32::from_le_bytes(bytes[base..base + 4].try_into().ok()?);
        let length = u32::from_le_bytes(bytes[base + 4..base + 8].try_into().ok()?);
        let landing_pad_offset = u32::from_le_bytes(bytes[base + 8..base + 12].try_into().ok()?);
        let kind = bytes[base + 12];
        if kind != ADESH_EH_KIND_CATCH && kind != ADESH_EH_KIND_CLEANUP {
            return None;
        }
        out.push(AlsdEntry {
            start_offset,
            length,
            landing_pad_offset,
            kind,
        });
    }
    Some(out)
}

/// Encode an `ALSD` v1 table for emission into `.gcc_except_table`.
pub fn encode_alsd_entries(entries: &[AlsdEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + entries.len() * 16);
    out.extend_from_slice(&ADESH_LSDA_MAGIC);
    out.extend_from_slice(&ADESH_LSDA_VERSION.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        out.extend_from_slice(&e.start_offset.to_le_bytes());
        out.extend_from_slice(&e.length.to_le_bytes());
        out.extend_from_slice(&e.landing_pad_offset.to_le_bytes());
        out.push(e.kind);
        out.extend_from_slice(&[0u8; 3]);
    }
    out
}

/// Match an instruction-relative offset inside an `ALSD` v1 table stored at `lsda`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe fn lookup_alsd_entry(lsda: *const u8, rel_pc: u64, want_kind: u8) -> Option<AlsdEntry> {
    if lsda.is_null() {
        return None;
    }
    let header = unsafe { std::slice::from_raw_parts(lsda, 12) };
    if header[0..4] != ADESH_LSDA_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(header[4..8].try_into().ok()?);
    if version != ADESH_LSDA_VERSION {
        return None;
    }
    let count = u32::from_le_bytes(header[8..12].try_into().ok()?) as usize;
    let total_len = 12usize.checked_add(count.checked_mul(16)?)?;
    let table = unsafe { std::slice::from_raw_parts(lsda, total_len) };

    for i in 0..count {
        let base = 12 + i * 16;
        let start_offset = u64::from(u32::from_le_bytes(table[base..base + 4].try_into().ok()?));
        let length = u64::from(u32::from_le_bytes(table[base + 4..base + 8].try_into().ok()?));
        let landing_pad_offset = u32::from_le_bytes(table[base + 8..base + 12].try_into().ok()?);
        let kind = table[base + 12];
        if kind != want_kind {
            continue;
        }
        let end_offset = start_offset.saturating_add(length);
        if rel_pc >= start_offset && rel_pc < end_offset {
            return Some(AlsdEntry {
                start_offset: start_offset as u32,
                length: length as u32,
                landing_pad_offset,
                kind,
            });
        }
    }
    None
}

/// DWARF/Itanium personality routine for Adesh ELF x86-64 frames.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __adesh_personality_v0(
    version: c_int,
    actions: c_int,
    exception_class: u64,
    exception_object: *mut UnwindExceptionHeader,
    context: *mut c_void,
) -> c_int {
    if version != 1 || context.is_null() {
        return _URC_FATAL_PHASE1_ERROR;
    }

    let lsda = unsafe { _Unwind_GetLanguageSpecificData(context) };
    if lsda.is_null() {
        return _URC_CONTINUE_UNWIND;
    }

    let header = unsafe { std::slice::from_raw_parts(lsda, 12) };
    if header[0..4] != ADESH_LSDA_MAGIC {
        return _URC_FATAL_PHASE1_ERROR;
    }
    let lsda_version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if lsda_version != ADESH_LSDA_VERSION {
        return _URC_FATAL_PHASE1_ERROR;
    }

    let region_start = unsafe { _Unwind_GetRegionStart(context) };
    let mut ip_before_insn: c_int = 0;
    let raw_ip = unsafe { _Unwind_GetIPInfo(context, &mut ip_before_insn) };
    let ip = if ip_before_insn != 0 {
        raw_ip
    } else {
        raw_ip.saturating_sub(1)
    };
    let Some(rel_pc) = ip.checked_sub(region_start) else {
        return _URC_CONTINUE_UNWIND;
    };

    let is_forced = (actions & _UA_FORCE_UNWIND) != 0;
    let is_adesh_exc = exception_class == ADESH_EXCEPTION_CLASS;

    if (actions & _UA_SEARCH_PHASE) != 0 {
        if !is_forced
            && is_adesh_exc
            && unsafe { lookup_alsd_entry(lsda, rel_pc, ADESH_EH_KIND_CATCH) }.is_some()
        {
            return _URC_HANDLER_FOUND;
        }
        return _URC_CONTINUE_UNWIND;
    }

    if (actions & _UA_CLEANUP_PHASE) != 0 {
        if (actions & _UA_HANDLER_FRAME) != 0 {
            if let Some(entry) = unsafe { lookup_alsd_entry(lsda, rel_pc, ADESH_EH_KIND_CATCH) } {
                unsafe {
                    _Unwind_SetGR(context, 0, exception_object as u64);
                    _Unwind_SetGR(context, 1, 1);
                    _Unwind_SetIP(context, region_start + u64::from(entry.landing_pad_offset));
                }
                return _URC_INSTALL_CONTEXT;
            }
            return _URC_FATAL_PHASE2_ERROR;
        }

        if let Some(entry) = unsafe { lookup_alsd_entry(lsda, rel_pc, ADESH_EH_KIND_CLEANUP) } {
            unsafe {
                _Unwind_SetGR(context, 0, exception_object as u64);
                _Unwind_SetGR(context, 1, 2);
                _Unwind_SetIP(context, region_start + u64::from(entry.landing_pad_offset));
            }
            return _URC_INSTALL_CONTEXT;
        }
        return _URC_CONTINUE_UNWIND;
    }

    _URC_FATAL_PHASE1_ERROR
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __adesh_personality_v0(
    _version: c_int,
    _actions: c_int,
    _exception_class: u64,
    _exception_object: *mut UnwindExceptionHeader,
    _context: *mut c_void,
) -> c_int {
    _URC_CONTINUE_UNWIND
}

fn format_exception_value(val: &RuntimeValue) -> String {
    match val {
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Int(i) => i.to_string(),
        RuntimeValue::Float(f) => f.to_string(),
        RuntimeValue::Bool(b) => b.to_string(),
        RuntimeValue::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

fn raise_exception_internal(val_handle: u64, rethrown: u8) -> ! {
    if current_cleanup_depth() > 0 {
        let mut stderr = std::io::stderr();
        let _ = writeln!(
            stderr,
            "adesh runtime: double exception during unwind cleanup; aborting"
        );
        let _ = stderr.flush();
        std::process::abort();
    }

    let val = unpack_aot_arg(val_handle);
    let durable_handle = aot_store_value(val);

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let exc = Box::new(AdeshUnwindException {
            header: UnwindExceptionHeader {
                exception_class: ADESH_EXCEPTION_CLASS,
                exception_cleanup: Some(adesh_exception_cleanup),
                private_1: 0,
                private_2: 0,
            },
            payload_handle: durable_handle,
            exception_tag: 0,
            rethrown,
        });
        let exc_ptr = Box::into_raw(exc);
        let rc = unsafe { _Unwind_RaiseException(&mut (*exc_ptr).header) };
        // `_Unwind_RaiseException` only returns on unhandled exception or fatal
        // unwind error. Clean up the exception box before exiting non-zero.
        let payload = unsafe {
            let h = (*exc_ptr).payload_handle;
            drop(Box::from_raw(exc_ptr));
            h
        };
        let msg_preview = format_exception_value(&unpack_aot_arg(payload));
        let mut stderr = std::io::stderr();
        let _ = writeln!(
            stderr,
            "adesh runtime: unhandled exception (unwind rc={rc}): {msg_preview}"
        );
        let _ = stderr.flush();
        std::process::exit(101);
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = rethrown;
        let msg_preview = format_exception_value(&unpack_aot_arg(durable_handle));
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "adesh runtime: unhandled exception: {msg_preview}");
        let _ = stderr.flush();
        std::process::exit(101);
    }
}

/// Throw an Adesh exception using real native two-phase stack unwinding.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn adesh_rt_unwind_throw(val_handle: u64) -> ! {
    raise_exception_internal(val_handle, 0)
}

/// Rethrow an Adesh exception using real native two-phase stack unwinding.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn adesh_rt_unwind_rethrow(val_handle: u64) -> ! {
    raise_exception_internal(val_handle, 1)
}

/// Enter a `Catch` landing pad, extract the payload handle, and delete the `_Unwind_Exception`.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_unwind_catch_enter(exc_ptr: u64) -> u64 {
    if exc_ptr == 0 {
        return aot_get_exception();
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let header_ptr = exc_ptr as *mut UnwindExceptionHeader;
        let payload = unsafe {
            if (*header_ptr).exception_class == ADESH_EXCEPTION_CLASS {
                let adesh_exc = header_ptr as *mut AdeshUnwindException;
                (*adesh_exc).payload_handle
            } else {
                aot_store_value(RuntimeValue::String("foreign exception".to_string()))
            }
        };
        unsafe {
            _Unwind_DeleteException(header_ptr);
        }
        payload
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = exc_ptr;
        aot_get_exception()
    }
}

/// Mark entry into a `Cleanup` landing pad so a nested throw during cleanup aborts deterministically.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_unwind_cleanup_enter() -> i64 {
    push_cleanup_depth();
    0
}

/// Mark exit from try-local cleanup before entering a `catch` body.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_unwind_cleanup_exit() -> i64 {
    pop_cleanup_depth();
    0
}

/// Resume stack unwinding after a `Cleanup` landing pad finishes all drops/defers/scope exits.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn adesh_rt_unwind_resume(exc_ptr: u64) -> ! {
    pop_cleanup_depth();
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        if exc_ptr != 0 {
            unsafe {
                _Unwind_Resume(exc_ptr as *mut UnwindExceptionHeader);
            }
        }
    }
    let _ = exc_ptr;
    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "adesh runtime: invalid null unwind state in adesh_rt_unwind_resume; aborting"
    );
    let _ = stderr.flush();
    std::process::abort();
}
