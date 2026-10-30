//! Tests for `WasmLinkInfo`, the plan the WASM writer and relocation stage
//! share. The plan renumbers types, functions and data segments out of the
//! parsed objects, so these tests pin exactly that numbering plus the loud
//! rejections for anything it cannot represent yet.

use adesh_linker::error::ErrorCode;
use adesh_linker::object::ObjectFile;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use adesh_linker::wasm::link_info::{
    DATA_BASE, DEFAULT_STACK_SIZE, STACK_POINTER_SYMBOL, WASM_PAGE_SIZE, WasmExportPlan,
    WasmLinkInfo,
};
use adesh_linker::wasm::object_info::{
    WasmDataSegmentInfo, WasmElementSegment, WasmFuncType, WasmImportDesc, WasmObjectInfo,
};
use std::path::PathBuf;

fn wasm_target() -> Target {
    Target::from_triple("wasm32-wasi").expect("wasm32-wasi")
}

/// `() -> i32`.
fn unit_to_i32() -> WasmFuncType {
    WasmFuncType {
        params: Vec::new(),
        results: vec![0x7F],
    }
}

/// `() -> ()`.
fn unit_to_unit() -> WasmFuncType {
    WasmFuncType {
        params: Vec::new(),
        results: Vec::new(),
    }
}

/// `(i32, i32) -> i32`.
fn two_i32_to_i32() -> WasmFuncType {
    WasmFuncType {
        params: vec![0x7F, 0x7F],
        results: vec![0x7F],
    }
}

/// `(i32) -> i32`.
fn i32_to_i32() -> WasmFuncType {
    WasmFuncType {
        params: vec![0x7F],
        results: vec![0x7F],
    }
}

/// Builds a parsed-object stand-in: a `.text` section whose bytes are the
/// concatenated bodies, plus the `WasmObjectInfo` the reader would have
/// captured for them.
struct ObjectBuilder {
    object: ObjectFile,
    code_index: usize,
}

impl ObjectBuilder {
    fn new(file_index: usize) -> Self {
        let mut object = ObjectFile::new(
            PathBuf::from(format!("obj{file_index}.o")),
            wasm_target(),
            file_index,
        );
        let code_index = object.add_section(Section::new_code(".text", Vec::new(), 16));
        object.wasm = Some(WasmObjectInfo::default());
        Self { object, code_index }
    }

    fn intern_type(&mut self, signature: &WasmFuncType) -> u32 {
        let info = self.object.wasm.as_mut().expect("wasm info");
        match info.types.iter().position(|existing| existing == signature) {
            Some(index) => index as u32,
            None => {
                info.types.push(signature.clone());
                (info.types.len() - 1) as u32
            }
        }
    }

    fn add_function(&mut self, name: Option<&str>, signature: &WasmFuncType, body: &[u8]) -> u64 {
        self.add_function_binding(name, SymbolBinding::Global, signature, body)
    }

    fn add_function_binding(
        &mut self,
        name: Option<&str>,
        binding: SymbolBinding,
        signature: &WasmFuncType,
        body: &[u8],
    ) -> u64 {
        let type_index = self.intern_type(signature);
        let section = &mut self.object.sections[self.code_index];
        let offset = section.data.len() as u64;
        section.data.extend_from_slice(body);
        let end = section.data.len() as u64;

        let info = self.object.wasm.as_mut().expect("wasm info");
        info.function_types.push(type_index);
        info.body_offsets.push(offset);
        info.body_ends.push(end);

        if let Some(name) = name {
            let symbol = Symbol::new_defined(
                name,
                binding,
                SymbolType::Function,
                self.code_index,
                offset,
                0,
                self.object.file_index,
            );
            self.object.add_symbol(symbol);
        }
        offset
    }

    fn export(&mut self, name: &str) -> &mut Self {
        let symbol = self
            .object
            .symbols
            .iter_mut()
            .find(|symbol| symbol.name == name)
            .expect("symbol to export");
        symbol.is_exported = true;
        self
    }

    fn add_import(&mut self, module: &str, name: &str, signature: &WasmFuncType) -> &mut Self {
        let type_index = self.intern_type(signature);
        self.object
            .wasm
            .as_mut()
            .expect("wasm info")
            .imports
            .push(WasmImportDesc::Function {
                module: module.to_string(),
                name: name.to_string(),
                type_index,
            });
        self
    }

    fn add_import_global(&mut self, name: &str, val_type: u8, mutable: bool) -> &mut Self {
        self.object
            .wasm
            .as_mut()
            .expect("wasm info")
            .imports
            .push(WasmImportDesc::Global {
                module: "env".to_string(),
                name: name.to_string(),
                val_type,
                mutable,
            });
        self
    }

