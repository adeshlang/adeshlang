//! Tests for the WebAssembly module writer.
//!
//! The writer emits exactly what `WasmLinkInfo` decided, so these tests drive
//! it two ways: through the real `Linker` with a relocatable (relocation-free)
//! object, and directly with a hand-built plan that carries host imports. The
//! output is checked byte-for-byte per section and, when `node` is on PATH,
//! handed to a real WebAssembly engine for validation.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use adesh_linker::wasm::WasmWriter;
use adesh_linker::wasm::link_info::{DATA_BASE, DEFAULT_STACK_SIZE, WasmLinkInfo};
use adesh_linker::wasm::object_info::{WasmFuncType, WasmImportDesc, WasmObjectInfo};
use tempfile::tempdir;

fn wasm_target() -> Target {
    Target::from_triple("wasm32-wasi").expect("wasm32-wasi")
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

fn decode_uleb(bytes: &[u8]) -> (u32, usize) {
    let mut value = 0u32;
    let mut shift = 0;
    for (index, byte) in bytes.iter().enumerate() {
        value |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return (value, index + 1);
        }
        shift += 7;
    }
    panic!("truncated LEB128");
}

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(uleb(payload.len() as u32));
    out.extend_from_slice(payload);
    out
}

fn name(text: &str) -> Vec<u8> {
    let mut out = uleb(text.len() as u32);
    out.extend_from_slice(text.as_bytes());
    out
}

fn custom(section_name: &str, payload: &[u8]) -> Vec<u8> {
    let mut content = name(section_name);
    content.extend_from_slice(payload);
    section(0x00, &content)
}

fn function_symbol(index: u32, symbol_name: &str, flags: u32) -> Vec<u8> {
    let mut out = vec![0x00];
    out.extend(uleb(flags));
    out.extend(uleb(index));
    out.extend(name(symbol_name));
    out
}

fn data_symbol(symbol_name: &str, segment: u32, offset: u32, size: u32, flags: u32) -> Vec<u8> {
    let mut out = vec![0x01];
    out.extend(uleb(flags));
    out.extend(name(symbol_name));
    out.extend(uleb(segment));
    out.extend(uleb(offset));
    out.extend(uleb(size));
    out
}

/// A `linking` section whose only subsection is the symbol table.
fn linking(symbols: &[Vec<u8>]) -> Vec<u8> {
    let mut table = uleb(symbols.len() as u32);
    for symbol in symbols {
        table.extend_from_slice(symbol);
    }
    let mut symtab = vec![0x08];
    symtab.extend(uleb(table.len() as u32));
    symtab.extend(table);

    let mut payload = vec![0x02];
    payload.extend(symtab);
    payload
}

/// A relocatable object with one exported `_start` function and one data
/// segment. No relocations, so every index is already final.
fn relocation_free_object() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    // type: one signature, `() -> ()`, the only shape a start function may have.
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x00]));
    // function: one function, type 0.
    bytes.extend(section(3, &[0x01, 0x00]));
    // code: locals=0, end.
    bytes.extend(section(10, &[0x01, 0x02, 0x00, 0x0B]));

    // data: one active segment at offset 0 holding "hi\0". The linker decides
    // where it actually lands.
    let mut data = vec![0x01, 0x00, 0x41, 0x00, 0x0B, 0x03];
    data.extend_from_slice(b"hi\0");
    bytes.extend(section(11, &data));

    bytes.extend(custom(
        "linking",
        &linking(&[
            // Defined function 0, marked exported.
            function_symbol(0, "_start", 0x20),
            // Local data symbol naming segment 0.
            data_symbol(".L.str", 0, 0, 3, 0x02),
        ]),
    ));

    bytes
}

fn parse_sections(bytes: &[u8]) -> Vec<(u8, &[u8])> {
    assert!(bytes.len() >= 8, "module header");
    let mut offset = 8;
    let mut sections = Vec::new();
    while offset < bytes.len() {
        let id = bytes[offset];
        offset += 1;
        let (len, used) = decode_uleb(&bytes[offset..]);
        offset += used;
        let end = offset + len as usize;
        assert!(end <= bytes.len(), "truncated section {id}");
        sections.push((id, &bytes[offset..end]));
        offset = end;
    }
    sections
}

