//! The WASM link plan: the single place where type, import, function, data and
//! export numbering is decided, so that relocation application and module
//! emission cannot disagree about them.
//!
//! Native formats decide these things while assigning virtual addresses and
//! patching words. WASM has no virtual addresses: its "addresses" are indices
//! into the type/function/global/table index spaces and offsets into linear
//! memory. Those indices are only final once every import and every function is
//! known, so the plan is built here from the parsed objects, and the writer
//! emits exactly what the plan says.
//!
//! The plan is built from `ObjectFile`s rather than merged sections on purpose.
//! Merging keeps `.text` payloads and concatenated relocations, which is not a
//! valid CODE section, and it folds `.wasm.type`/`.wasm.function` into `.rodata`,
//! where their bytes are meaningless. Each object's signatures, imports, body
//! boundaries and data segment payloads are captured while reading it (see
//! [`crate::wasm::object_info::WasmObjectInfo`]); the plan renumbers them.

use std::collections::HashMap;

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::section::{SectionKind, align_to};
use crate::symbol::{Symbol, SymbolType};
use crate::wasm::object_info::{WasmExportKind, WasmFuncType, WasmImportDesc};

/// Linear-memory page size, fixed by the WASM spec.
pub const WASM_PAGE_SIZE: u64 = 65_536;

/// Stack reserve placed above static data. The compiler's WASM backend and
/// clang's both expect a `__stack_pointer` global; without one, locals that
/// spill would write to offset 0 and clobber static data.
pub const DEFAULT_STACK_SIZE: u64 = 64 * 1024;

/// Lowest linear-memory offset the linker gives static data. Zero is left free
/// so that a null pointer dereference traps instead of reading real data.
pub const DATA_BASE: u64 = 1024;

/// Static data is placed on this boundary, matching the widest scalar the
/// backend emits into `.data`.
const DATA_ALIGNMENT: u64 = 8;

/// `i32` value type, used for the stack pointer global and its initializer.
pub const I32: u8 = 0x7F;

/// Symbols the toolchain synthesizes rather than imports as host objects.
pub const MEMORY_SYMBOL: &str = "__linear_memory";
pub const STACK_POINTER_SYMBOL: &str = "__stack_pointer";
pub const TABLE_SYMBOL: &str = "__indirect_function_table";

/// Name the memory is exported under, which is what hosts instantiate against.
pub const MEMORY_EXPORT: &str = "memory";

/// A function imported from the host. Only functions are importable today, so
/// `imports.len()` is also the size of the imported function index prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmImportPlan {
    pub module: String,
    pub name: String,
    /// Index into [`WasmLinkInfo::types`].
    pub type_index: u32,
}

/// One module-defined function body, with the object it comes from so the
/// writer can slice the body out verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFunctionPlan {
    /// `None` for a body no defined symbol names. Such a function is still
    /// emitted (it may be reachable only through the table) but cannot be
    /// exported or used as the entry point.
    pub name: Option<String>,
    /// Whether [`Self::name`] may be referenced by another input. A `static` C
    /// function is local to its object, so an undefined reference to the same
    /// name elsewhere must not bind to it.
    pub linkable: bool,
    pub type_index: u32,
    pub object_index: usize,
    pub code_section_index: usize,
    /// First body byte after the size LEB, in `object.sections[code].data`.
    pub body_offset: u64,
    /// One past the last body byte.
    pub body_end: u64,
}

/// One active data segment, placed at a fixed linear-memory offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmDataSegmentPlan {
    pub object_index: usize,
    pub data_section_index: usize,
    /// Payload location within `object.sections[data].data`.
    pub payload_offset: u64,
    pub payload_len: u64,
    /// Linear-memory offset the segment initializes.
    pub memory_offset: u64,
}

/// A function the host can call, under the name the host knows it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmExportPlan {
    pub name: String,
    /// Final index in the module's function index space.
    pub index: u32,
}

/// A function body one input defines, recorded under the name another input
/// references it by.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WasmFunctionDefinition {
    object_index: usize,
    /// First body byte of the defining body, in its object's CODE payload.
    body_offset: u64,
    /// Signature the definition provides, checked against every reference.
    signature: WasmFuncType,
    /// `true` when only a weak symbol defines it, so a strong definition found
    /// later replaces it.
    weak: bool,
}

/// A data object one input defines, placed by its defining object.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WasmDataDefinition {
    object_index: usize,
    /// Offset of the symbol within its object's `.data` payload.
    section_offset: u64,
    weak: bool,
}

