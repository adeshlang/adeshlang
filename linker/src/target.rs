//! Architecture, OS, Object Format, Accelerator, and Quantum target abstractions.

use crate::error::{ErrorCode, LinkError, LinkResult};
use std::fmt;

/// Target Hardware, Accelerator, or Quantum CPU/QPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    // Standard CPU Architectures
    X86_64,
    X86,
    AArch64,
    Arm,
    Ppc64,
    Ppc64le,
    Riscv64,
    Riscv32,
    S390x,
    Mips,
    Mipsle,
    Mips64,
    Mips64le,
    Loong64,
    Sparc64,
    Wasm32,
    Wasm64,

    // GPU Accelerators
    NvidiaPtx,
    AmdGpuHsa,
    SpirV,

    // NPU / AI Engines
    HexagonDsp,
    AppleAne,
    ArmEthos,

    // TPU Accelerators
    GoogleTpu,

    // Quantum Processing Units (QPU)
    QuantumQpu,
}

impl Arch {
    pub fn pointer_width(&self) -> PointerWidth {
        match self {
            Arch::X86_64
            | Arch::AArch64
            | Arch::Ppc64
            | Arch::Ppc64le
            | Arch::Riscv64
            | Arch::S390x
            | Arch::Mips64
            | Arch::Mips64le
            | Arch::Loong64
            | Arch::Sparc64
            | Arch::Wasm64
            | Arch::NvidiaPtx
            | Arch::AmdGpuHsa
            | Arch::GoogleTpu
            | Arch::QuantumQpu => PointerWidth::U64,

            Arch::X86
            | Arch::Arm
            | Arch::Riscv32
            | Arch::Mips
            | Arch::Mipsle
            | Arch::Wasm32
            | Arch::SpirV
            | Arch::HexagonDsp
            | Arch::AppleAne
            | Arch::ArmEthos => PointerWidth::U32,
        }
    }

    pub fn default_endianness(&self) -> Endianness {
        match self {
            Arch::Ppc64 | Arch::S390x | Arch::Mips | Arch::Mips64 | Arch::Sparc64 => {
                Endianness::Big
            }
            _ => Endianness::Little,
        }
    }

    pub fn is_accelerator(&self) -> bool {
        matches!(
            self,
            Arch::NvidiaPtx
                | Arch::AmdGpuHsa
                | Arch::SpirV
                | Arch::HexagonDsp
                | Arch::AppleAne
                | Arch::ArmEthos
                | Arch::GoogleTpu
        )
    }

    pub fn is_quantum(&self) -> bool {
        matches!(self, Arch::QuantumQpu)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::X86 => "x86",
            Arch::AArch64 => "aarch64",
            Arch::Arm => "arm",
            Arch::Ppc64 => "ppc64",
            Arch::Ppc64le => "ppc64le",
            Arch::Riscv64 => "riscv64",
            Arch::Riscv32 => "riscv32",
            Arch::S390x => "s390x",
            Arch::Mips => "mips",
            Arch::Mipsle => "mipsle",
            Arch::Mips64 => "mips64",
            Arch::Mips64le => "mips64le",
            Arch::Loong64 => "loong64",
            Arch::Sparc64 => "sparc64",
            Arch::Wasm32 => "wasm32",
            Arch::Wasm64 => "wasm64",
            Arch::NvidiaPtx => "nvptx64",
            Arch::AmdGpuHsa => "amdgcn",
            Arch::SpirV => "spirv",
            Arch::HexagonDsp => "hexagon",
            Arch::AppleAne => "ane",
            Arch::ArmEthos => "ethos",
            Arch::GoogleTpu => "tpu",
            Arch::QuantumQpu => "qpu",
        }
    }
}

/// Target Operating System or Runtime Environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Os {
    Linux,
    Windows,
    MacOS,
    FreeBSD,
    OpenBSD,
    NetBSD,
    DragonFly,
    Solaris,
    Aix,
    Plan9,
    Wasi,
    Android,
    Ios,
    CudaRuntime,
    RocmRuntime,
    QuantumRuntime,
    None,
}

