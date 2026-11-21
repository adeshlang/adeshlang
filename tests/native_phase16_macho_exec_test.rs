//! Phase 16 Mach-O host dyld gate: linker-built images under real dyld.
//!
//! Complements the structural suite (`macho_phase16_hardening_test`) and the
//! Phase 13 Adesh-source Mach-O gate (`native_phase13_macho_exec_test`):
//!
//! 1. TLV: a thread-local counter through `__thread_vars` / `__tlv_bootstrap`
//!    returns the incremented value as `main`'s exit status.
//! 2. Dylib: link `add42`, `dlopen`/`dlsym` it from a PIE driver, exit with
//!    `add42(5) == 47`.
//! 3. Both images pass `codesign --verify --strict` on the exact linker bytes
//!    (never re-signed), including the dylib's ad-hoc signature.
//!
//! Objects are assembled by hand against the linker API (not Adesh source):
//! source-level `extern` / TLS / `dlopen` remains Phase 18. macOS-only — every
//! case loud-SKIPs elsewhere (or fails under `ADESH_REQUIRE_FOREIGN_EXEC=1`).
#[path = "../linker/tests/support/foreign_exec.rs"]
#[allow(dead_code)]
mod foreign_exec;

use adesh_linker::object::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::{Arch, Target};
use adesh_linker::{LinkConfig, Linker, ObjectFile};
use foreign_exec::run;
use std::path::{Path, PathBuf};
use std::process::Command;

fn macos_host_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => {
            assert!(
                std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
                "required Phase 16 Mach-O dyld execution needs a macOS runner (host is {}/{})",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            eprintln!(
                "SKIP: Phase 16 Mach-O dyld execution needs macOS; host is {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            None
        }
    }
}

fn codesign_verify(path: &Path) {
    let verified = run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(path));
    assert!(
        verified.status.success(),
        "codesign rejected {path:?}: {verified:?}"
    );
}

fn assert_macho_magic(path: &Path, dylib: bool) {
    let bytes = std::fs::read(path).unwrap();
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let filetype = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    assert_eq!(magic, 0xfeedfacf, "Mach-O 64");
    assert_eq!(filetype, if dylib { 6 } else { 2 });
}

// ─── add42 dylib ─────────────────────────────────────────────────────────────

fn build_add42(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    // x86_64: lea eax,[rdi+42]; ret
    // aarch64: add w0,w0,#42; ret
    let code: Vec<u8> = if matches!(target.arch, Arch::AArch64) {
        vec![0x00, 0xa8, 0x00, 0x11, 0xc0, 0x03, 0x5f, 0xd6]
    } else {
        vec![0x8d, 0x47, 0x2a, 0xc3]
    };
    let text_idx = obj.add_section(Section::new_code(".text", code.clone(), 16));
    obj.add_symbol(Symbol::new_defined(
        "add42",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        code.len() as u64,
        0,
    ));
    obj
}

