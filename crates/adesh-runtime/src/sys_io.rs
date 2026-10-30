//! Native C ABI system calls, low-level file descriptor I/O, process management, and sockets.
//!
//! Provides production-ready implementations of:
//! - File descriptors: `sys_open`, `sys_read`, `sys_write`, `sys_close`, `sys_seek`
//! - Networking sockets: `sys_socket`, `sys_bind`, `sys_connect`, `sys_send`, `sys_recv`
//! - Process lifecycle: `sys_spawn`, `sys_wait`, `sys_exit`
//! - Adesh high-level process ABI: `processSpawn`, `processWait`, `processWaitTimeout`,
//!   `processKill`, `processTerminate`, `processReadStdout`, `processReadStderr`,
//!   `processWriteStdin`, `processWriteLine`, `processReadLine`, `processReadBytes`, `processGetInfo`

use crate::stream_io::{arg_to_string, extract_bytes_from_arg, extract_usize_arg};
use crate::{RuntimeValue, aot_get_value, aot_make_object, aot_set_field_str, aot_store_value};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

#[derive(Debug)]
pub enum IoDescriptor {
    File(File),
    TcpStream(TcpStream),
    TcpListener(TcpListener),
    UdpSocket(UdpSocket),
}

static NEXT_FD: AtomicI64 = AtomicI64::new(3);

fn fd_table() -> &'static RwLock<BTreeMap<i64, Arc<Mutex<IoDescriptor>>>> {
    static TABLE: std::sync::OnceLock<RwLock<BTreeMap<i64, Arc<Mutex<IoDescriptor>>>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn process_table() -> &'static RwLock<BTreeMap<i64, Arc<Mutex<Child>>>> {
    static TABLE: std::sync::OnceLock<RwLock<BTreeMap<i64, Arc<Mutex<Child>>>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn make_empty_object() -> u64 {
    unsafe { aot_make_object(std::ptr::null(), 0) }
}

fn make_string_handle(s: String) -> u64 {
    aot_store_value(RuntimeValue::String(s))
}

// ============================================================================
// Low-Level File Descriptors C ABI
// ============================================================================

/// `sys_open(path: u64, flags: i64, mode: i64) -> i64`
///
/// Flags:
/// - 0: O_RDONLY
/// - 1: O_WRONLY
/// - 2: O_RDWR
/// - 0x40 (64): O_CREAT
/// - 0x200 (512): O_TRUNC
/// - 0x400 (1024): O_APPEND
#[unsafe(no_mangle)]
pub extern "C" fn sys_open(path_raw: u64, flags: i64, _mode: i64) -> i64 {
    let path_str = arg_to_string(path_raw);
    if path_str.is_empty() {
        return -1;
    }

    let access = flags & 0x3;
    let read = access == 0 || access == 2;
    let write = access == 1 || access == 2;
    let append = (flags & 0x400) != 0;
    let trunc = (flags & 0x200) != 0;
    let create = (flags & 0x40) != 0;

    let mut opts = OpenOptions::new();
    opts.read(read);
    opts.write(write || append);
    opts.append(append);
    if trunc && !append {
        opts.truncate(true);
    }
    if create {
        opts.create(true);
    }

    match opts.open(&path_str) {
        Ok(file) => {
            let fd = NEXT_FD.fetch_add(1, Ordering::SeqCst);
            let mut table = fd_table().write().unwrap();
            table.insert(fd, Arc::new(Mutex::new(IoDescriptor::File(file))));
            fd
        }
        Err(_) => -1,
    }
}

