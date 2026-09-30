//! Central Linker pipeline driver and orchestrator.

use crate::archive::Archive;
use crate::config::{BuildIdStyle, LinkConfig, LtoMode};
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

        if config.lto != LtoMode::Off {
            return Err(LinkError::new(
                ErrorCode::InvalidTarget,
                format!(
                    "{:?} IR-level LTO is not supported for native object inputs",
                    config.lto
                ),
            )
            .with_suggestion(
                "Use -O3 for supported section GC/ICF. True LTO requires an IR-bearing input format and optimizer; no output was written.",
            ));
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

        // 4. Symbol Resolution and Archive Extraction (Target-Aware)
        ctx.resolver
            .resolve_with_target(&mut ctx.objects, &ctx.archives, &ctx.config.target)?;

        // 4.1 Synthesize Compiler Intrinsics, Entry Thunks, and Import Thunks for Unimplemented Symbols
        let is_pe = ctx.config.target.format == ObjectFormat::Pe;

        // Determine actual Adesh program entry symbol
        let program_entry_symbol = if ctx.resolver.table.contains_key("main") {
            "main".to_string()
        } else if ctx.resolver.table.contains_key("__user_main") {
            "__user_main".to_string()
        } else if ctx.resolver.table.contains_key("__top_level_wrapper") {
            "__top_level_wrapper".to_string()
        } else if ctx.resolver.table.contains_key("adesh_main") {
            "adesh_main".to_string()
        } else {
            "main".to_string()
        };

        let mut missing_symbols: Vec<String> = Vec::new();
        if is_pe && !ctx.resolver.table.contains_key("mainCRTStartup") {
            missing_symbols.push("mainCRTStartup".to_string());
        }
        for (name, resolved) in &ctx.resolver.table {
            if resolved.symbol.section_index.is_none() && !missing_symbols.contains(name) {
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
            let mut synth_relocs = Vec::new();

            if is_pe && missing_symbols.contains(&"mainCRTStartup".to_string()) {
                let (bytes, relocs, syms) = crate::pe::x86_64::synthesize_windows_x86_64_entry(
                    &program_entry_symbol,
                    file_idx,
                );
                let start_off = code_bytes.len() as u64;
                code_bytes.extend_from_slice(&bytes);
                for mut r in relocs {
                    r.offset += start_off;
                    synth_relocs.push(r);
                }
                for mut s in syms {
                    s.value += start_off;
                    synth_symbols.push(s);
                }
            }

            for name in &missing_symbols {
                if name == "mainCRTStartup" || name == "__adesh_windows_start" {
                    continue;
                }

                // If symbol is an __imp_ pointer or _fltused, handle appropriately
                if name.starts_with("__imp_")
                    || name.starts_with("_imp_")
                    || name == "_fltused"
                    // PE TLS data symbols are linker-owned. They are not
                    // functions and must never become import thunks.
                    || name == "_tls_index"
                    || name == "_tls_used"
                    || name == "__tls_used"
                    || name == "_load_config_used"
                {
                    continue;
                }

                let start_off = code_bytes.len() as u64;
                let (bytes, relocs) = if crate::intrinsics::IntrinsicsEngine::is_intrinsic(name) {
                    (
                        crate::intrinsics::IntrinsicsEngine::emit_intrinsic_code(
                            name,
                            &ctx.config.target,
                        ),
                        Vec::new(),
                    )
                } else if is_pe && ctx.config.target.arch == crate::target::Arch::X86_64 {
                    // PE x86_64 Import Thunk:
                    // jmp qword ptr [__imp_<name>]
                    // \xff\x25 <disp32> \x90 \x90
                    let raw = name
                        .strip_prefix("__imp_")
                        .or_else(|| name.strip_prefix("_imp_"))
                        .unwrap_or(name);
                    let b = vec![0xff, 0x25, 0x00, 0x00, 0x00, 0x00, 0x90, 0x90];
                    let r = vec![crate::relocation::Relocation {
                        offset: start_off + 2,
                        symbol_name: format!("__imp_{}", raw),
                        symbol_index: None,
                        file_index: None,
                        kind: crate::relocation::RelocationKind::PcRelative32,
                        addend: -4,
                    }];
                    (b, r)
                } else {
                    // If an unresolved symbol cannot be routed or synthesized, fail cleanly
                    return Err(LinkError::undefined_symbol(
                        name,
                        "unresolved required application symbol",
                        None,
                        None,
                    ));
                };

                let sz = bytes.len() as u64;
                code_bytes.extend_from_slice(&bytes);
                synth_relocs.extend(relocs);
                // 16-byte align each stub/thunk for performance and clean layout
                while (code_bytes.len() & 15) != 0 {
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

            let mut sec = crate::section::Section::new_code(".text.synth", code_bytes, 16);
            sec.relocations = synth_relocs;
            synth_obj.add_section(sec);

            // Add _fltused in .rdata if requested or on PE
            if is_pe || missing_symbols.iter().any(|s| s == "_fltused") {
                let flt_sec_idx = synth_obj.sections.len();
                let flt_sec = crate::section::Section::new_data(
                    ".rdata",
                    vec![0x01, 0x00, 0x00, 0x00],
                    false,
                    4,
                );
                synth_obj.add_section(flt_sec);
                let flt_sym = crate::symbol::Symbol {
                    name: "_fltused".to_string(),
                    binding: crate::symbol::SymbolBinding::Global,
                    visibility: crate::symbol::SymbolVisibility::Default,
                    sym_type: crate::symbol::SymbolType::Object,
                    section_index: Some(flt_sec_idx),
                    value: 0,
                    size: 4,
                    is_defined: true,
                    is_imported: false,
                    is_exported: false,
                    file_index: Some(file_idx),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };
                synth_symbols.push(flt_sym);
            }

            for sym in synth_symbols {
                if let Some(resolved) = ctx.resolver.table.get_mut(&sym.name) {
                    resolved.symbol = sym.clone();
                    resolved.defined_in_file_index = file_idx;
                    resolved.defined_in_sec_index = sym.section_index;
                } else {
                    ctx.resolver.table.insert(
                        sym.name.clone(),
                        crate::resolver::ResolvedSymbol {
                            symbol: sym.clone(),
                            defined_in_file_index: file_idx,
                            defined_in_sec_index: sym.section_index,
                            references: Vec::new(),
                        },
                    );
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

        // Surface non-fatal relocation issues (weak/internal symbols that could
        // not be resolved and were routed to trap stubs or NULL).
        for warning in &ctx.layout.warnings {
            ctx.diagnostics.emit_warning(warning.clone());
            eprintln!("adeshlink: warning: {}", warning);
        }

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
                // The import table and base relocations were built once during
                // layout (the same table whose IAT addresses were patched into
                // import thunks). Rebuilding them here from a differently
                // ordered symbol set would desynchronize the .idata bytes from
                // the patched code, so the layout result is authoritative.
                PeWriter::write_executable_with_layout(
                    &ctx.config.output_path,
                    &ctx.config.target,
                    ctx.layout.entry_va,
                    &ctx.layout.merged_sections,
                    &ctx.layout.resolved_symbols,
                    &[],
                    ctx.layout.pe_import_info.as_ref(),
                    ctx.layout.pe_tls_info.as_ref(),
                    &ctx.layout.base_relocs,
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
