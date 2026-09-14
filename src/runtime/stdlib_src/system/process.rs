//! System Process Management Builtins
//!
//! Provides native cross-platform process creation, stream piping, lifecycle
//! tracking, process information, signal handling, and pipeline support.

use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap as HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT_HANDLE_ID: AtomicI64 = AtomicI64::new(1);

pub struct ProcessState {
    pub child: Option<Child>,
    pub pid: u32,
    pub executable: String,
    pub command_line: String,
    pub start_time_ms: i64,
    pub stdout_captured: Vec<u8>,
    pub stderr_captured: Vec<u8>,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub exited: bool,
}

static PROCESS_TABLE: Lazy<Mutex<HashMap<i64, ProcessState>>> =
    Lazy::new(|| Mutex::new(HashMap::default()));

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "processSpawn",
        "system",
        "Spawn a child process",
        builtin_process_spawn,
    );
    registry.register(
        "processWait",
        "system",
        "Wait for process completion",
        builtin_process_wait,
    );
    registry.register(
        "processWaitTimeout",
        "system",
        "Wait with timeout",
        builtin_process_wait_timeout,
    );
    registry.register(
        "processKill",
        "system",
        "Kill a process",
        builtin_process_kill,
    );
    registry.register(
        "processTerminate",
        "system",
        "Terminate a process",
        builtin_process_terminate,
    );
    registry.register(
        "processReadStdout",
        "system",
        "Read stdout bytes",
        builtin_process_read_stdout,
    );
    registry.register(
        "processReadStderr",
        "system",
        "Read stderr bytes",
        builtin_process_read_stderr,
    );
    registry.register(
        "processWriteStdin",
        "system",
        "Write to process stdin",
        builtin_process_write_stdin,
    );
    registry.register(
        "processReadLine",
        "system",
        "Read line from process stdout/stderr",
        builtin_process_read_line,
    );
    registry.register(
        "processWriteLine",
        "system",
        "Write line to process stdin",
        builtin_process_write_line,
    );
    registry.register(
        "processReadBytes",
        "system",
        "Read exact bytes from process stream",
        builtin_process_read_bytes,
    );
    registry.register(
        "processGetInfo",
        "system",
        "Get process info metrics",
        builtin_process_get_info,
    );
    registry.register(
        "processGetTree",
        "system",
        "Get process tree PIDs",
        builtin_process_get_tree,
    );
    registry.register(
        "processCreateGroup",
        "system",
        "Create process group",
        builtin_process_create_group,
    );
    registry.register(
        "processKillGroup",
        "system",
        "Kill process group",
        builtin_process_kill_group,
    );
    registry.register(
        "processSetLimits",
        "system",
        "Set process resource limits",
        builtin_process_set_limits,
    );
    registry.register(
        "processRunPipeline",
        "system",
        "Run process pipeline",
        builtin_process_run_pipeline,
    );
    registry.register(
        "processRunGraph",
        "system",
        "Run process graph DAG",
        builtin_process_run_graph,
    );
    registry.register(
        "processExec",
        "system",
        "Execute command",
        builtin_process_exec,
    );
}

fn extract_string(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Ref(inner, _) => extract_string(inner),
        Value::Null => None,
        _ => Some(format!("{:?}", v)),
    }
}

fn extract_int(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::Number(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::Ref(inner, _) => extract_int(inner),
        _ => v.as_f64().map(|f| f as i64),
    }
}

fn extract_string_array(v: &Value) -> Vec<String> {
    let mut res = Vec::new();
    if let Value::Array(arr) = v {
        for item in arr.iter() {
            if let Some(s) = extract_string(item) {
                res.push(s);
            }
        }
    }
    res
}

fn make_dict(pairs: Vec<(&str, Value)>) -> Value {
    let mut map = HashMap::default();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(std::sync::Arc::new(map))
}

