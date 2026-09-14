# Filesystem Module

Unified filesystem API compatible with interpreter, JIT, AOT and WASM modes.

The `fs` object is globally available and provides filesystem operations.

## Namespaces

- `fs`: core operations
- `fs.async`: Promise-based async operations
- `fs.path`: path utilities
- `fs.vfs`: virtual filesystem (memory/sandbox polyfills)
- `fs.tx`: transactional operations
- `fs.snapshot`: create snapshots
- `fs.batch`: batch operations

## Core API

- `fs.read(path)` → string
- `fs.write(path, text)` → null
- `fs.writeAtomic(path, text)` → null
- `fs.delete(path)` → null
- `fs.copy(src, dst)` → number (bytes)
- `fs.move(src, dst)` → null
- `fs.exists(path)` → bool
- `fs.isFile(path)` → bool
- `fs.isDir(path)` → bool
- `fs.readDir(path)` → [string]
- `fs.mkdir(path)` → null
- `fs.tempFile(prefix)` → string
- `fs.tempDir(prefix)` → string
- `fs.lock(path)` → string (lock path)
- `fs.unlock(lockPath)` → null
- `fs.mmapRead(path)` → `RawArray("u8", ...)`
- `fs.crc32(data)` → `u32`
- `fs.sha256(data)` → hex string
- `fs.symlinkFile(src, link)` → null
- `fs.symlinkDir(src, link)` → null
- `fs.readLink(path)` → string
- `fs.watch(path, fn(event, path))` → id
- `fs.unwatch(id)` → null

## Path API

- `fs.path.join(a, b, ...)`
- `fs.path.basename(path)`
- `fs.path.dirname(path)`
- `fs.path.extname(path)`
- `fs.path.normalize(path)`

## Async API

- `fs.async.read(path)` → Promise<string>
- `fs.async.write(path, text)` → Promise<void>

## Transactions

- `fs.tx(fn(t){ t.write(p, s); t.rename(a,b); t.remove(p); t.commit(); })`
- `t.rollback()` restores best-effort using operation log.

## Snapshots

- `fs.snapshot(name)` → `{ name, files: { path: size } }`

## FSQL

- `fs.fsql(path, [ext])` → rows with `{name, ext, isFile, size}`

## Batch

- `fs.batch([{ op: "write", path, data }, { op: "read", path }, { op: "delete", path }])`

## Mode Support

- Interpreter/JIT/AOT: native OS-backed implementation
- WASM: polyfilled via in-memory operations and timers; avoid network unless feature `allow_internal_network` is enabled

## Examples

```adesh
import fs;

let p = "./examples/fs/demo.txt";
fs.write(p, "hello");
print(fs.read(p));
fs.writeAtomic(p, "world");
print(fs.sha256(fs.read(p)));

let id = fs.watch("examples/fs", fn(ev, path){
    print("changed: " + path);
});
fs.unwatch(id);

from fs import readDir, exists;
print(readDir("."));
print(exists("Cargo.toml"));

fs.tx(fn(t){
    let tmp = fs.tempFile("txn");
    t.write(tmp, "data");
    t.rollback();
});

let rows = fs.fsql("examples", "adesh");
print(rows);
```

## Performance Notes

- ARC-backed values and objects
- FastMaps for directory metadata and snapshots
- Zero-copy style `RawArray("u8", ...)` for byte pipelines
- Precompute path tables for AOT with `fs.snapshot`
- WASM uses timers and memory-based VFS polyfills

## Comparisons

- Rust: similar to `std::fs` plus helpers; atomic writes and hashing included
- Node.js: unified sync/async API via `fs` and `fs.async`
- Go: combines `os`, `path/filepath`, and `hash` conveniences under one namespace

## Best Practices

- Prefer `fs.writeAtomic` for durability
- Use `fs.lock` for simple cross-process coordination
- Guard watchers in production with debounce intervals
- Validate inputs and restrict to sandbox roots in untrusted contexts

## Mode Samples

- Interpreter: direct calls
- JIT: identical API, compiled hot paths
- AOT: use `fs.snapshot` to precompute file tables
- WASM: prefer `fs.async.*` and memory VFS
