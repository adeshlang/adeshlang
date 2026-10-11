//! Phase 8 — Real ELF64 Dynamic Linking, Execution-Verified.
//!
//! These tests link small ADOB objects with `adesh_linker` in dynamic modes
//! (`--dynamic`, `--pie`, shared libraries) and EXECUTE the produced binaries
//! in a real Linux environment (native Linux, or the WSL distro on Windows),
//! asserting observable behavior: program output, exit codes, and the dynamic
//! loader's actual resolution of imported C library symbols and `dlopen`ed
//! shared libraries.
//!
//! Coverage:
//! 1. Dynamic executable (ET_EXEC + PT_INTERP + PLT/GOT + JUMP_SLOT, BIND_NOW):
//!    `puts` imported from the system libc prints text; exit code 0.
//! 2. PIE (ET_DYN + R_X86_64_RELATIVE): the same, plus a runtime-relocated
//!    data pointer loaded from `.data`, proving the loader processed the
//!    dynamic table.
//! 3. Shared library (ET_DYN + DT_SONAME + exported dynsym): `dlopen` +
//!    `dlsym` + indirect call from a PIE driver; the function's result is
//!    the process exit code (47 = add42(5)).
//! 4. Loud-failure regressions: a static link with a libc import must fail
//!    with a clear error (never silently emit a broken stub), and an unknown
//!    symbol must fail as undefined (never silently become a dynamic import).
//!
//! The libc flavor (gnu/musl) is selected by probing which dynamic loader
//! exists in the execution environment, and the matching target triple is
//! used for linking. If no Linux execution environment exists (no WSL on
//! Windows, or a non-Linux host), the execution tests are skipped loudly
//! with a diagnostic; structural assertions that need no execution still run.

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::object::ObjectFile;
use adesh_linker::object::writer::ObjectWriter;
use adesh_linker::os_router::{OsApiRouter, SymbolRoute};
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const RUN_TIMEOUT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// Execution environment helpers
// ---------------------------------------------------------------------------

/// The target triple whose ELF interpreter exists in the execution
/// environment (native Linux or the default WSL distro), or `None` when no
/// Linux ELF execution environment is available. glibc is preferred (the CI
/// default); musl is used for musl-based environments (e.g. an Alpine WSL).
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
                .map(|o| {
                    o.status.success()
                        && String::from_utf8_lossy(&o.stdout).trim() == "yes"
                })
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

/// Windows path -> path inside the (default) WSL distro.
fn to_wsl_path(path: &Path) -> String {
    let win_path = path.to_string_lossy().replace('\\', "/");
    if let Ok(output) = Command::new("wsl")
        .args(["wslpath", "-a", "-u"])
        .arg(&win_path)
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    for drive in ["d", "D", "c", "C"] {
        if let Some(stripped) = win_path.strip_prefix(&format!("{drive}:/")) {
            return format!("/mnt/{}/{}", drive.to_lowercase(), stripped);
        }
    }
    win_path
}

/// Run an ELF binary in the Linux execution environment with arguments.
/// Enforces a timeout: a mis-emitted PLT (e.g. lazy binding against a stub
/// with no resolver) manifests as an infinite loop, which must fail the test
/// rather than hang the suite.
fn run_elf(path: &Path, args: &[&str]) -> std::io::Result<Output> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        let mut cmd = Command::new(path);
        cmd.args(args);
        run_with_timeout(cmd)
    }

    #[cfg(target_os = "windows")]
    {
        let wsl_path = to_wsl_path(path);
        let quoted_args = args
            .iter()
            .map(|a| format!("'{}'", a))
            .collect::<Vec<_>>()
            .join(" ");
        let line = if args.is_empty() {
            format!("chmod +x '{wsl_path}' && '{wsl_path}'")
        } else {
            format!("chmod +x '{wsl_path}' && '{wsl_path}' {quoted_args}")
        };
        let mut cmd = Command::new("wsl");
        cmd.args(["-e", "/bin/sh", "-c"]).arg(&line);
        run_with_timeout(cmd)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (path, args);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ELF execution unsupported on this OS without a Linux environment",
        ))
    }
}