fn build_dlopen_driver_x86(obj_path: PathBuf, target: &Target) -> ObjectFile {
    // main(argc, argv): dlopen(argv[1], RTLD_NOW); dlsym(..., "add42"); return add42(5);
    let mut text: Vec<u8> = Vec::new();
    text.extend_from_slice(&[0x48, 0x8b, 0x7e, 0x08]); // mov rdi,[rsi+8]
    text.extend_from_slice(&[0xbe, 0x02, 0x00, 0x00, 0x00]); // mov esi, RTLD_NOW
    text.extend_from_slice(&[0x31, 0xd2]); // xor edx,edx
    let call_dlopen = text.len() as u64;
    text.extend_from_slice(&[0xe8, 0, 0, 0, 0]); // call dlopen
    text.extend_from_slice(&[0x48, 0x85, 0xc0]); // test rax,rax
    text.extend_from_slice(&[0x74, 0x1c]); // jz fail99  (→ +28 → offset 49)
    text.extend_from_slice(&[0x48, 0x89, 0xc7]); // mov rdi,rax
    let lea_sym = text.len() as u64;
    text.extend_from_slice(&[0x48, 0x8d, 0x35, 0, 0, 0, 0]); // lea rsi,[rip+symname]
    let call_dlsym = text.len() as u64;
    text.extend_from_slice(&[0xe8, 0, 0, 0, 0]); // call dlsym
    text.extend_from_slice(&[0x48, 0x85, 0xc0]); // test rax,rax
    text.extend_from_slice(&[0x74, 0x0e]); // jz fail98
    text.extend_from_slice(&[0xbf, 0x05, 0x00, 0x00, 0x00]); // mov edi,5
    text.extend_from_slice(&[0xff, 0xd0]); // call rax
    text.push(0xc3); // ret
    // fail99:
    text.extend_from_slice(&[0xb8, 0x63, 0x00, 0x00, 0x00, 0xc3]);
    // fail98:
    text.extend_from_slice(&[0xb8, 0x62, 0x00, 0x00, 0x00, 0xc3]);

    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    let sym_bytes = b"add42\0";
    let ro_idx = obj.add_section(Section::new_data(".rodata", sym_bytes.to_vec(), false, 1));
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "symname",
        SymbolBinding::Local,
        SymbolType::Object,
        ro_idx,
        0,
        sym_bytes.len() as u64,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("dlopen", 0));
    obj.add_symbol(Symbol::new_undefined("dlsym", 0));

    let mut sec = obj.sections[text_idx].clone();
    sec.relocations.push(Relocation::new(
        call_dlopen + 1,
        "dlopen",
        RelocationKind::PcRelative32,
        -4,
    ));
    sec.relocations.push(Relocation::new(
        lea_sym + 3,
        "symname",
        RelocationKind::PcRelative32,
        -4,
    ));
    sec.relocations.push(Relocation::new(
        call_dlsym + 1,
        "dlsym",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.sections[text_idx] = sec;
    obj
}

fn build_dlopen_driver_aarch64(obj_path: PathBuf, target: &Target) -> ObjectFile {
    // Frame + x19: dlopen(argv[1]); dlsym; blr add42(5); return.
    let mut text: Vec<u8> = Vec::new();
    text.extend_from_slice(&0xa9be7bfd_u32.to_le_bytes()); // stp x29,x30,[sp,#-32]!
    text.extend_from_slice(&0xa90153f3_u32.to_le_bytes()); // stp x19,x20,[sp,#16]
    text.extend_from_slice(&0x910003fd_u32.to_le_bytes()); // mov x29,sp
    text.extend_from_slice(&0xf9400420_u32.to_le_bytes()); // ldr x0,[x1,#8]
    text.extend_from_slice(&0x52800041_u32.to_le_bytes()); // mov w1,#2
    text.extend_from_slice(&0xaa1f03e2_u32.to_le_bytes()); // mov x2,xzr
    let bl_dlopen = text.len() as u64;
    text.extend_from_slice(&0x94000000_u32.to_le_bytes()); // bl dlopen
    let cbz_fail99 = text.len() as u64;
    text.extend_from_slice(&0xb4000000_u32.to_le_bytes()); // cbz x0, fail99 (patched)
    text.extend_from_slice(&0xaa0003f3_u32.to_le_bytes()); // mov x19,x0
    let adrp_sym = text.len() as u64;
    text.extend_from_slice(&0x90000001_u32.to_le_bytes()); // adrp x1, symname
    let add_sym = text.len() as u64;
    text.extend_from_slice(&0x91000021_u32.to_le_bytes()); // add x1,x1,#lo12
    text.extend_from_slice(&0xaa1303e0_u32.to_le_bytes()); // mov x0,x19
    let bl_dlsym = text.len() as u64;
    text.extend_from_slice(&0x94000000_u32.to_le_bytes()); // bl dlsym
    let cbz_fail98 = text.len() as u64;
    text.extend_from_slice(&0xb4000000_u32.to_le_bytes()); // cbz x0, fail98 (patched)
    text.extend_from_slice(&0xaa0003f3_u32.to_le_bytes()); // mov x19,x0
    text.extend_from_slice(&0x528000a0_u32.to_le_bytes()); // mov w0,#5
    text.extend_from_slice(&0xd63f0260_u32.to_le_bytes()); // blr x19
    text.extend_from_slice(&0xa94153f3_u32.to_le_bytes()); // ldp x19,x20,[sp,#16]
    text.extend_from_slice(&0xa8c27bfd_u32.to_le_bytes()); // ldp x29,x30,[sp],#32
    text.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret
    let fail99 = text.len() as u64;
    text.extend_from_slice(&0x52800c60_u32.to_le_bytes()); // mov w0,#99
    text.extend_from_slice(&0xa94153f3_u32.to_le_bytes()); // ldp x19,x20,[sp,#16]
    text.extend_from_slice(&0xa8c27bfd_u32.to_le_bytes()); // ldp x29,x30,[sp],#32
    text.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret
    let fail98 = text.len() as u64;
    text.extend_from_slice(&0x52800c40_u32.to_le_bytes()); // mov w0,#98
    text.extend_from_slice(&0xa94153f3_u32.to_le_bytes()); // ldp x19,x20,[sp,#16]
    text.extend_from_slice(&0xa8c27bfd_u32.to_le_bytes()); // ldp x29,x30,[sp],#32
    text.extend_from_slice(&0xd65f03c0_u32.to_le_bytes()); // ret

    // Patch CBZ imm19: offset in instructions from the CBZ itself.
    let patch_cbz = |buf: &mut [u8], at: u64, target: u64| {
        let delta = (target as i64 - at as i64) / 4;
        assert!((-0x40000..0x40000).contains(&delta));
        let insn = 0xb4000000u32 | (((delta as u32) & 0x7ffff) << 5);
        buf[at as usize..at as usize + 4].copy_from_slice(&insn.to_le_bytes());
    };
    patch_cbz(&mut text, cbz_fail99, fail99);
    patch_cbz(&mut text, cbz_fail98, fail98);

    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    let sym_bytes = b"add42\0";
    let ro_idx = obj.add_section(Section::new_data(".rodata", sym_bytes.to_vec(), false, 1));
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "symname",
        SymbolBinding::Local,
        SymbolType::Object,
        ro_idx,
        0,
        sym_bytes.len() as u64,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("dlopen", 0));
    obj.add_symbol(Symbol::new_undefined("dlsym", 0));

    let mut sec = obj.sections[text_idx].clone();
    sec.relocations.push(Relocation::new(
        bl_dlopen,
        "dlopen",
        RelocationKind::AArch64Call26,
        0,
    ));
    sec.relocations.push(Relocation::new(
        adrp_sym,
        "symname",
        RelocationKind::AArch64Adrp,
        0,
    ));
    sec.relocations.push(Relocation::new(
        add_sym,
        "symname",
        RelocationKind::AArch64AddLo12,
        0,
    ));
    sec.relocations.push(Relocation::new(
        bl_dlsym,
        "dlsym",
        RelocationKind::AArch64Call26,
        0,
    ));
    obj.sections[text_idx] = sec;
    obj
}

