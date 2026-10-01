//! Target descriptor, architecture definitions, compute devices, and ABI descriptors.

use std::fmt;

/// Compute device category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ComputeDevice {
    #[default]
    Cpu,
    Gpu,
    Npu,
    Tpu,
    Dsp,
    Fpga,
    Accelerator,
    Embedded,
    Custom,
}

/// GPU architecture families.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GpuArchitecture {
    NvidiaCuda { compute_capability: (u32, u32) },
    AmdGcn { generation: String },
    AmdRdna { generation: u32 },
    AppleSiliconGpu { family: u32 },
    IntelGpu { generation: u32 },
    GenericSpirV,
    Custom(String),
}

/// NPU architecture families.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NpuArchitecture {
    GenericTensorNpu,
    QualcommHexagon,
    ArmEthos,
    AppleNeuralEngine,
    IntelNpu,
    Custom(String),
}

/// TPU architecture families.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TpuArchitecture {
    GoogleTpuV4,
    GoogleTpuV5,
    GenericMatrixCore,
    Custom(String),
}

/// Embedded MCU architectures.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EmbeddedArchitecture {
    ArmCortexM0,
    ArmCortexM3,
    ArmCortexM4F,
    ArmCortexM7F,
    RiscV32Imac,
    XtensaLx6,
    XtensaLx7,
    Avr,
    Custom(String),
}

/// Universal architecture enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Architecture {
    X86,
    #[default]
    X86_64,
    Arm,
    AArch64,
    RiscV32,
    RiscV64,
    PowerPc,
    PowerPc64,
    Mips,
    Wasm32,
    Wasm64,
    Gpu(GpuArchitecture),
    Npu(NpuArchitecture),
    Tpu(TpuArchitecture),
    Embedded(EmbeddedArchitecture),
    Custom(String),
}

impl Architecture {
    pub fn is_64bit(&self) -> bool {
        matches!(
            self,
            Architecture::X86_64
                | Architecture::AArch64
                | Architecture::RiscV64
                | Architecture::PowerPc64
                | Architecture::Wasm64
        )
    }

    pub fn default_pointer_width(&self) -> PointerWidth {
        if self.is_64bit() {
            PointerWidth::U64
        } else {
            PointerWidth::U32
        }
    }

    pub fn default_endianness(&self) -> Endianness {
        match self {
            Architecture::PowerPc | Architecture::PowerPc64 => Endianness::Big,
            _ => Endianness::Little,
        }
    }
}

/// Operating System.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OperatingSystem {
    None, // Bare metal / freestanding
    #[default]
    Windows,
    Linux,
    MacOS,
    Ios,
    Android,
    FreeBsd,
    NetBsd,
    OpenBsd,
    Wasi,
    CudaRuntime,
    Custom,
}

/// Target Environment / libc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Environment {
    Unknown,
    #[default]
    Msvc,
    Gnu,
    Musl,
    Eabi,
    Eabihf,
    Android,
    Sim,
}

/// Application Binary Interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Abi {
    #[default]
    Default,
    SystemV,
    WindowsX64,
    Aapcs64,
    Aapcs32,
    RiscvAbi,
    WasmAbi,
    AdeshInternal,
    BareMetal,
    CudaKernelAbi,
}

/// Binary Object format target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectFormat {
    #[default]
    Adob,
    PeCoff,
    Elf,
    MachO,
    Wasm,
    RawBinary,
}

/// Endianness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Endianness {
    #[default]
    Little,
    Big,
}

/// Pointer width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerWidth {
    U32,
    #[default]
    U64,
}

impl PointerWidth {
    pub fn bytes(&self) -> u8 {
        match self {
            PointerWidth::U32 => 4,
            PointerWidth::U64 => 8,
        }
    }
}

/// Named target feature with optional version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetFeature {
    pub name: String,
    pub version: Option<String>,
    pub enabled: bool,
}

