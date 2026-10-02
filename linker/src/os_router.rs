//! OS API Router — automatically routes undefined symbols to the correct system DLL
//! for each target platform, based on the actual symbol databases for each OS.
//!
//! This is the single authoritative source for DLL/library routing in the linker.
//! Replaces all ad-hoc prefix-heuristic checks scattered across resolver.rs,
//! linker.rs, and pe/import.rs.
//!
//! Design:
//!  1. After all archives are resolved, call `OsApiRouter::classify()` on each remaining
//!     undefined symbol.
//!  2. `classify()` returns `SymbolRoute` — either a DLL import, a CRT stub, an intrinsic,
//!     or a hard undefined error.
//!  3. The PE emitter consumes the `DllImport` routes directly for the import table.

use crate::target::{ObjectFormat, Target};

/// Where to route an unresolved symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolRoute {
    /// Symbol lives in a Windows DLL — add to import table.
    DllImport { dll: &'static str, name: String },
    /// Symbol is a compiler intrinsic — emit inline code.
    Intrinsic,
    /// Symbol is an internal Rust/Adesh runtime detail — skip (dead code).
    InternalRuntime,
    /// Symbol is an MSVC/PE linker special symbol — synthesize stub.
    LinkerSynthesized,
    /// Symbol is a CRT startup special — synthesize startup thunk.
    CrtStartup,
    /// Truly undefined — should be a link error.
    Undefined,
}

/// OS API Router — classifies an unresolved symbol for a given target.
pub struct OsApiRouter;

impl OsApiRouter {
    /// Classify an unresolved symbol for the given target.
    /// Returns the `SymbolRoute` that the linker should follow.
    pub fn classify(raw_name: &str, target: &Target) -> SymbolRoute {
        match target.format {
            ObjectFormat::Pe => Self::classify_windows(raw_name),
            ObjectFormat::Elf => Self::classify_elf(raw_name, target),
            ObjectFormat::MachO => Self::classify_macho(raw_name),
            _ => SymbolRoute::Undefined,
        }
    }

    /// Returns true if this symbol is an internal Rust/Adesh runtime detail
    /// that should never appear in a DLL import table.
    pub fn is_internal(name: &str) -> bool {
        // Rust mangled symbols: legacy (_ZN... / ZN...) and v0 RFC 2603 (_R... / R_...)
        if name.starts_with("_ZN")
            || name.starts_with("ZN")
            || name.starts_with("_R")
            || name.starts_with("R_")
        {
            return true;
        }
        // MSVC mangled (starts with ??)
        if name.starts_with("??") {
            return true;
        }
        // Rust/adesh internal prefixes and compiler alloc shims
        if name.starts_with("__rust")
            || name.starts_with("___rust")
            || name.starts_with("rust_")
            || name.starts_with("_rust_")
            || name.contains("__rust_")
            || name.contains("___rust")
            || name.contains("rust_eh_")
            || name.contains("rust_begin_unwind")
            || name.contains("rust_panic")
            || name.starts_with("anon.")
        {
            return true;
        }
        // Rust closure/const name-mangling markers
        if name.contains("$u7b$")
            || name.contains("$LT$")
            || name.contains("$GT$")
            || name.contains("..")
        {
            return true;
        }
        // Linker-generated or object-file private symbols
        if name.starts_with('$') || name.starts_with('.') || name.starts_with("__func__") {
            return true;
        }
        false
    }

    /// Returns true if an *undefined* symbol is a toolchain-internal detail that
    /// may be stubbed without changing program behaviour (mangled Rust/MSVC
    /// names, RTTI/EH tables and object-private symbols that are only ever
    /// address-taken from dead code).
    ///
    /// Adesh runtime entry points (`aot_*`, `adesh_*`) are deliberately excluded.
    /// They are required application symbols: when the runtime library does not
    /// define one, the link must fail instead of silently producing a binary
    /// that faults on the first call.
    pub fn is_stubbable_internal(name: &str) -> bool {
        if Self::is_adesh_runtime_symbol(name) {
            return false;
        }
        Self::is_internal(name)
    }