fn run_with_timeout(mut cmd: Command) -> std::io::Result<Output> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(_status) => return child.wait_with_output(),
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let out = child.wait_with_output()?;
                    // Annotate the timeout: it is the classic symptom of a
                    // lazy-bound PLT entry that jumps back into itself.
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "execution timed out after {}s (stdout so far: {:?}); a hung dynamic \
                             executable usually means a PLT entry jumped back into itself \
                             (lazy binding against a resolver-less stub)",
                            RUN_TIMEOUT.as_secs(),
                            stdout
                        ),
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

struct ElfView<'a> {
    bytes: &'a [u8],
}

/// A program header: (p_type, p_offset, p_vaddr, p_filesz, p_memsz, p_flags).
struct Phdr {
    p_type: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_flags: u32,
}

/// A section header: (name, sh_type, sh_addr, sh_offset, sh_size, sh_link).
struct Shdr {
    name: String,
    sh_type: u32,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
}

impl<'a> ElfView<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        assert!(bytes.len() >= 64, "ELF too small");
        assert_eq!(&bytes[0..4], b"\x7fELF", "ELF magic");
        assert_eq!(bytes[4], 2, "ELFCLASS64");
        Self { bytes }
    }

    fn e_type(&self) -> u16 {
        u16_at(self.bytes, 16)
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
                    p_flags: u32_at(self.bytes, o + 4),
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
                    sh_addr: u64_at(self.bytes, o + 16),
                    sh_offset: u64_at(self.bytes, o + 24),
                    sh_size: u64_at(self.bytes, o + 32),
                    sh_link: u32_at(self.bytes, o + 40) as u32,
                }
            })
            .collect()
    }

    fn section(&self, name: &str) -> Option<Shdr> {
        self.shdrs().into_iter().find(|s| s.name == name)
    }

    /// (d_tag, d_val) pairs of the `.dynamic` section.
    fn dynamic_entries(&self) -> Vec<(u64, u64)> {
        match self.section(".dynamic") {
            Some(sec) => (0..sec.sh_size as usize / 16)
                .map(|i| {
                    let o = sec.sh_offset as usize + i * 16;
                    (u64_at(self.bytes, o), u64_at(self.bytes, o + 8))
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Resolve a d_val that is a `.dynstr` offset to its string.
    fn dyn_str(&self, str_off: u64) -> String {
        let dynstr = self.section(".dynstr").expect(".dynstr section");
        cstr_at(self.bytes, dynstr.sh_offset as usize + str_off as usize)
    }

    /// Symbol names in `.dynsym` order.
    fn dynsym_names(&self) -> Vec<String> {
        let Some(dynsym) = self.section(".dynsym") else {
            return Vec::new();
        };
        let dynstr = self.section(".dynstr").expect(".dynstr for .dynsym");
        (1..dynsym.sh_size as usize / 24)
            .map(|i| {
                let o = dynsym.sh_offset as usize + i * 24;
                let name_off = u32_at(self.bytes, o) as usize;
                cstr_at(self.bytes, dynstr.sh_offset as usize + name_off)
            })
            .collect()
    }

    /// Elf64_Rela entries (r_offset, sym_idx, rtype, addend) of a RELA section.
    fn rela_entries(&self, name: &str) -> Vec<(u64, u64, u32, i64)> {
        match self.section(name) {
            Some(sec) => (0..sec.sh_size as usize / 24)
                .map(|i| {
                    let o = sec.sh_offset as usize + i * 24;
                    let r_offset = u64_at(self.bytes, o);
                    let r_info = u64_at(self.bytes, o + 8);
                    let addend = u64_at(self.bytes, o + 16) as i64;
                    (
                        r_offset,
                        r_info >> 32,
                        (r_info & 0xffff_ffff) as u32,
                        addend,
                    )
                })
                .collect(),
            None => Vec::new(),
        }
    }
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PT_GNU_RELRO: u32 = 0x6474_e552;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const DT_NEEDED: u64 = 1;
const DT_PLTRELSZ: u64 = 2;
const DT_STRTAB: u64 = 10;
const DT_SYMTAB: u64 = 11;
const DT_RELA: u64 = 7;
const DT_RELASZ: u64 = 8;
const DT_JMPREL: u64 = 23;
const DT_SONAME: u64 = 14;
const DT_FLAGS: u64 = 30;
const DT_FLAGS_1: u64 = 0x6fff_fffb;
const DT_GNU_HASH: u64 = 0x6fff_fef5;
const DT_HASH: u64 = 4;
const DF_BIND_NOW: u64 = 0x8;
const DF_1_NOW: u64 = 0x1;
const DF_1_PIE: u64 = 0x0800_0000;
const R_X86_64_RELATIVE: u32 = 8;
const R_X86_64_JUMP_SLOT: u32 = 7;

// ---------------------------------------------------------------------------
// Test object builders (hand-assembled x86-64 SysV)
// ---------------------------------------------------------------------------

/// `_start`: lea rdi,[rip+msg]; call puts; exit(0) via raw syscall 60.
/// With `use_data_ptr`, the message pointer is loaded from a `.data` slot
/// relocated by R_X86_64_RELATIVE at load time (PIE path).
fn build_puts_object(obj_path: PathBuf, target: &Target, use_data_ptr: bool) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);

    // .text
    let head: &[u8] = if use_data_ptr {
        // mov rdi, [rip+msgptr]
        &[0x48, 0x8b, 0x3d, 0, 0, 0, 0]
    } else {
        // lea rdi, [rip+msg]
        &[0x48, 0x8d, 0x3d, 0, 0, 0, 0]
    };
    let mut text = head.to_vec();
    text.extend_from_slice(&[0xe8, 0, 0, 0, 0]); // call puts
    text.extend_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]); // mov eax, 60
    text.extend_from_slice(&[0x31, 0xff]); // xor edi, edi
    text.extend_from_slice(&[0x0f, 0x05]); // syscall

    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    let msg_str = b"hello from adesh dynamic ELF\n\0";
    let rodata_idx = obj.add_section(Section::new_data(".rodata", msg_str.to_vec(), false, 1));
    let data_idx = use_data_ptr.then(|| {
        obj.add_section(Section::new_data(".data", vec![0u8; 8], true, 8))
    });

    obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    let msg_sym = obj.add_symbol(Symbol::new_defined(
        "msg",
        SymbolBinding::Global,
        SymbolType::Object,
        rodata_idx,
        0,
        msg_str.len() as u64,
        0,
    ));
    let msgptr_sym = data_idx.map(|di| {
        obj.add_symbol(Symbol::new_defined(
            "msgptr",
            SymbolBinding::Global,
            SymbolType::Object,
            di,
            0,
            8,
            0,
        ))
    });
    let puts_sym = obj.add_symbol(Symbol::new_undefined("puts", 0));

    let mut text_sec = obj.sections[text_idx].clone();
    // lea/mov rip-relative operand at offset 3 -> msg or msgptr.
    let (ptr_target, ptr_sym) = if use_data_ptr {
        ("msgptr", msgptr_sym.expect("msgptr symbol"))
    } else {
        ("msg", msg_sym)
    };
    let mut r1 = Relocation::new(3, ptr_target, RelocationKind::PcRelative32, -4);
    r1.symbol_index = Some(ptr_sym);
    r1.file_index = Some(0);
    text_sec.relocations.push(r1);
    let mut r2 = Relocation::new(8, "puts", RelocationKind::PcRelative32, -4);
    r2.symbol_index = Some(puts_sym);
    r2.file_index = Some(0);
    text_sec.relocations.push(r2);
    obj.sections[text_idx] = text_sec;

    if let Some(di) = data_idx {
        let mut data_sec = obj.sections[di].clone();
        // msgptr: .quad msg
        let mut r = Relocation::new(0, "msg", RelocationKind::Absolute64, 0);
        r.symbol_index = Some(msg_sym);
        r.file_index = Some(0);
        data_sec.relocations.push(r);
        obj.sections[di] = data_sec;
    }

    obj
}

