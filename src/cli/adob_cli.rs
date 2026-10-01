//! ADOB CLI tools: inspect, dump-symbols, dump-relocations, dump-sections, validate.

use adesh_object::{AdobReader, AdobValidator};
use std::fs;
use std::path::Path;

pub fn execute_adob_cli(args: &[String]) {
    if args.is_empty() {
        print_adob_help();
        return;
    }

    let subcommand = &args[0];
    let file_arg = args.get(1);

    match subcommand.as_str() {
        "inspect" => {
            if let Some(path) = file_arg {
                adob_inspect(path);
            } else {
                eprintln!("Error: Missing file argument for `adesh adob inspect <file.adob>`");
                std::process::exit(1);
            }
        }
        "dump-symbols" => {
            if let Some(path) = file_arg {
                adob_dump_symbols(path);
            } else {
                eprintln!("Error: Missing file argument for `adesh adob dump-symbols <file.adob>`");
                std::process::exit(1);
            }
        }
        "dump-relocations" => {
            if let Some(path) = file_arg {
                adob_dump_relocations(path);
            } else {
                eprintln!(
                    "Error: Missing file argument for `adesh adob dump-relocations <file.adob>`"
                );
                std::process::exit(1);
            }
        }
        "dump-sections" => {
            if let Some(path) = file_arg {
                adob_dump_sections(path);
            } else {
                eprintln!(
                    "Error: Missing file argument for `adesh adob dump-sections <file.adob>`"
                );
                std::process::exit(1);
            }
        }
        "validate" => {
            if let Some(path) = file_arg {
                adob_validate(path);
            } else {
                eprintln!("Error: Missing file argument for `adesh adob validate <file.adob>`");
                std::process::exit(1);
            }
        }
        "--help" | "-h" | "help" => {
            print_adob_help();
        }
        other => {
            eprintln!("Unknown ADOB subcommand `{}`", other);
            print_adob_help();
            std::process::exit(1);
        }
    }
}

fn print_adob_help() {
    println!("Adesh Native Object Binary (ADOB) Inspection Tools");
    println!("Usage:");
    println!("  adesh adob inspect <file.adob>          Display full ADOB object overview");
    println!("  adesh adob dump-symbols <file.adob>      Dump symbol table");
    println!("  adesh adob dump-relocations <file.adob>  Dump relocations table");
    println!("  adesh adob dump-sections <file.adob>     Dump section headers and metadata");
    println!("  adesh adob validate <file.adob>         Perform strict binary validation");
}

fn load_adob(path_str: &str) -> adesh_object::AdobObject {
    let path = Path::new(path_str);
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: Failed to read `{}`: {}", path_str, e);
            std::process::exit(1);
        }
    };

    match AdobReader::read_object(&bytes) {
        Ok(obj) => obj,
        Err(e) => {
            eprintln!("Error: Failed to decode ADOB object `{}`: {}", path_str, e);
            std::process::exit(1);
        }
    }
}

fn adob_inspect(path: &str) {
    let obj = load_adob(path);
    println!("=== ADOB Object Inspection: {} ===", path);
    println!(
        "Magic:             {:?}",
        std::str::from_utf8(&obj.header.magic).unwrap_or("????")
    );
    println!(
        "Version:           {}.{}.{}",
        obj.header.version_major, obj.header.version_minor, obj.header.version_patch
    );
    println!("Target Triple:     {}", obj.target.triple_string());
    println!("Compute Device:    {:?}", obj.target.device);
    println!("Architecture:      {:?}", obj.target.architecture);
    println!("Operating System:  {:?}", obj.target.operating_system);
    println!("ABI:               {:?}", obj.target.abi);
    println!(
        "Pointer Width:     {} bytes",
        obj.target.pointer_width.bytes()
    );
    println!("Endianness:        {:?}", obj.target.endianness);
    println!("Sections Count:    {}", obj.sections.len());
    println!("Symbols Count:     {}", obj.symbols.len());
    println!("Imports Count:     {}", obj.imports.len());
    println!("Exports Count:     {}", obj.exports.len());
    println!("Memory Regions:    {}", obj.memory_regions.len());
    println!("Extensions Count:  {}", obj.extensions.extensions.len());

    if !obj.imports.is_empty() {
        println!("\nImports:");
        for imp in &obj.imports {
            println!("  - {}", imp);
        }
    }

    if !obj.exports.is_empty() {
        println!("\nExports:");
        for exp in &obj.exports {
            println!("  - {}", exp);
        }
    }
}

