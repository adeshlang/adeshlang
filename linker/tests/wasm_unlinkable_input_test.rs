//! The linker must refuse inputs it cannot turn into a valid module instead of
//! emitting something that instantiates and miscomputes.
//!
//! An ADOB `.o` has no `linking` custom section, so it carries none of the
//! type, import or body-boundary facts the module writer re-emits.

use adesh_linker::config::LinkConfig;
use adesh_linker::error::ErrorCode;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use tempfile::tempdir;

fn adob_object(dir: &tempfile::TempDir, triple: &str) -> std::path::PathBuf {
    let target = Target::from_triple(triple).unwrap();
    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    obj.add_section(Section::new_code(".text", vec![0x00, 0x42, 0x00, 0x0B], 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let path = dir.path().join("main.o");
    ObjectWriter::write_to_file(&obj, &path).unwrap();
    path
}

#[test]
fn an_object_without_linking_facts_is_rejected() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("wasm32-wasi").unwrap();
    let obj_path = adob_object(&dir, "wasm32-wasi");
    let out_path = dir.path().join("app.wasm");

    let config = LinkConfig::new(out_path.clone(), target);
    let err =
        Linker::link(&[obj_path], config).expect_err("an ADOB carries no WASM type/import tables");

    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
    assert!(
        !out_path.exists(),
        "a failed link must not leave a .wasm file behind"
    );
}

#[test]
fn wasm32_unknown_unknown_takes_the_same_guard() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("wasm32-unknown-unknown").unwrap();
    let obj_path = adob_object(&dir, "wasm32-unknown-unknown");
    let out_path = dir.path().join("app.wasm");

    let config = LinkConfig::new(out_path.clone(), target);
    let err = Linker::link(&[obj_path], config)
        .expect_err("wasm32-unknown-unknown must not accept an ADOB input");

    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
    assert!(!out_path.exists());
}

#[test]
fn the_error_names_the_missing_facts() {
    let dir = tempdir().unwrap();
    let obj_path = adob_object(&dir, "wasm32-wasi");
    let out_path = dir.path().join("app.wasm");
    let target = Target::from_triple("wasm32-wasi").unwrap();

    let err = Linker::link(&[obj_path], LinkConfig::new(out_path, target)).unwrap_err();
    let rendered = err.to_string();
    assert!(rendered.contains("LNK017"), "got: {rendered}");
    assert!(
        rendered.contains("not a relocatable WebAssembly object"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("`linking` custom section"),
        "the suggestion must name what the input lacks: {rendered}"
    );
}
