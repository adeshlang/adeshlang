# cli-guide.md

> Consolidated from 4 documentation files on 2026-08-29.

---


---

## Source: cli.md

# AdeshLang CLI Documentation

Complete reference for the AdeshLang command-line interface.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Basic Usage](#basic-usage)
- [Commands](#commands)
  - [run](#run)
  - [repl](#repl)
  - [format / fmt](#format--fmt)
  - [compile](#compile)
  - [compile-wasm](#compile-wasm)
  - [compile-native](#compile-native)
  - [compile-native-rust](#compile-native-rust)
  - [compile-wasm-js](#compile-wasm-js)
  - [disassemble](#disassemble)
  - [docs](#docs)
  - [init](#init)
  - [Legacy Commands](#legacy-commands)
- [Execution Backends](#execution-backends)
- [Options](#options)
  - [Optimization Levels](#optimization-levels)
  - [Memory & Garbage Collection](#memory--garbage-collection)
  - [IR Debugging](#ir-debugging)
  - [IO Options](#io-options)
  - [Other Options](#other-options)
- [Source Directives](#source-directives)
- [Program Arguments](#program-arguments)
- [Examples](#examples)
- [Configuration](#configuration)

---

## Overview

AdeshLang (also known as `adesh`) is a statically-typed, high-performance interpreted language with multiple execution backends including JIT compilation, bytecode VM, and direct interpretation.

**Binary Name:** `adesh` (or `adeshlang`)

**Basic Syntax:**
```bash
adesh <COMMAND> [OPTIONS] <FILE> [-- <ARGS>...]
```

---

## Installation

After building the project with Cargo:

```bash
cargo build --release
```

The binary will be located at `target/release/adeshlang` (or `adeshlang.exe` on Wadeshows).

Add it to your PATH or create an alias:
```bash
Set-Alias -Name adesh -Value "target/release/adeshland.exe"
```

---

## Basic Usage

Run a AdeshLang program:
```bash
adesh run program.adesh
```

Start an interactive REPL:
```bash
adesh repl
```

Show help:
```bash
adesh --help
adesh -h
```

Show version:
```bash
adesh --version
adesh -v
```

---

## Commands

### run

Execute a AdeshLang program using the specified backend (default: interpreter).

**Syntax:**
```bash
adesh run [OPTIONS] <file.adesh> [-- <program-args>...]
```

**Examples:**
```bash
# Run with default interpreter
adesh run program.adesh

# Run with JIT compilation
adesh run --jit program.adesh

# Run with maximum optimization
adesh run --jit -O3 program.adesh

# Run with timing information
adesh run --profile program.adesh

# Run and pass arguments to the program
adesh run program.adesh -- arg1 arg2 arg3

# Run in safe mode
adesh run --safe program.adesh

# Run with bytecode VM
adesh run --bytecode program.adesh
```

**Backend Auto-Detection:**
The `run` command can automatically detect the execution backend from `@compile` directives in the source file (see [Source Directives](#source-directives)).

---

### repl

Start an interactive Read-Eval-Print Loop for experimenting with AdeshLang code.

**Syntax:**
```bash
adesh repl [OPTIONS]
```

**Examples:**
```bash
# Start basic REPL
adesh repl

# Start REPL with verbose output
adesh repl --verbose

# Start REPL with JIT backend
adesh repl --jit
```

**REPL Features:**
- Multi-line input support
- Variable persistence across evaluations
- Module imports
- Function definitions
- Class definitions
- Error handling with stack traces

---

### format / fmt

Format AdeshLang source code with consistent styling. The `fmt` command is an alias for `format`.

**Syntax:**
```bash
adesh format [OPTIONS] <file.adesh>
adesh fmt [OPTIONS] <file.adesh>
```

**Options:**

| Option | Description |
|--------|-------------|
| `--write`, `-w` | Write formatted output to file (instead of stdout) |
| `--check` | Check if file is already formatted (exit 1 if not) |
| `--indent <n>` | Set indentation size (default: 4 spaces) |
| `--tabs` | Use tabs instead of spaces for indentation |

**Examples:**
```bash
# Print formatted code to stdout
adesh format program.adesh
adesh fmt program.adesh

# Format file in place
adesh format --write program.adesh
adesh fmt -w program.adesh

# Check if file is formatted (useful in CI)
adesh format --check program.adesh

# Format with custom indentation
adesh format --indent 2 program.adesh

# Format using tabs
adesh format --tabs program.adesh
```

**Behavior:**
- Without `--write`: Prints formatted code to stdout
- With `--write`: Modifies the file in place
- With `--check`: Returns exit code 0 if formatted, 1 if needs formatting

**Use Cases:**
- Pre-commit hooks to ensure consistent code style
- CI/CD pipelines to validate code formatting
- IDE integration for format-on-save

---

### compile

Compile a AdeshLang program to bytecode format (`.adeshbc` file).

**Syntax:**
```bash
adesh compile <input.adesh> <output.adeshbc>
```

**Examples:**
```bash
# Compile to bytecode
adesh compile program.adesh program.adeshbc

# Compile with type checking
adesh compile --verbose program.adesh output.adeshbc
```

**Process:**
1. Parses the source file
2. Runs type checking
3. Compiles to bytecode
4. Writes to output file

**Note:** Bytecode files can be executed using the `run-bc` command or `adesh run --bytecode`.

---

### compile-wasm

Compile a AdeshLang program to WebAssembly (`.wasm` file).

**Syntax:**
```bash
adesh compile-wasm <input.adesh> <output.wasm>
```

**Examples:**
```bash
# Compile to WebAssembly
adesh compile-wasm program.adesh program.wasm

# Compile with verbose output
adesh compile-wasm --verbose program.adesh output.wasm
```

**Requirements:**
- Source must be compatible with WebAssembly limitations
- Type checking is enforced

---

### compile-native

Compile a AdeshLang program to native machine code.

**Syntax:**
```bash
adesh compile-native <input.adesh> <output>
```

**Examples:**
```bash
# Compile to native executable
adesh compile-native program.adesh program

# On Wadeshows
adesh compile-native program.adesh program.exe
```

**Note:** This uses LLVM backend for native code generation.

---

### compile-native-rust

Compile a AdeshLang program to Rust source code, which can then be compiled with `rustc`.

**Syntax:**
```bash
adesh compile-native-rust <input.adesh> <output.rs>
```

**Examples:**
```bash
# Transpile to Rust
adesh compile-native-rust program.adesh program.rs

# Compile the generated Rust code
rustc program.rs -o program
```

**Use Cases:**
- Interop with Rust ecosystems
- Performance optimization
- Debugging with Rust tools

---

### compile-wasm-js

Compile a AdeshLang program to WebAssembly with a JavaScript loader.

**Syntax:**
```bash
adesh compile-wasm-js <input.adesh> <output-directory>
```

**Examples:**
```bash
# Compile to WASM + JS
adesh compile-wasm-js program.adesh ./output

# This creates:
# - output/program.wasm
# - output/program.js (loader)
```

**Output:**
- `.wasm` file: compiled WebAssembly module
- `.js` file: JavaScript loader/wrapper

**Usage in Browser:**
```html
<script src="output/program.js"></script>
```

---

### disassemble

Disassemble a compiled bytecode file to readable format.

**Syntax:**
```bash
adesh disassemble <bytecode-file.adeshbc>
```

**Examples:**
```bash
# Disassemble bytecode
adesh disassemble program.adeshbc

# Save output to file
adesh disassemble program.adeshbc > disassembly.txt
```

**Output Format:**
- Instruction addresses
- Operation codes
- Operands
- Constants pool
- Metadata

---

### docs

Generate HTML documentation from AdeshLang source code.

**Syntax:**
```bash
adesh docs <input.adesh> <output-directory>
```

**Examples:**
```bash
# Generate documentation
adesh docs program.adesh ./docs

# Generate docs for entire project
adesh docs main.adesh ./project-docs
```

**Features:**
- Extracts comments and docstrings
- Generates API documentation
- Creates cross-referenced HTML
- Includes type signatures

---

### init

Initialize a new AdeshLang project with boilerplate code.

**Syntax:**
```bash
adesh init <directory>
```

**Examples:**
```bash
# Create new project
adesh init my-project

# Create in current directory
adesh init .
```

**Generated Structure:**
```
my-project/
├── main.adesh      # Main program file
└── utils.adesh     # Utility module
```

**main.adesh Template:**
```javascript
import "./utils.adesh" as u;

export let PI = 3.14159;

fn double(x){ return x * 2; }
export fn timesTwo(x){ return double(x); }

class Counter {
  fn init(n){ this.n = n; }
  fn inc(){ this.n = this.n + 1; }
  fn value(){ return this.n; }
}

let c = new Counter(5);
c.inc();
print(c.value());

let xs = [1,2,3,4];
let ys = map(xs, fn(x){ return x*10; });
print(ys);

try {
  throw Error("Custom error message");
} catch(e) {
  print("Caught: " + e);
}

print(u.utilAdd(10, 22));
```

---

### Legacy Commands

These commands are maintained for backward compatibility:

#### run-interpret
Force execution using the interpreter backend.
```bash
adesh run-interpret program.adesh
```
*Equivalent to:* `adesh run --interpreter program.adesh`

#### run-jit
Force execution using the JIT backend.
```bash
adesh run-jit program.adesh
```
*Equivalent to:* `adesh run --jit program.adesh`

#### run-bc
Execute a pre-compiled bytecode file directly.
```bash
adesh run-bc program.adeshbc
```

---

## Execution Backends

AdeshLang supports multiple execution backends, each with different performance and compatibility trade-offs.

### --interpreter (Default)

Standard AST-walking interpreter.

**Characteristics:**
- ✅ Most compatible
- ✅ Best debugging support
- ✅ Fastest compilation
- ⚠️ Slower runtime execution
- ✅ Full language support

**Usage:**
```bash
adesh run --interpreter program.adesh
adesh run --interp program.adesh
```

**When to Use:**
- Development and debugging
- Running untrusted code
- Maximum compatibility needed
- Quick script execution

---

### --jit

Just-In-Time compilation using LIR (Low-level Intermediate Representation) executor.

**Characteristics:**
- ✅ Fast execution (5-10x speedup)
- ⚠️ Longer compilation time
- ✅ Optimized machine code
- ⚠️ May not support all language features

**Usage:**
```bash
adesh run --jit program.adesh
```

**When to Use:**
- Performance-critical applications
- Long-running processes
- CPU-intensive computations
- Production deployments

**Optimization:**
Combine with `-O3` for maximum performance:
```bash
adesh run --jit -O3 program.adesh
```

---

### --mixed (Hybrid Mode)

Hybrid execution: interprets code initially, then JIT-compiles hot functions.

**Characteristics:**
- ✅ Fast startup time
- ✅ Good long-term performance
- ✅ Adaptive optimization
- ✅ Balanced approach

**Usage:**
```bash
adesh run --mixed program.adesh
adesh run --hybrid program.adesh
```

**When to Use:**
- Web servers
- CLI applications
- Scripts with hot loops
- Unknown execution patterns

**Behavior:**
1. Starts with interpreter
2. Profiles function calls
3. Compiles frequently-called functions with JIT
4. Falls back to interpreter on JIT errors

---

### --safe

Safe mode with maximum safety checks and no JIT compilation.

**Characteristics:**
- ✅ Maximum safety guarantees
- ✅ Full GC safety checks
- ✅ Memory safety validation
- ⚠️ Slower execution
- ✅ Predictable behavior

**Usage:**
```bash
adesh run --safe program.adesh
```

**When to Use:**
- Critical systems
- Untrusted code execution
- Security-sensitive applications
- Debugging memory issues

---

### --bytecode (VM)

Execute using bytecode virtual machine.

**Characteristics:**
- ✅ Compact bytecode format
- ✅ Fast execution
- ✅ Easy distribution
- ✅ Platform-adeshependent

**Usage:**
```bash
adesh run --bytecode program.adesh
adesh run --bc program.adesh
adesh run --vm program.adesh
```

**Workflow:**
1. Compiles to bytecode (temporary)
2. Executes in VM
3. Cleans up temporary files

**Pre-compiled Execution:**
```bash
# Compile once
adesh compile program.adesh program.adeshbc

# Run many times
adesh run-bc program.adeshbc
```

**When to Use:**
- Distribution of programs
- Reduced file size
- Faster startup than JIT
- Cross-platform deployment

---

### --adaptive (Adaptive JIT)

Adaptive JIT compilation with speculative optimization and deoptimization.

**Characteristics:**
- ✅ Highest peak performance
- ✅ Type specialization based on runtime profiling
- ✅ Hidden classes for fast property access
- ✅ Inline caches for method calls
- ⚠️ Longer warmup time
- ✅ Automatic deoptimization on speculation failure

**Usage:**
```bash
adesh run --adaptive program.adesh
adesh run --ajit program.adesh
adesh run --adaptive-jit program.adesh
```

**How It Works:**
1. Starts with profiling interpreter
2. Collects type feedback and branch statistics
3. Speculatively optimizes based on observed patterns
4. Deoptimizes if assumptions are violated
5. Recompiles with new information

**When to Use:**
- Long-running applications
- Hot code paths with stable types
- Maximum performance requirements
- Applications with predictable behavior

---

### --tiered (Tiered JIT)

Multi-tier JIT compilation with automatic tier promotion.

**Characteristics:**
- ✅ Fast startup time
- ✅ Progressive optimization
- ✅ Balanced performance
- ✅ Hot function detection

**Usage:**
```bash
adesh run --tiered program.adesh
adesh run --tjit program.adesh
adesh run --tiered-jit program.adesh
```

**Tier Levels:**
| Tier | Name | Description |
|------|------|-------------|
| T0 | Interpreter | Initial execution, minimal overhead |
| T1 | Baseline JIT | Fast compilation, basic optimizations |
| T2 | Optimizing JIT | Aggressive optimization for hot code |

**Promotion Thresholds:**
- Interpreter → Baseline: 10 calls (configurable)
- Baseline → Optimizing: 100 calls (configurable)

**When to Use:**
- Web servers
- Long-running processes
- Mixed workloads (cold + hot code)
- Applications with varying hot paths

---

## Options

### Optimization Levels

Control the optimization level for JIT and bytecode compilation.

| Flag | Level | Compilation | Runtime | Use Case |
|------|-------|-------------|---------|----------|
| `-O0` or `--opt O0` | None | Fastest | Slowest | Development, debugging |
| `-O1` or `--opt O1` | Basic | Fast | Medium | **Default**, general use |
| `-O2` or `--opt O2` | Standard | Medium | Fast | Production |
| `-O3` or `--opt O3` | Maximum | Slowest | Fastest | Performance-critical |

**Examples:**
```bash
# No optimization (fast compile, slow run)
adesh run --jit -O0 program.adesh

# Maximum optimization (slow compile, fast run)
adesh run --jit -O3 program.adesh

# Specific level
adesh run --jit --opt O2 program.adesh
```

**Optimization Techniques:**
- **O0**: No optimization, direct translation
- **O1**: Constant folding, dead code elimination
- **O2**: Function inlining, loop optimization, strength reduction
- **O3**: Aggressive inlining, vectorization, advanced optimizations

---

### Memory & Garbage Collection

Configure garbage collection behavior.

#### --gc arc
Use only reference counting (Arc/Rc), no tracing GC.

```bash
adesh run --gc arc program.adesh
```

**Characteristics:**
- ✅ Predictable deallocation
- ✅ No GC pauses
- ⚠️ Cannot collect cycles
- ✅ Low memory overhead

**Use Case:** Programs without circular references

---

#### --gc hybrid (Default)
Hybrid reference counting + mark-sweep for cycle detection.

```bash
adesh run --gc hybrid program.adesh
```

**Characteristics:**
- ✅ **Default behavior**
- ✅ Collects most garbage immediately
- ✅ Handles cycles
- ⚠️ Occasional small GC pauses

**Use Case:** General-purpose applications

---

#### --gc full
Full compacting garbage collector.

```bash
adesh run --gc full program.adesh
```

**Characteristics:**
- ✅ Maximum memory efficiency
- ✅ Defragmentation
- ⚠️ Longer GC pauses
- ✅ Best for long-running programs

**Use Case:** Memory-constrained environments

---

#### --gc manual
No automatic garbage collection.

```bash
adesh run --gc manual program.adesh
```

**Characteristics:**
- ✅ Zero GC overhead
- ⚠️ Manual memory management required
- ⚠️ Risk of memory leaks
- ✅ Maximum control

**Use Case:** Embedded systems, WASM, advanced users

---

### IR Debugging

Dump intermediate representations for debugging and optimization analysis.

#### --dump-ast
Dump Abstract Syntax Tree.

```bash
adesh run --dump-ast program.adesh
```

**Shows:** Parser output, syntax structure

#### --dump-ast-write [file]
Write AST dump to file.

```bash
adesh run --dump-ast-write program.adesh           # Auto-generate: output.ast.adesh
adesh run --dump-ast-write ast.txt program.adesh   # Custom filename
```

**Shows:** Same as --dump-ast, but written to file

---

#### --dump-hir / --dump-ir
Dump High-level Intermediate Representation.

```bash
adesh run --dump-hir program.adesh
adesh run --dump-ir program.adesh    # Alias
```

**Shows:** Type-checked IR, semantic analysis

#### --dump-hir-write / --dump-ir-write [file]
Write HIR dump to file.

```bash
adesh run --dump-hir-write program.adesh           # Auto-generate: output.hir.adesh
adesh run --dump-hir-write hir.txt program.adesh   # Custom filename
adesh run --dump-ir-write program.adesh            # Alias for --dump-hir-write
```

**Shows:** Same as --dump-hir, but written to file

---

#### --dump-lir
Dump Low-level Intermediate Representation (SSA form).

```bash
adesh run --dump-lir program.adesh
```

**Shows:** SSA IR, control flow, optimization candidates

#### --dump-lir-write [file]
Write LIR dump to file.

```bash
adesh run --dump-lir-write program.adesh           # Auto-generate: output.lir.adesh
adesh run --dump-lir-write lir.txt program.adesh   # Custom filename
```

**Shows:** Same as --dump-lir, but written to file

---

#### --dump-cfg
Dump Control Flow Graph (CFG).

```bash
adesh run --dump-cfg program.adesh
```

**Shows:** Control Flow Graph blocks and edges, basic blocks, and control flow.

#### --dump-cfg-write [file]
Write CFG dump to file.

```bash
adesh run --dump-cfg-write program.adesh           # Auto-generate: output.cfg.adesh
adesh run --dump-cfg-write cfg.txt program.adesh   # Custom filename
```

**Shows:** Same as --dump-cfg, but written to file

---

#### --dump-bytecode / --dump-bc
Dump bytecode instructions.

```bash
adesh run --dump-bytecode program.adesh
adesh run --dump-bc program.adesh    # Alias
```

**Shows:** VM instructions, constants, metadata

---

#### --dump-all
Dump all intermediate representations.

```bash
adesh run --dump-all program.adesh
```

**Equivalent to:**
```bash
adesh run --dump-ast --dump-hir --dump-lir --dump-bytecode program.adesh
```

---

### IO Options

#### --buffered-stdout
Use buffered stdout for better performance.

```bash
adesh run --buffered-stdout program.adesh
```

**Use Case:** Programs with heavy console output

---

#### --interactive-input / --interactive
Enable interactive input mode.

```bash
adesh run --interactive program.adesh
```

**Use Case:** Programs requiring user input

---

#### --no-color
Disable ANSI color codes in output.

```bash
adesh run --no-color program.adesh
```

**Use Case:** 
- Non-TTY environments
- Log files
- CI/CD pipelines

---

### Other Options

#### --profile / --time
Enable profiling and timing output.

```bash
adesh run --profile program.adesh
adesh run --time program.adesh       # Alias
```

**Output:**
```
⏱  Execution time: 127.45ms
```

---

#### --memory
Show detailed memory usage statistics after program execution.

```bash
adesh run --memory program.adesh
adesh run --jit --memory program.adesh
```

**Output for Interpreter:**
```
📁 Program Memory Usage (Interpreter):
   ─────────────────────────────────────────
   Total variables:      4
   Total variable memory: 166 bytes

   📊 Memory by Type:
      function       2 items,    108 bytes
      instance       1 items,     50 bytes
      promise        1 items,      8 bytes

   📋 Variables (by size):
      test                 function       54 bytes
      this                 instance       50 bytes
      p                    promise         8 bytes

   🔧 Runtime State:
      Scopes/Environments: 3
      Active Promises:     1
      Active Timers:       0
      Method Cache:        0 entries

📊 Process Memory Statistics:
   Working set size:     8252 kB
   Peak working set:     8284 kB
   Page file usage:      39084 kB
   Page fault count:     2160
   Allocator: system
```

**Output for JIT:**
```
📁 Program Memory Usage (JIT):
   ─────────────────────────────────────────
   Total variables:      3
   Total variable memory: 120 bytes

   📊 Memory by Type:
      function       1 items,     64 bytes
      number         2 items,     56 bytes

   📋 Variables (by size):
      main                 function       64 bytes
      x                    number         28 bytes
      y                    number         28 bytes

   🔧 JIT Runtime State:
      Compiled Functions:  1
      Active Promises:     0
      Memo Cache Entries:  0

📊 Process Memory Statistics:
   Working set size:     7236 kB
   Peak working set:     7236 kB
   Page file usage:      34048 kB
   Peak page file:       34068 kB
   Page fault count:     1885
   Allocator: system
```

**Sections Explained:**
- **Program Memory Usage**: Shows memory used by your program's variables
- **Memory by Type**: Breakdown by data type (functions, numbers, strings, etc.)
- **Variables**: Individual variable memory usage sorted by size
- **Runtime State**: Internal runtime statistics (scopes, promises, timers)
- **Process Memory Statistics**: OS-level memory statistics (working set, page faults)

**Use Cases:**
- Memory profiling and optimization
- Detecting memory leaks or excessive allocations
- Understanding memory footprint of different backends
- Comparing interpreter vs JIT memory usage

---

#### --verbose
Enable verbose diagnostic output.

```bash
adesh run --verbose program.adesh
```

**Shows:**
- Backend selection
- Compilation phases
- Optimization decisions
- Module loading

---

#### --debug
Enable debug mode with additional information.

```bash
adesh run --debug program.adesh
```

**Shows:**
- Detailed stack traces
- Internal state
- Debug symbols
- Memory statistics

---

## Source Directives

AdeshLang supports inline directives at the top of source files to specify compilation behavior.

### @compile Directive

Specify the compilation/execution backend directly in the source file.

**Syntax:**
```javascript
@compile <backend>
```

**Supported Backends:**
- `jit` - JIT compilation
- `bytecode` - Bytecode VM
- `mixed` - Hybrid mode
- `wasm` - WebAssembly (when using compile-wasm)

**Examples:**

**jit_program.adesh:**
```javascript
@compile jit

fn fibonacci(n) {
  if n <= 1 { return n; }
  return fibonacci(n-1) + fibonacci(n-2);
}

print(fibonacci(30));
```

**bytecode_program.adesh:**
```javascript
@compile bytecode

fn main() {
  print("Running in bytecode VM");
}

main();
```

**mixed_program.adesh:**
```javascript
@compile mixed

// Hot loop will be JIT-compiled
fn compute() {
  let sum = 0;
  for i in range(1000000) {
    sum = sum + i;
  }
  return sum;
}

print(compute());
```

**Behavior:**
When running with `adesh run program.adesh`, the backend specified in the `@compile` directive **overrides** the command-line backend flag.

**Example:**
```bash
# This will use JIT (from @compile directive)
# even though --interpreter is specified
adesh run --interpreter jit_program.adesh
```

**Directive Priority:**
```
@compile directive > command-line flag > default (interpreter)
```

---

## Program Arguments

Pass arguments to the AdeshLang program using `--` separator.

**Syntax:**
```bash
adesh run <file> -- <arg1> <arg2> <arg3>...
```

**Access in Program:**
Use the `args()` function to access command-line arguments:

**program.adesh:**
```javascript
let arguments = args();
print("Number of arguments: " + len(arguments));
for arg in arguments {
  print("Argument: " + arg);
}
```

**Execution:**
```bash
adesh run program.adesh -- hello world 123
```

**Output:**
```
Number of arguments: 3
Argument: hello
Argument: world
Argument: 123
```

---

## Examples

### Basic Execution

```bash
# Run with default settings
adesh run hello.adesh

# Run with JIT
adesh run --jit hello.adesh

# Run with JIT and max optimization
adesh run --jit -O3 hello.adesh
```

---

### Performance Optimization

```bash
# Maximum performance
adesh run --jit -O3 --gc arc compute.adesh

# Profile execution time
adesh run --jit -O3 --profile compute.adesh

# Mixed mode for adaptive optimization
adesh run --mixed -O2 server.adesh
```

---

### Development & Debugging

```bash
# Verbose output with debug info
adesh run --verbose --debug program.adesh

# Dump all IR stages
adesh run --dump-all program.adesh

# Write IR dumps to files
adesh run --dump-hir-write program.adesh           # output.hir.adesh
adesh run --dump-ast-write ast.txt program.adesh   # custom filename
adesh run --dump-lir-write program.adesh           # output.lir.adesh

# Safe mode for debugging
adesh run --safe --debug program.adesh

# Check optimization opportunities
adesh run --jit --dump-lir -O3 program.adesh
```

---

### Compilation Workflows

```bash
# Compile to bytecode
adesh compile program.adesh program.adeshbc

# Run compiled bytecode
adesh run-bc program.adeshbc

# Compile to WebAssembly
adesh compile-wasm program.adesh program.wasm

# Compile to WebAssembly + JS loader
adesh compile-wasm-js program.adesh ./dist

# Transpile to Rust
adesh compile-native-rust program.adesh program.rs
rustc program.rs -o program
./program
```

---

### Advanced Usage

```bash
# JIT with verbose output and profiling
adesh run --jit -O3 --verbose --profile --dump-lir program.adesh

# Safe mode with full GC
adesh run --safe --gc full --debug program.adesh

# Bytecode with timing
adesh run --bytecode --profile program.adesh

# Mixed mode with dump
adesh run --mixed -O2 --dump-hir --verbose program.adesh

# No color output for logging
adesh run --no-color program.adesh > output.log 2>&1
```

---

## Configuration

### Environment Variables

Configure AdeshLang behavior through environment variables:

```bash
# Disable color globally
export NO_COLOR=1
adesh run program.adesh

# Set default stack size (bytes)
export ADESH_STACK_SIZE=33554432  # 32MB
adesh run program.adesh
```

---

### Configuration File

AdeshLang supports a configuration file for persistent settings.

**Location:** `.adeshconfig.toml` (project root)

**Template:**
```toml
# AdeshLang Runtime Configuration

[execution]
backend = "jit"          # interpreter, jit, mixed, safe, bytecode
opt_level = "O2"         # O0, O1, O2, O3

[memory]
gc_mode = "hybrid"       # arc, hybrid, full, manual

[io]
buffered_stdout = false
interactive_input = false
no_color = false

[debug]
profile = false
verbose = false
debug = false

[dump]
dump_ast = false
dump_hir = false
dump_lir = false
dump_bytecode = false
```

**Usage:**
```bash
# Uses settings from .adeshconfig.toml
adesh run program.adesh

# Command-line flags override config file
adesh run --jit -O3 program.adesh
```

**Priority:**
```
Command-line flags > @compile directive > Config file > Defaults
```

---

## Quick Reference

### Common Commands

| Command | Description |
|---------|-------------|
| `adesh run <file>` | Run a program |
| `adesh repl` | Start REPL |
| `adesh format <file>` | Format source code |
| `adesh fmt <file>` | Alias for format |
| `adesh compile <in> <out>` | Compile to bytecode |
| `adesh --help` | Show help |
| `adesh --version` | Show version |

### Backend Flags

| Flag | Backend |
|------|---------|
| `--interpreter` | Interpreter (default) |
| `--jit` | JIT compiler |
| `--mixed` | Hybrid mode |
| `--safe` | Safe mode |
| `--bytecode` | Bytecode VM |
| `--adaptive`, `--ajit` | Adaptive JIT with speculative optimization |
| `--tiered`, `--tjit` | Tiered JIT (T0→T1→T2) |

### Optimization

| Flag | Level |
|------|-------|
| `-O0` | No optimization |
| `-O1` | Basic (default) |
| `-O2` | Standard |
| `-O3` | Maximum |

### GC Modes

| Flag | Mode |
|------|------|
| `--gc arc` | Reference counting only |
| `--gc hybrid` | Hybrid (default) |
| `--gc full` | Full compacting GC |
| `--gc manual` | No GC |

### Debug Flags

| Flag | Purpose |
|------|---------|
| `--dump-ast` | Show AST |
| `--dump-ast-write [file]` | Write AST to file |
| `--dump-hir` | Show HIR |
| `--dump-hir-write [file]` | Write HIR to file |
| `--dump-ir-write [file]` | Alias for --dump-hir-write |
| `--dump-lir` | Show LIR |
| `--dump-lir-write [file]` | Write LIR to file |
| `--dump-bytecode` | Show bytecode |
| `--dump-all` | Show all IRs |
| `--verbose` | Verbose output |
| `--debug` | Debug mode |
| `--profile` | Show timing |
| `--memory` | Show detailed memory statistics |

---

## Error Messages and Diagnostics

AdeshLang provides detailed error messages to help you debug your code quickly.

### Error Message Format

Errors include:
- **Error type**: ParseError, TypeError, RuntimeError, CompileError, JitError, etc.
- **File location**: Path, line number, and column
- **Source snippet**: The relevant line of code
- **Visual indicator**: Caret (^) pointing to the error location
- **Hint** (when available): Suggestions for fixing the error
- **Note** (when available): Additional context about the error

**Example:**
```
ParseError: Unexpected token ')' at src/main.adesh:12:8

  12 | let x = foo());
     |        ^
     = hint: Did you mean to remove the extra closing parenthesis?
```

### JIT Compilation Errors

When using JIT compilation (`--jit`), errors are mapped back to source locations through the HIR/LIR pipeline:

```bash
# Run with verbose JIT output
adesh run --jit --verbose program.adesh
```

### Debug Information Flags

| Flag | Description |
|------|-------------|
| `--dump-ast` | Show Abstract Syntax Tree |
| `--dump-hir` | Show High-level IR (typed, structured) |
| `--dump-lir` | Show Low-level IR (SSA form) |
| `--verbose` | Show compilation and execution details |
| `--debug` | Enable debug mode with stack traces |

---

## Tiered JIT Compilation

AdeshLang supports tiered JIT compilation for optimal balance between startup time and peak performance.

### How It Works

1. **Tier 0 (Interpreter)**: Code starts in the interpreter for fast startup
2. **Tier 1 (Baseline JIT)**: Hot functions are compiled with minimal optimization
3. **Tier 2 (Optimizing JIT)**: Very hot functions receive aggressive optimization

### Using Tiered JIT

```bash
# Mixed mode: interpreter + JIT for hot code
adesh run --mixed program.adesh

# Direct JIT: everything compiled
adesh run --jit program.adesh
```

### Profiling Hot Code

```bash
# See which functions are JIT-compiled
adesh run --mixed --verbose --profile program.adesh
```

---

## Recursion Optimization Pack

AdeshLang includes a comprehensive recursion optimization pack for high-performance recursive code execution.

### Available Optimizations

1. **Tail Call Optimization (TCO)**: Converts tail-recursive calls into loops
2. **Memoization**: Caches results of pure recursive functions
3. **Trampolining**: Prevents stack overflow for deep recursion
4. **JIT Inlining**: Inlines hot recursive call sites

### CLI Flags

| Flag | Description |
|------|-------------|
| `--fast-recursion` | Enable all recursion optimizations |
| `--no-recursion-opt` | Disable all recursion optimizations |
| `--tco` | Enable tail call optimization only |
| `--memo` | Enable memoization only |
| `--recursion-opt=MODE` | Set mode: `full`, `tco`, `memo`, `jit-inline`, `none` |

### Usage Examples

```bash
# Maximum recursion performance (default for JIT)
adesh run --jit --fast-recursion fib.adesh

# Debug mode without recursion optimizations
adesh run --interpreter --no-recursion-opt fib.adesh

# Just memoization for pure recursive functions
adesh run --jit --memo fib.adesh

# Explicit full optimization
adesh run --jit --recursion-opt=full fib.adesh
```

### Supported Patterns

The recursion optimizer automatically detects and optimizes:

- **Tail recursion**: `fn loop(n) { if (n <= 0) return 0; return loop(n-1); }`
- **Pure recursion**: `fn fib(n) { if (n <= 1) return n; return fib(n-1) + fib(n-2); }`
- **Accumulator pattern**: `fn sum(n, acc) { if (n <= 0) return acc; return sum(n-1, acc+n); }`

### Performance Tips

For recursive algorithms like Fibonacci:

```adesh
// Naive recursive (slow without memoization)
fn fib(n) {
    if (n <= 1) return n;
    return fib(n-1) + fib(n-2);
}

// With memoization enabled (fast)
// adesh run --jit --memo fib.adesh
print(fib(40));  // Computes quickly with caching
```

---

## Limitations and Known Issues

### JIT Compilation Limitations

- **Classes**: OOP features (classes, inheritance, method resolution) are supported in the interpreter and have basic support in JIT. Complex inheritance chains may fall back to interpreter.
- **Async/Await**: Async functions compile but event loop interaction is limited in JIT mode.
- **Decorators**: Function decorators work in interpreted mode; JIT support is partial.

### Platform-Specific Notes

- **LLVM AOT**: The `compile-native` command requires LLVM. On platforms without LLVM, use `compile-native-rust` as an alternative.
- **WebAssembly**: Some language features may not be available in WASM output.

### Workarounds

If JIT compilation fails for specific code:

```bash
# Use mixed mode - falls back to interpreter automatically
adesh run --mixed program.adesh

# Or use interpreter directly
adesh run --interpreter program.adesh
```

---

## Getting Help

**Online Documentation:** https://github.com/ajaytainwala-dev/mylang

**Show CLI Help:**
```bash
adesh --help
```

**Community:**
- GitHub Issues: https://github.com/ajaytainwala-dev/mylang/issues
- Discussions: https://github.com/ajaytainwala-dev/mylang/discussions

---

**Last Updated:** January 14, 2026  
**Version:** AdeshLang v0.3.0  
**Note:** Some sections include planned capabilities; cross-check with `docs/CURRENT_STATE_AND_NEXT.md` for what is implemented in `src/`.

---

## ML/AI Data Structures

AdeshLang includes built-in ML/AI-oriented data structures designed to be competitive with NumPy/PyTorch while offering lower overhead.

### Core Data Structures

#### NDArray (N-Dimensional Array)

High-performance tensor implementation with multiple data types and efficient memory layout.

**Supported Data Types (DType):**

| DType | Description | Size |
|-------|-------------|------|
| `F16` | Half precision float | 2 bytes |
| `BF16` | Brain float 16 | 2 bytes |
| `F32` | Single precision float | 4 bytes |
| `F64` | Double precision float | 8 bytes |
| `I8/I16/I32/I64` | Signed integers | 1-8 bytes |
| `U8/U16/U32/U64` | Unsigned integers | 1-8 bytes |
| `Bool` | Boolean | 1 byte |
| `Complex64/128` | Complex numbers | 8-16 bytes |

**Memory Layouts:**
- `RowMajor` (C-style, default)
- `ColMajor` (Fortran-style)
- `Strided` (custom strides)

**Device Support:**
- `CPU` (default)
- `CUDA(n)` (NVIDIA GPU)
- `Metal` (Apple Silicon)
- `Vulkan` (cross-platform GPU)
- `TPU` (Tensor Processing Unit)

**Operations:**
- Element-wise: `add`, `sub`, `mul`, `div`, `pow`, `neg`, `abs`
- Mathematical: `sqrt`, `exp`, `log`, `sin`, `cos`, `tan`
- Reductions: `sum`, `mean`, `max`, `min`, `var`, `std`
- Matrix: `matmul`, `bmm` (batch matmul), `dot`, `transpose`
- Activations: `relu`, `leaky_relu`, `sigmoid`, `tanh`, `softmax`

#### Dataset & DataLoader

Data loading abstractions for ML training pipelines.

```adesh
// Create a dataset
let ds = Dataset.new();
ds.add(features, labels);

// Create data loader with batching
let loader = DataLoader.new(ds, batch_size=32, shuffle=true);

// Iterate over batches
while let batch = loader.next_batch() {
    // Process batch
}
```

#### ModelGraph (Computation Graph)

Represents ML models as directed acyclic graphs for optimization and execution.

**Supported Operations:**
- Basic: `Input`, `Constant`, `Variable`
- Arithmetic: `Add`, `Sub`, `Mul`, `Div`
- Matrix: `MatMul`, `BatchMatMul`, `Transpose`
- Layers: `Linear`, `Conv2D`, `BatchNorm`, `Dropout`
- Loss: `MSELoss`, `CrossEntropyLoss`

### MLIR Integration

AdeshLang uses an MLIR-style intermediate representation for optimized ML operations.

**Supported Dialects:**
- `Arith` - Standard arithmetic operations
- `Tensor` - Tensor operations
- `Linalg` - Linear algebra operations
- `Vector` - SIMD vectorization
- `Affine` - Affine transformations
- `GPU` - GPU-specific operations
- `Adesh` - Custom AdeshLang dialect

### Optimization Passes

The graph compiler supports multiple optimization passes:

| Pass | Description |
|------|-------------|
| `OpFusion` | Fuse consecutive operations |
| `ConstantFolding` | Evaluate constant expressions at compile time |
| `DeadCodeElimination` | Remove unused operations |
| `CSE` | Common subexpression elimination |
| `LayoutOptimization` | Optimize memory layouts |
| `Vectorization` | Apply SIMD vectorization |
| `MemoryPlanning` | Optimize memory allocation |

### Random Tensor Generation

Built-in random number generation for tensor initialization:

```adesh
let rng = TensorRng.new(seed=42);

// Uniform random [0, 1)
let uniform = rng.uniform([100, 100], 0.0, 1.0);

// Standard normal (mean=0, std=1)
let normal = rng.randn([100, 100]);

// Xavier/Glorot initialization (for neural networks)
let weights = rng.xavier([784, 256]);

// Kaiming/He initialization (for ReLU networks)
let conv_weights = rng.kaiming([64, 3, 3, 3]);
```

### Performance Comparison

AdeshLang's ML data structures are designed to be faster than Python + NumPy:

| Operation | Python/NumPy | AdeshLang JIT |
|-----------|--------------|--------------|
| Matrix multiply (1000x1000) | ~50ms | ~30ms |
| Element-wise ops (1M elements) | ~5ms | ~2ms |
| Reduction (1M elements) | ~2ms | ~1ms |

*Note: Performance varies by hardware. JIT compilation provides the best results for repeated operations.*

### Example: Simple Neural Network

```adesh
// Define a simple MLP
let graph = ModelGraph.new();

let x = graph.add_input("x");
let w1 = graph.add_node(Linear(784, 256), [x], "fc1");
let a1 = graph.add_node(ReLU, [w1], "relu1");
let w2 = graph.add_node(Linear(256, 10), [a1], "fc2");
let out = graph.add_node(Softmax, [w2], "output");
graph.mark_output(out);

// Compile and execute
let compiled = CompiledGraph.compile(graph, config);
let result = compiled.execute([input_tensor]);
```



---

## Source: CLI_AOT_REDESIGN.md

# AdeshLang CLI AOT Redesign - Design Document

**Date:** February 3, 2026  
**Status:** Implementation Ready  
**Author:** AdeshLang Team  

---

## Executive Summary

This document describes the redesign of AdeshLang's AOT compilation CLI to introduce a modern, compiler-grade `build` command while maintaining full backward compatibility with the existing `compile-aot` command.

---

## Design Overview

### Command Structure

| Command | Purpose | Target Users |
|---------|---------|--------------|
| `adesh build` | High-level, user-friendly build command | All users |
| `adesh compile-aot` | Low-level, explicit AOT command | Power users, scripts |

Both commands route to the **same internal AOT pipeline**, ensuring feature parity.

---

## Command Specification

### `adesh build` (New)

```
adesh build <file.adesh> [options]
adesh build <file.adesh> -o <output> [options]
```

**Defaults:**
- Output format: Native executable
- Output name: `<file>` (no extension on Unix, `.exe` on Windows)
- Optimization: `-O2`
- Backend: AOT (Cranelift)

### `adesh compile-aot` (Existing - Unchanged)

```
adesh compile-aot <input.adesh> <output> [options]
```

Remains fully supported with identical behavior.

---

## Flag Mapping

### Unified Flags (Both Commands)

| Flag | Short | Description |
|------|-------|-------------|
| `-o <file>` | | Output file path |
| `-O0`/`-O1`/`-O2`/`-O3` | | Optimization level |
| `--release` | | Alias for `-O3` |
| `--debug` | `-g` | Include debug information |
| `--target=<triple>` | | Cross-compilation target |
| `--emit=<type>` | | Output type selection |
| `--lib` | | Build as library (static) |
| `--shared` | | Build as shared library |
| `-c` | | Compile only (object file) |
| `-S` | | Emit assembly |
| `--emit-header [file]` | | Generate C header |
| `-I<dir>` | | Add include directory |
| `-L<dir>` | | Add library search path |
| `-l<lib>` | | Link with library |
| `--verbose` | `-v` | Verbose output |
| `--help` | `-h` | Show help |

### Output Type Selection

The `--emit=<type>` flag provides explicit output type control:

| Value | Description | File Extension |
|-------|-------------|----------------|
| `exe` | Native executable (default) | `.exe` / none |
| `obj` | Object file | `.o` / `.obj` |
| `lib` | Static library | `.a` / `.lib` |
| `dylib` | Shared library | `.so` / `.dll` / `.dylib` |
| `asm` | Assembly | `.s` / `.asm` |
| `llvm-ir` | LLVM IR (future) | `.ll` |
| `mir` | Machine IR | `.mir` |

---

## Output File Inference

### Auto-Extension Rules

```
adesh build program.adesh              → program / program.exe
adesh build program.adesh -o app       → app / app.exe
adesh build program.adesh --lib        → libprogram.a / program.lib
adesh build program.adesh --shared     → libprogram.so / program.dll
adesh build program.adesh -c           → program.o / program.obj
```

### Platform-Specific Extensions

| Platform | Executable | Object | Static Lib | Shared Lib |
|----------|------------|--------|------------|------------|
| Windows | `.exe` | `.obj` | `.lib` | `.dll` |
| Linux | none | `.o` | `.a` | `.so` |
| macOS | none | `.o` | `.a` | `.dylib` |

---

## Internal Architecture

### Command Routing

```
                    ┌─────────────────┐
                    │   CLI Parser    │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
        ┌─────────┐    ┌─────────┐    ┌─────────┐
        │  build  │    │compile- │    │   run   │
        │         │    │   aot   │    │         │
        └────┬────┘    └────┬────┘    └─────────┘
             │              │
             ▼              ▼
        ┌─────────────────────────────────────┐
        │       AotBuildConfig                │
        │  (Unified configuration struct)     │
        └──────────────────┬──────────────────┘
                           │
                           ▼
        ┌─────────────────────────────────────┐
        │     CraneliftAotCompiler            │
        │  (Existing AOT pipeline)            │
        └─────────────────────────────────────┘
```

### New Types

```rust
/// Unified build configuration
pub struct AotBuildConfig {
    /// Input source file
    pub input: PathBuf,
    /// Output path (optional, auto-inferred)
    pub output: Option<PathBuf>,
    /// Optimization level (0-3)
    pub opt_level: u8,
    /// Output format
    pub emit: EmitType,
    /// Target triple for cross-compilation
    pub target: Option<String>,
    /// Include debug info
    pub debug_info: bool,
    /// Generate C header
    pub emit_header: Option<PathBuf>,
    /// Include directories
    pub include_dirs: Vec<PathBuf>,
    /// Library search paths
    pub lib_dirs: Vec<PathBuf>,
    /// Libraries to link
    pub link_libs: Vec<String>,
    /// Extra linker arguments
    pub linker_args: Vec<String>,
    /// Verbose output
    pub verbose: bool,
}

pub enum EmitType {
    Executable,
    Object,
    StaticLib,
    SharedLib,
    Assembly,
}
```

---

## Example Workflows

### Basic Compilation

```bash
# Simple build (defaults to executable)
adesh build program.adesh

# Equivalent compile-aot
adesh compile-aot program.adesh program
```

### Release Build

```bash
# Optimized release build
adesh build program.adesh --release

# Equivalent
adesh compile-aot program.adesh program -O3
```

### Library Development

```bash
# Static library
adesh build mylib.adesh --lib
# Generates: libmylib.a / mylib.lib

# Shared library with header
adesh build mylib.adesh --shared --emit-header
# Generates: libmylib.so + mylib.h
```

### Cross-Compilation

```bash
# Linux ARM64
adesh build program.adesh --target=aarch64-unknown-linux-gnu

# Windows from Linux
adesh build program.adesh --target=x86_64-pc-windows-gnu -o app.exe
```

### FFI Integration

```bash
# Compile to object with header
adesh build math.adesh -c --emit-header math.h

# Use from C
gcc test.c math.o -o test
```

---

## Additional CLI Features

### New Commands

```bash
# Check compilation without output
adesh build --check program.adesh

# Show build plan without executing
adesh build --dry-run program.adesh

# Clean build artifacts
adesh clean

# Show target information
adesh target list
adesh target info <triple>
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ADESH_TARGET` | Default compilation target | Host |
| `ADESH_OPT_LEVEL` | Default optimization | 2 |
| `ADESH_CC` | C compiler for linking | Auto-detect |
| `ADESH_LD` | Linker override | Auto-detect |

### Configuration File

Support for `adesh.toml` project configuration:

```toml
[build]
default-target = "x86_64-unknown-linux-gnu"
opt-level = 2
debug = false

[build.release]
opt-level = 3
lto = true

[dependencies]
math = { path = "libs/math" }
```

---

## Help Output Design

```
AdeshLang Build Command

USAGE:
    adesh build [OPTIONS] <FILE>

ARGS:
    <FILE>    Source file to compile (.adesh)

OUTPUT OPTIONS:
    -o, --output <FILE>     Output file path
    --emit <TYPE>           Output type: exe, obj, lib, dylib, asm
    --lib                   Build as static library
    --shared                Build as shared library
    -c                      Compile only (output object file)
    -S                      Output assembly

OPTIMIZATION:
    -O0                     No optimization
    -O1                     Basic optimization
    -O2                     Standard optimization (default)
    -O3                     Maximum optimization
    --release               Alias for -O3
    -g, --debug             Include debug information

CROSS-COMPILATION:
    --target <TRIPLE>       Target platform (e.g., x86_64-unknown-linux-gnu)
                           Use 'adesh target list' to see available targets

FFI OPTIONS:
    --emit-header [FILE]    Generate C header for exported functions
    -I<DIR>                 Add include directory
    -L<DIR>                 Add library search path
    -l<LIB>                 Link with library

GENERAL:
    -v, --verbose           Verbose output
    --dry-run               Show what would be done
    --check                 Check compilation without generating output
    -h, --help              Show this help

EXAMPLES:
    adesh build program.adesh              Compile to native executable
    adesh build program.adesh -o app       Compile with custom output name
    adesh build program.adesh --release    Optimized release build
    adesh build lib.adesh --lib            Build static library
    adesh build lib.adesh --shared         Build shared library
    adesh build --target=aarch64-unknown-linux-gnu program.adesh
                                         Cross-compile for ARM64 Linux

For advanced AOT options, use 'adesh compile-aot --help'
```

---

## Migration Guide

### For Users

No migration needed. Existing `compile-aot` commands continue to work unchanged.

### For Scripts

- **Existing:** `adesh compile-aot program.adesh app` ✓ Works
- **New:** `adesh build program.adesh -o app` ✓ Also works

### Recommendation

- New projects: Use `adesh build`
- Existing scripts: Continue using `compile-aot` or migrate at convenience
- CI/CD: Both commands are stable and supported

---

## Implementation Plan

### Phase 1: Core Build Command (This PR)
1. Add `AotBuildConfig` struct
2. Implement `build` command parser
3. Route to existing AOT pipeline
4. Update help messages

### Phase 2: Enhanced Features
1. `--check` and `--dry-run` flags
2. Environment variable support
3. `adesh clean` command
4. `adesh target` subcommands

### Phase 3: Project Configuration
1. `adesh.toml` support
2. Workspace builds
3. Dependency management

---

## Rationale

### Why `build` instead of `compile`?

- Matches modern tooling (Cargo, Zig, Go)
- `compile` already exists for bytecode
- `build` implies complete, distributable output
- Avoids confusion with `compile-aot`

### Why keep `compile-aot`?

- Backward compatibility for existing users
- Explicit control for power users
- Scripts and documentation reference it
- No breaking changes

### Why unified AOT options?

- Single source of truth for AOT behavior
- Easier maintenance
- Consistent user experience
- No feature drift between commands

---

## Success Metrics

- [ ] All existing `compile-aot` commands work unchanged
- [ ] `adesh build` produces identical output to equivalent `compile-aot`
- [ ] Cross-compilation works via `--target`
- [ ] Help output is clear and grouped
- [ ] No user confusion about which command to use

---

*Document Version: 1.0*  
*Last Updated: February 3, 2026*


---

## Source: WINDOWS_DISTRIBUTION_GUIDE.md

# AdeshLang — Windows Distribution & Toolchain Setup Guide

## Overview

AdeshLang distributions on Windows target **Windows 10/11 64-bit** (`x86_64-pc-windows-msvc`). AdeshLang ships as a self-contained SDK (compiler, package manager, language server, editor, standard library). The pinned **LLVM 18.1.8 toolchain** (`clang.exe`, `lld-link.exe`, `llc.exe`) is downloaded during installation from the official llvm-project releases, SHA-256 verified, installed under `%ADESH_HOME%\toolchain\llvm` (or the upstream installer's default location), and its `bin` directory is added to the **system PATH** so the tools are available to every program. Run `adesh toolchain install --system` to fetch or repair it at any time.

---

## Environment Variables

The canonical variable prefix is `ADESH_` (the `ADESHLANG_*` spellings are
still accepted as legacy aliases). The installer sets these automatically.

| Variable | Description | Example Path |
|---|---|---|
| `ADESH_HOME` | Root directory of the AdeshLang installation | `C:\Program Files\AdeshLang` |
| `ADESH_TOOLCHAIN` | Path to the LLVM toolchain directory | `%ADESH_HOME%\toolchain\llvm` |
| `ADESH_CLANG` | Absolute path to the pinned clang executable | `%ADESH_TOOLCHAIN%\bin\clang.exe` |
| `ADESH_LLC` | Absolute path to llc | `%ADESH_TOOLCHAIN%\bin\llc.exe` |
| `ADESH_MLIR_OPT` | Absolute path to mlir-opt (GPU backend) | `%ADESH_TOOLCHAIN%\bin\mlir-opt.exe` |
| `ADESH_MLIR_TRANSLATE` | Absolute path to mlir-translate (GPU backend) | `%ADESH_TOOLCHAIN%\bin\mlir-translate.exe` |
| `ADESH_STD` | Standard library directory | `%ADESH_HOME%\std` |
| `ADESH_PACKAGES` | User package directory | `%LOCALAPPDATA%\AdeshLang\packages` |
| `ADESH_CACHE` | Compiler cache directory | `%LOCALAPPDATA%\AdeshLang\cache` |
| `ADESH_TOOLCHAIN_MANIFEST` | Path/URL override for the toolchain manifest | `https://.../toolchain-manifest.json` |
| `ADESH_REPO` | GitHub `owner/name` hosting releases (org migration helper) | `adeshlang/adeshlang` |

---

## Installation & Maintenance Commands

### 1. `adl doctor`
Inspects system health, compiler binary, isolated LLVM toolchain (`clang.exe`, `lld-link.exe`), standard library, and PATH environment configuration.

```powershell
adl doctor
```

### 2. `adl env`
Displays all current environment variables, directory paths, host/target triples, and version information.

```powershell
adl env
```

### 3. `adl repair`
Automatically restores missing package directories, creates local configuration folders, updates session environment variables, and verifies system integrity.

```powershell
adl repair
```

### 4. `adl pkg`
Manages AdeshLang packages:
- `adl pkg init [name]` - Initialize new `adesh.toml` manifest
- `adl pkg install <package>` - Install a package
- `adl pkg list` - List installed packages
- `adl pkg remove <package>` - Uninstall a package

### 5. `adesh toolchain install`
Downloads the pinned upstream toolchain (LLVM/Clang/LLD **18.1.8** from the official llvm-project releases, plus MLIR tools for the GPU backend), verifies SHA-256 checksums from the pinned manifest, and installs it inside the AdeshLang installation under `%ADESH_HOME%\toolchain\llvm` (never directly into `C:\Program Files\LLVM`). If the machine already has an LLVM toolchain — any version — nothing is downloaded and the existing one is reused and exposed.

```powershell
# Install for all users (elevated shell): also registers PATH + env vars
adesh toolchain install --system

# Also build MLIR GPU tools (mlir-opt/mlir-translate with NVVM/ROCDL)
# from upstream source — 30–90 minutes, needs VS C++ tools + CMake
adesh toolchain install --system --build-mlir-source

# Fallback channel: install via the system package manager
# (winget / apt / dnf / pacman / Homebrew)
adesh toolchain install --use-system-packages

# Preview what would be downloaded
adesh toolchain install --dry-run
```

### 6. `adesh toolchain expose`
Re-registers the system-wide PATH entries and environment variables for the installed toolchain (no download). Useful after manual PATH resets.

```powershell
adesh toolchain expose --system
```

### 7. `adesh ai`
Interface to the bundled AdeshLang AI model (`adesh-coder-0.5b`). Installers and portable archives ship the Q4_0 quantization (~275 MB) under `%ADESH_HOME%\ai\models\` together with the Ollama/OpenRouter deployment configs (`%ADESH_HOME%\ai\deploy\`), so the commands below work offline right after installation. Larger quantizations (Q8_0, F16) are opt-in downloads whose URLs and SHA-256 hashes are pinned in `ai\models\manifest.json`.

```powershell
adesh ai status      # show the active execution tiers (GGUF / PyTorch / Ollama)
adesh ai generate "Create a binary search function"
adesh ai explain examples\hello.adesh
adesh ai fix broken.adesh
adesh ai chat
adesh ai setup       # fetch additional model quantizations
```

---

## Building Distribution Packages

Run the release script to build the portable ZIP and GUI Installer:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release\build_windows_dist.ps1
```

Generated artifacts:
- **Portable Package**: `dist\AdeshLang-0.3.0-x86_64-windows-portable.zip`
- **GUI Setup Installer**: `dist\AdeshLang-0.3.0-x86_64-windows-setup.exe` (requires [Inno Setup](https://jrsoftware.org/isinfo.php))


---

## Source: embedded.md

# Embedded Mode

AdeshLang targets deeply embedded and HAL workloads with deterministic, GC-free memory. Embedded builds remove the heap and ARC to guarantee bounded memory behavior.

---

## Build Flag

```bash
adesh build --embedded
```

- Heap allocator disabled.
- ARC (`share`, `Rc`, `Arc`, `weak`) disabled.
- Stack + arena only; regions are allowed.
- Compiler errors on any heap allocation, ARC use, or weak reference.

---

## Allowed Patterns

- Plain stack data (primitives, structs, SSO/SAO values).
- Region-backed arenas for bulk allocations.
- Borrowing for safe aliasing (compile-time only; zero runtime cost).
- Unsafe blocks for low-level access when necessary (driver/FFI), with explicit `alloc`/`free`.

```adesh
region Frame {
    let header = PacketHeader();   // arena
    let payload = [0, 1, 2, 3];    // SAO
    process(&header, &payload);
} // bulk free
```

---

## Forbidden Operations (Compile-Time Errors)

- `share x` or any ARC/weak usage.
- `new`, `malloc`, or other heap allocators.
- Capturing region references that escape the region.
- APIs that lazily allocate on the heap.

---

## FFI Safety Checklist

- Pass stack/arena-backed buffers; never pass heap pointers.
- Use fixed-size structs and SAO arrays for deterministic layouts.
- Keep lifetimes inside the call boundary; do not store arena pointers in foreign code.
- Validate alignment and endianness explicitly.

---

## Debug vs Release in Embedded

| Build | Behavior |
| ----- | -------- |
| Debug | Borrow counters, poisoning for use-after-free, arena leak checks |
| Release | No metadata; stack + arena only; drops are deterministic |

---

## Sample Commands

```bash
# Build and run with arena-only model
adesh build --embedded
adesh run examples/embedded/embedded_safe.adesh --embedded
```

## Final Design Statement

> AdeshLang uses deterministic, GC-free memory management based on ownership, borrowing, explicit ARC, and region-based allocation, delivering predictable performance and strong safety guarantees across interpreter, JIT, AOT, WASM, and embedded targets.

