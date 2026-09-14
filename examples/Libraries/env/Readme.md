# AdeshLang Env Standard Library Documentation

## Overview

The `Env` library provides object-oriented access to environment variables, system directories, platform information, command-line arguments, process metadata, and standard separators in AdeshLang.

It integrates seamlessly with AdeshLang's `Path` and `FS` standard libraries, returning `Path` objects for directory and executable queries to enable fluent path chaining.

```adesh
import Env;
import FS;

let home = Env.home();
let config = Env.home().join(".adesh").join("config.toml");

if Env.contains("JAVA_HOME") {
    print("Java installed: " + Env.get("JAVA_HOME"));
}

print("OS: " + Env.os());
print("Architecture: " + Env.architecture());
print("Args: " + str(Env.arguments()));
```

---

## API Reference

### Environment Variables

- `Env.get(key: string): string | null`  
  Retrieves the value of environment variable `key`, returning `null` if not set.
- `Env.getOrDefault(key: string, default = null): string`  
  Retrieves `key`, returning `default` when it is not set.
- `Env.set(key: string, value: string): void`  
  Sets or updates the environment variable `key` to `value`.
- `Env.setIfAbsent(key: string, value: string): boolean`  
  Sets `key` only if it is not already present; returns `true` when set, `false` otherwise.
- `Env.remove(key: string): void`  
  Removes the environment variable `key`.
- `Env.unset(key: string): void`  
  Alias of `Env.remove`.
- `Env.contains(key: string): boolean`  
  Returns `true` if `key` exists in the process or runtime environment.
- `Env.has(key: string): boolean`  
  Alias of `Env.contains`.
- `Env.exists(key: string): boolean`  
  Alias of `Env.contains`.
- `Env.clear(): void`  
  Clears all runtime-set environment variables.
- `Env.variables(): object`  
  Returns a dictionary/object of all environment key-value pairs.
- `Env.environ(): object`  
  Alias of `Env.variables`.
- `Env.toObject(): object`  
  Alias of `Env.variables`.
- `Env.keys(): array`  
  Returns an array of all environment variable names.
- `Env.values(): array`  
  Returns an array of all environment variable values.
- `Env.count(): int`  
  Returns the number of environment variables currently visible.
- `Env.getMany(keys: array): object`  
  Returns an object containing only the requested keys that exist.
- `Env.filter(prefix: string): object`  
  Returns an object of all variables whose key starts with `prefix`.
- `Env.filterKeys(prefix: string): array`  
  Returns an array of all variable names starting with `prefix`.

### .env File Support

- `Env.fromFile(path = ".env"): object`  
  Reads a `.env` file and returns its key-value pairs as an object.
- `Env.getFromFile(key: string, default = null, path = ".env"): string`  
  Reads a single key from a `.env` file.
- `Env.load(path = ".env", overwrite = false): int`  
  Loads variables from a `.env` file into the runtime environment.
- `Env.loadFromFile(path = ".env", overwrite = false): int`  
  Alias of `Env.load`.
- `Env.runtimeVariables(): object`  
  Returns all variables stored in the runtime environment.
- `Env.runtimeGet(key: string): string`  
  Gets a variable from the runtime environment.
- `Env.runtimeHas(key: string): boolean`  
  Checks a variable in the runtime environment.
- `Env.runtimeLoad(objectOrPath, overwrite = false): int`  
  Loads variables from an object or a file into the runtime environment.

---

### Standard Directories (Returning `Path` Instances)

- `Env.currentDirectory(): Path`  
  Returns the current working directory as a `Path` instance.
- `Env.setCurrentDirectory(path: string | Path): void`  
  Changes the current working directory to `path`.
- `Env.home(): Path`  
  Returns the user's home directory.
- `Env.desktop(): Path`  
  Returns the user's Desktop directory.
- `Env.documents(): Path`  
  Returns the user's Documents directory.
- `Env.downloads(): Path`  
  Returns the user's Downloads directory.
- `Env.pictures(): Path`  
  Returns the user's Pictures directory.
- `Env.music(): Path`  
  Returns the user's Music directory.