fn adob_dump_symbols(path: &str) {
    let obj = load_adob(path);
    println!("=== Symbol Table ({}) ===", path);
    println!(
        "{:<6} {:<30} {:<8} {:<10} {:<10} {:<8} {:<8} {:<10} {:<8}",
        "ID", "Name", "Binding", "Visibility", "Kind", "Defined", "Section", "Value", "Size"
    );
    println!("{:-<100}", "");

    for sym in &obj.symbols {
        let sec_str = sym
            .section_index
            .map_or("N/A".to_string(), |s| s.to_string());
        println!(
            "{:<6} {:<30} {:<8?} {:<10?} {:<10?} {:<8} {:<8} 0x{:<8X} 0x{:<6X}",
            sym.id,
            sym.name,
            sym.binding,
            sym.visibility,
            sym.kind,
            sym.is_defined,
            sec_str,
            sym.value,
            sym.size
        );
    }
}

fn adob_dump_relocations(path: &str) {
    let obj = load_adob(path);
    println!("=== Relocation Table ({}) ===", path);

    for (s_idx, sec) in obj.sections.iter().enumerate() {
        if sec.relocations.is_empty() {
            continue;
        }
        println!(
            "\nSection #{} `{}` ({} relocations):",
            s_idx,
            sec.name,
            sec.relocations.len()
        );
        println!(
            "  {:<12} {:<24} {:<24} {:<10} {:<6}",
            "Offset", "Symbol", "Kind", "Addend", "Width"
        );
        println!("  {:-<80}", "");

        for r in &sec.relocations {
            println!(
                "  0x{:<10X} {:<24} {:<24?} {:<10} {}B",
                r.offset, r.symbol_name, r.kind, r.addend, r.width
            );
        }
    }
}

fn adob_dump_sections(path: &str) {
    let obj = load_adob(path);
    println!("=== Section Table ({}) ===", path);
    println!(
        "{:<6} {:<20} {:<12} {:<10} {:<8} {:<12} {:<16}",
        "Index", "Name", "Kind", "Size", "Align", "Relocations", "Region/Comdat"
    );
    println!("{:-<90}", "");

    for (idx, sec) in obj.sections.iter().enumerate() {
        let extra = if let Some(ref comdat) = sec.comdat_group {
            format!("comdat: {}", comdat)
        } else if let Some(ref reg) = sec.memory_region {
            format!("region: {}", reg)
        } else {
            "-".to_string()
        };

        println!(
            "{:<6} {:<20} {:<12?} 0x{:<8X} {:<8} {:<12} {:<16}",
            idx,
            sec.name,
            sec.kind,
            sec.data.len(),
            sec.alignment,
            sec.relocations.len(),
            extra
        );
    }
}

fn adob_validate(path: &str) {
    let obj = load_adob(path);
    match AdobValidator::validate(&obj) {
        Ok(()) => {
            println!("✓ ADOB object `{}` passed all validation checks.", path);
        }
        Err(e) => {
            eprintln!("✗ ADOB Validation Failed for `{}`:", path);
            eprintln!("  Code:    {:?}", e.code);
            eprintln!("  Message: {}", e.message);
            if let Some(ref sec) = e.section {
                eprintln!("  Section: {}", sec);
            }
            if let Some(ref sym) = e.symbol {
                eprintln!("  Symbol:  {}", sym);
            }
            if let Some(off) = e.offset {
                eprintln!("  Offset:  0x{:X}", off);
            }
            std::process::exit(1);
        }
    }
}
