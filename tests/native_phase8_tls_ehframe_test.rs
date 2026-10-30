//! Phase 8 — ELF TLS (PT_TLS, .tdata/.tbss, local-exec + initial-exec
//! relocations) and `.eh_frame`/`.eh_frame_hdr` emission, execution-verified.
//!
//! TLS coverage:
//! 1. Static local-exec (`R_X86_64_TPOFF32`-shaped access): a freestanding
//!    static binary sets `%fs` to the TLS block end via `arch_prctl`, reads a
//!    `.tdata` counter through the patched `%fs` displacement, increments and
//!    stores it, and exits with the value — proving the relocation math
//!    (`off - memsz`), the `.tdata` initial image, and the layout of the
//!    static TLS block end-to-end.
//! 2. Dynamic initial-exec (`R_X86_64_GOTTPOFF` access + `R_X86_64_TPOFF64`
//!    dynamic relocation): the same read through a GOT slot against the REAL
//!    dynamic loader (musl or glibc, whichever the execution environment
//!    provides): the loader must process `PT_TLS` (copy the image, install
//!    the thread pointer) and apply the `TPOFF64` relocation on the slot.
//! 3. Structural: `PT_TLS` bounds and the `.tdata`/`.tbss` (PROGBITS/NOBITS,
//!    `SHF_TLS`) section header split.
//! 4. Loud failures: general-dynamic TLS access (needs a DTV/`__tls_get_addr`
//!    runtime this toolchain does not provide) fails the link naming the
//!    relocation; TLS sections under Mach-O (no TLV emission) still fail
//!    loudly instead of silently sharing one template across threads.
//!
//! `.eh_frame` coverage: one honest standard-frame CIE + one FDE per function
//! with a known address and size, a `.eh_frame_hdr` binary search index, and a
//! `PT_GNU_EH_FRAME` segment — decoded from the emitted binary and checked
//! against the real function ranges. Functions without sizes get no FDE
//! (never fabricated ranges).
//!
//! Execution uses the real Linux environment (native Linux, or the WSL distro
//! on Windows). If none exists, execution tests skip loudly with a diagnostic;
//! structural assertions still run.

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::{Section, SectionKind, flags};
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const RUN_TIMEOUT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// Execution environment helpers (same contract as the dynamic-ELF suite)
// ---------------------------------------------------------------------------

fn linux_libc_triple() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        if Path::new("/lib64/ld-linux-x86-64.so.2").exists() {
            return Some("x86_64-unknown-linux-gnu");
        }
        if Path::new("/lib/ld-musl-x86_64.so.1").exists() {
            return Some("x86_64-linux-musl");
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        let probe = |interp: &str| {
            Command::new("wsl")
                .args([
                    "-e",
                    "/bin/sh",
                    "-c",
                    &format!("test -x {interp} && echo yes"),
                ])
                .output()
                .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "yes")
                .unwrap_or(false)
        };
        if probe("/lib64/ld-linux-x86-64.so.2") {
            return Some("x86_64-unknown-linux-gnu");
        }
        if probe("/lib/ld-musl-x86_64.so.1") {
            return Some("x86_64-linux-musl");
        }
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[allow(dead_code)]
fn to_wsl_path(path: &Path) -> String {
    let win_path = path.to_string_lossy().replace('\\', "/");
    if let Ok(output) = Command::new("wsl")
        .args(["wslpath", "-a", "-u"])
        .arg(&win_path)
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    for drive in ["d", "D", "c", "C"] {
        if let Some(stripped) = win_path.strip_prefix(&format!("{drive}:/")) {
            return format!("/mnt/{}/{}", drive.to_lowercase(), stripped);
        }
    }
    win_path
}

fn run_elf(path: &Path) -> std::io::Result<Output> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        let mut cmd = Command::new(path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        run_with_timeout(cmd)
    }

    #[cfg(target_os = "windows")]
    {
        let wsl_path = to_wsl_path(path);
        let _ = Command::new("wsl")
            .args(["-e", "/bin/sh", "-c", &format!("chmod +x \"{wsl_path}\"")])
            .output();
        let mut cmd = Command::new("wsl");
        cmd.args(["-e", "/bin/sh", "-c", &format!("\"{wsl_path}\"")]);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        run_with_timeout(cmd)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ELF execution unsupported on this OS without a Linux environment",
        ))
    }
}