    /// Returns true for Adesh compiler/runtime ABI entry points.
    pub fn is_adesh_runtime_symbol(name: &str) -> bool {
        let clean = name
            .strip_prefix("__imp_")
            .or_else(|| name.strip_prefix("_imp_"))
            .unwrap_or(name);
        clean.starts_with("aot_") || clean.starts_with("adesh_")
    }

    // ─── Windows / PE ──────────────────────────────────────────────────────────

    fn classify_windows(raw: &str) -> SymbolRoute {
        // Strip import prefixes to get the base symbol name
        let name = raw
            .strip_prefix("__imp_")
            .or_else(|| raw.strip_prefix("_imp_"))
            .unwrap_or(raw);
        let clean = name.trim_start_matches('_');

        // ── 1. Internal / dead-code symbols — never become imports ──────────
        if Self::is_internal(raw) || Self::is_internal(name) || Self::is_internal(clean) {
            return SymbolRoute::InternalRuntime;
        }

        // ── 2. Compiler intrinsics ───────────────────────────────────────────
        if crate::intrinsics::IntrinsicsEngine::is_intrinsic(raw)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(name)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(clean)
        {
            return SymbolRoute::Intrinsic;
        }

        // ── 3. PE/MSVC/ELF linker-synthesized specials ───────────────────────
        if matches!(
            clean,
            "fltused"
                | "tls_index"
                | "tls_used"
                | "load_config_used"
                | "ImageBase"
                | "dso_handle"
                | "ehdr_start"
        ) || matches!(
            name,
            "_fltused"
                | "_tls_index"
                | "_tls_used"
                | "_load_config_used"
                | "__ImageBase"
                | "__dso_handle"
                | "__ehdr_start"
        ) {
            return SymbolRoute::LinkerSynthesized;
        }

        // ── 4. CRT startup thunks ────────────────────────────────────────────
        if matches!(
            clean,
            "mainCRTStartup" | "WinMainCRTStartup" | "wmainCRTStartup" | "wWinMainCRTStartup"
        ) || matches!(name, "mainCRTStartup" | "_mainCRTStartup")
        {
            return SymbolRoute::CrtStartup;
        }

        // ── 5. Windows system DLLs — route by actual symbol database ─────────
        if let Some(dll) = Self::windows_dll_for(name) {
            return SymbolRoute::DllImport {
                dll,
                name: name.to_string(),
            };
        }
        if let Some(dll) = Self::windows_dll_for(clean) {
            return SymbolRoute::DllImport {
                dll,
                name: clean.to_string(),
            };
        }

        SymbolRoute::Undefined
    }

    /// Maps a symbol name (with or without leading `_`, or `__imp_`) to the Windows DLL
    /// that exports it. Returns `None` if the symbol is not a known Windows API / CRT export.
    ///
    /// Priority: exact match tables first, then prefix rules, then CRT fallback.
    pub fn windows_dll_for(sym: &str) -> Option<&'static str> {
        let raw = sym
            .strip_prefix("__imp_")
            .or_else(|| sym.strip_prefix("_imp_"))
            .unwrap_or(sym);
        let clean = raw.trim_start_matches('_');

        if clean.is_empty() || Self::is_internal(raw) || Self::is_internal(clean) {
            return None;
        }

        // ── Exact-match: ntdll.dll ──────────────────────────────────────────
        const NTDLL_EXACT: &[&str] = &[
            "NtReadFile",
            "NtWriteFile",
            "NtCreateNamedPipeFile",
            "NtOpenFile",
            "RtlNtStatusToDosError",
            "NtClose",
            "NtSetInformationFile",
            "NtQueryInformationFile",
            "NtCreateFile",
            "NtDeviceIoControlFile",
            "NtCancelIoFileEx",
            "NtQueryVolumeInformationFile",
        ];
        if NTDLL_EXACT.contains(&clean) || NTDLL_EXACT.contains(&raw) {
            return Some("ntdll.dll");
        }

        // ── Exact-match: kernelbase.dll ──────────────────────────────────────
        const KERNELBASE_EXACT: &[&str] =
            &["WaitOnAddress", "WakeByAddressAll", "WakeByAddressSingle"];
        if KERNELBASE_EXACT.contains(&clean) || KERNELBASE_EXACT.contains(&raw) {
            return Some("kernelbase.dll");
        }

