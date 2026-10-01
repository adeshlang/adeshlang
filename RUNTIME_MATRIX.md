# Adesh Standalone Runtime Matrix

**Generated:** 2026-10-01  
**Scope:** Standalone Native Runtime (`crates/adesh-runtime`) Features & OS Mapping

---

## 1. Runtime Modes

Adesh binaries can be compiled with selectable runtime profiles:

```bash
adesh build --runtime=minimal     # Size-optimized minimal runtime (no networking/crypto)
adesh build --runtime=static      # Full statically-linked runtime (default for native executables)
adesh build --runtime=dynamic     # Linked against shared adesh_rt.dll / libadesh_rt.so
adesh build --runtime=none        # Bare-metal / embedded no_std mode
```

---

## 2. Platform Feature Mapping

| Runtime Feature | Windows Native | Linux Native | macOS Native | Embedded / Bare-Metal | WASM / WASI |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Virtual Memory** | `VirtualAlloc` / `VirtualFree` | `mmap` / `munmap` | `mmap` / `munmap` | Fixed Static Heap / Arena | `memory.grow` |
| **OS Threads** | `CreateThread` / `WaitForSingleObject` | `pthread_create` / `pthread_join` | `pthread_create` / `pthread_join` | No threads / RTOS task | Web Workers / WASI Threads |
| **Atomics & Futex** | `WaitOnAddress` / `WakeByAddressSingle` | `sys_futex` (`FUTEX_WAIT`/`WAKE`) | `__ulock_wait` / `__ulock_wake` | Primitive spinlocks / CLI-SEI | `atomic.wait` / `atomic.notify` |
| **High-Res Timers** | `QueryPerformanceCounter` | `clock_gettime(CLOCK_MONOTONIC)` | `mach_absolute_time` | SysTick timer / cycle counter | `clock_time_get` |
| **Dynamic Libraries** | `LoadLibraryW` / `GetProcAddress` | `dlopen` / `dlsym` | `dlopen` / `dlsym` | Not Supported | Not Supported |
| **File System** | Win32 File APIs (`CreateFileW`) | POSIX `open`, `read`, `write` | POSIX `open`, `read`, `write` | Flash memory / LittleFS | `fd_read` / `fd_write` |
| **Networking** | `Winsock2` (`WSAStartup`, `socket`) | Berkeley Sockets (`sys/socket.h`) | Berkeley Sockets (`sys/socket.h`) | LwIP / Ethernet driver | WASI Sockets (0.2) |
| **Process Control** | `CreateProcessW` / `TerminateProcess` | `fork` / `execve` / `waitpid` | `posix_spawn` / `waitpid` | Reset vector | Not Supported |
| **Terminal / ANSI** | Win32 Console Mode / Virtual Term | POSIX termios / ANSI sequences | POSIX termios / ANSI sequences | UART / Serial console | Canvas / DOM Console |

---

## 3. C ABI Export Catalog (`adesh_rt_*`)

All runtime functions follow the stable C ABI for seamless static/dynamic linking without symbol name mangling:

* **Memory**: `adesh_rt_alloc(size, align)`, `adesh_rt_free(ptr, size, align)`, `adesh_rt_realloc(ptr, old_size, new_size, align)`.
* **ARC Reference Counting**: `adesh_rt_retain(ptr)`, `adesh_rt_release(ptr, drop_fn)`.
* **Value System**: `aot_store_value(val) -> handle`, `aot_get_value(handle)`, `aot_remove_value(handle)`, `unpack_aot_arg(raw)`.
* **I/O & Output**: `adesh_rt_print(val_handle)`, `adesh_rt_eprint(val_handle)`, `adesh_rt_println()`.
* **Strings**: `adesh_rt_string_concat(h1, h2)`, `adesh_rt_string_len(h)`, `adesh_rt_string_slice(h, start, end)`.
* **Collections**: `adesh_rt_array_new(cap)`, `adesh_rt_array_push(arr_h, val_h)`, `adesh_rt_object_new()`, `adesh_rt_object_insert(obj_h, key, val_h)`.
* **Panic & Diagnostics**: `adesh_rt_panic(msg_ptr, len)`, `adesh_rt_assert_failed(file, line, col, msg)`.
