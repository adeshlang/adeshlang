//! Tests for the WebAssembly relocatable-object reader.
//!
//! A relocatable wasm object is a version-1 module plus a `linking` custom
//! section (whose subsection 8 is the symbol table) and one
//! `reloc.<section>` custom section per relocated section. Every expectation
//! here was cross-checked against `llvm-readobj --relocations --symbols` on
//! the real clang output embedded below.

use adesh_linker::object::{ObjectFile, ObjectReader};
use adesh_linker::relocation::RelocationKind;
use adesh_linker::section::{SectionKind, flags};
use adesh_linker::symbol::{SymbolBinding, SymbolType, SymbolVisibility};
use adesh_linker::target::Target;
use std::path::Path;

fn wasm_target() -> Target {
    Target::from_triple("wasm32-wasi").expect("wasm32-wasi triple")
}

#[allow(clippy::result_large_err)] // mirrors the crate's own reader signature
fn read(bytes: &[u8]) -> adesh_linker::error::LinkResult<ObjectFile> {
    ObjectReader::read_from_memory(bytes, Path::new("input.o"), &wasm_target(), 0)
}

/// `clang --target=wasm32 -c` output for:
///
/// ```c
/// __attribute__((import_module("env"), import_name("puts")))
/// int puts(const char *);
/// static const char str[] = "hi";
/// int add(int a, int b) { return a + b; }
/// int main(void) { puts(str); return add(1, 2); }
/// ```
const CLANG_OBJECT_HEX: &str = "\
0061736d010000000190808080000360027f7f017f6000017f60017f017f02dc808080000403656e760f5f5f6c696e65
61725f6d656d6f727902000103656e760f5f5f737461636b5f706f696e746572037f0103656e76047075747300020365
6e76195f5f696e6469726563745f66756e6374696f6e5f7461626c6501700000038480808000030001000c8180808000
010ab581808000033d01067f238080808000210241102103200220036b21042004200036020c20042001360208200428
020c210520042802082106200520066a210720070f0b65010a7f238080808000210041102101200020016b2102200224
8080808000410021032002200336020c418080808000210420041080808080001a410121054102210620052006108180
808000210741102108200220086a2109200924808080800020070f0b0f01017f108280808000210220020f0b0b898080
8000010041000b0368690000ef80808000076c696e6b696e670208c88080800008000401036164640210000004020f5f
5f6f726967696e616c5f6d61696e0102062e4c2e737472000003001000000403046d61696e0004020b5f5f6d61696e5f
766f696405900100059280808000010e2e726f646174612e2e4c2e737472000100a9808080000a72656c6f632e434f44
450408070601074401075901046a030000740400870100079c010100aa010200c4808080000970726f64756365727301
0c70726f6365737365642d62790123286275696c74206279204272656368742053616e646572732c2072322920636c61
6e670631392e312e3100c9808080000f7461726765745f6665617475726573042b0a6d756c746976616c75652b0f6d75
7461626c652d676c6f62616c732b0f7265666572656e63652d74797065732b087369676e2d657874";

fn hex_bytes(text: &str) -> Vec<u8> {
    let digits: String = text.split_whitespace().collect();
    assert_eq!(digits.len() % 2, 0, "odd number of hex digits");
    (0..digits.len() / 2)
        .map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
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

/// One wasm section: id byte, LEB length, payload.
fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(uleb(payload.len() as u32));
    out.extend_from_slice(payload);
    out
}

/// A custom section whose content starts with the name.
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