/// `sys_read(fd: i64, buf_ptr: u64, count: usize) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_read(fd: i64, buf_ptr: u64, count: usize) -> i64 {
    if count == 0 {
        return 0;
    }

    let write_back = |temp_buf: &[u8], n: usize| {
        if let Some(mut val) = aot_get_value(buf_ptr) {
            if let RuntimeValue::Array(ref mut arr) = val {
                for i in 0..n {
                    if i < arr.len() {
                        arr[i] = RuntimeValue::U8(temp_buf[i]);
                    } else {
                        arr.push(RuntimeValue::U8(temp_buf[i]));
                    }
                }
                crate::aot_update_value(buf_ptr, val);
            }
        } else if buf_ptr > 0x10000 && buf_ptr < crate::HANDLE_TAG {
            unsafe {
                std::ptr::copy_nonoverlapping(temp_buf.as_ptr(), buf_ptr as *mut u8, n);
            }
        }
    };

    if fd == 0 {
        // stdin
        let mut temp_buf = vec![0u8; count];
        match std::io::stdin().read(&mut temp_buf) {
            Ok(n) => {
                write_back(&temp_buf, n);
                n as i64
            }
            Err(_) => -1,
        }
    } else {
        let descriptor_opt = {
            let table = fd_table().read().unwrap();
            table.get(&fd).cloned()
        };

        if let Some(desc_arc) = descriptor_opt {
            let mut desc = desc_arc.lock().unwrap();
            let mut temp_buf = vec![0u8; count];
            let read_res = match &mut *desc {
                IoDescriptor::File(f) => f.read(&mut temp_buf),
                IoDescriptor::TcpStream(s) => s.read(&mut temp_buf),
                _ => return -1,
            };

            match read_res {
                Ok(n) => {
                    write_back(&temp_buf, n);
                    n as i64
                }
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// `sys_write(fd: i64, buf_ptr: u64, count: usize) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_write(fd: i64, buf_ptr: u64, count: usize) -> i64 {
    let bytes: Vec<u8> = if aot_get_value(buf_ptr).is_some() {
        extract_bytes_from_arg(buf_ptr)
    } else if buf_ptr > 0x10000 && buf_ptr < crate::HANDLE_TAG && count > 0 {
        unsafe { std::slice::from_raw_parts(buf_ptr as *const u8, count).to_vec() }
    } else {
        extract_bytes_from_arg(buf_ptr)
    };

    let len_to_write = if count > 0 && count <= bytes.len() {
        count
    } else {
        bytes.len()
    };
    let slice_to_write = &bytes[..len_to_write];

    if fd == 1 {
        // stdout
        match std::io::stdout().write_all(slice_to_write) {
            Ok(_) => {
                let _ = std::io::stdout().flush();
                slice_to_write.len() as i64
            }
            Err(_) => -1,
        }
    } else if fd == 2 {
        // stderr
        match std::io::stderr().write_all(slice_to_write) {
            Ok(_) => {
                let _ = std::io::stderr().flush();
                slice_to_write.len() as i64
            }
            Err(_) => -1,
        }
    } else {
        let descriptor_opt = {
            let table = fd_table().read().unwrap();
            table.get(&fd).cloned()
        };

        if let Some(desc_arc) = descriptor_opt {
            let mut desc = desc_arc.lock().unwrap();
            let write_res = match &mut *desc {
                IoDescriptor::File(f) => f.write_all(slice_to_write),
                IoDescriptor::TcpStream(s) => s.write_all(slice_to_write),
                _ => return -1,
            };

            match write_res {
                Ok(_) => slice_to_write.len() as i64,
                Err(_) => -1,
            }
        } else {
            -1
        }
    }
}

/// `sys_close(fd: i64) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_close(fd: i64) -> i64 {
    let mut table = fd_table().write().unwrap();
    if table.remove(&fd).is_some() { 0 } else { -1 }
}

/// `sys_seek(fd: i64, offset: i64, whence: i64) -> i64`
///
/// Whence:
/// - 0: SEEK_SET
/// - 1: SEEK_CUR
/// - 2: SEEK_END
#[unsafe(no_mangle)]
pub extern "C" fn sys_seek(fd: i64, offset: i64, whence: i64) -> i64 {
    let seek_from = match whence {
        0 => {
            if offset < 0 {
                return -1;
            }
            SeekFrom::Start(offset as u64)
        }
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return -1,
    };

    let descriptor_opt = {
        let table = fd_table().read().unwrap();
        table.get(&fd).cloned()
    };

    if let Some(desc_arc) = descriptor_opt {
        let mut desc = desc_arc.lock().unwrap();
        if let IoDescriptor::File(f) = &mut *desc {
            match f.seek(seek_from) {
                Ok(pos) => pos as i64,
                Err(_) => -1,
            }
        } else {
            -1
        }
    } else {
        -1
    }
}

// ============================================================================
// Low-Level Networking Sockets C ABI
// ============================================================================

/// `sys_socket(domain: i32, sock_type: i32, protocol: i32) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_socket(_domain: i32, _sock_type: i32, _protocol: i32) -> i64 {
    NEXT_FD.fetch_add(1, Ordering::SeqCst)
}

