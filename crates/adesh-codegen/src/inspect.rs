//! Phase 9 Compiler Introspection & Object Inspection Framework.
//!
//! Provides inspection capabilities for:
//! - AST, HIR, MIR, SSA, and Machine IR graphs
//! - ADOB object file layout, sections, symbols, relocations, and metadata
//! - Executable PE/ELF/Mach-O headers and symbol tables
//! - Linker maps, target specs, and ABI contract summaries

use crate::machine_ir::NativeModule;
use adesh_object::AdobObject;
use serde::{Deserialize, Serialize};

/// Target component for compiler introspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectComponent {
    Ast,
    Hir,
    Mir,
    Ssa,
    MachineIr,
    Adob,
    Symbols,
    Relocations,
    Sections,
    Abi,
    Target,
    OptimizationReport,
}

/// Detailed dump report for ADOB or binary objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectDumpReport {
    pub object_name: String,
    pub target_triple: String,
    pub pointer_width: u8,
    pub endianness: String,
    pub entry_point: Option<String>,
    pub sections: Vec<SectionDumpInfo>,
    pub symbols: Vec<SymbolDumpInfo>,
    pub relocations_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionDumpInfo {
    pub name: String,
    pub size_bytes: usize,
    pub flags: String,
    pub virtual_address: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDumpInfo {
    pub name: String,
    pub section: String,
    pub value: u64,
    pub size: u64,
    pub is_exported: bool,
}

/// Compiler Introspection Engine.
pub struct CompilerInspector;

impl CompilerInspector {
    /// Inspect a NativeModule and format the requested component.
    pub fn inspect_machine_ir(module: &NativeModule) -> String {
        let mut out = String::new();
        out.push_str(&format!("Module: {}\n", module.name));
        out.push_str(&format!("Functions ({}):\n", module.functions.len()));
        for func in &module.functions {
            out.push_str(&format!(
                "  Function {} (blocks: {}, exported: {})\n",
                func.name,
                func.blocks.len(),
                func.is_exported
            ));
            for block in &func.blocks {
                out.push_str(&format!(
                    "    Block {} (insts: {}):\n",
                    block.label,
                    block.instructions.len()
                ));
                for inst in &block.instructions {
                    out.push_str(&format!("      {:?}\n", inst));
                }
            }
        }
        out
    }

    /// Disassemble / inspect an AdobObject (equivalent of `adesh objdump`).
    pub fn dump_adob(obj: &AdobObject) -> ObjectDumpReport {
        let mut sections = Vec::new();
        for sec in &obj.sections {
            sections.push(SectionDumpInfo {
                name: sec.name.clone(),
                size_bytes: sec.data.len(),
                flags: format!("{:?}", sec.flags),
                virtual_address: sec.alignment,
            });
        }

        let mut symbols = Vec::new();
        for sym in &obj.symbols {
            let sec_str = sym
                .section_index
                .map(|idx| format!(".sec_{}", idx))
                .unwrap_or_else(|| "*UND*".to_string());
            symbols.push(SymbolDumpInfo {
                name: sym.name.clone(),
                section: sec_str,
                value: sym.value,
                size: sym.size,
                is_exported: sym.binding == adesh_object::SymbolBinding::Global,
            });
        }

        let total_relocs = obj.sections.iter().map(|s| s.relocations.len()).sum();

        ObjectDumpReport {
            object_name: obj.build_metadata.target_triple.clone(),
            target_triple: format!("{:?}", obj.target.architecture),
            pointer_width: obj.target.pointer_width.bytes(),
            endianness: format!("{:?}", obj.target.endianness),
            entry_point: None,
            sections,
            symbols,
            relocations_count: total_relocs,
        }
    }

    /// Format ObjectDumpReport as a readable disassembly listing.
    pub fn format_objdump(report: &ObjectDumpReport) -> String {
        let mut s = String::new();
        s.push_str(&format!("Object:   {}\n", report.object_name));
        s.push_str(&format!("Target:   {}\n", report.target_triple));
        s.push_str(&format!("Endian:   {}\n", report.endianness));
        s.push_str(&format!(
            "Entry:    {}\n",
            report.entry_point.as_deref().unwrap_or("<none>")
        ));
        s.push_str("\nSections:\n");
        for sec in &report.sections {
            s.push_str(&format!(
                "  [0x{:08x}] {:<16} size: {:>6} bytes  flags: {}\n",
                sec.virtual_address, sec.name, sec.size_bytes, sec.flags
            ));
        }
        s.push_str("\nSymbols:\n");
        for sym in &report.symbols {
            s.push_str(&format!(
                "  0x{:08x} {:<24} (sec: {:<12} size: {:>4} exp: {})\n",
                sym.value, sym.name, sym.section, sym.size, sym.is_exported
            ));
        }
        s.push_str(&format!(
            "\nTotal Relocations: {}\n",
            report.relocations_count
        ));
        s
    }
}