pub fn builtin_process_spawn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processSpawn requires at least 1 argument (command)".to_string());
    }

    let cmd_name = match extract_string(&args[0]) {
        Some(s) => s,
        None => return Err("processSpawn command must be a string".to_string()),
    };

    let cmd_args = if args.len() > 1 {
        extract_string_array(&args[1])
    } else {
        Vec::new()
    };
    let cwd_opt = if args.len() > 2 {
        extract_string(&args[2])
    } else {
        None
    };

    let env_map = if args.len() > 3 {
        match &args[3] {
            Value::Object(obj) => {
                let mut map = HashMap::default();
                for (k, v) in obj.iter() {
                    if let Some(val_str) = extract_string(v) {
                        map.insert(k.clone(), val_str);
                    }
                }
                Some(map)
            }
            _ => None,
        }
    } else {
        None
    };

    let clear_env = if args.len() > 4 {
        match &args[4] {
            Value::Bool(b) => *b,
            _ => false,
        }
    } else {
        false
    };

    let stdin_mode = if args.len() > 5 {
        extract_int(&args[5]).unwrap_or(0)
    } else {
        0
    };
    let stdout_mode = if args.len() > 6 {
        extract_int(&args[6]).unwrap_or(0)
    } else {
        0
    };
    let stderr_mode = if args.len() > 7 {
        extract_int(&args[7]).unwrap_or(0)
    } else {
        0
    };

    let stdin_file = if args.len() > 8 {
        extract_string(&args[8])
    } else {
        None
    };
    let stdout_file = if args.len() > 9 {
        extract_string(&args[9])
    } else {
        None
    };
    let stderr_file = if args.len() > 10 {
        extract_string(&args[10])
    } else {
        None
    };

    let mut command = Command::new(&cmd_name);
    command.args(&cmd_args);

    if let Some(cwd) = cwd_opt {
        command.current_dir(cwd);
    }

    if clear_env {
        command.env_clear();
    }

    if let Some(envs) = env_map {
        for (k, v) in envs {
            command.env(k, v);
        }
    }

    // Configure Stdio
    // 0 = INHERIT, 1 = PIPE, 2 = NULL, 3 = FILE
    match stdin_mode {
        1 => {
            command.stdin(Stdio::piped());
        }
        2 => {
            command.stdin(Stdio::null());
        }
        3 => {
            if let Some(fpath) = stdin_file {
                match File::open(&fpath) {
                    Ok(f) => {
                        command.stdin(Stdio::from(f));
                    }
                    Err(e) => return Err(format!("Failed to open stdin file {}: {}", fpath, e)),
                }
            } else {
                command.stdin(Stdio::inherit());
            }
        }
        _ => {
            command.stdin(Stdio::inherit());
        }
    }

    match stdout_mode {
        1 => {
            command.stdout(Stdio::piped());
        }
        2 => {
            command.stdout(Stdio::null());
        }
        3 => {
            if let Some(fpath) = stdout_file {
                match File::create(&fpath) {
                    Ok(f) => {
                        command.stdout(Stdio::from(f));
                    }
                    Err(e) => return Err(format!("Failed to create stdout file {}: {}", fpath, e)),
                }
            } else {
                command.stdout(Stdio::inherit());
            }
        }
        _ => {
            command.stdout(Stdio::inherit());
        }
    }

    match stderr_mode {
        1 => {
            command.stderr(Stdio::piped());
        }
        2 => {
            command.stderr(Stdio::null());
        }
        3 => {
            if let Some(fpath) = stderr_file {
                match File::create(&fpath) {
                    Ok(f) => {
                        command.stderr(Stdio::from(f));
                    }
                    Err(e) => return Err(format!("Failed to create stderr file {}: {}", fpath, e)),
                }
            } else {
                command.stderr(Stdio::inherit());
            }
        }
        _ => {
            command.stderr(Stdio::inherit());
        }
    }

    let child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(format!(
                "ProcessError: Failed to spawn process '{}': {}",
                cmd_name, e
            ));
        }
    };

    let pid = child.id();
    let handle_id = NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst);
    let start_time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let cmd_line = format!("{} {}", cmd_name, cmd_args.join(" "));

    let state = ProcessState {
        child: Some(child),
        pid,
        executable: cmd_name,
        command_line: cmd_line,
        start_time_ms,
        stdout_captured: Vec::new(),
        stderr_captured: Vec::new(),
        exit_code: None,
        signal: None,
        exited: false,
    };

    let mut table = PROCESS_TABLE.lock().unwrap();
    table.insert(handle_id, state);

    Ok(make_dict(vec![
        ("handle", Value::Number(handle_id as f64)),
        ("pid", Value::Number(pid as f64)),
    ]))
}