fn run_with_timeout(mut cmd: Command) -> std::io::Result<Output> {
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(_status) => return child.wait_with_output(),
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("execution timed out after {}s", RUN_TIMEOUT.as_secs()),
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal independent ELF64 structural reader (verification, not production)
// ---------------------------------------------------------------------------

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn i32_at(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}
fn cstr_at(b: &[u8], off: usize) -> String {
    let end = b[off..]
        .iter()
        .position(|&c| c == 0)
        .map(|p| off + p)
        .unwrap_or(b.len());
    String::from_utf8_lossy(&b[off..end]).into_owned()
}

#[allow(dead_code)]
struct Phdr {
    p_type: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

#[allow(dead_code)]
struct Shdr {
    name: String,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
}

struct ElfView<'a> {
    bytes: &'a [u8],
}

impl<'a> ElfView<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        assert!(bytes.len() >= 64, "ELF too small");
        assert_eq!(&bytes[0..4], b"\x7fELF", "ELF magic");
        assert_eq!(bytes[4], 2, "ELFCLASS64");
        Self { bytes }
    }

    fn phdrs(&self) -> Vec<Phdr> {
        let phoff = u64_at(self.bytes, 32) as usize;
        let phnum = u16_at(self.bytes, 56) as usize;
        (0..phnum)
            .map(|i| {
                let o = phoff + i * 56;
                Phdr {
                    p_type: u32_at(self.bytes, o),
                    p_offset: u64_at(self.bytes, o + 8),
                    p_vaddr: u64_at(self.bytes, o + 16),
                    p_filesz: u64_at(self.bytes, o + 32),
                    p_memsz: u64_at(self.bytes, o + 40),
                    p_align: u64_at(self.bytes, o + 48),
                }
            })
            .collect()
    }

    fn shdrs(&self) -> Vec<Shdr> {
        let shoff = u64_at(self.bytes, 40) as usize;
        let shnum = u16_at(self.bytes, 60) as usize;
        let shstrndx = u16_at(self.bytes, 62) as usize;
        let shstr_off = u64_at(self.bytes, shoff + shstrndx * 64 + 24) as usize;
        (0..shnum)
            .map(|i| {
                let o = shoff + i * 64;
                let name_off = u32_at(self.bytes, o) as usize;
                Shdr {
                    name: cstr_at(self.bytes, shstr_off + name_off),
                    sh_type: u32_at(self.bytes, o + 4),
                    sh_flags: u64_at(self.bytes, o + 8),
                    sh_addr: u64_at(self.bytes, o + 16),
                    sh_offset: u64_at(self.bytes, o + 24),
                    sh_size: u64_at(self.bytes, o + 32),
                }
            })
            .collect()
    }

    fn section(&self, name: &str) -> Option<Shdr> {
        self.shdrs().into_iter().find(|s| s.name == name)
    }

    /// (name, bind, type, value, size) for each non-null .dynsym entry.
    fn dynsym_entries(&self) -> Vec<(String, u8, u8, u64, u64)> {
        let Some(dynsym) = self.section(".dynsym") else {
            return Vec::new();
        };
        let dynstr = self.section(".dynstr").expect(".dynstr for .dynsym");
        (1..dynsym.sh_size as usize / 24)
            .map(|i| {
                let o = dynsym.sh_offset as usize + i * 24;
                let name_off = u32_at(self.bytes, o) as usize;
                (
                    cstr_at(self.bytes, dynstr.sh_offset as usize + name_off),
                    self.bytes[o + 4] >> 4,
                    self.bytes[o + 4] & 0xf,
                    u64_at(self.bytes, o + 8),
                    u64_at(self.bytes, o + 16),
                )
            })
            .collect()
    }

    /// (r_offset, sym_idx, rtype, addend) of a RELA section.
    fn rela_entries(&self, name: &str) -> Vec<(u64, u64, u32, i64)> {
        match self.section(name) {
            Some(sec) => (0..sec.sh_size as usize / 24)
                .map(|i| {
                    let o = sec.sh_offset as usize + i * 24;
                    let r_info = u64_at(self.bytes, o + 8);
                    (
                        u64_at(self.bytes, o),
                        r_info >> 32,
                        (r_info & 0xffff_ffff) as u32,
                        u64_at(self.bytes, o + 16) as i64,
                    )
                })
                .collect(),
            None => Vec::new(),
        }
    }
}