/// A relocatable object that calls the imported function `e.ext` and defines
/// `f`. It exercises the parts clang's own output does not: a `linking`
/// subsection other than the symbol table, and a section symbol whose name
/// must come from the section table.
fn hand_built_object(reloc_type: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    // type: () -> ()
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x00]));
    // import: one function `e.ext`, type 0
    let mut import = vec![0x01];
    import.extend(name("e"));
    import.extend(name("ext"));
    import.extend([0x00, 0x00]);
    bytes.extend(section(2, &import));
    // function: one function, type 0
    bytes.extend(section(3, &[0x01, 0x00]));
    // code: `call 0; call 0` (function index space position of the import)
    bytes.extend(section(
        10,
        &[0x01, 0x06, 0x00, 0x10, 0x00, 0x10, 0x00, 0x0B],
    ));

    // linking, symbol table (subsection 8):
    //   0: defined function `f`, function index 1 (import occupies index 0)
    //   1: undefined function, import index 0
    //   2: local section symbol, section index 0
    let mut symbols = vec![0x03];
    symbols.extend([0x00, 0x00, 0x01]);
    symbols.extend(name("f"));
    symbols.extend([0x00, 0x10, 0x00]);
    symbols.extend([0x03, 0x02, 0x00]);
    let mut symtab = vec![0x08];
    symtab.extend(uleb(symbols.len() as u32));
    symtab.extend(symbols);
    // A second subsection (segment info, type 5) with no content: the reader
    // must skip it by its declared length instead of losing alignment.
    let mut linking = vec![0x02];
    linking.extend(symtab);
    linking.extend([0x05, 0x00]);
    bytes.extend(custom("linking", &linking));

    // reloc.CODE: 1 relocation at the `call` operand (offset 4 of the code
    // payload) against symbol 1, the imported `ext`.
    let code_section_index = 3u32;
    let mut reloc = Vec::new();
    reloc.extend(uleb(code_section_index));
    reloc.extend(uleb(1));
    reloc.push(reloc_type);
    reloc.extend(uleb(4));
    reloc.extend(uleb(1));
    bytes.extend(custom("reloc.CODE", &reloc));

    bytes
}

#[test]
fn hand_built_object_keeps_sections_symbols_and_relocations() {
    let object = read(&hand_built_object(0)).expect("hand-built object parses");

    let text = object.find_section(".text").expect(".text");
    assert_eq!(text.kind, SectionKind::Text);
    assert_eq!(text.flags, flags::READ | flags::EXEC | flags::ALLOC);
    assert_eq!(
        text.data,
        vec![0x01, 0x06, 0x00, 0x10, 0x00, 0x10, 0x00, 0x0B]
    );

    // Metadata sections are not allocatable image content.
    let type_section = object.find_section(".wasm.type").expect(".wasm.type");
    assert_eq!(type_section.kind, SectionKind::Custom);
    assert_eq!(type_section.flags, 0);

    assert_eq!(
        object.find_section(".linking").map(|s| s.kind),
        Some(SectionKind::Custom)
    );
    assert_eq!(
        object.find_section(".reloc.CODE").map(|s| s.kind),
        Some(SectionKind::Reloc)
    );

    // Defined function `f` is function index 1; the import takes index 0, so
    // it lands on the single code body, whose payload begins at offset 2.
    let f = object.find_symbol("f").expect("f");
    assert_eq!(f.sym_type, SymbolType::Function);
    assert!(f.is_defined);
    assert_eq!(f.section_index, Some(3));
    assert_eq!(f.value, 2);

    // The undefined symbol has no name of its own; it is resolved from the
    // import section.
    let ext = object.find_symbol("ext").expect("ext");
    assert_eq!(ext.sym_type, SymbolType::Function);
    assert!(!ext.is_defined);
    assert!(ext.is_imported);
    assert_eq!(ext.section_index, None);

    let section_symbol = object
        .symbols
        .iter()
        .find(|s| s.sym_type == SymbolType::Section)
        .expect("section symbol");
    assert_eq!(section_symbol.name, ".wasm.type");
    assert_eq!(section_symbol.section_index, Some(0));
    assert_eq!(section_symbol.binding, SymbolBinding::Local);

    assert_eq!(text.relocations.len(), 1);
    let reloc = &text.relocations[0];
    assert_eq!(reloc.offset, 4);
    assert_eq!(reloc.symbol_name, "ext");
    assert_eq!(reloc.symbol_index, Some(1));
    assert_eq!(reloc.kind, RelocationKind::WasmFunctionIndex);
    assert_eq!(reloc.addend, 0);
}

