//! System V AMD64 ABI (x86-64 psABI) argument and return classification.
//!
//! Implements Section 3.2.3 ("Parameter Passing") and "Returning of Values" of the
//! System V AMD64 psABI specification:
//! - Eightbyte classification: INTEGER, SSE, SSEUP, X87, X87UP, COMPLEX_X87, NO_CLASS, MEMORY.
//! - Struct field flattening and recursive eightbyte merging.
//! - Register assignment for up to two eightbytes across GPRs (RDI..R9) and XMMs (XMM0..XMM7).
//! - Fallback to stack memory if insufficient registers for all eightbytes.
//! - Aggregate return classification (RAX, RDX, XMM0, XMM1, or hidden sret in RDI).

use crate::abi::{AbiType, ArgumentLocation, ReturnLocation, StackArgument};
use crate::machine_ir::PhysicalRegister;

/// System V AMD64 eightbyte classes per psABI section 3.2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EightbyteClass {
    NoClass,
    Integer,
    Sse,
    SseUp,
    X87,
    X87Up,
    ComplexX87,
    Memory,
}

impl EightbyteClass {
    /// Merge two eightbyte classes according to psABI rules:
    /// (a) If both classes are equal, this is the resulting class.
    /// (b) If one of the classes is NO_CLASS, the resulting class is the other class.
    /// (c) If one of the classes is MEMORY, the result is MEMORY.
    /// (d) If one of the classes is INTEGER, the result is INTEGER.
    /// (e) If one of the classes is X87, X87UP, or COMPLEX_X87, the result is MEMORY.
    /// (f) Otherwise class is SSE.
    pub fn merge(self, other: EightbyteClass) -> EightbyteClass {
        if self == other {
            return self;
        }
        if self == EightbyteClass::NoClass {
            return other;
        }
        if other == EightbyteClass::NoClass {
            return self;
        }
        if self == EightbyteClass::Memory || other == EightbyteClass::Memory {
            return EightbyteClass::Memory;
        }
        if self == EightbyteClass::Integer || other == EightbyteClass::Integer {
            return EightbyteClass::Integer;
        }
        if matches!(
            self,
            EightbyteClass::X87 | EightbyteClass::X87Up | EightbyteClass::ComplexX87
        ) || matches!(
            other,
            EightbyteClass::X87 | EightbyteClass::X87Up | EightbyteClass::ComplexX87
        ) {
            return EightbyteClass::Memory;
        }
        EightbyteClass::Sse
    }
}

pub const SYSV_GPR_ARGS: [PhysicalRegister; 6] = [
    PhysicalRegister(7), // RDI
    PhysicalRegister(6), // RSI
    PhysicalRegister(2), // RDX
    PhysicalRegister(1), // RCX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

pub const SYSV_FPR_ARGS: [PhysicalRegister; 8] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
    PhysicalRegister::xmm(4),
    PhysicalRegister::xmm(5),
    PhysicalRegister::xmm(6),
    PhysicalRegister::xmm(7),
];

/// Classify primitive types into eightbytes.
fn classify_primitive_eightbytes(ty: &AbiType) -> Vec<EightbyteClass> {
    match ty {
        AbiType::Void => vec![],
        AbiType::Integer { bits, .. } => {
            if *bits > 64 {
                // 128-bit integers use two INTEGER eightbytes
                vec![EightbyteClass::Integer, EightbyteClass::Integer]
            } else {
                vec![EightbyteClass::Integer]
            }
        }
        AbiType::Pointer => vec![EightbyteClass::Integer],
        AbiType::Float { .. } => vec![EightbyteClass::Sse],
        AbiType::Vector { total_bytes, .. } => {
            if *total_bytes > 8 {
                vec![EightbyteClass::Sse, EightbyteClass::SseUp]
            } else {
                vec![EightbyteClass::Sse]
            }
        }
        AbiType::Struct { .. } => classify_eightbytes(ty),
    }
}

/// Recursively walk fields of an aggregate and merge classes into eightbytes.
fn walk_struct_fields(fields: &[AbiType], base_offset: usize, eightbytes: &mut [EightbyteClass]) {
    let mut current_offset = base_offset;
    for field in fields {
        let align = field.alignment().max(1);
        current_offset = (current_offset + align - 1) & !(align - 1);
        match field {
            AbiType::Struct { fields: inner, .. } => {
                walk_struct_fields(inner, current_offset, eightbytes);
                current_offset += field.size_in_bytes();
            }
            _ => {
                let f_size = field.size_in_bytes();
                let f_classes = classify_primitive_eightbytes(field);
                for (i, &f_class) in f_classes.iter().enumerate() {
                    let byte_pos = current_offset + i * 8;
                    let eb_idx = byte_pos / 8;
                    if eb_idx < eightbytes.len() {
                        eightbytes[eb_idx] = eightbytes[eb_idx].merge(f_class);
                    }
                }
                current_offset += f_size;
            }
        }
    }
}

