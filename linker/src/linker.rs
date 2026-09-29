//! Central Linker pipeline driver and orchestrator.

use crate::archive::Archive;
use crate::config::{BuildIdStyle, LinkConfig};
use crate::context::LinkContext;
use crate::debug::DebugProcessor;
use crate::elf::ElfWriter;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::gc::GarbageCollector;
use crate::hash::Sha256;
use crate::icf::IcfEngine;
use crate::macho::MachOWriter;
use crate::map::LinkMapGenerator;
use crate::object::ObjectReader;
use crate::pe::writer::PeWriter;
use crate::target::ObjectFormat;
use crate::wasm::WasmWriter;
use std::path::PathBuf;
use std::time::Instant;

pub struct Linker;

impl Linker {
    /// Execute the complete linking pipeline for the provided input files and configuration.
    pub fn link(input_paths: &[PathBuf], config: LinkConfig) -> LinkResult<()> {
        let start_time = Instant::now();

        if input_paths.is_empty() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "no input object files or archives specified for linking",
            )
            .with_suggestion("Pass at least one object file (e.g. `adeshlink main.o -o app`)"));
        }

        let mut ctx = LinkContext::new(config);

        // 1. Ingest input files (objects and archives)
        let mut file_index = 0;
        for path in input_paths {
            let bytes = std::fs::read(path).map_err(|e| {
                LinkError::new(
                    ErrorCode::IoError,
                    format!("cannot read file `{}`: {}", path.display(), e),
                )
            })?;

            if bytes.starts_with(b"!<arch>\n") {
                let archive = Archive::parse(&bytes, path)?;
                ctx.archives.push(archive);
            } else {
                let obj =
                    ObjectReader::read_from_memory(&bytes, path, &ctx.config.target, file_index)?;
                ctx.objects.push(obj);
                file_index += 1;
            }
        }

        // Ingest library flags (-L / -l)
        for lib_name in &ctx.config.libraries {
            let mut found = false;
            for search_dir in &ctx.config.library_search_paths {
                let lib_path_a = search_dir.join(format!("lib{}.a", lib_name));
                let lib_path_lib = search_dir.join(format!("{}.lib", lib_name));
                let target_path = if lib_path_a.exists() {
                    Some(lib_path_a)
                } else if lib_path_lib.exists() {
                    Some(lib_path_lib)
                } else {
                    None
                };

                if let Some(p) = target_path {
                    let bytes = std::fs::read(&p)?;
                    if bytes.starts_with(b"!<arch>\n") {
                        let archive = Archive::parse(&bytes, &p)?;
                        ctx.archives.push(archive);
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                ctx.diagnostics.emit_warning(format!(
                    "library `-l{}` was not found in specified search paths",
                    lib_name
                ));
            }
        }

        // 2. ABI and Architecture Compatibility Validation
        for obj in &ctx.objects {
            if obj.target.arch != ctx.config.target.arch {
                return Err(LinkError::architecture_mismatch(
                    obj.display_name(),
                    obj.target.arch.as_str(),
                    ctx.config.target.arch.as_str(),
                ));
            }
        }

        // Validate Adesh metadata consistency
        let mut first_meta: Option<crate::metadata::AdeshMetadata> = None;
        for obj in &ctx.objects {
            if let Some(ref meta) = obj.metadata {
                if let Some(ref first) = first_meta {
                    meta.validate_compatibility(first, &obj.display_name(), "first object")?;
                } else {
                    first_meta = Some(meta.clone());
                }
            }
        }

        // 3. Debug Information Processing
        DebugProcessor::strip_debug_sections(
            &mut ctx.objects,
            ctx.config.strip,
            ctx.config.strip_debug,
        );

        // 4. Symbol Resolution and Archive Extraction
        ctx.resolver.resolve(&mut ctx.objects, &ctx.archives)?;

        // 4.1 Synthesize Compiler Intrinsics and Stubs for Unimplemented Symbols
        let mut missing_symbols: Vec<String> = Vec::new();
        for (name, resolved) in &ctx.resolver.table {
            if resolved.symbol.section_index.is_none() {
                missing_symbols.push(name.clone());
            }
        }

        if !missing_symbols.is_empty() {
            let file_idx = ctx.objects.len();
            let mut synth_obj = crate::object::ObjectFile::new(
                PathBuf::from("__runtime_intrinsics_and_stubs.o"),
                ctx.config.target.clone(),
                file_idx,
            );

            let mut code_bytes = Vec::new();
            let mut synth_symbols = Vec::new();

            for name in &missing_symbols {
                let start_off = code_bytes.len() as u64;
                let bytes = if crate::intrinsics::IntrinsicsEngine::is_intrinsic(name) {
                    crate::intrinsics::IntrinsicsEngine::emit_intrinsic_code(
                        name,
                        &ctx.config.target,
                    )
                } else {
                    // Default return 0 stub for CRT / external functions: xor eax, eax; ret
                    match ctx.config.target.arch {
                        crate::target::Arch::X86_64 | crate::target::Arch::X86 => {
                            vec![0x31, 0xc0, 0xc3, 0x90]
                        }
                        crate::target::Arch::AArch64 => {
                            vec![0x00, 0x00, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6]
                        } // mov w0, #0; ret
                        crate::target::Arch::Riscv64 | crate::target::Arch::Riscv32 => {
                            vec![0x13, 0x05, 0x00, 0x00, 0x67, 0x80, 0x00, 0x00]
                        } // li a0, 0; ret
                        _ => vec![0xc3],
                    }
                };

                let sz = bytes.len() as u64;
                code_bytes.extend_from_slice(&bytes);
                // 4-byte align each stub
                while (code_bytes.len() & 3) != 0 {
                    code_bytes.push(0x90);
                }

                let sym = crate::symbol::Symbol {
                    name: name.clone(),
                    binding: crate::symbol::SymbolBinding::Global,
                    visibility: crate::symbol::SymbolVisibility::Default,
                    sym_type: crate::symbol::SymbolType::Function,
                    section_index: Some(0),
                    value: start_off,
                    size: sz,
                    is_defined: true,
                    is_imported: false,
                    is_exported: false,
                    file_index: Some(file_idx),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };
                synth_symbols.push(sym);
            }

            let sec = crate::section::Section::new_code(".text.synth", code_bytes, 16);
            synth_obj.add_section(sec);
            for sym in synth_symbols {
                if let Some(resolved) = ctx.resolver.table.get_mut(&sym.name) {
                    resolved.symbol = sym.clone();
                    resolved.defined_in_file_index = file_idx;
                    resolved.defined_in_sec_index = Some(0);
                }
                synth_obj.add_symbol(sym);
            }
            ctx.objects.push(synth_obj);
        }

        // 5. Optimization: Section Garbage Collection (--gc-sections)
        let mut roots = vec![
            ctx.config.effective_entry().to_string(),
            "main".to_string(),
            "_start".to_string(),
            "mainCRTStartup".to_string(),
            "__top_level_wrapper".to_string(),
        ];
        roots.extend(ctx.config.exports.iter().cloned());

        let removed_sections = if ctx.config.gc_sections {
            GarbageCollector::collect_dead_sections(
                &mut ctx.objects,
                &roots,
                ctx.config.print_gc_sections,
            )
        } else {
            0
        };

        // 6. Optimization: Identical Code Folding (--icf)
        let folded_sections =
            IcfEngine::fold_sections(&mut ctx.objects, ctx.config.icf, ctx.config.print_icf);

        // 7. Memory Layout, Virtual Address Assignment & Relocations
        ctx.layout.layout_and_relocate(
            &ctx.objects,
            &ctx.resolver,
            &ctx.config.target,
            ctx.config.effective_entry(),
        )?;

        // 8. Generate Build ID if requested
        let build_id_bytes = match ctx.config.build_id {
            BuildIdStyle::None => None,
            BuildIdStyle::Sha256 => {
                let mut hasher = Sha256::new();
                for sec in &ctx.layout.merged_sections {
                    hasher.update(&sec.data);
                }
                Some(hasher.finalize())
            }
            BuildIdStyle::Fast | BuildIdStyle::Uuid => {
                let digest = Sha256::digest(ctx.config.target.triple_string().as_bytes());
                Some(digest)
            }
        };

        // 9. Emit Output Executable / Library according to Target Object Format
        match ctx.config.target.format {
            ObjectFormat::Elf => {
                ElfWriter::write_executable(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    ctx.layout.entry_va,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                    build_id_bytes.as_ref().map(|b| &b[..20]),
                )?;
            }
            ObjectFormat::Pe => {
                let mut imports = Vec::new();
                let mut seen_imports = std::collections::HashSet::new();
                for sym in &ctx.layout.resolved_symbols {
                    if (sym.is_imported || !sym.is_defined)
                        && !sym.name.starts_with('$')
                        && !sym.name.starts_with("anon.")
                        && !sym.name.starts_with("_ZN")
                        && !sym.name.starts_with("__rust")
                        && !sym.name.starts_with("rust_")
                        && !sym.name.starts_with("??")
                        && !sym.name.starts_with('.')
                        && !sym.name.is_empty()
                        && seen_imports.insert(sym.name.clone())
                    {
                        let dll_name = if sym.name == "ExitProcess"
                            || sym.name.starts_with("Get")
                            || sym.name.starts_with("Write")
                            || sym.name.starts_with("Read")
                            || sym.name.starts_with("Virtual")
                            || sym.name.starts_with("Close")
                            || sym.name.starts_with("Sleep")
                            || sym.name.starts_with("Query")
                            || sym.name.starts_with("Rtl")
                            || sym.name.starts_with("Create")
                            || sym.name.starts_with("Set")
                        {
                            "KERNEL32.dll".to_string()
                        } else {
                            "msvcrt.dll".to_string()
                        };
                        imports.push(crate::pe::import::ImportSymbol {
                            dll_name,
                            symbol_name: sym.name.clone(),
                            ordinal: None,
                        });
                    }
                }
                PeWriter::write_executable(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    ctx.layout.entry_va,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                    &imports,
                )?;
            }
            ObjectFormat::MachO => {
                MachOWriter::write_executable(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    ctx.layout.entry_va,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                )?;
            }
            ObjectFormat::Wasm => {
                WasmWriter::write_binary(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                )?;
            }
            ObjectFormat::Xcoff => {
                crate::xcoff::XcoffWriter::write_executable(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    ctx.layout.entry_va,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                )?;
            }
            ObjectFormat::GpuFatbin => {
                crate::accelerators::GpuFatbinWriter::write_fatbin(
                    &ctx.config.output_path,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                )?;
            }
            ObjectFormat::QirQuantum => {
                crate::quantum::QuantumPackageWriter::write_qir_artifact(
                    &ctx.config.output_path,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                )?;
            }
            ObjectFormat::AdeshNative => {
                // Adesh native object writer
                let mut dummy_obj = crate::object::ObjectFile::new(
                    ctx.config.output_path.clone(),
                    ctx.config.target.clone(),
                    0,
                );
                for sec in &ctx.layout.merged_sections {
                    dummy_obj.add_section(crate::section::Section {
                        name: sec.name.clone(),
                        kind: sec.kind,
                        flags: sec.flags,
                        alignment: sec.alignment,
                        virtual_address: sec.virtual_address,
                        file_offset: sec.file_offset,
                        size: sec.size,
                        data: sec.data.clone(),
                        relocations: Vec::new(),
                        comdat_group: None,
                        file_index: Some(0),
                        is_live: true,
                        is_folded: false,
                        folded_into: None,
                    });
                }
                crate::object::writer::ObjectWriter::write_to_file(
                    &dummy_obj,
                    &ctx.config.output_path,
                )?;
            }
        }

        let elapsed = start_time.elapsed().as_millis();

        // 10. Write Link Map if requested
        if let Some(ref map_p) = ctx.config.map_file {
            LinkMapGenerator::write_map_to_file(
                &ctx.objects,
                &ctx.layout,
                ctx.config.map_format,
                map_p,
            )?;
        }

        // 11. Print Link Report and Dependency Graph if requested
        if ctx.config.report {
            LinkMapGenerator::print_report(
                &ctx.objects,
                &ctx.layout,
                removed_sections,
                folded_sections,
                elapsed,
            );
        }
        if ctx.config.dependency_graph {
            LinkMapGenerator::print_dependency_graph(&ctx.objects);
        }

        if ctx.config.verbose {
            println!(
                "adeslink: successfully linked `{}` for target `{}` in {} ms",
                ctx.config.output_path.display(),
                ctx.config.target,
                elapsed
            );
        }

        Ok(())
    }
}
