//! SIMD type representation in the compiler IR

use std::fmt;

/// Scalar element types supported in SIMD vectors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimdElement {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
}

impl SimdElement {
    pub fn element_size(&self) -> usize {
        match self {
            SimdElement::I8 | SimdElement::U8 | SimdElement::Bool => 1,
            SimdElement::I16 | SimdElement::U16 => 2,
            SimdElement::I32 | SimdElement::U32 | SimdElement::F32 => 4,
            SimdElement::I64 | SimdElement::U64 | SimdElement::F64 => 8,
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, SimdElement::F32 | SimdElement::F64)
    }

    pub fn is_integer(&self) -> bool {
        !self.is_float() && !matches!(self, SimdElement::Bool)
    }

    /// Parse from AdeshLang type name (e.g. "f32", "i64")
    pub fn from_type_name(name: &str) -> Option<Self> {
        match name {
            "i8" => Some(SimdElement::I8),
            "i16" => Some(SimdElement::I16),
            "i32" => Some(SimdElement::I32),
            "i64" => Some(SimdElement::I64),
            "u8" => Some(SimdElement::U8),
            "u16" => Some(SimdElement::U16),
            "u32" => Some(SimdElement::U32),
            "u64" => Some(SimdElement::U64),
            "f32" => Some(SimdElement::F32),
            "f64" => Some(SimdElement::F64),
            "bool" => Some(SimdElement::Bool),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            SimdElement::I8 => "i8",
            SimdElement::I16 => "i16",
            SimdElement::I32 => "i32",
            SimdElement::I64 => "i64",
            SimdElement::U8 => "u8",
            SimdElement::U16 => "u16",
            SimdElement::U32 => "u32",
            SimdElement::U64 => "u64",
            SimdElement::F32 => "f32",
            SimdElement::F64 => "f64",
            SimdElement::Bool => "bool",
        }
    }
}

/// A SIMD vector type: Simd<T, LANES>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimdType {
    pub element: SimdElement,
    pub lanes: u32,
}

impl SimdType {
    pub fn new(element: SimdElement, lanes: u32) -> Self {
        debug_assert!(lanes > 0, "SIMD lanes must be > 0");
        SimdType { element, lanes }
    }

    pub fn total_bytes(&self) -> usize {
        self.element.element_size() * self.lanes as usize
    }

    /// Parse `vec4<f32>` or `Simd<f32, 4>` annotation strings
    pub fn from_annotation(s: &str) -> Option<Self> {
        let s = s.trim();
        // vecN<T> form
        if let Some(rest) = s.strip_prefix("vec") {
            if let Some(lt) = rest.find('<') {
                let lanes_str = &rest[..lt];
                if let Ok(lanes) = lanes_str.parse::<u32>() {
                    let elem = rest[lt + 1..].trim_end_matches('>').trim_end_matches('?');
                    if let Some(e) = SimdElement::from_type_name(elem) {
                        return Some(SimdType::new(e, lanes));
                    }
                }
            }
        }
        // Simd<T, N> form
        if let Some(rest) = s.strip_prefix("Simd<") {
            let inner = rest.trim_end_matches('>').trim_end_matches('?');
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                let elem = SimdElement::from_type_name(parts[0])?;
                let lanes = parts[1].parse::<u32>().ok()?;
                return Some(SimdType::new(elem, lanes));
            }
        }
        None
    }
}

impl fmt::Display for SimdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Simd<{}, {}>", self.element.type_name(), self.lanes)
    }
}

/// Target SIMD instruction set capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdIsa {
    Scalar,
    Sse2,
    Sse42,
    Avx,
    Avx2,
    Avx512,
    Neon,
    Sve,
}

impl SimdIsa {
    pub fn max_lanes(&self, element: SimdElement) -> u32 {
        let elem_bytes = element.element_size();
        let register_bytes: u32 = match self {
            SimdIsa::Scalar => 0,
            SimdIsa::Sse2 | SimdIsa::Sse42 => 16,
            SimdIsa::Avx | SimdIsa::Avx2 => 32,
            SimdIsa::Avx512 => 64,
            SimdIsa::Neon => 16,
            SimdIsa::Sve => 256, // scalable; conservative default
        };
        if register_bytes == 0 {
            return 1;
        }
        (register_bytes / elem_bytes as u32).max(1)
    }
}

/// Alignment information tracked through the compiler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Unknown,
    Known(u32),
}

impl Alignment {
    pub fn is_aligned_for(&self, simd_type: &SimdType) -> bool {
        match self {
            Alignment::Unknown => false,
            Alignment::Known(align) => *align >= simd_type.element.element_size() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_type_parse_vec() {
        let t = SimdType::from_annotation("vec4<f32>").unwrap();
        assert_eq!(t.element, SimdElement::F32);
        assert_eq!(t.lanes, 4);
    }

    #[test]
    fn test_simd_type_parse_simd() {
        let t = SimdType::from_annotation("Simd<f64, 8>").unwrap();
        assert_eq!(t.element, SimdElement::F64);
        assert_eq!(t.lanes, 8);
    }
}
