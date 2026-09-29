//! Formal `adesh::ffi` ABI specification, C/Rust ABI interoperability,
//! struct layout verification, and panic boundary policies.

use std::fmt;

/// FFI Calling Convention / ABI specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallingConvention {
    C,
    Rust,
    System,
    Cdecl,
    Stdcall,
    Fastcall,
    AdeshRuntimeV1,
}

impl CallingConvention {
    pub fn as_str(&self) -> &'static str {
        match self {
            CallingConvention::C => "C",
            CallingConvention::Rust => "Rust",
            CallingConvention::System => "system",
            CallingConvention::Cdecl => "cdecl",
            CallingConvention::Stdcall => "stdcall",
            CallingConvention::Fastcall => "fastcall",
            CallingConvention::AdeshRuntimeV1 => "AdeshRuntimeV1",
        }
    }
}

/// FFI Panic boundary policy across foreign language barriers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FfiPanicPolicy {
    /// Abort execution immediately on panic before crossing FFI boundary.
    Abort,
    /// Catch panic at boundary and return default/null.
    Catch,
    /// Translate panic to structured C error code or Result wrapper.
    TranslatedError,
}

impl FfiPanicPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            FfiPanicPolicy::Abort => "abort",
            FfiPanicPolicy::Catch => "catch",
            FfiPanicPolicy::TranslatedError => "translated_error",
        }
    }
}

/// C-compatible repr(C) struct field layout descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CFieldLayout {
    pub name: String,
    pub type_name: String,
    pub size: usize,
    pub align: usize,
    pub offset: usize,
}

/// C-compatible repr(C) struct layout calculator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CStructLayout {
    pub name: String,
    pub fields: Vec<CFieldLayout>,
    pub total_size: usize,
    pub align: usize,
}

impl CStructLayout {
    pub fn compute(name: impl Into<String>, fields_info: &[(&str, &str, usize, usize)]) -> Self {
        let mut fields = Vec::new();
        let mut current_offset = 0;
        let mut struct_align = 1;

        for &(fname, ftype, fsize, falign) in fields_info {
            let align = falign.max(1);
            struct_align = struct_align.max(align);

            // Pad offset to match field alignment
            if current_offset % align != 0 {
                current_offset += align - (current_offset % align);
            }

            fields.push(CFieldLayout {
                name: fname.to_string(),
                type_name: ftype.to_string(),
                size: fsize,
                align,
                offset: current_offset,
            });

            current_offset += fsize;
        }

        // Pad struct size to multiple of struct alignment
        if struct_align > 0 && current_offset % struct_align != 0 {
            current_offset += struct_align - (current_offset % struct_align);
        }

        Self {
            name: name.into(),
            fields,
            total_size: current_offset,
            align: struct_align,
        }
    }

    pub fn offset_of(&self, field_name: &str) -> Option<usize> {
        self.fields
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.offset)
    }
}

/// Adesh Runtime ABI Version 1 constant.
pub const ADESH_RUNTIME_ABI_VERSION: &str = "ADESH_RUNTIME_ABI_V1";

/// Core Adesh runtime exported ABI function contracts.
#[derive(Debug, Clone)]
pub struct RuntimeSymbolContract {
    pub name: &'static str,
    pub signature: &'static str,
    pub description: &'static str,
}

pub const ADESH_RUNTIME_CONTRACTS: &[RuntimeSymbolContract] = &[
    RuntimeSymbolContract {
        name: "__adesh_alloc",
        signature: "extern \"C\" fn __adesh_alloc(size: usize, align: usize) -> *mut u8",
        description: "Allocates memory with specified size and alignment.",
    },
    RuntimeSymbolContract {
        name: "__adesh_free",
        signature: "extern \"C\" fn __adesh_free(ptr: *mut u8, size: usize, align: usize)",
        description: "Deallocates memory block.",
    },
    RuntimeSymbolContract {
        name: "__adesh_panic",
        signature: "extern \"C\" fn __adesh_panic(msg: *const u8, len: usize) -> !",
        description: "Triggers runtime panic with message.",
    },
    RuntimeSymbolContract {
        name: "__adesh_thread_init",
        signature: "extern \"C\" fn __adesh_thread_init()",
        description: "Initializes thread-local state.",
    },
];

impl fmt::Display for CStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "struct {} (size: {}, align: {}):",
            self.name, self.total_size, self.align
        )?;
        for field in &self.fields {
            writeln!(
                f,
                "  +{} {}: {} (size: {}, align: {})",
                field.offset, field.name, field.type_name, field.size, field.align
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_struct_layout_padding_alignment() {
        // C struct layout test: struct { u8 a; u64 b; u32 c; }
        // expected: a @ 0, padding 7 bytes, b @ 8, c @ 16, total size padded to 24 (align 8)
        let layout = CStructLayout::compute(
            "TestStruct",
            &[("a", "u8", 1, 1), ("b", "u64", 8, 8), ("c", "u32", 4, 4)],
        );

        assert_eq!(layout.offset_of("a"), Some(0));
        assert_eq!(layout.offset_of("b"), Some(8));
        assert_eq!(layout.offset_of("c"), Some(16));
        assert_eq!(layout.total_size, 24);
        assert_eq!(layout.align, 8);
    }
}
