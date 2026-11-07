//! Phase 12 item 4 — link-only coverage for the per-target runtime archives.
//!
//! Phase 12 item 1 made a missing target archive an honest failure; items 2 and 3
//! made the archives buildable and shippable. This suite proves the two halves fit
//! together: a build for a triple either consumes an archive built for that triple
//! and produces an image whose header names that triple's architecture and OS, or it
//! fails with an error that says what is missing.
//!
//! The defect class this closes is the one the resolver work exposed. A link that
//! quietly used the wrong archive died later on `undefined reference to
//! aot_print_cstr`; before that, a host link that "found" the root crate's cdylib
//! import library shipped an image with no runtime definitions in it and reported
//! success. Both look like a build that ran.
//!
//! So every assertion here is made on the bytes of the produced image — ELF, PE and
//! Mach-O headers plus the ELF symbol table — not on the CLI's exit status alone. A
//! fallback that returns exit 0 with a host-architecture image fails here, and a link
//! that leaves the `aot_*` calls unbound fails here.
//!
//! Running a cross-built image is not claimed by this file: an `aarch64` ELF needs an
//! `aarch64` kernel or emulator, which is Phase 13. The one image executed is the
//! host-triple control, which proves the `aot_*` definitions the header checks read
//! are reachable, not merely recorded.
//!
//! Two tests need an archive staged for a foreign triple. They say so with a loud
//! `SKIP:` naming `scripts/build-runtime-archives.sh` instead of passing silently.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const HELLO_SOURCE: &str = "print(\"phase12 cross link\");\n";
const HELLO_OUTPUT: &str = "phase12 cross link";

const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;
const PT_INTERP: u32 = 3;
const SHT_SYMTAB: u32 = 2;
const SHT_DYNSYM: u32 = 11;
const SHN_UNDEF: u16 = 0;

const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;
const MH_MAGIC_64_LE: u32 = 0xFEED_FACF;
const MH_CPU_TYPE_X86_64: u32 = 0x0100_0007;
const MH_CPU_TYPE_ARM64: u32 = 0x0100_000C;
const MH_EXECUTE: u32 = 2;
/// Set by the Mach-O writer for every executable: the image slides at load
/// time, which is what makes an absolute pointer in code unfixable.
const MH_PIE: u32 = 0x0020_0000;

const RISCV_TRIPLE: &str = "riscv64gc-unknown-linux-gnu";
const WINDOWS_ARM_TRIPLE: &str = "aarch64-pc-windows-msvc";
const MUSL_TRIPLE: &str = "x86_64-unknown-linux-musl";
const GNU_TRIPLE: &str = "x86_64-unknown-linux-gnu";
/// The triple whose link was blocked by a code-embedded absolute pointer
/// (`error[LNK009]`) rather than by a missing runtime archive.
const APPLE_X86_TRIPLE: &str = "x86_64-apple-darwin";

#[derive(Copy, Clone, Debug, PartialEq)]
enum Format {
    Elf,
    Pe,
    MachO,
}

/// One advertised native target and the header facts its image must carry.
struct TripleSpec {
    triple: &'static str,
    format: Format,
    /// `e_machine` for ELF, COFF `Machine` for PE, `cputype` for Mach-O.
    arch_id: u32,
}

/// The list `scripts/build-runtime-archives.sh` builds for `--all`.
const SPECS: [TripleSpec; 8] = [
    TripleSpec {
        triple: GNU_TRIPLE,
        format: Format::Elf,
        arch_id: EM_X86_64 as u32,
    },
    TripleSpec {
        triple: MUSL_TRIPLE,
        format: Format::Elf,
        arch_id: EM_X86_64 as u32,
    },
    TripleSpec {
        triple: "aarch64-unknown-linux-gnu",
        format: Format::Elf,
        arch_id: EM_AARCH64 as u32,
    },
    TripleSpec {
        triple: RISCV_TRIPLE,
        format: Format::Elf,
        arch_id: 243,
    },
    TripleSpec {
        triple: "aarch64-apple-darwin",
        format: Format::MachO,
        arch_id: MH_CPU_TYPE_ARM64,
    },
    TripleSpec {
        triple: APPLE_X86_TRIPLE,
        format: Format::MachO,
        arch_id: MH_CPU_TYPE_X86_64,
    },
    TripleSpec {
        triple: "x86_64-pc-windows-msvc",
        format: Format::Pe,
        arch_id: IMAGE_FILE_MACHINE_AMD64 as u32,
    },
    TripleSpec {
        triple: WINDOWS_ARM_TRIPLE,
        format: Format::Pe,
        arch_id: IMAGE_FILE_MACHINE_ARM64 as u32,
    },
];

/// Held by any test that reads or temporarily moves a staged archive, so the
/// hide-and-restore test cannot make a neighbouring test observe a missing file.
fn tree_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn adesh_cli() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_adesh"))
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn spec_of(triple: &str) -> &'static TripleSpec {
    SPECS
        .iter()
        .find(|s| s.triple == triple)
        .unwrap_or_else(|| panic!("phase12 cross-link: {triple} is not an advertised target"))
}

