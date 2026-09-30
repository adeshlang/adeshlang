use adesh_linker::object::{ObjectFile, ObjectWriter};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_lld_style_aliases_link_without_silently_ignoring_options() {
    let dir = tempdir().unwrap();
    let target = Target::x86_64_windows();
    let input = dir.path().join("main.obj");
    let output = dir.path().join("app.exe");
    let map = dir.path().join("app.map");
    let mut obj = ObjectFile::new(input.clone(), target, 0);
    obj.add_section(Section::new_code(".text", vec![0xc3], 16));
    obj.add_symbol(Symbol::new_defined(
        "mainCRTStartup",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        1,
        0,
    ));
    ObjectWriter::write_to_file(&obj, &input).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_adeshlink"))
        .arg("--target=x86_64-windows")
        .arg(format!("--output={}", output.display()))
        .arg("-O3")
        .arg("--icf=none")
        .arg("-s")
        .arg("-Map")
        .arg(&map)
        .arg(&input)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    assert!(std::fs::read(output).unwrap().starts_with(b"MZ"));
    assert!(map.exists());
    assert!(result.stderr.is_empty(), "{result:?}");
}

#[test]
fn test_unsupported_cli_flags_fail_instead_of_being_ignored() {
    let result = Command::new(env!("CARGO_BIN_EXE_adeshlink"))
        .arg("--plugin-opt=made-up")
        .arg("missing.o")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported linker option"));
}
