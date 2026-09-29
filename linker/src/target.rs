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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endianness {
    Little,
    Big,
}

/// Relocation and code address model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelocationModel {
    Static,
    Pic,
    Dynamic,
}

/// Target maturity tier and production verification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetTier {
    /// Tier 1: Fully Supported, Production-Verified & End-to-End Tested.
    /// Full binary emission, relocations, runtime integration, automated testing.
    Tier1Supported,

    /// Tier 2: Experimental / Code-Gen Validated.
    /// Working code-gen and binary emission; ongoing platform hardware validation.
    Tier2Experimental,

    /// Tier 3: Declared / Format-Ready.
    /// Binary format headers and relocation structures declared; awaiting target runner.
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
}

impl Target {
    pub fn from_triple(triple_str: &str) -> LinkResult<Self> {
        let lower = triple_str.to_lowercase();
        let parts: Vec<&str> = lower.split('-').collect();

        let arch = match parts.first().copied().unwrap_or("") {
            "x86_64" | "amd64" | "x64" => Arch::X86_64,
            "i386" | "i686" | "386" | "x86" => Arch::X86,
            "aarch64" | "arm64" => Arch::AArch64,
            "arm" | "armv7" | "armv7a" | "armv8" => Arch::Arm,
            "ppc64le" => Arch::Ppc64le,
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

    pub fn tier(&self) -> TargetTier {
        match (self.arch, self.os) {
            (Arch::X86_64, Os::Linux | Os::Windows | Os::MacOS | Os::FreeBSD) => {
                TargetTier::Tier1Supported
            }
            (Arch::AArch64, Os::Linux | Os::MacOS | Os::Windows | Os::Android | Os::Ios) => {
                TargetTier::Tier1Supported
            }
            (Arch::X86, Os::Linux | Os::Windows) => TargetTier::Tier1Supported,
            (Arch::Wasm32, Os::Wasi | Os::None) => TargetTier::Tier1Supported,

            (Arch::Riscv64 | Arch::Riscv32, Os::Linux | Os::None) => TargetTier::Tier2Experimental,
            (Arch::Ppc64le, Os::Linux) => TargetTier::Tier2Experimental,
            (Arch::Arm, Os::Linux | Os::Android) => TargetTier::Tier2Experimental,
            (Arch::NvidiaPtx, _) => TargetTier::Tier2Experimental,
            (Arch::AmdGpuHsa, _) => TargetTier::Tier2Experimental,
            (Arch::QuantumQpu, _) => TargetTier::Tier2Experimental,

            _ => TargetTier::Tier3Declared,
        }
    }

    pub fn is_64_bit(&self) -> bool {
        self.pointer_width == PointerWidth::U64
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
