use adesh_linker::config::LinkConfig;
use adesh_linker::error::ErrorCode;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use tempfile::tempdir;

fn wasm_object(dir: &tempfile::TempDir, triple: &str) -> std::path::PathBuf {
    let target = Target::from_triple(triple).unwrap();
    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    // `i64.const 0` then `end`: the payload the stub writer used to copy
    // verbatim into a single hard-coded `() -> i64` function body.
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
fn wasm_link_fails_loudly_instead_of_emitting_stub_module() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("wasm32-wasi").unwrap();
    let obj_path = wasm_object(&dir, "wasm32-wasi");
    let out_path = dir.path().join("app.wasm");

    let config = LinkConfig::new(out_path.clone(), target);
    let err = Linker::link(&[obj_path], config)
        .expect_err("wasm32 linking must not succeed while the writer is a stub");

    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
    assert!(
        !out_path.exists(),
        "no .wasm file may be written by the unimplemented writer"
    );
}

#[test]
fn wasm32_unknown_unknown_links_through_the_same_guard() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("wasm32-unknown-unknown").unwrap();
    let obj_path = wasm_object(&dir, "wasm32-unknown-unknown");
    let out_path = dir.path().join("app.wasm");

    let config = LinkConfig::new(out_path.clone(), target);
    let err = Linker::link(&[obj_path], config)
        .expect_err("wasm32-unknown-unknown must not silently emit a stub module");

    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
    assert!(!out_path.exists());
}

#[test]
fn stub_error_message_names_the_missing_capability() {
    let dir = tempdir().unwrap();
    let obj_path = wasm_object(&dir, "wasm32-wasi");
    let out_path = dir.path().join("app.wasm");
    let target = Target::from_triple("wasm32-wasi").unwrap();

    let err = Linker::link(&[obj_path], LinkConfig::new(out_path, target)).unwrap_err();
    let rendered = err.to_string();
    assert!(rendered.contains("LNK017"), "got: {rendered}");
    assert!(rendered.contains("not implemented"), "got: {rendered}");
}