- `Env.videos(): Path`  
  Returns the user's Videos directory.
- `Env.public(): Path`  
  Returns the public shared directory.
- `Env.tempDirectory(): Path`  
  Returns the system's temporary directory.
- `Env.userCacheDirectory(): Path`  
  Returns the user's cache directory (e.g. `AppData/Local` on Windows).
- `Env.userConfigDirectory(): Path`  
  Returns the user's config directory (e.g. `AppData/Roaming` on Windows).

---

### Executable

- `Env.executable(): Path`  
  Returns the absolute path to the running executable script or binary.
- `Env.executableDirectory(): Path`  
  Returns the parent directory containing the running executable script or binary.
- `Env.executableName(): string`  
  Returns just the file name of the running executable script or binary.

---

### Platform & Metadata

- `Env.os(): string`  
  Returns operating system identifier (`"windows"`, `"linux"`, `"macos"`, `"wasm"`).
- `Env.architecture(): string`  
  Returns CPU architecture (`"x86_64"`, `"aarch64"`, `"x86"`, `"wasm32"`).
- `Env.arch(): string`  
  Alias of `Env.architecture`.
- `Env.platform(): string`  
  Returns platform target name (`"windows"`, `"linux"`, `"darwin"`, `"wasm"`).
- `Env.hostname(): string`  
  Returns system hostname.
- `Env.username(): string`  
  Returns current system username.
- `Env.userId(): string`  
  Returns current user ID.
- `Env.processId(): int`  
  Returns process ID (PID).
- `Env.shell(): string`  
  Returns system shell executable path.
- `Env.osType(): string`  
  Returns the OS name as reported by the system (e.g. `"Windows_NT"`).
- `Env.osVersion(): string`  
  Returns the OS version string.
- `Env.osFamily(): string`  
  Returns the OS family identifier.
- `Env.osRelease(): string`  
  Returns the OS release version.
- `Env.osDescription(): string`  
  Returns a human-readable OS description.
- `Env.cpuCount(): int`  
  Returns the number of logical CPUs.
- `Env.totalMemory(): int`  
  Returns total system memory in bytes.
- `Env.freeMemory(): int`  
  Returns free system memory in bytes.
- `Env.usedMemory(): int`  
  Returns used system memory in bytes.
- `Env.memoryUsage(): object`  
  Returns `{ total, free, used, percent }` describing memory usage.
- `Env.uptime(): int`  
  Returns the current process uptime in seconds.
- `Env.systemUptime(): int`  
  Returns the system uptime in seconds.
- `Env.loadAverage(): array`  
  Returns the load average tuple (1, 5, 15 minute averages; `0`s where unavailable).
- `Env.endianness(): string`  
  Returns `"LE"` or `"BE"`.
- `Env.parentProcessId(): int`  
  Returns the parent process ID.
- `Env.processTitle(): string`  
  Returns the process title / current argv.
- `Env.pagesize(): int`  
  Returns the system page size in bytes.
- `Env.machineId(): string`  
  Returns a stable machine identifier.
- `Env.userInfo(): object`  
  Returns `{ username, userId, userGid, home, shell }`.
- `Env.systemInfo(): object`  
  Returns a comprehensive object with all system metadata (see below).

### `Env.systemInfo()` Shape

```adesh
let info = Env.systemInfo();
print(info.os);            // "windows"
print(info.osType);        // "Windows_NT"
print(info.osVersion);     // "6.2.9200"
print(info.osFamily);      // "windows"
print(info.osRelease);     // "6.2.9200"
print(info.osDescription); // "Windows_NT 6.2.9200"
print(info.arch);          // "x86_64"
print(info.platform);      // "windows"
print(info.hostname);
print(info.username);
print(info.userId);
print(info.userGid);
print(info.pid);
print(info.ppid);
print(info.shell);
print(info.cpuCount);
print(info.totalMemory);   // bytes
print(info.freeMemory);    // bytes
print(info.processUptime); // seconds
print(info.systemUptime);  // seconds
print(info.loadavg);       // [1m, 5m, 15m]
print(info.endianness);    // "LE" | "BE"
print(info.pagesize);      // bytes
print(info.processTitle);
print(info.machineId);
print(info.userHome);
print(info.userCache);
print(info.userConfig);
print(info.tempDir);
print(info.currentDir);
print(info.currentExe);
```

