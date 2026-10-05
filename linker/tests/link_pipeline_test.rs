use adesh_linker::config::LinkConfig;
use adesh_linker::elf::ElfReader;
use adesh_linker::linker::Linker;
use adesh_linker::macho::MachOReader;
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

#[test]
fn test_link_elf_shared_library_full() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-linux").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("libfunc.o"), target.clone(), 0);
    let sec = Section::new_code(".text", vec![0x48, 0x31, 0xC0, 0xC3], 16);
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "exported_api_func",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let obj_path = dir.path().join("libfunc.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("libmyapi.so");
    let mut config = LinkConfig::new(out_path.clone(), target);
    config.shared = true;
    config.libraries.push("m".to_string()); // DT_NEEDED: libm.so

    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "ELF shared library linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..4], b"\x7fELF");
    // Verify e_type == ET_DYN (3)
    let e_type = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
    assert_eq!(e_type, 3, "Expected ET_DYN (3) for shared library");

    // Verify ElfReader reads the produced shared library successfully
    let read_obj = ElfReader::read(&bytes, &out_path, 0);
    assert!(read_obj.is_ok(), "ElfReader failed to parse emitted shared library: {:?}", read_obj.err());
    let parsed = read_obj.unwrap();
    assert!(parsed.symbols.iter().any(|s| s.name == "exported_api_func"));
}

#[test]
fn test_link_elf_dynamic_executable_with_interp() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-linux").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    let sec = Section::new_code(".text", vec![0x48, 0x31, 0xC0, 0xC3], 16);
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let obj_path = dir.path().join("main.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("dynamic_app");
    let mut config = LinkConfig::new(out_path.clone(), target);
    config.libraries.push("c".to_string());

    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "ELF dynamic executable link failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..4], b"\x7fELF");

    // Verify ElfReader roundtrips
    let read_obj = ElfReader::read(&bytes, &out_path, 0);
    assert!(read_obj.is_ok());
}

#[test]
fn test_link_macho_dylib_full() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("aarch64-macos").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("lib.o"), target.clone(), 0);
    let sec = Section::new_code("__text", vec![0xC0, 0x03, 0x5F, 0xD6], 16); // ret
    obj.add_section(sec);
    obj.add_symbol(Symbol::new_defined(
        "_my_dylib_symbol",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    let obj_path = dir.path().join("lib.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("libmytest.dylib");
    let mut config = LinkConfig::new(out_path.clone(), target);
    config.shared = true;

    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "Mach-O dylib linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0xFEEDFACF);

    // Verify filetype == MH_DYLIB (6)
    let filetype = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert_eq!(filetype, 6, "Expected MH_DYLIB (6) for shared dylib");

    // Verify MachOReader reads the produced dylib successfully
    let read_obj = MachOReader::read(&bytes, &out_path, 0);
    assert!(read_obj.is_ok(), "MachOReader failed to parse emitted dylib: {:?}", read_obj.err());
    let parsed = read_obj.unwrap();
    assert!(parsed.symbols.iter().any(|s| s.name == "_my_dylib_symbol"));
}

#[test]
fn test_link_macho_executable_full_headers() {
    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-macos").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("main.o"), target.clone(), 0);
    let sec = Section::new_code("__text", vec![0x48, 0x31, 0xC0, 0xC3], 16);
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

    let out_path = dir.path().join("mac_app");
    let config = LinkConfig::new(out_path.clone(), target);

    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "Mach-O executable linking failed: {:?}", res.err());
    assert!(out_path.exists());

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0xFEEDFACF);
    let filetype = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert_eq!(filetype, 2, "Expected MH_EXECUTE (2) for executable");

    // Verify MachOReader roundtrip
    let read_obj = MachOReader::read(&bytes, &out_path, 0);
    assert!(read_obj.is_ok());
    let parsed = read_obj.unwrap();
    assert!(parsed.symbols.iter().any(|s| s.name == "_main"));
}

#[test]
fn test_link_pe_shared_library_dll_full() {
    use adesh_linker::pe::PeReader;

    let dir = tempdir().unwrap();
    let target = Target::from_triple("x86_64-pc-windows-msvc").unwrap();

    let mut obj = ObjectFile::new(dir.path().join("lib.o"), target.clone(), 0);
    // Function returning 42: mov eax, 42; ret
    let sec = Section::new_code(".text", vec![0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3], 16);
    obj.add_section(sec);
    let mut exp_sym = Symbol::new_defined(
        "compute_answer",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        6,
        0,
    );
    exp_sym.is_exported = true;
    obj.add_symbol(exp_sym);
    let obj_path = dir.path().join("lib.o");
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let out_path = dir.path().join("mylib.dll");
    let mut config = LinkConfig::new(out_path.clone(), target);
    config.shared = true;

    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "PE DLL linking failed: {:?}", res.err());
    assert!(out_path.exists());

    // Check .lib import library exists
    let lib_path = out_path.with_extension("lib");
    assert!(lib_path.exists(), "Expected import library mylib.lib to be generated");

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[0..2], b"MZ", "Missing DOS magic");

    let lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[lfanew..lfanew + 4], b"PE\0\0");

    let characteristics = u16::from_le_bytes(bytes[lfanew + 22..lfanew + 24].try_into().unwrap());
    assert_ne!(
        characteristics & 0x2000,
        0,
        "Expected IMAGE_FILE_DLL (0x2000) characteristic"
    );

    // Verify PeReader reads the produced DLL and finds exported symbol
    let read_obj = PeReader::read(&bytes, &out_path, 0);
    assert!(
        read_obj.is_ok(),
        "PeReader failed to parse emitted DLL: {:?}",
        read_obj.err()
    );
    let parsed = read_obj.unwrap();
    assert!(
        parsed.symbols.iter().any(|s| s.name == "compute_answer"),
        "Exported symbol compute_answer not found in emitted DLL"
    );
}

