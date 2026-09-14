# AdeshLang Production-Grade IO Subsystem Architecture & Reference

The `IO` standard library module provides a zero-GC, ownership-safe, borrowing-safe, UTF-8 first, deterministic streaming subsystem for AdeshLang.

---

## Core Principles

- **Zero GC Overhead**: All streams use deterministic stack and arena allocation.
- **Backend Independence**: Identical behavior on Interpreter, VM, JIT, Native JIT, AOT, and WASM.
- **UTF-8 Native**: Character and line reading are natively UTF-8 compliant.
- **Configurable Endianness**: Full primitive serialization with `"little"` and `"big"` endian support.

---

## Architecture Diagram

```
+---------------------------------------------------------------------------------+
|                                 IO Namespace                                    |
|   IO.stdin() | IO.stdout() | IO.stderr() | IO.print() | IO.println() | IO.readLine()|
+---------------------------------------------------------------------------------+
                                         |
     +-----------------------------------+-----------------------------------+
     |                                                                       |
+----+----+                                                             +----+----+
| Reader  | (abstract)                                                  | Writer  | (abstract)
+----+----+                                                             +----+----+
     |                                                                       |
     +---------------------------+---------------------------+               |
     |                           |                           |               |
+----+-------------+    +--------+--------+    +-------------+----+    +-----+------+
| BufferedReader   |    | BinaryReader    |    | MemoryReader     |    | Pipe       |
+------------------+    +-----------------+    +------------------+    +------------+
     |                           |                           |               |
+----+-------------+    +--------+--------+    +-------------+----+    +-----+------+
| BufferedWriter   |    | BinaryWriter    |    | MemoryWriter     |    | NullWriter |
+------------------+    +-----------------+    +------------------+    +------------+
```

---

## API Reference

### 1. Errors and Results
- `IOError(kind: string, message: string)`
- `IOResult(val, err)`: `isOk()`, `isErr()`, `unwrap()`, `error()`

### 2. Stream Interfaces & Classes
- `Reader`: `read(buf, off, len)`, `readLine()`, `readLines()`, `readToEnd()`, `readUntil(delim)`
- `Writer`: `write(buf, off, len)`, `writeLine(s)`, `writeLines(lines)`, `flush()`
- `ReadWriter`: Wraps Reader and Writer.
- `SeekableStream`: `seek(offset, whence)`, `rewind()`, `skip(n)`, `position()`, `length()`, `remaining()`

### 3. Buffering
- `BufferedReader(reader, capacity)`: `fillBuffer()`, `peek()`, `consume()`, `clear()`
- `BufferedWriter(writer, capacity)`: `reserve()`, `flush()`, `clear()`

### 4. Binary Stream Serialization
- `BinaryReader(reader, endianness)`: `readU8()`, `readU16()`, `readU32()`, `readU64()`, `readI8()`, `readI16()`, `readI32()`, `readI64()`, `readF32()`, `readF64()`, `readBool()`, `readChar()`, `readString(len)`
- `BinaryWriter(writer, endianness)`: `writeU8()`, `writeU16()`, `writeU32()`, `writeU64()`, `writeI8()`, `writeI16()`, `writeI32()`, `writeI64()`, `writeF32()`, `writeF64()`, `writeBool()`, `writeChar()`, `writeString(s)`

### 5. Memory Buffers & Streams
- `MemoryReader(data)`: In-memory byte slice reader.
- `MemoryWriter()`: Dynamic in-memory byte slice writer.
- `ByteBuffer(capacity)`: Byte array with read/write positions.
- `StringBuffer()`: Efficient string builder.

### 6. Stream Adapters & Combinators
- `Pipe()`: Bounded reader-writer pipe.
- `NullReader()`, `NullWriter()`: Discard / empty streams.
- `TeeReader(reader, writer)`, `TeeWriter(w1, w2)`: Multi-stream branching.
- `LimitedReader(reader, n)`: Byte bounded reader.
- `CountingReader(reader)`, `CountingWriter(writer)`: Stream byte metrics.
- Utility functions: `copy(src, dst)`, `copyN(src, dst, n)`, `pipe()`, `tee(reader, writer)`, `limit(reader, n)`, `repeat(byte_val, count)`, `discard(reader)`, `concat(readers)`, `chain(readers)`

---

## Backend Compatibility Matrix

| Feature | Interpreter | Bytecode VM | JIT | Native JIT | LLVM AOT | WASM |
|---|---|---|---|---|---|---|
| Reader / Writer | YES | YES | YES | YES | YES | YES |
| BufferedReader / Writer | YES | YES | YES | YES | YES | YES |
| Binary I/O & Endianness | YES | YES | YES | YES | YES | YES |
| In-Memory Streams | YES | YES | YES | YES | YES | YES |
| Stream Pipes / Adapters | YES | YES | YES | YES | YES | YES |
| Console I/O | YES | YES | YES | YES | YES | YES |
