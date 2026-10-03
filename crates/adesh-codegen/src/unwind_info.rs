//! Unwind Information Modeling for Exception Handling and Stack Walking.
//!
//! Generates `.pdata`/`.xdata` descriptors for Windows x64 and `.eh_frame` CIE/FDE structures
//! for SysV ELF platforms, ensuring unwind tables strictly mirror prologue/epilogue/callee-saves.

use serde::{Deserialize, Serialize};

/// Unwind Opcode representation for x86-64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnwindOpcode {
    PushNonVolatileReg {
        reg: u8,
        code_offset: u8,
    },
    AllocLargeStack {
        size: u32,
        code_offset: u8,
    },
    AllocSmallStack {
        size: u8,
        code_offset: u8,
    },
    SetFramePointer {
        reg: u8,
        offset: u8,
        code_offset: u8,
    },
}

/// Windows x64 Function Unwind Info (`.pdata` / `.xdata`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Win64UnwindInfo {
    pub function_name: String,
    pub start_rva: u32,
    pub end_rva: u32,
    pub unwind_info_rva: u32,
    pub prologue_size: u8,
    pub frame_reg: u8, // RBP = 5
    pub frame_reg_offset: u8,
    pub opcodes: Vec<UnwindOpcode>,
}

/// SysV x86-64 DWARF Call Frame Information (`.eh_frame`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DwarfCallFrameInfo {
    pub function_name: String,
    pub start_pc: u64,
    pub length: u64,
    pub initial_cfa_offset: i32,
    pub callee_saved_offsets: Vec<(u8, i32)>, // (reg, offset)
}

/// Unified Function Unwind Descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionUnwindDescriptor {
    pub function_name: String,
    pub win64_unwind: Option<Win64UnwindInfo>,
    pub dwarf_unwind: Option<DwarfCallFrameInfo>,
}

impl FunctionUnwindDescriptor {
    pub fn for_win64(
        function_name: impl Into<String>,
        prologue_size: u8,
        stack_size: u32,
        saved_regs: &[(u8, u8)], // (reg, code_offset)
    ) -> Self {
        let name = function_name.into();
        let mut opcodes = Vec::new();

        for &(reg, code_offset) in saved_regs {
            opcodes.push(UnwindOpcode::PushNonVolatileReg { reg, code_offset });
        }

        if stack_size > 0 {
            if stack_size <= 128 {
                opcodes.push(UnwindOpcode::AllocSmallStack {
                    size: (stack_size / 8 - 1) as u8,
                    code_offset: prologue_size,
                });
            } else {
                opcodes.push(UnwindOpcode::AllocLargeStack {
                    size: stack_size,
                    code_offset: prologue_size,
                });
            }
        }

        let win64 = Win64UnwindInfo {
            function_name: name.clone(),
            start_rva: 0,
            end_rva: 0,
            unwind_info_rva: 0,
            prologue_size,
            frame_reg: 5,
            frame_reg_offset: 0,
            opcodes,
        };

        Self {
            function_name: name,
            win64_unwind: Some(win64),
            dwarf_unwind: None,
        }
    }
}