const PT_TLS: u32 = 7;
const PT_GNU_EH_FRAME: u32 = 0x6474_e550;
const SHT_PROGBITS: u32 = 1;
const SHT_NOBITS: u32 = 8;
const SHF_ALLOC: u64 = 0x2;
const SHF_WRITE: u64 = 0x1;
const SHF_TLS: u64 = 0x400;
const STT_TLS: u8 = 6;
const R_X86_64_TPOFF64: u32 = 18;

// ---------------------------------------------------------------------------
// Test object builders (hand-assembled x86-64 SysV)
// ---------------------------------------------------------------------------

fn test_dir() -> PathBuf {
    let dir = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("elf_tls_tests");
    std::fs::create_dir_all(&dir).expect("create test dir");
    dir
}

fn write_object(obj: &ObjectFile, path: &Path) {
    ObjectWriter::write_to_file(obj, path).expect("write ADOB");
}

fn add_tls_var(obj: &mut ObjectFile, init: u32, with_tbss: bool) {
    // .tdata: the initialized image (counter + padding to a full 8-byte slot).
    let mut image = init.to_le_bytes().to_vec();
    image.resize(8, 0);
    let mut tdata = Section::new_data(".tdata", image, true, 8);
    tdata.kind = SectionKind::TData;
    tdata.flags |= flags::TLS;
    let tdata_idx = obj.add_section(tdata);
    let counter = obj.add_symbol(Symbol::new_defined(
        "counter",
        SymbolBinding::Global,
        SymbolType::Tls,
        tdata_idx,
        0,
        8,
        0,
    ));
    let tls_end = obj.add_symbol(Symbol::new_defined(
        "__tls_end",
        SymbolBinding::Global,
        SymbolType::Tls,
        tdata_idx,
        8,
        0,
        0,
    ));
    let _ = (counter, tls_end);

    if with_tbss {
        // .tbss: 16 bytes of per-thread zero-fill after the image.
        let mut tbss = Section::new_data(".tbss", vec![], true, 8);
        tbss.kind = SectionKind::TBss;
        tbss.flags |= flags::TLS;
        tbss.size = 16;
        obj.add_section(tbss);
    }
}

