//! Exception handling frame information (.eh_frame, .pdata/.xdata) and RAII drop management.
//!
//! Provides zero-cost exception and panic unwinding table synthesis for Linux/macOS (.eh_frame, .eh_frame_hdr)
//! and Windows x64 SEH (.pdata, .xdata), as well as GC-free RAII drop tables for the Adesh ownership model.

/// Unwind metadata representation.
#[derive(Debug, Clone, Default)]
pub struct UnwindInfo {
    pub eh_frame_hdr: Vec<u8>,
    pub eh_frame: Vec<u8>,
    pub pdata: Vec<u8>,
    pub xdata: Vec<u8>,
}

/// Generator for GNU `.eh_frame_hdr` binary search index tables.
pub struct EhFrameHdrGenerator;

impl EhFrameHdrGenerator {
    /// DW_EH_PE pointer encodings
    pub const DW_EH_PE_PCREL: u8 = 0x10;
    pub const DW_EH_PE_SDATA4: u8 = 0x0b;
    pub const DW_EH_PE_UDATA4: u8 = 0x03;
    pub const DW_EH_PE_DATAREL: u8 = 0x30;

    /// Build `.eh_frame_hdr` binary search index table.
    ///
    /// - `hdr_va`: Virtual address of the `.eh_frame_hdr` section
    /// - `eh_frame_va`: Virtual address of the `.eh_frame` section
    /// - `fde_entries`: Slice of `(func_initial_va, fde_va)` sorted by `func_initial_va`
    pub fn build(hdr_va: u64, eh_frame_va: u64, mut fde_entries: Vec<(u64, u64)>) -> Vec<u8> {
        let mut out = Vec::new();

        // 1. Version (1)
        out.push(1);
        // 2. eh_frame_ptr_enc (DW_EH_PE_pcrel | DW_EH_PE_sdata4 = 0x1b)
        out.push(Self::DW_EH_PE_PCREL | Self::DW_EH_PE_SDATA4);
        // 3. fde_count_enc (DW_EH_PE_udata4 = 0x03)
        out.push(Self::DW_EH_PE_UDATA4);
        // 4. table_enc (DW_EH_PE_datarel | DW_EH_PE_sdata4 = 0x3b)
        out.push(Self::DW_EH_PE_DATAREL | Self::DW_EH_PE_SDATA4);

        // 5. eh_frame_ptr (sdata4 relative to current PC)
        let pc_offset = out.len() as u64 + 4;
        let eh_rel = (eh_frame_va as i64) - ((hdr_va + pc_offset) as i64);
        out.extend_from_slice(&(eh_rel as i32).to_le_bytes());

        // 6. fde_count (udata4)
        let fde_count = fde_entries.len() as u32;
        out.extend_from_slice(&fde_count.to_le_bytes());

        // 7. Binary search table: sorted by initial_loc
        fde_entries.sort_by_key(|&(initial_va, _)| initial_va);

        for (initial_va, fde_va) in fde_entries {
            // initial_loc relative to hdr_va
            let loc_rel = (initial_va as i64) - (hdr_va as i64);
            // fde_ptr relative to hdr_va
            let fde_rel = (fde_va as i64) - (hdr_va as i64);

            out.extend_from_slice(&(loc_rel as i32).to_le_bytes());
            out.extend_from_slice(&(fde_rel as i32).to_le_bytes());
        }

        out
    }
}

/// Windows x64 Structured Exception Handling (`.pdata` & `.xdata`) Generator.
pub struct WindowsPdataGenerator;

#[derive(Debug, Clone)]
pub struct PdataEntry {
    pub begin_rva: u32,
    pub end_rva: u32,
    pub unwind_info_rva: u32,
}

/// Win64 Unwind operation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnwindOp {
    /// Push nonvolatile integer register (op_info = reg).
    PushNonvol(u8),
    /// Allocate large stack area (unscaled byte size).
    AllocLarge(u32),
    /// Allocate small stack area (8..=128 bytes).
    AllocSmall(u8),
    /// Set frame pointer register (RBP).
    SetFpReg,
    /// Save nonvolatile register to stack (reg, byte offset).
    SaveNonvol(u8, u32),
}

/// Win64 function unwind descriptor.
#[derive(Debug, Clone, Default)]
pub struct Win64FunctionUnwind {
    pub prolog_size: u8,
    pub frame_reg: u8,        // 0 if none, 5 for RBP
    pub frame_reg_offset: u8, // scaled by 16
    pub codes: Vec<(u8, UnwindOp)>, // (prolog_offset, op)
}