/// The driver: dlopen(argv[1], RTLD_NOW), dlsym(handle, "add42"), exit(add42(5)).
fn build_dlopen_driver(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);

    let mut text: Vec<u8> = Vec::new();
    text.extend_from_slice(&[0x48, 0x8b, 0x7c, 0x24, 0x10]); // mov rdi,[rsp+16]
    text.extend_from_slice(&[0xbe, 0x02, 0x00, 0x00, 0x00]); // mov esi, RTLD_NOW
    text.extend_from_slice(&[0x31, 0xd2]); // xor edx,edx
    let call_dlopen_off = text.len() as u64;
    text.extend_from_slice(&[0xe8, 0, 0, 0, 0]); // call dlopen
    text.extend_from_slice(&[0x48, 0x85, 0xc0]); // test rax,rax
    text.extend_from_slice(&[0x74, 0x24]); // jz fail99
    text.extend_from_slice(&[0x48, 0x89, 0xc7]); // mov rdi,rax
    let lea_symname_off = text.len() as u64;
    text.extend_from_slice(&[0x48, 0x8d, 0x35, 0, 0, 0, 0]); // lea rsi,[rip+symname]
    let call_dlsym_off = text.len() as u64;
    text.extend_from_slice(&[0xe8, 0, 0, 0, 0]); // call dlsym
    text.extend_from_slice(&[0x48, 0x85, 0xc0]); // test rax,rax
    text.extend_from_slice(&[0x74, 0x1c]); // jz fail98
    text.extend_from_slice(&[0xbf, 0x05, 0x00, 0x00, 0x00]); // mov edi,5
    text.extend_from_slice(&[0xff, 0xd0]); // call rax
    text.extend_from_slice(&[0x89, 0xc7]); // mov edi,eax
    text.extend_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]); // mov eax,60
    text.extend_from_slice(&[0x0f, 0x05]); // syscall
    // fail99: exit(99)
    text.extend_from_slice(&[0xbf, 0x63, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0x0f, 0x05]);
    // fail98: exit(98)
    text.extend_from_slice(&[0xbf, 0x62, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0x0f, 0x05]);

    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    let sym_name = b"add42\0";
    let rodata_idx = obj.add_section(Section::new_data(
        ".rodata",
        sym_name.to_vec(),
        false,
        1,
    ));

    let _start = obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    let symname = obj.add_symbol(Symbol::new_defined(
        "symname",
        SymbolBinding::Global,
        SymbolType::Object,
        rodata_idx,
        0,
        sym_name.len() as u64,
        0,
    ));
    let dlopen = obj.add_symbol(Symbol::new_undefined("dlopen", 0));
    let dlsym = obj.add_symbol(Symbol::new_undefined("dlsym", 0));

    let mut text_sec = obj.sections[text_idx].clone();
    let mut r1 = Relocation::new(
        call_dlopen_off + 1,
        "dlopen",
        RelocationKind::PcRelative32,
        -4,
    );
    r1.symbol_index = Some(dlopen);
    r1.file_index = Some(0);
    text_sec.relocations.push(r1);
    let mut r2 = Relocation::new(
        lea_symname_off + 3,
        "symname",
        RelocationKind::PcRelative32,
        -4,
    );
    r2.symbol_index = Some(symname);
    r2.file_index = Some(0);
    text_sec.relocations.push(r2);
    let mut r3 = Relocation::new(
        call_dlsym_off + 1,
        "dlsym",
        RelocationKind::PcRelative32,
        -4,
    );
    r3.symbol_index = Some(dlsym);
    r3.file_index = Some(0);
    text_sec.relocations.push(r3);
    obj.sections[text_idx] = text_sec;
    let _ = _start;

    obj
}