/// What the inputs define for each other.
///
/// A relocatable WASM object cannot say "call the `greet` another object
/// defines": it emits an import plus a relocation against it, byte-for-byte the
/// way it refers to a host function. Only here, where every input is visible at
/// once, can the two be told apart — so this is the table that decides whether a
/// name belongs in the module's import section or in its own function index
/// space.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WasmDefinitions {
    functions: HashMap<String, WasmFunctionDefinition>,
    data: HashMap<String, WasmDataDefinition>,
}

impl WasmDefinitions {
    fn collect(
        objects: &[ObjectFile],
        infos: &[&crate::wasm::object_info::WasmObjectInfo],
    ) -> LinkResult<Self> {
        let mut definitions = Self::default();
        for (object_index, (object, info)) in objects.iter().zip(infos).enumerate() {
            let code_section = object
                .sections
                .iter()
                .position(|section| section.kind == SectionKind::Text);
            let data_section = object
                .sections
                .iter()
                .position(|section| section.kind == SectionKind::Data && section.name == ".data");

            for symbol in object
                .symbols
                .iter()
                .filter(|symbol| symbol.is_defined && !symbol.is_local() && !symbol.name.is_empty())
            {
                if symbol.sym_type == SymbolType::Function && symbol.section_index == code_section {
                    let definition = WasmFunctionDefinition {
                        object_index,
                        body_offset: symbol.value,
                        signature: function_signature(object, info, symbol)?,
                        weak: symbol.is_weak(),
                    };
                    let clash = definitions.functions.get(&symbol.name).map(|incumbent| {
                        (
                            collide(incumbent.weak, definition.weak),
                            incumbent.object_index,
                        )
                    });
                    match clash {
                        Some((Collision::Keep, _)) => {}
                        Some((Collision::Conflict, incumbent)) => {
                            return Err(duplicate_definition(
                                objects,
                                incumbent,
                                object_index,
                                &symbol.name,
                            ));
                        }
                        _ => {
                            definitions
                                .functions
                                .insert(symbol.name.clone(), definition);
                        }
                    }
                } else if symbol.sym_type == SymbolType::Object
                    && symbol.section_index == data_section
                {
                    let definition = WasmDataDefinition {
                        object_index,
                        section_offset: symbol.value,
                        weak: symbol.is_weak(),
                    };
                    let clash = definitions.data.get(&symbol.name).map(|incumbent| {
                        (
                            collide(incumbent.weak, definition.weak),
                            incumbent.object_index,
                        )
                    });
                    match clash {
                        Some((Collision::Keep, _)) => {}
                        Some((Collision::Conflict, incumbent)) => {
                            return Err(duplicate_definition(
                                objects,
                                incumbent,
                                object_index,
                                &symbol.name,
                            ));
                        }
                        _ => {
                            definitions.data.insert(symbol.name.clone(), definition);
                        }
                    }
                }
            }
        }
        Ok(definitions)
    }

    /// Whether `object_index`'s body at `body_offset` is the one a reference to
    /// `name` binds to.
    fn defines_body(&self, object_index: usize, body_offset: u64) -> bool {
        self.functions.values().any(|definition| {
            definition.object_index == object_index && definition.body_offset == body_offset
        })
    }
}

/// What two inputs defining the same name means for the definition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Collision {
    /// The newcomer replaces the incumbent, which is only weakly defined.
    Replace,
    /// The incumbent, a strong definition, stands.
    Keep,
    /// Two strong definitions of one name.
    Conflict,
}

fn collide(incumbent_weak: bool, newcomer_weak: bool) -> Collision {
    if incumbent_weak {
        Collision::Replace
    } else if newcomer_weak {
        Collision::Keep
    } else {
        Collision::Conflict
    }
}

fn duplicate_definition(
    objects: &[ObjectFile],
    incumbent: usize,
    newcomer: usize,
    name: &str,
) -> LinkError {
    LinkError::new(
        ErrorCode::DuplicateSymbol,
        format!(
            "`{}` is defined by both `{}` and `{}`; a WASM call binds by name, so the link cannot \
             say which definition runs",
            name,
            objects[incumbent].path.display(),
            objects[newcomer].path.display()
        ),
    )
    .with_suggestion("rename one of the definitions, or drop the duplicate input")
}