impl Os {
    pub fn default_object_format(&self) -> ObjectFormat {
        match self {
            Os::Linux
            | Os::FreeBSD
            | Os::OpenBSD
            | Os::NetBSD
            | Os::DragonFly
            | Os::Solaris
            | Os::Android => ObjectFormat::Elf,
            Os::Windows => ObjectFormat::Pe,
            Os::MacOS | Os::Ios => ObjectFormat::MachO,
            Os::Aix => ObjectFormat::Xcoff,
            Os::Plan9 => ObjectFormat::Elf,
            Os::Wasi => ObjectFormat::Wasm,
            Os::CudaRuntime | Os::RocmRuntime => ObjectFormat::GpuFatbin,
            Os::QuantumRuntime => ObjectFormat::QirQuantum,
            Os::None => ObjectFormat::Elf,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Os::Linux => "linux",
            Os::Windows => "windows",
            Os::MacOS => "macos",
            Os::FreeBSD => "freebsd",
            Os::OpenBSD => "openbsd",
            Os::NetBSD => "netbsd",
            Os::DragonFly => "dragonfly",
            Os::Solaris => "solaris",
            Os::Aix => "aix",
            Os::Plan9 => "plan9",
            Os::Wasi => "wasi",
            Os::Android => "android",
            Os::Ios => "ios",
            Os::CudaRuntime => "cuda",
            Os::RocmRuntime => "rocm",
            Os::QuantumRuntime => "quantum",
            Os::None => "none",
        }
    }
}

/// Binary Object and Executable Format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectFormat {
    Elf,
    Pe,
    MachO,
    Xcoff,
    Wasm,
    AdeshNative,
    GpuFatbin,
    QirQuantum,
}

impl ObjectFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            ObjectFormat::Elf => "elf",
            ObjectFormat::Pe => "pe",
            ObjectFormat::MachO => "macho",
            ObjectFormat::Xcoff => "xcoff",
            ObjectFormat::Wasm => "wasm",
            ObjectFormat::AdeshNative => "adesh",
            ObjectFormat::GpuFatbin => "fatbin",
            ObjectFormat::QirQuantum => "qir",
        }
    }
}

/// Target ABI convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abi {
    SystemV,
    WindowsMsvc,
    Darwin,
    Aix,
    Plan9,
    Wasi,
    CudaAbi,
    RocmHsa,
    QirAbi,
    BareMetal,
}

/// Pointer width in bytes and bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerWidth {
    U32,
    U64,
}

impl PointerWidth {
    pub fn bytes(&self) -> usize {
        match self {
            PointerWidth::U32 => 4,
            PointerWidth::U64 => 8,
        }
    }

    pub fn bits(&self) -> usize {
        self.bytes() * 8
    }
}

/// Byte endianness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Endianness {
    #[default]
    Little,
    Big,
}

impl Endianness {
    pub fn is_little(self) -> bool {
        self == Endianness::Little
    }

    pub fn is_big(self) -> bool {
        self == Endianness::Big
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Endianness::Little => "little",
            Endianness::Big => "big",
        }
    }
}

/// Relocation and code address model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelocationModel {
    Static,
    Pic,
    Dynamic,
}

/// Target maturity tier, derived from **verification status** — not from how
/// much code exists for the target.
///
/// The three levels are the same ones `TARGET_MATRIX.md` uses, and a target only
/// moves up by gaining a checked-in test that CI runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetTier {
    /// Tier 1: CI builds an artifact for this target and **runs it**, asserting
    /// output or exit status. Capability caveats (which host functions exist,
    /// whether the full ABI is reachable from source) are recorded in
    /// `TARGET_MATRIX.md`, not hidden here.
    Tier1Supported,

    /// Tier 2: CI emits and parses back the artifact (object structure,
    /// relocations, section layout), but nothing executes it. "Emission-tested",
    /// never "supported".
    Tier2Experimental,

    /// Tier 3: declared only — the format header and relocation constants exist,
    /// but there is no compiler backend and no CI-tested link path.
    Tier3Declared,
}