/// A minimal object whose `linking` symbol table is supplied by the caller.
/// It imports `e.ext` and defines one function, so a symbol entry may refer to
/// either index space.
fn object_with_symbol_table(table: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x00]));

    let mut import = vec![0x01];
    import.extend(name("e"));
    import.extend(name("ext"));
    import.extend([0x00, 0x00]);
    bytes.extend(section(2, &import));

    bytes.extend(section(3, &[0x01, 0x00]));
    bytes.extend(section(10, &[0x01, 0x04, 0x00, 0x10, 0x00, 0x0B]));

    let mut symtab = vec![0x08];
    symtab.extend(uleb(table.len() as u32));
    symtab.extend_from_slice(table);
    let mut linking = vec![0x02];
    linking.extend(symtab);
    bytes.extend(custom("linking", &linking));
    bytes
}

#[test]
fn explicit_symbol_name_replaces_the_import_name() {
    // clang writes this for
    // `__attribute__((import_module("e"), import_name("ext"))) int alias(void);`
    // when the local symbol name differs from the import field name.
    let mut table = vec![0x01];
    table.extend([0x00, 0x50, 0x00]);
    table.extend(name("local_alias"));

    let object = read(&object_with_symbol_table(&table)).expect("explicit-name symbol parses");
    let symbol = object.find_symbol("local_alias").expect("local_alias");
    assert!(symbol.is_imported);
    assert!(!symbol.is_defined);
    assert_eq!(symbol.sym_type, SymbolType::Function);
    assert_eq!(symbol.section_index, None);
    assert!(
        object.find_symbol("ext").is_none(),
        "the import field name is not the linker-visible symbol name"
    );
}

#[test]
fn undefined_data_symbol_carries_its_name_and_no_segment() {
    let mut table = vec![0x01];
    table.extend([0x01, 0x10]);
    table.extend(name("ext_data"));

    let object = read(&object_with_symbol_table(&table)).expect("undefined data symbol parses");
    let symbol = object.find_symbol("ext_data").expect("ext_data");
    // The WASM import section has no data entry, so an undefined data symbol is
    // a cross-object reference the resolver must satisfy, never a host import.
    assert!(!symbol.is_imported);
    assert!(!symbol.is_defined);
    assert_eq!(symbol.sym_type, SymbolType::Object);
    assert_eq!(symbol.section_index, None);
}

#[test]
fn common_data_symbols_are_rejected_instead_of_misplaced() {
    let mut table = vec![0x01];
    table.extend([0x01, 0x03]);
    table.extend(name("tentative"));
    table.extend(uleb(4));
    table.push(0x02);

    let error =
        read(&object_with_symbol_table(&table)).expect_err("common symbols are unimplemented");
    assert_eq!(
        error.code,
        adesh_linker::error::ErrorCode::UnsupportedFormat
    );
    assert!(
        error.message.contains("common data symbol `tentative`"),
        "{}",
        error.message
    );
}