/// The signature the body named by `symbol` actually has.
fn function_signature(
    object: &ObjectFile,
    info: &crate::wasm::object_info::WasmObjectInfo,
    symbol: &Symbol,
) -> LinkResult<WasmFuncType> {
    let body_index = info
        .body_offsets
        .iter()
        .position(|offset| *offset == symbol.value)
        .ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "WASM object `{}` defines function `{}` at code offset 0x{:x}, which is not \
                     the start of any function body",
                    object.path.display(),
                    symbol.name,
                    symbol.value
                ),
            )
        })?;
    let type_index = *info.function_types.get(body_index).ok_or_else(|| {
        LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "WASM object `{}` defines function `{}` as body {}, but it has {} bodies",
                object.path.display(),
                symbol.name,
                body_index,
                info.function_types.len()
            ),
        )
    })?;
    info.func_type(type_index).cloned().ok_or_else(|| {
        LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "WASM object `{}` gives function `{}` type {}, but declares {} types",
                object.path.display(),
                symbol.name,
                type_index,
                info.types.len()
            ),
        )
    })
}

/// Place every address-taken function into the module's one table.
///
/// Each object lays out its own element segment for a table of its own, and an
/// entry naming a function of another object appears as the object's import
/// index. The plan re-bases all of them into one numbering — slot 0 stays free
/// so calling a null function pointer traps — and resolves a cross-object entry
/// through the name its import declares.
fn build_table_entries(
    objects: &[ObjectFile],
    infos: &[&crate::wasm::object_info::WasmObjectInfo],
    functions: &[WasmFunctionPlan],
    definitions: &WasmDefinitions,
    import_count: usize,
) -> LinkResult<Vec<u32>> {
    let mut entries: Vec<u32> = Vec::new();

    for (object_index, (object, info)) in objects.iter().zip(infos).enumerate() {
        let function_imports: Vec<&WasmImportDesc> = info
            .imports
            .iter()
            .filter(|import| matches!(import, WasmImportDesc::Function { .. }))
            .collect();

        for segment in &info.element_segments {
            for &local in &segment.functions {
                let (definition_object, body_offset) = if (local as usize) < function_imports.len()
                {
                    let WasmImportDesc::Function { name, .. } = function_imports[local as usize]
                    else {
                        unreachable!("filtered to function imports above");
                    };
                    let definition = definitions.functions.get(name.as_str()).ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::UndefinedSymbol,
                            format!(
                                "WASM object `{}` puts `{}` in the function table, but \
                                             no input defines it",
                                object.path.display(),
                                name
                            ),
                        )
                        .with_suggestion(
                            "the table can only hold functions the link defines; a \
                                         host function has no body to place",
                        )
                    })?;
                    (definition.object_index, definition.body_offset)
                } else {
                    let body_index = local as usize - function_imports.len();
                    let body_offset = *info.body_offsets.get(body_index).ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "WASM object `{}` puts function {} in the table, but \
                                         defines {} bodies and imports {} functions",
                                object.path.display(),
                                local,
                                info.body_offsets.len(),
                                function_imports.len()
                            ),
                        )
                    })?;
                    (object_index, body_offset)
                };

                let index = functions
                    .iter()
                    .position(|function| {
                        function.object_index == definition_object
                            && function.body_offset == body_offset
                    })
                    .map(|position| position as u32 + import_count as u32)
                    .ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "a function table entry in `{}` names a body the link plan does \
                                 not contain",
                                object.path.display()
                            ),
                        )
                    })?;

                if !entries.contains(&index) {
                    entries.push(index);
                }
            }
        }
    }

    Ok(entries)
}

/// Everything the writer and the relocation stage must agree on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmLinkInfo {
    /// Type section entries, deduplicated by signature.
    pub types: Vec<WasmFuncType>,
    /// Imported functions in emission order. They occupy function indices
    /// `0..imports.len()`; module-defined functions follow.
    pub imports: Vec<WasmImportPlan>,
    /// Module-defined functions in emission order.
    pub functions: Vec<WasmFunctionPlan>,
    pub data_segments: Vec<WasmDataSegmentPlan>,
    /// Functions to export, in emission order.
    pub exports: Vec<WasmExportPlan>,
    /// Minimum size of the function table. Zero means no table is emitted.
    pub table_min: u32,
    /// Functions in table-slot order. Slot 0 is left free so a null function
    /// pointer traps, so entry `i` belongs in slot `i + 1`.
    pub table_entries: Vec<u32>,
    /// Lowest linear-memory offset holding static data.
    pub data_start: u64,
    /// One past the last static-data byte.
    pub data_end: u64,
    /// Initial `__stack_pointer` value (stacks grow downwards).
    pub stack_top: u64,
    pub stack_size: u64,
    /// Declared initial memory size.
    pub initial_pages: u32,
    /// Whether any input used bulk-memory instructions and therefore needs a
    /// data count section.
    pub data_count_required: bool,
    /// What the inputs define for each other, which is what separates a
    /// cross-object reference from a real host import.
    definitions: WasmDefinitions,
}