impl TargetFeature {
    pub fn new(name: impl Into<String>, enabled: bool) -> Self {
        Self {
            name: name.into(),
            version: None,
            enabled,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// Collection of target features.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetFeatures {
    pub features: Vec<TargetFeature>,
}

impl TargetFeatures {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        self.features
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .is_some_and(|f| f.enabled)
    }

    pub fn enable(&mut self, name: impl Into<String>) {
        let name_str = name.into();
        if let Some(f) = self
            .features
            .iter_mut()
            .find(|f| f.name.eq_ignore_ascii_case(&name_str))
        {
            f.enabled = true;
        } else {
            self.features.push(TargetFeature::new(name_str, true));
        }
    }

    pub fn disable(&mut self, name: impl Into<String>) {
        let name_str = name.into();
        if let Some(f) = self
            .features
            .iter_mut()
            .find(|f| f.name.eq_ignore_ascii_case(&name_str))
        {
            f.enabled = false;
        } else {
            self.features.push(TargetFeature::new(name_str, false));
        }
    }
}

/// Complete Target Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDescriptor {
    pub device: ComputeDevice,
    pub architecture: Architecture,
    pub operating_system: OperatingSystem,
    pub environment: Environment,
    pub abi: Abi,
    pub object_format: ObjectFormat,
    pub pointer_width: PointerWidth,
    pub endianness: Endianness,
    pub features: TargetFeatures,
}

impl Default for TargetDescriptor {
    fn default() -> Self {
        Self {
            device: ComputeDevice::Cpu,
            architecture: Architecture::X86_64,
            operating_system: OperatingSystem::Windows,
            environment: Environment::Msvc,
            abi: Abi::WindowsX64,
            object_format: ObjectFormat::Adob,
            pointer_width: PointerWidth::U64,
            endianness: Endianness::Little,
            features: TargetFeatures::new(),
        }
    }
}