#[test]
fn clang_object_round_trips_into_object_file() {
    let object = read(&hex_bytes(CLANG_OBJECT_HEX)).expect("clang object parses");

    let shape: Vec<(&str, SectionKind)> = object
        .sections
        .iter()
        .map(|s| (s.name.as_str(), s.kind))
        .collect();
    assert_eq!(
        shape,
        vec![
            (".wasm.type", SectionKind::Custom),
            (".wasm.import", SectionKind::Custom),
            (".wasm.function", SectionKind::Custom),
            (".wasm.datacount", SectionKind::Custom),
            (".text", SectionKind::Text),
            (".data", SectionKind::Data),
            (".linking", SectionKind::Custom),
            (".reloc.CODE", SectionKind::Reloc),
            (".producers", SectionKind::Custom),
            (".target_features", SectionKind::Custom),
        ]
    );

    // `main` is hidden in clang output, not exported.
    let names: Vec<&str> = object.symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "add",
            "__stack_pointer",
            "__original_main",
            ".L.str",
            "puts",
            "main",
            "__main_void",
            "__indirect_function_table",
        ]
    );

    // Function symbol values are offsets into the code payload, indexed after
    // the single imported function. clang's code section holds
    // `add`, `__original_main`, `main` in that order.
    let add = object.find_symbol("add").expect("add");
    assert_eq!((add.value, add.section_index), (2, Some(4)));
    let original_main = object
        .find_symbol("__original_main")
        .expect("__original_main");
    assert_eq!(original_main.value, 64);
    let main = object.find_symbol("main").expect("main");
    assert_eq!(main.value, 166);
    assert_eq!(main.visibility, SymbolVisibility::Hidden);
    assert_eq!(main.binding, SymbolBinding::Global);
    assert!(!main.is_exported, "a hidden symbol is not exported");

    // The data symbol lands on its payload inside `.data` (`hi\0`).
    let data = object.find_section(".data").expect(".data");
    let str_symbol = object.find_symbol(".L.str").expect(".L.str");
    assert_eq!(str_symbol.sym_type, SymbolType::Object);
    assert_eq!(str_symbol.section_index, Some(5));
    assert_eq!(str_symbol.size, 3);
    assert_eq!(&data.data[str_symbol.value as usize..][..3], b"hi\0");
    assert_eq!(str_symbol.binding, SymbolBinding::Local);

    // Undefined symbols carry their import names.
    for imported in ["__stack_pointer", "puts", "__indirect_function_table"] {
        let symbol = object.find_symbol(imported).expect(imported);
        assert!(symbol.is_imported, "{imported} should be imported");
        assert_eq!(symbol.section_index, None);
    }

    // The reader must not invent an entry point: the module has no start
    // section, and `_start` is not one of its symbols.
    assert!(object.find_symbol("_start").is_none());
}

#[test]
fn clang_object_relocations_keep_their_kinds_and_targets() {
    let object = read(&hex_bytes(CLANG_OBJECT_HEX)).expect("clang object parses");
    let text = object.find_section(".text").expect(".text");

    let actual: Vec<(u64, &str, RelocationKind, i64)> = text
        .relocations
        .iter()
        .map(|r| (r.offset, r.symbol_name.as_str(), r.kind, r.addend))
        .collect();
    assert_eq!(
        actual,
        vec![
            (0x06, "__stack_pointer", RelocationKind::WasmGlobalIndex, 0),
            (0x44, "__stack_pointer", RelocationKind::WasmGlobalIndex, 0),
            (0x59, "__stack_pointer", RelocationKind::WasmGlobalIndex, 0),
            (0x6a, ".L.str", RelocationKind::WasmMemoryOffsetSleb, 0),
            (0x74, "puts", RelocationKind::WasmFunctionIndex, 0),
            (0x87, "add", RelocationKind::WasmFunctionIndex, 0),
            (0x9c, "__stack_pointer", RelocationKind::WasmGlobalIndex, 0),
            (
                0xaa,
                "__original_main",
                RelocationKind::WasmFunctionIndex,
                0
            ),
        ]
    );
}

#[test]
fn plain_module_reads_without_synthesising_a_start_symbol() {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x01, 0x7F]));
    bytes.extend(section(3, &[0x01, 0x00]));
    bytes.extend(section(10, &[0x01, 0x04, 0x00, 0x42, 0x07, 0x0B]));

    let object = read(&bytes).expect("module parses");
    assert!(object.symbols.is_empty(), "a module declares no symbols");
    assert!(object.find_symbol("_start").is_none());
    assert_eq!(object.sections.len(), 3);
}