impl WindowsPdataGenerator {
    /// Win64 register numbers for unwind codes.
    pub const REG_RAX: u8 = 0;
    pub const REG_RCX: u8 = 1;
    pub const REG_RDX: u8 = 2;
    pub const REG_RBX: u8 = 3;
    pub const REG_RSP: u8 = 4;
    pub const REG_RBP: u8 = 5;
    pub const REG_RSI: u8 = 6;
    pub const REG_RDI: u8 = 7;
    pub const REG_R8: u8 = 8;
    pub const REG_R9: u8 = 9;
    pub const REG_R10: u8 = 10;
    pub const REG_R11: u8 = 11;
    pub const REG_R12: u8 = 12;
    pub const REG_R13: u8 = 13;
    pub const REG_R14: u8 = 14;
    pub const REG_R15: u8 = 15;

    // Unwind op codes
    pub const UWOP_PUSH_NONVOL: u8 = 0;
    pub const UWOP_ALLOC_LARGE: u8 = 1;
    pub const UWOP_ALLOC_SMALL: u8 = 2;
    pub const UWOP_SET_FPREG: u8 = 3;
    pub const UWOP_SAVE_NONVOL: u8 = 4;

    /// Build `.pdata` 12-byte RUNTIME_FUNCTION table.
    pub fn build_pdata(mut entries: Vec<PdataEntry>) -> Vec<u8> {
        entries.sort_by_key(|e| e.begin_rva);
        entries.dedup_by_key(|e| e.begin_rva);
        let mut out = Vec::with_capacity(entries.len() * 12);
        for entry in entries {
            out.extend_from_slice(&entry.begin_rva.to_le_bytes());
            out.extend_from_slice(&entry.end_rva.to_le_bytes());
            out.extend_from_slice(&entry.unwind_info_rva.to_le_bytes());
        }
        out
    }

    /// Build minimal `.xdata` UNWIND_INFO block for a standard leaf or frame function.
    pub fn build_default_xdata() -> Vec<u8> {
        // UNWIND_INFO: Version=1, Flags=0, PrologSize=0, CodeCount=0, FrameReg=0
        vec![0x01, 0x00, 0x00, 0x00]
    }

    /// Build a standard frame function `.xdata` UNWIND_INFO block.
    /// Standard frame:
    ///   push rbp          (prolog off 1)
    ///   mov rbp, rsp      (prolog off 4)
    ///   sub rsp, alloc_sz (prolog off 8 or 11)
    pub fn build_standard_frame_xdata(frame_size: u32, callee_saved: &[u8]) -> Vec<u8> {
        let mut unwind = Win64FunctionUnwind {
            prolog_size: 4,
            frame_reg: Self::REG_RBP,
            frame_reg_offset: 0,
            codes: Vec::new(),
        };

        // 1. Stack allocation
        if frame_size > 0 {
            if frame_size <= 128 && frame_size % 8 == 0 {
                unwind.prolog_size = 8;
                unwind.codes.push((8, UnwindOp::AllocSmall(frame_size as u8)));
            } else {
                unwind.prolog_size = 11;
                unwind.codes.push((11, UnwindOp::AllocLarge(frame_size)));
            }
        }

        // 2. Set FP register
        unwind.codes.push((4, UnwindOp::SetFpReg));

        // 3. Push RBP
        unwind.codes.push((1, UnwindOp::PushNonvol(Self::REG_RBP)));

        // 4. Callee-saved register pushes
        for &reg in callee_saved {
            unwind.codes.push((1, UnwindOp::PushNonvol(reg)));
        }

        Self::encode_unwind_info(&unwind)
    }