---

### Platform Predicates

- `Env.isWindows(): boolean`
- `Env.isLinux(): boolean`
- `Env.isMacOS(): boolean`
- `Env.isWasm(): boolean`
- `Env.isUnix(): boolean`  
  Returns `true` on Linux/macOS (any non-Windows, non-WASM platform).

---

### Runtime & Separators

- `Env.languageVersion(): string`
- `Env.compilerVersion(): string`
- `Env.runtimeVersion(): string`
- `Env.version(): object`  
  Returns `{ language, compiler, runtime }`.
- `Env.pathSeparator(): string` (`";"` on Windows, `":"` on Unix)
- `Env.pathListSeparator(): string`  
  Alias of `Env.pathSeparator`.
- `Env.fileSeparator(): string` (`"\\"` on Windows, `"/"` on Unix)
- `Env.lineSeparator(): string` (`"\r\n"` on Windows, `"\n"` on Unix)
- `Env.eol(): string`  
  Alias of `Env.lineSeparator`.

---

### Command-Line Arguments

- `Env.arguments(): array`  
  Returns array of command-line arguments (excluding the executable).
- `Env.args(): array`  
  Alias of `Env.arguments`.
- `Env.argumentCount(): int`  
  Returns the count of command-line arguments (excluding the executable).
- `Env.arg(index: int)`  
  Returns the 0-based command-line argument at `index` (first user argument is `arg(0)`); returns `null` when out of bounds.
- `Env.argc(): int`  
  Alias of `Env.argumentCount` (bare builtin parity).
- `Env.argv(): array`  
  Alias of `Env.arguments` (bare builtin parity).
- `Env.argsCount(): int`  
  Alias of `Env.argumentCount` (bare builtin parity).
- `Env.execName(): string`  
  Alias of `Env.executableName` (bare builtin parity).
- `Env.argsSlice(start: int): array`  
  Returns arguments from `start` onward.
- `Env.argsJoin(sep = " "): string`  
  Joins arguments with `sep`.
- `Env.argsIndexOf(value: string): int`  
  Returns the 0-based index of the first argument equal to `value`, or `-1`.
- `Env.parseArgs(): object`  
  Parses `--flag` / `--key=value` style arguments into an object.
- `Env.argGet(name: string, default = null)`  
  Returns the parsed value for flag `name`.
- `Env.argHas(name: string): boolean`  
  Checks whether flag `name` is present.
- `Env.env(key: string, default = null)`  
  Returns an environment variable (runtime-loaded values first, then OS); returns `default` when unset (bare builtin parity).
- `Env.envFromFile(path = ".env"): object`  
  Loads a `.env` file into an object (bare builtin parity; see `Env.fromFile`).
- `Env.envFileGet(key, default = null, path = ".env")`  
  Gets a single key from a `.env` file (bare builtin parity; see `Env.getFromFile`).
- `Env.envRuntimeLoad(objectOrPath, overwrite = false): int`  
  Loads variables into the runtime env (bare builtin parity; see `Env.runtimeLoad`).
- `Env.envRuntimeGet(key)` / `Env.envRuntimeHas(key)` / `Env.envRuntimeAll()`  
  Runtime-env access (bare builtin parity; see `Env.runtimeGet` / `Env.runtimeHas` / `Env.runtimeVariables`).

---

## Path Integration

All directory methods on `Env` return instances of `Path`, supporting fluent method chaining with `Path` and `FS`:

```adesh
let configFile = Env.home()
    .join(".config")
    .join("myapp")
    .join("settings.json");

print(configFile.toString());
```

---

## Backend Parity & Ownership Safety

`Env` provides identical semantics across all 6 AdeshLang backends:
1. Interpreter
2. Bytecode VM
3. JIT
4. Native JIT
5. Cranelift/LLVM AOT
6. WASM

Memory safety and deterministic borrowing rules are guaranteed by shared runtime layer intrinsics with RAII memory management.
