//! Standardized Application Binary Interface (ABI) descriptors and in-memory layouts for ADOB.

/// Standardized runtime ABI versions.
pub const ADESH_ABI_V1: u32 = 1;

/// Standardized in-memory type representation tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum AbiTypeKind {
    #[default]
    Void = 0,
    Bool = 1,
    Int8 = 2,
    Int16 = 3,
    Int32 = 4,
    Int64 = 5,
    Int128 = 6,
    UInt8 = 7,
    UInt16 = 8,
    UInt32 = 9,
    UInt64 = 10,
    UInt128 = 11,
    Float32 = 12,
    Float64 = 13,
    Pointer = 14,
    String = 15,
    Slice = 16,
    Array = 17,
    Option = 18,
    Result = 19,
    Closure = 20,
    FatPointer = 21,
    Struct = 22,
    Enum = 23,
    Custom = 255,
}

/// Standardized field layout for composite types in C-ABI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AbiFieldDescriptor {
    pub name: String,
    pub type_kind: AbiTypeKind,
    pub offset: usize,
    pub size: usize,
    pub align: usize,
}

/// Complete ABI composite struct descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AbiStructDescriptor {
    pub name: String,
    pub size: usize,
    pub align: usize,
    pub fields: Vec<AbiFieldDescriptor>,
}

impl AbiStructDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 0,
            align: 8,
            fields: Vec::new(),
        }
    }

    pub fn add_field(
        &mut self,
        name: impl Into<String>,
        type_kind: AbiTypeKind,
        size: usize,
        align: usize,
    ) {
        let align_val = if align == 0 { 1 } else { align };
        let offset = (self.size + align_val - 1) & !(align_val - 1);
        self.fields.push(AbiFieldDescriptor {
            name: name.into(),
            type_kind,
            offset,
            size,
            align: align_val,
        });
        self.size = offset + size;
        if align_val > self.align {
            self.align = align_val;
        }
    }
}
