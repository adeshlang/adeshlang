use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use tempfile::tempdir;

#[test]
fn test_link_elf_x86_64_executable() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-linux").unwrap();

    // 1. Create main.o
    let mut main_obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    let mut main_sec = Section::new_code(
        ".text",
        vec![0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00, 0x0F, 0x05],
        16,
    ); // mov rax, 60; syscall
    main_sec.relocations.push(Relocation::new(
        0,
        "helper_func",
        RelocationKind::PcRelative32,
        0,
    ));
    main_obj.add_section(main_sec);
    main_obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        9,
        0,
    ));
    main_obj.add_symbol(Symbol::new_undefined("helper_func", 0));
    let main_path = dir.path().join("main.o");
    ObjectWriter::write_to_file(&main_obj, &main_path).unwrap();

    // 2. Create helper.o
    let mut helper_obj = ObjectFile::new(dir.path().join("helper.o"), target.clone(), 1);
    let helper_sec = Section::new_code(".text", vec![0xC3], 16); // ret
    helper_obj.add_section(helper_sec);
    helper_obj.add_symbol(Symbol::new_defined(
        "helper_func",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        1,
        1,
    ));
    let helper_path = dir.path().join("helper.o");
    ObjectWriter::write_to_file(&helper_obj, &helper_path).unwrap();

    // 3. Link executable
    let out_path = dir.path().join("app");
    let mut config = LinkConfig::new(out_path.clone(), target);
    config.entry_point = Some("_start".to_string());
    config.report = true;

    let res = Linker::link(&[main_path, helper_path], config);
    assert!(res.is_ok(), "ELF linking failed: {:?}", res.err());
    assert!(out_path.exists());

    // Verify ELF binary magic
    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..4], b"\x7fELF");
}

#[test]
fn test_link_pe_windows_executable() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-windows").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("main.obj"), target.clone(), 0);
    let sec = Section::new_code(".text", vec![0x48, 0x31, 0xC0, 0xC3], 16); // xor rax, rax; ret
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

    let out_path = dir.path().join("app.exe");
    let config = LinkConfig::new(out_path.clone(), target);
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "PE linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..2], b"MZ");
}

#[test]
fn test_link_macho_darwin_executable() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("aarch64-macos").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    let sec = Section::new_code("__text", vec![0xC0, 0x03, 0x5F, 0xD6], 16); // ret
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "_main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let obj_path = dir.path().join("main.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("app.dylib");
    let config = LinkConfig::new(out_path.clone(), target);
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "Mach-O linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        0xFEEDFACF
    );
}

#[test]
fn test_link_wasm_module() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("wasm32-wasi").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    let sec = Section::new_code(".text", vec![0x0B], 1); // end
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        1,
        0,
    ));
    let obj_path = dir.path().join("main.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("app.wasm");
    let config = LinkConfig::new(out_path.clone(), target);
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "WASM linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..4], b"\0asm");
}

#[test]
fn test_link_gpu_fatbin_and_quantum_qir() {
    let dir = tempdir().unwrap();

    // GPU Target
    let gpu_target = Target::from_triple("nvptx64-cuda").unwrap();
    let mut gpu_obj = ObjectFile::new(dir.path().join("kernel.o"), gpu_target.clone(), 0);
    gpu_obj.add_section(Section::new_code(".nv.text", vec![0x90, 0x90], 8));
    gpu_obj.add_symbol(Symbol::new_defined(
        "__adesh_gpu_kernel_entry",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        2,
        0,
    ));
    let gpu_obj_path = dir.path().join("kernel.o");
    ObjectWriter::write_to_file(&gpu_obj, &gpu_obj_path).unwrap();

    let gpu_out = dir.path().join("kernel.fatbin");
    let gpu_cfg = LinkConfig::new(gpu_out.clone(), gpu_target);
    assert!(Linker::link(&[gpu_obj_path], gpu_cfg).is_ok());
    assert!(gpu_out.exists());

    // Quantum Target
    let q_target = Target::from_triple("qpu-quantum").unwrap();
    let mut q_obj = ObjectFile::new(dir.path().join("circuit.o"), q_target.clone(), 0);
    q_obj.add_section(Section::new_code(
        ".qir",
        b"; ModuleID = 'circuit.ll'\n".to_vec(),
        4,
    ));
    q_obj.add_symbol(Symbol::new_defined(
        "__adesh_quantum_main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        10,
        0,
    ));
    let q_obj_path = dir.path().join("circuit.o");
    ObjectWriter::write_to_file(&q_obj, &q_obj_path).unwrap();

    let q_out = dir.path().join("circuit.qir");
    let q_cfg = LinkConfig::new(q_out.clone(), q_target);
    assert!(Linker::link(&[q_obj_path], q_cfg).is_ok());
    assert!(q_out.exists());
}