#[test]
fn object_version_other_than_one_is_rejected() {
    let mut bytes = hex_bytes(CLANG_OBJECT_HEX);
    bytes[4..8].copy_from_slice(&[0x0D, 0x00, 0x01, 0x00]);

    let error = read(&bytes).expect_err("version 0x0d objects are not supported");
    assert!(
        error.message.contains("unsupported WASM version"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("0d") || error.message.contains("0D"),
        "the version must be named: {}",
        error.message
    );
}

#[test]
fn unimplemented_relocation_types_are_loud_errors() {
    // 23 is R_WASM_MEMORY_ADDR_LOCREL_I32: a real type this linker cannot
    // apply, so the reader must refuse rather than drop the fixup.
    let error = read(&hand_built_object(23)).expect_err("type 23 is not implemented");
    assert!(
        error.message.contains("relocation type 23"),
        "{}",
        error.message
    );
    assert_eq!(
        error.code,
        adesh_linker::error::ErrorCode::UnsupportedRelocation
    );
}

#[test]
fn truncated_section_payload_is_rejected() {
    let mut bytes = hand_built_object(0);
    bytes.pop();

    let error = read(&bytes).expect_err("truncated object is rejected");
    assert!(error.message.contains("truncated"), "{}", error.message);
}

/// A relocatable object that defines one function and places it in the table.
/// `flags` and `opcode` are parameters so the reader's rejection of anything
/// but the MVP encoding can be pinned.
fn object_with_element(flags: u32, opcode: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    // type: () -> ()
    bytes.extend(section(1, &[0x01, 0x60, 0x00, 0x00]));
    // function: one function, type 0
    bytes.extend(section(3, &[0x01, 0x00]));
    // element: one active segment for table 0, `i32.const 1`, one entry
    let mut element = vec![0x01];
    element.extend(uleb(flags));
    element.push(opcode);
    element.push(0x01);
    element.push(0x0B);
    element.extend(uleb(1));
    element.extend(uleb(0));
    bytes.extend(section(9, &element));
    // code: one empty body
    bytes.extend(section(10, &[0x01, 0x02, 0x00, 0x0B]));

    // linking, symbol table: one defined function `f` at index 0
    let mut symbols = vec![0x01];
    symbols.extend([0x00, 0x00, 0x00]);
    symbols.extend(name("f"));
    let mut symtab = vec![0x08];
    symtab.extend(uleb(symbols.len() as u32));
    symtab.extend(symbols);
    let mut linking = vec![0x02];
    linking.extend(symtab);
    bytes.extend(custom("linking", &linking));

    bytes
}

#[test]
fn element_segments_are_kept_with_the_objects_function_indices() {
    let object = read(&object_with_element(0, 0x41)).expect("hand-built object parses");
    let info = object.wasm.as_ref().expect("link facts");

    assert_eq!(info.element_segments.len(), 1);
    assert_eq!(info.element_segments[0].table_offset, 1);
    assert_eq!(
        info.element_segments[0].functions,
        vec![0],
        "entries use the object's own function index space (imports first)"
    );
}

#[test]
fn non_mvp_element_encodings_are_loud_errors() {
    // Flags 1 is a passive segment; the linker has nowhere to put it.
    let passive = read(&object_with_element(1, 0x41)).expect_err("passive segment is refused");
    assert_eq!(
        passive.code,
        adesh_linker::error::ErrorCode::UnsupportedFormat
    );
    assert!(passive.message.contains("flags 1"), "{}", passive.message);

    // `global.get` as the offset expression also has no representation here.
    let global_offset =
        read(&object_with_element(0, 0x23)).expect_err("non-i32.const offset is refused");
    assert_eq!(
        global_offset.code,
        adesh_linker::error::ErrorCode::UnsupportedFormat
    );
    assert!(
        global_offset.message.contains("0x23"),
        "{}",
        global_offset.message
    );
}