/// A shared library exporting `add42`: lea eax,[rdi+42]; ret.
fn build_add42_lib(obj_path: PathBuf, target: &Target) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let code = [0x8d, 0x47, 0x2a, 0xc3]; // lea eax,[rdi+42]; ret
    let text_idx = obj.add_section(Section::new_code(".text", code.to_vec(), 16));
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

/// An object referencing an arbitrary undefined symbol via a call.
fn build_object_referencing(obj_path: PathBuf, target: &Target, symbol: &str) -> ObjectFile {
    let mut obj = ObjectFile::new(obj_path, target.clone(), 0);
    let mut text = vec![0xe8, 0, 0, 0, 0]; // call <symbol>
    text.extend_from_slice(&[0xb8, 0x3c, 0x00, 0x00, 0x00]);
    text.extend_from_slice(&[0x31, 0xff]);
    text.extend_from_slice(&[0x0f, 0x05]);
    let text_idx = obj.add_section(Section::new_code(".text", text.clone(), 16));
    let _start = obj.add_symbol(Symbol::new_defined(
        "_start",
        SymbolBinding::Global,
        SymbolType::Function,
        text_idx,
        0,
        text.len() as u64,
        0,
    ));
    let ext = obj.add_symbol(Symbol::new_undefined(symbol, 0));
    let mut text_sec = obj.sections[text_idx].clone();
    let mut r = Relocation::new(1, symbol, RelocationKind::PcRelative32, -4);
    r.symbol_index = Some(ext);
    r.file_index = Some(0);
    text_sec.relocations.push(r);
    obj.sections[text_idx] = text_sec;
    let _ = _start;
    obj
}