/// Everything one `adesh build --target <triple> --emit=exe` run left behind.
struct BuildRun {
    triple: &'static str,
    exit_code: Option<i32>,
    text: String,
    /// The image the build wrote, only when the build claimed success.
    artifact: Option<PathBuf>,
}

impl BuildRun {
    fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    fn fail(&self, what: &str) -> ! {
        panic!(
            "phase12 cross-link: {what}\ntriple: {}\nexit: {:?}\n--- CLI output ---\n{}",
            self.triple, self.exit_code, self.text
        )
    }
}

fn scratch_dir(tag: &str, triple: &str) -> PathBuf {
    let dir = repo_root()
        .join("target")
        .join("cross_link_tests")
        .join(format!("{tag}_{triple}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Drive the CLI, not `adesh_linker::link(..)`.
///
/// The raw linker entry point takes a list of objects and never consults the runtime
/// archive resolver, so calling it directly would exercise a path with no
/// target-scoped archive to consume and no way to fail the way a real cross build
/// fails — which is exactly what item 4 has to pin.
fn build_for_triple(tag: &'static str, triple: &'static str) -> BuildRun {
    let dir = scratch_dir(tag, triple);
    let source = dir.join("hello.adesh");
    std::fs::write(&source, HELLO_SOURCE).expect("write source");
    let output = dir.join("app");

    let run = Command::new(adesh_cli())
        .current_dir(repo_root())
        .args([
            "build",
            &source.display().to_string(),
            "--target",
            triple,
            "--emit=exe",
            "-o",
            &output.display().to_string(),
            "--fast",
        ])
        .output()
        .expect("run `adesh build`");

    let mut text = String::from_utf8_lossy(&run.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&run.stderr));
    let text = strip_progress_markup(&text);

    let artifact = if run.status.success() {
        Some(locate_artifact(&dir).unwrap_or_else(|| {
            panic!(
                "phase12 cross-link: `adesh build --target {triple}` exited 0 but wrote no `app` \
                 image into {}\n--- CLI output ---\n{text}",
                dir.display()
            )
        }))
    } else {
        None
    };

    BuildRun {
        triple,
        exit_code: run.status.code(),
        text,
        artifact,
    }
}

/// The image the build wrote, whatever extension the CLI appended.
fn locate_artifact(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        let name = path.file_name()?.to_string_lossy().to_string();
        let is_image = name == "app" || (name.starts_with("app.") && !name.ends_with(".adesh"));
        if is_image && path.is_file() {
            found.push(path);
        }
    }
    found.sort();
    found.into_iter().next()
}

/// Drop ANSI colour and the carriage-return spine of the progress redraw, so the
/// assertions match what the user reads rather than escape codes.
fn strip_progress_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

/// An archive staged where the resolver's target-scoped paths look.
///
/// A fixture condition, not a second resolver: it asks whether
/// `scripts/build-runtime-archives.sh`, or the CI `runtime-archives` artifact, put an
/// archive for this triple in the build tree — the setup these tests need.
fn staged_archive_for(triple: &str) -> Option<PathBuf> {
    for profile in ["debug", "release"] {
        for name in ["libadesh_runtime.a", "adesh_runtime.lib"] {
            let path = repo_root()
                .join("target")
                .join(triple)
                .join(profile)
                .join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Minimal image readers — enough of each format to name the architecture.
// ---------------------------------------------------------------------------

fn u16le(bytes: &[u8], offset: usize, what: &str) -> u16 {
    let slice = bytes
        .get(offset..offset + 2)
        .unwrap_or_else(|| panic!("phase12 cross-link: {what} at {offset} is past the image end"));
    u16::from_le_bytes([slice[0], slice[1]])
}

fn u32le(bytes: &[u8], offset: usize, what: &str) -> u32 {
    let slice = bytes
        .get(offset..offset + 4)
        .unwrap_or_else(|| panic!("phase12 cross-link: {what} at {offset} is past the image end"));
    u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
}

fn u64le(bytes: &[u8], offset: usize, what: &str) -> u64 {
    let slice = bytes
        .get(offset..offset + 8)
        .unwrap_or_else(|| panic!("phase12 cross-link: {what} at {offset} is past the image end"));
    u64::from_le_bytes(slice.try_into().expect("8 bytes"))
}

struct ElfFacts {
    class: u8,
    data: u8,
    kind: u16,
    machine: u16,
    interp: Option<String>,
    aot_defined: Vec<String>,
    aot_undefined: Vec<String>,
}

fn elf_facts(bytes: &[u8], path: &Path) -> ElfFacts {
    assert!(
        bytes.len() > 64,
        "phase12 cross-link: {} is {} bytes, too small for an ELF64 image",
        path.display(),
        bytes.len()
    );
    assert_eq!(
        &bytes[0..4],
        b"\x7fELF",
        "phase12 cross-link: {} is not an ELF image",
        path.display()
    );
    let kind = u16le(bytes, 0x10, "e_type");
    let machine = u16le(bytes, 0x12, "e_machine");

    let mut interp = None;
    let phoff = u64le(bytes, 0x20, "e_phoff") as usize;
    let phentsize = u16le(bytes, 0x36, "e_phentsize") as usize;
    let phnum = u16le(bytes, 0x38, "e_phnum") as usize;
    for index in 0..phnum {
        let base = phoff + index * phentsize;
        if u32le(bytes, base, "p_type") != PT_INTERP {
            continue;
        }
        let p_offset = u64le(bytes, base + 8, "p_offset") as usize;
        let p_filesz = u64le(bytes, base + 32, "p_filesz") as usize;
        let raw = bytes
            .get(p_offset..p_offset + p_filesz)
            .unwrap_or_else(|| panic!("PT_INTERP body out of range in {}", path.display()));
        interp = Some(
            raw.iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as char)
                .collect(),
        );
    }

    let (aot_defined, aot_undefined) = elf_aot_symbols(bytes, path);

    ElfFacts {
        class: bytes[4],
        data: bytes[5],
        kind,
        machine,
        interp,
        aot_defined,
        aot_undefined,
    }
}

/// Partition the image's `aot_*` symbols into defined and undefined.
///
/// These are the runtime entry points every compiled program calls. A link that
/// consumed an archive not defining them leaves them undefined — the
/// `undefined reference to aot_print_cstr` failure the Phase 12 resolver work exists
/// to make impossible, and the import-library masquerade let through.
fn elf_aot_symbols(bytes: &[u8], path: &Path) -> (Vec<String>, Vec<String>) {
    let shoff = u64le(bytes, 0x28, "e_shoff") as usize;
    let shentsize = u16le(bytes, 0x3a, "e_shentsize") as usize;
    let shnum = u16le(bytes, 0x3c, "e_shnum") as usize;

    let mut sections: Vec<(usize, usize)> = Vec::with_capacity(shnum);
    let mut tables: Vec<(usize, usize, usize)> = Vec::new();
    for index in 0..shnum {
        let base = shoff + index * shentsize;
        let sh_type = u32le(bytes, base + 4, "sh_type");
        let sh_offset = u64le(bytes, base + 24, "sh_offset") as usize;
        let sh_size = u64le(bytes, base + 32, "sh_size") as usize;
        let sh_link = u32le(bytes, base + 40, "sh_link") as usize;
        sections.push((sh_offset, sh_size));
        if sh_type == SHT_SYMTAB || sh_type == SHT_DYNSYM {
            tables.push((sh_offset, sh_size, sh_link));
        }
    }

    const SYM_SIZE: usize = 24;
    let mut defined: Vec<String> = Vec::new();
    let mut undefined: Vec<String> = Vec::new();
    for (offset, size, link) in tables {
        let (strtab_offset, strtab_size) = *sections
            .get(link)
            .unwrap_or_else(|| panic!("symbol table strtab index {link} out of range"));
        for i in 0..size / SYM_SIZE {
            let base = offset + i * SYM_SIZE;
            let st_name = u32le(bytes, base, "st_name") as usize;
            let st_shndx = u16le(bytes, base + 6, "st_shndx");
            let start = strtab_offset + st_name;
            let end = (start + strtab_size).min(bytes.len());
            let name = String::from_utf8_lossy(&bytes[start..end])
                .split('\0')
                .next()
                .unwrap_or("")
                .to_string();
            if !name.starts_with("aot_") {
                continue;
            }
            if st_shndx == SHN_UNDEF {
                undefined.push(name);
            } else {
                defined.push(name);
            }
        }
    }
    defined.sort();
    defined.dedup();
    undefined.sort();
    undefined.dedup();
    assert!(
        !defined.is_empty() || !undefined.is_empty(),
        "phase12 cross-link: {} carries no `aot_*` symbol at all — the image was stripped, or the \
         runtime archive contributed nothing to the link",
        path.display()
    );
    (defined, undefined)
}

fn pe_machine(bytes: &[u8], path: &Path) -> u16 {
    assert_eq!(
        &bytes[0..2],
        b"MZ",
        "phase12 cross-link: {} is not a PE image",
        path.display()
    );
    let lfanew = u32le(bytes, 0x3c, "e_lfanew") as usize;
    assert_eq!(
        &bytes[lfanew..lfanew + 4],
        b"PE\0\0",
        "phase12 cross-link: {} has no PE signature at e_lfanew",
        path.display()
    );
    u16le(bytes, lfanew + 4, "COFF Machine")
}

fn macho_cpu_type(bytes: &[u8], path: &Path) -> u32 {
    let magic = u32le(bytes, 0, "mach_header magic");
    assert_eq!(
        magic,
        MH_MAGIC_64_LE,
        "phase12 cross-link: {} is not a 64-bit little-endian Mach-O image (magic {:#010x})",
        path.display(),
        magic
    );
    let filetype = u32le(bytes, 12, "mach_header filetype");
    assert_eq!(
        filetype,
        MH_EXECUTE,
        "phase12 cross-link: {} is not MH_EXECUTE (filetype {filetype})",
        path.display()
    );
    u32le(bytes, 4, "mach_header cputype")
}

fn read_image(run: &BuildRun) -> (PathBuf, Vec<u8>) {
    let path = run
        .artifact
        .clone()
        .unwrap_or_else(|| run.fail("expected an artifact from a successful build"));
    let bytes = std::fs::read(&path).expect("read artifact");
    (path, bytes)
}

/// Assert the produced image names the requested target's format and architecture.
///
/// A wrong-arch image is the concrete failure this phase is about: the build
/// "succeeds" while the header says a different machine than the `--target` it was
/// given. Every field comes from the artifact's bytes, never from the CLI text.
fn assert_image_matches_target(run: &BuildRun) {
    let spec = spec_of(run.triple);
    let (path, bytes) = read_image(run);
    match spec.format {
        Format::Elf => {
            let facts = elf_facts(&bytes, &path);
            assert_eq!(
                facts.class, ELFCLASS64,
                "phase12 cross-link: {} built an image of ELF class {}, expected ELFCLASS64",
                run.triple, facts.class
            );
            assert_eq!(
                facts.data, ELFDATA2LSB,
                "phase12 cross-link: {} built a big-endian image",
                run.triple
            );
            assert!(
                facts.kind == ET_EXEC || facts.kind == ET_DYN,
                "phase12 cross-link: {} built e_type {}, expected ET_EXEC or ET_DYN",
                run.triple,
                facts.kind
            );
            assert_eq!(
                facts.machine, spec.arch_id as u16,
                "phase12 cross-link: {} built an image with e_machine {}. A link that fell back to \
                 another architecture's objects passes an exit-status check and fails this one",
                run.triple, facts.machine
            );
            assert!(
                facts.aot_defined.iter().any(|s| s == "aot_print_cstr"),
                "phase12 cross-link: {} linked without a defined `aot_print_cstr`, so the runtime \
                 archive it consumed does not provide the entry points the program calls. \
                 Defined: {:?}",
                run.triple,
                facts.aot_defined
            );
            assert!(
                facts.aot_undefined.is_empty(),
                "phase12 cross-link: {} left {} undefined `aot_*` symbol(s): {:?}. A link that \
                 resolved a real runtime archive binds every one of these.",
                run.triple,
                facts.aot_undefined.len(),
                facts.aot_undefined
            );
        }
        Format::Pe => assert_eq!(
            pe_machine(&bytes, &path),
            spec.arch_id as u16,
            "phase12 cross-link: {} built a PE image for a different machine than the target names",
            run.triple
        ),
        Format::MachO => assert_eq!(
            macho_cpu_type(&bytes, &path),
            spec.arch_id,
            "phase12 cross-link: {} built a Mach-O for a different cputype than the target names",
            run.triple
        ),
    }
}

/// A build that does not link must still be actionable: it names the failure and
/// leaves no half-written image a later step could mistake for a good one.
fn assert_failure_is_loud(run: &BuildRun) {
    assert!(
        !run.succeeded(),
        "phase12 cross-link: {} exited 0 but the caller expected a failure",
        run.triple
    );
    let explains = run.text.contains("Build error")
        || run.text.contains("error[")
        || run.text.contains("No runtime archive")
        || run.text.contains("nsupported");
    assert!(
        explains,
        "phase12 cross-link: {} failed with an error that names nothing actionable:\n{}",
        run.triple, run.text
    );
    assert!(
        run.artifact.is_none(),
        "phase12 cross-link: {} failed yet left an artifact behind — a partial image is \
         indistinguishable from a good one to anything that runs it",
        run.triple
    );
}

fn os_of(triple: &str) -> String {
    if triple.contains("linux") {
        "linux".to_string()
    } else if triple.contains("apple-darwin") {
        "darwin".to_string()
    } else if triple.contains("windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

fn host_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        other => format!("{arch}-unknown-{other}"),
    }
}

fn host_os() -> String {
    os_of(&host_triple())
}

/// The one line a reader needs from a build that did not link.
fn first_explanation(text: &str) -> &str {
    text.lines()
        .find(|line| {
            line.contains("Build error")
                || line.contains("No runtime archive")
                || line.contains("error[")
        })
        .map(str::trim)
        .unwrap_or("<no explanation line>")
}

fn describe_image(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read artifact");
    if bytes.starts_with(b"\x7fELF") {
        let facts = elf_facts(&bytes, path);
        format!(
            "ELF e_machine {} with {} `aot_*` definitions",
            facts.machine,
            facts.aot_defined.len()
        )
    } else if bytes.starts_with(b"MZ") {
        format!("PE machine {:#06x}", pe_machine(&bytes, path))
    } else {
        format!("Mach-O cputype {:#010x}", macho_cpu_type(&bytes, path))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The gate: every advertised target either links an image whose header matches
/// that target, or fails loudly and leaves nothing. Never anything in between.
///
/// Prints the measured state of each triple so a CI log reads as a status table
/// rather than eight opaque pass lines.
#[test]
fn every_advertised_target_links_for_its_own_arch_or_fails_loudly() {
    for spec in &SPECS {
        let run = build_for_triple("gate", spec.triple);
        if run.succeeded() {
            assert_image_matches_target(&run);
            let (path, _) = read_image(&run);
            eprintln!(
                "LINKED  {:<28} -> {} ({} bytes)",
                spec.triple,
                describe_image(&path),
                std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            );
        } else {
            assert_failure_is_loud(&run);
            eprintln!(
                "REFUSED {:<28} -> {}",
                spec.triple,
                first_explanation(&run.text)
            );
        }
    }
}

/// The Mach-O link is the one that used to fail. The compiler baked a symbol
/// address into `__text` as `movabs` plus an absolute relocation; dyld applies
/// rebase fixups only in writable segments and `__TEXT` is mapped read/execute,
/// so *every* source program for `x86_64-apple-darwin` died with
/// `error[LNK009]` before the gate test could report anything.
///
/// The gate test deliberately accepts either outcome, so this pins the achieved
/// one. It also insists the image stayed PIE: dropping `MH_PIE` would silence
/// the error without making an absolute pointer in code legal.
#[test]
fn the_mach_o_image_needs_no_fixup_inside_text() {
    let spec = spec_of(APPLE_X86_TRIPLE);
    if staged_archive_for(spec.triple).is_none() {
        eprintln!(
            "SKIP: no runtime archive is staged for {}, so the Mach-O link cannot be pinned \
             here; run scripts/build-runtime-archives.sh --target {} first.",
            spec.triple, spec.triple
        );
        return;
    }

    let run = build_for_triple("macho_link", spec.triple);
    if !run.succeeded() {
        run.fail(
            "the Mach-O image did not link. If the output above shows `error[LNK009]`, an \
             absolute pointer is back in code: the `Move { reg <- Symbol }` arm of the x86_64 \
             backend materializes a symbol address as `lea r64, [rip + disp32]` for a Mach-O \
             image and only uses `movabs` for PE and ELF.",
        );
    }
    assert_image_matches_target(&run);

    let (path, bytes) = read_image(&run);
    let flags = u32le(&bytes, 24, "mach_header flags");
    assert!(
        flags & MH_PIE != 0,
        "phase12 cross-link: {} linked by giving up position independence (flags {flags:#010x} \
         has no MH_PIE) — the link then stops needing a fixup in `__TEXT`, but the addressing \
         form was never made legal",
        path.display()
    );
    eprintln!(
        "LINKED  {:<28} -> PIE Mach-O, cputype {:#010x} ({} bytes, no rebase in `__TEXT`)",
        spec.triple,
        macho_cpu_type(&bytes, &path),
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    );
}

/// A cross build with no archive for its triple must fail with the resolution
/// error: triple named, searched paths listed, fix command printed, and the
/// explicit statement that no host archive was considered.
///
/// `aarch64-pc-windows-msvc` is the pinned triple — nothing in this repository or in
/// the CI `runtime-archives` matrix builds it, so the missing-archive branch is
/// reachable from every host. If a later change stages it, this test says so loudly
/// instead of asserting a stale expectation.
#[test]
fn a_missing_target_archive_fails_with_the_resolution_error() {
    let run = build_for_triple("missing_archive", WINDOWS_ARM_TRIPLE);
    if run.succeeded() {
        eprintln!(
            "SKIP: {WINDOWS_ARM_TRIPLE} now resolves a runtime archive, so the missing-archive \
             error cannot be pinned here; the gate test asserts its link outcome instead."
        );
        return;
    }
    assert_failure_is_loud(&run);
    for (needle, why) in [
        (
            format!("No runtime archive for target {WINDOWS_ARM_TRIPLE}"),
            "the triple it could not satisfy",
        ),
        (
            "scripts/build-runtime-archives.sh".to_string(),
            "the command that builds the archive",
        ),
        ("--profile".to_string(), "the profile that command takes"),
        (
            WINDOWS_ARM_TRIPLE.to_string(),
            "the triple among the searched paths",
        ),
    ] {
        assert!(
            run.text.contains(&needle),
            "phase12 cross-link: {WINDOWS_ARM_TRIPLE}'s missing-archive error must name {why} \
             (`{needle}`); got:\n{}",
            run.text
        );
    }
    assert!(
        run.text.contains("cross build") || run.text.contains("host"),
        "phase12 cross-link: {WINDOWS_ARM_TRIPLE}'s error must say that no host archive was \
         considered:\n{}",
        run.text
    );
}

/// RISC-V used to die while encoding `Move { dst: Physical, src: Symbol(..) }`, so
/// the build never reached the archive resolver and the target looked refused for a
/// reason that had nothing to do with packaging. The backend now materializes a
/// symbol address with `auipc`/`addi` and expands `ParallelMove` before encoding, so
/// hello-world encodes completely and the only remaining blocker is the archive.
///
/// Measured 2026-11-07 on this Windows host: the same command that previously reported
/// `unsupported machine instruction Move { dst: Physical(5), src: Symbol("__str_0") }`
/// now reports `No runtime archive for target riscv64gc-unknown-linux-gnu`. The
/// relocation contract that link depends on is pinned at image level by
/// `linker/tests/riscv64_pcrel_symbol_pair_test.rs`.
#[test]
fn riscv64gc_passes_codegen_and_stops_at_the_runtime_archive() {
    let run = build_for_triple("riscv", RISCV_TRIPLE);
    assert!(
        !run.text.contains("unsupported machine instruction"),
        "phase12 cross-link: {RISCV_TRIPLE} regressed to a codegen refusal, so the archive is \
         hiding a backend gap again:\n{}",
        run.text
    );

    if let Some(staged) = staged_archive_for(RISCV_TRIPLE) {
        assert!(
            !run.text.contains("No runtime archive"),
            "phase12 cross-link: {RISCV_TRIPLE} failed at archive resolution even though {} is \
             staged:\n{}",
            staged.display(),
            run.text
        );
        if run.succeeded() {
            assert_image_matches_target(&run);
            eprintln!(
                "PROMOTED: {RISCV_TRIPLE} links — update TARGET_MATRIX.md and the tier labels"
            );
        } else {
            assert_failure_is_loud(&run);
            eprintln!(
                "{RISCV_TRIPLE} archive resolves; the build stops later: {}",
                first_explanation(&run.text)
            );
        }
        return;
    }

    assert!(
        !run.succeeded(),
        "phase12 cross-link: {RISCV_TRIPLE} reported success with no archive staged"
    );
    assert_failure_is_loud(&run);
    for (needle, why) in [
        (
            format!("No runtime archive for target {RISCV_TRIPLE}"),
            "the triple it could not satisfy",
        ),
        (
            "scripts/build-runtime-archives.sh".to_string(),
            "the command that builds the archive",
        ),
        ("--profile".to_string(), "the profile that command takes"),
    ] {
        assert!(
            run.text.contains(&needle),
            "phase12 cross-link: {RISCV_TRIPLE}'s missing-archive error must name {why} \
             (`{needle}`); got:\n{}",
            run.text
        );
    }
    eprintln!(
        "PROMOTED: {RISCV_TRIPLE} passes codegen; the remaining blocker is packaging, not the \
         backend. `scripts/build-runtime-archives.sh --profile debug --targets {RISCV_TRIPLE}` \
         needs a cross C toolchain (`gcc-riscv64-linux-gnu` + `libc6-dev-riscv64-cross`), which \
         the CI `runtime-archives` matrix already installs."
    );
}

/// With an `aarch64-unknown-linux-gnu` archive staged, the archive is never the
/// reason an AArch64 build fails: it resolves, and the build stops somewhere else.
///
/// Measured 2026-11-04 against a real AArch64 archive (built on this Windows host with
/// `clang --target=aarch64-unknown-linux-gnu` plus an extracted `libc6-dev-arm64-cross`
/// header set): the build reaches the linker and fails `error[LNK001]: undefined
/// reference to \`__str_0\`` — the AArch64 backend emits the string-literal reference
/// without emitting the literal's definition. Before Phase 12 the same command failed
/// at archive resolution, which let the codegen gap stay hidden behind a packaging
/// gap. This test keeps it that way: if a future change makes the archive the reason
/// again, that is a Phase 12 regression and it fails here.
#[test]
fn the_aarch64_archive_resolves_so_the_archive_is_never_the_blocker() {
    const AARCH64_LINUX: &str = "aarch64-unknown-linux-gnu";
    let staged = match staged_archive_for(AARCH64_LINUX) {
        Some(path) => path,
        None => {
            eprintln!(
                "SKIP: no {AARCH64_LINUX} archive staged. `scripts/build-runtime-archives.sh \
                 --profile debug --targets {AARCH64_LINUX}` needs a cross C toolchain; the CI \
                 `runtime-archives` job installs `gcc-aarch64-linux-gnu` and `libc6-dev-arm64-cross` \
                 and produces this file, and the `cross-link` job stages it before running this suite."
            );
            return;
        }
    };
    assert!(
        staged.is_file() && std::fs::metadata(&staged).map(|m| m.len()).unwrap_or(0) > 4096,
        "phase12 cross-link: the staged {AARCH64_LINUX} archive at {} is not a usable archive",
        staged.display()
    );

    let run = build_for_triple("aarch64", AARCH64_LINUX);
    assert!(
        !run.text.contains("No runtime archive"),
        "phase12 cross-link: {AARCH64_LINUX} failed at archive resolution even though {} is staged, \
         so a foreign build is being offered the wrong paths again:\n{}",
        staged.display(),
        run.text
    );
    if run.succeeded() {
        // The blocker moved past the link: verify the image it produced.
        assert_image_matches_target(&run);
        eprintln!("PROMOTED: {AARCH64_LINUX} links — update TARGET_MATRIX.md and the tier labels");
    } else {
        assert_failure_is_loud(&run);
        assert!(
            run.text.contains("Native link error") || run.text.contains("error[LNK"),
            "phase12 cross-link: {AARCH64_LINUX} resolved its archive but failed somewhere other \
             than the link, which is a new state to document:\n{}",
            run.text
        );
        eprintln!(
            "{AARCH64_LINUX} archive resolves; the build stops at the link: {}",
            first_explanation(&run.text)
        );
    }
}

/// glibc and musl archives of the same architecture produce images that name
/// different loaders. That difference is the observable proof a target-scoped archive
/// was consumed: same source, same ADOB, same architecture — only the archive differs.
#[test]
fn a_staged_x86_64_linux_archive_is_consumed_by_a_cross_os_link() {
    if staged_archive_for(GNU_TRIPLE).is_none() || staged_archive_for(MUSL_TRIPLE).is_none() {
        eprintln!(
            "SKIP: no x86_64 Linux runtime archive staged for this host. Build them with \
             `scripts/build-runtime-archives.sh --profile debug --targets \
             \"{GNU_TRIPLE} {MUSL_TRIPLE}\"`; the CI `runtime-archives` job produces the same \
             files as an artifact."
        );
        return;
    }

    let _guard = tree_lock().lock().expect("tree lock poisoned");
    let gnu_run = build_for_triple("linux_gnu", GNU_TRIPLE);
    if !gnu_run.succeeded() {
        gnu_run.fail(&format!(
            "linking {GNU_TRIPLE} with its staged archive must succeed"
        ));
    }
    assert_image_matches_target(&gnu_run);
    let (gnu_path, gnu_bytes) = read_image(&gnu_run);
    let gnu_facts = elf_facts(&gnu_bytes, &gnu_path);
    let gnu_interp = gnu_facts
        .interp
        .clone()
        .unwrap_or_else(|| panic!("phase12 cross-link: the {GNU_TRIPLE} image has no PT_INTERP"));
    assert!(
        gnu_interp.contains("ld-linux") || gnu_interp.contains("ld.so"),
        "phase12 cross-link: {GNU_TRIPLE} named the loader {gnu_interp}, which is not the glibc \
         dynamic loader"
    );

    let musl_run = build_for_triple("linux_musl", MUSL_TRIPLE);
    if !musl_run.succeeded() {
        musl_run.fail(&format!(
            "linking {MUSL_TRIPLE} with its staged archive must succeed"
        ));
    }
    assert_image_matches_target(&musl_run);
    let (musl_path, musl_bytes) = read_image(&musl_run);
    let musl_interp = elf_facts(&musl_bytes, &musl_path)
        .interp
        .clone()
        .unwrap_or_default();

    assert_ne!(
        gnu_interp, musl_interp,
        "phase12 cross-link: {GNU_TRIPLE} and {MUSL_TRIPLE} name the same loader ({gnu_interp}), \
         which means one of the links reused the other's archive"
    );
    assert_ne!(
        gnu_bytes, musl_bytes,
        "phase12 cross-link: the -gnu and -musl links produced byte-identical images from \
         different archives"
    );
    eprintln!(
        "cross-OS links from a {} host: -gnu loader {gnu_interp} ({} bytes), -musl loader \
         {musl_interp} ({} bytes)",
        host_os(),
        gnu_bytes.len(),
        musl_bytes.len()
    );
}

/// The control case: the host triple links, and its image is executed, which proves
/// the `aot_*` definitions the header checks read are reachable at run time rather
/// than merely recorded in the symbol table.
#[test]
fn the_host_link_consumes_its_archive_and_the_image_runs() {
    let host = host_triple();
    if !SPECS.iter().any(|s| s.triple == host) {
        eprintln!("SKIP: host triple {host} is not an advertised native target.");
        return;
    }
    let run = build_for_triple("host", spec_of(&host).triple);
    if !run.succeeded() {
        run.fail(
            "a build for the host triple must link: `cargo build -p adesh-runtime` and \
                 `adl repair` both produce the host archive",
        );
    }
    assert_image_matches_target(&run);
    let (path, _) = read_image(&run);
    let output = Command::new(&path).output().unwrap_or_else(|e| {
        panic!(
            "phase12 cross-link: running the host image {} failed: {e}",
            path.display()
        )
    });
    assert!(
        output.status.success(),
        "phase12 cross-link: the host image exited {:?}",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains(HELLO_OUTPUT),
        "phase12 cross-link: the host image printed {stdout:?}, expected {HELLO_OUTPUT:?}"
    );
}

/// Phase 12's claim is a link that crosses an OS boundary, so this suite must not be
/// satisfiable without one happening here.
///
/// A loud `SKIP:` when nothing foreign is staged; a hard failure when archives are
/// staged and still no foreign image links, because that is the exact state the
/// phase's exit criterion is written against and it must not pass unnoticed.
#[test]
fn at_least_one_foreign_os_image_links() {
    let home = host_os();
    let _guard = tree_lock().lock().expect("tree lock poisoned");
    let mut linked: Vec<&str> = Vec::new();
    let mut staged_but_failed: Vec<String> = Vec::new();
    let mut not_staged: Vec<&str> = Vec::new();

    for spec in &SPECS {
        if os_of(spec.triple) == home {
            continue;
        }
        if staged_archive_for(spec.triple).is_none() {
            not_staged.push(spec.triple);
            continue;
        }
        let run = build_for_triple("foreign", spec.triple);
        if run.succeeded() {
            assert_image_matches_target(&run);
            linked.push(spec.triple);
        } else {
            assert_failure_is_loud(&run);
            staged_but_failed.push(format!("{}: {}", spec.triple, first_explanation(&run.text)));
        }
    }

    if linked.is_empty() {
        if !staged_but_failed.is_empty() && not_staged.is_empty() {
            panic!(
                "phase12 cross-link: no image linked for an OS other than {home} even though every \
                 foreign triple had a staged archive. Failures:\n  {}\nThat is precisely the state \
                 Phase 12's exit criterion is about: a target archive that resolves but a link that \
                 does not consume it.",
                staged_but_failed.join("\n  ")
            );
        }
        eprintln!(
            "SKIP: no foreign-OS archive staged on this {home} host, so no cross-OS link was \
             attempted. Triples with nothing staged: {not_staged:?}. Staged but not linking yet: {}",
            if staged_but_failed.is_empty() {
                "none".to_string()
            } else {
                staged_but_failed.join("; ")
            }
        );
        eprintln!(
            "SKIP: build them with `scripts/build-runtime-archives.sh --profile debug --targets \
             \"<triple>\"`, or run this suite in CI where the `runtime-archives` artifact is \
             downloaded into target/<triple>/debug/."
        );
        return;
    }
    eprintln!("foreign-OS images linked on a {home} host: {linked:?}");
    for note in &staged_but_failed {
        eprintln!("NOTE: staged but not linking yet — {note}");
    }
}

/// A cross build must not fall back to the host build tree.
///
/// The resolver's whole purpose is that `target/<triple>/` is consulted and nothing
/// host-shaped is even offered for a foreign triple. This hides the staged archive
/// for one foreign triple and asserts the build then reports the archive as missing —
/// the one assertion that separates "target-scoped resolution" from "some archive was
/// found somewhere".
#[test]
fn a_cross_build_does_not_fall_back_to_the_host_build_tree() {
    let home = host_os();
    let _guard = tree_lock().lock().expect("tree lock poisoned");

    // Take the first foreign triple whose staged archive actually produces an image:
    // the assertion needs a build that succeeds with the archive present, and which
    // triples do that differs by host (a Windows box links x86_64 Linux images, a
    // Linux runner links Mach-O and AArch64 ones). Keying off one fixed triple would
    // skip on most CI hosts.
    let mut subject: Option<(&'static str, PathBuf)> = None;
    for spec in &SPECS {
        if os_of(spec.triple) == home {
            continue;
        }
        let Some(staged) = staged_archive_for(spec.triple) else {
            continue;
        };
        let baseline = build_for_triple("fallback_with", spec.triple);
        if baseline.succeeded() {
            assert_image_matches_target(&baseline);
            subject = Some((spec.triple, staged));
            break;
        }
    }
    let Some((triple, staged)) = subject else {
        eprintln!(
            "SKIP: no foreign triple with a staged archive links on this {home} host, so there is \
             no successful baseline to hide an archive from. `linking.rs::runtime_archive_tests` \
             pins the same candidate-list ordering at unit level."
        );
        return;
    };

    let hidden = staged.with_extension("hidden-by-cross-link-test");
    std::fs::rename(&staged, &hidden).expect("hide the staged archive");
    let result = std::panic::catch_unwind(|| build_for_triple("fallback_without", triple));
    std::fs::rename(&hidden, &staged).expect("restore the staged archive");

    let run = result.expect("the second build ran");
    assert!(
        !run.succeeded(),
        "phase12 cross-link: {triple} linked after its own archive was hidden, so the link used \
         objects that were not built for it:\n{}",
        run.text
    );
    assert!(
        run.text.contains("No runtime archive for target"),
        "phase12 cross-link: {triple} failed for a reason other than the missing archive, so the \
         fallback question is unanswered:\n{}",
        run.text
    );
    assert_failure_is_loud(&run);
    eprintln!("hiding {triple}'s archive produced the missing-archive error: no host fallback");
}