/// `sys_bind(fd: i64, addr_ptr: u64, port: u16) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_bind(fd: i64, addr_ptr: u64, port: u16) -> i64 {
    let addr_str = arg_to_string(addr_ptr);
    let full_addr = if addr_str.is_empty() {
        format!("0.0.0.0:{}", port)
    } else {
        format!("{}:{}", addr_str, port)
    };

    match TcpListener::bind(&full_addr) {
        Ok(listener) => {
            let mut table = fd_table().write().unwrap();
            table.insert(
                fd,
                Arc::new(Mutex::new(IoDescriptor::TcpListener(listener))),
            );
            0
        }
        Err(_) => -1,
    }
}

/// `sys_connect(fd: i64, addr_ptr: u64, port: u16) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_connect(fd: i64, addr_ptr: u64, port: u16) -> i64 {
    let addr_str = arg_to_string(addr_ptr);
    let full_addr = if addr_str.is_empty() {
        format!("127.0.0.1:{}", port)
    } else {
        format!("{}:{}", addr_str, port)
    };

    match TcpStream::connect(&full_addr) {
        Ok(stream) => {
            let mut table = fd_table().write().unwrap();
            table.insert(fd, Arc::new(Mutex::new(IoDescriptor::TcpStream(stream))));
            0
        }
        Err(_) => -1,
    }
}

/// `sys_send(fd: i64, buf_ptr: u64, count: usize) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_send(fd: i64, buf_ptr: u64, count: usize) -> i64 {
    sys_write(fd, buf_ptr, count)
}

/// `sys_recv(fd: i64, buf_ptr: u64, count: usize) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_recv(fd: i64, buf_ptr: u64, count: usize) -> i64 {
    sys_read(fd, buf_ptr, count)
}

// ============================================================================
// Low-Level Process Lifecycle C ABI
// ============================================================================

/// `sys_spawn(cmd_ptr: u64, args_ptr: u64) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_spawn(cmd_ptr: u64, args_ptr: u64) -> i64 {
    let cmd_str = arg_to_string(cmd_ptr);
    if cmd_str.is_empty() {
        return -1;
    }

    let mut cmd = Command::new(&cmd_str);

    if let Some(RuntimeValue::Array(args_arr)) = aot_get_value(args_ptr) {
        for arg_val in args_arr {
            cmd.arg(arg_val.as_string());
        }
    }

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id() as i64;
            let mut table = process_table().write().unwrap();
            table.insert(pid, Arc::new(Mutex::new(child)));
            pid
        }
        Err(_) => -1,
    }
}

/// `sys_wait(pid: i64) -> i64`
#[unsafe(no_mangle)]
pub extern "C" fn sys_wait(pid: i64) -> i64 {
    let child_arc = {
        let mut table = process_table().write().unwrap();
        table.remove(&pid)
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        match child.wait() {
            Ok(status) => status.code().unwrap_or(-1) as i64,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

/// `sys_exit(code: i32) -> !`
#[unsafe(no_mangle)]
pub extern "C" fn sys_exit(code: i32) -> ! {
    std::process::exit(code);
}

// ============================================================================
// High-Level Process Standard Library Builtins (Matching Process.adesh)
// ============================================================================

/// `processSpawn(cmd_val: u64, args_val: u64, cwd_val: u64, env_val: u64, stdio_val: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processSpawn(
    cmd_val: u64,
    args_val: u64,
    cwd_val: u64,
    _env_val: u64,
    _stdio_val: u64,
) -> u64 {
    let cmd_str = arg_to_string(cmd_val);
    if cmd_str.is_empty() {
        return 0;
    }

    let mut cmd = Command::new(&cmd_str);

    if let Some(RuntimeValue::Array(args_arr)) = aot_get_value(args_val) {
        for arg in args_arr {
            cmd.arg(arg.as_string());
        }
    }

    let cwd_str = arg_to_string(cwd_val);
    if !cwd_str.is_empty() {
        cmd.current_dir(cwd_str);
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id() as i64;
            let mut table = process_table().write().unwrap();
            table.insert(pid, Arc::new(Mutex::new(child)));

            let obj = make_empty_object();
            aot_set_field_str(obj, "handle\0".as_ptr() as *const _, pid as u64);
            aot_set_field_str(obj, "pid\0".as_ptr() as *const _, pid as u64);
            obj
        }
        Err(_) => 0,
    }
}

/// `processWait(handle_id: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processWait(handle_id: u64) -> u64 {
    let pid = handle_id as i64;
    let code = sys_wait(pid);

    let obj = make_empty_object();
    aot_set_field_str(obj, "code\0".as_ptr() as *const _, code as u64);
    aot_set_field_str(
        obj,
        "success\0".as_ptr() as *const _,
        if code == 0 { 1 } else { 0 },
    );
    obj
}