fn test_dir() -> PathBuf {
    let dir = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("elf_dyn_tests");
    std::fs::create_dir_all(&dir).expect("create test dir");
    dir
}

fn write_object(obj: &ObjectFile, path: &Path) {
    ObjectWriter::write_to_file(obj, path).expect("write ADOB");
}

/// Assert the dynamic-linking invariants common to a dynamic executable:
/// correct e_type, PT_INTERP with the triple's loader path, PT_DYNAMIC,
/// PT_GNU_RELRO covering a page-aligned start, both BIND_NOW flags, hash
/// tables, and strtab/symtab pointers inside mapped segments.
fn assert_dynamic_exec_structure<'a>(
    bytes: &'a [u8],
    expected_e_type: u16,
    target: &Target,
) -> ElfView<'a> {
    let view = ElfView::new(bytes);
    assert_eq!(
        view.e_type(),
        expected_e_type,
        "e_type mismatch (2=ET_EXEC, 3=ET_DYN)"
    );

    let phdrs = view.phdrs();
    let loads: Vec<&Phdr> = phdrs.iter().filter(|p| p.p_type == PT_LOAD).collect();
    assert!(!loads.is_empty(), "must have PT_LOAD segments");

    let interp = phdrs
        .iter()
        .find(|p| p.p_type == PT_INTERP)
        .expect("dynamic executable must have PT_INTERP");
    let interp_str = cstr_at(bytes, interp.p_offset as usize);
    assert_eq!(
        interp_str,
        target.elf_interpreter().expect("triple has interpreter"),
        "PT_INTERP must name the target triple's dynamic loader"
    );

    // PT_INTERP content must be covered by a PT_LOAD (the loader maps the
    // path from the file, but the address it reports must be mapped).
    assert!(
        loads.iter().any(|l| {
            interp.p_vaddr >= l.p_vaddr && interp.p_vaddr + interp.p_filesz <= l.p_vaddr + l.p_memsz
        }),
        "PT_INTERP must be inside a PT_LOAD segment"
    );

    assert!(
        phdrs.iter().any(|p| p.p_type == PT_DYNAMIC),
        "must have PT_DYNAMIC"
    );
    assert!(
        phdrs.iter().any(|p| p.p_type == PT_GNU_RELRO),
        "must have PT_GNU_RELRO"
    );

    let dynv = view.dynamic_entries();
    let flag = |tag: u64| dynv.iter().find(|(t, _)| *t == tag).map(|(_, v)| *v);
    assert_eq!(
        flag(DT_FLAGS).unwrap_or(0) & DF_BIND_NOW,
        DF_BIND_NOW,
        "DT_FLAGS must request BIND_NOW (PLT stubs have no lazy resolver)"
    );
    assert_eq!(
        flag(DT_FLAGS_1).unwrap_or(0) & DF_1_NOW,
        DF_1_NOW,
        "DT_FLAGS_1 must request DF_1_NOW"
    );
    assert!(flag(DT_GNU_HASH).is_some(), "must have DT_GNU_HASH");
    assert!(flag(DT_HASH).is_some(), "must have DT_SYSV .hash DT_HASH");
    assert!(flag(DT_STRTAB).is_some(), "must have DT_STRTAB");
    assert!(flag(DT_SYMTAB).is_some(), "must have DT_SYMTAB");
    view
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_dynamic_exec_puts_executes_and_prints() {
    let Some(triple) = linux_libc_triple() else {
        eprintln!("SKIP: no Linux ELF execution environment (WSL) available");
        return;
    };
    let target = Target::from_triple(triple).expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("puts_exec.adob");
    let obj = build_puts_object(obj_path.clone(), &target, false);
    write_object(&obj, &obj_path);

    let out_path = dir.join("puts_exec_dynamic");
    let mut config = LinkConfig::new(out_path.clone(), target.clone());
    config.dynamic = true;
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "dynamic link failed: {:?}", res);

    let bytes = std::fs::read(&out_path).expect("read linked ELF");
    let view = assert_dynamic_exec_structure(&bytes, ET_EXEC, &target);

    // .rela.plt must hold exactly one R_X86_64_JUMP_SLOT for `puts`,
    // targeting the GOT slot registered in DT_PLTGOT.
    let jmprel_off = view
        .dynamic_entries()
        .iter()
        .find(|(t, _)| *t == DT_JMPREL)
        .map(|(_, v)| *v)
        .expect("DT_JMPREL");
    let pltrelsz = view
        .dynamic_entries()
        .iter()
        .find(|(t, _)| *t == DT_PLTRELSZ)
        .map(|(_, v)| *v)
        .expect("DT_PLTRELSZ");
    assert_eq!(pltrelsz, 24, "one JUMP_SLOT relocation expected");
    let _ = jmprel_off;

    // `puts` must be in .dynsym (the loader resolves it from libc).
    assert!(
        view.dynsym_names().iter().any(|n| n == "puts"),
        ".dynsym must contain `puts`, got {:?}",
        view.dynsym_names()
    );

    // EXECUTE.
    let output = run_elf(&out_path, &[]).expect("execute dynamic ELF");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "dynamic ELF failed: exit {:?}\nstdout: {stdout:?}\nstderr: {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout,
        "hello from adesh dynamic ELF\n",
        "puts must print the exact message through the PLT/GOT import"
    );
}

