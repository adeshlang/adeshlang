//! Thread-Local Storage (TLS) model representations and synthesizers.

use crate::section::{Section, SectionKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};

/// TLS linkage and access models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsModel {
    /// Local Exec (static binary, fixed thread offset)
    LocalExec,
    /// Initial Exec (dynamic executable or DLL loaded at process start)
    InitialExec,
    /// General Dynamic (runtime dlopen TLS resolution via __tls_get_addr)
    GeneralDynamic,
    /// Local Dynamic (module-relative TLS resolution)
    LocalDynamic,
}

/// TLS segment bounds and alignment.
#[derive(Debug, Clone, Default)]
pub struct TlsLayout {
    pub image_size: u64,
    pub file_size: u64,
    pub alignment: u64,
    pub offset: u64,
}

/// Thread-Local Storage Synthesizer for PE/COFF and ELF targets.
pub struct TlsSynthesizer;

impl TlsSynthesizer {
    /// Synthesizes Windows PE `IMAGE_TLS_DIRECTORY64` in `.rdata` and initial `.tls` section.
    pub fn synthesize_pe_tls_directory(
        image_base: u64,
        tls_data_rva: u64,
        tls_data_size: u64,
        tls_zero_fill: u64,
        tls_index_rva: u64,
        callbacks_rva: u64,
        alignment: u32,
    ) -> (Section, Vec<Symbol>) {
        let mut dir_bytes = Vec::with_capacity(40);

        let start_va = image_base + tls_data_rva;
        let end_va = start_va + tls_data_size + tls_zero_fill;
        let index_va = image_base + tls_index_rva;
        let callbacks_va = if callbacks_rva > 0 {
            image_base + callbacks_rva
        } else {
            0
        };

        // IMAGE_TLS_DIRECTORY64 layout:
        // StartAddressOfRawData (8B)
        dir_bytes.extend_from_slice(&start_va.to_le_bytes());
        // EndAddressOfRawData (8B)
        dir_bytes.extend_from_slice(&end_va.to_le_bytes());
        // AddressOfIndex (8B)
        dir_bytes.extend_from_slice(&index_va.to_le_bytes());
        // AddressOfCallBacks (8B)
        dir_bytes.extend_from_slice(&callbacks_va.to_le_bytes());
        // SizeOfZeroFill (4B)
        dir_bytes.extend_from_slice(&(tls_zero_fill as u32).to_le_bytes());
        // Characteristics / Alignment (4B)
        let characteristics = (alignment.trailing_zeros() + 1) << 20;
        dir_bytes.extend_from_slice(&characteristics.to_le_bytes());

        let sec = Section::new_data(".tls_dir", dir_bytes, false, 16);

        let sym = Symbol::new_defined(
            "_tls_used",
            SymbolBinding::Global,
            SymbolType::Object,
            0,
            0,
            40,
            0,
        );

        (sec, vec![sym])
    }

    /// Synthesizes ELF `PT_TLS` segment layout metadata.
    pub fn synthesize_elf_tls_layout(tls_sections: &[&Section], default_align: u64) -> TlsLayout {
        let mut file_sz = 0u64;
        let mut total_sz = 0u64;
        let mut max_align = default_align.max(8);

        for sec in tls_sections {
            max_align = max_align.max(sec.alignment);
            total_sz = (total_sz + sec.alignment - 1) & !(sec.alignment - 1);
            if sec.kind != SectionKind::Bss {
                file_sz = (file_sz + sec.alignment - 1) & !(sec.alignment - 1);
                file_sz += sec.data.len() as u64;
            }
            total_sz += sec.size;
        }

        TlsLayout {
            image_size: total_sz,
            file_size: file_sz,
            alignment: max_align,
            offset: 0,
        }
    }
}