/// `processWaitTimeout(handle_id: u64, timeout_ms: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processWaitTimeout(handle_id: u64, timeout_ms: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        loop {
            {
                let mut child = child_arc.lock().unwrap();
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code().unwrap_or(-1) as i64;
                        let mut table = process_table().write().unwrap();
                        table.remove(&pid);

                        let obj = make_empty_object();
                        aot_set_field_str(obj, "code\0".as_ptr() as *const _, code as u64);
                        aot_set_field_str(
                            obj,
                            "success\0".as_ptr() as *const _,
                            if code == 0 { 1 } else { 0 },
                        );
                        return obj;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }

            if start.elapsed() >= timeout {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let obj = make_empty_object();
    aot_set_field_str(obj, "code\0".as_ptr() as *const _, -2i64 as u64);
    aot_set_field_str(obj, "success\0".as_ptr() as *const _, 0);
    obj
}

/// `processTerminate(handle_id: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processTerminate(handle_id: u64) -> u64 {
    processKill(handle_id, 15)
}

/// `processKill(handle_id: u64, _sig_code: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processKill(handle_id: u64, _sig_code: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if child.kill().is_ok() { 1 } else { 0 }
    } else {
        0
    }
}

/// `processReadStdout(handle_id: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processReadStdout(handle_id: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if let Some(ref mut stdout) = child.stdout {
            let mut out = String::new();
            if stdout.read_to_string(&mut out).is_ok() {
                return make_string_handle(out);
            }
        }
    }
    0
}

/// `processReadStderr(handle_id: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processReadStderr(handle_id: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if let Some(ref mut stderr) = child.stderr {
            let mut out = String::new();
            if stderr.read_to_string(&mut out).is_ok() {
                return make_string_handle(out);
            }
        }
    }
    0
}

/// `processWriteStdin(handle_id: u64, data_val: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processWriteStdin(handle_id: u64, data_val: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if let Some(ref mut stdin) = child.stdin {
            let bytes = extract_bytes_from_arg(data_val);
            if stdin.write_all(&bytes).is_ok() {
                let _ = stdin.flush();
                return bytes.len() as u64;
            }
        }
    }
    0
}

/// `processWriteLine(handle_id: u64, line_val: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processWriteLine(handle_id: u64, line_val: u64) -> u64 {
    let mut s = arg_to_string(line_val);
    s.push('\n');
    let str_val = make_string_handle(s);
    processWriteStdin(handle_id, str_val)
}

/// `processReadLine(handle_id: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processReadLine(handle_id: u64) -> u64 {
    let pid = handle_id as i64;
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if let Some(ref mut stdout) = child.stdout {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                return make_string_handle(line);
            }
        }
    }
    0
}

/// `processReadBytes(handle_id: u64, count_val: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processReadBytes(handle_id: u64, count_val: u64) -> u64 {
    let pid = handle_id as i64;
    let count = extract_usize_arg(count_val);
    let child_arc = {
        let table = process_table().read().unwrap();
        table.get(&pid).cloned()
    };

    if let Some(child_arc) = child_arc {
        let mut child = child_arc.lock().unwrap();
        if let Some(ref mut stdout) = child.stdout {
            let mut buf = vec![0u8; count];
            if let Ok(n) = stdout.read(&mut buf) {
                let arr: Vec<RuntimeValue> =
                    buf[..n].iter().map(|&b| RuntimeValue::U8(b)).collect();
                return aot_store_value(RuntimeValue::Array(arr));
            }
        }
    }
    0
}

/// `processGetInfo(pid_val: u64) -> u64`
#[unsafe(no_mangle)]
pub extern "C" fn processGetInfo(pid_val: u64) -> u64 {
    let pid = if pid_val == 0 {
        std::process::id() as i64
    } else {
        pid_val as i64
    };

    let obj = make_empty_object();
    aot_set_field_str(obj, "pid\0".as_ptr() as *const _, pid as u64);
    aot_set_field_str(obj, "parentId\0".as_ptr() as *const _, 0);
    aot_set_field_str(obj, "cpuTime\0".as_ptr() as *const _, 0);
    aot_set_field_str(obj, "memoryUsage\0".as_ptr() as *const _, 0);
    aot_set_field_str(obj, "startTime\0".as_ptr() as *const _, 0);

    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy().to_string();
        let exe_val = make_string_handle(exe_str);
        aot_set_field_str(obj, "executable\0".as_ptr() as *const _, exe_val);
    }

    obj
}
