//! Tests for applying WASM relocations against the link plan.
//!
//! A relocatable object stores object-local indices; the plan decides the final
//! ones. These tests build objects small enough to reason about byte-for-byte,
//! so a wrong index or a truncated field fails here rather than in a browser.

use adesh_linker::object::{ObjectFile, ObjectReader};
use adesh_linker::target::Target;
use adesh_linker::wasm::link_info::DATA_BASE;
use adesh_linker::wasm::{WasmLinkInfo, apply_relocations};
use std::path::Path;

fn wasm_target() -> Target {
    Target::from_triple("wasm32-wasi").expect("wasm32-wasi triple")
}

fn read(bytes: &[u8]) -> ObjectFile {
    ObjectReader::read_from_memory(bytes, Path::new("input.o"), &wasm_target(), 0)
        .expect("object parses")
}

fn uleb(mut value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(uleb(payload.len() as u32));
    out.extend_from_slice(payload);
    out
}

fn custom(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut content = uleb(name.len() as u32);
    content.extend_from_slice(name.as_bytes());
    content.extend_from_slice(payload);
    section(0x00, &content)
}

fn name(text: &str) -> Vec<u8> {
    let mut out = uleb(text.len() as u32);
    out.extend_from_slice(text.as_bytes());
    out
}

/// Relocation kinds, as numbered by the `reloc.` custom section.
const FUNCTION_INDEX_LEB: u8 = 0;
const MEMORY_ADDR_LEB: u8 = 3;
const MEMORY_ADDR_SLEB: u8 = 4;
const MEMORY_ADDR_I32: u8 = 5;
const GLOBAL_INDEX_LEB: u8 = 7;

/// `linking` symbol table flags this builder needs, as the single LEB byte this
/// test uses for them.
const SYMBOL_UNDEFINED: u8 = 0x10;
const BINDING_GLOBAL: u8 = 0x00;
const BINDING_LOCAL: u8 = 0x02;

/// A relocatable object whose every function has signature `() -> ()` (type 0).
struct ObjectSpec<'a> {
    /// `(module, field)` pairs, in function index order.
    function_imports: &'a [(&'a str, &'a str)],
    /// `(module, field)` pairs, in global index order.
    global_imports: &'a [(&'a str, &'a str)],
    /// `(symbol name, body)` pairs, in code order.
    defined_functions: &'a [(&'a str, &'a [u8])],
    /// Data segment payloads, in segment order. Each gets an active segment.
    data_segments: &'a [&'a [u8]],
    /// `(symbol name, segment, offset, size, binding)` for defined data symbols.
    data_symbols: &'a [(&'a str, u32, u32, u32, u8)],
    /// Names of data this object uses but does not define, so the address must
    /// come from whichever input does.
    undefined_data: &'a [&'a str],
    /// `(kind, offset, symbol index)` against CODE.
    code_relocations: &'a [(u8, u32, u32)],
    /// `(kind, offset, symbol index)` against DATA.
    data_relocations: &'a [(u8, u32, u32)],
}

