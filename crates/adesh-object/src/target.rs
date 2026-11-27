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
    I8086,
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
    S390x,
    Sparc64,
    Wasm32,
    Wasm64,
    Dsp56000,
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
                | Architecture::S390x
                | Architecture::Sparc64
                | Architecture::Wasm64
        )
    }

    pub fn default_pointer_width(&self) -> PointerWidth {
        match self {
            Architecture::X86_64
            | Architecture::AArch64
            | Architecture::RiscV64
            | Architecture::PowerPc64
            | Architecture::S390x
            | Architecture::Sparc64
            | Architecture::Wasm64 => PointerWidth::U64,
            Architecture::I8086
            | Architecture::Dsp56000
            | Architecture::Embedded(EmbeddedArchitecture::Avr) => PointerWidth::U16,
            Architecture::X86
            | Architecture::Arm
            | Architecture::RiscV32
            | Architecture::PowerPc
            | Architecture::Mips
            | Architecture::Wasm32
            | Architecture::Embedded(_)
            | Architecture::Gpu(_)
            | Architecture::Npu(_)
            | Architecture::Tpu(_)
            | Architecture::Custom(_) => PointerWidth::U32,
        }
    }

    pub fn default_endianness(&self) -> Endianness {
        match self {
            Architecture::PowerPc
            | Architecture::PowerPc64
            | Architecture::Mips
            | Architecture::S390x
            | Architecture::Sparc64
            | Architecture::Dsp56000 => Endianness::Big,
            _ => Endianness::Little,
        }
    }
}

/// Operating System.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OperatingSystem {
    None, // Bare metal / freestanding
    Dos,
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
    DosRealMode16,
    AvrBareMetal,
    Dsp56000,
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

/// Pointer width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PointerWidth {
    U8,
    U16,
    U24,
    U32,
    #[default]
    U64,
}

impl PointerWidth {
    pub fn bytes(&self) -> u8 {
        match self {
            PointerWidth::U8 => 1,
            PointerWidth::U16 => 2,
            PointerWidth::U24 => 3,
            PointerWidth::U32 => 4,
            PointerWidth::U64 => 8,
        }
    }

    pub fn bits(&self) -> usize {
        self.bytes() as usize * 8
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

        let arch_str = parts[0].to_ascii_lowercase();
        let is_le_override = arch_str.ends_with("le") || arch_str.ends_with("el");
        let arch = match arch_str.as_str() {
            "x86_64" | "amd64" => Architecture::X86_64,
            "i386" | "i686" | "x86" => Architecture::X86,
            "i8086" | "8086" | "x86_16" => Architecture::I8086,
            "avr" | "atmega328p" | "atmega2560" => {
                Architecture::Embedded(EmbeddedArchitecture::Avr)
            }
            "dsp56000" | "dsp56k" | "dsp56001" => Architecture::Dsp56000,
            "aarch64" | "arm64" => Architecture::AArch64,
            "arm" | "armv7" | "armv7a" => Architecture::Arm,
            "armv7m" | "thumbv7em" | "thumbv6m" => {
                Architecture::Embedded(EmbeddedArchitecture::ArmCortexM4F)
            }
            "riscv32" | "rv32" | "riscv32imac" => Architecture::RiscV32,
            "riscv64" | "rv64" | "riscv64gc" => Architecture::RiscV64,
            "powerpc" | "ppc" => Architecture::PowerPc,
            "powerpc64" | "ppc64" | "powerpc64le" | "ppc64le" => Architecture::PowerPc64,
            "mips" | "mipsel" | "mips64" | "mips64el" => Architecture::Mips,
            "s390x" | "s390" => Architecture::S390x,
            "sparc64" | "sparc" => Architecture::Sparc64,
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
        let endianness = if is_le_override {
            Endianness::Little
        } else {
            arch.default_endianness()
        };

        let mut os = OperatingSystem::None;
        let mut env = Environment::Unknown;
        let mut abi = Abi::Default;

        let lower = triple.to_ascii_lowercase();
        if lower.contains("dos") || arch == Architecture::I8086 {
            os = OperatingSystem::Dos;
            abi = Abi::DosRealMode16;
            env = Environment::Unknown;
        } else if arch == Architecture::Embedded(EmbeddedArchitecture::Avr) {
            os = OperatingSystem::None;
            abi = Abi::AvrBareMetal;
            env = Environment::Unknown;
        } else if arch == Architecture::Dsp56000 {
            os = OperatingSystem::None;
            abi = Abi::Dsp56000;
            env = Environment::Unknown;
        } else if lower.contains("windows") {
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
            Architecture::Dsp56000 => ComputeDevice::Dsp,
            Architecture::Embedded(_) => ComputeDevice::Embedded,
            _ if os == OperatingSystem::None => ComputeDevice::Embedded,
            _ => ComputeDevice::Cpu,
        };

        let mut features = TargetFeatures::new();
        if arch_str == "atmega2560" {
            features.enable("atmega2560");
            features.enable("avr6");
        } else if arch == Architecture::Embedded(EmbeddedArchitecture::Avr) {
            features.enable("atmega328p");
            features.enable("avr5");
        }

        Ok(Self {
            device,
            architecture: arch,
            operating_system: os,
            environment: env,
            abi,
            object_format: ObjectFormat::Adob,
            pointer_width: ptr_width,
            endianness,
            features,
        })
    }

    /// The image format the linker produces for this target.
    ///
    /// Deliberately not `object_format`: that field names the container this
    /// toolchain *emits* (always ADOB), so it says nothing about the final
    /// image. Codegen needs the image format because the image decides which
    /// addressing forms are legal — a Mach-O image is always PIE and dyld
    /// applies fixups only in writable segments, so an absolute pointer baked
    /// into code has no legal relocation and fails the link.
    pub fn image_format(&self) -> ObjectFormat {
        match (&self.architecture, self.operating_system) {
            (Architecture::I8086, _) | (_, OperatingSystem::Dos) => ObjectFormat::RawBinary,
            (_, OperatingSystem::Windows) => ObjectFormat::PeCoff,
            (_, OperatingSystem::MacOS | OperatingSystem::Ios) => ObjectFormat::MachO,
            (_, OperatingSystem::Wasi) => ObjectFormat::Wasm,
            (Architecture::Wasm32 | Architecture::Wasm64, _) => ObjectFormat::Wasm,
            _ => ObjectFormat::Elf,
        }
    }

    pub fn arch_descriptor(&self) -> &'static crate::arch::ArchitectureDescriptor {
        crate::arch::ArchitectureDescriptor::for_target(self)
    }