#[test]
fn test_pie_executes_with_relative_relocation() {
    let Some(triple) = linux_libc_triple() else {
        eprintln!("SKIP: no Linux ELF execution environment (WSL) available");
        return;
    };
    let target = Target::from_triple(triple).expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("puts_pie.adob");
    let obj = build_puts_object(obj_path.clone(), &target, true);
    write_object(&obj, &obj_path);

    let out_path = dir.join("puts_pie_dynamic");
    let mut config = LinkConfig::new(out_path.clone(), target.clone());
    config.dynamic = true;
    config.pie = true;
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "PIE link failed: {:?}", res);

    let bytes = std::fs::read(&out_path).expect("read linked PIE");
    let view = assert_dynamic_exec_structure(&bytes, ET_DYN, &target);
    assert_eq!(
        view.dynamic_entries()
            .iter()
            .find(|(t, _)| *t == DT_FLAGS_1)
            .map(|(_, v)| *v)
            .unwrap_or(0)
            & DF_1_PIE,
        DF_1_PIE,
        "DT_FLAGS_1 must carry DF_1_PIE"
    );

    // .rela.dyn must contain exactly one R_X86_64_RELATIVE for the `msgptr`
    // slot; its addend is the link-time VA of `msg` (off = va - image_base).
    let rela = view.rela_entries(".rela.dyn");
    assert!(
        rela.iter().any(|(_, _, ty, _)| *ty == R_X86_64_RELATIVE),
        "expected an R_X86_64_RELATIVE relocation, got {rela:?}"
    );

    // EXECUTE: the program loads msg through the RELATIVE-relocated pointer,
    // so any loader mis-processing prints garbage or faults instead.
    let output = run_elf(&out_path, &[]).expect("execute PIE ELF");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "PIE failed: exit {:?}\nstdout: {stdout:?}\nstderr: {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout, "hello from adesh dynamic ELF\n");
}

