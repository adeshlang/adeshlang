# AdeshLang `Compression` Standard Library

> World-Class Lossless Compression, Streaming IO, Disk-to-Disk Processing, Adaptive Codecs & Secure Archives for AdeshLang.

## Table of Contents

- [Overview](#overview)
- [Installation & Import](#installation--import)
- [Quick Start](#quick-start)
- [File Compression & Decompression (Disk-to-Disk)](#file-compression--decompression-disk-to-disk)
- [ZIP Archive Creation & Decompression](#zip-archive-creation--decompression)
- [Available Codecs](#available-codecs)
- [Adaptive Compression](#adaptive-compression)
- [Seekable Random-Access Format](#seekable-random-access-format)
- [Security & Limits](#security--limits)
- [Examples Directory](#examples-directory)

---

## Overview

The `Compression` standard library delivers production-grade, memory-safe compression capabilities for AdeshLang. Built on zero-GC Rust primitives (`flate2`, `zstd`, `brotli`, `lz4_flex`, `lzma-rs`, `zip`, `tar`), it supports multi-gigabyte file streaming, pre-trained dictionaries, multi-threaded parallel execution, format auto-detection, and safeguards against decompression bombs and archive path traversal attacks.

---

## Installation & Import

Import in any AdeshLang script:

```adesh
import Compression;
import Encoding;
```

---

## File Compression & Decompression (Disk-to-Disk)

To process large files without loading the full content into memory:

### 1. File Compression

```adesh
import Compression;

// Stream compress a file from disk to disk (64KB block buffer)
let inputPath = "server.log";
let outputPath = "server.log.zst";

let success = Compression.compressFile(inputPath, outputPath, "zstd", 5);
print("File compressed successfully:", success);
```

### 2. File Decompression

```adesh
import Compression;

// Stream decompress a file from disk to disk
let compressedPath = "server.log.zst";
let restoredPath = "server_restored.log";

let success = Compression.decompressFile(compressedPath, restoredPath, "zstd");
print("File decompressed successfully:", success);
```

### 3. Atomic File Compression

```adesh
import Compression;

// Writes to server.log.zst.tmp and renames atomically upon completion
Compression.compressFileAtomic("server.log", "server.log.zst", "zstd", 5);
```

---

## ZIP Archive Creation & Decompression

AdeshLang supports creating ZIP archives from in-memory contents OR direct disk file paths, as well as safe extraction:

### 1. ZIP File Creation from Disk Files

```adesh
import Compression;

let filesToZip = [
    { "name": "logs/server.log", "path": "server.log" },
    { "name": "data/db_dump.sql", "path": "db_dump.sql" }
];

let entryCount = Compression.createZipFile(filesToZip, "archive.zip");
print("Packed ZIP entries:", entryCount);
```

### 2. ZIP Archive Extraction on Disk

```adesh
import Compression;

// Safely extracts "archive.zip" to "output_directory/"
let extractedCount = Compression.extractZipFile("archive.zip", "output_directory");
print("Extracted ZIP files count:", extractedCount);
```

---

## Available Codecs

| Codec | Method Name | Decompress Method | Level Range | Recommended Use Case |
| :--- | :--- | :--- | :--- | :--- |
| **Zstd** | `Compression.zstd(data, level)` | `Compression.unzstd(data)` | 1 – 22 | General purpose, logs, RPC, storage |
| **Gzip** | `Compression.gzip(data, level)` | `Compression.gunzip(data)` | 1 – 9 | Legacy web, HTTP requests, unix tools |
| **Brotli** | `Compression.brotli(data, level)` | `Compression.unbrotli(data)` | 1 – 11 | Static web assets, JSON, HTML |
| **LZ4** | `Compression.lz4(data)` | `Compression.unlz4(data)` | Fast | High-throughput streaming, IPC |
| **XZ** | `Compression.xz(data)` | `Compression.unxz(data)` | Max | Software packages, firmware updates |
| **DEFLATE** | `Compression.deflate(data, level)` | `Compression.inflate(data)` | 1 – 9 | Raw RFC 1951 zip/image streams |
| **ZLIB** | `Compression.zlib(data, level)` | `Compression.unzlib(data)` | 1 – 9 | Raw RFC 1950 zlib streams |

---

## Security & Safeguards

1. **Decompression Bomb Protection**: Enforces max output memory limits (`maxOutputSize`).
2. **Path Traversal Defense**: Normalizes target paths and rejects `..` or root-relative paths escaping the target directory.