/// Classifies any `AbiType` into its System V eightbyte classes (up to 2 eightbytes).
///
/// Returns an empty vector for `Void` or 0-sized types,
/// or `[Memory]` if the type exceeds 16 bytes or must be passed in memory.
pub fn classify_eightbytes(ty: &AbiType) -> Vec<EightbyteClass> {
    match ty {
        AbiType::Void => vec![],
        AbiType::Integer { bits, .. } => {
            if *bits > 64 {
                vec![EightbyteClass::Integer, EightbyteClass::Integer]
            } else {
                vec![EightbyteClass::Integer]
            }
        }
        AbiType::Pointer => vec![EightbyteClass::Integer],
        AbiType::Float { .. } => vec![EightbyteClass::Sse],
        AbiType::Vector { total_bytes, .. } => {
            if *total_bytes > 8 {
                vec![EightbyteClass::Sse, EightbyteClass::SseUp]
            } else {
                vec![EightbyteClass::Sse]
            }
        }
        AbiType::Struct { fields, size, .. } => {
            let total_size = *size;
            if total_size == 0 {
                return vec![];
            }
            if total_size > 16 {
                return vec![EightbyteClass::Memory];
            }
            let num_eightbytes = (total_size + 7) / 8;
            let mut eightbytes = vec![EightbyteClass::NoClass; num_eightbytes];
            walk_struct_fields(fields, 0, &mut eightbytes);

            // Post-merger rules:
            // 1. If any eightbyte is Memory, entire aggregate is Memory.
            if eightbytes.iter().any(|&c| c == EightbyteClass::Memory) {
                return vec![EightbyteClass::Memory; num_eightbytes];
            }
            // 2. If size exceeds two eightbytes and is not SSEUP, Memory.
            if num_eightbytes > 2 {
                return vec![EightbyteClass::Memory; num_eightbytes];
            }
            // 3. If SSEUP is not preceded by SSE or SSEUP, it is converted to SSE.
            if num_eightbytes == 2
                && eightbytes[1] == EightbyteClass::SseUp
                && eightbytes[0] != EightbyteClass::Sse
            {
                eightbytes[1] = EightbyteClass::Sse;
            }
            // 4. Any remaining NoClass eightbyte becomes Integer.
            for eb in &mut eightbytes {
                if *eb == EightbyteClass::NoClass {
                    *eb = EightbyteClass::Integer;
                }
            }

            eightbytes
        }
    }
}

/// Classifies a list of arguments into their System V AMD64 locations.
pub fn classify_sysv_arguments(args: &[AbiType]) -> Vec<ArgumentLocation> {
    let mut locations = Vec::new();
    let mut gpr_idx = 0;
    let mut fpr_idx = 0;
    let mut stack_offset = 16i32;

    for arg in args {
        let classes = classify_eightbytes(arg);
        if classes.is_empty() {
            // 0-sized argument
            continue;
        }

        let is_memory = classes.iter().any(|&c| c == EightbyteClass::Memory);
        if is_memory {
            let sz = arg.size_in_bytes().max(8);
            locations.push(ArgumentLocation::Stack(StackArgument {
                offset: stack_offset,
                size: arg.size_in_bytes(),
                align: arg.alignment().max(8),
            }));
            let padded = ((sz + 7) & !7) as i32;
            stack_offset += padded;
            continue;
        }

        if classes.len() == 1 {
            match classes[0] {
                EightbyteClass::Integer => {
                    if gpr_idx < SYSV_GPR_ARGS.len() {
                        locations.push(ArgumentLocation::Register(SYSV_GPR_ARGS[gpr_idx]));
                        gpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
                EightbyteClass::Sse => {
                    if fpr_idx < SYSV_FPR_ARGS.len() {
                        locations.push(ArgumentLocation::FloatRegister(SYSV_FPR_ARGS[fpr_idx]));
                        fpr_idx += 1;
                    } else {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset: stack_offset,
                            size: 8,
                            align: 8,
                        }));
                        stack_offset += 8;
                    }
                }
                _ => {
                    locations.push(ArgumentLocation::Stack(StackArgument {
                        offset: stack_offset,
                        size: 8,
                        align: 8,
                    }));
                    stack_offset += 8;
                }
            }
        } else if classes.len() == 2 {
            let gprs_needed = classes
                .iter()
                .filter(|&&c| c == EightbyteClass::Integer)
                .count();
            let fprs_needed = classes
                .iter()
                .filter(|&&c| c == EightbyteClass::Sse || c == EightbyteClass::SseUp)
                .count();

            if gpr_idx + gprs_needed <= SYSV_GPR_ARGS.len()
                && fpr_idx + fprs_needed <= SYSV_FPR_ARGS.len()
            {
                let reg0 = if classes[0] == EightbyteClass::Integer {
                    let r = SYSV_GPR_ARGS[gpr_idx];
                    gpr_idx += 1;
                    r
                } else {
                    let r = SYSV_FPR_ARGS[fpr_idx];
                    fpr_idx += 1;
                    r
                };

                let reg1 = if classes[1] == EightbyteClass::Integer {
                    let r = SYSV_GPR_ARGS[gpr_idx];
                    gpr_idx += 1;
                    r
                } else {
                    let r = SYSV_FPR_ARGS[fpr_idx];
                    fpr_idx += 1;
                    r
                };

                locations.push(ArgumentLocation::Pair(reg0, reg1));
            } else {
                // Not enough registers for all eightbytes: whole aggregate goes to memory!
                let sz = arg.size_in_bytes().max(16);
                locations.push(ArgumentLocation::Stack(StackArgument {
                    offset: stack_offset,
                    size: arg.size_in_bytes(),
                    align: arg.alignment().max(8),
                }));
                let padded = ((sz + 7) & !7) as i32;
                stack_offset += padded;
            }
        } else {
            // More than 2 eightbytes -> memory
            let sz = arg.size_in_bytes().max(8);
            locations.push(ArgumentLocation::Stack(StackArgument {
                offset: stack_offset,
                size: arg.size_in_bytes(),
                align: arg.alignment().max(8),
            }));
            let padded = ((sz + 7) & !7) as i32;
            stack_offset += padded;
        }
    }

    locations
}