    /// Encode `Win64FunctionUnwind` into raw `UNWIND_INFO` bytes.
    pub fn encode_unwind_info(unwind: &Win64FunctionUnwind) -> Vec<u8> {
        let mut slots: Vec<[u8; 2]> = Vec::new();

        // Encode unwind codes in reverse order
        for &(prolog_off, op) in &unwind.codes {
            match op {
                UnwindOp::PushNonvol(reg) => {
                    let b0 = prolog_off;
                    let b1 = (reg << 4) | (Self::UWOP_PUSH_NONVOL & 0x0F);
                    slots.push([b0, b1]);
                }
                UnwindOp::AllocSmall(sz) => {
                    let op_info = ((sz / 8) - 1) & 0x0F;
                    let b0 = prolog_off;
                    let b1 = (op_info << 4) | (Self::UWOP_ALLOC_SMALL & 0x0F);
                    slots.push([b0, b1]);
                }
                UnwindOp::AllocLarge(sz) => {
                    if sz <= 512 * 1024 - 8 {
                        let scaled = (sz / 8) as u16;
                        let b0 = prolog_off;
                        let b1 = (0 << 4) | (Self::UWOP_ALLOC_LARGE & 0x0F);
                        slots.push([b0, b1]);
                        let val_bytes = scaled.to_le_bytes();
                        slots.push([val_bytes[0], val_bytes[1]]);
                    } else {
                        let b0 = prolog_off;
                        let b1 = (1 << 4) | (Self::UWOP_ALLOC_LARGE & 0x0F);
                        slots.push([b0, b1]);
                        let val_bytes = sz.to_le_bytes();
                        slots.push([val_bytes[0], val_bytes[1]]);
                        slots.push([val_bytes[2], val_bytes[3]]);
                    }
                }
                UnwindOp::SetFpReg => {
                    let b0 = prolog_off;
                    let b1 = (0 << 4) | (Self::UWOP_SET_FPREG & 0x0F);
                    slots.push([b0, b1]);
                }
                UnwindOp::SaveNonvol(reg, offset) => {
                    let scaled = (offset / 8) as u16;
                    let b0 = prolog_off;
                    let b1 = (reg << 4) | (Self::UWOP_SAVE_NONVOL & 0x0F);
                    slots.push([b0, b1]);
                    let val_bytes = scaled.to_le_bytes();
                    slots.push([val_bytes[0], val_bytes[1]]);
                }
            }
        }

        let code_count = slots.len() as u8;
        let mut out = Vec::new();
        // Byte 0: Version (1) | Flags (0) -> 0x01
        out.push(1);
        // Byte 1: Size of prolog
        out.push(unwind.prolog_size);
        // Byte 2: Count of codes
        out.push(code_count);
        // Byte 3: Frame register (4 bits) and frame register offset (4 bits)
        let frame_byte = ((unwind.frame_reg_offset & 0x0F) << 4) | (unwind.frame_reg & 0x0F);
        out.push(frame_byte);

        // Slots
        for slot in &slots {
            out.push(slot[0]);
            out.push(slot[1]);
        }

        // Align to 4 bytes (even number of slots)
        if slots.len() % 2 != 0 {
            out.push(0);
            out.push(0);
        }

        out
    }
}

/// Ownership and RAII Drop Table generator for zero-cost destructors.
pub struct RaiiDropTable;

#[derive(Debug, Clone)]
pub struct DropTableEntry {
    pub type_id: u64,
    pub drop_fn_va: u64,
    pub type_size: u64,
    pub type_align: u64,
}

impl RaiiDropTable {
    /// Build `.adesh.drop_table` binary section bytes.
    pub fn build_drop_table(entries: &[DropTableEntry]) -> Vec<u8> {
        let mut out = Vec::new();
        // Magic header: b"ADROP\x01\x00\x00" (8 bytes)
        out.extend_from_slice(b"ADROP\x01\x00\x00");
        // Count of entries (u64)
        out.extend_from_slice(&(entries.len() as u64).to_le_bytes());

        for entry in entries {
            out.extend_from_slice(&entry.type_id.to_le_bytes());
            out.extend_from_slice(&entry.drop_fn_va.to_le_bytes());
            out.extend_from_slice(&entry.type_size.to_le_bytes());
            out.extend_from_slice(&entry.type_align.to_le_bytes());
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_pdata_encoding() {
        let entries = vec![
            PdataEntry {
                begin_rva: 0x1000,
                end_rva: 0x1050,
                unwind_info_rva: 0x2000,
            },
            PdataEntry {
                begin_rva: 0x1050,
                end_rva: 0x1100,
                unwind_info_rva: 0x2020,
            },
        ];

        let pdata_bytes = WindowsPdataGenerator::build_pdata(entries);
        assert_eq!(pdata_bytes.len(), 24);

        let begin_0 = u32::from_le_bytes(pdata_bytes[0..4].try_into().unwrap());
        let end_0 = u32::from_le_bytes(pdata_bytes[4..8].try_into().unwrap());
        let unwind_0 = u32::from_le_bytes(pdata_bytes[8..12].try_into().unwrap());
        assert_eq!(begin_0, 0x1000);
        assert_eq!(end_0, 0x1050);
        assert_eq!(unwind_0, 0x2000);
    }

    #[test]
    fn test_windows_xdata_standard_frame() {
        let callee_saved = vec![WindowsPdataGenerator::REG_RBX, WindowsPdataGenerator::REG_R12];
        let xdata = WindowsPdataGenerator::build_standard_frame_xdata(64, &callee_saved);

        assert!(xdata.len() >= 4);
        assert_eq!(xdata[0], 0x01); // Version=1, Flags=0
        assert_eq!(xdata[3] & 0x0F, WindowsPdataGenerator::REG_RBP); // Frame reg is RBP (5)
        assert_eq!(xdata.len() % 4, 0); // 4-byte aligned
    }
}

