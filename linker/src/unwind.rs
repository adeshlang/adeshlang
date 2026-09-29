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
    pub fn build(
        hdr_va: u64,
        eh_frame_va: u64,
        mut fde_entries: Vec<(u64, u64)>,
    ) -> Vec<u8> {
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

impl WindowsPdataGenerator {
    /// Build `.pdata` 12-byte RUNTIME_FUNCTION table.
    pub fn build_pdata(mut entries: Vec<PdataEntry>) -> Vec<u8> {
        entries.sort_by_key(|e| e.begin_rva);
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
        // UNWIND_INFO:
        // Byte 0: Version (1) | Flags (0) -> 0x01
        // Byte 1: Size of prolog (0) -> 0x00
        // Byte 2: Count of unwind codes (0) -> 0x00
        // Byte 3: Frame register & offset (0) -> 0x00
        vec![0x01, 0x00, 0x00, 0x00]
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