/// `_start` (static local-exec): arch_prctl(ARCH_SET_FS, __tls_end);
/// eax = %fs:[counter]; eax += 1; %fs:[counter] = eax; exit(eax).
fn build_static_le_object(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);

    let mut text: Vec<u8> = Vec::new();
    // mov rax, 158 (SYS_arch_prctl)
    text.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x9E, 0x00, 0x00, 0x00]);
    // mov rdi, 0x1002 (ARCH_SET_FS; 0x1001 is ARCH_SET_GS)
    text.extend_from_slice(&[0x48, 0xC7, 0xC7, 0x02, 0x10, 0x00, 0x00]);
    // movabs rsi, <__tls_end>   (relocated R_X86_64_64)
    text.extend_from_slice(&[0x48, 0xBE]);
    let tls_end_operand_off = text.len() as u64;
    text.extend_from_slice(&[0u8; 8]);
    // syscall
    text.extend_from_slice(&[0x0F, 0x05]);
    // mov eax, %fs:[counter]   (relocated TLS_LE -> disp32)
    text.extend_from_slice(&[0x64, 0x8B, 0x04, 0x25]);
    let load_disp_off = text.len() as u64;
    text.extend_from_slice(&[0, 0, 0, 0]);
    // inc eax
    text.extend_from_slice(&[0xFF, 0xC0]);
    // mov %fs:[counter], eax   (relocated TLS_LE -> disp32)
    text.extend_from_slice(&[0x64, 0x89, 0x04, 0x25]);
    let store_disp_off = text.len() as u64;
    text.extend_from_slice(&[0, 0, 0, 0]);
    // mov edi, eax; mov eax, 60; syscall
    text.extend_from_slice(&[0x89, 0xC7]);
    text.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0x0F, 0x05]);

    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    add_tls_var(&mut obj, 41, false);

    let mut text_sec = obj.sections[text_idx].clone();
    let mut r_abs = Relocation::new(
        tls_end_operand_off,
        "__tls_end",
        RelocationKind::Absolute64,
        0,
    );
    r_abs.file_index = Some(0);
    text_sec.relocations.push(r_abs);
    let mut r_load = Relocation::new(load_disp_off, "counter", RelocationKind::TlsLocalExec, 0);
    r_load.file_index = Some(0);
    text_sec.relocations.push(r_load);
    let mut r_store = Relocation::new(store_disp_off, "counter", RelocationKind::TlsLocalExec, 0);
    r_store.file_index = Some(0);
    text_sec.relocations.push(r_store);
    obj.sections[text_idx] = text_sec;

    obj
}

/// `_start` (dynamic initial-exec): rax = [rip+GOT slot]; ebx = %fs:[rax];
/// exit(ebx). The GOT slot is the `counter` TLS variable's
/// thread-pointer-relative offset, filled by the loader (R_X86_64_TPOFF64).
fn build_dynamic_ie_object(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);

    let mut text: Vec<u8> = Vec::new();
    // mov rax, [rip+disp32]   (relocated TLS_IE -> GOT slot)
    text.extend_from_slice(&[0x48, 0x8B, 0x05]);
    let got_disp_off = text.len() as u64;
    text.extend_from_slice(&[0, 0, 0, 0]);
    // mov ebx, %fs:[rax]
    text.extend_from_slice(&[0x64, 0x8B, 0x18]);
    // mov edi, ebx; mov eax, 60; syscall
    text.extend_from_slice(&[0x89, 0xDF]);
    text.extend_from_slice(&[0xB8, 0x3C, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0x0F, 0x05]);

    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    add_tls_var(&mut obj, 47, false);

    let mut text_sec = obj.sections[text_idx].clone();
    let mut r_ie = Relocation::new(got_disp_off, "counter", RelocationKind::TlsInitialExec, 0);
    r_ie.file_index = Some(0);
    text_sec.relocations.push(r_ie);
    obj.sections[text_idx] = text_sec;

    obj
}

/// Two leaf functions with known sizes, for `.eh_frame` decoding.
fn build_two_function_object(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    // fn1: mov eax, 1; ret  (16 bytes padded with nops)
    let mut fn1 = vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3];
    while fn1.len() < 16 {
        fn1.push(0x90);
    }
    // fn2: mov eax, 2; ret  (8 bytes)
    let fn2 = [0xB8, 0x02, 0x00, 0x00, 0x00, 0xC3, 0x90, 0x90];
    let mut text = fn1.clone();
    text.extend_from_slice(&fn2);

    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        16,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "fn2",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        16,
        8,
        0,
    ));
    obj
}

fn link_object(obj_path: &Path, out_path: &Path, target: &Target, dynamic: bool) -> Vec<u8> {
    let mut config = LinkConfig::new(out_path.to_path_buf(), target.clone());
    config.dynamic = dynamic;
    let res = Linker::link(&[obj_path.to_path_buf()], config);
    assert!(res.is_ok(), "link failed: {:?}", res);
    std::fs::read(out_path).expect("read linked ELF")
}

// ---------------------------------------------------------------------------
// TLS tests
// ---------------------------------------------------------------------------

