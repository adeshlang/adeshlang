use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::discovery::find_executable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Interpreter,
    Jit,
    NativeJit,
    Bytecode,
    AdaptiveJit,
    TieredJit,
    Mixed,
    Safe,
    Aot,
    Wasm,
    Gpu,
}

impl Backend {
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Interpreter => "Interpreter",
            Backend::Jit => "JIT",
            Backend::NativeJit => "Native JIT",
            Backend::Bytecode => "Bytecode VM",
            Backend::AdaptiveJit => "Adaptive JIT",
            Backend::TieredJit => "Tiered JIT",
            Backend::Mixed => "Mixed",
            Backend::Safe => "Safe",
            Backend::Aot => "AOT",
            Backend::Wasm => "WASM",
            Backend::Gpu => "GPU",
        }
    }

    pub fn flag(&self) -> &'static str {
        match self {
            Backend::Interpreter => "--interpreter",
            Backend::Jit => "--jit",
            Backend::NativeJit => "--native-jit",
            Backend::Bytecode => "--bytecode",
            Backend::AdaptiveJit => "--adaptive",
            Backend::TieredJit => "--tiered",
            Backend::Mixed => "--mixed",
            Backend::Safe => "--safe",
            Backend::Aot => "--aot",
            Backend::Wasm => "--wasm",
            Backend::Gpu => "--gpu",
        }
    }

    pub fn all() -> Vec<Backend> {
        vec![
            Backend::Interpreter,
            Backend::Jit,
            Backend::NativeJit,
            Backend::Bytecode,
            Backend::AdaptiveJit,
            Backend::TieredJit,
            Backend::Mixed,
            Backend::Safe,
            Backend::Aot,
            Backend::Wasm,
            Backend::Gpu,
        ]
    }
}

pub struct ExecutionOutput {
    pub lines: Vec<String>,
    pub is_running: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u128>,
}

impl ExecutionOutput {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            is_running: false,
            exit_code: None,
            duration_ms: None,
        }
    }
}

/// Cleans raw process output:
/// 1. Splits lines by '\n' and handles '\r' (carriage returns from progress spinners).
/// 2. Strips ANSI escape codes (CSI, OSC, 2-byte escape sequences).
/// 3. Filters out raw non-printable ASCII control characters (preserving tab as spaces and standard unicode).
pub fn clean_terminal_output(raw: &str) -> Vec<String> {
    let mut cleaned_lines = Vec::new();

    for chunk in raw.split('\n') {
        let r_parts: Vec<&str> = chunk.split('\r').collect();
        for part in r_parts {
            let mut cleaned = String::with_capacity(part.len());
            let mut chars = part.chars().peekable();

            while let Some(c) = chars.next() {
                if c == '\x1b' {
                    match chars.peek() {
                        Some(&'[') => {
                            chars.next();
                            let mut seq = String::new();
                            while let Some(&nc) = chars.peek() {
                                chars.next();
                                if (nc >= '@' && nc <= '~')
                                    || nc == 'm'
                                    || nc == 'K'
                                    || nc == 'H'
                                    || nc == 'J'
                                {
                                    if nc == 'm' {
                                        cleaned.push_str("\x1b[");
                                        cleaned.push_str(&seq);
                                        cleaned.push('m');
                                    }
                                    break;
                                } else {
                                    seq.push(nc);
                                }
                            }
                        }
                        Some(&']') => {
                            chars.next();
                            while let Some(&nc) = chars.peek() {
                                chars.next();
                                if nc == '\x07' || nc == '\\' {
                                    break;
                                }
                            }
                        }
                        Some(&'(') | Some(&')') => {
                            chars.next();
                            chars.next();
                        }
                        _ => {
                            if let Some(&nc) = chars.peek() {
                                if nc.is_ascii_alphabetic() {
                                    chars.next();
                                }
                            }
                        }
                    }
                } else if c == '\t' {
                    cleaned.push_str("    ");
                } else if !c.is_control() {
                    cleaned.push(c);
                }
            }

            let trimmed = cleaned.trim_end();
            if !trimmed.is_empty() {
                cleaned_lines.push(trimmed.to_string());
            }
        }
    }

    cleaned_lines
}

