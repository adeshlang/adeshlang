//! Windows x64 (MSVC x64) ABI argument and return classification.
//!
//! Implements Microsoft x64 calling convention:
//! - 4 argument register slots: RCX/XMM0, RDX/XMM1, R8/XMM2, R9/XMM3.
//! - 32-byte shadow space provided by the caller.
//! - Structs of size 1, 2, 4, or 8 bytes passed by value in GPR slots (even if containing floats).
//! - Structs with size not in {1, 2, 4, 8} passed by reference (hidden pointer in GPR or stack slot).
//! - Returns: small structs (1, 2, 4, 8 bytes) returned in RAX; all others returned via hidden
//!   sret pointer in RCX.

use crate::abi::{AbiType, ArgumentLocation, ReturnLocation, StackArgument};
use crate::machine_ir::PhysicalRegister;

pub const WIN64_GPR_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister(1), // RCX
    PhysicalRegister(2), // RDX
    PhysicalRegister(8), // R8
    PhysicalRegister(9), // R9
];

pub const WIN64_FPR_ARGS: [PhysicalRegister; 4] = [
    PhysicalRegister::xmm(0),
    PhysicalRegister::xmm(1),
    PhysicalRegister::xmm(2),
    PhysicalRegister::xmm(3),
];

/// Classifies a list of arguments into their Windows x64 locations.
pub fn classify_win64_arguments(args: &[AbiType]) -> Vec<ArgumentLocation> {
    let mut locations = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i < 4 {
            match arg {
                AbiType::Float { .. } => {
                    locations.push(ArgumentLocation::FloatRegister(WIN64_FPR_ARGS[i]));
                }
                AbiType::Vector { .. } => {
                    locations.push(ArgumentLocation::VectorRegister(WIN64_FPR_ARGS[i]));
                }
                AbiType::Struct { size, .. } => {
                    if [1, 2, 4, 8].contains(size) {
                        locations.push(ArgumentLocation::Register(WIN64_GPR_ARGS[i]));
                    } else {
                        locations.push(ArgumentLocation::IndirectByReference(WIN64_GPR_ARGS[i]));
                    }
                }
                _ => {
                    locations.push(ArgumentLocation::Register(WIN64_GPR_ARGS[i]));
                }
            }
        } else {
            // Stack arguments starting after 32 bytes shadow space + 16 bytes return addr/frame
            let offset = 48 + ((i - 4) as i32 * 8);
            match arg {
                AbiType::Struct { size, .. } => {
                    if [1, 2, 4, 8].contains(size) {
                        locations.push(ArgumentLocation::Stack(StackArgument {
                            offset,
                            size: 8,
                            align: 8,
                        }));
                    } else {
                        // Structs > 8 bytes or odd sizes passed by reference even on stack
                        locations.push(ArgumentLocation::IndirectStack(StackArgument {
                            offset,
                            size: 8,
                            align: 8,
                        }));
                    }
                }
                _ => {
                    locations.push(ArgumentLocation::Stack(StackArgument {
                        offset,
                        size: arg.size_in_bytes().max(8),
                        align: arg.alignment().max(8),
                    }));
                }
            }
        }
    }
    locations
}

/// Classifies a return type into its Windows x64 location.
pub fn classify_win64_return(ret: &AbiType) -> ReturnLocation {
    match ret {
        AbiType::Void => ReturnLocation::Void,
        AbiType::Float { .. } => ReturnLocation::FloatRegister(PhysicalRegister::xmm(0)),
        AbiType::Vector { .. } => ReturnLocation::VectorRegister(PhysicalRegister::xmm(0)),
        AbiType::Struct { size, .. } => {
            if [1, 2, 4, 8].contains(size) {
                ReturnLocation::Register(PhysicalRegister(0)) // RAX
            } else {
                ReturnLocation::HiddenSret(PhysicalRegister(1)) // RCX
            }
        }
        _ => ReturnLocation::Register(PhysicalRegister(0)), // RAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_win64_small_and_large_struct_arguments() {
        let args = vec![
            // 1. Small struct of size 4 passed by value in RCX (GPR!)
            AbiType::Struct {
                fields: vec![AbiType::i32()],
                size: 4,
                align: 4,
            },
            // 2. Struct with float of size 8 passed by value in RDX (GPR!)
            AbiType::Struct {
                fields: vec![AbiType::f64()],
                size: 8,
                align: 8,
            },
            // 3. Struct of size 16 (>8 bytes) passed by reference in R8!
            AbiType::Struct {
                fields: vec![AbiType::i64(), AbiType::i64()],
                size: 16,
                align: 8,
            },
            // 4. Primitive f64 in XMM3
            AbiType::f64(),
            // 5. 5th arg small struct on stack by value
            AbiType::Struct {
                fields: vec![AbiType::i32()],
                size: 4,
                align: 4,
            },
            // 6. 6th arg large struct on stack by reference (IndirectStack)
            AbiType::Struct {
                fields: vec![AbiType::i64(), AbiType::i64()],
                size: 16,
                align: 8,
            },
        ];

        let locs = classify_win64_arguments(&args);
        assert_eq!(locs.len(), 6);
        assert_eq!(locs[0], ArgumentLocation::Register(PhysicalRegister(1))); // RCX
        assert_eq!(locs[1], ArgumentLocation::Register(PhysicalRegister(2))); // RDX
        assert_eq!(
            locs[2],
            ArgumentLocation::IndirectByReference(PhysicalRegister(8))
        ); // R8
        assert_eq!(
            locs[3],
            ArgumentLocation::FloatRegister(PhysicalRegister::xmm(3))
        ); // XMM3
        assert!(matches!(locs[4], ArgumentLocation::Stack(s) if s.offset == 48));
        assert!(matches!(locs[5], ArgumentLocation::IndirectStack(s) if s.offset == 56));
    }

    #[test]
    fn test_win64_return_classification() {
        // Struct of size 4 -> RAX
        let s4 = AbiType::Struct {
            fields: vec![AbiType::i32()],
            size: 4,
            align: 4,
        };
        assert_eq!(
            classify_win64_return(&s4),
            ReturnLocation::Register(PhysicalRegister(0))
        );

        // Struct of size 8 -> RAX
        let s8 = AbiType::Struct {
            fields: vec![AbiType::f64()],
            size: 8,
            align: 8,
        };
        assert_eq!(
            classify_win64_return(&s8),
            ReturnLocation::Register(PhysicalRegister(0))
        );

        // Struct of size 16 -> HiddenSret(RCX)
        let s16 = AbiType::Struct {
            fields: vec![AbiType::i64(), AbiType::i64()],
            size: 16,
            align: 8,
        };
        assert_eq!(
            classify_win64_return(&s16),
            ReturnLocation::HiddenSret(PhysicalRegister(1))
        );
    }
}