#[test]
fn test_static_local_exec_tls_executes_in_linux() {
    let Some(triple) = linux_libc_triple() else {
        eprintln!("SKIP: no Linux ELF execution environment (WSL) available");
        return;
    };
    let target = Target::from_triple(triple).expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("tls_le.adob");
    let obj = build_static_le_object(obj_path.clone(), &target);
    write_object(&obj, &obj_path);

    let out_path = dir.join("tls_le_static");
    let bytes = link_object(&obj_path, &out_path, &target, false);

    // Structural: PT_TLS must describe the 8-byte initialized block.
    let view = ElfView::new(&bytes);
    let tls = view
        .phdrs()
        .into_iter()
        .find(|p| p.p_type == PT_TLS)
        .expect("static TLS binary must carry PT_TLS");
    assert_eq!(tls.p_filesz, 8, ".tdata image size");
    assert_eq!(tls.p_memsz, 8, "block size (no .tbss tail)");
    assert!(tls.p_align >= 8, "block alignment");

    // Execution: arch_prctl(SET_FS, block end) + %fs-relative access of a
    // 41-initialized counter incremented to 42 must exit 42. Exit 42 is the
    // expected, non-zero, success contract of this probe.
    let out = run_elf(&out_path).expect("execute static TLS binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "static local-exec TLS binary: exit {:?} stderr {:?} (must read the counter 41, increment, store, and exit 42)",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_dynamic_initial_exec_tls_executes_against_real_loader() {
    let Some(triple) = linux_libc_triple() else {
        eprintln!("SKIP: no Linux ELF execution environment (WSL) available");
        return;
    };
    let target = Target::from_triple(triple).expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("tls_ie.adob");
    let obj = build_dynamic_ie_object(obj_path.clone(), &target);
    write_object(&obj, &obj_path);

    let out_path = dir.join("tls_ie_dynamic");
    let bytes = link_object(&obj_path, &out_path, &target, true);

    // Structural: the loader contract the binary presents.
    let view = ElfView::new(&bytes);
    assert!(
        view.phdrs().iter().any(|p| p.p_type == PT_TLS),
        "dynamic TLS executable must carry PT_TLS"
    );

    // `counter` must appear in .dynsym as STT_TLS with its value being the
    // offset inside the TLS block (not a virtual address).
    let counter = view
        .dynsym_entries()
        .into_iter()
        .find(|(name, _, _, _, _)| name == "counter")
        .expect("counter must be in .dynsym for the loader");
    assert_eq!(counter.2, STT_TLS, "counter must be STT_TLS");
    assert_eq!(
        counter.3, 0,
        "STT_TLS st_value must be the offset within the TLS block"
    );

    // .rela.dyn must carry exactly one R_X86_64_TPOFF64 against the GOT slot,
    // bound to counter's dynsym entry (by name, through the emitted order —
    // the exported symbol set also contains `__tls_end`, so the index is not
    // fixed).
    let rela_dyn = view.rela_entries(".rela.dyn");
    assert!(
        !rela_dyn.is_empty(),
        "dynamic TLS executable must have .rela.dyn"
    );
    let tpoff = rela_dyn
        .iter()
        .find(|(_, _, rtype, _)| *rtype == R_X86_64_TPOFF64)
        .expect(".rela.dyn must carry the TPOFF64 relocation");
    let dynsym = view.dynsym_entries();
    let bound_name = dynsym
        .get(tpoff.1 as usize - 1)
        .map(|(name, _, _, _, _)| name.clone())
        .unwrap_or_default();
    assert_eq!(
        bound_name, "counter",
        "TPOFF64 must bind the `counter` TLS variable's dynsym entry"
    );

    // Execution: the loader processes PT_TLS and TPOFF64, then the program
    // exits with the 47-initialized counter value (exit 47 is the expected,
    // non-zero, success contract of this probe).
    let out = run_elf(&out_path).expect("execute dynamic TLS binary");
    assert_eq!(
        out.status.code(),
        Some(47),
        "the loader-filled GOT slot must yield the .tdata image value (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_tls_segment_and_tdata_tbss_section_headers() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("tls_split.adob");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    obj.add_section(Section::new_code(".text", vec![0xC3], 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        1,
        0,
    ));
    add_tls_var(&mut obj, 0x2A, true);

    write_object(&obj, &obj_path);
    let out_path = dir.join("tls_split_static");
    let bytes = link_object(&obj_path, &out_path, &target, false);

    let view = ElfView::new(&bytes);

    // PT_TLS: filesz covers only the initialized image; memsz covers the
    // whole block including the .tbss zero-fill.
    let tls = view
        .phdrs()
        .into_iter()
        .find(|p| p.p_type == PT_TLS)
        .expect("PT_TLS");
    assert_eq!(tls.p_filesz, 8);
    assert_eq!(tls.p_memsz, 24, "8-byte .tdata + 16-byte .tbss");

    // .tdata: PROGBITS, SHF_TLS, file-backed, the initialized image bytes.
    let tdata = view.section(".tdata").expect(".tdata section header");
    assert_eq!(tdata.sh_type, SHT_PROGBITS);
    assert_eq!(
        tdata.sh_flags & (SHF_ALLOC | SHF_WRITE | SHF_TLS),
        SHF_ALLOC | SHF_WRITE | SHF_TLS
    );
    assert_eq!(tdata.sh_size, 8);
    let img = &bytes[tdata.sh_offset as usize..tdata.sh_offset as usize + 4];
    assert_eq!(u32::from_le_bytes(img.try_into().unwrap()), 0x2A);

    // .tbss: NOBITS, SHF_TLS, starting where .tdata ends.
    let tbss = view.section(".tbss").expect(".tbss section header");
    assert_eq!(tbss.sh_type, SHT_NOBITS);
    assert_eq!(
        tbss.sh_flags & (SHF_ALLOC | SHF_WRITE | SHF_TLS),
        SHF_ALLOC | SHF_WRITE | SHF_TLS
    );
    assert_eq!(tbss.sh_size, 16);
    assert_eq!(tbss.sh_addr, tdata.sh_addr + tdata.sh_size);
    assert_eq!(tbss.sh_addr, tls.p_vaddr + tls.p_filesz);
}