        // ── Exact-match: userenv.dll ─────────────────────────────────────────
        if clean == "GetUserProfileDirectoryW" || raw == "GetUserProfileDirectoryW" {
            return Some("userenv.dll");
        }

        // ── Exact-match: bcryptprimitives.dll ────────────────────────────────
        if clean == "ProcessPrng" || raw == "ProcessPrng" {
            return Some("bcryptprimitives.dll");
        }

        // ── Exact-match: ws2_32.dll — socket functions ───────────────────────
        const WS2_EXACT: &[&str] = &[
            "socket",
            "connect",
            "bind",
            "listen",
            "accept",
            "send",
            "recv",
            "sendto",
            "recvfrom",
            "closesocket",
            "shutdown",
            "getaddrinfo",
            "freeaddrinfo",
            "getnameinfo",
            "getpeername",
            "getsockname",
            "select",
            "ioctlsocket",
            "setsockopt",
            "getsockopt",
            "htons",
            "ntohs",
            "htonl",
            "ntohl",
            "inet_ntop",
            "inet_pton",
            "inet_addr",
            "inet_ntoa",
            "gethostname",
            "GetHostNameW",
        ];
        if WS2_EXACT.contains(&clean) || WS2_EXACT.contains(&raw) {
            return Some("ws2_32.dll");
        }

        // ── Exact-match: KERNEL32.dll — misc ────────────────────────────────
        const KERNEL32_EXACT: &[&str] = &[
            "ExitProcess",
            "Sleep",
            "SwitchToThread",
            "DeviceIoControl",
            "CancelIo",
            "CancelIoEx",
            "WideCharToMultiByte",
            "MultiByteToWideChar",
            "FormatMessageW",
            "FormatMessageA",
            "CompareStringOrdinal",
            "CompareStringW",
            "CopyFileExW",
            "CopyFileW",
            "CopyFileA",
            "UpdateProcThreadAttribute",
            "DeleteProcThreadAttributeList",
            "InitializeProcThreadAttributeList",
            "GetModuleHandleA",
            "GetModuleHandleW",
            "LoadLibraryA",
            "LoadLibraryW",
            "LoadLibraryExA",
            "LoadLibraryExW",
            "FreeLibrary",
            "GetProcAddress",
            "GetCommandLineA",
            "GetCommandLineW",
            "GetProcessId",
            "GetCurrentProcess",
            "GetCurrentThread",
            "GetCurrentProcessId",
            "GetCurrentThreadId",
            "GetLastError",
            "SetLastError",
            "GetSystemInfo",
            "GetNativeSystemInfo",
            "GetSystemTimePreciseAsFileTime",
            "GetSystemTimeAsFileTime",
            "CreateThread",
            "SuspendThread",
            "ResumeThread",
            "WaitForSingleObject",
            "WaitForSingleObjectEx",
            "WaitForMultipleObjects",
            "WaitForMultipleObjectsEx",
            "CreateMutexA",
            "CreateMutexW",
            "ReleaseMutex",
            "CreateEventA",
            "CreateEventW",
            "SetEvent",
            "ResetEvent",
            "CreateWaitableTimerExW",
            "SetWaitableTimer",
            "CreatePipe",
            "CreateNamedPipeW",
            "WriteFileEx",
            "ReadFileEx",
            "FlushFileBuffers",
            "SetThreadStackGuarantee",
            "AddVectoredExceptionHandler",
            "RemoveVectoredExceptionHandler",
            "RtlCaptureContext",
            "RtlLookupFunctionEntry",
            "RtlVirtualUnwind",
            "UnhandledExceptionFilter",
            "SetUnhandledExceptionFilter",
            "GetWindowsDirectoryW",
            "GetSystemDirectoryW",
            "GetTempPathW",
            "GetEnvironmentStringsW",
            "FreeEnvironmentStringsW",
            "GetEnvironmentVariableW",
            "SetEnvironmentVariableW",
            "HeapReAlloc",
            "HeapSize",
            "TlsAlloc",
            "TlsFree",
            "TlsGetValue",
            "TlsSetValue",
            "LocalAlloc",
            "LocalFree",
            "LocalReAlloc",
            "GlobalAlloc",
            "GlobalFree",
            "VirtualAllocEx",
            "VirtualFreeEx",
            "VirtualProtect",
            "VirtualQuery",
            "FlushInstructionCache",
            "CreateSymbolicLinkW",
            "CreateHardLinkW",
            "GetFileInformationByHandle",
            "GetFileInformationByHandleEx",
            "SetFileInformationByHandle",
            "GetFinalPathNameByHandleW",
            "GetFullPathNameW",
            "GetFileAttributesW",
            "SetFileAttributesW",
            "GetFileSizeEx",
            "SetFilePointerEx",
            "MoveFileExW",
            "DeleteFileW",
            "RemoveDirectoryW",
            "FindFirstFileExW",
            "FindNextFileW",
            "FindClose",
            "CreateDirectoryW",
            "GetCurrentDirectoryW",
            "SetCurrentDirectoryW",
            "GetModuleFileNameW",
            "GetOverlappedResult",
            "GetOverlappedResultEx",
            "DuplicateHandle",
            "GetConsoleMode",
            "SetConsoleMode",
            "GetConsoleOutputCP",
            "WriteConsoleW",
            "WriteConsoleA",
            "ReadConsoleW",
            "ReadConsoleA",
            "GetStdHandle",
            "SetStdHandle",
            "GetFileType",
            "QueryPerformanceCounter",
            "QueryPerformanceFrequency",
            "CreateProcessW",
            "CreateProcessA",
            "GetExitCodeProcess",
            "TerminateProcess",
            "SetHandleInformation",
            "GetHandleInformation",
            "SleepEx",
            "lstrlenW",
            "lstrlenA",
            "LockFileEx",
            "UnlockFile",
            "LockFile",
        ];
        if KERNEL32_EXACT.contains(&clean) || KERNEL32_EXACT.contains(&raw) {
            return Some("KERNEL32.dll");
        }

