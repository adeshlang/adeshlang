//! Link Map, Link Report, and Dependency Graph generators.

use crate::config::MapFormat;
use crate::layout::LayoutEngine;
use crate::object::ObjectFile;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

pub struct LinkMapGenerator;

impl LinkMapGenerator {
    pub fn generate_map(
        objects: &[ObjectFile],
        layout: &LayoutEngine,
        format: MapFormat,
    ) -> String {
        match format {
            MapFormat::Text => Self::generate_text_map(objects, layout),
            MapFormat::Json => Self::generate_json_map(objects, layout),
        }
    }

    pub fn write_map_to_file(
        objects: &[ObjectFile],
        layout: &LayoutEngine,
        format: MapFormat,
        path: &Path,
    ) -> std::io::Result<()> {
        let content = Self::generate_map(objects, layout, format);
        fs::write(path, content)
    }

    pub fn generate_text_map(objects: &[ObjectFile], layout: &LayoutEngine) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "================================================================================"
        )
        .unwrap();
        writeln!(
            out,
            "                             ADESH LINK MAP                                     "
        )
        .unwrap();
        writeln!(
            out,
            "================================================================================\n"
        )
        .unwrap();

        writeln!(out, "Input Objects ({}):", objects.len()).unwrap();
        for obj in objects {
            writeln!(out, "  * {}", obj.display_name()).unwrap();
        }
        writeln!(out).unwrap();

        writeln!(out, "Output Sections:").unwrap();
        writeln!(
            out,
            "  {:<18} {:<18} {:<12} {:<8}",
            "Section", "Virtual Address", "Size (bytes)", "Align"
        )
        .unwrap();
        writeln!(
            out,
            "  ------------------------------------------------------------"
        )
        .unwrap();
        for sec in &layout.merged_sections {
            writeln!(
                out,
                "  {:<18} 0x{:016x} {:<12} {:<8}",
                sec.name, sec.virtual_address, sec.size, sec.alignment
            )
            .unwrap();
        }
        writeln!(out).unwrap();

        writeln!(out, "Symbols:").unwrap();
        writeln!(
            out,
            "  {:<18} {:<8} {:<8} {:<32}",
            "Address", "Size", "Bind", "Name"
        )
        .unwrap();
        writeln!(
            out,
            "  ------------------------------------------------------------"
        )
        .unwrap();
        for sym in &layout.resolved_symbols {
            if sym.is_defined {
                let bind_str = match sym.binding {
                    crate::symbol::SymbolBinding::Global => "GLOBAL",
                    crate::symbol::SymbolBinding::Local => "LOCAL",
                    crate::symbol::SymbolBinding::Weak => "WEAK",
                };
                writeln!(
                    out,
                    "  0x{:016x} {:<8} {:<8} {:<32}",
                    sym.value, sym.size, bind_str, sym.name
                )
                .unwrap();
            }
        }

        out
    }

    pub fn generate_json_map(objects: &[ObjectFile], layout: &LayoutEngine) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str("  \"inputs\": [\n");
        for (i, obj) in objects.iter().enumerate() {
            let comma = if i + 1 < objects.len() { "," } else { "" };
            out.push_str(&format!(
                "    \"{}\"{}\n",
                obj.display_name().replace('\\', "/"),
                comma
            ));
        }
        out.push_str("  ],\n");

        out.push_str("  \"sections\": [\n");
        for (i, sec) in layout.merged_sections.iter().enumerate() {
            let comma = if i + 1 < layout.merged_sections.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "    {{\"name\": \"{}\", \"address\": \"0x{:x}\", \"size\": {}}}{}\n",
                sec.name, sec.virtual_address, sec.size, comma
            ));
        }
        out.push_str("  ],\n");

        out.push_str("  \"symbols\": [\n");
        for (i, sym) in layout.resolved_symbols.iter().enumerate() {
            let comma = if i + 1 < layout.resolved_symbols.len() {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "    {{\"name\": \"{}\", \"address\": \"0x{:x}\", \"size\": {}}}{}\n",
                sym.name, sym.value, sym.size, comma
            ));
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        out
    }

    pub fn print_report(
        objects: &[ObjectFile],
        layout: &LayoutEngine,
        removed_sections: usize,
        folded_sections: usize,
        duration_ms: u128,
    ) {
        let mut total_symbols = 0;
        let mut total_relocs = 0;
        let mut total_code_size = 0u64;
        let mut total_data_size = 0u64;

        for obj in objects {
            total_symbols += obj.symbols.len();
            for sec in &obj.sections {
                total_relocs += sec.relocations.len();
            }
        }

        for sec in &layout.merged_sections {
            if sec.is_executable() {
                total_code_size += sec.size;
            } else {
                total_data_size += sec.size;
            }
        }

        println!("\n========================================");
        println!("           Adesh Link Report            ");
        println!("========================================");
        println!("Input objects:        {}", objects.len());
        println!("Total symbols:        {}", total_symbols);
        println!("Total relocations:    {}", total_relocs);
        println!("Dead sections purged: {}", removed_sections);
        println!("Functions folded:     {}", folded_sections);
        println!("Final code size:      {} bytes", total_code_size);
        println!("Final data size:      {} bytes", total_data_size);
        println!("Link duration:        {} ms", duration_ms);
        println!("========================================\n");
    }

    pub fn print_dependency_graph(objects: &[ObjectFile]) {
        println!("\nDependency Graph:");
        for obj in objects {
            println!("  {}", obj.display_name());
            for sec in &obj.sections {
                for r in &sec.relocations {
                    println!(
                        "    └── references `{}` via {}",
                        r.symbol_name,
                        r.kind.name()
                    );
                }
            }
        }
        println!();
    }
}
