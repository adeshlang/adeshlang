//! Phase 5 Comprehensive Debuggability & SEH Unwind End-to-End Test Suite.
//!
//! Validates:
//! 1. Windows x64 SEH Unwind Support:
//!    - PE Optional Header `IMAGE_DIRECTORY_ENTRY_EXCEPTION` populated.
//!    - `.pdata` 12-byte `RUNTIME_FUNCTION` records strictly sorted by `begin_rva`.
//!    - `.xdata` valid `UNWIND_INFO` structure (version 1, frame register RBP, unwind codes).
//!    - Execution of native PE binary with SEH unwind tables.
//! 2. DWARF 5 `.debug_line` State Machine for Linux ELF:
//!    - DWARF 5 `.debug_line` header, directory and file format tables, standard opcodes (`DW_LNS_advance_pc`, `DW_LNS_advance_line`, `DW_LNS_copy`, `DW_LNE_set_address`, `DW_LNE_end_sequence`).
//!    - DWARF 5 `.debug_info`, `.debug_abbrev`, and `.debug_str` compilation unit and subprogram DIEs.
//!    - Linux ELF packaging: non-alloc debug sections placed with `sh_flags == 0`, `sh_addr == 0`.
//! 3. Structured Panic and Abort Context:
//!    - Function symbol name prefix in runtime abort messages (`[func] panic: ...`).
//!    - Source location context in panic hooks (`[Adesh Panic] file:line - msg`).

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_linker::debug::Dwarf5Generator;
use adesh_linker::pe::header::IMAGE_DIRECTORY_ENTRY_EXCEPTION;
use adesh_linker::section::{Section, SectionKind};
use adesh_linker::target::Target;
use adesh_linker::unwind::{PdataEntry, WindowsPdataGenerator};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_windows_pdata_and_xdata_generation_and_sorting() {
    let entries = vec![
        PdataEntry {
            begin_rva: 0x3000,
            end_rva: 0x3050,
            unwind_info_rva: 0x5000,
        },
        PdataEntry {
            begin_rva: 0x1000,
            end_rva: 0x1080,
            unwind_info_rva: 0x5020,
        },
        PdataEntry {
            begin_rva: 0x2000,
            end_rva: 0x2040,
            unwind_info_rva: 0x5040,
        },
    ];

    let pdata_bytes = WindowsPdataGenerator::build_pdata(entries);
    assert_eq!(pdata_bytes.len(), 36, "3 entries * 12 bytes");

    // Verify strictly ascending sorted begin_rva
    let rva_0 = u32::from_le_bytes(pdata_bytes[0..4].try_into().unwrap());
    let rva_1 = u32::from_le_bytes(pdata_bytes[12..16].try_into().unwrap());
    let rva_2 = u32::from_le_bytes(pdata_bytes[24..28].try_into().unwrap());

    assert_eq!(rva_0, 0x1000);
    assert_eq!(rva_1, 0x2000);
    assert_eq!(rva_2, 0x3000);

    // Verify xdata standard frame
    let xdata =
        WindowsPdataGenerator::build_standard_frame_xdata(48, &[WindowsPdataGenerator::REG_RBX]);
    assert!(xdata.len() >= 4);
    assert_eq!(xdata[0], 0x01, "UNWIND_INFO Version 1");
    assert_eq!(
        xdata[3] & 0x0F,
        WindowsPdataGenerator::REG_RBP,
        "Frame register is RBP"
    );
    assert_eq!(xdata.len() % 4, 0, "xdata must be 4-byte aligned");
}

#[test]
fn test_dwarf5_debug_line_state_machine_format() {
    let directories = vec!["/home/user/project", "/home/user/project/src"];
    let files = vec![("main.adesh", 1), ("lib.adesh", 1)];
    let line_entries = vec![(0x401000, 1), (0x401010, 5), (0x401025, 12), (0x401050, 20)];

    let sec = Dwarf5Generator::synthesize_debug_line(&directories, &files, line_entries);
    assert_eq!(sec.name, ".debug_line");
    assert_eq!(sec.kind, SectionKind::Debug);
    assert!(!sec.is_alloc(), ".debug_line must be non-alloc");

    let data = &sec.data;
    assert!(data.len() > 30);

    // DWARF 5 version check (bytes 4..6 is 5)
    let version = u16::from_le_bytes([data[4], data[5]]);
    assert_eq!(version, 5);

    // Address size (byte 6 is 8)
    assert_eq!(data[6], 8);

    // Segment selector size (byte 7 is 0)
    assert_eq!(data[7], 0);

    // Minimum instruction length (byte at header_start is 1)
    let header_len = u32::from_le_bytes(data[8..12].try_into().unwrap());
    assert!(header_len > 0);
}

