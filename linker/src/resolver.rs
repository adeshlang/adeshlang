//! Global Symbol Resolution, Weak/Strong precedence, Duplicate detection, and Archive extraction.

use crate::archive::Archive;
use crate::error::{LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::symbol::{Symbol, SymbolBinding};
use std::collections::{HashMap, HashSet};

/// Global symbol table entry tracking resolution origin.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub symbol: Symbol,
    pub defined_in_file_index: usize,
    pub defined_in_sec_index: Option<usize>,
    pub references: Vec<(usize, usize)>, // (file_index, offset)
}

/// Symbol resolution engine.
pub struct SymbolResolver {
    pub table: HashMap<String, ResolvedSymbol>,
    pub undefined: HashSet<String>,
}

impl SymbolResolver {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
            undefined: HashSet::new(),
        }
    }

    /// Ingest symbols from object files and resolve them.
    pub fn resolve(
        &mut self,
        objects: &mut Vec<ObjectFile>,
        archives: &[Archive],
    ) -> LinkResult<()> {
        // 1. Initial pass: Ingest all explicitly provided object files
        for obj in objects.iter() {
            self.ingest_object(obj)?;
        }

        // 2. Archive resolution loop: Extract required members until fixed point
        let mut progress = true;
        let mut extracted_members = HashSet::new();
        while progress && !self.undefined.is_empty() {
            progress = false;
            for ar in archives {
                let current_undef: Vec<String> = self.undefined.iter().cloned().collect();
                for undef_sym in current_undef {
                    if let Some(&m_idx) = ar.symbol_index.get(&undef_sym) {
                        if m_idx < ar.members.len() && extracted_members.insert((ar.path.clone(), m_idx)) {
                            let member = &ar.members[m_idx];
                            let mut member_obj = if let Some(ref o) = member.obj {
                                let mut cloned = o.clone();
                                cloned.file_index = objects.len();
                                cloned
                            } else {
                                crate::object::reader::ObjectReader::read_from_memory(
                                    &member.data,
                                    std::path::Path::new(&member.name),
                                    &crate::target::Target::host(),
                                    objects.len(),
                                )?
                            };
                            member_obj.is_archive_member = true;
                            member_obj.archive_name = Some(ar.path.display().to_string());
                            self.ingest_object(&member_obj)?;
                            objects.push(member_obj);
                            progress = true;
                        }
                    }
                }
            }
        }

        // 3. Resolve well-known system, CRT, and Adesh runtime bridge symbols as dynamic imports or stubs
        let system_crt_symbols = [
            "printf", "puts", "putchar", "malloc", "free", "calloc", "realloc", "exit", "abort",
            "memcpy", "memset", "memmove", "memcmp", "strlen", "strcmp", "strncmp", "strcpy", "strncpy",
            "snprintf", "sprintf", "vsnprintf", "getchar", "fprintf", "fflush", "fopen", "fclose",
            "fread", "fwrite", "fseek", "ftell", "time", "clock", "getenv", "system",
            "trunc", "truncf", "floor", "floorf", "ceil", "ceilf", "round", "roundf",
            "sin", "sinf", "cos", "cosf", "tan", "tanf", "asin", "asinf", "acos", "acosf",
            "atan", "atanf", "atan2", "atan2f", "sinh", "sinhf", "cosh", "coshf", "tanh", "tanhf",
            "exp", "expf", "log", "logf", "log10", "log10f", "log2", "log2f", "pow", "powf",
            "sqrt", "sqrtf", "fmod", "fmodf", "fabs", "fabsf", "fmin", "fminf", "fmax", "fmaxf",
            "copysign", "copysignf", "hypot", "hypotf", "ldexp", "frexp", "modf",
            "ExitProcess", "GetStdHandle", "WriteFile", "ReadFile", "CreateFileA", "CreateFileW",
            "CloseHandle", "GetLastError", "SetLastError", "VirtualAlloc", "VirtualFree",
            "GetProcessHeap", "HeapAlloc", "HeapFree", "Sleep", "QueryPerformanceCounter",
            "QueryPerformanceFrequency", "GetSystemTimeAsFileTime", "GetCurrentProcessId",
            "GetCurrentThreadId", "RtlCaptureContext", "RtlLookupFunctionEntry", "RtlVirtualUnwind",
            "__acrt_iob_func", "__stdio_common_vfprintf", "__stdio_common_vsprintf",
            "__CxxFrameHandler3", "__CxxFrameHandler4", "_CxxThrowException", "__chkstk",
        ];

        let remaining_undef: Vec<String> = self.undefined.iter().cloned().collect();
        for undef in remaining_undef {
            let is_system = undef.starts_with("aot_")
                || undef.starts_with("adesh_")
                || undef.starts_with("__rust")
                || undef.starts_with("rust_")
                || undef.starts_with("_")
                || undef.starts_with("?")
                || undef.starts_with("??")
                || undef.starts_with('$')
                || system_crt_symbols.iter().any(|&s| s == undef || undef.ends_with(s) || undef.trim_start_matches('_') == s);
            if is_system {
                // Synthesize defined entry for system CRT / runtime import
                self.undefined.remove(&undef);
                self.table.insert(
                    undef.clone(),
                    ResolvedSymbol {
                        symbol: Symbol {
                            name: undef,
                            binding: SymbolBinding::Global,
                            visibility: crate::symbol::SymbolVisibility::Default,
                            sym_type: crate::symbol::SymbolType::Function,
                            section_index: None,
                            value: 0,
                            size: 0,
                            is_defined: true,
                            is_imported: true,
                            is_exported: false,
                            file_index: Some(0),
                            alias_of: None,
                            comdat_group: None,
                            version: None,
                        },
                        defined_in_file_index: 0,
                        defined_in_sec_index: None,
                        references: Vec::new(),
                    },
                );
            }
        }

        // 4. Final verification of remaining undefined symbols
        if !self.undefined.is_empty() {
            // Pick first undefined symbol to report
            let first_undef = self.undefined.iter().next().unwrap();
            let mut ref_file = "input object";
            let mut ref_offset = None;
            for obj in objects.iter() {
                for sec in &obj.sections {
                    for r in &sec.relocations {
                        if r.symbol_name == *first_undef {
                            ref_file = obj.path.to_str().unwrap_or("input object");
                            ref_offset = Some(r.offset);
                            break;
                        }
                    }
                }
            }

            return Err(LinkError::undefined_symbol(
                first_undef,
                ref_file,
                None,
                ref_offset,
            ));
        }

        Ok(())
    }

    fn ingest_object(&mut self, obj: &ObjectFile) -> LinkResult<()> {
        for sym in &obj.symbols {
            if sym.is_local() {
                continue;
            }

            if sym.is_defined {
                if let Some(existing) = self.table.get_mut(&sym.name) {
                    if existing.symbol.is_defined {
                        if existing.symbol.binding == SymbolBinding::Global && sym.binding == SymbolBinding::Global {
                            // Check COMDAT / compiler-generated constant deduplication
                            let is_dedup_constant = sym.name.starts_with("__xmm@")
                                || sym.name.starts_with("__real@")
                                || sym.name.starts_with("__mask@")
                                || sym.name.starts_with("__ymm@")
                                || sym.name.starts_with("__zmm@")
                                || sym.name.starts_with(".refptr.")
                                || obj.is_archive_member
                                || (sym.comdat_group.is_some() && sym.comdat_group == existing.symbol.comdat_group);

                            if is_dedup_constant {
                                continue;
                            }
                            let first_file = format!("object #{}", existing.defined_in_file_index);
                            return Err(LinkError::duplicate_symbol(
                                &sym.name,
                                first_file,
                                obj.display_name(),
                            ));
                        } else if existing.symbol.binding == SymbolBinding::Weak && sym.binding == SymbolBinding::Global {
                            // Strong symbol overrides existing weak
                            existing.symbol = sym.clone();
                            existing.defined_in_file_index = obj.file_index;
                            existing.defined_in_sec_index = sym.section_index;
                        }
                    } else {
                        // Resolves previously undefined symbol
                        existing.symbol = sym.clone();
                        existing.defined_in_file_index = obj.file_index;
                        existing.defined_in_sec_index = sym.section_index;
                        self.undefined.remove(&sym.name);
                    }
                } else {
                    self.table.insert(
                        sym.name.clone(),
                        ResolvedSymbol {
                            symbol: sym.clone(),
                            defined_in_file_index: obj.file_index,
                            defined_in_sec_index: sym.section_index,
                            references: Vec::new(),
                        },
                    );
                    self.undefined.remove(&sym.name);
                }
            } else {
                // Undefined symbol reference
                if !self.table.contains_key(&sym.name) {
                    self.undefined.insert(sym.name.clone());
                }
            }
        }

        // Check relocations for symbol references
        let local_sym_names: HashSet<&str> = obj.symbols.iter().filter(|s| s.is_defined).map(|s| s.name.as_str()).collect();
        for sec in &obj.sections {
            for r in &sec.relocations {
                if !self.table.contains_key(&r.symbol_name) && !local_sym_names.contains(r.symbol_name.as_str()) {
                    self.undefined.insert(r.symbol_name.clone());
                }
            }
        }

        Ok(())
    }

    pub fn lookup(&self, name: &str) -> Option<&ResolvedSymbol> {
        self.table.get(name)
    }
}
