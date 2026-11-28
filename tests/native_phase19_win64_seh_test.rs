//! Phase 19.6 — Windows x64 SEH execution & structural verification.
//!
//! Structural verification runs on ALL hosts (Linux, macOS, Windows):
//! 1. Compiles real `.adesh` programs for `x86_64-pc-windows-msvc` and verifies
//!    that `AFDE` v2 `win64_prolog` metadata (`SizeOfProlog`, `FrameReg`,
//!    `FrameOffset`, `UWOP_*` unwind codes in descending `CodeOffset` order)
//!    matches the exact machine-code prologue bytes emitted in `.text`.
//! 2. Links the `.adob` alongside a companion COFF/ADOB runtime stub object
//!    that also carries its own `.pdata`/`.xdata` `RUNTIME_FUNCTION` entry,
//!    producing a PE32+ executable via `adesh-linker`.
//! 3. Parses the PE32+ headers, `IMAGE_DIRECTORY_ENTRY_EXCEPTION`, `.pdata`,
//!    and `.xdata` from the emitted `.exe` and verifies:
//!    - `EXCEPTION` directory RVA/size match `.pdata`.
//!    - All `RUNTIME_FUNCTION` entries are sorted, non-overlapping, and valid.
//!    - Both synthesized Adesh function entries (with `__adesh_win64_personality`
//!      + `ALSD` LSDA) and merged input COFF `.pdata`/`.xdata` entries are
//!      present and pass `WindowsPdataGenerator::validate_xdata`.
//!    - `mainCRTStartup` and `__adesh_windows_start` have valid `.pdata`/`.xdata`.
//! 4. Verifies `WindowsPdataGenerator::validate_xdata` rejects malformed
//!    `UNWIND_INFO` records.
//!
//! Real OS SEH execution tests (`#[cfg(all(target_os = "windows", target_arch = "x86_64"))]`)
//! link against the host `adesh_runtime` and execute real Windows exceptions:
//! - Same-function `throw` / `catch`
//! - 3-frame exception propagation preserving callee-saved registers
//! - `defer` + tracked allocation + ARC cleanup during unwind
//! - Nested `try/catch` and `rethrow`
//! - Unhandled exception exit (code 101)
//! - Double exception in cleanup abort policy

use adesh_codegen::targets::create_backend;
use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::target::Target;
use adesh_linker::unwind::{
    AdeshFdeInfo, WindowsPdataGenerator, Win64PrologInfo, parse_adesh_fde_info,
};
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adesh_object::{AdobObject, TargetDescriptor};
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use std::process::{Command, Output};

fn compile_for_win64(src: &str) -> AdobObject {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");
    let mut backend = create_backend(target).expect("backend creation");
    let lowered = backend.lower_module(&native_mod).expect("backend lower");
    let obj = backend.emit_object(&lowered).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");
    obj
}

fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

struct PeSectionInfo {
    name: String,
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
}

struct ParsedPe {
    bytes: Vec<u8>,
    exception_dir_rva: u32,
    exception_dir_size: u32,
    sections: Vec<PeSectionInfo>,
}

impl ParsedPe {
    fn parse(bytes: Vec<u8>) -> Self {
        assert_eq!(&bytes[0..2], b"MZ", "DOS magic");
        let lfanew = read_u32_le(&bytes, 0x3C) as usize;
        assert_eq!(&bytes[lfanew..lfanew + 4], b"PE\0\0", "PE signature");
        let coff = lfanew + 4;
        let machine = read_u16_le(&bytes, coff);
        assert_eq!(machine, 0x8664, "PE machine must be AMD64");
        let num_sections = read_u16_le(&bytes, coff + 2) as usize;
        let opt_size = read_u16_le(&bytes, coff + 16) as usize;
        let opt = coff + 20;
        let magic = read_u16_le(&bytes, opt);
        assert_eq!(magic, 0x020B, "PE32+ magic");
        let num_rvas = read_u32_le(&bytes, opt + 108) as usize;
        assert!(num_rvas > 3, "PE must have at least 4 data directories");
        let dd_base = opt + 112;
        let exception_dir_rva = read_u32_le(&bytes, dd_base + 3 * 8);
        let exception_dir_size = read_u32_le(&bytes, dd_base + 3 * 8 + 4);

        let sec_base = opt + opt_size;
        let mut sections = Vec::with_capacity(num_sections);
        for i in 0..num_sections {
            let s = sec_base + i * 40;
            let raw_name = &bytes[s..s + 8];
            let end = raw_name.iter().position(|&b| b == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&raw_name[..end]).into_owned();
            sections.push(PeSectionInfo {
                name,
                virtual_size: read_u32_le(&bytes, s + 8),
                virtual_address: read_u32_le(&bytes, s + 12),
                size_of_raw_data: read_u32_le(&bytes, s + 16),
                pointer_to_raw_data: read_u32_le(&bytes, s + 20),
            });
        }
        Self {
            bytes,
            exception_dir_rva,
            exception_dir_size,
            sections,
        }
    }