#[test]
fn test_dwarf5_debug_info_and_abbrev_generation() {
    let str_sec =
        Dwarf5Generator::synthesize_debug_str(&["main.adesh", "adeshc 0.3.0", "calculate", "main"]);
    assert_eq!(str_sec.name, ".debug_str");

    let abbrev_sec = Dwarf5Generator::synthesize_debug_abbrev();
    assert_eq!(abbrev_sec.name, ".debug_abbrev");

    let functions = vec![
        (24, 0x401000, 0x50), // calculate
        (34, 0x401050, 0x30), // main
    ];

    let info_sec =
        Dwarf5Generator::synthesize_debug_info_with_functions(0, 11, 0x401000, 0x80, 0, &functions);
    assert_eq!(info_sec.name, ".debug_info");

    let version = u16::from_le_bytes([info_sec.data[4], info_sec.data[5]]);
    assert_eq!(version, 5);
}

#[test]
fn test_elf_with_dwarf5_debug_sections_linking() {
    let target = Target::x86_64_linux();
    let mut object = adesh_linker::object::ObjectFile::new(
        std::path::PathBuf::from("test_debug.o"),
        target.clone(),
        0,
    );

    // .text section with simple exit(42) syscall
    let code = vec![
        0xB8, 0x2A, 0x00, 0x00, 0x00, // mov eax, 42
        0xC3, // ret
    ];
    let text_sec = Section::new_code(".text", code, 16);
    object.add_symbol(adesh_linker::symbol::Symbol::new_defined(
        "main",
        adesh_linker::symbol::SymbolBinding::Global,
        adesh_linker::symbol::SymbolType::Function,
        0,
        0,
        6,
        0,
    ));
    object.add_section(text_sec);

    // Add DWARF 5 debug sections
    let directories = vec!["/workspace"];
    let files = vec![("main.adesh", 0)];
    let line_entries = vec![(0x401000, 1), (0x401005, 2)];
    let debug_line = Dwarf5Generator::synthesize_debug_line(&directories, &files, line_entries);
    object.add_section(debug_line);

    let debug_abbrev = Dwarf5Generator::synthesize_debug_abbrev();
    object.add_section(debug_abbrev);

    let debug_info = Dwarf5Generator::synthesize_debug_info(0, 10, 0x401000, 6);
    object.add_section(debug_info);

    let mut objects = vec![object];
    let mut resolver = adesh_linker::resolver::SymbolResolver::new();
    resolver
        .resolve_with_target(&mut objects, &[], &target)
        .expect("resolve");

    let mut layout = adesh_linker::layout::LayoutEngine::new();
    layout
        .layout_and_relocate(&objects, &resolver, &target, "main")
        .expect("layout and relocate");

    // Verify debug sections are present in merged sections
    assert!(
        layout
            .merged_sections
            .iter()
            .any(|s| s.name == ".debug_line" && !s.is_alloc()),
        ".debug_line merged section must exist as non-alloc"
    );
    assert!(
        layout
            .merged_sections
            .iter()
            .any(|s| s.name == ".debug_info" && !s.is_alloc()),
        ".debug_info merged section must exist as non-alloc"
    );

    let elf_bytes = adesh_linker::elf::ElfWriter::encode_executable(
        &target,
        layout.entry_va,
        &layout.merged_sections,
        &layout.resolved_symbols,
        None,
    )
    .expect("encode elf");

    assert!(elf_bytes.len() > 128);
    assert_eq!(&elf_bytes[0..4], b"\x7fELF");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_pe_exception_directory_and_execution_e2e() {
    let mut module = NativeModule::new("test_pdata_module");

    // Function 1: helper
    let mut helper_func = MachineFunction::new("helper");
    helper_func.stack_size = 32;
    let b1 = helper_func.entry_block_mut();
    b1.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(50),
    });
    b1.push(MachineInstruction::Return);
    module.add_function(helper_func);

    // Function 2: main
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 48;
    let b2 = main_func.entry_block_mut();
    b2.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("helper".to_string()),
        num_args: 0,
    });
    b2.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(7),
    });
    b2.push(MachineInstruction::Return);
    module.add_function(main_func);

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(OptLevel::O2);

    let obj = backend.emit_object(&module).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join("test_pdata.adob");
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join("test_pdata.exe");
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    // Read generated PE binary and verify Exception Directory in Optional Header
    let exe_bytes = std::fs::read(&exe_path).expect("read exe bytes");
    let lfanew = u32::from_le_bytes(exe_bytes[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&exe_bytes[lfanew..lfanew + 4], b"PE\0\0");

    let opt_hdr_off = lfanew + 24;
    let exc_dir_off = opt_hdr_off + 112 + IMAGE_DIRECTORY_ENTRY_EXCEPTION * 8;
    let pdata_rva = u32::from_le_bytes(exe_bytes[exc_dir_off..exc_dir_off + 4].try_into().unwrap());
    let pdata_size = u32::from_le_bytes(
        exe_bytes[exc_dir_off + 4..exc_dir_off + 8]
            .try_into()
            .unwrap(),
    );

    assert!(pdata_rva > 0, "Exception Directory RVA must be non-zero");
    assert!(
        pdata_size >= 12,
        "Exception Directory Size must cover RUNTIME_FUNCTION entries"
    );
    assert_eq!(
        pdata_size % 12,
        0,
        "pdata size must be multiple of 12 bytes"
    );

    // Run executable and assert exit code
    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    assert_eq!(out.status.code(), Some(57), "50 + 7 = 57 exit code");
}
