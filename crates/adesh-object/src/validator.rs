use crate::error::{AdobError, AdobErrorCode, AdobResult};
use crate::format::{ADOB_MAGIC, ADOB_VERSION_MAJOR, AdobObject};
use crate::symbol::SymbolBinding;
use std::collections::HashSet;

pub struct AdobValidator;

impl AdobValidator {
    /// Perform full validation on an AdobObject.
    pub fn validate(obj: &AdobObject) -> AdobResult<()> {
        Self::validate_header(obj)?;
        Self::validate_sections(obj)?;
        Self::validate_symbols(obj)?;
        Self::validate_relocations(obj)?;
        Self::validate_imports_exports(obj)?;
        Self::validate_metadata(obj)?;
        Ok(())
    }

    /// Validate header magic and version.
    pub fn validate_header(obj: &AdobObject) -> AdobResult<()> {
        if obj.header.magic != *ADOB_MAGIC {
            return Err(AdobError::new(
                AdobErrorCode::InvalidMagic,
                format!("Invalid ADOB magic: {:?}", obj.header.magic),
            ));
        }

        if obj.header.version_major != ADOB_VERSION_MAJOR {
            return Err(AdobError::new(
                AdobErrorCode::UnsupportedVersion,
                format!(
                    "Unsupported ADOB version {}.{}. Expected {}.x",
                    obj.header.version_major, obj.header.version_minor, ADOB_VERSION_MAJOR
                ),
            ));
        }

        Ok(())
    }

    /// Validate sections: non-empty names, alignments (powers of 2), non-overflowing sizes.
    pub fn validate_sections(obj: &AdobObject) -> AdobResult<()> {
        let mut section_names = HashSet::new();

        for (idx, sec) in obj.sections.iter().enumerate() {
            if sec.name.is_empty() {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidSection,
                    format!("Section at index {} has an empty name", idx),
                ));
            }

            // Alignment must be non-zero power of 2
            if sec.alignment == 0 || (sec.alignment & (sec.alignment - 1)) != 0 {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidAlignment,
                    format!(
                        "Section `{}` has invalid non-power-of-two alignment: {}",
                        sec.name, sec.alignment
                    ),
                )
                .with_section(&sec.name));
            }

            // Check if section name is duplicate unless COMDAT group
            if sec.comdat_group.is_none() && !section_names.insert(&sec.name) {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidSection,
                    format!("Duplicate non-COMDAT section name: `{}`", sec.name),
                )
                .with_section(&sec.name));
            }
        }

        Ok(())
    }

    /// Validate symbol table: unique global symbol names, valid section indices, bounds.
    pub fn validate_symbols(obj: &AdobObject) -> AdobResult<()> {
        let mut global_symbols = HashSet::new();
        let num_sections = obj.sections.len();

        for sym in &obj.symbols {
            if sym.name.is_empty() {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidSymbol,
                    format!("Symbol id {} has an empty name", sym.id),
                ));
            }

            // Duplicate global symbol check (unless weak)
            if sym.binding == SymbolBinding::Global
                && sym.is_defined
                && !global_symbols.insert(&sym.name)
            {
                return Err(AdobError::new(
                    AdobErrorCode::DuplicateSymbol,
                    format!("Duplicate defined global symbol: `{}`", sym.name),
                )
                .with_symbol(&sym.name));
            }

            // If symbol is defined, verify section index
            if sym.is_defined {
                match sym.section_index {
                    Some(sec_idx) if (sec_idx as usize) < num_sections => {
                        let sec = &obj.sections[sec_idx as usize];
                        // Verify symbol value does not exceed section data size (for non-BSS sections)
                        if sec.kind != crate::section::SectionKind::Bss
                            && !sec.data.is_empty()
                            && sym.value > sec.data.len() as u64
                        {
                            return Err(AdobError::new(
                                AdobErrorCode::InvalidSymbol,
                                format!(
                                    "Symbol `{}` value 0x{:X} exceeds section `{}` size (0x{:X})",
                                    sym.name,
                                    sym.value,
                                    sec.name,
                                    sec.data.len()
                                ),
                            )
                            .with_symbol(&sym.name)
                            .with_section(&sec.name));
                        }
                    }
                    Some(invalid_idx) => {
                        return Err(AdobError::new(
                            AdobErrorCode::InvalidSectionOffset,
                            format!(
                                "Symbol `{}` references non-existent section index {}",
                                sym.name, invalid_idx
                            ),
                        )
                        .with_symbol(&sym.name));
                    }
                    None => {
                        return Err(AdobError::new(
                            AdobErrorCode::InvalidSymbol,
                            format!(
                                "Defined symbol `{}` has no associated section index",
                                sym.name
                            ),
                        )
                        .with_symbol(&sym.name));
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate relocations: in-bounds offset within section, valid target symbol.
    pub fn validate_relocations(obj: &AdobObject) -> AdobResult<()> {
        let sym_count = obj.symbols.len() as u32;

        for sec in &obj.sections {
            let sec_size = sec.data.len() as u64;

            for (r_idx, reloc) in sec.relocations.iter().enumerate() {
                // Check relocation offset bounds
                if reloc
                    .offset
                    .checked_add(reloc.width as u64)
                    .is_none_or(|end| end > sec_size)
                {
                    return Err(AdobError::new(
                        AdobErrorCode::RelocationOutOfRange,
                        format!(
                            "Relocation #{} at offset 0x{:X} (width {}) exceeds section `{}` size (0x{:X})",
                            r_idx, reloc.offset, reloc.width, sec.name, sec_size
                        ),
                    ).with_section(&sec.name));
                }

                // Check symbol reference
                if reloc.symbol >= sym_count
                    && !obj.symbols.iter().any(|s| s.name == reloc.symbol_name)
                {
                    return Err(AdobError::new(
                        AdobErrorCode::RelocationTargetNotFound,
                        format!(
                            "Relocation at offset 0x{:X} references non-existent symbol `{}` (id: {}) in section `{}`",
                            reloc.offset, reloc.symbol_name, reloc.symbol, sec.name
                        ),
                    )
                    .with_section(&sec.name)
                    .with_symbol(&reloc.symbol_name));
                }
            }
        }

        Ok(())
    }

    /// Validate imports and exports.
    pub fn validate_imports_exports(obj: &AdobObject) -> AdobResult<()> {
        for imp in &obj.imports {
            if imp.is_empty() {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidMetadata,
                    "Empty import symbol name found",
                ));
            }
        }

        for exp in &obj.exports {
            if exp.is_empty() {
                return Err(AdobError::new(
                    AdobErrorCode::InvalidMetadata,
                    "Empty export symbol name found",
                ));
            }
        }

        Ok(())
    }

    /// Validate metadata consistency.
    pub fn validate_metadata(obj: &AdobObject) -> AdobResult<()> {
        // Validate memory regions for collisions
        let mut regions = obj.memory_regions.clone();
        regions.sort_by_key(|r| r.address);

        for i in 0..regions.len() {
            if i + 1 < regions.len() {
                let current_end = regions[i].address.saturating_add(regions[i].size);
                if current_end > regions[i + 1].address {
                    return Err(AdobError::new(
                        AdobErrorCode::SectionOverlap,
                        format!(
                            "Memory region `{}` (0x{:X}..0x{:X}) overlaps with region `{}` (0x{:X}..0x{:X})",
                            regions[i].name,
                            regions[i].address,
                            current_end,
                            regions[i + 1].name,
                            regions[i + 1].address,
                            regions[i + 1].address.saturating_add(regions[i + 1].size)
                        ),
                    ));
                }
            }
        }

        Ok(())
    }
}