    fn section(&self, name: &str) -> Option<&PeSectionInfo> {
        self.sections.iter().find(|s| s.name == name)
    }

    fn rva_to_slice(&self, rva: u32, len: usize) -> Option<&[u8]> {
        for sec in &self.sections {
            let span = sec.virtual_size.max(sec.size_of_raw_data);
            if rva >= sec.virtual_address && rva < sec.virtual_address + span {
                let off = (rva - sec.virtual_address) as usize;
                let file_off = sec.pointer_to_raw_data as usize + off;
                if file_off + len <= self.bytes.len() {
                    return Some(&self.bytes[file_off..file_off + len]);
                }
            }
        }
        None
    }

    fn section_bytes(&self, sec: &PeSectionInfo) -> &[u8] {
        let len = (sec.virtual_size as usize).min(sec.size_of_raw_data as usize);
        let start = sec.pointer_to_raw_data as usize;
        &self.bytes[start..start + len]
    }
}

/// Build a synthetic companion COFF `.obj` exporting the Phase 19.6 runtime
/// entry points and carrying its own `.pdata` + `.xdata` entry to test
/// input COFF `.pdata`/`.xdata` relocation & merging alongside synthesized
/// Adesh `.adob` entries.
fn build_synthetic_win64_runtime_obj(dir: &Path, name: &str, extra_imports: &[String]) -> PathBuf {
    let path = dir.join(name);

    let pers_code = [
        0x48, 0x83, 0xEC, 0x28, // sub rsp, 40
        0x31, 0xC0, // xor eax, eax
        0x48, 0x83, 0xC4, 0x28, // add rsp, 40
        0xC3, // ret
    ];
    let mut syms: Vec<(String, i16, u32, u8, u16)> = vec![
        ("__adesh_win64_personality".to_string(), 1, 0x00, 2, 0x20),
    ];
    let mut next_off = 0x10u32;
    let mut seen = std::collections::HashSet::new();
    seen.insert("__adesh_win64_personality".to_string());
    for imp in [
        "adesh_rt_unwind_throw",
        "adesh_rt_unwind_resume",
        "adesh_rt_unwind_catch_begin",
        "adesh_rt_unwind_catch_end",
        "adesh_rt_scope_enter",
        "adesh_rt_scope_exit",
        "adesh_rt_alloc",
        "adesh_rt_free",
    ]
    .into_iter()
    .map(str::to_string)
    .chain(extra_imports.iter().cloned())
    {
        if seen.insert(imp.clone()) {
            syms.push((imp, 1, next_off, 2, 0x20));
            next_off += 0x10;
        }
    }
    let mut text = vec![0xCCu8; next_off as usize];
    text[0..pers_code.len()].copy_from_slice(&pers_code);
    for &(_, sec, val, _, _) in &syms[1..] {
        if sec == 1 {
            text[val as usize] = 0xC3;
        }
    }

    // `.xdata` for __adesh_win64_personality: Version=1, Flags=0, Prolog=4, Codes=1, UWOP_ALLOC_SMALL(40) at offset 4.
    let xdata = WindowsPdataGenerator::build_win64_entry_xdata();
    assert_eq!(xdata[1], 4);
    assert_eq!(xdata[2], 1);

    // `.pdata` (12 bytes): Begin=0(+0), End=0(+pers_code.len()), UnwindInfo=0(+0)
    let mut pdata = [0u8; 12];
    pdata[4..8].copy_from_slice(&(pers_code.len() as u32).to_le_bytes());

    let xdata_sym_idx = syms.len() as u32;
    syms.push(("__pers_xdata".to_string(), 2, 0x00, 3, 0x00));

    // 3 COFF relocations on `.pdata` (10 bytes each): IMAGE_REL_AMD64_ADDR32NB = 0x0003
    let mut pdata_relocs = Vec::with_capacity(30);
    for (va, sym_idx) in [(0u32, 0u32), (4u32, 0u32), (8u32, xdata_sym_idx)] {
        pdata_relocs.extend_from_slice(&va.to_le_bytes());
        pdata_relocs.extend_from_slice(&sym_idx.to_le_bytes());
        pdata_relocs.extend_from_slice(&0x0003u16.to_le_bytes());
    }

    // Layout: 20 (COFF header) + 3 * 40 (section headers) = 140
    let text_off = 140u32;
    let xdata_off = text_off + text.len() as u32;
    let pdata_off = xdata_off + xdata.len() as u32;
    let reloc_off = pdata_off + pdata.len() as u32;
    let symtab_off = reloc_off + pdata_relocs.len() as u32;

    let mut buf = Vec::new();
    // COFF header (20 bytes)
    buf.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine = AMD64
    buf.extend_from_slice(&3u16.to_le_bytes()); // NumberOfSections
    buf.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    buf.extend_from_slice(&symtab_off.to_le_bytes()); // PointerToSymbolTable
    buf.extend_from_slice(&(syms.len() as u32).to_le_bytes()); // NumberOfSymbols
    buf.extend_from_slice(&0u16.to_le_bytes()); // SizeOfOptionalHeader
    buf.extend_from_slice(&0u16.to_le_bytes()); // Characteristics

    let push_sec_hdr = |b: &mut Vec<u8>,
                        sname: &[u8; 8],
                        raw_sz: u32,
                        raw_ptr: u32,
                        rel_ptr: u32,
                        nrel: u16,
                        chars: u32| {
        b.extend_from_slice(sname);
        b.extend_from_slice(&0u32.to_le_bytes()); // VirtualSize (0 in COFF obj)
        b.extend_from_slice(&0u32.to_le_bytes()); // VirtualAddress
        b.extend_from_slice(&raw_sz.to_le_bytes());
        b.extend_from_slice(&raw_ptr.to_le_bytes());
        b.extend_from_slice(&rel_ptr.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // PointerToLinenumbers
        b.extend_from_slice(&nrel.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // NumberOfLinenumbers
        b.extend_from_slice(&chars.to_le_bytes());
    };

    // Sec 1: .text
    push_sec_hdr(
        &mut buf,
        b".text\0\0\0",
        text.len() as u32,
        text_off,
        0,
        0,
        0x6000_0020 | 0x0050_0000, // CODE | EXEC | READ | ALIGN_16
    );
    // Sec 2: .xdata
    push_sec_hdr(
        &mut buf,
        b".xdata\0\0",
        xdata.len() as u32,
        xdata_off,
        0,
        0,
        0x4000_0040 | 0x0030_0000, // INITIALIZED_DATA | READ | ALIGN_4
    );
    // Sec 3: .pdata
    push_sec_hdr(
        &mut buf,
        b".pdata\0\0",
        pdata.len() as u32,
        pdata_off,
        reloc_off,
        3,
        0x4000_0040 | 0x0030_0000, // INITIALIZED_DATA | READ | ALIGN_4
    );

    assert_eq!(buf.len() as u32, text_off);
    buf.extend_from_slice(&text);
    buf.extend_from_slice(&xdata);
    buf.extend_from_slice(&pdata);
    buf.extend_from_slice(&pdata_relocs);
    assert_eq!(buf.len() as u32, symtab_off);

    // Build string table (starts with 4-byte total size) + symbol table (18 bytes each).
    let mut strtab = vec![0u8; 4];
    for (sym_name, sec_num, val, storage, ty) in &syms {
        if sym_name.len() <= 8 {
            let mut n = [0u8; 8];
            n[..sym_name.len()].copy_from_slice(sym_name.as_bytes());
            buf.extend_from_slice(&n);
        } else {
            let str_off = strtab.len() as u32;
            strtab.extend_from_slice(sym_name.as_bytes());
            strtab.push(0);
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&str_off.to_le_bytes());
        }
        buf.extend_from_slice(&val.to_le_bytes());
        buf.extend_from_slice(&sec_num.to_le_bytes());
        buf.extend_from_slice(&ty.to_le_bytes());
        buf.push(*storage);
        buf.push(0); // NumberOfAuxSymbols
    }
    let strtab_len = strtab.len() as u32;
    strtab[0..4].copy_from_slice(&strtab_len.to_le_bytes());
    buf.extend_from_slice(&strtab);

    std::fs::write(&path, buf).expect("write companion COFF obj");
    path
}

fn verify_prolog_matches_text(func_name: &str, code: &[u8], prolog: &Win64PrologInfo) {
    assert!(
        (prolog.prolog_size as usize) <= code.len(),
        "{func_name}: prolog_size {} exceeds function length {}",
        prolog.prolog_size,
        code.len()
    );
    // Byte 0 of every Adesh Win64 function prologue is `push rbp` (0x55).
    assert_eq!(
        code[0], 0x55,
        "{func_name}: expected `push rbp` (0x55) at offset 0, got {:02X?}",
        &code[..code.len().min(16)]
    );

    // Check unwind code ordering (strictly non-increasing CodeOffset, all <= prolog_size).
    let mut prev_off = u8::MAX;
    let mut idx = 0usize;
    let mut saw_set_fpreg = false;
    while idx < prolog.unwind_codes.len() {
        let word = prolog.unwind_codes[idx];
        let code_off = (word & 0xFF) as u8;
        let op = ((word >> 8) & 0x0F) as u8;
        let info = ((word >> 12) & 0x0F) as u8;
        assert!(
            code_off <= prolog.prolog_size,
            "{func_name}: unwind code offset {code_off} > prolog_size {}",
            prolog.prolog_size
        );
        assert!(
            code_off <= prev_off,
            "{func_name}: unwind codes not in descending offset order ({code_off} after {prev_off})"
        );
        prev_off = code_off;

        match op {
            // UWOP_PUSH_NONVOL (0): 1 slot. Preceding byte(s) must be `push reg`!
            0 => {
                assert!(code_off >= 1, "{func_name}: UWOP_PUSH_NONVOL at offset 0");
                if info >= 8 {
                    assert!(code_off >= 2);
                    assert_eq!(code[code_off as usize - 2], 0x41, "{func_name}: expected REX.B before push r{}", info);
                    assert_eq!(code[code_off as usize - 1], 0x50 + (info - 8));
                } else {
                    assert_eq!(code[code_off as usize - 1], 0x50 + info, "{func_name}: expected push r{}", info);
                }
                idx += 1;
            }
            // UWOP_ALLOC_LARGE (1): 2 slots (info=0: u16 * 8) or 3 slots (info=1: u32)
            1 => {
                let expected_imm = if info == 0 {
                    (prolog.unwind_codes[idx + 1] as u32) * 8
                } else {
                    (prolog.unwind_codes[idx + 1] as u32)
                        | ((prolog.unwind_codes[idx + 2] as u32) << 16)
                };
                let is_imm32 = code_off >= 7
                    && &code[code_off as usize - 7..code_off as usize - 4] == &[0x48, 0x81, 0xEC]
                    && read_u32_le(code, code_off as usize - 4) == expected_imm;
                assert!(
                    is_imm32,
                    "{func_name}: expected `sub rsp, {expected_imm}` (UWOP_ALLOC_LARGE) ending at {code_off}, got {:02X?}",
                    &code[..code_off as usize]
                );
                idx += if info == 0 { 2 } else { 3 };
            }
            // UWOP_ALLOC_SMALL (2): 1 slot. Matches either `sub rsp, imm8` (48 83 EC xx)
            // or `sub rsp, imm32` (48 81 EC xx xx xx xx).
            2 => {
                let expected_imm = ((info as u32) + 1) * 8;
                let is_imm8 = code_off >= 4
                    && &code[code_off as usize - 4..code_off as usize - 1] == &[0x48, 0x83, 0xEC]
                    && (code[code_off as usize - 1] as u32) == expected_imm;
                let is_imm32 = code_off >= 7
                    && &code[code_off as usize - 7..code_off as usize - 4] == &[0x48, 0x81, 0xEC]
                    && read_u32_le(code, code_off as usize - 4) == expected_imm;
                assert!(
                    is_imm8 || is_imm32,
                    "{func_name}: expected `sub rsp, {expected_imm}` ending at {code_off}, got {:02X?}",
                    &code[..code_off as usize]
                );
                idx += 1;
            }
            // UWOP_SET_FPREG (3): 1 slot. Must match `mov rbp, rsp` (48 89 E5) or `lea rbp, [rsp + off]`.
            3 => {
                saw_set_fpreg = true;
                assert_eq!(prolog.frame_reg, WindowsPdataGenerator::REG_RBP);
                if prolog.frame_reg_offset == 0 {
                    assert!(code_off >= 3);
                    assert_eq!(
                        &code[code_off as usize - 3..code_off as usize],
                        &[0x48, 0x89, 0xE5],
                        "{func_name}: expected `mov rbp, rsp` ending at {code_off}"
                    );
                } else {
                    // `lea rbp, [rsp + disp8/disp32]`: 48 8D 6C 24 xx  or  48 8D AC 24 xx xx xx xx
                    assert!(code_off >= 5);
                    let is_disp8 = &code[code_off as usize - 5..code_off as usize - 1]
                        == &[0x48, 0x8D, 0x6C, 0x24];
                    let is_disp32 = code_off >= 8
                        && &code[code_off as usize - 8..code_off as usize - 4]
                            == &[0x48, 0x8D, 0xAC, 0x24];
                    assert!(
                        is_disp8 || is_disp32,
                        "{func_name}: expected `lea rbp, [rsp + off]` ending at {code_off}, got {:02X?}",
                        &code[..code_off as usize]
                    );
                }
                idx += 1;
            }
            // UWOP_SAVE_XMM128 (8): 2 slots. Verify `movups [rsp + off], xmmN` ends at code_off.
            8 => {
                let slot_off = (prolog.unwind_codes[idx + 1] as u32) * 16;
                assert!(
                    code_off >= 4,
                    "{func_name}: UWOP_SAVE_XMM128 offset {code_off} < 4"
                );
                // `movups [rsp + disp8/disp32], xmmN` has opcode `0F 11` (optional REX prefix before it).
                let window_start = (code_off as usize).saturating_sub(10);
                let has_movups = code[window_start..code_off as usize]
                    .windows(2)
                    .any(|w| w == [0x0F, 0x11]);
                assert!(
                    has_movups,
                    "{func_name}: expected `movups [rsp + {slot_off}], xmm{info}` ending at {code_off}, got {:02X?}",
                    &code[window_start..code_off as usize]
                );
                idx += 2;
            }
            other => panic!("{func_name}: unexpected unwind op {other}"),
        }
    }
    assert!(
        saw_set_fpreg,
        "{func_name}: Win64 prologue must record UWOP_SET_FPREG"
    );
}

#[test]
fn win64_prolog_matches_emitted_text_prologue_and_produces_valid_pe_exception_dir() {
    let src = r#"
        fn level3(x: int): int {
            if x < 0 {
                throw x;
            }
            return x + 10;
        }

        fn level2(x: int): int {
            let a = x * 2;
            let b = x + 3;
            let c = a + b;
            defer { let _d = c + 1; }
            return level3(x) + c;
        }

        fn level1(x: int): int {
            try {
                return level2(x);
            } catch (e) {
                return e + 100;
            }
        }

        fn main(): int {
            let r1 = level1(-5);
            let r2 = level1(7);
            if r1 != 95 { return 1; }
            if r2 != 38 { return 2; }
            return 0;
        }
    "#;

    let obj = compile_for_win64(src);
    let text_sec = obj
        .sections
        .iter()
        .find(|s| s.name == ".text")
        .expect(".text section");
    let fde_sec = obj
        .sections
        .iter()
        .find(|s| s.name == ".adesh.fde_info")
        .expect(".adesh.fde_info section");

    let fdes: Vec<AdeshFdeInfo> =
        parse_adesh_fde_info(&fde_sec.data).expect("parse AFDE v2 section");
    assert!(
        fdes.len() >= 4,
        "expected AFDE entries for level3, level2, level1, main; got {}",
        fdes.len()
    );

    let fde_map: HashMap<&str, &AdeshFdeInfo> =
        fdes.iter().map(|f| (f.func_name.as_str(), f)).collect();

    for fn_name in ["level3", "level2", "level1", "main"] {
        let sym = obj
            .symbols
            .iter()
            .find(|s| s.name == fn_name)
            .unwrap_or_else(|| panic!("missing symbol {fn_name}"));
        let fde = fde_map
            .get(fn_name)
            .unwrap_or_else(|| panic!("missing AFDE entry for {fn_name}"));
        let prolog = fde
            .win64_prolog
            .as_ref()
            .unwrap_or_else(|| panic!("missing win64_prolog for {fn_name}"));
        let code = &text_sec.data[sym.value as usize..(sym.value + sym.size) as usize];
        verify_prolog_matches_text(fn_name, code, prolog);
    }

    // `level2` (defer) and `level1` (try/catch) must attach LSDA + __adesh_win64_personality.
    assert!(
        fde_map["level2"].lsda_symbol.is_some(),
        "level2 (defer) must have LSDA"
    );
    assert_eq!(
        fde_map["level2"].personality_symbol.as_deref(),
        Some("__adesh_win64_personality")
    );
    assert!(
        fde_map["level1"].lsda_symbol.is_some(),
        "level1 (try/catch) must have LSDA"
    );
    assert_eq!(
        fde_map["level1"].personality_symbol.as_deref(),
        Some("__adesh_win64_personality")
    );

    // Link into a real PE32+ image alongside the companion COFF runtime object.
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("phase19_win64_seh_tests");
    std::fs::create_dir_all(&test_dir).expect("create test dir");

    let app_path = test_dir.join("win64_seh_struct.adob");
    std::fs::write(&app_path, AdobWriter::write(&obj).expect("write adob")).expect("write");
    let rt_path = build_synthetic_win64_runtime_obj(&test_dir, "win64_rt_stub.obj", &obj.imports);
    let exe_path = test_dir.join("win64_seh_struct.exe");

    let link_target = Target::from_triple("x86_64-pc-windows-msvc").unwrap();
    let config = LinkConfig::new(exe_path.clone(), link_target);
    Linker::link(&[app_path, rt_path], config).expect("link PE32+ executable");

    let pe_bytes = std::fs::read(&exe_path).expect("read linked PE");
    let pe = ParsedPe::parse(pe_bytes);

    let pdata_sec = pe.section(".pdata").expect("PE must have .pdata section");
    let xdata_sec = pe.section(".xdata").expect("PE must have .xdata section");
    assert_ne!(
        pe.exception_dir_rva, 0,
        "IMAGE_DIRECTORY_ENTRY_EXCEPTION RVA must be nonzero"
    );
    assert_eq!(pe.exception_dir_rva, pdata_sec.virtual_address);
    assert_eq!(pe.exception_dir_size, pdata_sec.virtual_size);
    assert_eq!(pdata_sec.virtual_size % 12, 0);

    let pdata_bytes = pe.section_bytes(pdata_sec);
    let num_entries = pdata_bytes.len() / 12;
    // At least 4 Adesh functions + mainCRTStartup + __adesh_windows_start + companion __adesh_win64_personality!
    assert!(
        num_entries >= 7,
        "expected >= 7 RUNTIME_FUNCTION entries (4 app + 2 entry stubs + 1 runtime), got {num_entries}"
    );

    let mut prev_begin = 0u32;
    let mut ehandler_count = 0usize;
    let mut saw_input_coff_entry = false;
    for i in 0..num_entries {
        let base = i * 12;
        let begin = read_u32_le(pdata_bytes, base);
        let end = read_u32_le(pdata_bytes, base + 4);
        let unwind_rva = read_u32_le(pdata_bytes, base + 8);
        assert!(begin < end, "entry {i}: BeginAddress {begin:#x} >= EndAddress {end:#x}");
        if i > 0 {
            assert!(
                begin >= prev_begin,
                "entry {i}: .pdata not sorted ({begin:#x} < {prev_begin:#x})"
            );
        }
        prev_begin = begin;

        let rdata_sec = pe.section(".rdata").or_else(|| pe.section(".rodata"));
        let in_xdata = unwind_rva >= xdata_sec.virtual_address
            && unwind_rva < xdata_sec.virtual_address + xdata_sec.virtual_size;
        let in_rdata = rdata_sec.is_some_and(|r| {
            unwind_rva >= r.virtual_address && unwind_rva < r.virtual_address + r.virtual_size
        });
        assert!(
            in_xdata || in_rdata,
            "entry {i}: UnwindInfoAddress {unwind_rva:#x} outside .xdata and .rdata/.rodata"
        );

        let xdata_slice = pe
            .rva_to_slice(unwind_rva, 64)
            .expect("UNWIND_INFO inside .xdata");
        // Compute exact record size so validate_xdata checks the exact slice.
        let flags = xdata_slice[0] >> 3;
        let num_codes = xdata_slice[2] as usize;
        let code_bytes = num_codes * 2;
        let aligned_header = 4 + ((code_bytes + 3) & !3);
        let has_handler = (flags & 0x03) != 0;
        let total_len = if has_handler {
            aligned_header + 8 // ExceptionHandler RVA + HandlerData RVA
        } else {
            aligned_header
        };
        let exact_record = pe
            .rva_to_slice(unwind_rva, total_len)
            .expect("full UNWIND_INFO slice");
        WindowsPdataGenerator::validate_xdata(exact_record)
            .unwrap_or_else(|e| panic!("entry {i} invalid UNWIND_INFO: {e}"));

        if exact_record == WindowsPdataGenerator::build_win64_entry_xdata() {
            saw_input_coff_entry = true;
        }

        if has_handler {
            ehandler_count += 1;
            let handler_rva = read_u32_le(exact_record, aligned_header);
            let lsda_rva = read_u32_le(exact_record, aligned_header + 4);
            let text_pe = pe.section(".text").unwrap();
            assert!(
                handler_rva >= text_pe.virtual_address
                    && handler_rva < text_pe.virtual_address + text_pe.virtual_size,
                "entry {i}: ExceptionHandler RVA {handler_rva:#x} must be inside .text"
            );
            let lsda_magic = pe
                .rva_to_slice(lsda_rva, 12)
                .expect("HandlerData LSDA RVA must resolve inside PE");
            assert_eq!(
                &lsda_magic[0..4],
                b"ALSD",
                "entry {i}: HandlerData must point to ALSD v1 table"
            );
        }
    }

    assert!(
        ehandler_count >= 2,
        "expected at least 2 EH-enabled UNWIND_INFO entries (level1 + level2), got {ehandler_count}"
    );
    assert!(
        saw_input_coff_entry,
        "expected relocated input COFF .pdata/.xdata entry for __adesh_win64_personality"
    );
}

#[test]
fn win64_prolog_with_large_frame_and_fp_matches_text_and_pe() {
    // 25 local variables force >128 bytes of stack locals (UWOP_ALLOC_LARGE)
    // and floating-point ops across a call exercise XMM allocation + Win64 prologue.
    let src = r#"
        fn callee_fp(x: float): float {
            return x + 1.5;
        }

        fn large_fp_frame(a: float, b: float): float {
            let v0 = callee_fp(a);
            let v1 = callee_fp(b);
            let l0 = 1; let l1 = 2; let l2 = 3; let l3 = 4;
            let l4 = 5; let l5 = 6; let l6 = 7; let l7 = 8;
            let l8 = 9; let l9 = 10; let l10 = 11; let l11 = 12;
            let l12 = 13; let l13 = 14; let l14 = 15; let l15 = 16;
            let l16 = 17; let l17 = 18;
            let sum = l0 + l1 + l2 + l3 + l4 + l5 + l6 + l7 + l8 + l9
                    + l10 + l11 + l12 + l13 + l14 + l15 + l16 + l17;
            if sum < 0 { throw 1; }
            return v0 + v1;
        }

        fn main(): int {
            try {
                let _r = large_fp_frame(2.0, 3.0);
                return 0;
            } catch (e) {
                return e;
            }
        }
    "#;

    let obj = compile_for_win64(src);
    let text_sec = obj.sections.iter().find(|s| s.name == ".text").unwrap();
    let fde_sec = obj
        .sections
        .iter()
        .find(|s| s.name == ".adesh.fde_info")
        .unwrap();
    let fdes = parse_adesh_fde_info(&fde_sec.data).expect("parse AFDE");
    for fn_name in ["callee_fp", "large_fp_frame", "main"] {
        let sym = obj.symbols.iter().find(|s| s.name == fn_name).unwrap();
        let fde = fdes.iter().find(|f| f.func_name == fn_name).unwrap();
        let prolog = fde.win64_prolog.as_ref().unwrap();
        let code = &text_sec.data[sym.value as usize..(sym.value + sym.size) as usize];
        verify_prolog_matches_text(fn_name, code, prolog);
    }

    let large_fde = fdes.iter().find(|f| f.func_name == "large_fp_frame").unwrap();
    let large_prolog = large_fde.win64_prolog.as_ref().unwrap();
    assert!(
        large_prolog
            .unwind_codes
            .iter()
            .any(|w| ((w >> 8) & 0x0F) == 1),
        "expected UWOP_ALLOC_LARGE in large_fp_frame, got {:04X?}",
        large_prolog.unwind_codes
    );
}

#[test]
fn win64_malformed_unwind_info_rejected() {
    // 1. Too short
    assert!(WindowsPdataGenerator::validate_xdata(&[0x01, 0x00]).is_err());
    // 2. Bad version (v2 instead of v1)
    assert!(WindowsPdataGenerator::validate_xdata(&[0x02, 0x00, 0x00, 0x00]).is_err());
    // 3. FrameOffset nonzero when FrameReg == 0
    assert!(WindowsPdataGenerator::validate_xdata(&[0x01, 0x04, 0x00, 0x10]).is_err());
    // 4. Non-descending code offsets
    let mut non_desc = vec![
        0x01, 0x04, 0x02, 0x00, // Version=1, Prolog=4, Count=2
        0x01, 0x00, // offset 1: UWOP_PUSH_NONVOL rbp
        0x04, 0x00, // offset 4 (> 1 -> invalid!)
    ];
    assert!(WindowsPdataGenerator::validate_xdata(&non_desc).is_err());
    non_desc[6] = 0x01; // valid descending now, but let's test truncated UWOP_ALLOC_LARGE (op=1, info=0 -> needs 2 slots)
    let trunc_large = vec![
        0x01, 0x04, 0x01, 0x00, // Count=1
        0x04, 0x01, // op=1 (UWOP_ALLOC_LARGE), info=0 -> needs 2 slots, only 1 present!
        0x00, 0x00,
    ];
    assert!(WindowsPdataGenerator::validate_xdata(&trunc_large).is_err());
    // 5. EHANDLER flag set without 4-byte ExceptionHandler RVA
    let trunc_handler = vec![
        0x01 | (0x01 << 3), // Version=1, Flags=UNW_FLAG_EHANDLER
        0x00,
        0x00,
        0x00,
    ];
    assert!(WindowsPdataGenerator::validate_xdata(&trunc_handler).is_err());
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn compile_link_and_run_win64(src: &str, test_name: &str) -> Output {
    let obj = compile_for_win64(src);
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("phase19_win64_exec_tests");
    std::fs::create_dir_all(&test_dir).expect("create exec dir");

    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique = format!("{test_name}_{}_{}", std::process::id(), id);

    let adob_path = test_dir.join(format!("{unique}.adob"));
    std::fs::write(&adob_path, AdobWriter::write(&obj).expect("write adob")).expect("write");

    let exe_path = test_dir.join(format!("{unique}.exe"));
    let target = Target::from_triple("x86_64-pc-windows-msvc").unwrap();
    let config = LinkConfig::new(exe_path.clone(), target);
    Linker::link(&[adob_path], config).expect("native Win64 link");
    assert!(exe_path.exists(), "linked PE executable must exist");

    Command::new(&exe_path)
        .output()
        .expect("execute linked Win64 PE")
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn win64_seh_same_function_and_three_frame_propagation() {
    let src = r#"
        fn same_fn_catch(v: int): int {
            try {
                if v < 0 {
                    throw v * 3;
                }
                return v + 1;
            } catch (e) {
                return e + 100;
            }
        }

        fn frame3(n: int): int {
            let k1 = n + 10;
            let k2 = n + 20;
            if n == 7 {
                throw 42;
            }
            return k1 + k2;
        }

        fn frame2(n: int): int {
            let local_a = 100;
            let local_b = 200;
            let r = frame3(n);
            return r + local_a + local_b;
        }

        fn frame1(n: int): int {
            let outer_keep = 55;
            try {
                return frame2(n);
            } catch (e) {
                return e + outer_keep;
            }
        }

        fn main(): int {
            if same_fn_catch(-4) != 88 { return 1; }
            if same_fn_catch(5) != 6 { return 2; }
            if frame1(7) != 97 { return 3; }
            if frame1(1) != 332 { return 4; }
            return 0;
        }
    "#;
    let out = compile_link_and_run_win64(src, "win64_three_frame");
    assert_eq!(
        out.status.code(),
        Some(0),
        "win64 3-frame SEH failed: status={:?}, stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn win64_seh_defer_arc_and_nested_rethrow() {
    let src = r#"
        extern "C" fn adesh_rt_tracked_live_count(): int;

        fn inner_with_cleanup(arr_len: int): int {
            let xs = [1, 2, 3, 4];
            defer {
                let _s = xs[0] + xs[1];
            }
            if arr_len < 0 {
                throw 19;
            }
            return xs[2];
        }

        fn middle_rethrow(x: int): int {
            try {
                return inner_with_cleanup(x);
            } catch (e) {
                throw e + 1;
            }
        }

        fn outer_catch(x: int): int {
            try {
                return middle_rethrow(x);
            } catch (e) {
                return e * 2;
            }
        }

        fn main(): int {
            let before = adesh_rt_tracked_live_count();
            let v = outer_catch(-1);
            let after = adesh_rt_tracked_live_count();
            if v != 40 { return 1; }
            if after != before { return 2; }
            return 0;
        }
    "#;
    let out = compile_link_and_run_win64(src, "win64_defer_rethrow");
    assert_eq!(
        out.status.code(),
        Some(0),
        "win64 defer/rethrow SEH failed: status={:?}, stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn win64_seh_unhandled_exception_exits_101() {
    let src = r#"
        fn boom(): int {
            throw 99;
        }

        fn main(): int {
            return boom();
        }
    "#;
    let out = compile_link_and_run_win64(src, "win64_unhandled");
    assert_eq!(
        out.status.code(),
        Some(101),
        "unhandled Win64 exception must exit 101: status={:?}, stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn win64_seh_double_exception_in_cleanup_aborts() {
    let src = r#"
        fn throw_in_defer(): int {
            defer {
                throw 2;
            }
            throw 1;
        }

        fn main(): int {
            try {
                return throw_in_defer();
            } catch (e) {
                return e;
            }
        }
    "#;
    let out = compile_link_and_run_win64(src, "win64_double_exc");
    assert_ne!(
        out.status.code(),
        Some(0),
        "double exception during cleanup must abort cleanly, not succeed"
    );
}