pub struct Runner {
    pub output: Arc<Mutex<ExecutionOutput>>,
    stop_flag: Arc<AtomicBool>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(ExecutionOutput::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Run an Adesh program. Spawns process asynchronously without blocking the UI.
    pub fn run_program(&self, file_path: &Path, backend: &Backend) -> Result<(), String> {
        self.stop_flag.store(false, Ordering::SeqCst);

        let adesh_bin = find_executable("adesh").ok_or_else(|| {
            "Adesh compiler/runtime not found in PATH or environment.".to_string()
        })?;

        let mut cmd = Command::new(adesh_bin);
        cmd.arg("run").arg(backend.flag()).arg(file_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn adesh process: {}", e))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let output_arc = Arc::clone(&self.output);
        let stop_flag = Arc::clone(&self.stop_flag);
        let backend_name = backend.name().to_string();
        let start_time = Instant::now();

        tokio::spawn(async move {
            {
                if let Ok(mut out) = output_arc.lock() {
                    out.lines.clear();
                    out.lines
                        .push(format!("Compiling & executing with {}...", backend_name));
                    out.is_running = true;
                    out.exit_code = None;
                    out.duration_ms = None;
                }
            }

            if let Some(stdout) = stdout {
                let oc = Arc::clone(&output_arc);
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let cleaned = clean_terminal_output(&line);
                        if let Ok(mut out) = oc.lock() {
                            for cl in cleaned {
                                out.lines.push(cl);
                            }
                        }
                    }
                });
            }

            if let Some(stderr) = stderr {
                let oc = Arc::clone(&output_arc);
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        let cleaned = clean_terminal_output(&line);
                        if let Ok(mut out) = oc.lock() {
                            for cl in cleaned {
                                out.lines.push(format!("[err] {}", cl));
                            }
                        }
                    }
                });
            }

            loop {
                if stop_flag.load(Ordering::SeqCst) {
                    let _ = child.kill().await;
                    if let Ok(mut out) = output_arc.lock() {
                        out.is_running = false;
                        out.lines.push("[Process terminated by user]".to_string());
                    }
                    return;
                }
                match tokio::time::timeout(Duration::from_millis(100), child.wait()).await {
                    Ok(status) => {
                        let duration = start_time.elapsed().as_millis();
                        if let Ok(mut out) = output_arc.lock() {
                            out.is_running = false;
                            out.duration_ms = Some(duration);
                            match status {
                                Ok(s) => {
                                    let code: i32 = s.code().unwrap_or(-1);
                                    out.exit_code = Some(code);
                                    out.lines.push(format!(
                                        "Process exited with code {} ({} ms)",
                                        code, duration
                                    ));
                                }
                                Err(e) => out.lines.push(format!("Process error: {}", e)),
                            }
                        }
                        return;
                    }
                    Err(_) => continue,
                }
            }
        });

        Ok(())
    }

    /// Stop the running program.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

// ── Integrated Terminal ─────────────────────────────────────────────