impl WasmLinkInfo {
    /// Build the plan from the objects that will be linked.
    ///
    /// `requested_start` is the configured entry point: it becomes the `start`
    /// section only if the module actually defines it, because a start index
    /// pointing at a function that does not exist makes the module invalid.
    ///
    /// Every capability the writer cannot honour is a loud error rather than a
    /// silently dropped section: a module that parses but miscomputes is worse
    /// than one that fails to link.
    pub fn build(
        objects: &[ObjectFile],
        requested_start: Option<&str>,
        requested_exports: &[String],
        export_all: bool,
    ) -> LinkResult<WasmLinkInfo> {
        if objects.is_empty() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "no input objects were provided to the WebAssembly linker",
            ));
        }

        let infos = objects
            .iter()
            .map(object_info)
            .collect::<LinkResult<Vec<_>>>()?;

        for object in objects {
            reject_unsupported_sections(object)?;
        }
        let definitions = WasmDefinitions::collect(objects, &infos)?;

        let mut types = TypeTable::default();
        let mut imports = Vec::new();
        let mut memory_min = 0;
        let mut table_min = 0;
        let mut data_count_required = false;

        for (object, info) in objects.iter().zip(&infos) {
            for import in &info.imports {
                match import {
                    WasmImportDesc::Function {
                        module,
                        name,
                        type_index,
                    } => {
                        let signature = info.func_type(*type_index).ok_or_else(|| {
                            LinkError::new(
                                ErrorCode::InvalidObject,
                                format!(
                                    "WASM object `{}` imports function `{}` with type {}, but \
                                     declares {} types",
                                    object.path.display(),
                                    name,
                                    type_index,
                                    info.types.len()
                                ),
                            )
                        })?;
                        if let Some(definition) = definitions.functions.get(name.as_str()) {
                            // Another input defines this name, so the object's
                            // `import` is really a call into the module: no host
                            // import is emitted, and the relocation stage binds
                            // the call to the defining body. A signature that
                            // disagrees would corrupt the operand stack at run
                            // time, long after the link looked clean.
                            if &definition.signature != signature {
                                return Err(LinkError::new(
                                    ErrorCode::AbiMismatch,
                                    format!(
                                        "WASM object `{}` calls `{}` as {}, but `{}` defines it \
                                         as {}",
                                        object.path.display(),
                                        name,
                                        signature_text(signature),
                                        objects[definition.object_index].path.display(),
                                        signature_text(&definition.signature)
                                    ),
                                ));
                            }
                            continue;
                        }
                        if imports.iter().any(|existing: &WasmImportPlan| {
                            existing.module == *module && existing.name == *name
                        }) {
                            continue;
                        }
                        imports.push(WasmImportPlan {
                            module: module.clone(),
                            name: name.clone(),
                            type_index: types.intern(signature),
                        });
                    }
                    WasmImportDesc::Memory { min, .. } => {
                        // The module defines its own memory: static data, stack
                        // and heap all live in one linear memory the linker
                        // sizes. An imported memory's minimum is a floor.
                        memory_min = memory_min.max(*min);
                    }
                    WasmImportDesc::Table { min, .. } => {
                        table_min = table_min.max(*min);
                    }
                    WasmImportDesc::Global { name, .. } if name == STACK_POINTER_SYMBOL => {
                        // Synthesized below; the host must not supply the stack.
                    }
                    WasmImportDesc::Global { module, name, .. } => {
                        return Err(unsupported(
                            format!(
                                "WASM object `{}` imports global `{}` from `{}`, which the \
                                 linker cannot synthesize",
                                object.path.display(),
                                name,
                                module
                            ),
                            "only `__stack_pointer` is defined by the linker; data its address \
                             space cannot reach must be linked into the module",
                        ));
                    }
                    WasmImportDesc::Tag { name, .. } => {
                        return Err(unsupported(
                            format!(
                                "WASM object `{}` imports exception tag `{}`, which the linker \
                                 does not support",
                                object.path.display(),
                                name
                            ),
                            "exception handling in the WASM linker is not implemented",
                        ));
                    }
                }
            }

            data_count_required |= object
                .sections
                .iter()
                .any(|section| section.name == ".wasm.datacount");
        }

        // A table is defined only when something needs one: an object that
        // imports the table, or code that takes a function's address. The
        // latter carries a relocation, which the guard above rejects, so only
        // the import case remains.
        // A table is defined only when something needs one: an object that
        // imports the table, or code that takes a function's address. The
        // latter carries a relocation, which the guard above rejects, so only
        // the import case remains.
        let mut table_min = if table_min == 0
            && objects
                .iter()
                .any(|object| object.symbols.iter().any(|s| s.name == TABLE_SYMBOL))
        {
            1
        } else {
            table_min
        };

        let mut functions = Vec::new();
        for (object_index, (object, info)) in objects.iter().zip(&infos).enumerate() {
            let code_section_index = object
                .sections
                .iter()
                .position(|section| section.kind == SectionKind::Text);
            let Some(code_section_index) = code_section_index else {
                if info.function_types.is_empty() {
                    continue;
                }
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM object `{}` declares {} function types but has no code section",
                        object.path.display(),
                        info.function_types.len()
                    ),
                ));
            };

            for (body_index, type_index) in info.function_types.iter().enumerate() {
                let signature = info.func_type(*type_index).ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "WASM object `{}` gives function body {} type {}, but declares {} \
                             types",
                            object.path.display(),
                            body_index,
                            type_index,
                            info.types.len()
                        ),
                    )
                })?;
                let (body_offset, body_end) = match (
                    info.body_offsets.get(body_index),
                    info.body_ends.get(body_index),
                ) {
                    (Some(start), Some(end)) => (*start, *end),
                    _ => {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "WASM object `{}` gives function body {} no code range",
                                object.path.display(),
                                body_index
                            ),
                        ));
                    }
                };
                functions.push(WasmFunctionPlan {
                    name: defined_function_name(object, code_section_index, body_offset),
                    linkable: definitions.defines_body(object_index, body_offset),
                    type_index: types.intern(signature),
                    object_index,
                    code_section_index,
                    body_offset,
                    body_end,
                });
            }
        }

        let table_entries =
            build_table_entries(objects, &infos, &functions, &definitions, imports.len())?;
        if !table_entries.is_empty() {
            table_min = table_min.max(table_entries.len() as u32 + 1);
        }

        let mut data_segments = Vec::new();
        let mut cursor = DATA_BASE;
        for (object_index, (object, info)) in objects.iter().zip(&infos).enumerate() {
            let Some(data_section_index) = object
                .sections
                .iter()
                .position(|section| section.kind == SectionKind::Data && section.name == ".data")
            else {
                if info.data_segments.is_empty() {
                    continue;
                }
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM object `{}` declares {} data segments but has no data section",
                        object.path.display(),
                        info.data_segments.len()
                    ),
                ));
            };

            for segment in &info.data_segments {
                cursor = align_to(cursor, DATA_ALIGNMENT);
                data_segments.push(WasmDataSegmentPlan {
                    object_index,
                    data_section_index,
                    payload_offset: segment.payload_offset,
                    payload_len: segment.payload_len,
                    memory_offset: cursor,
                });
                cursor += segment.payload_len;
            }
        }

        let (data_start, data_end) = if data_segments.is_empty() {
            (0, 0)
        } else {
            (DATA_BASE, cursor)
        };

        let stack_size = DEFAULT_STACK_SIZE;
        let stack_top = align_to(data_end, 16) + stack_size;
        let initial_pages = stack_top
            .max(memory_min as u64 * WASM_PAGE_SIZE)
            .div_ceil(WASM_PAGE_SIZE)
            .max(1) as u32;

        let mut info = WasmLinkInfo {
            types: types.types,
            imports,
            functions,
            data_segments,
            exports: Vec::new(),
            table_min,
            table_entries,
            data_start,
            data_end,
            stack_top,
            stack_size,
            initial_pages,
            data_count_required,
            definitions,
        };
        info.exports = collect_exports(&info, objects, requested_exports, export_all);
        add_object_exports(&mut info, objects, &infos)?;
        // The entry point is exported, not started: wasm-ld does the same, and a
        // start section would run it again when the host also calls `_start`.
        // An object that already exports the entry (under any name) keeps that
        // name; otherwise the symbol name becomes the export name.
        if let Some(name) = requested_start {
            if let Some(index) = info.function_index(name) {
                let position = (index - info.imports.len() as u32) as usize;
                let signature = &info.types[info.functions[position].type_index as usize];
                // A host calls the entry with no arguments and ignores any
                // result, so a different signature would corrupt the stack.
                if !signature.params.is_empty() || !signature.results.is_empty() {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "the WASM entry point `{}` must take no arguments and return \
                             nothing, but it has {} parameter(s) and {} result(s)",
                            name,
                            signature.params.len(),
                            signature.results.len()
                        ),
                    )
                    .with_suggestion(
                        "choose an entry symbol with type `() -> ()`, or wrap it in a stub that \
                         has that signature",
                    ));
                }
                if !info.exports.iter().any(|export| export.index == index) {
                    info.exports.push(WasmExportPlan {
                        name: name.to_string(),
                        index,
                    });
                }
            }
        }

        Ok(info)
    }

    /// Final function index of a defined function, or `None` when the module
    /// does not define it. Imported functions occupy the indices before these.
    pub fn function_index(&self, name: &str) -> Option<u32> {
        self.functions
            .iter()
            .position(|function| function.name.as_deref() == Some(name))
            .map(|index| self.imports.len() as u32 + index as u32)
    }

    /// Number of functions in the module's function index space.
    pub fn function_count(&self) -> u32 {
        self.imports.len() as u32 + self.functions.len() as u32
    }

    /// Final function index of an imported function, identified by the
    /// `(module, name)` pair its object imports it as.
    pub fn imported_function_index(&self, module: &str, name: &str) -> Option<u32> {
        self.imports
            .iter()
            .position(|import| import.module == module && import.name == name)
            .map(|index| index as u32)
    }

    /// Final function index of the body that a reference to `name` binds to,
    /// when some input defines that name. The plan drops the import such a
    /// reference is written as, so this is the only way back to its target.
    pub fn local_function_index(&self, name: &str) -> Option<u32> {
        let definition = self.definitions.functions.get(name)?;
        self.functions
            .iter()
            .position(|function| {
                function.object_index == definition.object_index
                    && function.body_offset == definition.body_offset
            })
            .map(|position| self.imports.len() as u32 + position as u32)
    }

    /// Final linear-memory address of the data some input defines as `name`.
    pub fn local_data_address(&self, name: &str) -> Option<u32> {
        let definition = self.definitions.data.get(name)?;
        self.data_address(definition.object_index, definition.section_offset)
    }

    /// Whether `name` is defined by an input rather than imported from the host.
    pub fn defines_function(&self, name: &str) -> bool {
        self.definitions.functions.contains_key(name)
    }

    /// Final function index of the defined body that starts at `body_offset` in
    /// `object_index`'s code section.
    pub fn defined_function_index(&self, object_index: usize, body_offset: u64) -> Option<u32> {
        self.functions
            .iter()
            .position(|function| {
                function.object_index == object_index && function.body_offset == body_offset
            })
            .map(|index| self.imports.len() as u32 + index as u32)
    }

    /// The single global the writer emits, which relocations to
    /// `__stack_pointer` must reference.
    pub fn stack_pointer_global_index(&self) -> u32 {
        0
    }

    /// Table slot of the body that starts at `body_offset` in `object_index`'s
    /// code section, or `None` when no input takes its address.
    pub fn table_slot(&self, object_index: usize, body_offset: u64) -> Option<u32> {
        self.table_slot_of_function(self.defined_function_index(object_index, body_offset)?)
    }

    /// Table slot a reference to `name` resolves to. Slot 0 is never assigned:
    /// it stays a trap for a null function pointer.
    pub fn table_slot_for_name(&self, name: &str) -> Option<u32> {
        let definition = self.definitions.functions.get(name)?;
        self.table_slot(definition.object_index, definition.body_offset)
    }

    /// Slot holding the function at final index `index`.
    pub fn table_slot_of_function(&self, index: u32) -> Option<u32> {
        self.table_entries
            .iter()
            .position(|entry| *entry == index)
            .map(|position| position as u32 + 1)
    }

    /// Final type index of the function at final index `index`.
    pub fn function_type_index(&self, index: u32) -> Option<u32> {
        if let Some(import) = self.imports.get(index as usize) {
            Some(import.type_index)
        } else {
            self.functions
                .get(index as usize - self.imports.len())
                .map(|function| function.type_index)
        }
    }

    /// Final linear-memory address of a data symbol at `section_offset` within
    /// `object_index`'s `.data` section payload.
    pub fn data_address(&self, object_index: usize, section_offset: u64) -> Option<u32> {
        self.data_segments
            .iter()
            .find(|segment| {
                segment.object_index == object_index
                    && section_offset >= segment.payload_offset
                    && section_offset < segment.payload_offset + segment.payload_len
            })
            .map(|segment| {
                let within = section_offset - segment.payload_offset;
                (segment.memory_offset + within) as u32
            })
    }
}