impl TargetTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetTier::Tier1Supported => "Tier 1 (SUPPORTED)",
            TargetTier::Tier2Experimental => "Tier 2 (EXPERIMENTAL)",
            TargetTier::Tier3Declared => "Tier 3 (DECLARED)",
        }
    }

    /// The line printed above each group of [`advertised_by_tier`]. Keeping it
    /// here means `adeshlink targets` and `adesh target list` state the same
    /// verification claim for the same tier.
    pub fn advertised_heading(self) -> &'static str {
        match self {
            TargetTier::Tier1Supported => {
                "Tier 1 (execution-verified in CI: the artifact is built and run, with output or exit status asserted)"
            }
            TargetTier::Tier2Experimental => {
                "Tier 2 (emission-tested in CI: the artifact is built and parsed back, never executed)"
            }
            TargetTier::Tier3Declared => {
                "Tier 3 (declared: no compiler backend, and no CI-run link or execution test)"
            }
        }
    }
}

/// C library flavor governing dynamic-linking details (interpreter path,
/// DT_NEEDED names, TLS variable model). Only meaningful for ELF targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibcFlavor {
    /// glibc: `/lib64/ld-linux-*.so`, DT_NEEDED `libc.so.6`.
    Gnu,
    /// musl: `/lib/ld-musl-<arch>.so.1`; the interpreter is the C library,
    /// named `libc.musl-<arch>.so.1` in DT_NEEDED on musl distributions.
    Musl,
    /// The Windows CRT. Named here because it is what separates
    /// `x86_64-pc-windows-msvc` from `x86_64-pc-windows-gnu`: both write the
    /// same PE, but only the MSVC flavour is the one every CI suite asks for,
    /// so the tier table has to be able to tell them apart.
    Msvc,
}

impl LibcFlavor {
    pub fn as_str(&self) -> &'static str {
        match self {
            LibcFlavor::Gnu => "gnu",
            LibcFlavor::Musl => "musl",
            LibcFlavor::Msvc => "msvc",
        }
    }
}

/// Comprehensive Target Model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub arch: Arch,
    pub os: Os,
    pub format: ObjectFormat,
    pub abi: Abi,
    pub pointer_width: PointerWidth,
    pub endianness: Endianness,
    pub relocation_model: RelocationModel,
    pub page_size: u64,
    pub image_base: u64,
    pub default_entry: String,
    /// C library flavor for ELF targets (detected from the triple: `musl`
    /// vs glibc). Never guessed from the host: a Windows host building a
    /// Linux target must take the flavor from the requested triple alone.
    pub libc_flavor: LibcFlavor,
}