    fn add_memory_import(&mut self, min: u32) -> &mut Self {
        self.object
            .wasm
            .as_mut()
            .expect("wasm info")
            .imports
            .push(WasmImportDesc::Memory {
                module: "env".to_string(),
                name: "__linear_memory".to_string(),
                min,
            });
        self
    }

    fn add_data_segment(&mut self, payload: &[u8]) -> &mut Self {
        let data_index = match self
            .object
            .sections
            .iter()
            .position(|section| section.name == ".data")
        {
            Some(index) => index,
            None => self
                .object
                .add_section(Section::new_data(".data", Vec::new(), true, 8)),
        };
        let section = &mut self.object.sections[data_index];
        let payload_offset = section.data.len() as u64;
        section.data.extend_from_slice(payload);
        self.object
            .wasm
            .as_mut()
            .expect("wasm info")
            .data_segments
            .push(WasmDataSegmentInfo {
                payload_offset,
                payload_len: payload.len() as u64,
            });
        self
    }

    fn add_relocation(&mut self, symbol_name: &str) -> &mut Self {
        self.object.sections[self.code_index]
            .relocations
            .push(Relocation {
                offset: 0,
                symbol_name: symbol_name.to_string(),
                symbol_index: None,
                file_index: None,
                kind: RelocationKind::WasmFunctionIndex,
                addend: 0,
            });
        self
    }

    fn add_element_segment(&mut self, functions: &[u32]) -> &mut Self {
        self.object
            .wasm
            .as_mut()
            .expect("wasm info")
            .element_segments
            .push(WasmElementSegment {
                table_offset: 1,
                functions: functions.to_vec(),
            });
        self
    }

    fn add_section(&mut self, section: Section) -> &mut Self {
        self.object.add_section(section);
        self
    }

    fn finish(self) -> ObjectFile {
        self.object
    }
}

fn body_i32(value: u8) -> Vec<u8> {
    // locals=0, i32.const value, end
    vec![0x00, 0x41, value, 0x0B]
}

#[test]
fn function_signatures_are_interned_once_per_signature() {
    let mut first = ObjectBuilder::new(0);
    first.add_function(Some("add"), &two_i32_to_i32(), &body_i32(7));
    let mut second = ObjectBuilder::new(1);
    second.add_function(Some("sub"), &two_i32_to_i32(), &body_i32(9));
    second.add_function(Some("main"), &unit_to_i32(), &body_i32(1));

    let info = WasmLinkInfo::build(&[first.finish(), second.finish()], None, &[], false)
        .expect("plan builds");

    assert_eq!(
        info.types,
        vec![two_i32_to_i32(), unit_to_i32()],
        "one type entry per distinct signature, in first-use order"
    );
    let type_indices: Vec<u32> = info.functions.iter().map(|f| f.type_index).collect();
    assert_eq!(type_indices, vec![0, 0, 1]);
    assert_eq!(info.function_index("add"), Some(0));
    assert_eq!(info.function_index("sub"), Some(1));
    assert_eq!(info.function_index("main"), Some(2));
    assert_eq!(info.function_count(), 3);
}

#[test]
fn imports_are_deduplicated_across_objects_and_keep_their_module() {
    let mut first = ObjectBuilder::new(0);
    first.add_import("wasi_snapshot_preview1", "fd_write", &i32_to_i32());
    first.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    let mut second = ObjectBuilder::new(1);
    second.add_import("wasi_snapshot_preview1", "fd_write", &i32_to_i32());

    let info = WasmLinkInfo::build(&[first.finish(), second.finish()], None, &[], false)
        .expect("plan builds");

    assert_eq!(info.imports.len(), 1, "one host import, not one per input");
    assert_eq!(info.imports[0].module, "wasi_snapshot_preview1");
    assert_eq!(info.imports[0].name, "fd_write");
    assert_eq!(info.imports[0].type_index, 0);
    // The imported function occupies index 0, so `main` is function 1.
    assert_eq!(info.function_index("main"), Some(1));
}

#[test]
fn the_stack_pointer_import_is_synthesized_not_imported() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_import_global(STACK_POINTER_SYMBOL, 0x7F, true);
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));

    let info = WasmLinkInfo::build(&[builder.finish()], None, &[], false).expect("plan builds");

    assert!(
        info.imports.is_empty(),
        "the linker defines the stack pointer"
    );
    assert_eq!(info.stack_top, DEFAULT_STACK_SIZE);
}

