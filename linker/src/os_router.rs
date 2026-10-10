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
use std::collections::HashSet;
use std::sync::LazyLock;

/// O(1) membership test for one exact DLL export table.
///
/// `clean` has its leading underscores stripped; `raw` keeps them (several
/// tables also list `__`-prefixed MSVC specials). Building the set once per
/// table removes the two linear scans per symbol the previous implementation
/// performed.
fn in_set(table: &HashSet<&'static str>, clean: &str, raw: &str) -> bool {
    table.contains(clean) || (raw != clean && table.contains(raw))
}

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

    /// Strictly anchored internal-name test: Rust and MSVC/Adesh mangled
    /// identities only.
    ///
    /// Unlike [`Self::is_internal`], this never matches on a loose substring
    /// (for example `..`), so a genuinely unknown application symbol is not
    /// misclassified as toolchain-internal and silently stubbed.
    pub fn is_mangled_internal(name: &str) -> bool {
        let n = name
            .strip_prefix("__imp_")
            .or_else(|| name.strip_prefix("_imp_"))
            .unwrap_or(name);
        // Rust legacy (_ZN.../ZN...) and v0 (R_.../_R...) mangling.
        if n.starts_with("_ZN") || n.starts_with("ZN") || n.starts_with("_R") || n.starts_with("R_")
        {
            return true;
        }
        // MSVC C++ mangling.
        if n.starts_with("??") {
            return true;
        }
        // Rust/Adesh runtime internals and compiler-local labels.
        n.starts_with("__rust")
            || n.starts_with("___rust")
            || n.starts_with("rust_")
            || n.starts_with("_rust_")
            || n.starts_with("anon.")
            || n.starts_with("__func__")
            || n.starts_with('$')
            || n.starts_with('.')
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
        static NTDLL_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| NTDLL_EXACT.iter().copied().collect());
        if in_set(&NTDLL_SET, clean, raw) {
            return Some("ntdll.dll");
        }

        // ── Exact-match: kernelbase.dll ──────────────────────────────────────
        const KERNELBASE_EXACT: &[&str] =
            &["WaitOnAddress", "WakeByAddressAll", "WakeByAddressSingle"];
        static KERNELBASE_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| KERNELBASE_EXACT.iter().copied().collect());
        if in_set(&KERNELBASE_SET, clean, raw) {
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
        static WS2_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| WS2_EXACT.iter().copied().collect());
        if in_set(&WS2_SET, clean, raw) {
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
            "FlsAlloc",
            "FlsFree",
            "FlsGetValue",
            "FlsSetValue",
            "IsThreadAFiber",
            "ConvertFiberToThread",
            "ConvertThreadToFiber",
            "CreateFiber",
            "CreateFiberEx",
            "DeleteFiber",
            "SwitchToFiber",
            "GetSystemTimeAdjustment",
            "SetSystemTimeAdjustment",
            "SetThreadErrorMode",
            "GetThreadErrorMode",
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
            // Common APIs that the previous broad verb prefixes used to catch.
            // Prefix routing was removed (it turned user symbols such as
            // `GetValue` into bogus KERNEL32 imports), so the well-known names
            // must be listed explicitly.
            "GetFileSize",
            "GetFileTime",
            "GetVersion",
            "GetVersionExA",
            "GetVersionExW",
            "GetSystemTime",
            "GetLocalTime",
            "GetTickCount",
            "GetTickCount64",
            "GetTimeZoneInformation",
            "GetACP",
            "GetOEMCP",
            "GetCPInfo",
            "GetStartupInfoA",
            "GetStartupInfoW",
            "GetThreadContext",
            "GetModuleHandleExA",
            "GetModuleHandleExW",
            "GetModuleFileNameA",
            "GetFileAttributesA",
            "GetFileAttributesExA",
            "GetFileAttributesExW",
            "GetSystemWindowsDirectoryW",
            "GetComputerNameW",
            "GetUserNameW",
            "GetActiveProcessorCount",
            "GetCurrentThreadStackLimits",
            "GetThreadDescription",
            "GetEnvironmentStringsA",
            "GetEnvironmentVariableA",
            "GetStringTypeW",
            "GetLocaleInfoW",
            "GetLocaleInfoEx",
            "GetProcessHeap",
            "GetProcessTimes",
            "GetThreadTimes",
            "GetConsoleCP",
            "GetConsoleWindow",
            "GetConsoleScreenBufferInfo",
            "GetExitCodeThread",
            "GetThreadPriority",
            "GetTempFileNameW",
            "GetTempPathA",
            "GetVolumeInformationW",
            "GetDiskFreeSpaceExW",
            "GetUserDefaultLocaleName",
            "GetSystemDefaultLocaleName",
            "CreateFileA",
            "CreateFileW",
            "CreateFileMappingA",
            "CreateFileMappingW",
            "OpenFileMappingW",
            "CreateIoCompletionPort",
            "CreateSemaphoreW",
            "CreateTimerQueueTimer",
            "CreateToolhelp32Snapshot",
            "ReadFile",
            "ReadDirectoryChangesW",
            "WriteFile",
            "CloseHandle",
            "DeleteCriticalSection",
            "DeleteFileA",
            "MoveFileA",
            "MoveFileW",
            "MoveFileExA",
            "FindFirstFileA",
            "FindFirstFileW",
            "FindNextFileA",
            "FindFirstFileExA",
            "GlobalLock",
            "GlobalUnlock",
            "GlobalHandle",
            "GlobalFlags",
            "GlobalReAlloc",
            "GlobalSize",
            "FreeLibraryAndExitThread",
            "HeapAlloc",
            "HeapFree",
            "HeapCreate",
            "HeapDestroy",
            "HeapValidate",
            "HeapLock",
            "HeapUnlock",
            "HeapSetInformation",
            "HeapQueryInformation",
            "HeapSummary",
            "HeapCompact",
            "VirtualAlloc",
            "VirtualFree",
            "VirtualLock",
            "VirtualUnlock",
            "VirtualProtectEx",
            "QueryFullProcessImageNameW",
            "QueryThreadCycleTime",
            "QueryProcessCycleTime",
            "AddVectoredContinueHandler",
            "RemoveVectoredContinueHandler",
            "AddDllDirectory",
            "RemoveDllDirectory",
            "InitializeCriticalSection",
            "InitializeCriticalSectionEx",
            "InitializeCriticalSectionAndSpinCount",
            "InitializeSListHead",
            "InitializeConditionVariable",
            "InitializeSRWLock",
            "EnterCriticalSection",
            "TryEnterCriticalSection",
            "LeaveCriticalSection",
            "DeleteTimerQueueTimer",
            "ReleaseSemaphore",
            "SleepConditionVariableCS",
            "WakeConditionVariable",
            "WakeAllConditionVariable",
            "SetFilePointer",
            "SetFileTime",
            "SetEndOfFile",
            "SetErrorMode",
            "SetPriorityClass",
            "SetThreadPriority",
            "SetProcessAffinityMask",
            "SetConsoleCtrlHandler",
            "SetConsoleTextAttribute",
            "SetConsoleTitleW",
            "SetDefaultDllDirectories",
            "SetDllDirectoryW",
            "SetThreadLocale",
            "SetEnvironmentVariableA",
            "SetNamedPipeHandleState",
            "SetFileValidData",
            "SetFileCompletionNotificationModes",
            "Process32FirstW",
            "Process32NextW",
            "Module32FirstW",
            "Module32NextW",
            "OpenProcess",
            "OpenThread",
            "CancelSynchronousIo",
            "MapViewOfFile",
            "UnmapViewOfFile",
            "FlushViewOfFile",
            "CompareStringA",
            "LCMapStringW",
            "TerminateThread",
            "SetThreadContext",
            "GetSystemFirmwareTable",
        ];
        static KERNEL32_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| KERNEL32_EXACT.iter().copied().collect());
        if in_set(&KERNEL32_SET, clean, raw) {
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
        static ADVAPI32_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| ADVAPI32_EXACT.iter().copied().collect());
        if in_set(&ADVAPI32_SET, clean, raw) {
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
        static USER32_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| USER32_EXACT.iter().copied().collect());
        if in_set(&USER32_SET, clean, raw) {
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
        static BCRYPT_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| BCRYPT_EXACT.iter().copied().collect());
        if in_set(&BCRYPT_SET, clean, raw) {
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
        static DBGHELP_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| DBGHELP_EXACT.iter().copied().collect());
        if in_set(&DBGHELP_SET, clean, raw) {
            return Some("dbghelp.dll");
        }

        // ── Prefix rules: unambiguous API namespaces only ────────────────────
        //
        // Only families whose names cannot plausibly be user symbols are routed
        // by prefix. The previous implementation also routed the broad English
        // verb families (`Get*`, `Set*`, `Create*`, `Read*`, `Write*`, ...),
        // which turned any undefined application symbol such as `GetValue`
        // into a bogus KERNEL32.dll import: the image still linked, but the
        // Windows loader then refused to start it. Everything else must be in
        // the exact tables above, otherwise the symbol stays Undefined and the
        // link fails with a clear, actionable error.
        if clean.starts_with("Nt") || clean.starts_with("Zw") || clean.starts_with("RtlNtStatus") {
            return Some("ntdll.dll");
        }
        if clean.starts_with("Rtl") {
            return Some("KERNEL32.dll");
        }
        if clean.starts_with("WSA") {
            return Some("ws2_32.dll");
        }
        if clean.starts_with("BCrypt") {
            return Some("bcrypt.dll");
        }
        if clean.starts_with("Reg")
            || clean.starts_with("Crypt")
            || clean.starts_with("SystemFunction")
        {
            return Some("advapi32.dll");
        }
        if clean.starts_with("MessageBox") {
            return Some("user32.dll");
        }

        // ── Universal CRT internals → ucrtbase.dll ───────────────────────────
        // msvcrt.dll does not export these symbols; routing them there produced
        // an image the loader refuses to start (STATUS_ENTRYPOINT_NOT_FOUND).
        static UCRT_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
            HashSet::from([
                "__acrt_iob_func",
                "acrt_iob_func",
                "__stdio_common_vfprintf",
                "stdio_common_vfprintf",
                "__stdio_common_vsprintf",
                "stdio_common_vsprintf",
                "__stdio_common_vsscanf",
                "stdio_common_vsscanf",
                "__stdio_common_vfwprintf",
                "__stdio_common_vswprintf",
                "_get_osfhandle",
                "_open_osfhandle",
                "_fileno",
                "_isatty",
                "_setmode",
            ])
        });
        if in_set(&UCRT_SET, clean, raw)
            || clean.starts_with("acrt")
            || clean.starts_with("stdio")
            || clean.starts_with("ucrt")
            || raw.starts_with("__acrt")
            || raw.starts_with("__stdio")
            || raw.starts_with("__ucrt")
            || raw.starts_with("__crt")
        {
            return Some("ucrtbase.dll");
        }

        // ── CRT functions → msvcrt.dll ───────────────────────────────────────
        static MSVCRT_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
            HashSet::from([
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
                // wide string
                "wcslen",
                "wcscpy",
                "wcsncpy",
                "wcscat",
                "wcsncat",
                "wcscmp",
                "wcsncmp",
                "wcschr",
                "wcsrchr",
                "wcsstr",
                "wcstok",
                "wcsdup",
                "towlower",
                "towupper",
                "iswalpha",
                "iswdigit",
                "iswspace",
                "iswpunct",
                "_wcsicmp",
                "_wcsnicmp",
                "wmemchr",
                "wmemcmp",
                "wmemcpy",
                "wmemmove",
                "wmemset",
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
                "chkstk",
                "CxxFrameHandler3",
                "CxxFrameHandler4",
            ])
        });

        // ── MSVC C++ / CRT specials → msvcrt.dll ─────────────────────────────
        if MSVCRT_SET.contains(clean)
            || MSVCRT_SET.contains(raw)
            || clean.starts_with("Cxx")
            || clean.starts_with("chkstk")
            || clean.starts_with("crt")
            || raw.starts_with("__Cxx")
            || raw.starts_with("_Cxx")
            || raw.starts_with("__chkstk")
        {
            return Some("msvcrt.dll");
        }

        None
    }

    /// Returns true if a symbol is a standard C runtime / libc / libm function.
    pub fn is_libc_symbol(sym: &str) -> bool {
        let raw = sym
            .strip_prefix("__imp_")
            .or_else(|| sym.strip_prefix("_imp_"))
            .unwrap_or(sym);
        let clean = raw.trim_start_matches('_');
        if clean.is_empty() || Self::is_internal(raw) || Self::is_internal(clean) {
            return false;
        }
        Self::windows_dll_for(sym).map_or(false, |dll| dll == "msvcrt.dll" || dll == "ucrtbase.dll")
    }

    /// Returns true if a symbol is a standard C library, math, POSIX, or Linux/Unix runtime symbol.
    pub fn is_libc_or_posix_symbol(sym: &str) -> bool {
        let raw = sym
            .strip_prefix("__imp_")
            .or_else(|| sym.strip_prefix("_imp_"))
            .unwrap_or(sym);
        let clean = raw.trim_start_matches('_');
        if clean.is_empty() || Self::is_internal(raw) || Self::is_internal(clean) {
            return false;
        }
        if Self::is_libc_symbol(sym) || Self::is_libc_symbol(clean) {
            return true;
        }

        const POSIX_EXACT: &[&str] = &[
            // POSIX pthreads
            "pthread_self",
            "pthread_create",
            "pthread_join",
            "pthread_detach",
            "pthread_exit",
            "pthread_equal",
            "pthread_once",
            "pthread_cancel",
            "pthread_setcancelstate",
            "pthread_setcanceltype",
            "pthread_testcancel",
            "pthread_mutex_init",
            "pthread_mutex_lock",
            "pthread_mutex_trylock",
            "pthread_mutex_timedlock",
            "pthread_mutex_unlock",
            "pthread_mutex_destroy",
            "pthread_rwlock_init",
            "pthread_rwlock_rdlock",
            "pthread_rwlock_tryrdlock",
            "pthread_rwlock_timedrdlock",
            "pthread_rwlock_wrlock",
            "pthread_rwlock_trywrlock",
            "pthread_rwlock_timedwrlock",
            "pthread_rwlock_unlock",
            "pthread_rwlock_destroy",
            "pthread_cond_init",
            "pthread_cond_wait",
            "pthread_cond_timedwait",
            "pthread_cond_signal",
            "pthread_cond_broadcast",
            "pthread_cond_destroy",
            "pthread_key_create",
            "pthread_key_delete",
            "pthread_getspecific",
            "pthread_setspecific",
            "pthread_attr_init",
            "pthread_attr_destroy",
            "pthread_attr_setstacksize",
            "pthread_attr_getstacksize",
            "pthread_attr_setstack",
            "pthread_attr_getstack",
            "pthread_attr_setguardsize",
            "pthread_attr_getguardsize",
            "pthread_attr_setdetachstate",
            "pthread_attr_getdetachstate",
            "pthread_getattr_np",
            "pthread_setname_np",
            "pthread_getname_np",
            "pthread_sigmask",
            "pthread_kill",
            "sem_init",
            "sem_destroy",
            "sem_open",
            "sem_close",
            "sem_unlink",
            "sem_wait",
            "sem_trywait",
            "sem_timedwait",
            "sem_post",
            "sem_getvalue",
            // POSIX I/O & Filesystem
            "open",
            "open64",
            "openat",
            "openat64",
            "creat",
            "creat64",
            "close",
            "read",
            "write",
            "pread",
            "pread64",
            "pwrite",
            "pwrite64",
            "readv",
            "writev",
            "preadv",
            "pwritev",
            "preadv64",
            "pwritev64",
            "lseek",
            "lseek64",
            "dup",
            "dup2",
            "dup3",
            "pipe",
            "pipe2",
            "fcntl",
            "ioctl",
            "stat",
            "stat64",
            "fstat",
            "fstat64",
            "lstat",
            "lstat64",
            "fstatat",
            "fstatat64",
            "newfstatat",
            "access",
            "faccessat",
            "faccessat2",
            "chmod",
            "fchmod",
            "fchmodat",
            "chown",
            "fchown",
            "lchown",
            "fchownat",
            "truncate",
            "truncate64",
            "ftruncate",
            "ftruncate64",
            "mkdir",
            "mkdirat",
            "rmdir",
            "unlink",
            "unlinkat",
            "rename",
            "renameat",
            "renameat2",
            "link",
            "linkat",
            "symlink",
            "symlinkat",
            "readlink",
            "readlinkat",
            "statvfs",
            "statvfs64",
            "fstatvfs",
            "fstatvfs64",
            "sync",
            "fsync",
            "fdatasync",
            "syncfs",
            "opendir",
            "fdopendir",
            "closedir",
            "readdir",
            "readdir_r",
            "rewinddir",
            "seekdir",
            "telldir",
            "scandir",
            "alphasort",
            "versionsort",
            "getcwd",
            "chdir",
            "fchdir",
            "chroot",
            // POSIX Process, Signal & Memory
            "fork",
            "vfork",
            "execv",
            "execve",
            "execvp",
            "execvpe",
            "execl",
            "execlp",
            "execle",
            "posix_spawn",
            "posix_spawnp",
            "wait",
            "waitpid",
            "wait3",
            "wait4",
            "waitid",
            "getpid",
            "getppid",
            "gettid",
            "getuid",
            "geteuid",
            "getgid",
            "getegid",
            "setuid",
            "seteuid",
            "setgid",
            "setegid",
            "kill",
            "killpg",
            "raise",
            "signal",
            "sigaction",
            "sigprocmask",
            "sigpending",
            "sigsuspend",
            "sigemptyset",
            "sigfillset",
            "sigaddset",
            "sigdelset",
            "sigwait",
            "sigwaitinfo",
            "sigtimedwait",
            "sigaltstack",
            "alarm",
            "pause",
            "sleep",
            "usleep",
            "nanosleep",
            "clock_nanosleep",
            "clock_gettime",
            "clock_getres",
            "clock_settime",
            "gettimeofday",
            "settimeofday",
            "times",
            "sched_yield",
            "sched_getaffinity",
            "sched_setaffinity",
            "prctl",
            "sysconf",
            "pathconf",
            "fpathconf",
            "getrlimit",
            "setrlimit",
            "getrlimit64",
            "setrlimit64",
            "getrusage",
            "mmap",
            "mmap64",
            "munmap",
            "mprotect",
            "msync",
            "madvise",
            "posix_madvise",
            "mlock",
            "munlock",
            "posix_memalign",
            "aligned_alloc",
            // Sockets & Network
            "socket",
            "socketpair",
            "bind",
            "listen",
            "accept",
            "accept4",
            "connect",
            "send",
            "sendto",
            "sendmsg",
            "recv",
            "recvfrom",
            "recvmsg",
            "shutdown",
            "getsockname",
            "getpeername",
            "getsockopt",
            "setsockopt",
            "gethostbyname",
            "gethostbyaddr",
            "getaddrinfo",
            "freeaddrinfo",
            "getnameinfo",
            "inet_ntop",
            "inet_pton",
            "inet_addr",
            "inet_ntoa",
            "poll",
            "ppoll",
            "select",
            "pselect",
            "epoll_create",
            "epoll_create1",
            "epoll_ctl",
            "epoll_wait",
            "epoll_pwait",
            "eventfd",
            "timerfd_create",
            "timerfd_settime",
            "timerfd_gettime",
            "signalfd",
            "inotify_init",
            "inotify_init1",
            "inotify_add_watch",
            "inotify_rm_watch",
            // File timestamp & modern Linux syscalls
            "utimensat",
            "futimens",
            "futimes",
            "utimes",
            "lutimes",
            "utime",
            "statx",
            "copy_file_range",
            "getrandom",
            "getentropy",
            "memfd_create",
            "sync_file_range",
            "fallocate",
            "posix_fallocate",
            "splice",
            "vmsplice",
            "tee",
            "name_to_handle_at",
            "open_by_handle_at",
            // Unwind & Runtime ABI
            "_Unwind_Resume",
            "_Unwind_RaiseException",
            "_Unwind_DeleteException",
            "_Unwind_GetGR",
            "_Unwind_SetGR",
            "_Unwind_GetIP",
            "_Unwind_GetIPInfo",
            "_Unwind_SetIP",
            "_Unwind_GetLanguageSpecificData",
            "_Unwind_GetRegionStart",
            "_Unwind_GetTextRelBase",
            "_Unwind_GetDataRelBase",
            "_Unwind_FindEnclosingFunction",
            "_Unwind_Backtrace",
            "_Unwind_ForcedUnwind",
            "__cxa_atexit",
            "__cxa_finalize",
            "__cxa_thread_atexit_impl",
            "__tls_get_addr",
            "__libc_start_main",
            // Dynamic linking & Error
            "dlopen",
            "dlsym",
            "dlclose",
            "dlerror",
            "dladdr",
            "dl_iterate_phdr",
            "__errno_location",
            "__error",
            "errno",
            "backtrace",
            "backtrace_symbols",
            "backtrace_symbols_fd",
            "syscall",
            "environ",
            "__environ",
        ];
        static POSIX_SET: LazyLock<HashSet<&'static str>> =
            LazyLock::new(|| POSIX_EXACT.iter().copied().collect());

        if POSIX_SET.contains(clean)
            || POSIX_SET.contains(raw)
            || POSIX_SET.contains(sym)
            || clean.starts_with("pthread_")
            || clean.starts_with("posix_")
            || clean.starts_with("clock_")
            || clean.starts_with("epoll_")
            || clean.starts_with("timerfd_")
            || clean.starts_with("eventfd_")
            || clean.starts_with("signalfd")
            || clean.starts_with("inotify_")
            || clean.starts_with("sem_")
            || clean.starts_with("sched_")
            || clean.starts_with("utime")
            || clean.starts_with("futime")
            || clean.starts_with("lutime")
            || clean.starts_with("stat")
            || clean.starts_with("fstat")
            || clean.starts_with("lstat")
            || raw.starts_with("_Unwind_")
            || clean.starts_with("Unwind_")
            || raw.starts_with("__cxa_")
            || clean.starts_with("cxa_")
            || raw.starts_with("__gxx_")
            || raw.starts_with("__gcc_")
            || raw.starts_with("__libc_")
        {
            return true;
        }

        if let Some(stripped) = clean.strip_suffix("64") {
            if POSIX_SET.contains(stripped) {
                return true;
            }
        }

        false
    }

    // ─── Linux / ELF ───────────────────────────────────────────────────────────

    fn classify_elf(raw: &str, _target: &Target) -> SymbolRoute {
        let sym_name = raw
            .strip_prefix("__imp_")
            .or_else(|| raw.strip_prefix("_imp_"))
            .unwrap_or(raw);
        let clean = sym_name.trim_start_matches('_');

        if Self::is_internal(raw) || Self::is_internal(sym_name) {
            return SymbolRoute::InternalRuntime;
        }
        if crate::intrinsics::IntrinsicsEngine::is_intrinsic(raw)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(sym_name)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(clean)
        {
            return SymbolRoute::Intrinsic;
        }

        // Required Adesh runtime symbols must not be silently treated as dynamic libc imports
        if Self::is_adesh_runtime_symbol(raw) || Self::is_adesh_runtime_symbol(sym_name) {
            return SymbolRoute::Undefined;
        }

        // Unmangled C / system library / POSIX symbols dynamically resolve via libc/libgcc on ELF
        let dll = if raw.starts_with("_Unwind_") || sym_name.starts_with("_Unwind_") {
            "libgcc_s.so.1"
        } else {
            "libc.so.6"
        };
        SymbolRoute::DllImport {
            dll,
            name: sym_name.to_string(),
        }
    }

    // ─── macOS / Mach-O ────────────────────────────────────────────────────────

    fn classify_macho(raw: &str) -> SymbolRoute {
        let sym_name = raw
            .strip_prefix("__imp_")
            .or_else(|| raw.strip_prefix("_imp_"))
            .unwrap_or(raw);
        let clean = sym_name.trim_start_matches('_');

        if Self::is_internal(raw) || Self::is_internal(sym_name) {
            return SymbolRoute::InternalRuntime;
        }
        if crate::intrinsics::IntrinsicsEngine::is_intrinsic(raw)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(sym_name)
            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(clean)
        {
            return SymbolRoute::Intrinsic;
        }

        // Required Adesh runtime symbols must not be silently treated as dynamic libSystem imports
        if Self::is_adesh_runtime_symbol(raw) || Self::is_adesh_runtime_symbol(sym_name) {
            return SymbolRoute::Undefined;
        }

        let name = if sym_name.starts_with('_') {
            sym_name.to_string()
        } else {
            format!("_{}", sym_name)
        };
        SymbolRoute::DllImport {
            dll: "/usr/lib/libSystem.B.dylib",
            name,
        }
    }
}