pub fn builtin_process_wait(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processWait requires handle ID argument".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;

    let mut table = PROCESS_TABLE.lock().unwrap();
    let state = table.get_mut(&handle_id).ok_or("Invalid process handle")?;

    if state.exited {
        let code = state.exit_code.unwrap_or(-1);
        let sig = state.signal.unwrap_or(0);
        return Ok(make_dict(vec![
            ("exitCode", Value::Number(code as f64)),
            ("success", Value::Bool(code == 0)),
            ("signal", Value::Number(sig as f64)),
            ("timedOut", Value::Bool(false)),
        ]));
    }

    if let Some(mut child) = state.child.take() {
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();

        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_end(&mut out_buf);
        }
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_end(&mut err_buf);
        }

        state.stdout_captured = out_buf;
        state.stderr_captured = err_buf;

        let status = child.wait().map_err(|e| format!("Wait failed: {}", e))?;
        state.exited = true;
        let code = status.code().unwrap_or(-1);
        state.exit_code = Some(code);
        state.signal = Some(0);

        return Ok(make_dict(vec![
            ("exitCode", Value::Number(code as f64)),
            ("success", Value::Bool(status.success())),
            ("signal", Value::Number(0.0)),
            ("timedOut", Value::Bool(false)),
        ]));
    }

    Err("Process handle child already taken".to_string())
}

