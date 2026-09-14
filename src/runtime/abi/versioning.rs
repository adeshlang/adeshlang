//! ABI Versioning System
//!
//! Ensures stable ABI across compiler versions and provides
//! compatibility guarantees.

/// ABI version number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AbiVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl AbiVersion {
    /// Current ABI version
    pub const CURRENT: AbiVersion = AbiVersion {
        major: 1,
        minor: 0,
        patch: 0,
    };

    /// Creates a new ABI version
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        AbiVersion {
            major,
            minor,
            patch,
        }
    }

    /// Checks if this version is compatible with another
    ///
    /// Compatible if:
    /// - Major versions match
    /// - This minor >= other minor (forward compatible)
    pub fn is_compatible_with(&self, other: &AbiVersion) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Checks if this is a breaking change from another version
    pub fn is_breaking_change_from(&self, other: &AbiVersion) -> bool {
        self.major != other.major
    }

    /// Returns the version as a string
    pub fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// ABI calling convention
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    /// C calling convention (default for extern)
    C,
    /// Adesh internal calling convention
    Adesh,
    /// System calling convention
    System,
    /// Fast calling convention (optimized for speed)
    Fast,
}

impl CallingConvention {
    /// Returns the convention name
    pub fn name(&self) -> &'static str {
        match self {
            CallingConvention::C => "C",
            CallingConvention::Adesh => "adesh",
            CallingConvention::System => "system",
            CallingConvention::Fast => "fast",
        }
    }
}

/// Struct layout representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructLayout {
    /// C-compatible layout (for FFI)
    C,
    /// Adesh default layout (optimized)
    Adesh,
    /// Packed layout (no padding)
    Packed,
}

/// ABI attribute for functions and types
#[derive(Debug, Clone)]
pub struct AbiAttribute {
    /// Calling convention
    pub calling_convention: CallingConvention,
    /// Minimum ABI version required
    pub min_version: AbiVersion,
    /// Whether this is an FFI boundary
    pub is_ffi_boundary: bool,
}

impl AbiAttribute {
    /// Creates a default ABI attribute
    pub fn default() -> Self {
        AbiAttribute {
            calling_convention: CallingConvention::Adesh,
            min_version: AbiVersion::CURRENT,
            is_ffi_boundary: false,
        }
    }

    /// Creates an extern C ABI attribute
    pub fn extern_c() -> Self {
        AbiAttribute {
            calling_convention: CallingConvention::C,
            min_version: AbiVersion::CURRENT,
            is_ffi_boundary: true,
        }
    }
}

/// Type representation attribute
#[derive(Debug, Clone)]
pub struct TypeRepr {
    /// Layout representation
    pub layout: StructLayout,
    /// Alignment in bytes (0 = default)
    pub align: usize,
    /// Is this type transparent (same representation as inner type)?
    pub transparent: bool,
}

impl TypeRepr {
    /// Creates a default type representation
    pub fn default() -> Self {
        TypeRepr {
            layout: StructLayout::Adesh,
            align: 0,
            transparent: false,
        }
    }

    /// Creates a C-compatible representation
    pub fn c_repr() -> Self {
        TypeRepr {
            layout: StructLayout::C,
            align: 0,
            transparent: false,
        }
    }

    /// Creates a packed representation
    pub fn packed() -> Self {
        TypeRepr {
            layout: StructLayout::Packed,
            align: 1,
            transparent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version_compatibility() {
        let v1_0_0 = AbiVersion::new(1, 0, 0);
        let v1_1_0 = AbiVersion::new(1, 1, 0);
        let v2_0_0 = AbiVersion::new(2, 0, 0);

        assert!(v1_1_0.is_compatible_with(&v1_0_0));
        assert!(!v1_0_0.is_compatible_with(&v1_1_0));
        assert!(!v2_0_0.is_compatible_with(&v1_0_0));
        assert!(v2_0_0.is_breaking_change_from(&v1_0_0));
    }

    #[test]
    fn test_calling_convention() {
        let cc = CallingConvention::C;
        assert_eq!(cc.name(), "C");
    }
}
