use adesh_linker::config::{BuildIdStyle, LinkConfig, LtoMode};
use adesh_linker::hash::{compute_build_id, sha256};
use adesh_linker::layout::LayoutEngine;
use adesh_linker::linker::Linker;
use adesh_linker::map::LinkMapGenerator;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::section::{MergedSection, Section, SectionKind, flags};
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_lto_whole_program_optimization_modes() {
    let dir = tempdir().unwrap();
    let target = Target::x86_64_windows();

    let mut obj = ObjectFile::new(dir.path().join("main.obj"), target.clone(), 0);
    let sec = Section::new_code(".text", vec![0x48, 0x31, 0xC0, 0xC3], 16);
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "mainCRTStartup",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let obj_path = dir.path().join("main.obj");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    for (i, mode) in [LtoMode::Thin, LtoMode::Full].into_iter().enumerate() {
        let output = dir.path().join(format!("lto_{}.exe", i));
        let mut config = LinkConfig::new(output.clone(), target.clone());
        config.lto = mode;
        let res = Linker::link(&[obj_path.clone()], config);
        assert!(
            res.is_ok(),
            "LTO linking failed for {:?}: {:?}",
            mode,
            res.err()
        );
        assert!(output.exists());
    }
}

#[test]
fn test_build_id_generation_modes() {
    let dummy_data = b"ADESH_NATIVE_BINARY_PAYLOAD_TEST_DATA";

    let none_id = compute_build_id(BuildIdStyle::None, dummy_data);
    assert!(none_id.is_empty());

    let sha256_id = compute_build_id(BuildIdStyle::Sha256, dummy_data);
    assert_eq!(sha256_id.len(), 32);

    let fast_id = compute_build_id(BuildIdStyle::Fast, dummy_data);
    assert_eq!(fast_id.len(), 8);

    let uuid_id = compute_build_id(BuildIdStyle::Uuid, dummy_data);
    assert_eq!(uuid_id.len(), 16);
}

#[test]
fn test_sha256_hashing_consistency() {
    let text = b"hello world";
    let hash = sha256(text);
    // sha256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    let hex = hash
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    assert_eq!(
        hex,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn test_link_map_text_and_json_generation() {
    let mut layout = LayoutEngine::new();

    let mut text_merged = MergedSection::new(
        ".text",
        SectionKind::Text,
        flags::READ | flags::EXEC | flags::ALLOC,
        16,
    );
    text_merged.virtual_address = 0x401000;
    text_merged.size = 64;
    layout.merged_sections.push(text_merged);

    let sym = Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0x401000,
        64,
        0,
    );
    layout.resolved_symbols.push(sym);

    let target = Target::x86_64_linux();
    let obj = ObjectFile::new(PathBuf::from("main.o"), target, 0);
    let objects = vec![obj];

    let text_out = LinkMapGenerator::generate_text_map(&objects, &layout);
    assert!(text_out.contains("ADESH LINK MAP"));
    assert!(text_out.contains(".text"));
    assert!(text_out.contains("main"));

    let json_out = LinkMapGenerator::generate_json_map(&objects, &layout);
    assert!(json_out.contains("\"sections\":"));
    assert!(json_out.contains(".text"));
    assert!(json_out.contains("main"));
}