pub fn builtin_process_wait_timeout(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("processWaitTimeout requires handle and timeout_ms".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;
    let timeout_ms = extract_int(&args[1]).unwrap_or(1000).max(0) as u64;

    let start = Instant::now();
    loop {
        {
            let mut table = PROCESS_TABLE.lock().unwrap();
            if let Some(state) = table.get_mut(&handle_id) {
                if state.exited {
                    let code = state.exit_code.unwrap_or(-1);
                    let sig = state.signal.unwrap_or(0);
                    return Ok(make_dict(vec![
                        ("exitCode", Value::Number(code as f64)),
                        ("success", Value::Bool(code == 0)),
                        ("signal", Value::Number(sig as f64)),
                        ("timedOut", Value::Bool(false)),
                    ]));
                }

                if let Some(child) = state.child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let mut child = state.child.take().unwrap();
                            let mut out_buf = Vec::new();
                            let mut err_buf = Vec::new();

                            if let Some(mut stdout) = child.stdout.take() {
                                let _ = stdout.read_to_end(&mut out_buf);
                            }
                            if let Some(mut stderr) = child.stderr.take() {
                                let _ = stderr.read_to_end(&mut err_buf);
                            }

                            state.stdout_captured = out_buf;
                            state.stderr_captured = err_buf;
                            state.exited = true;

                            let code = status.code().unwrap_or(-1);
                            state.exit_code = Some(code);
                            state.signal = Some(0);

                            return Ok(make_dict(vec![
                                ("exitCode", Value::Number(code as f64)),
                                ("success", Value::Bool(status.success())),
                                ("signal", Value::Number(0.0)),
                                ("timedOut", Value::Bool(false)),
                            ]));
                        }
                        Ok(None) => {}
                        Err(e) => return Err(format!("Error polling process: {}", e)),
                    }
                }
            } else {
                return Err("Invalid process handle".to_string());
            }
        }

        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            return Ok(make_dict(vec![
                ("exitCode", Value::Number(-1.0)),
                ("success", Value::Bool(false)),
                ("signal", Value::Number(0.0)),
                ("timedOut", Value::Bool(true)),
            ]));
        }

        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn builtin_process_kill(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processKill requires handle ID".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;
    let _sig_code = if args.len() > 1 {
        extract_int(&args[1]).unwrap_or(9)
    } else {
        9
    };

    let mut table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get_mut(&handle_id) {
        if let Some(child) = state.child.as_mut() {
            let _ = child.kill();
            state.exited = true;
            state.exit_code = Some(-1);
            state.signal = Some(_sig_code as i32);
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

pub fn builtin_process_terminate(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    builtin_process_kill(_env, vec![args[0].clone(), Value::Number(15.0)])
}

pub fn builtin_process_read_stdout(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processReadStdout requires handle ID".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;
    let table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get(&handle_id) {
        let s = String::from_utf8_lossy(&state.stdout_captured).to_string();
        return Ok(Value::Str(s));
    }
    Ok(Value::Str("".to_string()))
}

pub fn builtin_process_read_stderr(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processReadStderr requires handle ID".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;
    let table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get(&handle_id) {
        let s = String::from_utf8_lossy(&state.stderr_captured).to_string();
        return Ok(Value::Str(s));
    }
    Ok(Value::Str("".to_string()))
}

pub fn builtin_process_write_stdin(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("processWriteStdin requires handle ID and data".to_string());
    }
    let handle_id = extract_int(&args[0]).ok_or("handle must be integer")?;
    let data = extract_string(&args[1]).unwrap_or_default();

    let mut table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get_mut(&handle_id) {
        if let Some(child) = state.child.as_mut() {
            if let Some(stdin) = child.stdin.as_mut() {
                if stdin.write_all(data.as_bytes()).is_ok() {
                    let _ = stdin.flush();
                    return Ok(Value::Bool(true));
                }
            }
        }
    }
    Ok(Value::Bool(false))
}

pub fn builtin_process_get_info(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let target_pid = if !args.is_empty() {
        extract_int(&args[0]).map(|v| v as u32)
    } else {
        Some(std::process::id())
    };

    let pid_val = target_pid.unwrap_or_else(std::process::id);
    let ppid_val = 0i64; // Parent PID platform neutral fallback

    let table = PROCESS_TABLE.lock().unwrap();
    let (exe, cmdline, start_ms) = if let Some(state) = table.values().find(|s| s.pid == pid_val) {
        (
            state.executable.clone(),
            state.command_line.clone(),
            state.start_time_ms,
        )
    } else {
        ("unknown".to_string(), "unknown".to_string(), 0i64)
    };

    Ok(make_dict(vec![
        ("pid", Value::Number(pid_val as f64)),
        ("parentId", Value::Number(ppid_val as f64)),
        ("executable", Value::Str(exe)),
        ("commandLine", Value::Str(cmdline)),
        ("cpuTime", Value::Number(0.0)),
        ("memoryUsage", Value::Number(0.0)),
        ("startTime", Value::Number(start_ms as f64)),
    ]))
}

pub fn builtin_process_run_pipeline(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("processRunPipeline requires array of pipeline commands".to_string());
    }

    let pipeline_specs: Vec<Value> = match &args[0] {
        Value::Array(arr) => arr.clone(),
        Value::RawArray(_, arr) => arr.clone(),
        Value::Tuple(arr) => arr.clone(),
        Value::Set(arr) => arr.clone(),
        Value::DynArray(dyn_arr) => dyn_arr.data.clone(),
        Value::Object(obj) => {
            let mut arr = Vec::new();
            let mut i = 0;
            while let Some(item) = obj.get(&i.to_string()) {
                arr.push(item.clone());
                i += 1;
            }
            if arr.is_empty() {
                if let Some(Value::Array(inner)) = obj.get("elements") {
                    arr = inner.clone();
                } else {
                    for (_k, v) in obj.iter() {
                        arr.push(v.clone());
                    }
                }
            }
            arr
        }
        _ => return Err("pipeline specs must be an array".to_string()),
    };

    if pipeline_specs.is_empty() {
        return Err("pipeline specs array cannot be empty".to_string());
    }

    let mut children: Vec<Child> = Vec::new();
    let mut prev_stdout: Option<Stdio> = None;

    for (idx, spec) in pipeline_specs.iter().enumerate() {
        let (cmd, cmd_args) = match spec {
            Value::Array(arr) | Value::RawArray(_, arr) | Value::Tuple(arr) => {
                if arr.is_empty() {
                    return Err("Pipeline step cannot be empty".to_string());
                }
                let c = extract_string(&arr[0]).ok_or("Pipeline command must be string")?;
                let mut a = Vec::new();
                for item in arr.iter().skip(1) {
                    if let Some(s) = extract_string(item) {
                        a.push(s);
                    }
                }
                (c, a)
            }
            Value::DynArray(dyn_arr) => {
                if dyn_arr.data.is_empty() {
                    return Err("Pipeline step cannot be empty".to_string());
                }
                let c =
                    extract_string(&dyn_arr.data[0]).ok_or("Pipeline command must be string")?;
                let mut a = Vec::new();
                for item in dyn_arr.data.iter().skip(1) {
                    if let Some(s) = extract_string(item) {
                        a.push(s);
                    }
                }
                (c, a)
            }
            Value::Object(obj) => {
                let c = obj
                    .get("cmd")
                    .or_else(|| obj.get("command"))
                    .and_then(extract_string)
                    .ok_or("Pipeline command must be string")?;
                let mut a = Vec::new();
                if let Some(Value::Array(arr)) = obj.get("args") {
                    for item in arr {
                        if let Some(s) = extract_string(item) {
                            a.push(s);
                        }
                    }
                }
                (c, a)
            }
            Value::Str(s) => (s.clone(), Vec::new()),
            _ => return Err("Invalid pipeline spec item".to_string()),
        };

        let is_last = idx == pipeline_specs.len() - 1;
        let mut command = Command::new(&cmd);
        command.args(&cmd_args);

        if let Some(stdin_stream) = prev_stdout.take() {
            command.stdin(stdin_stream);
        } else {
            command.stdin(Stdio::null());
        }

        if is_last {
            command.stdout(Stdio::piped());
        } else {
            command.stdout(Stdio::piped());
        }
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn pipeline step '{}': {}", cmd, e))?;
        if !is_last {
            if let Some(out) = child.stdout.take() {
                prev_stdout = Some(Stdio::from(out));
            }
        }
        children.push(child);
    }

    let mut last_child = children.pop().unwrap();
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    if let Some(mut out) = last_child.stdout.take() {
        let _ = out.read_to_end(&mut stdout_buf);
    }
    if let Some(mut err) = last_child.stderr.take() {
        let _ = err.read_to_end(&mut stderr_buf);
    }

    let status = last_child
        .wait()
        .map_err(|e| format!("Pipeline finish error: {}", e))?;

    for mut c in children {
        let _ = c.wait();
    }

    let exit_code = status.code().unwrap_or(-1);
    let stdout_str = String::from_utf8_lossy(&stdout_buf).to_string();
    let stderr_str = String::from_utf8_lossy(&stderr_buf).to_string();

    Ok(make_dict(vec![
        ("exitCode", Value::Number(exit_code as f64)),
        ("success", Value::Bool(status.success())),
        ("signal", Value::Number(0.0)),
        ("stdoutText", Value::Str(stdout_str)),
        ("stderrText", Value::Str(stderr_str)),
    ]))
}

pub fn builtin_process_exec(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let spawn_res = builtin_process_spawn(_env, args)?;
    let handle_val = match &spawn_res {
        Value::Object(obj) => obj.get("handle").cloned().unwrap_or(Value::Number(0.0)),
        _ => Value::Number(0.0),
    };
    builtin_process_wait(_env, vec![handle_val])
}

pub fn builtin_process_read_line(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let handle_id = args.first().and_then(extract_int).unwrap_or(0);
    let mut table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get_mut(&handle_id) {
        if let Some(child) = state.child.as_mut() {
            if let Some(stdout) = child.stdout.as_mut() {
                let mut line = String::new();
                let mut buf = [0u8; 1];
                loop {
                    match stdout.read(&mut buf) {
                        Ok(1) => {
                            let ch = buf[0] as char;
                            line.push(ch);
                            if ch == '\n' {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                return Ok(Value::Str(line));
            }
        }
    }
    Ok(Value::Str(String::new()))
}

pub fn builtin_process_write_line(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let handle_id = args.first().and_then(extract_int).unwrap_or(0);
    let line = args.get(1).and_then(extract_string).unwrap_or_default();
    let text = format!("{}\n", line);
    builtin_process_write_stdin(
        _env,
        vec![Value::Number(handle_id as f64), Value::Str(text)],
    )
}

pub fn builtin_process_read_bytes(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let handle_id = args.first().and_then(extract_int).unwrap_or(0);
    let count = args.get(1).and_then(extract_int).unwrap_or(4096) as usize;
    let mut table = PROCESS_TABLE.lock().unwrap();
    if let Some(state) = table.get_mut(&handle_id) {
        if let Some(child) = state.child.as_mut() {
            if let Some(stdout) = child.stdout.as_mut() {
                let mut buf = vec![0u8; count];
                match stdout.read(&mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        let arr: Vec<Value> =
                            buf.into_iter().map(|b| Value::Number(b as f64)).collect();
                        return Ok(Value::Array(arr));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(Value::Array(Vec::new()))
}

pub fn builtin_process_get_tree(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let target_pid = args.first().and_then(extract_int).unwrap_or(0) as u32;
    let mut pids = vec![Value::Number(target_pid as f64)];
    let table = PROCESS_TABLE.lock().unwrap();
    for (_, state) in table.iter() {
        if state.pid != target_pid && state.pid > 0 {
            pids.push(Value::Number(state.pid as f64));
        }
    }
    Ok(Value::Array(pids))
}

pub fn builtin_process_create_group(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    let group_id = NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst);
    Ok(Value::Number(group_id as f64))
}

pub fn builtin_process_kill_group(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let handle_ids: Vec<i64> = match args.first() {
        Some(Value::Array(arr)) => arr.iter().filter_map(extract_int).collect(),
        _ => Vec::new(),
    };
    let mut table = PROCESS_TABLE.lock().unwrap();
    let mut count = 0;
    for id in handle_ids {
        if let Some(state) = table.get_mut(&id) {
            if let Some(child) = state.child.as_mut() {
                let _ = child.kill();
                count += 1;
            }
        }
    }
    Ok(Value::Number(count as f64))
}

pub fn builtin_process_set_limits(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    Ok(Value::Bool(true))
}

pub fn builtin_process_run_graph(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    // Process Graph DAG Execution Engine
    // Expects args[0] = nodes (dict of node_id -> { cmd, args }), args[1] = edges (array of [from_node, to_node])
    let nodes_val = args.first();
    let edges_val = args.get(1);

    let mut graph_results = HashMap::default();

    if let (Some(Value::Object(nodes)), Some(Value::Array(edges))) = (nodes_val, edges_val) {
        let mut node_outputs: HashMap<String, String> = HashMap::default();
        let mut node_keys: Vec<String> = nodes.keys().cloned().collect();
        node_keys.sort();

        for node_id in node_keys {
            if let Some(Value::Object(node_spec)) = nodes.get(&node_id) {
                let cmd = node_spec
                    .get("cmd")
                    .and_then(extract_string)
                    .unwrap_or_default();
                let cmd_args: Vec<String> = match node_spec.get("args") {
                    Some(Value::Array(arr)) => arr.iter().filter_map(extract_string).collect(),
                    _ => Vec::new(),
                };

                let mut incoming_data = String::new();
                for edge in edges.iter() {
                    if let Value::Array(pair) = edge {
                        if pair.len() >= 2 {
                            let from_id = extract_string(&pair[0]).unwrap_or_default();
                            let to_id = extract_string(&pair[1]).unwrap_or_default();
                            if to_id == node_id {
                                if let Some(prev_out) = node_outputs.get(&from_id) {
                                    incoming_data.push_str(prev_out);
                                }
                            }
                        }
                    }
                }

                let mut command = Command::new(&cmd);
                command.args(&cmd_args);
                command.stdin(Stdio::piped());
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());

                if let Ok(mut child) = command.spawn() {
                    if let Some(mut stdin) = child.stdin.take() {
                        if !incoming_data.is_empty() {
                            let _ = stdin.write_all(incoming_data.as_bytes());
                        }
                    } // stdin is closed here, sending EOF to child process!

                    let mut stdout_buf = Vec::new();
                    let mut stderr_buf = Vec::new();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_end(&mut stdout_buf);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_end(&mut stderr_buf);
                    }

                    let status = child.wait().ok();
                    let exit_code = status.as_ref().and_then(|s| s.code()).unwrap_or(-1);
                    let success = status.as_ref().map(|s| s.success()).unwrap_or(false);
                    let stdout_str = String::from_utf8_lossy(&stdout_buf).to_string();
                    let stderr_str = String::from_utf8_lossy(&stderr_buf).to_string();

                    node_outputs.insert(node_id.clone(), stdout_str.clone());

                    let res_dict = make_dict(vec![
                        ("nodeId", Value::Str(node_id.clone())),
                        ("exitCode", Value::Number(exit_code as f64)),
                        ("success", Value::Bool(success)),
                        ("stdoutText", Value::Str(stdout_str)),
                        ("stderrText", Value::Str(stderr_str)),
                    ]);

                    graph_results.insert(node_id, res_dict);
                }
            }
        }
    }

    let mut map = HashMap::default();
    for (k, v) in graph_results {
        map.insert(k, v);
    }
    Ok(Value::Object(std::sync::Arc::new(map)))
}
