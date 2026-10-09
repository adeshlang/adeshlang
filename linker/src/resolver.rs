//! Global Symbol Resolution, Weak/Strong precedence, Duplicate detection, and Archive extraction.

use crate::archive::Archive;
use crate::error::{LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use std::collections::{HashMap, HashSet};

/// Policy governing unresolved symbol resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndefinedSymbolPolicy {
    /// Emit an explicit link error for any unresolved symbol.
    Error,
    /// Treat unresolved symbol as dynamic import.
    Import,
    /// Treat unresolved symbol as weak undefined (NULL).
    WeakUndefined,
    /// Resolve via native intrinsic generator.
    Intrinsic,
    /// Resolve via declared Adesh runtime symbol.
    RuntimeProvided,
}

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
    /// Undefined weak symbols bound to their ABI-defined zero value. The
    /// layout stage uses this set to diagnose only symbols actually referenced
    /// by a retained relocation, not every declaration seen in an object.
    pub weak_undefined_symbols: HashSet<String>,
    pub policy: UndefinedSymbolPolicy,
    /// Resolver-stage warnings. Weak-undefined diagnostics are deferred to
    /// relocation processing, where unused declarations can be ignored.
    pub warnings: Vec<String>,
}

impl Default for SymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolResolver {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
            undefined: HashSet::new(),
            weak_undefined_symbols: HashSet::new(),
            policy: UndefinedSymbolPolicy::Error,
            warnings: Vec::new(),
        }
    }

    pub fn with_policy(policy: UndefinedSymbolPolicy) -> Self {
        Self {
            table: HashMap::new(),
            undefined: HashSet::new(),
            weak_undefined_symbols: HashSet::new(),
            policy,
            warnings: Vec::new(),
        }
    }

    /// Ingest symbols from object files and resolve them using the default target.
    pub fn resolve(
        &mut self,
        objects: &mut Vec<ObjectFile>,
        archives: &[Archive],
    ) -> LinkResult<()> {
        self.resolve_with_target(objects, archives, &crate::target::Target::host())
    }

    /// Ingest symbols from object files and resolve them for a specified target.
    pub fn resolve_with_target(
        &mut self,
        objects: &mut Vec<ObjectFile>,
        archives: &[Archive],
        target: &crate::target::Target,
    ) -> LinkResult<()> {
        // 1. Initial pass: Ingest all explicitly provided object files
        for obj in objects.iter() {
            self.ingest_object(obj)?;
        }

        // 2. Archive resolution loop: Extract required members until fixed point
        let mut progress = true;
        // Keyed by (archive index, member index) so the fixed-point loop does
        // not allocate a PathBuf per lookup.
        let mut extracted_members: HashSet<(usize, usize)> = HashSet::new();
        while progress && !self.undefined.is_empty() {
            progress = false;
            for (ar_idx, ar) in archives.iter().enumerate() {
                let mut current_undef: Vec<String> = self.undefined.iter().cloned().collect();
                current_undef.sort();
                for undef_sym in current_undef {
                    if !self.undefined.contains(&undef_sym) {
                        continue;
                    }
                    let candidates = ar.symbol_index.get(&undef_sym);
                    let prefixed;
                    let candidates = if candidates.is_some() {
                        candidates
                    } else if target.arch == crate::target::Arch::X86 {
                        // The `_`-prefix equivalence is a 32-bit x86
                        // (COFF/ELF) name-mangling convention only. On 64-bit
                        // targets `_foo` and `foo` are distinct symbols and
                        // aliasing them binds the wrong definition.
                        let stripped = undef_sym.strip_prefix('_').unwrap_or(&undef_sym);
                        prefixed = format!("_{}", undef_sym);
                        ar.symbol_index
                            .get(stripped)
                            .or_else(|| ar.symbol_index.get(&prefixed))
                    } else {
                        None
                    };
                    if let Some(candidates) = candidates {
                        // A COFF archive can contain several COMDAT
                        // definitions with the same name. If one candidate
                        // was extracted for another symbol, try another
                        // defining member instead of abandoning this symbol.
                        if let Some(&m_idx) = candidates.iter().find(|&&idx| {
                            idx < ar.members.len() && !extracted_members.contains(&(ar_idx, idx))
                        }) {
                            extracted_members.insert((ar_idx, m_idx));
                            let member = &ar.members[m_idx];
                            let mut member_obj = if let Some(ref o) = member.obj {
                                o.clone()
                            } else {
                                crate::object::reader::ObjectReader::read_from_memory(
                                    &member.data,
                                    std::path::Path::new(&member.name),
                                    &crate::target::Target::host(),
                                    0,
                                )?
                            };
                            // Archive members are pre-parsed with a
                            // placeholder file index; every embedded
                            // (file, index) pair must be re-stamped to the
                            // slot this member now occupies, or precise
                            // relocation resolution consults the wrong object.
                            member_obj.reassign_file_index(objects.len());
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

        // 3. Classify remaining undefined symbols using the OS API Router.
        //    The router checks the actual target platform and routes each symbol to the
        //    correct system DLL, intrinsic generator, or marks it as a hard undefined error.
        let remaining_undef: Vec<String> = self.undefined.iter().cloned().collect();
        for undef in remaining_undef {
            use crate::os_router::{OsApiRouter, SymbolRoute};

            let route = OsApiRouter::classify(&undef, target);

            match route {
                SymbolRoute::InternalRuntime => {
                    // Dead Rust/Adesh internal — remove from undefined, register weak zero stub
                    self.undefined.remove(&undef);
                    self.table.insert(
                        undef.clone(),
                        ResolvedSymbol {
                            symbol: Symbol {
                                name: undef,
                                binding: SymbolBinding::Weak,
                                visibility: crate::symbol::SymbolVisibility::Hidden,
                                sym_type: crate::symbol::SymbolType::Function,
                                section_index: None,
                                value: 0,
                                size: 0,
                                is_defined: true,
                                is_imported: false,
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
                SymbolRoute::Intrinsic => {
                    // Will be code-generated in Step 4.1 of linker.rs
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
                                is_imported: false,
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
                SymbolRoute::LinkerSynthesized | SymbolRoute::CrtStartup => {
                    // Will be synthesized in Step 4.1 of linker.rs
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
                                is_imported: false,
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
                SymbolRoute::DllImport { dll, ref name } => {
                    // System DLL import — mark as imported so Step 9 puts it in the import table
                    self.undefined.remove(&undef);
                    let sym_name = name.clone();
                    self.table.insert(
                        undef.clone(),
                        ResolvedSymbol {
                            symbol: Symbol {
                                name: undef.clone(),
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
                    // Also store the clean name variant with the DLL tag in the table
                    // so Step 9 can look it up directly
                    let imp_name = format!("__imp_{}", sym_name);
                    if !self.table.contains_key(&imp_name) {
                        self.table.insert(
                            imp_name.clone(),
                            ResolvedSymbol {
                                symbol: Symbol {
                                    name: imp_name,
                                    binding: SymbolBinding::Global,
                                    visibility: crate::symbol::SymbolVisibility::Default,
                                    sym_type: crate::symbol::SymbolType::Object,
                                    section_index: None,
                                    value: 0,
                                    size: 8,
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
                    let _ = dll; // dll routing is done in Step 9 via OsApiRouter::windows_dll_for
                }
                SymbolRoute::Undefined => {
                    // Remains in self.undefined — will be caught in Step 4 below
                }
            }
        }

        // 4. Final verification of remaining undefined symbols
        if !self.undefined.is_empty() {
            // Under the explicit `WeakUndefined` policy every remaining
            // unresolved symbol is bound to NULL (the ABI-defined value of a
            // weak reference) instead of failing the link.
            if self.policy == UndefinedSymbolPolicy::WeakUndefined {
                let remaining: Vec<String> = self.undefined.drain().collect();
                for name in remaining {
                    self.bind_weak_undefined(&name);
                }
                return Ok(());
            }

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
                        // A weak-undefined placeholder (bound to NULL earlier)
                        // is not a real definition: a real one replaces it.
                        let is_weak_zero_placeholder = existing.symbol.section_index.is_none()
                            && existing.symbol.binding == SymbolBinding::Weak
                            && existing.symbol.sym_type != SymbolType::Common;
                        if is_weak_zero_placeholder {
                            existing.symbol = sym.clone();
                            existing.defined_in_file_index = obj.file_index;
                            existing.defined_in_sec_index = sym.section_index;
                            self.weak_undefined_symbols.remove(&sym.name);
                            continue;
                        }

                        // Tentative definitions (COMMON blocks) are never a
                        // duplicate-symbol conflict. A real definition wins;
                        // among several common blocks the first one wins
                        // (matching the first-seen-wins .bss allocation in
                        // the layout engine).
                        if sym.sym_type == SymbolType::Common {
                            continue;
                        }
                        if existing.symbol.sym_type == SymbolType::Common {
                            existing.symbol = sym.clone();
                            existing.defined_in_file_index = obj.file_index;
                            existing.defined_in_sec_index = sym.section_index;
                            continue;
                        }

                        if existing.symbol.binding == SymbolBinding::Global
                            && sym.binding == SymbolBinding::Global
                        {
                            // Check COMDAT / compiler-generated constant deduplication
                            let is_dedup_constant = sym.name.starts_with("__xmm@")
                                || sym.name.starts_with("__real@")
                                || sym.name.starts_with("__mask@")
                                || sym.name.starts_with("__ymm@")
                                || sym.name.starts_with("__zmm@")
                                || sym.name.starts_with(".refptr.")
                                || obj.is_archive_member
                                || (sym.comdat_group.is_some()
                                    && sym.comdat_group == existing.symbol.comdat_group);

                            if is_dedup_constant {
                                continue;
                            }
                            let first_file = format!("object #{}", existing.defined_in_file_index);
                            return Err(LinkError::duplicate_symbol(
                                &sym.name,
                                first_file,
                                obj.display_name(),
                            ));
                        } else if existing.symbol.binding == SymbolBinding::Weak
                            && sym.binding == SymbolBinding::Global
                        {
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
            } else if sym.binding == SymbolBinding::Weak {
                // An undefined weak symbol has the ABI-defined value NULL in
                // ELF (`SHN_UNDEF` + `STB_WEAK`) and COFF
                // (`IMAGE_SYM_CLASS_WEAK_EXTERNAL`, section 0). It must bind
                // to address 0, never fail the link.
                self.bind_weak_undefined(&sym.name);
            } else {
                // Undefined symbol reference
                if !self.table.contains_key(&sym.name) {
                    self.undefined.insert(sym.name.clone());
                }
            }
        }

        // Check relocations for symbol references
        let local_sym_names: HashSet<&str> = obj
            .symbols
            .iter()
            .filter(|s| s.is_defined)
            .map(|s| s.name.as_str())
            .collect();
        for sec in &obj.sections {
            for r in &sec.relocations {
                if !self.table.contains_key(&r.symbol_name)
                    && !local_sym_names.contains(r.symbol_name.as_str())
                {
                    self.undefined.insert(r.symbol_name.clone());
                }
            }
        }

        Ok(())
    }

    pub fn lookup(&self, name: &str) -> Option<&ResolvedSymbol> {
        self.table.get(name)
    }

    /// Bind an undefined weak symbol to address 0.
    ///
    /// This is the ABI-defined value of a weak reference: ELF encodes it as
    /// `SHN_UNDEF` with `STB_WEAK`, COFF as `IMAGE_SYM_CLASS_WEAK_EXTERNAL`
    /// with section number 0. The link must not fail, but the binding is
    /// recorded separately so layout can diagnose only retained relocations
    /// that actually use the weak symbol.
    fn bind_weak_undefined(&mut self, name: &str) {
        if self.table.contains_key(name) {
            return;
        }
        self.undefined.remove(name);
        self.table.insert(
            name.to_string(),
            ResolvedSymbol {
                symbol: Symbol {
                    name: name.to_string(),
                    binding: SymbolBinding::Weak,
                    visibility: SymbolVisibility::Hidden,
                    sym_type: SymbolType::Function,
                    section_index: None,
                    value: 0,
                    size: 0,
                    is_defined: true,
                    is_imported: false,
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
        self.weak_undefined_symbols.insert(name.to_string());
    }
}