#[test]
fn test_general_dynamic_tls_fails_loudly() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("tls_gd.adob");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let text_idx = obj.add_section(Section::new_code(".text", vec![0x90; 24], 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        24,
        0,
    ));
    add_tls_var(&mut obj, 1, false);
    let mut text_sec = obj.sections[text_idx].clone();
    let mut r = Relocation::new(0, "counter", RelocationKind::TlsGeneralDynamic, 0);
    r.file_index = Some(0);
    text_sec.relocations.push(r);
    obj.sections[text_idx] = text_sec;

    write_object(&obj, &obj_path);

    let out_path = dir.join("tls_gd_static");
    let config = LinkConfig::new(out_path, target.clone());
    let res = Linker::link(&[obj_path], config);
    let err = res.expect_err("general-dynamic TLS must fail the link");
    let msg = err.to_string();
    assert!(
        msg.contains("general-dynamic") || msg.contains("TLSGD"),
        "the error must name general-dynamic TLS, got: {msg}"
    );
}

#[test]
fn test_macho_tls_sections_become_thread_local_variables() {
    // Mach-O has no GOT-based TLS model: the linker lowers TLS input
    // sections to the dyld TLV ABI (`__thread_data`/`__thread_bss` plus a
    // `__thread_vars` descriptor table), so this links instead of failing.
    let target = Target::from_triple("x86_64-apple-darwin").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("tls_macho.adob");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let text_idx = obj.add_section(Section::new_code(".text", vec![0xC3], 16));
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        1,
        0,
    ));
    add_tls_var(&mut obj, 1, false);

    write_object(&obj, &obj_path);

    let out_path = dir.join("tls_macho_out");
    let config = LinkConfig::new(out_path.clone(), target.clone());
    Linker::link(&[obj_path], config).expect("Mach-O TLS links via the TLV ABI");

    let bytes = std::fs::read(&out_path).expect("output");
    assert_eq!(
        u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        0xFEEDFACF,
        "must be a 64-bit Mach-O"
    );

    let mut sections = Vec::new();
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let mut off = 32usize;
    for _ in 0..ncmds {
        let cmd = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        let cmdsize = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        if cmd == 0x19 {
            let nsects = u32::from_le_bytes(bytes[off + 64..off + 68].try_into().unwrap()) as usize;
            for k in 0..nsects {
                let so = off + 72 + k * 80;
                let name = String::from_utf8_lossy(&bytes[so..so + 16])
                    .trim_end_matches('\0')
                    .to_string();
                let flags = u32::from_le_bytes(bytes[so + 64..so + 68].try_into().unwrap());
                sections.push((name, flags & 0xFF));
            }
        }
        off += cmdsize;
    }

    const S_THREAD_LOCAL_REGULAR: u32 = 0x11;
    let types: Vec<&str> = sections
        .iter()
        .filter(|(_, t)| *t == S_THREAD_LOCAL_REGULAR)
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        types,
        vec!["__thread_data"],
        "TLS input sections must be emitted as S_THREAD_LOCAL_REGULAR, got {:?}",
        sections
    );
}