/// Symbol indices are numbered: defined functions, function imports, global
/// imports, defined data symbols, then undefined data symbols.
fn object(spec: &ObjectSpec<'_>) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    // Every function and the sole type share `() -> ()`.
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x00]));
    let mut section_names = vec!["type"];

    if !spec.function_imports.is_empty() || !spec.global_imports.is_empty() {
        let mut import = Vec::new();
        import.extend(uleb(
            (spec.function_imports.len() + spec.global_imports.len()) as u32,
        ));
        for (module, field) in spec.function_imports {
            import.extend(name(module));
            import.extend(name(field));
            import.extend([0x00, 0x00]);
        }
        for (module, field) in spec.global_imports {
            import.extend(name(module));
            import.extend(name(field));
            // global, i32, mutable
            import.extend([0x03, 0x7F, 0x01]);
        }
        bytes.extend(section(2, &import));
        section_names.push("import");
    }

    // Section indices are assigned in emission order; relocations name them.
    if !spec.defined_functions.is_empty() {
        let mut function = uleb(spec.defined_functions.len() as u32);
        function.extend(vec![0u8; spec.defined_functions.len()]);
        bytes.extend(section(3, &function));
        section_names.push("function");

        let mut code = uleb(spec.defined_functions.len() as u32);
        for (_, body) in spec.defined_functions {
            code.extend(uleb(body.len() as u32));
            code.extend_from_slice(body);
        }
        bytes.extend(section(10, &code));
        section_names.push("code");
    }

    if !spec.data_segments.is_empty() {
        let mut data = uleb(spec.data_segments.len() as u32);
        for payload in spec.data_segments {
            // Active segment, memory 0, offset 0.
            data.extend([0x00, 0x41, 0x00, 0x0B]);
            data.extend(uleb(payload.len() as u32));
            data.extend_from_slice(payload);
        }
        bytes.extend(section(11, &data));
        section_names.push("data");
    }

    // Symbol table, in the order documented on `ObjectSpec`.
    let mut symbols = Vec::new();
    for (index, (symbol, _)) in spec.defined_functions.iter().enumerate() {
        symbols.push(0x00);
        symbols.push(0x00);
        symbols.extend(uleb(spec.function_imports.len() as u32 + index as u32));
        symbols.extend(name(symbol));
    }
    for index in 0..spec.function_imports.len() {
        symbols.push(0x00);
        symbols.push(0x10);
        symbols.extend(uleb(index as u32));
    }
    for index in 0..spec.global_imports.len() {
        symbols.push(0x02);
        symbols.push(0x10);
        symbols.extend(uleb(index as u32));
    }
    for (symbol, segment, offset, size, binding) in spec.data_symbols {
        symbols.push(0x01);
        symbols.push(*binding);
        symbols.extend(name(symbol));
        symbols.extend(uleb(*segment));
        symbols.extend(uleb(*offset));
        symbols.extend(uleb(*size));
    }
    for symbol in spec.undefined_data {
        symbols.push(0x01);
        symbols.push(SYMBOL_UNDEFINED);
        symbols.extend(name(symbol));
    }

    let mut symtab = vec![0x08];
    let mut table = uleb(entry_count(spec) as u32);
    table.extend(symbols);
    symtab.extend(uleb(table.len() as u32));
    symtab.extend(table);
    let mut linking = vec![0x02];
    linking.extend(symtab);
    bytes.extend(custom("linking", &linking));

    let index_of = |wanted: &str| {
        section_names
            .iter()
            .position(|name| *name == wanted)
            .expect("section exists") as u32
    };
    for (reloc_name, target, relocations) in [
        ("reloc.CODE", "code", spec.code_relocations),
        ("reloc.DATA", "data", spec.data_relocations),
    ] {
        if relocations.is_empty() {
            continue;
        }
        let mut payload = uleb(index_of(target));
        payload.extend(uleb(relocations.len() as u32));
        for (kind, offset, symbol) in relocations {
            payload.push(*kind);
            payload.extend(uleb(*offset));
            payload.extend(uleb(*symbol));
            // Memory-address relocations carry an explicit addend on disk.
            if matches!(*kind, MEMORY_ADDR_LEB | MEMORY_ADDR_SLEB | MEMORY_ADDR_I32) {
                payload.push(0x00);
            }
        }
        bytes.extend(custom(reloc_name, &payload));
    }

    bytes
}

fn entry_count(spec: &ObjectSpec<'_>) -> usize {
    spec.defined_functions.len()
        + spec.function_imports.len()
        + spec.global_imports.len()
        + spec.data_symbols.len()
        + spec.undefined_data.len()
}

fn link(mut objects: Vec<ObjectFile>) -> (Vec<ObjectFile>, WasmLinkInfo) {
    let info = WasmLinkInfo::build(&objects, None, &[], false).expect("plan builds");
    apply_relocations(&mut objects, &info).expect("relocations apply");
    (objects, info)
}

/// A `call <index>` instruction with a five-byte LEB placeholder.
fn call_body(index: u8) -> Vec<u8> {
    let mut body = vec![0x00, 0x10];
    body.extend([0x80 | index, 0x80, 0x80, 0x80, 0x00]);
    body.push(0x0B);
    body
}