impl Target {
    pub fn from_triple(triple_str: &str) -> LinkResult<Self> {
        let lower = triple_str.to_lowercase();
        let parts: Vec<&str> = lower.split('-').collect();

        let arch = match parts.first().copied().unwrap_or("") {
            "x86_64" | "amd64" | "x64" => Arch::X86_64,
            "i386" | "i686" | "386" | "x86" => Arch::X86,
            "aarch64" | "arm64" => Arch::AArch64,
            "arm" | "armv7" | "armv7a" | "armv8" | "armv7em" | "thumbv6m" | "thumbv7em" => {
                Arch::Arm
            }
            "ppc64le" | "powerpc64le" => Arch::Ppc64le,
            "ppc64" | "powerpc64" => Arch::Ppc64,
            "riscv64" | "riscv64gc" => Arch::Riscv64,
            "riscv32" | "riscv32imac" => Arch::Riscv32,
            "s390x" => Arch::S390x,
            "mips" => Arch::Mips,
            "mipsle" => Arch::Mipsle,
            "mips64" => Arch::Mips64,
            "mips64le" => Arch::Mips64le,
            "loong64" | "loongarch64" => Arch::Loong64,
            "sparc64" | "sparcv9" => Arch::Sparc64,
            "wasm32" => Arch::Wasm32,
            "wasm64" => Arch::Wasm64,
            "nvptx64" | "cuda" | "ptx" => Arch::NvidiaPtx,
            "amdgcn" | "rocm" | "hsaco" => Arch::AmdGpuHsa,
            "spirv" | "vulkan" => Arch::SpirV,
            "hexagon" | "qdsp6" => Arch::HexagonDsp,
            "ane" | "coreml" => Arch::AppleAne,
            "ethos" | "ethos_u" => Arch::ArmEthos,
            "tpu" | "xla" => Arch::GoogleTpu,
            "qpu" | "quantum" | "qir" | "qasm" => Arch::QuantumQpu,
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidTarget,
                    format!("unrecognized target architecture: `{}` in triple `{}`", other, triple_str),
                ).with_suggestion("Supported architectures: x86_64, x86, aarch64, arm, ppc64, ppc64le, riscv64, riscv32, s390x, mips, mips64, loong64, sparc64, wasm32, wasm64, nvptx64, amdgcn, spirv, hexagon, ane, ethos, tpu, qpu"));
            }
        };

        let (os, format, abi) = if lower.contains("linux") {
            (Os::Linux, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("windows")
            || lower.contains("win32")
            || lower.contains("msvc")
            || lower.contains("mingw")
        {
            (Os::Windows, ObjectFormat::Pe, Abi::WindowsMsvc)
        } else if lower.contains("darwin") || lower.contains("macos") || lower.contains("apple") {
            (Os::MacOS, ObjectFormat::MachO, Abi::Darwin)
        } else if lower.contains("ios") {
            (Os::Ios, ObjectFormat::MachO, Abi::Darwin)
        } else if lower.contains("freebsd") {
            (Os::FreeBSD, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("openbsd") {
            (Os::OpenBSD, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("netbsd") {
            (Os::NetBSD, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("dragonfly") {
            (Os::DragonFly, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("solaris") || lower.contains("illumos") {
            (Os::Solaris, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("aix") {
            (Os::Aix, ObjectFormat::Xcoff, Abi::Aix)
        } else if lower.contains("plan9") {
            (Os::Plan9, ObjectFormat::Elf, Abi::Plan9)
        } else if lower.contains("android") {
            (Os::Android, ObjectFormat::Elf, Abi::SystemV)
        } else if lower.contains("wasi") {
            (Os::Wasi, ObjectFormat::Wasm, Abi::Wasi)
        } else if lower.contains("cuda") || arch == Arch::NvidiaPtx {
            (Os::CudaRuntime, ObjectFormat::GpuFatbin, Abi::CudaAbi)
        } else if lower.contains("rocm") || arch == Arch::AmdGpuHsa {
            (Os::RocmRuntime, ObjectFormat::GpuFatbin, Abi::RocmHsa)
        } else if lower.contains("quantum") || arch == Arch::QuantumQpu {
            (Os::QuantumRuntime, ObjectFormat::QirQuantum, Abi::QirAbi)
        } else if lower.contains("none") || lower.contains("baremetal") {
            (
                Os::None,
                if arch == Arch::Wasm32 || arch == Arch::Wasm64 {
                    ObjectFormat::Wasm
                } else {
                    ObjectFormat::Elf
                },
                Abi::BareMetal,
            )
        } else if arch == Arch::Wasm32 || arch == Arch::Wasm64 {
            (Os::Wasi, ObjectFormat::Wasm, Abi::Wasi)
        } else {
            // Default fallback based on host OS
            #[cfg(target_os = "windows")]
            {
                (Os::Windows, ObjectFormat::Pe, Abi::WindowsMsvc)
            }
            #[cfg(target_os = "macos")]
            {
                (Os::MacOS, ObjectFormat::MachO, Abi::Darwin)
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            {
                (Os::Linux, ObjectFormat::Elf, Abi::SystemV)
            }
        };

        let pointer_width = arch.pointer_width();
        let endianness = arch.default_endianness();

        // The C library flavor is a property of the requested target triple,
        // never of the machine running the linker. `*-*-linux-musl` triples
        // select musl; every other Linux triple defaults to glibc. On Windows
        // the default is the MSVC CRT — it is the flavour every executed CI
        // suite names — and only an explicit `-gnu` suffix means MinGW.
        let libc_flavor = if os == Os::Windows {
            if lower.contains("gnu") {
                LibcFlavor::Gnu
            } else {
                LibcFlavor::Msvc
            }
        } else if os == Os::Linux && lower.contains("musl") {
            LibcFlavor::Musl
        } else {
            LibcFlavor::Gnu
        };

        let page_size = match (arch, os) {
            (Arch::AArch64, Os::MacOS | Os::Ios) => 16384,
            (Arch::AArch64 | Arch::Ppc64 | Arch::Ppc64le, Os::Linux) => 65536,
            (Arch::Wasm32 | Arch::Wasm64, _) => 65536,
            _ => 4096,
        };

        let image_base = match format {
            ObjectFormat::Elf => match pointer_width {
                PointerWidth::U64 => 0x400000,
                PointerWidth::U32 => 0x08048000,
            },
            ObjectFormat::Pe => match pointer_width {
                PointerWidth::U64 => 0x140000000,
                PointerWidth::U32 => 0x00400000,
            },
            ObjectFormat::MachO => 0x100000000,
            ObjectFormat::Xcoff => 0x10000000,
            ObjectFormat::Wasm => 0x0,
            ObjectFormat::AdeshNative => 0x10000,
            ObjectFormat::GpuFatbin => 0x0,
            ObjectFormat::QirQuantum => 0x0,
        };

        let default_entry = match (format, os) {
            (ObjectFormat::Pe, _) => "mainCRTStartup".to_string(),
            (ObjectFormat::MachO, _) => "_main".to_string(),
            (ObjectFormat::Wasm, _) => "_start".to_string(),
            (ObjectFormat::QirQuantum, _) => "__adesh_quantum_main".to_string(),
            (ObjectFormat::GpuFatbin, _) => "__adesh_gpu_kernel_entry".to_string(),
            _ => "_start".to_string(),
        };

        Ok(Self {
            arch,
            os,
            format,
            abi,
            pointer_width,
            endianness,
            relocation_model: RelocationModel::Static,
            page_size,
            image_base,
            default_entry,
            libc_flavor,
        })
    }

    /// Construct default target for the host compiling platform.
    pub fn host() -> Self {
        #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
        {
            Self::from_triple("x86_64-windows").unwrap()
        }

        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            Self::from_triple("x86_64-linux").unwrap()
        }

        #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
        {
            Self::from_triple("aarch64-linux").unwrap()
        }

        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            Self::from_triple("aarch64-macos").unwrap()
        }

        #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
        {
            Self::from_triple("x86_64-macos").unwrap()
        }

        #[cfg(not(any(
            all(target_arch = "x86_64", target_os = "windows"),
            all(target_arch = "x86_64", target_os = "linux"),
            all(target_arch = "aarch64", target_os = "linux"),
            all(target_arch = "aarch64", target_os = "macos"),
            all(target_arch = "x86_64", target_os = "macos")
        )))]
        {
            Self::from_triple("x86_64-linux").unwrap()
        }
    }

    /// x86_64 Linux target.
    pub fn x86_64_linux() -> Self {
        Self::from_triple("x86_64-linux").unwrap()
    }

    /// x86_64 Windows target.
    pub fn x86_64_windows() -> Self {
        Self::from_triple("x86_64-windows").unwrap()
    }

    /// ARM64 macOS target.
    pub fn aarch64_macos() -> Self {
        Self::from_triple("aarch64-macos").unwrap()
    }

    /// WASM32 WASI target.
    pub fn wasm32_wasi() -> Self {
        Self::from_triple("wasm32-wasi").unwrap()
    }

    /// Tier from verification status only. Each arm names the CI evidence that
    /// earns it; a target with no evidence in that list is never Tier 1, however
    /// much of the pipeline exists for it.
    pub fn tier(&self) -> TargetTier {
        match (self.arch, self.os) {
            // Windows PE executables are built and *run* by CI with asserted output
            // and exit codes: `tests/native_x86_64_e2e_test.rs` plus the phase 2 and
            // phase 8 suites, all Windows-gated and executed on the windows-latest
            // job, and `linker/tests/pe_windows_e2e_test.rs` (a real `msvcrt!puts`
            // import). Every one of those suites asks for the `-msvc` triple, so the
            // MinGW flavour of the same PE writer is Tier 2: the same bytes are
            // produced, but no CI job has requested, linked or run a `-gnu` triple.
            (Arch::X86_64, Os::Windows) if self.libc_flavor == LibcFlavor::Msvc => {
                TargetTier::Tier1Supported
            }
            (Arch::X86_64, Os::Windows) => TargetTier::Tier2Experimental,
            // Static and dynamic ELF: `tests/native_phase3_linux_e2e_test.rs` runs
            // static ELFs against the host loader,
            // `tests/native_phase8_dynamic_elf_test.rs` exercises real `ld.so`
            // including a `dlopen`/`dlsym` shared library, and
            // `tests/native_phase8_tls_ehframe_test.rs` asserts TLS exit codes — all
            // on the ubuntu-22.04 job.
            (Arch::X86_64, Os::Linux) if self.libc_flavor == LibcFlavor::Gnu => {
                TargetTier::Tier1Supported
            }
            // musl is the exception among x86-64 Linux targets: its TLS binaries have
            // been executed against a real musl `ld.so` on a developer WSL machine,
            // and the Windows CI job only reaches that step when the runner provides
            // a distro — it skips loudly otherwise. There is no dedicated musl job,
            // so CI does not guarantee the execution.
            (Arch::X86_64, Os::Linux) => TargetTier::Tier2Experimental,
            // WASM modules produced from real clang objects execute in CI on two
            // engines with asserted output: V8 through Node
            // (linker/tests/wasm_hello_world_test.rs, wasm_cross_object_test.rs,
            // wasm_pure_module_test.rs, wasm_table_test.rs) and Cranelift through
            // wasmtime (tests/wasm_wasmtime_execution_test.rs). The linker ships no
            // WASI runtime of its own and only `fd_write` is importable — a
            // capability limit recorded in TARGET_MATRIX.md, not a verification one.
            (Arch::Wasm32, Os::Wasi | Os::None) => TargetTier::Tier1Supported,

            // Everything below is emitted and parsed back in CI but never executed.
            // AArch64: codegen refuses what it cannot encode, ADOB/ELF/Mach-O
            // structure is tested, and no qemu or hardware job runs the image. The
            // macOS AOT suites do run an Apple Silicon Mach-O built by this
            // pipeline, but no test asserts the dyld bind/rebase/TLV structures at
            // run time — `macho_dyld_binding_test.rs` decodes them, it does not run
            // them.
            (Arch::AArch64, _) => TargetTier::Tier2Experimental,
            // x86-64 Mach-O: there is no x86-64 macOS runner, so nothing executes
            // the image this writer produces.
            (Arch::X86_64, Os::MacOS) => TargetTier::Tier2Experimental,
            // RISC-V 64: RV64GC encodings and ELF structure are tested; real source
            // builds stop at `ParallelMove`, and no qemu job runs the binary.
            (Arch::Riscv64, Os::Linux | Os::None) => TargetTier::Tier2Experimental,

            // Declared only. The architectures below have no compiler backend at
            // all (`create_backend` refuses them), and the targets listed here have
            // no CI-run link or execution test — including the ones whose link path
            // is exercised only by suites the workflow does not invoke, and i686,
            // whose linker relocation handler is still the x86-64 one
            // (linker/src/arch/mod.rs).
            _ => TargetTier::Tier3Declared,
        }
    }

    pub fn is_64_bit(&self) -> bool {
        self.pointer_width == PointerWidth::U64
    }

    /// PT_INTERP path for a dynamic ELF executable on this target.
    ///
    /// Returns `None` when the target has no known dynamic loader; the ELF
    /// writer must then refuse a dynamic link loudly instead of guessing a
    /// path (a wrong interpreter path is a runtime crash, not a link error).
    pub fn elf_interpreter(&self) -> Option<&'static str> {
        if self.format != ObjectFormat::Elf {
            return None;
        }
        match (self.os, self.arch) {
            (Os::Linux, Arch::X86_64) => match self.libc_flavor {
                LibcFlavor::Musl => Some("/lib/ld-musl-x86_64.so.1"),
                LibcFlavor::Gnu => Some("/lib64/ld-linux-x86-64.so.2"),
                LibcFlavor::Msvc => None,
            },
            (Os::Linux, Arch::AArch64) => match self.libc_flavor {
                LibcFlavor::Musl => Some("/lib/ld-musl-aarch64.so.1"),
                LibcFlavor::Gnu => Some("/lib/ld-linux-aarch64.so.1"),
                LibcFlavor::Msvc => None,
            },
            (Os::Linux, Arch::X86) => match self.libc_flavor {
                LibcFlavor::Musl => Some("/lib/ld-musl-i386.so.1"),
                LibcFlavor::Gnu => Some("/lib/ld-linux.so.2"),
                LibcFlavor::Msvc => None,
            },
            (Os::Linux, Arch::Riscv64) => match self.libc_flavor {
                LibcFlavor::Musl => Some("/lib/ld-musl-riscv64.so.1"),
                LibcFlavor::Gnu => Some("/lib/ld-linux-riscv64-lp64d.so.1"),
                LibcFlavor::Msvc => None,
            },
            (Os::FreeBSD, _) => Some("/libexec/ld-elf.so.1"),
            _ => None,
        }
    }

    /// DT_NEEDED soname for the target C library.
    ///
    /// glibc links name `libc.so.6`. musl's interpreter is the C library,
    /// but it is NOT implicitly part of a program's symbol scope: musl-based
    /// distributions (Alpine and any musl install following its naming)
    /// carry it as `libc.musl-<arch>.so.1` and every dynamically linked
    /// program has a DT_NEEDED for it — omitting it produces
    /// "symbol not found" for every libc import at load time.
    ///
    /// Returns `None` for musl architectures whose distro soname is not
    /// established; the writer then emits no libc DT_NEEDED.
    pub fn libc_soname(&self) -> Option<&'static str> {
        match (self.libc_flavor, self.arch) {
            (LibcFlavor::Gnu, _) => Some("libc.so.6"),
            (LibcFlavor::Musl, Arch::X86_64) => Some("libc.musl-x86_64.so.1"),
            (LibcFlavor::Musl, Arch::AArch64) => Some("libc.musl-aarch64.so.1"),
            (LibcFlavor::Musl, Arch::Riscv64) => Some("libc.musl-riscv64.so.1"),
            (LibcFlavor::Musl, _) => None,
            // The Windows CRT is not an ELF DT_NEEDED entry; a PE links against
            // its import tables instead.
            (LibcFlavor::Msvc, _) => None,
        }
    }

    pub fn triple_string(&self) -> String {
        format!("{}-{}", self.arch.as_str(), self.os.as_str())
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{} ({} {}-bit)",
            self.arch.as_str(),
            self.os.as_str(),
            self.format.as_str(),
            match self.endianness {
                Endianness::Little => "little-endian",
                Endianness::Big => "big-endian",
            },
            self.pointer_width.bits()
        )
    }
}

/// A target the toolchain advertises to users, with the one-line capability
/// summary printed beside it.
///
/// `adeshlink targets` and `adesh target list` both render
/// [`advertised_by_tier`], so an advertised triple can never claim a tier its
/// verification evidence does not support — the tier is looked up from
/// [`Target::tier`], not written out again.
pub struct AdvertisedTarget {
    pub triple: &'static str,
    pub summary: &'static str,
}

impl AdvertisedTarget {
    const fn new(triple: &'static str, summary: &'static str) -> Self {
        Self { triple, summary }
    }
}

/// Every target the CLI offers, with its current capability summary. Adding an
/// entry here is the only way a target becomes user-visible; the tier comes
/// from the CI evidence recorded in [`Target::tier`].
pub const ADVERTISED_TARGETS: &[AdvertisedTarget] = &[
    // Tier 1 — CI builds and runs the artifact.
    AdvertisedTarget::new(
        "x86_64-unknown-linux-gnu",
        "Linux x86-64 (glibc): static + dynamic ELF, PLT/GOT, TLS, .eh_frame",
    ),
    AdvertisedTarget::new(
        "x86_64-pc-windows-msvc",
        "Windows x86-64 (MSVC): PE32+ with imports/IAT, TLS, base relocs, DLL exports",
    ),
    AdvertisedTarget::new(
        "wasm32-unknown-wasi",
        "WebAssembly WASI: links clang objects, executes on V8 and Cranelift; only \
         wasi_snapshot_preview1.fd_write is importable",
    ),
    AdvertisedTarget::new(
        "wasm32-unknown-unknown",
        "WebAssembly: same linker path, env.* imports served by the generated Node loader",
    ),
    // Tier 2 — emitted and parsed back in CI, never executed.
    AdvertisedTarget::new(
        "x86_64-pc-windows-gnu",
        "Windows x86-64 (MinGW flavour): the same PE32+ writer as `-msvc`, but every CI \
         suite that links and runs a PE asks for the `-msvc` triple",
    ),
    AdvertisedTarget::new(
        "x86_64-unknown-linux-musl",
        "Linux x86-64 (musl): same ELF relocations; TLS verified on a developer WSL host, \
         no dedicated musl CI job",
    ),
    AdvertisedTarget::new(
        "aarch64-unknown-linux-gnu",
        "Linux ARM64: AAPCS64 codegen, loud about what it cannot encode; no runtime archive, \
         no qemu execution",
    ),
    AdvertisedTarget::new(
        "aarch64-apple-darwin",
        "macOS Apple Silicon Mach-O: dyld bind/rebase/TLV emitted and parsed back; the AOT \
         suites run an image on this arch but assert no binding at run time",
    ),
    AdvertisedTarget::new(
        "x86_64-apple-darwin",
        "macOS Intel Mach-O: emitted and parsed back; no x86-64 macOS runner exists",
    ),
    AdvertisedTarget::new(
        "riscv64gc-unknown-linux-gnu",
        "RISC-V 64 LP64D: RV64GC encodings and ELF structure tested; source builds stop at \
         ParallelMove, no qemu execution",
    ),
    AdvertisedTarget::new(
        "riscv64gc-unknown-none-elf",
        "RISC-V 64 bare metal: same backend, ELF output with the synthesized RV64 _start stub",
    ),
    // Tier 3 — declared only: no compiler backend and no CI-run link or execution test.
    AdvertisedTarget::new(
        "armv7-unknown-linux-gnueabihf",
        "Linux ARM32: no compiler backend; the linker refuses it for native codegen",
    ),
    AdvertisedTarget::new(
        "thumbv7em-none-eabihf",
        "ARM Cortex-M: proof-of-concept Thumb-2 ALU subset, no end-to-end firmware link test",
    ),
    AdvertisedTarget::new(
        "riscv32imac-unknown-none-elf",
        "RISC-V 32: refused — the backend implements RV64 only and the linker has no RV32 \
         entry stub",
    ),
    AdvertisedTarget::new(
        "i686-unknown-linux-gnu",
        "Linux x86-32: no compiler backend; the linker's 32-bit relocation handler is still \
         the x86-64 one",
    ),
    AdvertisedTarget::new(
        "i686-pc-windows-gnu",
        "Windows x86-32: no compiler backend; PE32 emission is untested",
    ),
    AdvertisedTarget::new(
        "powerpc64le-unknown-linux-gnu",
        "PowerPC 64 LE: format declared, no backend",
    ),
    AdvertisedTarget::new(
        "s390x-unknown-linux-gnu",
        "IBM z/Architecture: format declared, no backend",
    ),
    AdvertisedTarget::new(
        "nvptx64-nvidia-cuda",
        "NVIDIA CUDA: ADOB kernel container packaging only, no compiler backend, never run",
    ),
    AdvertisedTarget::new(
        "amdgcn-amd-amdhsa",
        "AMD ROCm: ADOB kernel container packaging only, no compiler backend, never run",
    ),
    AdvertisedTarget::new(
        "quantum-circuit-qasm3",
        "QIR / OpenQASM 3.0: container embed and state-vector simulator only, no compiler \
         backend, no hardware backend, not run by CI",
    ),
];

/// The advertised targets grouped by the tier their verification evidence earns,
/// most-verified first. A triple in [`ADVERTISED_TARGETS`] that does not parse is
/// a table bug, and `linker/tests/phase11_target_tiers_test.rs` fails on it.
pub fn advertised_by_tier() -> Vec<(TargetTier, Vec<&'static AdvertisedTarget>)> {
    let mut groups: Vec<(TargetTier, Vec<&'static AdvertisedTarget>)> = Vec::new();
    for entry in ADVERTISED_TARGETS {
        let tier = Target::from_triple(entry.triple)
            .unwrap_or_else(|_| panic!("advertised triple `{}` does not parse", entry.triple))
            .tier();
        if let Some((_, items)) = groups.iter_mut().find(|(grouped, _)| *grouped == tier) {
            items.push(entry);
        } else {
            groups.push((tier, vec![entry]));
        }
    }
    // Tier 1 first, then 2, then 3 — the declaration order above is prose order,
    // and the printed list must read as a verification ladder.
    groups.sort_by_key(|(tier, _)| *tier);
    groups
}