// ─── TLV counter ─────────────────────────────────────────────────────────────

fn build_tlv_counter_x86(obj_path: PathBuf, target: &Target) -> ObjectFile {
    // movq _DESC@TLVP(%rip),%rdi ; callq *(%rdi) ; addl $42,(%rax) ; movl (%rax),%eax ; ret
    let text = vec![
        0x48, 0x8b, 0x3d, 0x00, 0x00, 0x00, 0x00, // movq → leaq by linker
        0xff, 0x17, // callq *(%rdi)
        0x83, 0x00, 0x2a, // addl $42, (%rax)
        0x8b, 0x00, // movl (%rax), %eax
        0xc3, // ret
    ];

    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let mut text_sec = Section::new_code(".text", text, 16);
    text_sec.relocations.push(Relocation::new(
        3,
        "_DESC",
        RelocationKind::X86_64TlvDescriptorRel32,
        -4,
    ));
    obj.add_section(text_sec);

    let mut tlv = Section::new_data("__thread_vars", vec![0u8; 24], true, 8);
    tlv.flags |= adesh_linker::section::flags::TLV_TABLE;
    tlv.relocations.push(Relocation::new(
        0,
        "__tlv_bootstrap",
        RelocationKind::Absolute64,
        0,
    ));
    tlv.relocations.push(Relocation::new(
        16,
        "_VAR$tlv$init",
        RelocationKind::Absolute64,
        0,
    ));
    obj.add_section(tlv);

    let mut tbss = Section::new_bss("__thread_bss", 8, 8);
    tbss.kind = adesh_linker::section::SectionKind::TBss;
    tbss.flags |= adesh_linker::section::flags::TLS;
    obj.add_section(tbss);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        15,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_VAR$tlv$init",
        SymbolBinding::Global,
        SymbolType::Tls,
        2,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_DESC",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        24,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("__tlv_bootstrap", 0));
    obj
}