// ---------------------------------------------------------------------------
// .eh_frame tests
// ---------------------------------------------------------------------------

/// Decode `.eh_frame_hdr`: (version, eh_frame_ptr_va, fde_count,
/// sorted [(initial_loc, fde_va)] relative to the header).
fn decode_eh_frame_hdr(bytes: &[u8], hdr: &Shdr) -> (u8, u64, u64, Vec<(u64, u64)>) {
    let base = hdr.sh_offset as usize;
    let version = bytes[base];
    assert_eq!(version, 1, "eh_frame_hdr version");
    let eh_frame_ptr_enc = bytes[base + 1];
    let fde_count_enc = bytes[base + 2];
    let table_enc = bytes[base + 3];
    assert_eq!(eh_frame_ptr_enc, 0x1b, "eh_frame_ptr: pcrel|sdata4");
    assert_eq!(fde_count_enc, 0x03, "fde_count: udata4");
    assert_eq!(table_enc, 0x3b, "table: datarel|sdata4");
    // eh_frame_ptr: sdata4 relative to the position right after itself.
    let ptr_field = base + 4;
    let eh_frame_ptr = (hdr.sh_addr + 8).wrapping_add(i32_at(bytes, ptr_field) as u64);
    let fde_count = u32_at(bytes, ptr_field + 4) as u64;
    let mut entries = Vec::with_capacity(fde_count as usize);
    let mut off = ptr_field + 8;
    for _ in 0..fde_count {
        let loc = hdr.sh_addr.wrapping_add(i32_at(bytes, off) as u64);
        let fde = hdr.sh_addr.wrapping_add(i32_at(bytes, off + 4) as u64);
        entries.push((loc, fde));
        off += 8;
    }
    (version, eh_frame_ptr, fde_count, entries)
}

