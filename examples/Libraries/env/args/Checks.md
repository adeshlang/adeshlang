# 08-12-2025
- args_basics.adesh , working in interpreter , jit , aot, vm
- args_parse.adesh , working in interpreter and jit
- args_simple.adesh , working in I,J,A,V

# 01-08-2026
- Examples migrated to `import Env;` (canonical API) instead of bare builtin calls.
- Added `Env.argc()`, `Env.argv()`, `Env.argsCount()`, `Env.execName()`, `Env.arg(i)` (0-based),
  `Env.env(key, default)`, `Env.envFromFile()`, `Env.envFileGet()`, `Env.envRuntimeLoad()`,
  `Env.envRuntimeGet()`, `Env.envRuntimeHas()`, `Env.envRuntimeAll()` to `src/stdlib/Env.adesh`.
- Bare builtins (`argc`, `argv`, `arg`, `env`, ...) still work but direct use is discouraged.
- All examples verified exit=0 in the interpreter (debug build).