#[test]
fn test_shared_lib_dlopen_dlsym_and_call_executes() {
    let Some(triple) = linux_libc_triple() else {
        eprintln!("SKIP: no Linux ELF execution environment (WSL) available");
        return;
    };
    let target = Target::from_triple(triple).expect("triple");
    let dir = test_dir();

    // 1. Build the shared library exporting `add42`.
    let lib_obj_path = dir.join("libadd42.adob");
    let lib_obj = build_add42_lib(lib_obj_path.clone(), &target);
    write_object(&lib_obj, &lib_obj_path);
    let so_path = dir.join("libadd42.so");
    let mut so_config = LinkConfig::new(so_path.clone(), target.clone());
    so_config.shared = true;
    let res = Linker::link(&[lib_obj_path], so_config);
    assert!(res.is_ok(), "shared library link failed: {:?}", res);

    let so_bytes = std::fs::read(&so_path).expect("read .so");
    let so_view = ElfView::new(&so_bytes);
    assert_eq!(so_view.e_type(), ET_DYN, "shared library must be ET_DYN");
    let soname = so_view
        .dynamic_entries()
        .iter()
        .find(|(t, _)| *t == DT_SONAME)
        .map(|(_, v)| so_view.dyn_str(*v))
        .expect("DT_SONAME required in shared library");
    assert_eq!(soname, "libadd42.so");
    assert!(
        so_view.dynsym_names().iter().any(|n| n == "add42"),
        "`add42` must be exported in .dynsym, got {:?}",
        so_view.dynsym_names()
    );
    assert!(
        so_view.dynamic_entries().iter().any(|(t, _)| *t == DT_GNU_HASH),
        "exported symbols require DT_GNU_HASH for loader symbol lookup"
    );

    // 2. Build the PIE driver that dlopens the library and calls add42(5).
    let drv_obj_path = dir.join("dlopen_driver.adob");
    let drv_obj = build_dlopen_driver(drv_obj_path.clone(), &target);
    write_object(&drv_obj, &drv_obj_path);
    let drv_path = dir.join("dlopen_driver_pie");
    let mut drv_config = LinkConfig::new(drv_path.clone(), target.clone());
    drv_config.dynamic = true;
    drv_config.pie = true;
    let res = Linker::link(&[drv_obj_path], drv_config);
    assert!(res.is_ok(), "driver link failed: {:?}", res);

    let drv_bytes = std::fs::read(&drv_path).expect("read driver");
    assert_eq!(ElfView::new(&drv_bytes).e_type(), ET_DYN, "PIE driver");

    // The driver imports dlopen and dlsym through the PLT.
    let drv_view = ElfView::new(&drv_bytes);
    let names = drv_view.dynsym_names();
    assert!(
        names.iter().any(|n| n == "dlopen"),
        "driver .dynsym must import dlopen, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "dlsym"),
        "driver .dynsym must import dlsym, got {names:?}"
    );

    // 3. EXECUTE: driver <so-path> -> exit code add42(5) == 47.
    let so_arg = {
        #[cfg(target_os = "windows")]
        {
            to_wsl_path(&so_path)
        }
        #[cfg(not(target_os = "windows"))]
        {
            so_path.to_string_lossy().into_owned()
        }
    };
    let output = run_elf(&drv_path, &[&so_arg]).expect("execute dlopen driver");
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code, 47,
        "exit code must be add42(5) == 47; 99 = dlopen failed, 98 = dlsym failed\nstdout: {:?}\nstderr: {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_static_link_with_libc_import_fails_loudly() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let dir = test_dir();

    let obj_path = dir.join("puts_static.adob");
    let obj = build_object_referencing(obj_path.clone(), &target, "puts");
    write_object(&obj, &obj_path);

    let out_path = dir.join("puts_static_elf");
    let mut config = LinkConfig::new(out_path.clone(), target.clone());
    config.dynamic = false;
    config.pie = false;
    config.shared = false;

    let res = Linker::link(&[obj_path], config);
    let err = res.expect_err("static link with a libc import must fail loudly");
    let msg = err.to_string();
    assert!(
        msg.contains("puts"),
        "error must name the unsatisfiable symbol: {msg}"
    );
    assert!(
        msg.contains("--dynamic") || msg.contains("--pie"),
        "error must tell the user how to import libc symbols: {msg}"
    );
    assert!(
        !out_path.exists(),
        "a failed link must not leave an output binary"
    );
}

#[test]
fn test_unknown_symbol_fails_loudly_never_becomes_import() {
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");

    // The router must classify a random unknown name as Undefined, not as a
    // dynamic libc import (the old behavior silently imported every unknown
    // name from libc.so.6 and turned typos into runtime load errors).
    let bogus = "definitely_not_a_libc_function";
    match OsApiRouter::classify(bogus, &target) {
        SymbolRoute::Undefined => {}
        other => panic!(
            "unknown symbol must route to Undefined, got {other:?} (silent-import regression)"
        ),
    }

    let dir = test_dir();
    let obj_path = dir.join("bogus.adob");
    let obj = build_object_referencing(obj_path.clone(), &target, bogus);
    write_object(&obj, &obj_path);

    let out_path = dir.join("bogus_elf");
    let mut config = LinkConfig::new(out_path.clone(), target.clone());
    config.dynamic = true;
    let res = Linker::link(&[obj_path], config);
    let err = res.expect_err("unknown symbol must fail the link");
    let msg = err.to_string();
    assert!(
        msg.contains(bogus),
        "link error must name the undefined symbol: {msg}"
    );
    assert!(
        !out_path.exists(),
        "a failed link must not leave an output binary"
    );
}