#[test]
fn global_imports_other_than_the_stack_pointer_are_rejected() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_import_global("errno", 0x7F, false);

    let error = WasmLinkInfo::build(&[builder.finish()], None, &[], false)
        .expect_err("a host global has no known address");

    assert_eq!(error.code, ErrorCode::UnsupportedFormat);
    assert!(error.message.contains("errno"), "{}", error.message);
}

#[test]
fn pending_relocations_do_not_block_plan_construction() {
    // Relocations name object-local indices, so the plan must be buildable
    // while they are still unapplied; `wasm::apply_relocations` rewrites them
    // afterwards, against this same plan.
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    builder.add_relocation("main");

    let info = WasmLinkInfo::build(&[builder.finish()], None, &[], false)
        .expect("the plan describes final indices; pending fixups are applied later");

    assert_eq!(info.function_index("main"), Some(0));
}

#[test]
fn defined_globals_are_rejected_instead_of_dropped() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    builder.add_section(Section::new_data(".wasm.global", vec![0x01, 0x7F], true, 1));

    let error = WasmLinkInfo::build(&[builder.finish()], None, &[], false)
        .expect_err("the writer cannot re-emit globals yet");

    assert_eq!(error.code, ErrorCode::UnsupportedFormat);
    assert!(
        error.message.contains("defined globals"),
        "{}",
        error.message
    );
}

#[test]
fn a_cross_object_reference_becomes_a_call_not_a_host_import() {
    let mut caller = ObjectBuilder::new(0);
    caller.add_import("env", "helper", &unit_to_i32());
    caller.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    let mut callee = ObjectBuilder::new(1);
    callee.add_function(Some("helper"), &unit_to_i32(), &body_i32(7));

    let info = WasmLinkInfo::build(&[caller.finish(), callee.finish()], None, &[], false)
        .expect("the module defines the name, so nothing has to be imported for it");

    assert!(
        info.imports.is_empty(),
        "`env.helper` is defined by the second input, so the module imports nothing: {:?}",
        info.imports
    );
    assert!(info.defines_function("helper"));
    // Both bodies are emitted; `helper` is the second, and no import prefix
    // shifts it.
    assert_eq!(info.function_count(), 2);
    assert_eq!(info.local_function_index("helper"), Some(1));
    assert_eq!(
        info.functions[1].linkable, true,
        "the name is what the other object binds to"
    );
}

#[test]
fn address_taken_functions_fill_the_table_from_slot_one() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("sq"), &i32_to_i32(), &body_i32(1));
    builder.add_function(Some("cu"), &i32_to_i32(), &body_i32(2));
    builder.add_element_segment(&[1, 0]);

    let info = WasmLinkInfo::build(&[builder.finish()], None, &[], false).expect("plan builds");

    assert_eq!(
        info.table_entries,
        vec![1, 0],
        "element order is table-slot order, and the entries are final function indices"
    );
    assert_eq!(info.table_min, 3, "slot 0 stays free for a null pointer");
    assert_eq!(info.table_slot_for_name("cu"), Some(1));
    assert_eq!(info.table_slot_for_name("sq"), Some(2));
    assert_eq!(info.table_slot_of_function(0), Some(2));
}

#[test]
fn a_table_entry_naming_a_host_function_is_undefined() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_import("wasi_snapshot_preview1", "fd_write", &i32_to_i32());
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    builder.add_element_segment(&[0]);

    let error = WasmLinkInfo::build(&[builder.finish()], None, &[], false)
        .expect_err("a host function has no body to put in the table");

    assert_eq!(error.code, ErrorCode::UndefinedSymbol);
    assert!(error.message.contains("fd_write"), "{}", error.message);
}

#[test]
fn a_cross_object_table_entry_binds_to_the_definition() {
    let mut caller = ObjectBuilder::new(0);
    caller.add_function(Some("apply"), &unit_to_i32(), &body_i32(1));
    caller.add_import("env", "sq", &i32_to_i32());
    caller.add_element_segment(&[0]);
    let mut callee = ObjectBuilder::new(1);
    callee.add_function(Some("sq"), &i32_to_i32(), &body_i32(2));

    let info = WasmLinkInfo::build(&[caller.finish(), callee.finish()], None, &[], false)
        .expect("the entry names a definition, so the table can hold it");

    assert!(info.imports.is_empty(), "`env.sq` resolves inside the link");
    let sq = info.function_index("sq").expect("`sq` is defined");
    assert_eq!(info.table_entries, vec![sq]);
    assert_eq!(info.table_slot_for_name("sq"), Some(1));
}

