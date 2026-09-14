use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::session::AdeshSession;

unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn string_to_c_str(s: String) -> *mut c_char {
    let sanitized = if s.contains('\0') {
        s.replace('\0', "\\0")
    } else {
        s
    };
    CString::new(sanitized).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_session_create() -> *mut AdeshSession {
    let session = Box::new(AdeshSession::new());
    Box::into_raw(session)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_session_destroy(session: *mut AdeshSession) {
    if !session.is_null() {
        unsafe {
            let _ = Box::from_raw(session);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_run(
    session: *mut AdeshSession,
    code: *const c_char,
    filename: *const c_char,
) -> *mut c_char {
    if session.is_null() {
        let err_json = r#"{"status":"failed","stdout":"","stderr":"Null session pointer","execution_time_ms":0.0,"diagnostics":[],"backend_used":"Interpreter"}"#;
        return string_to_c_str(err_json.to_string());
    }

    let session = unsafe { &*session };
    let code_str = unsafe { c_str_to_str(code) };
    let fn_str = unsafe { c_str_to_str(filename) };

    let result = session.run(
        code_str,
        if fn_str.is_empty() {
            None
        } else {
            Some(fn_str)
        },
    );
    string_to_c_str(result.to_json())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_check(
    session: *mut AdeshSession,
    code: *const c_char,
    filename: *const c_char,
) -> *mut c_char {
    let code_str = unsafe { c_str_to_str(code) };
    let fn_str = unsafe { c_str_to_str(filename) };

    let diagnostics = if !session.is_null() {
        let session = unsafe { &*session };
        session.check(
            code_str,
            if fn_str.is_empty() {
                None
            } else {
                Some(fn_str)
            },
        )
    } else {
        let temp_session = AdeshSession::new();
        temp_session.check(
            code_str,
            if fn_str.is_empty() {
                None
            } else {
                Some(fn_str)
            },
        )
    };

    let json = serde_json::to_string(&diagnostics).unwrap_or_else(|_| "[]".to_string());
    string_to_c_str(json)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_cancel(session: *mut AdeshSession) -> i32 {
    if session.is_null() {
        return 0;
    }
    let session = unsafe { &*session };
    session.cancel();
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_format(code: *const c_char) -> *mut c_char {
    let code_str = unsafe { c_str_to_str(code) };
    let temp_session = AdeshSession::new();
    match temp_session.format(code_str) {
        Ok(formatted) => string_to_c_str(formatted),
        Err(e) => string_to_c_str(format!("/* Format Error: {} */\n{}", e, code_str)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn adesh_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_get_version() -> *mut c_char {
    string_to_c_str("AdeshLang v0.3.0 (Mobile Bridge)".to_string())
}
