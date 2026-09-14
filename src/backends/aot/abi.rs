// ABI (Application Binary Interface) compatibility
// Generic, language-agnostic ABI specifications

/// Calling convention
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    /// System V AMD64 ABI (Linux, macOS, BSD)
    SystemV,
    /// Windows x64 calling convention
    Win64,
    /// ARM AAPCS
    ARM,
    /// Custom convention
    Custom,
}

/// Register classification for parameter passing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterClass {
    /// Integer/pointer in register
    Integer,
    /// Floating point in register
    SSE,
    /// Passed in memory
    Memory,
    /// No class (empty struct)
    NoClass,
}

/// Data layout specification
#[derive(Debug, Clone)]
pub struct DataLayout {
    /// Pointer size in bytes
    pub pointer_size: usize,
    /// Pointer alignment in bytes
    pub pointer_align: usize,
    /// Integer alignments (size -> alignment)
    pub int_align: Vec<(usize, usize)>,
    /// Float alignments (size -> alignment)
    pub float_align: Vec<(usize, usize)>,
    /// Aggregate alignment
    pub aggregate_align: usize,
    /// Stack alignment
    pub stack_align: usize,
}

impl DataLayout {
    /// Create System V data layout (64-bit)
    pub fn system_v_64() -> Self {
        Self {
            pointer_size: 8,
            pointer_align: 8,
            int_align: vec![
                (1, 1), // i8
                (2, 2), // i16
                (4, 4), // i32
                (8, 8), // i64
            ],
            float_align: vec![
                (4, 4), // f32
                (8, 8), // f64
            ],
            aggregate_align: 8,
            stack_align: 16,
        }
    }

    /// Create Windows x64 data layout
    pub fn win64() -> Self {
        Self {
            pointer_size: 8,
            pointer_align: 8,
            int_align: vec![(1, 1), (2, 2), (4, 4), (8, 8)],
            float_align: vec![(4, 4), (8, 8)],
            aggregate_align: 8,
            stack_align: 16,
        }
    }

    /// Get alignment for integer type
    pub fn int_alignment(&self, size: usize) -> usize {
        self.int_align
            .iter()
            .find(|(s, _)| *s == size)
            .map(|(_, a)| *a)
            .unwrap_or(self.aggregate_align)
    }

    /// Get alignment for float type
    pub fn float_alignment(&self, size: usize) -> usize {
        self.float_align
            .iter()
            .find(|(s, _)| *s == size)
            .map(|(_, a)| *a)
            .unwrap_or(self.aggregate_align)
    }
}

/// ABI specification
#[derive(Debug, Clone)]
pub struct Abi {
    /// Calling convention
    pub convention: CallingConvention,
    /// Data layout
    pub layout: DataLayout,
    /// Integer argument registers (System V)
    pub int_arg_regs: Vec<String>,
    /// Float argument registers (System V)
    pub float_arg_regs: Vec<String>,
    /// Return value registers
    pub return_regs: Vec<String>,
}

impl Abi {
    /// Create System V ABI
    pub fn system_v() -> Self {
        Self {
            convention: CallingConvention::SystemV,
            layout: DataLayout::system_v_64(),
            int_arg_regs: vec![
                "rdi".to_string(),
                "rsi".to_string(),
                "rdx".to_string(),
                "rcx".to_string(),
                "r8".to_string(),
                "r9".to_string(),
            ],
            float_arg_regs: vec![
                "xmm0".to_string(),
                "xmm1".to_string(),
                "xmm2".to_string(),
                "xmm3".to_string(),
                "xmm4".to_string(),
                "xmm5".to_string(),
                "xmm6".to_string(),
                "xmm7".to_string(),
            ],
            return_regs: vec!["rax".to_string(), "rdx".to_string()],
        }
    }

    /// Create Windows x64 ABI
    pub fn win64() -> Self {
        Self {
            convention: CallingConvention::Win64,
            layout: DataLayout::win64(),
            int_arg_regs: vec![
                "rcx".to_string(),
                "rdx".to_string(),
                "r8".to_string(),
                "r9".to_string(),
            ],
            float_arg_regs: vec![
                "xmm0".to_string(),
                "xmm1".to_string(),
                "xmm2".to_string(),
                "xmm3".to_string(),
            ],
            return_regs: vec!["rax".to_string()],
        }
    }

    /// Detect platform ABI
    pub fn platform() -> Self {
        #[cfg(target_os = "windows")]
        return Self::win64();

        #[cfg(not(target_os = "windows"))]
        return Self::system_v();
    }

    /// Classify argument for register allocation
    pub fn classify_arg(&self, _size: usize, is_float: bool) -> RegisterClass {
        if is_float {
            if !self.float_arg_regs.is_empty() {
                RegisterClass::SSE
            } else {
                RegisterClass::Memory
            }
        } else {
            if !self.int_arg_regs.is_empty() {
                RegisterClass::Integer
            } else {
                RegisterClass::Memory
            }
        }
    }

    /// Get stack alignment
    pub fn stack_alignment(&self) -> usize {
        self.layout.stack_align
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_v_abi() {
        let abi = Abi::system_v();
        assert_eq!(abi.convention, CallingConvention::SystemV);
        assert_eq!(abi.layout.pointer_size, 8);
        assert_eq!(abi.int_arg_regs.len(), 6);
    }

    #[test]
    fn test_win64_abi() {
        let abi = Abi::win64();
        assert_eq!(abi.convention, CallingConvention::Win64);
        assert_eq!(abi.int_arg_regs.len(), 4);
    }

    #[test]
    fn test_data_layout() {
        let layout = DataLayout::system_v_64();
        assert_eq!(layout.int_alignment(4), 4);
        assert_eq!(layout.float_alignment(8), 8);
    }
}