fn object_info(object: &ObjectFile) -> LinkResult<&crate::wasm::object_info::WasmObjectInfo> {
    object.wasm.as_ref().ok_or_else(|| {
        unsupported(
            format!(
                "input `{}` is not a relocatable WebAssembly object: it carries no type, import \
                 or code-layout facts",
                object.path.display()
            ),
            "only relocatable WASM objects (those with a `linking` custom section) can be linked",
        )
    })
}

fn unsupported(message: String, suggestion: &str) -> LinkError {
    LinkError::new(ErrorCode::UnsupportedFormat, message).with_suggestion(suggestion)
}

fn signature_text(signature: &WasmFuncType) -> String {
    let types = |list: &[u8]| -> String {
        list.iter()
            .map(|val_type| val_type_text(*val_type))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "({}) -> ({})",
        types(&signature.params),
        types(&signature.results)
    )
}

fn val_type_text(val_type: u8) -> String {
    match val_type {
        0x7F => "i32".to_string(),
        0x7E => "i64".to_string(),
        0x7D => "f32".to_string(),
        0x7C => "f64".to_string(),
        0x7B => "v128".to_string(),
        0x70 => "funcref".to_string(),
        0x6F => "externref".to_string(),
        other => format!("unknown(0x{other:02X})"),
    }
}