    pub fn data_layout(&self) -> &'static crate::arch::DataLayoutSpec {
        &self.arch_descriptor().data_layout
    }

    pub fn triple_string(&self) -> String {
        let arch_str = match &self.architecture {
            Architecture::X86_64 => "x86_64",
            Architecture::X86 => "i686",
            Architecture::I8086 => "i8086",
            Architecture::AArch64 => "aarch64",
            Architecture::Arm => "arm",
            Architecture::RiscV64 => "riscv64",
            Architecture::RiscV32 => "riscv32",
            Architecture::PowerPc64 => "powerpc64",
            Architecture::PowerPc => "powerpc",
            Architecture::Mips => "mips",
            Architecture::S390x => "s390x",
            Architecture::Sparc64 => "sparc64",
            Architecture::Wasm32 => "wasm32",
            Architecture::Wasm64 => "wasm64",
            Architecture::Dsp56000 => "dsp56000",
            Architecture::Gpu(_) => "gpu",
            Architecture::Npu(_) => "npu",
            Architecture::Tpu(_) => "tpu",
            Architecture::Embedded(EmbeddedArchitecture::Avr) => "avr",
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
            OperatingSystem::Dos => "dos",
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

        if matches!(
            (&self.architecture, self.operating_system),
            (Architecture::I8086, OperatingSystem::Dos)
        ) {
            return format!("{}-unknown-{}", arch_str, os_str);
        }

        // Bare-metal AVR and DSP56000 are advertised, documented, and accepted as
        // `<arch>-unknown-unknown`; diagnostics must name that same spelling.
        if matches!(
            (&self.architecture, self.operating_system),
            (
                Architecture::Embedded(EmbeddedArchitecture::Avr),
                OperatingSystem::None
            ) | (Architecture::Dsp56000, OperatingSystem::None)
        ) {
            return format!("{}-unknown-unknown", arch_str);
        }

        format!("{}-{}-{}", arch_str, os_str, env_str)
    }
}

impl fmt::Display for TargetDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.triple_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `object_format` names the container this toolchain emits, so it is the
    /// wrong thing for codegen to branch on when it needs to know what the
    /// final image will be — a guard written against it silently never fires.
    #[test]
    fn image_format_comes_from_the_operating_system_not_the_emitted_container() {
        for triple in [
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            "x86_64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
        ] {
            let desc =
                TargetDescriptor::from_triple(triple).unwrap_or_else(|e| panic!("{triple}: {e}"));
            assert_eq!(
                desc.object_format,
                ObjectFormat::Adob,
                "{triple}: every triple is emitted as ADOB, so `object_format` cannot \
                 distinguish an apple-darwin image from a windows one"
            );
        }

        for (triple, expected) in [
            ("x86_64-apple-darwin", ObjectFormat::MachO),
            ("aarch64-apple-darwin", ObjectFormat::MachO),
            ("x86_64-unknown-linux-gnu", ObjectFormat::Elf),
            ("aarch64-unknown-linux-gnu", ObjectFormat::Elf),
            ("x86_64-pc-windows-msvc", ObjectFormat::PeCoff),
        ] {
            let desc =
                TargetDescriptor::from_triple(triple).unwrap_or_else(|e| panic!("{triple}: {e}"));
            assert_eq!(
                desc.image_format(),
                expected,
                "{triple} links into a {expected:?} image"
            );
        }
    }
}