#[test]
fn an_imported_call_is_renumbered_to_the_plan_index() {
    // The first object's import decides index 0, so the second object's `a`
    // moves from its local index 1 to final index 0.
    let plan_owner = object(&ObjectSpec {
        function_imports: &[("m", "a")],
        global_imports: &[],
        defined_functions: &[],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        code_relocations: &[],
        data_relocations: &[],
    });
    let caller = object(&ObjectSpec {
        function_imports: &[("m", "b"), ("m", "a")],
        global_imports: &[],
        defined_functions: &[("fb", &call_body(1))],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        // Symbol 2 is the second function import, `a`.
        code_relocations: &[(FUNCTION_INDEX_LEB, 4, 2)],
        data_relocations: &[],
    });

    let (objects, info) = link(vec![read(&plan_owner), read(&caller)]);

    assert_eq!(info.imports.len(), 2);
    assert_eq!(info.imports[0].name, "a");
    assert_eq!(info.imports[1].name, "b");
    assert_eq!(info.function_index("fb"), Some(2));

    let text = objects[1].find_section(".text").expect(".text");
    assert_eq!(
        &text.data[4..9],
        &[0x80, 0x80, 0x80, 0x80, 0x00],
        "`call a` was index 1 and must now call imported function 0"
    );
}

#[test]
fn a_defined_call_lands_after_the_import_prefix() {
    let definer = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[("fa", &[0x00, 0x0B]), ("fb", &call_body(0))],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        // Symbol 0 is `fa`; its body starts after the first body frame.
        code_relocations: &[(FUNCTION_INDEX_LEB, 7, 0)],
        data_relocations: &[],
    });
    let importer = object(&ObjectSpec {
        function_imports: &[("host", "x")],
        global_imports: &[],
        defined_functions: &[],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        code_relocations: &[],
        data_relocations: &[],
    });

    let (objects, info) = link(vec![read(&definer), read(&importer)]);

    assert_eq!(info.function_index("fa"), Some(1));
    assert_eq!(info.function_index("fb"), Some(2));

    let text = objects[0].find_section(".text").expect(".text");
    assert_eq!(
        &text.data[7..12],
        &[0x81, 0x80, 0x80, 0x80, 0x00],
        "`fa` is function 1 once the host import takes index 0"
    );
}

#[test]
fn global_and_memory_relocations_are_patched_in_place() {
    let mut body = vec![0x00, 0x23];
    body.extend([0x80, 0x80, 0x80, 0x80, 0x00]); // global.get <5-byte>
    body.push(0x41);
    body.extend([0x80, 0x80, 0x00]); // i32.const <3-byte sleb>
    body.push(0x41);
    body.extend([0x80, 0x80, 0x80, 0x80, 0x00]); // i32.const <5-byte leb>
    body.push(0x0B);

    let module = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[("env", "__stack_pointer")],
        defined_functions: &[("f", &body)],
        data_segments: &[b"abcd"],
        data_symbols: &[("msg", 0, 0, 4, BINDING_LOCAL)],
        undefined_data: &[],
        code_relocations: &[
            (GLOBAL_INDEX_LEB, 4, 1),
            (MEMORY_ADDR_SLEB, 10, 2),
            (MEMORY_ADDR_LEB, 14, 2),
        ],
        data_relocations: &[(MEMORY_ADDR_I32, 6, 2)],
    });

    let (objects, info) = link(vec![read(&module)]);

    assert_eq!(info.stack_pointer_global_index(), 0);
    assert_eq!(info.data_address(0, 6), Some(DATA_BASE as u32));

    let text = objects[0].find_section(".text").expect(".text");
    assert_eq!(&text.data[4..9], &[0x80, 0x80, 0x80, 0x80, 0x00]);
    assert_eq!(
        &text.data[10..13],
        &[0x80, 0x88, 0x00],
        "signed LEB encoding of 1024"
    );
    assert_eq!(
        &text.data[14..19],
        &[0x80, 0x88, 0x80, 0x80, 0x00],
        "unsigned LEB encoding of 1024"
    );

    let data = objects[0].find_section(".data").expect(".data");
    assert_eq!(
        &data.data[6..10],
        &(DATA_BASE as u32).to_le_bytes(),
        "the data-segment pointer literal now names the placed segment"
    );
}