        // ── Exact-match: advapi32.dll ────────────────────────────────────────
        const ADVAPI32_EXACT: &[&str] = &[
            "ProcessPrng",
            "OpenProcessToken",
            "GetTokenInformation",
            "SetTokenInformation",
            "AdjustTokenPrivileges",
            "LookupPrivilegeValueW",
            "SystemFunction036",
        ];
        if ADVAPI32_EXACT.contains(&clean) || ADVAPI32_EXACT.contains(&raw) {
            return Some("advapi32.dll");
        }

        // ── Exact-match: user32.dll ──────────────────────────────────────────
        const USER32_EXACT: &[&str] = &[
            "MessageBoxW",
            "MessageBoxA",
            "MessageBoxExW",
            "GetDesktopWindow",
            "GetForegroundWindow",
            "ShowWindow",
            "UpdateWindow",
            "DestroyWindow",
            "CreateWindowExW",
            "CreateWindowExA",
            "DefWindowProcW",
            "DefWindowProcA",
            "RegisterClassExW",
            "RegisterClassExA",
            "LoadCursorW",
            "LoadIconW",
            "TranslateMessage",
            "DispatchMessageW",
            "DispatchMessageA",
            "PeekMessageW",
            "PeekMessageA",
            "GetMessageW",
            "PostQuitMessage",
            "PostMessageW",
            "BeginPaint",
            "EndPaint",
            "GetClientRect",
            "GetWindowRect",
        ];
        if USER32_EXACT.contains(&clean) || USER32_EXACT.contains(&raw) {
            return Some("user32.dll");
        }

        // ── Exact-match: bcrypt.dll ──────────────────────────────────────────
        const BCRYPT_EXACT: &[&str] = &[
            "BCryptGenRandom",
            "BCryptOpenAlgorithmProvider",
            "BCryptCloseAlgorithmProvider",
            "BCryptCreateHash",
            "BCryptDestroyHash",
            "BCryptHashData",
            "BCryptFinishHash",
        ];
        if BCRYPT_EXACT.contains(&clean) || BCRYPT_EXACT.contains(&raw) {
            return Some("bcrypt.dll");
        }

        // ── Exact-match: dbghelp.dll ─────────────────────────────────────────
        const DBGHELP_EXACT: &[&str] = &[
            "SymInitialize",
            "SymCleanup",
            "SymFromAddr",
            "StackWalk64",
            "MiniDumpWriteDump",
        ];
        if DBGHELP_EXACT.contains(&clean) || DBGHELP_EXACT.contains(&raw) {
            return Some("dbghelp.dll");
        }

