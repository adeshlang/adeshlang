# Command-Line Arguments and Environment Variables

This directory contains examples demonstrating how to work with command-line arguments and environment variables in AdeshLang.

The canonical way to access these features is through the `Env` standard library:

```adesh
import Env;
```

The bare builtin functions (`argc`, `argv`, `arg`, `env`, ...) still exist for backward compatibility, but direct use is discouraged. Prefer `Env.*`.

## Table of Contents

- [Argument Functions](#argument-functions)
- [Environment Variable Functions](#environment-variable-functions)
- [Examples](#examples)
- [Usage](#usage)

---

## Argument Functions

All of the following are exposed on the `Env` class. Command-line arguments exclude the executable/script name; use `Env.execName()` to get the running program name.

### `Env.argc() -> Int`

Returns the number of command-line arguments (excluding the executable name).

```adesh
let count = Env.argc();
print("Total arguments:", count);
```

**Example Output:**
```
$ adesh run script.adesh arg1 arg2
Total arguments: 2
```

---

### `Env.argv() -> Array<String>`

Returns an array of all command-line arguments (excluding the executable name).

```adesh
let args = Env.argv();
print("All arguments:", args);
```

**Example Output:**
```
$ adesh run script.adesh hello world
All arguments: ["hello", "world"]
```

---

### `Env.arg(index: Int) -> String`

Returns the 0-based command-line argument at `index` (`arg(0)` is the first user argument). Returns `null` when the index is out of bounds.

```adesh
let firstArg = Env.arg(0);
print("First argument:", firstArg);
```

**Example Output:**
```
$ adesh run script.adesh test
First argument: test
```

---

### `Env.execName() -> String`

Returns the executable/script name (also available as `Env.executableName()`).

```adesh
print("Running:", Env.execName());
```

---

### `Env.argsCount() -> Int`

Alias for `Env.argc()`.

---

### `Env.argsSlice(start: Int) -> Array<String>`

Returns a slice of arguments starting from the specified index.

```adesh
let rest = Env.argsSlice(1);
print("Arguments from index 1:", rest);
```

---

### `Env.argsJoin(separator: String) -> String`

Joins all command-line arguments into a single string using the specified separator.

```adesh
let commandLine = Env.argsJoin(" ");
print("Full command:", commandLine);
```

---

### `Env.argHas(name: String) -> Bool`

Checks whether a flag is present in the command-line arguments (supports `--name`, `-n`, and grouped short flags like `-abc`).

```adesh
if (Env.argHas("verbose")) {
    print("Verbose mode enabled");
}
```

---

### `Env.argGet(name: String, default = null)`

Retrieves the value associated with a key. Supports both `--key=value` and `--key value` formats.

```adesh
let name = Env.argGet("name", "Anonymous");
let port = Env.argGet("port", "8080");
print(`Name: ${name}, Port: ${port}`);
```

**Example Output:**
```
$ adesh run script.adesh --name=Alice --port 3000
Name: Alice, Port: 3000
```

---

### `Env.argsIndexOf(value: String) -> Int`

Returns the index of the first occurrence of the specified value in the arguments, or `-1` if not found.

```adesh
let pos = Env.argsIndexOf("--config");
if (pos >= 0) {
    print(`Config flag found at position ${pos}`);
}
```

---

### `Env.parseArgs() -> Object`

Parses command-line arguments and returns an object with `flags` and `positionals`.

```adesh
let parsed = Env.parseArgs();
print(parsed);
```

**Example Output:**
```
$ adesh run script.adesh --port=8080 file1 file2
{positionals: ["file1", "file2"], flags: {port: "8080"}}
```

---

## Environment Variable Functions

### `Env.env(key: String, default = null)`

Retrieves the value of an environment variable. Runtime-loaded variables are checked first, then the OS environment. Returns `default` when the variable doesn't exist.

```adesh
let path = Env.env("PATH");
let home = Env.env("HOME", "unknown");
print(`PATH: ${path}`);
print(`HOME: ${home}`);
```

---

### `Env.envFromFile(path = ".env") -> Object`

Loads environment variables from a `.env` file and returns them as an object. Does not modify the runtime environment.

```adesh
let dotenv = Env.envFromFile("examples/args/.env");
print("Database host:", dotenv.DB_HOST);
print("API key:", dotenv.API_KEY);
```

---

### `Env.envFileGet(key, default = null, path = ".env")`

Retrieves a specific value from a `.env` file without loading all variables.

```adesh
let apiKey = Env.envFileGet("API_KEY", "default_key", ".env");
print(`API Key: ${apiKey}`);
```

---

### `Env.envRuntimeLoad(objectOrPath, overwrite = false) -> Int`

Loads variables (from an object or a `.env` path) into the runtime environment, making them available via `Env.env()`.

```adesh
let dotenv = Env.envFromFile(".env");
Env.envRuntimeLoad(dotenv);
let dbHost = Env.env("DB_HOST");
print("Database:", dbHost);
```

---

### `Env.envRuntimeGet(key)` / `Env.envRuntimeHas(key)` / `Env.envRuntimeAll()`

Direct runtime-environment access (runtime-loaded values only, no OS fallback).

---

## Execution Modes

### Interpreter Backend - ✅ Fully Supported
All `Env` argument/environment methods work via registered builtins.

**Run:**
```bash
adesh run examples/args/args_parse.adesh -- --port=8080 --host localhost -v file1
```

### AOT / Bytecode VM / JIT - ✅ Fully Supported
The same `Env` methods map to the common backend builtins (`argc`, `argv`, `arg`, `args`, `execName`, `argsCount`, `argsSlice`, `argsJoin`, `argsIndexOf`, `parseArgs`, `argGet`, `argHas`, `env`, `envGet`, `envFromFile`, `envFileGet`, `envRuntimeLoad`, `envRuntimeGet`, `envRuntimeHas`, `envRuntimeAll`).

> Note: AOT/bytecode backends historically treat `arg(0)` as the executable path and include it in `argv()`/`argc()`. If cross-backend parity matters, prefer `Env.execName()` for the program name and treat `Env.arg(0)` as the first user argument only in the interpreter backend.

---

## Examples

### args_basic.adesh
Basic argument access using `Env.argc()`, `Env.argv()`, `Env.arg()`, and `.env` loading.

### args_parse.adesh
Demonstrates all argument parsing functions with structured parsing.

### args_simple.adesh
Minimal argument access for bytecode testing.

### test_short_flags.adesh
Short-flag (`-abc`) parsing with `Env.argHas()`.

### runtime_env_test.adesh / simple_runtime_test.adesh / test_env*.adesh
Environment-variable access and runtime `.env` loading.

---

## Function Summary Table

| Function | Return Type | Description |
|----------|------------|-------------|
| `Env.argc()` | `Int` | Number of arguments (excl. executable) |
| `Env.argv()` | `Array<String>` | All arguments as array (excl. executable) |
| `Env.arg(index)` | `String` | 0-based argument access |
| `Env.execName()` | `String` | Running program name |
| `Env.argsCount()` | `Int` | Alias for `Env.argc()` |
| `Env.argsSlice(start)` | `Array<String>` | Arguments from start index |
| `Env.argsJoin(sep)` | `String` | Join all arguments with separator |
| `Env.argHas(name)` | `Bool` | Check if flag exists |
| `Env.argGet(name, default)` | `String` | Get key's value or default |
| `Env.argsIndexOf(value)` | `Int` | Find index of value |
| `Env.parseArgs()` | `Object` | Parse arguments into flags/positionals |
| `Env.env(key, default)` | `String` | Get environment variable |
| `Env.envFromFile(path)` | `Object` | Load .env file to object |
| `Env.envFileGet(key, default, path)` | `String` | Get value from .env file |
| `Env.envRuntimeLoad(obj)` | `Int` | Load object/path into runtime env |

---

## Notes

- **Executable name**: `argc()`/`argv()`/`arg()` exclude the executable; use `Env.execName()` to get it.
- **User Arguments**: Use `Env.arg(0)` / `Env.argsSlice(0)` to access user-provided arguments.
- **Environment Variables**: `Env.env()` checks runtime-loaded variables first, then falls back to OS environment.
- **`.env` Files**: Support comments (`#`), quoted values, and `key=value` format.
- **Template Literals**: Work correctly with `Env.env()` in all execution modes.
- **Cross-Platform**: All functions work consistently across Windows, Linux, and macOS.
- **All Execution Modes**: All `Env` methods work in interpreter, bytecode VM, JIT, and AOT-compiled modes.

---

## See Also

- [Env Library Documentation](../Libraries/env/Readme.md)
- [Language Documentation](../../docs/IndiaLang_Documentation.md)
- [Examples Directory](../README.md)
- [CLI Documentation](../../docs/cli.md)
