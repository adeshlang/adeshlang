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
    /// Symbols routed to a system shared library (`DllImport`). These become
    /// dynamic imports (PLT/GOT + undefined `.dynsym` entries) in dynamic
    /// links, or a loud link error in static links. The routed name is the
    /// symbol name as it appears in the input objects (the C name for normal
    /// ELF inputs).
    pub dynamic_imports: Vec<String>,
    /// Routed dynamic imports that are *data objects* (no function stub may
    /// be synthesized; address-of-imported-data is unsupported).
    pub dynamic_data_imports: HashSet<String>,
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
    fn macho_symbol_aliases(name: &str, target: &crate::target::Target) -> Vec<String> {
        if target.format != crate::target::ObjectFormat::MachO {
            return vec![name.to_string()];
        }
        if let Some(stripped) = name.strip_prefix('_') {
            if !stripped.is_empty() {
                return vec![name.to_string(), stripped.to_string()];
            }
        }
        vec![name.to_string(), format!("_{}", name)]
    }

    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
            undefined: HashSet::new(),
            weak_undefined_symbols: HashSet::new(),
            dynamic_imports: Vec::new(),
            dynamic_data_imports: HashSet::new(),
            policy: UndefinedSymbolPolicy::Error,
            warnings: Vec::new(),
        }
    }

    pub fn with_policy(policy: UndefinedSymbolPolicy) -> Self {
        Self {
            table: HashMap::new(),
            undefined: HashSet::new(),
            weak_undefined_symbols: HashSet::new(),
            dynamic_imports: Vec::new(),
            dynamic_data_imports: HashSet::new(),
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
            self.ingest_object(obj, target)?;
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
                    if undef_sym.contains("rust_alloc") {
                        eprintln!("DEBUG_LINK: undef_sym={}", undef_sym);
                        eprintln!("DEBUG_LINK: exact_get={:?}", ar.symbol_index.get(&undef_sym));
                        for (k, v) in &ar.symbol_index {
                            if k.contains("rust_alloc") {
                                eprintln!("DEBUG_LINK: ar_key={} val={:?}", k, v);
                            }
                        }
                    }
                    let candidates = ar.symbol_index.get(&undef_sym);
                    let prefixed;
                    let unprefixed;
                    let candidates = if candidates.is_some() {
                        candidates
                    } else if target.format == crate::target::ObjectFormat::MachO {
                        if let Some(stripped) = undef_sym.strip_prefix('_') {
                            unprefixed = format!("_{}", stripped);
                            ar.symbol_index
                                .get(stripped)
                                .or_else(|| ar.symbol_index.get(&unprefixed))
                        } else {
                            prefixed = format!("_{}", undef_sym);
                            ar.symbol_index
                                .get(&prefixed)
                                .or_else(|| ar.symbol_index.get(undef_sym.as_str()))
                        }
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
                                    target,
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
                            self.ingest_object(&member_obj, target)?;
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
                    // Dead Rust/Adesh internal — bind as weak undefined (NULL)
                    self.bind_weak_undefined(&undef);
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
                SymbolRoute::DllImport {
                    dll,
                    ref name,
                    is_data,
                } => {
                    // System shared-library import — mark as imported so the
                    // emitter puts it in the dynamic import set. Data objects
                    // are recorded as `Object` so no code stub is ever
                    // synthesized for them and the layout stage can reject
                    // absolute (address-of) references loudly.
                    self.undefined.remove(&undef);
                    let sym_name = name.clone();
                    if !self.dynamic_imports.contains(&undef) {
                        self.dynamic_imports.push(undef.clone());
                    }
                    if is_data {
                        self.dynamic_data_imports.insert(undef.clone());
                    }
                    self.table.insert(
                        undef.clone(),
                        ResolvedSymbol {
                            symbol: Symbol {
                                name: undef.clone(),
                                binding: SymbolBinding::Global,
                                visibility: crate::symbol::SymbolVisibility::Default,
                                sym_type: if is_data {
                                    crate::symbol::SymbolType::Object
                                } else {
                                    crate::symbol::SymbolType::Function
                                },
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
                    // Data imports have no function stub; nothing else to
                    // register for them beyond the primary symbol record.
                    if is_data {
                        continue;
                    }
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

            // Report every undefined symbol, not an arbitrary first pick.
            // The undefined set iterates in nondeterministic hash order, so
            // a first-pick report hides the full repair surface behind
            // whichever symbol the set yields first and forces one
            // fix-compile cycle per missing symbol.
            let mut missing: Vec<String> = self.undefined.iter().cloned().collect();
            missing.sort();
            missing.dedup();

            // First referencing relocation per symbol (for the notes).
            let mut first_ref: std::collections::HashMap<&str, (&str, u64)> =
                std::collections::HashMap::new();
            for obj in objects.iter() {
                let obj_name = obj.path.to_str().unwrap_or("input object");
                for sec in &obj.sections {
                    for r in &sec.relocations {
                        if self.undefined.contains(&r.symbol_name)
                            && !first_ref.contains_key(r.symbol_name.as_str())
                        {
                            first_ref.insert(r.symbol_name.as_str(), (obj_name, r.offset));
                        }
                    }
                }
            }

            let first_undef = missing.first().unwrap().clone();
            let (ref_file, ref_offset) = first_ref
                .get(first_undef.as_str())
                .cloned()
                .unwrap_or(("input object", 0));
            let mut err =
                LinkError::undefined_symbol(first_undef, ref_file, None, Some(ref_offset));
            if missing.len() > 1 {
                let rest: Vec<String> = missing[1..]
                    .iter()
                    .map(|name| match first_ref.get(name.as_str()) {
                        Some((file, offset)) => {
                            format!("`{}` (referenced in {} at 0x{:x})", name, file, offset)
                        }
                        None => format!("`{}`", name),
                    })
                    .collect();
                err = err.with_note(format!(
                    "also undefined ({}): {}",
                    missing.len() - 1,
                    rest.join(", ")
                ));
            }
            return Err(err);
        }

        Ok(())
    }

    fn ingest_object(
        &mut self,
        obj: &ObjectFile,
        target: &crate::target::Target,
    ) -> LinkResult<()> {
        for sym in &obj.symbols {
            if sym.is_local() {
                continue;
            }
            let aliases = Self::macho_symbol_aliases(&sym.name, target);

            if sym.is_defined {
                let existing_key = aliases
                    .iter()
                    .find(|k| self.table.contains_key(k.as_str()))
                    .cloned()
                    .unwrap_or_else(|| sym.name.clone());
                if let Some(existing) = self.table.get_mut(&existing_key) {
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
                            for alias in &aliases {
                                self.weak_undefined_symbols.remove(alias);
                            }
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
                        for alias in &aliases {
                            self.undefined.remove(alias);
                        }
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
                    for alias in &aliases {
                        self.undefined.remove(alias);
                    }
                }
                for alias in aliases {
                    self.table.entry(alias).or_insert_with(|| ResolvedSymbol {
                        symbol: sym.clone(),
                        defined_in_file_index: obj.file_index,
                        defined_in_sec_index: sym.section_index,
                        references: Vec::new(),
                    });
                }
            } else if sym.binding == SymbolBinding::Weak {
                // An undefined weak symbol has the ABI-defined value NULL in
                // ELF (`SHN_UNDEF` + `STB_WEAK`) and COFF
                // (`IMAGE_SYM_CLASS_WEAK_EXTERNAL`, section 0). It must bind
                // to address 0, never fail the link.
                self.bind_weak_undefined(&sym.name);
            } else {
                // Undefined symbol reference
                if !aliases.iter().any(|a| self.table.contains_key(a)) {
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
                let reloc_aliases = Self::macho_symbol_aliases(&r.symbol_name, target);
                if !reloc_aliases.iter().any(|a| self.table.contains_key(a))
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