        // ── Prefix rules: ntdll.dll ──────────────────────────────────────────
        if clean.starts_with("Nt") || clean.starts_with("Zw") || clean.starts_with("RtlNtStatus") {
            return Some("ntdll.dll");
        }

        // ── Prefix rules: KERNEL32.dll ───────────────────────────────────────
        if clean.starts_with("Rtl")         // RtlCaptureContext, RtlVirtualUnwind…
            || clean.starts_with("Heap")    // HeapAlloc, HeapFree…
            || clean.starts_with("Virtual") // VirtualAlloc, VirtualFree…
            || clean.starts_with("Tls")     // TlsAlloc, TlsFree…
            || clean.starts_with("Get")
            || clean.starts_with("Set")
            || clean.starts_with("Create")
            || clean.starts_with("Close")
            || clean.starts_with("Read")
            || clean.starts_with("Write")
            || clean.starts_with("Delete")
            || clean.starts_with("Move")
            || clean.starts_with("Find")
            || clean.starts_with("Local")
            || clean.starts_with("Global")
            || clean.starts_with("Load")
            || clean.starts_with("Free")
            || clean.starts_with("Sleep")
            || clean.starts_with("Switch")
            || clean.starts_with("Query")
            || clean.starts_with("Add")
            || clean.starts_with("Remove")
            || clean.starts_with("Duplicate")
            || clean.starts_with("Flush")
            || clean.starts_with("WaitFor")
            || clean.starts_with("Terminate")
            || clean.starts_with("FormatMessage")
            || clean.starts_with("WideChar")
            || clean.starts_with("MultiByte")
            || clean.starts_with("SystemTime")
            || clean.starts_with("FileTime")
            || clean.starts_with("DeviceIoControl")
            || clean.starts_with("CancelIo")
            || clean.starts_with("Initialize")
            || clean.starts_with("Enter")
            || clean.starts_with("Leave")
        {
            return Some("KERNEL32.dll");
        }

        // ── Prefix rules: ws2_32.dll ─────────────────────────────────────────
        if clean.starts_with("WSA")
            || clean.starts_with("GetAddrInfo")
            || clean.starts_with("FreeAddrInfo")
        {
            return Some("ws2_32.dll");
        }

        // ── Prefix rules: advapi32.dll ───────────────────────────────────────
        if clean.starts_with("Reg")
            || clean.starts_with("Crypt")
            || clean.starts_with("BCrypt")
            || clean.starts_with("SystemFunction")
        {
            return Some("advapi32.dll");
        }

        // ── Prefix rules: user32.dll ─────────────────────────────────────────
        if clean.starts_with("MessageBox")
            || clean.starts_with("GetDesktop")
            || clean.starts_with("ShowWindow")
        {
            return Some("user32.dll");
        }