/// Sections the writer cannot re-emit yet. Each one holds content that has no
/// representation in the plan, so emitting the module anyway would drop it.
fn reject_unsupported_sections(object: &ObjectFile) -> LinkResult<()> {
    const UNSUPPORTED: &[(&str, &str)] = &[
        (".wasm.global", "defined globals"),
        (".wasm.table", "defined tables"),
        (".wasm.start", "start sections"),
    ];

    for section in &object.sections {
        if section.data.is_empty() {
            continue;
        }
        if let Some((_, what)) = UNSUPPORTED.iter().find(|(name, _)| *name == section.name) {
            return Err(unsupported(
                format!(
                    "WASM object `{}` contains {}, which the linker cannot emit",
                    object.path.display(),
                    what
                ),
                "function tables, element segments and globals must be produced by the linker \
                 that defines them; only code, data, function imports and function exports are \
                 supported",
            ));
        }
        if section.name == ".wasm.memory" {
            return Err(unsupported(
                format!(
                    "WASM object `{}` defines its own linear memory",
                    object.path.display()
                ),
                "the linked module defines one linear memory sized for static data, stack and \
                 heap; objects must import it instead of defining it",
            ));
        }
    }
    Ok(())
}

/// Name of the defined function symbol covering `body_offset` in the object's
/// code section. Prefers a name other than a local `.L` label so that an
/// exported name wins over a compiler-generated one.
fn defined_function_name(
    object: &ObjectFile,
    code_section_index: usize,
    body_offset: u64,
) -> Option<String> {
    let covering = |symbol: &&Symbol| {
        symbol.is_defined
            && symbol.sym_type == SymbolType::Function
            && symbol.section_index == Some(code_section_index)
            && symbol.value == body_offset
            && !symbol.name.is_empty()
    };

    object
        .symbols
        .iter()
        .filter(covering)
        .find(|symbol| !symbol.is_local())
        .or_else(|| object.symbols.iter().find(covering))
        .map(|symbol| symbol.name.clone())
}