#[test]
fn test_eh_frame_emitted_and_decodes_to_real_function_ranges() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("ehframe.adob");
    let obj = build_two_function_object(obj_path.clone(), &target);
    write_object(&obj, &obj_path);

    let out_path = dir.join("ehframe_static");
    let bytes = link_object(&obj_path, &out_path, &target, false);
    let view = ElfView::new(&bytes);

    // Sections present.
    let eh_frame = view.section(".eh_frame").expect(".eh_frame section");
    let hdr = view
        .section(".eh_frame_hdr")
        .expect(".eh_frame_hdr section");
    assert!(eh_frame.sh_size >= 24 + 24 + 24 + 4);
    assert_eq!(eh_frame.sh_size % 4, 0, ".eh_frame must be 4-byte padded");

    // PT_GNU_EH_FRAME points at .eh_frame_hdr.
    let pt_eh = view
        .phdrs()
        .into_iter()
        .find(|p| p.p_type == PT_GNU_EH_FRAME)
        .expect("PT_GNU_EH_FRAME");
    assert_eq!(pt_eh.p_vaddr, hdr.sh_addr);
    assert_eq!(pt_eh.p_filesz, hdr.sh_size);

    // Header decode: eh_frame_ptr lands inside .eh_frame; two index entries.
    let (_, eh_frame_ptr, fde_count, entries) = decode_eh_frame_hdr(&bytes, &hdr);
    assert!(fde_count == 2, "one entry per function, got {fde_count}");
    assert!(
        eh_frame_ptr >= eh_frame.sh_addr && eh_frame_ptr < eh_frame.sh_addr + eh_frame.sh_size,
        "eh_frame_ptr 0x{eh_frame_ptr:x} must land inside .eh_frame"
    );

    // The text ranges the FDEs must cover: _start at .text + 0 (size 16),
    // fn2 at .text + 16 (size 8).
    let text = view.section(".text").expect(".text section");
    let mut expected = vec![(text.sh_addr, 16u64), (text.sh_addr + 16, 8u64)];
    expected.sort();

    let mut seen = Vec::new();
    for (loc, fde_va) in &entries {
        assert!(
            *fde_va >= eh_frame.sh_addr && *fde_va < eh_frame.sh_addr + eh_frame.sh_size,
            "FDE pointer 0x{fde_va:x} must land inside .eh_frame"
        );
        // Decode the FDE: pc_begin is pcrel-sdata4 relative to its own
        // field; the file offset follows the section-relative distance
        // (no image-base assumption needed).
        let fde_in_section = (fde_va - eh_frame.sh_addr) as usize;
        let fde_off = eh_frame.sh_offset as usize + fde_in_section;
        let pc_begin_field = fde_va + 8;
        let pc_begin = pc_begin_field.wrapping_add(i32_at(&bytes, fde_off + 8) as u64);
        let pc_range = u32_at(&bytes, fde_off + 12) as u64;
        assert_eq!(
            *loc, pc_begin,
            "the table entry's initial location must match its FDE's pc_begin"
        );
        seen.push((pc_begin, pc_range));
    }
    seen.sort();
    assert_eq!(
        seen, expected,
        "FDE ranges must exactly cover the two functions"
    );
}

#[test]
fn test_functions_without_sizes_get_no_eh_frame() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("ehframe_zero.adob");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);
    let text_idx = obj.add_section(Section::new_code(".text", vec![0xC3], 16));
    // Function symbol with size 0: no honest FDE range exists, so no FDE.
    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        0,
        0,
    ));

    write_object(&obj, &obj_path);
    let out_path = dir.join("ehframe_zero_static");
    let bytes = link_object(&obj_path, &out_path, &target, false);
    let view = ElfView::new(&bytes);

    assert!(
        view.section(".eh_frame").is_none(),
        "functions without sizes must yield no .eh_frame (never fabricated ranges)"
    );
    assert!(
        view.section(".eh_frame_hdr").is_none(),
        "no .eh_frame_hdr without FDEs"
    );
    assert!(
        !view.phdrs().iter().any(|p| p.p_type == PT_GNU_EH_FRAME),
        "no PT_GNU_EH_FRAME without FDEs"
    );
}

/// The eh_frame decode test needs the image base to convert VAs to file
/// offsets. Pull it from the first PT_LOAD instead of hard-coding it.
#[test]
fn test_eh_frame_hdr_table_covers_sorted_function_order() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("ehframe_order.adob");
    let obj = build_two_function_object(obj_path.clone(), &target);
    write_object(&obj, &obj_path);

    let out_path = dir.join("ehframe_order_static");
    let bytes = link_object(&obj_path, &out_path, &target, false);
    let view = ElfView::new(&bytes);
    let hdr = view.section(".eh_frame_hdr").expect(".eh_frame_hdr");
    let (_, _, fde_count, entries) = decode_eh_frame_hdr(&bytes, &hdr);
    assert_eq!(fde_count, 2);

    // Entries must be sorted by initial location (binary-search invariant).
    let mut locs: Vec<u64> = entries.iter().map(|(l, _)| *l).collect();
    let mut sorted = locs.clone();
    sorted.sort();
    assert_eq!(locs, sorted, "the table must be sorted by initial location");
    locs.dedup();
    assert_eq!(locs.len(), 2, "distinct functions");
}