#[test]
fn a_host_import_no_input_defines_is_still_emitted() {
    let mut caller = ObjectBuilder::new(0);
    caller.add_import("wasi_snapshot_preview1", "fd_write", &i32_to_i32());
    caller.add_function(Some("main"), &unit_to_i32(), &body_i32(1));

    let info = WasmLinkInfo::build(&[caller.finish()], None, &[], false).expect("plan builds");

    assert_eq!(info.imports.len(), 1, "the host still supplies `fd_write`");
    assert_eq!(
        info.imported_function_index("wasi_snapshot_preview1", "fd_write"),
        Some(0)
    );
    assert_eq!(
        info.local_function_index("fd_write"),
        None,
        "a host function is not a definition the module owns"
    );
}

#[test]
fn two_strong_definitions_of_one_name_are_a_loud_error() {
    let mut first = ObjectBuilder::new(0);
    first.add_function(Some("dup"), &unit_to_i32(), &body_i32(1));
    let mut second = ObjectBuilder::new(1);
    second.add_function(Some("dup"), &unit_to_i32(), &body_i32(2));

    let error = WasmLinkInfo::build(&[first.finish(), second.finish()], None, &[], false)
        .expect_err("binding by name cannot pick between two bodies");

    assert_eq!(error.code, ErrorCode::DuplicateSymbol);
    assert!(error.message.contains("dup"), "{}", error.message);
    assert!(
        error.message.contains("obj0.o") && error.message.contains("obj1.o"),
        "the message must name both inputs: {}",
        error.message
    );
}

#[test]
fn a_strong_definition_wins_over_a_weak_one_whichever_comes_first() {
    let weak_then_strong = |weak_first: bool| {
        let mut first = ObjectBuilder::new(0);
        let mut second = ObjectBuilder::new(1);
        let binding = |weak: bool| {
            if weak {
                SymbolBinding::Weak
            } else {
                SymbolBinding::Global
            }
        };
        first.add_function_binding(
            Some("dup"),
            binding(weak_first),
            &unit_to_i32(),
            &body_i32(1),
        );
        second.add_function_binding(
            Some("dup"),
            binding(!weak_first),
            &unit_to_i32(),
            &body_i32(2),
        );
        WasmLinkInfo::build(&[first.finish(), second.finish()], None, &[], false)
            .expect("a weak definition never competes with a strong one")
    };

    // The strong body is the second in one order and the first in the other; the
    // plan must follow the strength, not the input order.
    assert_eq!(weak_then_strong(true).local_function_index("dup"), Some(1));
    assert_eq!(weak_then_strong(false).local_function_index("dup"), Some(0));
}

#[test]
fn a_reference_with_the_wrong_signature_is_a_loud_error() {
    let mut caller = ObjectBuilder::new(0);
    caller.add_import("env", "helper", &i32_to_i32());
    let mut callee = ObjectBuilder::new(1);
    callee.add_function(Some("helper"), &unit_to_i32(), &body_i32(7));

    let error = WasmLinkInfo::build(&[caller.finish(), callee.finish()], None, &[], false)
        .expect_err("the call would leave the operand stack in the wrong shape");

    assert_eq!(error.code, ErrorCode::AbiMismatch);
    assert!(error.message.contains("helper"), "{}", error.message);
    assert!(
        error.message.contains("(i32) -> (i32)") && error.message.contains("() -> (i32)"),
        "the message must show both signatures: {}",
        error.message
    );
}

#[test]
fn exports_follow_flags_config_list_and_export_all() {
    let build = |objects: Vec<ObjectFile>, requested: &[String], export_all: bool| {
        WasmLinkInfo::build(&objects, None, requested, export_all).expect("plan builds")
    };

    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("_start"), &unit_to_i32(), &body_i32(1));
    builder.add_function(Some("helper"), &unit_to_i32(), &body_i32(2));
    builder.add_function(Some(".Ltmp"), &unit_to_i32(), &body_i32(3));
    builder.export("_start");

    let export_names = |info: &WasmLinkInfo| -> Vec<String> {
        info.exports.iter().map(|e| e.name.clone()).collect()
    };
    assert_eq!(
        export_names(&build(vec![builder.finish()], &[], false)),
        vec!["_start"]
    );

    let mut quiet = ObjectBuilder::new(0);
    quiet.add_function(Some("_start"), &unit_to_i32(), &body_i32(1));
    quiet.add_function(Some("helper"), &unit_to_i32(), &body_i32(2));

    let selected = build(vec![quiet.finish()], &["helper".to_string()], false);
    assert_eq!(
        export_names(&selected),
        vec!["helper"],
        "only the requested export"
    );

    let mut quiet = ObjectBuilder::new(0);
    quiet.add_function(Some("_start"), &unit_to_i32(), &body_i32(1));
    quiet.add_function(Some("helper"), &unit_to_i32(), &body_i32(2));
    let all = build(vec![quiet.finish()], &[], true);
    assert_eq!(
        export_names(&all),
        vec!["_start", "helper"],
        "`export_all` follows function order"
    );
}