/// Exports in emitted-function order, so the export section is deterministic.
fn collect_exports(
    info: &WasmLinkInfo,
    objects: &[ObjectFile],
    requested_exports: &[String],
    export_all: bool,
) -> Vec<WasmExportPlan> {
    let mut exports: Vec<WasmExportPlan> = Vec::new();
    let import_count = info.imports.len() as u32;

    for (position, function) in info.functions.iter().enumerate() {
        let Some(name) = function.name.as_deref() else {
            continue;
        };
        let object = &objects[function.object_index];
        let wanted = export_all
            || requested_exports.iter().any(|requested| requested == name)
            || object.symbols.iter().any(|symbol| {
                symbol.is_defined && symbol.name == name && symbol.is_exported && !symbol.is_local()
            });
        if wanted && !exports.iter().any(|export| export.name == name) {
            exports.push(WasmExportPlan {
                name: name.to_string(),
                index: import_count + position as u32,
            });
        }
    }

    exports
}

/// Add the exports the input objects asked for by name. An object's export
/// section is the only place a name like `export_name("_start")` exists: the
/// symbol table keeps the C name, so the flag-driven path above cannot see it.
fn add_object_exports(
    info: &mut WasmLinkInfo,
    objects: &[ObjectFile],
    infos: &[&crate::wasm::object_info::WasmObjectInfo],
) -> LinkResult<()> {
    for (object_index, (object, facts)) in objects.iter().zip(infos).enumerate() {
        for export in &facts.exports {
            if export.kind == WasmExportKind::Memory {
                // The writer always exports linear memory as `memory`.
                if export.name == MEMORY_EXPORT {
                    continue;
                }
                return Err(unsupported(
                    format!(
                        "WASM object `{}` exports its memory as `{}`, but the linker exports it \
                         as `{}`",
                        object.path.display(),
                        export.name,
                        MEMORY_EXPORT
                    ),
                    "linear memory has one canonical export name; drop the extra export or use \
                     `export_name(\"memory\")`",
                ));
            }
            if export.kind != WasmExportKind::Function {
                return Err(unsupported(
                    format!(
                        "WASM object `{}` exports {} `{}`, which the linker cannot emit",
                        object.path.display(),
                        export_kind_name(export.kind),
                        export.name
                    ),
                    "only function exports are supported; tables and globals are produced by the \
                     linker that defines them",
                ));
            }

            let index = final_function_index(facts, info, object_index, export.index)?;

            // An export section entry *names* an export; the symbol flag only
            // says a function is exported. So when the flag already produced an
            // entry for this function, the section renames it (`export_name`),
            // exactly as lld does.
            if let Some(existing) = info.exports.iter().position(|plan| plan.index == index) {
                if info.exports[existing].name == export.name {
                    continue;
                }
                if let Some(other) = info
                    .exports
                    .iter()
                    .find(|plan| plan.name == export.name && plan.index != index)
                {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "WASM object `{}` exports function {} as `{}`, but that name already \
                             exports function {}",
                            object.path.display(),
                            index,
                            export.name,
                            other.index
                        ),
                    ));
                }
                info.exports[existing].name = export.name.clone();
                continue;
            }

            if info.exports.iter().any(|plan| plan.name == export.name) {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM object `{}` exports `{}` twice, to different functions",
                        object.path.display(),
                        export.name
                    ),
                ));
            }
            info.exports.push(WasmExportPlan {
                name: export.name.clone(),
                index,
            });
        }
    }
    Ok(())
}