impl TargetDescriptor {
    pub fn host() -> Self {
        #[cfg(target_arch = "x86_64")]
        let arch = Architecture::X86_64;
        #[cfg(target_arch = "aarch64")]
        let arch = Architecture::AArch64;
        #[cfg(target_arch = "riscv64")]
        let arch = Architecture::RiscV64;
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )))]
        let arch = Architecture::X86_64;

        #[cfg(target_os = "windows")]
        let (os, env, abi) = (OperatingSystem::Windows, Environment::Msvc, Abi::WindowsX64);
        #[cfg(target_os = "linux")]
        let (os, env, abi) = (OperatingSystem::Linux, Environment::Gnu, Abi::SystemV);
        #[cfg(target_os = "macos")]
        let (os, env, abi) = (OperatingSystem::MacOS, Environment::Unknown, Abi::SystemV);
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        let (os, env, abi) = (OperatingSystem::Windows, Environment::Msvc, Abi::WindowsX64);

        Self {
            device: ComputeDevice::Cpu,
            architecture: arch,
            operating_system: os,
            environment: env,
            abi,
            object_format: ObjectFormat::Adob,
            pointer_width: PointerWidth::U64,
            endianness: Endianness::Little,
            features: TargetFeatures::new(),
        }
    }

    pub fn from_triple(triple: &str) -> Result<Self, String> {
        let parts: Vec<&str> = triple.split('-').collect();
        if parts.is_empty() {
            return Err("Empty target triple".to_string());
        }

        let arch = match parts[0].to_ascii_lowercase().as_str() {
            "x86_64" | "amd64" => Architecture::X86_64,
            "i386" | "i686" | "x86" => Architecture::X86,
            "aarch64" | "arm64" => Architecture::AArch64,
            "arm" | "armv7" | "armv7a" | "armv7m" | "thumbv7em" => Architecture::Arm,
            "riscv32" | "rv32" | "riscv32imac" => Architecture::RiscV32,
            "riscv64" | "rv64" | "riscv64gc" => Architecture::RiscV64,
            "powerpc" | "ppc" => Architecture::PowerPc,
            "powerpc64" | "ppc64" | "ppc64le" => Architecture::PowerPc64,
            "mips" | "mipsel" => Architecture::Mips,
            "wasm32" => Architecture::Wasm32,
            "wasm64" => Architecture::Wasm64,
            "nvptx64" | "cuda" => Architecture::Gpu(GpuArchitecture::NvidiaCuda {
                compute_capability: (8, 0),
            }),
            "amdgcn" => Architecture::Gpu(GpuArchitecture::AmdRdna { generation: 3 }),
            "spirv" => Architecture::Gpu(GpuArchitecture::GenericSpirV),
            other => Architecture::Custom(other.to_string()),
        };

        let ptr_width = arch.default_pointer_width();
        let endianness = arch.default_endianness();

        let mut os = OperatingSystem::None;
        let mut env = Environment::Unknown;
        let mut abi = Abi::Default;

        let lower = triple.to_ascii_lowercase();
        if lower.contains("windows") {
            os = OperatingSystem::Windows;
            abi = Abi::WindowsX64;
            env = if lower.contains("gnu") {
                Environment::Gnu
            } else {
                Environment::Msvc
            };
        } else if lower.contains("linux") {
            os = OperatingSystem::Linux;
            abi = match arch {
                Architecture::AArch64 => Abi::Aapcs64,
                Architecture::RiscV64 | Architecture::RiscV32 => Abi::RiscvAbi,
                _ => Abi::SystemV,
            };
            env = if lower.contains("musl") {
                Environment::Musl
            } else {
                Environment::Gnu
            };
        } else if lower.contains("darwin") || lower.contains("macos") || lower.contains("apple") {
            os = OperatingSystem::MacOS;
            abi = Abi::SystemV;
        } else if lower.contains("wasi") {
            os = OperatingSystem::Wasi;
            abi = Abi::WasmAbi;
        } else if lower.contains("none") || lower.contains("baremetal") {
            os = OperatingSystem::None;
            abi = Abi::BareMetal;
            env = if lower.contains("eabihf") {
                Environment::Eabihf
            } else {
                Environment::Eabi
            };
        }

        let device = match arch {
            Architecture::Gpu(_) => ComputeDevice::Gpu,
            Architecture::Npu(_) => ComputeDevice::Npu,
            Architecture::Tpu(_) => ComputeDevice::Tpu,
            Architecture::Embedded(_) => ComputeDevice::Embedded,
            _ if os == OperatingSystem::None => ComputeDevice::Embedded,
            _ => ComputeDevice::Cpu,
        };

        Ok(Self {
            device,
            architecture: arch,
            operating_system: os,
            environment: env,
            abi,
            object_format: ObjectFormat::Adob,
            pointer_width: ptr_width,
            endianness,
            features: TargetFeatures::new(),
        })
    }

    pub fn triple_string(&self) -> String {
        let arch_str = match &self.architecture {
            Architecture::X86_64 => "x86_64",
            Architecture::X86 => "i686",
            Architecture::AArch64 => "aarch64",
            Architecture::Arm => "arm",
            Architecture::RiscV64 => "riscv64",
            Architecture::RiscV32 => "riscv32",
            Architecture::PowerPc64 => "powerpc64",
            Architecture::PowerPc => "powerpc",
            Architecture::Mips => "mips",
            Architecture::Wasm32 => "wasm32",
            Architecture::Wasm64 => "wasm64",
            Architecture::Gpu(_) => "gpu",
            Architecture::Npu(_) => "npu",
            Architecture::Tpu(_) => "tpu",
            Architecture::Embedded(_) => "embedded",
            Architecture::Custom(s) => s.as_str(),
        };

        let os_str = match self.operating_system {
            OperatingSystem::Windows => "windows",
            OperatingSystem::Linux => "linux",
            OperatingSystem::MacOS => "macos",
            OperatingSystem::Ios => "ios",
            OperatingSystem::Android => "android",
            OperatingSystem::FreeBsd => "freebsd",
            OperatingSystem::NetBsd => "netbsd",
            OperatingSystem::OpenBsd => "openbsd",
            OperatingSystem::Wasi => "wasi",
            OperatingSystem::CudaRuntime => "cuda",
            OperatingSystem::None => "none",
            OperatingSystem::Custom => "custom",
        };

        let env_str = match self.environment {
            Environment::Msvc => "msvc",
            Environment::Gnu => "gnu",
            Environment::Musl => "musl",
            Environment::Eabi => "eabi",
            Environment::Eabihf => "eabihf",
            Environment::Android => "android",
            Environment::Sim => "sim",
            Environment::Unknown => "unknown",
        };

        format!("{}-{}-{}", arch_str, os_str, env_str)
    }
}

impl fmt::Display for TargetDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.triple_string())
    }
}