/// Classifies a return type into its System V AMD64 location.
pub fn classify_sysv_return(ret: &AbiType) -> ReturnLocation {
    match ret {
        AbiType::Void => ReturnLocation::Void,
        AbiType::Integer { bits, .. } => {
            if *bits > 64 {
                ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2)) // RAX:RDX
            } else {
                ReturnLocation::Register(PhysicalRegister(0)) // RAX
            }
        }
        AbiType::Pointer => ReturnLocation::Register(PhysicalRegister(0)), // RAX
        AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)),
        AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister::xmm(0)),
        AbiType::Struct { .. } => {
            let classes = classify_eightbytes(ret);
            if classes.is_empty() {
                ReturnLocation::Void
            } else if classes.len() > 2 || classes.iter().any(|&c| c == EightbyteClass::Memory) {
                // Large struct or memory class -> hidden sret pointer in RDI
                ReturnLocation::HiddenSret(PhysicalRegister(7)) // RDI
            } else if classes.len() == 1 {
                match classes[0] {
                    EightbyteClass::Integer => ReturnLocation::Register(PhysicalRegister(0)), // RAX
                    EightbyteClass::Sse => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)), // XMM0
                    _ => ReturnLocation::HiddenSret(PhysicalRegister(7)),
                }
            } else if classes.len() == 2 {
                match (classes[0], classes[1]) {
                    (EightbyteClass::Integer, EightbyteClass::Integer) => {
                        ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister(2)) // RAX:RDX
                    }
                    (EightbyteClass::Sse, EightbyteClass::Sse)
                    | (EightbyteClass::Sse, EightbyteClass::SseUp) => {
                        ReturnLocation::Pair(PhysicalRegister::xmm(0), PhysicalRegister::xmm(1)) // XMM0:XMM1
                    }
                    (EightbyteClass::Integer, EightbyteClass::Sse) => {
                        ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister::xmm(0)) // RAX:XMM0
                    }
                    (EightbyteClass::Sse, EightbyteClass::Integer) => {
                        // First SSE in XMM0, first INTEGER in RAX
                        ReturnLocation::Pair(PhysicalRegister::xmm(0), PhysicalRegister(0)) // XMM0:RAX
                    }
                    _ => ReturnLocation::HiddenSret(PhysicalRegister(7)),
                }
            } else {
                ReturnLocation::HiddenSret(PhysicalRegister(7))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sysv_eightbyte_classification() {
        // 1. Primitive types
        assert_eq!(
            classify_eightbytes(&AbiType::i32()),
            vec![EightbyteClass::Integer]
        );
        assert_eq!(
            classify_eightbytes(&AbiType::f64()),
            vec![EightbyteClass::Sse]
        );
        assert_eq!(
            classify_eightbytes(&AbiType::v128_f32()),
            vec![EightbyteClass::Sse, EightbyteClass::SseUp]
        );

        // 2. Struct with all ints <= 8 bytes: { i32, i32 }
        let s_ii = AbiType::Struct {
            fields: vec![AbiType::i32(), AbiType::i32()],
            size: 8,
            align: 4,
        };
        assert_eq!(classify_eightbytes(&s_ii), vec![EightbyteClass::Integer]);

        // 3. Struct with all floats <= 8 bytes: { f32, f32 }
        let s_ff = AbiType::Struct {
            fields: vec![AbiType::f32(), AbiType::f32()],
            size: 8,
            align: 4,
        };
        assert_eq!(classify_eightbytes(&s_ff), vec![EightbyteClass::Sse]);

        // 4. Mixed struct 16 bytes: { i64, f64 } -> [Integer, Sse]
        let s_if = AbiType::Struct {
            fields: vec![AbiType::i64(), AbiType::f64()],
            size: 16,
            align: 8,
        };
        assert_eq!(
            classify_eightbytes(&s_if),
            vec![EightbyteClass::Integer, EightbyteClass::Sse]
        );

        // 5. Mixed struct 16 bytes: { f64, i64 } -> [Sse, Integer]
        let s_fi = AbiType::Struct {
            fields: vec![AbiType::f64(), AbiType::i64()],
            size: 16,
            align: 8,
        };
        assert_eq!(
            classify_eightbytes(&s_fi),
            vec![EightbyteClass::Sse, EightbyteClass::Integer]
        );

        // 6. Struct > 16 bytes: { i64, i64, i64 } -> Memory
        let s_large = AbiType::Struct {
            fields: vec![AbiType::i64(), AbiType::i64(), AbiType::i64()],
            size: 24,
            align: 8,
        };
        assert_eq!(classify_eightbytes(&s_large), vec![EightbyteClass::Memory]);
    }

    #[test]
    fn test_sysv_argument_allocation_and_register_exhaustion() {
        // Pass 5 GPRs, then a struct needing 2 GPRs -> struct must spill entirely to stack!
        let args = vec![
            AbiType::i64(),
            AbiType::i64(),
            AbiType::i64(),
            AbiType::i64(),
            AbiType::i64(),
            // 6th arg is a 16-byte struct { i64, i64 } -> needs 2 GPRs, but only 1 remains!
            AbiType::Struct {
                fields: vec![AbiType::i64(), AbiType::i64()],
                size: 16,
                align: 8,
            },
        ];
        let locs = classify_sysv_arguments(&args);
        assert_eq!(locs.len(), 6);
        assert_eq!(locs[0], ArgumentLocation::Register(PhysicalRegister(7))); // RDI
        assert_eq!(locs[1], ArgumentLocation::Register(PhysicalRegister(6))); // RSI
        assert_eq!(locs[2], ArgumentLocation::Register(PhysicalRegister(2))); // RDX
        assert_eq!(locs[3], ArgumentLocation::Register(PhysicalRegister(1))); // RCX
        assert_eq!(locs[4], ArgumentLocation::Register(PhysicalRegister(8))); // R8
        // 6th arg must NOT be split: must go to stack!
        assert!(matches!(locs[5], ArgumentLocation::Stack(s) if s.offset == 16 && s.size == 16));
    }

    #[test]
    fn test_sysv_return_classification() {
        // Small int struct -> RAX
        let s_ii = AbiType::Struct {
            fields: vec![AbiType::i32(), AbiType::i32()],
            size: 8,
            align: 4,
        };
        assert_eq!(
            classify_sysv_return(&s_ii),
            ReturnLocation::Register(PhysicalRegister(0))
        );

        // Small float struct -> XMM0
        let s_ff = AbiType::Struct {
            fields: vec![AbiType::f32(), AbiType::f32()],
            size: 8,
            align: 4,
        };
        assert_eq!(
            classify_sysv_return(&s_ff),
            ReturnLocation::FloatRegister(PhysicalRegister::xmm(0))
        );

        // 16-byte mixed struct { i64, f64 } -> Pair(RAX, XMM0)
        let s_if = AbiType::Struct {
            fields: vec![AbiType::i64(), AbiType::f64()],
            size: 16,
            align: 8,
        };
        assert_eq!(
            classify_sysv_return(&s_if),
            ReturnLocation::Pair(PhysicalRegister(0), PhysicalRegister::xmm(0))
        );

        // 24-byte large struct -> HiddenSret(RDI)
        let s_large = AbiType::Struct {
            fields: vec![AbiType::i64(), AbiType::i64(), AbiType::i64()],
            size: 24,
            align: 8,
        };
        assert_eq!(
            classify_sysv_return(&s_large),
            ReturnLocation::HiddenSret(PhysicalRegister(7))
        );
    }
}