fn build_tlv_counter_aarch64(obj_path: PathBuf, target: &Target) -> ObjectFile {
    // adrp/add desc; ldr thunk; blr; ldr/add/str *x0 += 42; return
    let text = vec![
        0x00, 0x00, 0x00, 0x90, // adrp x0, _DESC
        0x00, 0x00, 0x40, 0xf9, // ldr x0,[x0] → add
        0x01, 0x00, 0x40, 0xf9, // ldr x1,[x0]
        0x20, 0x00, 0x3f, 0xd6, // blr x1
        0x01, 0x00, 0x40, 0xb9, // ldr w1,[x0]
        0x21, 0xa8, 0x00, 0x11, // add w1,w1,#42
        0x01, 0x00, 0x00, 0xb9, // str w1,[x0]
        0xe0, 0x03, 0x01, 0x2a, // mov w0,w1
        0xc0, 0x03, 0x5f, 0xd6, // ret
    ];

    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let mut text_sec = Section::new_code(".text", text, 16);
    text_sec
        .relocations
        .push(Relocation::new(0, "_DESC", RelocationKind::AArch64Adrp, 0));
    text_sec.relocations.push(Relocation::new(
        4,
        "_DESC",
        RelocationKind::AArch64TlvDescriptorLo12,
        0,
    ));
    obj.add_section(text_sec);

    let mut tlv = Section::new_data("__thread_vars", vec![0u8; 24], true, 8);
    tlv.flags |= adesh_linker::section::flags::TLV_TABLE;
    tlv.relocations.push(Relocation::new(
        0,
        "__tlv_bootstrap",
        RelocationKind::Absolute64,
        0,
    ));
    tlv.relocations.push(Relocation::new(
        16,
        "_VAR$tlv$init",
        RelocationKind::Absolute64,
        0,
    ));
    obj.add_section(tlv);

    let mut tbss = Section::new_bss("__thread_bss", 8, 8);
    tbss.kind = adesh_linker::section::SectionKind::TBss;
    tbss.flags |= adesh_linker::section::flags::TLS;
    obj.add_section(tbss);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        36,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_VAR$tlv$init",
        SymbolBinding::Global,
        SymbolType::Tls,
        2,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "_DESC",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        24,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("__tlv_bootstrap", 0));
    obj
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[test]
fn tlv_thread_local_counter_executes_under_dyld() {
    let Some(triple) = macos_host_triple() else {
        return;
    };
    let target = Target::from_triple(triple).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let obj_path = dir.path().join("tlv.o");
    let out = dir.path().join("tlv_counter");

    let obj = if matches!(target.arch, Arch::AArch64) {
        build_tlv_counter_aarch64(obj_path.clone(), &target)
    } else {
        build_tlv_counter_x86(obj_path.clone(), &target)
    };
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();
    Linker::link(&[obj_path], LinkConfig::new(out.clone(), target)).expect("TLV link");

    assert_macho_magic(&out, false);
    codesign_verify(&out);

    let executed = run(&mut Command::new(&out));
    assert_eq!(
        executed.status.code(),
        Some(42),
        "TLV counter must return 0+42; got {executed:?}"
    );
}

#[test]
fn dylib_dlopen_dlsym_add42_executes_under_dyld() {
    let Some(triple) = macos_host_triple() else {
        return;
    };
    let target = Target::from_triple(triple).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let lib_obj_path = dir.path().join("add42.o");
    let lib_path = dir.path().join("libadd42.dylib");
    let lib_obj = build_add42(lib_obj_path.clone(), &target);
    ObjectWriter::write_to_file(&lib_obj, &lib_obj_path).unwrap();
    let mut lib_cfg = LinkConfig::new(lib_path.clone(), target.clone());
    lib_cfg.shared = true;
    lib_cfg.install_name = Some("@loader_path/libadd42.dylib".into());
    Linker::link(&[lib_obj_path], lib_cfg).expect("dylib link");
    assert_macho_magic(&lib_path, true);
    codesign_verify(&lib_path);

    let drv_obj_path = dir.path().join("driver.o");
    let drv_path = dir.path().join("dlopen_driver");
    let drv_obj = if matches!(target.arch, Arch::AArch64) {
        build_dlopen_driver_aarch64(drv_obj_path.clone(), &target)
    } else {
        build_dlopen_driver_x86(drv_obj_path.clone(), &target)
    };
    ObjectWriter::write_to_file(&drv_obj, &drv_obj_path).unwrap();
    Linker::link(&[drv_obj_path], LinkConfig::new(drv_path.clone(), target)).expect("driver link");
    assert_macho_magic(&drv_path, false);
    codesign_verify(&drv_path);

    let lib_arg = lib_path.to_string_lossy().into_owned();
    let executed = run(Command::new(&drv_path).arg(&lib_arg));
    assert_eq!(
        executed.status.code(),
        Some(47),
        "exit must be add42(5)==47; 99=dlopen failed, 98=dlsym failed\n{executed:?}"
    );
}