pub struct Terminal {
    pub output: Arc<Mutex<Vec<String>>>,
    pub input: String,
    pub history: Vec<String>,
    pub history_index: usize,
    pub is_running: bool,
    stop_flag: Arc<AtomicBool>,
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            output: Arc::new(Mutex::new(vec![
                "Adesh Editor — Integrated Terminal".to_string(),
                "Type a command and press Enter. Ctrl+J to close.".to_string(),
            ])),
            input: String::new(),
            history: Vec::new(),
            history_index: 0,
            is_running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn input_char(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    pub fn get_input(&self) -> String {
        self.input.clone()
    }

    pub fn get_history(&self) -> Vec<String> {
        self.output.lock().map(|o| o.clone()).unwrap_or_default()
    }

    pub fn execute_line(&mut self) {
        let cmd = self.input.clone();
        self.input.clear();
        self.execute(&cmd);
    }

    /// Execute a shell command. Spawns process and collects output async.
    pub fn execute(&mut self, command: &str) {
        let cmd = command.trim();
        if cmd.is_empty() {
            return;
        }

        self.history.push(cmd.to_string());
        self.history_index = self.history.len();

        match cmd {
            "clear" | "cls" => {
                let output = self.output.clone();
                if let Ok(mut o) = output.lock() {
                    o.clear();
                    o.push("Adesh Editor — Integrated Terminal".to_string());
                }
                return;
            }
            "help" => {
                let output = self.output.clone();
                let cmd_owned = cmd.to_string();
                let help_lines = vec![
                    "Built-in commands:".to_string(),
                    "  clear / cls  — Clear terminal".to_string(),
                    "  help        — Show this help".to_string(),
                    "  exit        — Close terminal (or Ctrl+J)".to_string(),
                    "Any other command is run via the system shell.".to_string(),
                ];
                if let Ok(mut o) = output.lock() {
                    o.push(format!("> {}", cmd_owned));
                    o.extend(help_lines);
                }
                return;
            }
            _ => {}
        }

        self.is_running = true;
        self.stop_flag.store(false, Ordering::SeqCst);

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut command = Command::new(shell);
        command.arg(flag).arg(cmd);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        match command.spawn() {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let output_arc = Arc::clone(&self.output);
                let stop_flag = Arc::clone(&self.stop_flag);
                let cmd_display = cmd.to_string();

                tokio::spawn(async move {
                    {
                        if let Ok(mut o) = output_arc.lock() {
                            o.push(format!("> {}", cmd_display));
                        }
                    }

                    if let Some(stdout) = stdout {
                        let oc = Arc::clone(&output_arc);
                        tokio::spawn(async move {
                            let mut reader = BufReader::new(stdout).lines();
                            while let Ok(Some(line)) = reader.next_line().await {
                                let cleaned = clean_terminal_output(&line);
                                if let Ok(mut o) = oc.lock() {
                                    for cl in cleaned {
                                        o.push(cl);
                                    }
                                }
                            }
                        });
                    }

                    if let Some(stderr) = stderr {
                        let oc = Arc::clone(&output_arc);
                        tokio::spawn(async move {
                            let mut reader = BufReader::new(stderr).lines();
                            while let Ok(Some(line)) = reader.next_line().await {
                                let cleaned = clean_terminal_output(&line);
                                if let Ok(mut o) = oc.lock() {
                                    for cl in cleaned {
                                        o.push(format!("[err] {}", cl));
                                    }
                                }
                            }
                        });
                    }

                    loop {
                        if stop_flag.load(Ordering::SeqCst) {
                            let _ = child.kill().await;
                            if let Ok(mut o) = output_arc.lock() {
                                o.push("[terminated]".to_string());
                            }
                            break;
                        }
                        match tokio::time::timeout(Duration::from_millis(100), child.wait()).await {
                            Ok(Ok(status)) => {
                                let code: i32 = status.code().unwrap_or(-1);
                                if let Ok(mut o) = output_arc.lock() {
                                    o.push(format!("[exit: {}]", code));
                                }
                                break;
                            }
                            Ok(Err(e)) => {
                                if let Ok(mut o) = output_arc.lock() {
                                    o.push(format!("[error: {}]", e));
                                }
                                break;
                            }
                            Err(_) => continue,
                        }
                    }
                });
            }
            Err(e) => {
                let output = self.output.clone();
                let cmd_owned = cmd.to_string();
                let msg = format!("Error: {}", e);
                if let Ok(mut o) = output.lock() {
                    o.push(format!("> {}", cmd_owned));
                    o.push(msg);
                }
            }
        }

        self.is_running = false;
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.is_running = false;
    }

    pub fn prev_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_index > 0 {
            self.history_index -= 1;
            self.input = self.history[self.history_index].clone();
        }
    }

    pub fn next_history(&mut self) {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            self.input = self.history[self.history_index].clone();
        } else {
            self.history_index = self.history.len();
            self.input.clear();
        }
    }
}
