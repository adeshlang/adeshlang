//! Top-level ADOB (Adesh Native Object Binary) structure definition.

use crate::capabilities::TargetCapabilities;
use crate::extension::ExtensionTable;
use crate::metadata::{
    AcceleratorMetadata, BuildMetadata, DebugInfo, OptimizationMetadata, SafetyMetadata,
    SecurityMetadata, ThreadSafetyMetadata, UnwindMetadata,
};
use crate::section::{AdobSection, MemoryRegion, SectionKind};
use crate::symbol::AdobSymbol;
use crate::target::TargetDescriptor;

pub const ADOB_MAGIC: &[u8; 4] = b"ADOB";
pub const ADOB_VERSION_MAJOR: u16 = 1;
pub const ADOB_VERSION_MINOR: u16 = 0;
pub const ADOB_VERSION_PATCH: u16 = 0;

/// ADOB binary header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobHeader {
    pub magic: [u8; 4],
    pub version_major: u16,
    pub version_minor: u16,
    pub version_patch: u16,
    pub header_size: u32,
    pub flags: u32,
    pub section_count: u32,
    pub symbol_count: u32,
    pub import_count: u32,
    pub export_count: u32,
    pub extension_count: u32,
}

impl Default for AdobHeader {
    fn default() -> Self {
        Self {
            magic: *ADOB_MAGIC,
            version_major: ADOB_VERSION_MAJOR,
            version_minor: ADOB_VERSION_MINOR,
            version_patch: ADOB_VERSION_PATCH,
            header_size: 64,
            flags: 0,
            section_count: 0,
            symbol_count: 0,
            import_count: 0,
            export_count: 0,
            extension_count: 0,
        }
    }
}

/// First-class ADOB Object representing a compiled module or unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobObject {
    pub header: AdobHeader,
    pub target: TargetDescriptor,
    pub capabilities: TargetCapabilities,
    pub sections: Vec<AdobSection>,
    pub symbols: Vec<AdobSymbol>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub security: SecurityMetadata,
    pub safety: SafetyMetadata,
    pub thread_safety: ThreadSafetyMetadata,
    pub optimization: OptimizationMetadata,
    pub debug: DebugInfo,
    pub unwind: UnwindMetadata,
    pub accelerator: AcceleratorMetadata,
    pub memory_regions: Vec<MemoryRegion>,
    pub extensions: ExtensionTable,
    pub build_metadata: BuildMetadata,
}

impl AdobObject {
    pub fn new(target: TargetDescriptor) -> Self {
        let capabilities = TargetCapabilities::for_architecture(&target.architecture);
        Self {
            header: AdobHeader::default(),
            target,
            capabilities,
            sections: Vec::new(),
            symbols: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            security: SecurityMetadata::default(),
            safety: SafetyMetadata::default(),
            thread_safety: ThreadSafetyMetadata::default(),
            optimization: OptimizationMetadata::default(),
            debug: DebugInfo::default(),
            unwind: UnwindMetadata::default(),
            accelerator: AcceleratorMetadata::default(),
            memory_regions: Vec::new(),
            extensions: ExtensionTable::new(),
            build_metadata: BuildMetadata::default(),
        }
    }

    /// Add a section and return its index.
    pub fn add_section(&mut self, section: AdobSection) -> u32 {
        let idx = self.sections.len() as u32;
        self.sections.push(section);
        self.header.section_count = self.sections.len() as u32;
        idx
    }

    /// Add a symbol and return its id.
    pub fn add_symbol(&mut self, mut symbol: AdobSymbol) -> u32 {
        let id = self.symbols.len() as u32;
        symbol.id = id;
        self.symbols.push(symbol);
        self.header.symbol_count = self.symbols.len() as u32;
        id
    }

    /// Add an import dependency name.
    pub fn add_import(&mut self, name: impl Into<String>) {
        let name_str = name.into();
        if !self.imports.contains(&name_str) {
            self.imports.push(name_str);
            self.header.import_count = self.imports.len() as u32;
        }
    }

    /// Add an export symbol name.
    pub fn add_export(&mut self, name: impl Into<String>) {
        let name_str = name.into();
        if !self.exports.contains(&name_str) {
            self.exports.push(name_str);
            self.header.export_count = self.exports.len() as u32;
        }
    }

    /// Find section by name.
    pub fn find_section(&self, name: &str) -> Option<(usize, &AdobSection)> {
        self.sections
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == name)
    }

    /// Find mutable section by name.
    pub fn find_section_mut(&mut self, name: &str) -> Option<(usize, &mut AdobSection)> {
        self.sections
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.name == name)
    }

    /// Find section by kind.
    pub fn find_section_by_kind(&self, kind: SectionKind) -> Option<(usize, &AdobSection)> {
        self.sections
            .iter()
            .enumerate()
            .find(|(_, s)| s.kind == kind)
    }

    /// Find symbol by name.
    pub fn find_symbol(&self, name: &str) -> Option<&AdobSymbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// Find mutable symbol by name.
    pub fn find_symbol_mut(&mut self, name: &str) -> Option<&mut AdobSymbol> {
        self.symbols.iter_mut().find(|s| s.name == name)
    }

    /// Get text section data, if present.
    pub fn text_data(&self) -> Option<&[u8]> {
        self.find_section_by_kind(SectionKind::Text)
            .map(|(_, s)| s.data.as_slice())
    }

    /// Get total size of all sections.
    pub fn total_section_size(&self) -> u64 {
        self.sections.iter().map(|s| s.data.len() as u64).sum()
    }
}
