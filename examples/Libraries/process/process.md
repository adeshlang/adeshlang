# AdeshLang Process Library Specification & Documentation

## Overview

The `Process` library in AdeshLang provides a production-grade, ownership-safe, zero-leak, cross-platform API for spawning, managing, piping, and inspecting OS processes. It draws inspiration from Rust (`std::process`), Go (`os/exec`), Python (`subprocess`), Java (`ProcessBuilder`), .NET (`System.Diagnostics.Process`), Node.js (`child_process`), and Zig (`std.process`).

---

## Architecture & Integration

`Process` integrates seamlessly with other AdeshLang standard library components:
- **`Env`**: Passes, clears, or inherits environment variables (`environment()`, `inheritEnvironment()`, `clearEnvironment()`).
- **`Path`**: Configures execution working directory (`workingDirectory()`) and file stream redirections (`redirectToFile()`, `redirectFromFile()`).
- **`IO`**: Manages stdio stream pipes (`stdinPipe()`, `stdoutPipe()`, `stderrPipe()`, `inheritStdout()`).
- **`Time`**: Controls process timeouts (`runTimeout(Duration)`, `waitTimeout(Duration)`).

---

## Backend Parity Matrix

| Feature / Backend | Interpreter | Bytecode VM | JIT | Native JIT | LLVM AOT | WASM |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `Process.run()` | ✅ Canonical | ✅ | ✅ | ✅ | ✅ | ⚠️ Graceful Fallback |
| `Process.spawn()` | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ Graceful Fallback |
| `ProcessBuilder` | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ Graceful Fallback |
| Stdio Pipes | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| File Redirection | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| Pipelines | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |
| Timeouts | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ |

---

## Public API Reference

### Public Classes & Types

- **`Process`**: Static facade for process management.
- **`ProcessBuilder`**: Fluent builder for process configuration.
- **`ChildProcess`**: Handle to a running/spawned child process.
- **`ExitStatus`**: Code, success flag, and signal of a finished process.
- **`ProcessError`**: Exception type raised on failure.
- **`Signal`**: Signal abstraction (`Signal.TERM()`, `Signal.KILL()`, etc.).
- **`ProcessHandle`**: Opaque ID and PID wrapper.
- **`ProcessInfo`**: Metrics including PID, parent ID, executable, command line, and start time.
- **`CommandResult`**: Captured stdout/stderr text & bytes along with exit status.
- **`Stdio`**: Stdio stream configuration modes (`INHERIT`, `PIPE`, `NULL`, `FILE`).

---

## Ergonomic Code Examples

### 1. Basic Execution
```adesh
import Process;

let result = Process.run("git", ["status"])?;
if result.success() {
    print(result.stdoutText());
}
```

### 2. Builder API & Timeouts
```adesh
import Process;
import Env;
import Time;

let result = Process.builder("git")
    .args(["status"])
    .workingDirectory(Env.currentDirectory())
    .environment("MODE", "dev")
    .stdoutPipe()
    .runTimeout(Duration.seconds(5))?;

print(result.stdoutText());
```

### 3. Background Spawning
```adesh
import Process;

let child = Process.builder("python")
    .arg("server.py")
    .stdoutPipe()
    .spawn()?;

print("Running with PID: " + str(child.processId()));
let status = child.wait()?;
```

### 4. Process Pipelines
```adesh
import Process;

let result = Process.pipeline([
    ["git", "log", "-n", "10"],
    ["grep", "feat"]
])?;

print(result.stdoutText());
```

---

## Security & Ownership Guarantees

1. **Shell-Injection Resistance**: `Process.run()` and `ProcessBuilder` execute binaries directly via OS process creation APIs (`CreateProcessW` / `posix_spawnp`) without invoking shell parsers (`sh`, `cmd.exe`) unless explicitly requested via `Process.shell()`.
2. **RAII & Zombie Cleanup**: Native handles and stdio pipes are cleaned up automatically upon process completion or garbage collection.
3. **Handle Reuse Prevention**: Opaque handle IDs prevent race conditions or invalid handle access.