#[test]
fn the_entry_point_is_exported_not_started() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("_start"), &unit_to_unit(), &[0x00, 0x0B]);

    let info =
        WasmLinkInfo::build(&[builder.finish()], Some("_start"), &[], false).expect("plan builds");
    assert_eq!(
        info.exports,
        vec![WasmExportPlan {
            name: "_start".to_string(),
            index: 0
        }],
        "a host calls the entry, so it must be exported"
    );

    // An entry that names nothing must not invent an export: the module would
    // be unusable under a name it does not define.
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("_start"), &unit_to_unit(), &[0x00, 0x0B]);
    let missing =
        WasmLinkInfo::build(&[builder.finish()], Some("nope"), &[], false).expect("plan builds");
    assert!(missing.exports.is_empty());

    // Naming a data object as the entry exports nothing either.
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("_start"), &unit_to_unit(), &[0x00, 0x0B]);
    let mut data = ObjectFile::new(PathBuf::from("data.o"), wasm_target(), 0);
    data.wasm = Some(WasmObjectInfo::default());
    data.add_symbol(Symbol::new_defined(
        "data",
        SymbolBinding::Global,
        SymbolType::Object,
        0,
        0,
        4,
        0,
    ));
    let not_a_function = WasmLinkInfo::build(&[builder.finish(), data], Some("data"), &[], false)
        .expect("plan builds");
    assert!(not_a_function.exports.is_empty());
}

#[test]
fn the_entry_point_must_have_the_empty_signature() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("_start"), &unit_to_i32(), &body_i32(1));

    let error = WasmLinkInfo::build(&[builder.finish()], Some("_start"), &[], false)
        .expect_err("a host cannot call an entry that returns a value");

    assert_eq!(error.code, ErrorCode::InvalidObject);
    assert!(error.message.contains("`_start`"), "{}", error.message);
    assert!(
        error
            .message
            .contains("must take no arguments and return nothing"),
        "{}",
        error.message
    );
}

#[test]
fn data_segments_are_placed_above_the_null_page_and_stack_follows() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));
    builder.add_data_segment(b"abc");
    builder.add_data_segment(b"de");

    let info = WasmLinkInfo::build(&[builder.finish()], None, &[], false).expect("plan builds");

    let offsets: Vec<u64> = info.data_segments.iter().map(|s| s.memory_offset).collect();
    assert_eq!(
        offsets,
        vec![DATA_BASE, DATA_BASE + 8],
        "segments follow each other on an 8-byte boundary, clear of the null page"
    );
    assert_eq!(info.data_start, DATA_BASE);
    assert_eq!(info.data_end, DATA_BASE + 8 + 2);

    let stack_top = (DATA_BASE + 10).next_multiple_of(16) + DEFAULT_STACK_SIZE;
    assert_eq!(info.stack_top, stack_top);
    assert_eq!(info.stack_size, DEFAULT_STACK_SIZE);
    assert_eq!(
        info.initial_pages,
        stack_top.div_ceil(WASM_PAGE_SIZE) as u32
    );
}

#[test]
fn imported_memory_minimum_is_a_floor_for_the_initial_size() {
    let mut builder = ObjectBuilder::new(0);
    builder.add_memory_import(4);
    builder.add_function(Some("main"), &unit_to_i32(), &body_i32(1));

    let info = WasmLinkInfo::build(&[builder.finish()], None, &[], false).expect("plan builds");

    assert_eq!(info.initial_pages, 4, "the imported minimum is honoured");
    assert!(info.imports.is_empty(), "the linker defines the memory");
}

#[test]
fn an_object_without_wasm_facts_is_rejected() {
    let object = ObjectFile::new(PathBuf::from("adob.o"), wasm_target(), 0);

    let error = WasmLinkInfo::build(&[object], None, &[], false)
        .expect_err("an ADOB carries no type/import tables");

    assert_eq!(error.code, ErrorCode::UnsupportedFormat);
    assert!(
        error
            .message
            .contains("not a relocatable WebAssembly object"),
        "{}",
        error.message
    );
}

#[test]
fn an_empty_link_is_an_error() {
    let error = WasmLinkInfo::build(&[], None, &[], false).expect_err("nothing to link");
    assert_eq!(error.code, ErrorCode::InvalidObject);
}