fn payload<'a>(sections: &'a [(u8, &'a [u8])], id: u8) -> &'a [u8] {
    sections
        .iter()
        .find(|(section_id, _)| *section_id == id)
        .map(|(_, payload)| *payload)
        .unwrap_or_else(|| panic!("module has no section {id}"))
}

fn link_fixture(dir: &tempfile::TempDir) -> Vec<u8> {
    let input = dir.path().join("main.o");
    fs::write(&input, relocation_free_object()).expect("write input object");
    let output = dir.path().join("app.wasm");

    Linker::link(&[input], LinkConfig::new(output.clone(), wasm_target()))
        .expect("a relocation-free wasm object links");
    fs::read(&output).expect("read linked module")
}

#[test]
fn linker_emits_the_module_the_plan_describes() {
    let dir = tempdir().expect("temp dir");
    let bytes = link_fixture(&dir);

    assert_eq!(
        &bytes[..8],
        &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00],
        "module magic and version"
    );

    let sections = parse_sections(&bytes);
    let ids: Vec<u8> = sections.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        ids,
        vec![1, 3, 5, 6, 7, 10, 11],
        "type, function, memory, global, export, code, data"
    );

    assert_eq!(
        payload(&sections, 1),
        &[0x01, 0x60, 0x00, 0x00],
        "the object's signature is re-emitted verbatim"
    );
    assert_eq!(payload(&sections, 3), &[0x01, 0x00]);

    // Data lands at DATA_BASE, so the stack sits above it and memory must
    // cover DATA_BASE + 3 bytes + a 64 KiB stack: two pages.
    let data_end = DATA_BASE + 3;
    let stack_top = data_end.next_multiple_of(16) + DEFAULT_STACK_SIZE;
    assert_eq!(stack_top, 66_576);
    assert_eq!(
        payload(&sections, 5),
        &[0x01, 0x00, 0x02],
        "one memory, no maximum, two 64 KiB pages"
    );

    let mut expected_global = vec![0x01, 0x7F, 0x01, 0x41];
    // `i32.const 66576` as a signed LEB.
    expected_global.extend_from_slice(&[0x90, 0x88, 0x04]);
    expected_global.push(0x0B);
    assert_eq!(
        payload(&sections, 6),
        expected_global.as_slice(),
        "one mutable i32 `__stack_pointer` initialised to the stack top"
    );

    let mut expected_exports = vec![0x02];
    expected_exports.extend(name("_start"));
    expected_exports.extend([0x00, 0x00]);
    expected_exports.extend(name("memory"));
    expected_exports.extend([0x02, 0x00]);
    assert_eq!(
        payload(&sections, 7),
        expected_exports.as_slice(),
        "`_start` and the memory are exported"
    );

    assert_eq!(
        payload(&sections, 10),
        &[0x01, 0x02, 0x00, 0x0B],
        "the body is copied byte-for-byte and re-framed"
    );

    let mut expected_data = vec![0x01, 0x00, 0x41];
    // `i32.const 1024` as a signed LEB, then end, size, payload.
    expected_data.extend_from_slice(&[0x80, 0x08, 0x0B, 0x03]);
    expected_data.extend_from_slice(b"hi\0");
    assert_eq!(
        payload(&sections, 11),
        expected_data.as_slice(),
        "the segment keeps its bytes but moves above the null page"
    );
}

#[test]
fn writer_emits_host_imports_and_shifts_function_indices() {
    let mut object = ObjectFile::new(PathBuf::from("caller.o"), wasm_target(), 0);
    object.add_section(Section::new_code(".text", vec![0x00, 0x0B], 16));
    object.wasm = Some(WasmObjectInfo {
        types: vec![
            // The object's own type 0, used only by the import.
            WasmFuncType {
                params: vec![0x7F],
                results: vec![0x7F],
            },
            // The object's own type 1, used by its defined function.
            WasmFuncType {
                params: Vec::new(),
                results: Vec::new(),
            },
        ],
        function_types: vec![1],
        imports: vec![WasmImportDesc::Function {
            module: "wasi_snapshot_preview1".to_string(),
            name: "fd_write".to_string(),
            type_index: 0,
        }],
        body_offsets: vec![0],
        body_ends: vec![2],
        data_segments: Vec::new(),
        exports: Vec::new(),
    });
    let mut start = Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        0,
        0,
    );
    start.is_exported = true;
    object.add_symbol(start);

    let objects = std::slice::from_ref(&object);
    let info = WasmLinkInfo::build(objects, Some("_start"), &[], false).expect("plan builds");
    assert_eq!(info.function_index("_start"), Some(1));

    let bytes = WasmWriter::encode_binary(objects, &info).expect("writer emits a module");
    let sections = parse_sections(&bytes);

    assert_eq!(
        payload(&sections, 1),
        &[0x02, 0x60, 0x01, 0x7F, 0x01, 0x7F, 0x60, 0x00, 0x00]
    );

    let mut expected_import = vec![0x01];
    expected_import.extend(name("wasi_snapshot_preview1"));
    expected_import.extend(name("fd_write"));
    expected_import.extend([0x00, 0x00]);
    assert_eq!(
        payload(&sections, 2),
        expected_import.as_slice(),
        "the host module name travels with the import"
    );

    assert_eq!(payload(&sections, 3), &[0x01, 0x01]);

    let mut expected_exports = vec![0x02];
    expected_exports.extend(name("_start"));
    expected_exports.extend([0x00, 0x01]);
    expected_exports.extend(name("memory"));
    expected_exports.extend([0x02, 0x00]);
    assert_eq!(
        payload(&sections, 7),
        expected_exports.as_slice(),
        "the export names the shifted function index"
    );
}

#[test]
fn emitted_module_is_accepted_by_a_real_webassembly_engine() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("node is not on PATH; skipping WebAssembly engine validation");
        return;
    }

    let dir = tempdir().expect("temp dir");
    let output = dir.path().join("app.wasm");
    fs::write(&output, link_fixture(&dir)).expect("write module");

    let script = concat!(
        "const fs = require('fs');",
        "const m = new WebAssembly.Module(fs.readFileSync(process.argv[1]));",
        "const ex = WebAssembly.Module.exports(m)",
        "  .map((e) => e.name + ':' + e.kind).join(',');",
        "console.log(ex);"
    );
    let result = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(&output)
        .output()
        .expect("run node");

    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success(),
        "the engine rejected the module: {stderr}"
    );
    assert!(stdout.contains("_start:function"), "exports were: {stdout}");
    assert!(stdout.contains("memory:memory"), "exports were: {stdout}");
}