#[test]
fn an_address_relocation_against_unplaced_data_is_a_loud_error() {
    let mut body = vec![0x00, 0x41];
    body.extend([0x80, 0x80, 0x80, 0x80, 0x00]);
    body.push(0x0B);

    let module = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[("f", &body)],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        code_relocations: &[],
        data_relocations: &[],
    });

    // Add a relocation by hand: the symbol does not exist in the table, which
    // must be reported rather than silently skipped.
    let mut objects = vec![read(&module)];
    let code_index = objects[0]
        .sections
        .iter()
        .position(|section| section.name == ".text")
        .expect("code section");
    objects[0].sections[code_index]
        .relocations
        .push(adesh_linker::relocation::Relocation {
            offset: 2,
            symbol_name: "msg".to_string(),
            symbol_index: Some(99),
            file_index: None,
            kind: adesh_linker::relocation::RelocationKind::WasmMemoryOffset32,
            addend: 0,
        });
    let info = WasmLinkInfo::build(&objects, None, &[], false).expect("plan builds");

    let error = apply_relocations(&mut objects, &info).expect_err("unknown symbol index");
    assert_eq!(error.code, adesh_linker::error::ErrorCode::InvalidObject);
    assert!(error.message.contains("99"), "{}", error.message);
}

/// An `i32.const` with a four-byte placeholder for an absolute address.
fn address_body() -> Vec<u8> {
    vec![0x00, 0x41, 0xAA, 0xAA, 0xAA, 0xAA, 0x0B]
}

#[test]
fn a_call_to_another_objects_definition_binds_to_its_body() {
    let definer = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[("callee", &[0x00, 0x0B]), ("other", &[0x00, 0x0B])],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        code_relocations: &[],
        data_relocations: &[],
    });
    let caller = object(&ObjectSpec {
        // A relocatable object cannot tell a sibling's definition from a host
        // function, so it writes the reference the only way it can: an import.
        function_imports: &[("env", "callee")],
        global_imports: &[],
        defined_functions: &[("main", &call_body(4))],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &[],
        // Symbol 1 is the function import, and takes its name from it.
        code_relocations: &[(FUNCTION_INDEX_LEB, 4, 1)],
        data_relocations: &[],
    });

    let (objects, info) = link(vec![read(&definer), read(&caller)]);

    assert!(
        info.imports.is_empty(),
        "the definer supplies `callee`, so `env.callee` must not become a host import"
    );
    assert_eq!(info.function_index("callee"), Some(0));
    assert_eq!(info.function_index("main"), Some(2));

    let text = objects[1].find_section(".text").expect(".text");
    assert_eq!(
        &text.data[4..9],
        &[0x80, 0x80, 0x80, 0x80, 0x00],
        "`call 4` was the object-local placeholder and must now call the definition at index 0"
    );
}

#[test]
fn an_address_relocation_against_another_objects_data_resolves_to_its_segment() {
    let owner = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[],
        data_segments: &[b"abcd"],
        // Only a non-local data symbol can be referenced from another object.
        data_symbols: &[("blob", 0, 1, 3, BINDING_GLOBAL)],
        undefined_data: &[],
        code_relocations: &[],
        data_relocations: &[],
    });
    let user = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[("where_is", &address_body())],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &["blob"],
        // Symbol 1 is the undefined data symbol; the payload starts one byte
        // into the segment, so the answer is the placed segment plus one.
        code_relocations: &[(MEMORY_ADDR_I32, 4, 1)],
        data_relocations: &[],
    });

    let (objects, info) = link(vec![read(&owner), read(&user)]);

    let expected = DATA_BASE as u32 + 1;
    assert_eq!(info.local_data_address("blob"), Some(expected));

    let text = objects[1].find_section(".text").expect(".text");
    assert_eq!(
        &text.data[4..8],
        &expected.to_le_bytes(),
        "the address literal must name the byte the defining object's segment landed on"
    );
}

#[test]
fn an_address_of_data_no_input_defines_is_a_loud_error() {
    let loner = object(&ObjectSpec {
        function_imports: &[],
        global_imports: &[],
        defined_functions: &[("where_is", &address_body())],
        data_segments: &[],
        data_symbols: &[],
        undefined_data: &["nowhere"],
        code_relocations: &[(MEMORY_ADDR_I32, 4, 1)],
        data_relocations: &[],
    });

    let mut objects = vec![read(&loner)];
    let info = WasmLinkInfo::build(&objects, None, &[], false).expect("plan builds");

    let error = apply_relocations(&mut objects, &info)
        .expect_err("no input defines the data whose address is taken");
    assert_eq!(error.code, adesh_linker::error::ErrorCode::UndefinedSymbol);
    assert!(error.message.contains("nowhere"), "{}", error.message);
}