        // ── CRT functions → msvcrt.dll ───────────────────────────────────────
        const MSVCRT_EXACT: &[&str] = &[
            // stdio
            "printf",
            "fprintf",
            "sprintf",
            "snprintf",
            "vprintf",
            "vfprintf",
            "vsprintf",
            "vsnprintf",
            "puts",
            "putchar",
            "putchar_unlocked",
            "gets",
            "getchar",
            "fopen",
            "fclose",
            "fread",
            "fwrite",
            "fseek",
            "ftell",
            "fflush",
            "feof",
            "ferror",
            "clearerr",
            "fgetc",
            "fputc",
            "fgets",
            "fputs",
            "freopen",
            "tmpfile",
            "tmpnam",
            "scanf",
            "fscanf",
            "sscanf",
            "perror",
            "__acrt_iob_func",
            "__stdio_common_vfprintf",
            "__stdio_common_vsprintf",
            "__stdio_common_vsscanf",
            // stdlib
            "malloc",
            "calloc",
            "realloc",
            "free",
            "exit",
            "abort",
            "_exit",
            "atexit",
            "at_quick_exit",
            "getenv",
            "system",
            "atoi",
            "atol",
            "atoll",
            "atof",
            "strtol",
            "strtoul",
            "strtoll",
            "strtoull",
            "strtod",
            "strtof",
            "qsort",
            "bsearch",
            "abs",
            "labs",
            "llabs",
            "rand",
            "srand",
            // string
            "strlen",
            "strcpy",
            "strncpy",
            "strcat",
            "strncat",
            "strcmp",
            "strncmp",
            "strcasecmp",
            "strncasecmp",
            "strchr",
            "strrchr",
            "strstr",
            "strtok",
            "strtok_r",
            "strdup",
            "strndup",
            // memory
            "memcpy",
            "memmove",
            "memset",
            "memcmp",
            "memchr",
            // math
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "atan2",
            "sinh",
            "cosh",
            "tanh",
            "asinh",
            "acosh",
            "atanh",
            "exp",
            "exp2",
            "log",
            "log2",
            "log10",
            "pow",
            "sqrt",
            "cbrt",
            "hypot",
            "floor",
            "ceil",
            "round",
            "trunc",
            "fabs",
            "fmod",
            "remainder",
            "fmin",
            "fmax",
            "fma",
            "copysign",
            "ldexp",
            "frexp",
            "modf",
            "scalbn",
            "sinf",
            "cosf",
            "tanf",
            "asinf",
            "acosf",
            "atanf",
            "atan2f",
            "sinhf",
            "coshf",
            "tanhf",
            "expf",
            "exp2f",
            "logf",
            "log2f",
            "log10f",
            "powf",
            "sqrtf",
            "cbrtf",
            "hypotf",
            "floorf",
            "ceilf",
            "roundf",
            "truncf",
            "fabsf",
            "fmodf",
            "fminf",
            "fmaxf",
            "copysignf",
            // time
            "time",
            "clock",
            "difftime",
            "mktime",
            "gmtime",
            "localtime",
            "strftime",
            "asctime",
            "ctime",
            // io / locale
            "setlocale",
            "islower",
            "isupper",
            "isdigit",
            "isalpha",
            "isalnum",
            "isspace",
            "ispunct",
            "tolower",
            "toupper",
            // MSVC CRT specials
            "__chkstk",
            "___chkstk_ms",
            "__CxxFrameHandler3",
            "__CxxFrameHandler4",
            "_CxxThrowException",
            "CxxThrowException",
            "_purecall",
            "purecall",
            "__p__argc",
            "__p__argv",
            "__p__wenviron",
            "acrt_iob_func",
            "stdio_common_vfprintf",
            "stdio_common_vsprintf",
            "chkstk",
            "CxxFrameHandler3",
            "CxxFrameHandler4",
        ];
        if MSVCRT_EXACT.contains(&clean) || MSVCRT_EXACT.contains(&raw) {
            return Some("msvcrt.dll");
        }

        // ── Prefix rules: msvcrt.dll ─────────────────────────────────────────
        if clean.starts_with("acrt")
            || clean.starts_with("stdio")
            || clean.starts_with("Cxx")
            || clean.starts_with("crt")
            || clean.starts_with("chkstk")
            || raw.starts_with("__acrt")
            || raw.starts_with("__stdio")
            || raw.starts_with("__Cxx")
            || raw.starts_with("_Cxx")
            || raw.starts_with("__crt")
            || raw.starts_with("__chkstk")
        {
            return Some("msvcrt.dll");
        }

        None
    }

    // ─── Linux / ELF ───────────────────────────────────────────────────────────

    fn classify_elf(raw: &str, _target: &Target) -> SymbolRoute {
        let clean = raw.trim_start_matches('_');

        if Self::is_internal(raw) {
            return SymbolRoute::InternalRuntime;
        }
        if crate::intrinsics::IntrinsicsEngine::is_intrinsic(raw) {
            return SymbolRoute::Intrinsic;
        }

        // On Linux, libc symbols are resolved via -lc (shared link), not DLL import tables.
        // Return `Undefined` so the ELF writer can handle them via dynamic entries.
        let _ = clean;
        SymbolRoute::Undefined
    }

    // ─── macOS / Mach-O ────────────────────────────────────────────────────────

    fn classify_macho(raw: &str) -> SymbolRoute {
        if Self::is_internal(raw) {
            return SymbolRoute::InternalRuntime;
        }
        if crate::intrinsics::IntrinsicsEngine::is_intrinsic(raw) {
            return SymbolRoute::Intrinsic;
        }
        SymbolRoute::Undefined
    }
}