/// Map an object-local function index onto the module's function index space.
/// Imported functions come first in both, but the plan deduplicates imports
/// across objects, so the ordinal is looked up rather than assumed.
fn final_function_index(
    facts: &crate::wasm::object_info::WasmObjectInfo,
    info: &WasmLinkInfo,
    object_index: usize,
    local_index: u32,
) -> LinkResult<u32> {
    let imported = facts
        .imports
        .iter()
        .filter(|import| matches!(import, WasmImportDesc::Function { .. }))
        .count() as u32;

    if local_index < imported {
        let WasmImportDesc::Function { module, name, .. } = facts
            .imports
            .iter()
            .filter(|import| matches!(import, WasmImportDesc::Function { .. }))
            .nth(local_index as usize)
            .expect("local_index is below the function-import count")
        else {
            unreachable!("filtered to function imports above");
        };
        return info.imported_function_index(module, name).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "WASM export names imported function `{}::{}`, which is missing from the \
                         link plan",
                    module, name
                ),
            )
        });
    }

    let ordinal = (local_index - imported) as usize;
    let mut seen = 0usize;
    for (position, function) in info.functions.iter().enumerate() {
        if function.object_index != object_index {
            continue;
        }
        if seen == ordinal {
            return Ok(info.imports.len() as u32 + position as u32);
        }
        seen += 1;
    }

    Err(LinkError::new(
        ErrorCode::InvalidObject,
        format!(
            "WASM export names function {}, but the object defines only {} of them",
            local_index,
            facts.function_types.len()
        ),
    ))
}

fn export_kind_name(kind: WasmExportKind) -> &'static str {
    match kind {
        WasmExportKind::Function => "function",
        WasmExportKind::Table => "table",
        WasmExportKind::Memory => "memory",
        WasmExportKind::Global => "global",
        WasmExportKind::Tag => "tag",
    }
}

/// Deduplicating type section builder.
#[derive(Default)]
struct TypeTable {
    types: Vec<WasmFuncType>,
    index: HashMap<(Vec<u8>, Vec<u8>), u32>,
}

impl TypeTable {
    fn intern(&mut self, signature: &WasmFuncType) -> u32 {
        let key = (signature.params.clone(), signature.results.clone());
        if let Some(index) = self.index.get(&key) {
            return *index;
        }
        let index = self.types.len() as u32;
        self.types.push(signature.clone());
        self.index.insert(key, index);
        index
    }
}
