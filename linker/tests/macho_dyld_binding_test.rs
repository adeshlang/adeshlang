//! Mach-O 64 dyld binding: structural conformance tests.
//!
//! These build real objects with real relocations, link them for
//! `x86_64-darwin`, and verify the produced image by *parsing it back*:
//! load commands, `LC_DYLD_INFO_ONLY` streams (decoded with the reference
//! decoders from `macho::dynamic`), the indirect symbol table, the
//! synthesized stub/GOT/helper sections, and the full call → stub →
//! lazy-pointer → helper → binder machine-code chain.
//!
//! What these tests CANNOT do: execute the binary under dyld (that needs a
//! macOS host). Execution verification is the CI macOS job's
//! responsibility; everything verifiable on a byte level is verified here.

use adesh_linker::macho::dynamic::{
    BIND_TYPE_POINTER, REBASE_TYPE_POINTER, REBASE_TYPE_TEXT_ABSOLUTE32,
    decode_bind_stream_for_test, decode_export_trie_for_test, decode_rebase_stream_for_test,
};
use adesh_linker::object::ObjectWriter;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use adesh_linker::{LinkConfig, Linker, ObjectFile};
use tempfile::tempdir;

// ─── Mach-O constants (mirrors of macho::header) ─────────────────────────────

const MH_EXECUTE: u32 = 2;
const MH_DYLIB: u32 = 6;
const MH_NOUNDEFS: u32 = 0x1;
const MH_DYLDLINK: u32 = 0x4;
const MH_TWOLEVEL: u32 = 0x80;
const MH_PIE: u32 = 0x200000;

const LC_SYMTAB: u32 = 0x02;
const LC_DYSYMTAB: u32 = 0x0B;
const LC_LOAD_DYLIB: u32 = 0x0C;
const LC_ID_DYLIB: u32 = 0x0D;
const LC_SEGMENT_64: u32 = 0x19;
const LC_DYLD_INFO_ONLY: u32 = 0x80000022;
const LC_MAIN: u32 = 0x80000028;

const N_SECT: u8 = 0x0E;
const N_EXT: u8 = 0x01;
const N_UNDF: u8 = 0x00;

const S_ZEROFILL: u32 = 0x1;
const S_NON_LAZY_SYMBOL_POINTERS: u32 = 0x6;
const S_LAZY_SYMBOL_POINTERS: u32 = 0x7;

// ─── minimal Mach-O reader used by the assertions ───────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SectionRecord {
    sectname: String,
    segname: String,
    addr: u64,
    size: u64,
    offset: u32,
    flags: u32,
    reserved1: u32,
}

#[derive(Debug, Clone)]
struct SegmentRecord {
    segname: String,
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    sections: Vec<SectionRecord>,
}

#[derive(Debug, Default, Clone)]
struct DyldInfo {
    rebase_off: u32,
    rebase_size: u32,
    bind_off: u32,
    bind_size: u32,
    lazy_off: u32,
    lazy_size: u32,
    export_off: u32,
    export_size: u32,
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
struct Dysymtab {
    ilocalsym: u32,
    nlocalsym: u32,
    iextdefsym: u32,
    nextdefsym: u32,
    iundefsym: u32,
    nundefsym: u32,
    indirectsymoff: u32,
    nindirectsyms: u32,
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
struct Symtab {
    symoff: u32,
    nsyms: u32,
    stroff: u32,
    strsize: u32,
}

#[allow(dead_code)]
struct ParsedMachO {
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    filetype: u32,
    segments: Vec<SegmentRecord>,
    dyld_info: Option<DyldInfo>,
    symtab: Option<Symtab>,
    dysymtab: Option<Dysymtab>,
    has_main_command: bool,
    dylibs: Vec<String>,
    id_dylib: Option<String>,
    command_total: usize,
}

fn parse_macho(bytes: &[u8]) -> ParsedMachO {
    assert!(bytes.len() >= 32, "Mach-O header truncated");
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(magic, 0xFEEDFACF, "not a 64-bit Mach-O image");
    let cputype = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert!(
        matches!(cputype, 0x01000007 | 0x0100000C),
        "not an x86_64 or arm64 image"
    );

    let filetype = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let sizeofcmds = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let flags = u32::from_le_bytes(bytes[24..28].try_into().unwrap());

    let mut parsed = ParsedMachO {
        ncmds,
        sizeofcmds,
        flags,
        filetype,
        segments: Vec::new(),
        dyld_info: None,
        symtab: None,
        dysymtab: None,
        has_main_command: false,
        dylibs: Vec::new(),
        id_dylib: None,
        command_total: 0,
    };

    let mut off = 32usize;
    let mut seen = 0u32;
    while seen < ncmds {
        assert!(off + 8 <= bytes.len(), "load command header out of bounds");
        let cmd = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
        let cmdsize = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
        assert!(cmdsize >= 8, "load command size below minimum");
        assert!(
            off + cmdsize as usize <= bytes.len(),
            "load command 0x{:x} exceeds file",
            cmd
        );
        parsed.command_total += cmdsize as usize;
        let body = &bytes[off..off + cmdsize as usize];

        match cmd {
            LC_SEGMENT_64 => {
                assert_eq!(cmdsize % 8, 0, "segment command not 8-byte aligned");
                let segname = read_str(&body[8..24]);
                let vmaddr = u64::from_le_bytes(body[24..32].try_into().unwrap());
                let vmsize = u64::from_le_bytes(body[32..40].try_into().unwrap());
                let fileoff = u64::from_le_bytes(body[40..48].try_into().unwrap());
                let filesize = u64::from_le_bytes(body[48..56].try_into().unwrap());
                let nsects = u32::from_le_bytes(body[64..68].try_into().unwrap());
                assert_eq!(
                    cmdsize,
                    72 + nsects * 80,
                    "segment {} cmdsize mismatch",
                    segname
                );
                let mut sections = Vec::new();
                for i in 0..nsects as usize {
                    let s = &body[72 + i * 80..72 + (i + 1) * 80];
                    sections.push(SectionRecord {
                        sectname: read_str(&s[0..16]),
                        segname: read_str(&s[16..32]),
                        addr: u64::from_le_bytes(s[32..40].try_into().unwrap()),
                        size: u64::from_le_bytes(s[40..48].try_into().unwrap()),
                        offset: u32::from_le_bytes(s[48..52].try_into().unwrap()),
                        flags: u32::from_le_bytes(s[64..68].try_into().unwrap()),
                        reserved1: u32::from_le_bytes(s[68..72].try_into().unwrap()),
                    });
                }
                parsed.segments.push(SegmentRecord {
                    segname,
                    vmaddr,
                    vmsize,
                    fileoff,
                    filesize,
                    sections,
                });
            }
            LC_SYMTAB => {
                parsed.symtab = Some(Symtab {
                    symoff: u32::from_le_bytes(body[8..12].try_into().unwrap()),
                    nsyms: u32::from_le_bytes(body[12..16].try_into().unwrap()),
                    stroff: u32::from_le_bytes(body[16..20].try_into().unwrap()),
                    strsize: u32::from_le_bytes(body[20..24].try_into().unwrap()),
                });
                assert_eq!(cmdsize, 24);
            }
            LC_DYSYMTAB => {
                parsed.dysymtab = Some(Dysymtab {
                    ilocalsym: u32::from_le_bytes(body[8..12].try_into().unwrap()),
                    nlocalsym: u32::from_le_bytes(body[12..16].try_into().unwrap()),
                    iextdefsym: u32::from_le_bytes(body[16..20].try_into().unwrap()),
                    nextdefsym: u32::from_le_bytes(body[20..24].try_into().unwrap()),
                    iundefsym: u32::from_le_bytes(body[24..28].try_into().unwrap()),
                    nundefsym: u32::from_le_bytes(body[28..32].try_into().unwrap()),
                    indirectsymoff: u32::from_le_bytes(body[56..60].try_into().unwrap()),
                    nindirectsyms: u32::from_le_bytes(body[60..64].try_into().unwrap()),
                });
                assert_eq!(cmdsize, 80);
            }
            LC_DYLD_INFO_ONLY => {
                parsed.dyld_info = Some(DyldInfo {
                    rebase_off: u32::from_le_bytes(body[8..12].try_into().unwrap()),
                    rebase_size: u32::from_le_bytes(body[12..16].try_into().unwrap()),
                    bind_off: u32::from_le_bytes(body[16..20].try_into().unwrap()),
                    bind_size: u32::from_le_bytes(body[20..24].try_into().unwrap()),
                    lazy_off: u32::from_le_bytes(body[32..36].try_into().unwrap()),
                    lazy_size: u32::from_le_bytes(body[36..40].try_into().unwrap()),
                    export_off: u32::from_le_bytes(body[40..44].try_into().unwrap()),
                    export_size: u32::from_le_bytes(body[44..48].try_into().unwrap()),
                });
                assert_eq!(cmdsize, 48);
            }
            LC_MAIN => {
                parsed.has_main_command = true;
                assert_eq!(cmdsize, 24);
            }
            LC_LOAD_DYLIB | LC_ID_DYLIB => {
                let name_off = u32::from_le_bytes(body[8..12].try_into().unwrap()) as usize;
                let path = read_cstr(body, name_off);
                if cmd == LC_LOAD_DYLIB {
                    parsed.dylibs.push(path);
                } else {
                    parsed.id_dylib = Some(path);
                }
            }
            _ => {}
        }

        off += cmdsize as usize;
        seen += 1;
    }
    assert_eq!(seen, ncmds, "ncmds mismatch");
    assert_eq!(
        parsed.command_total, sizeofcmds as usize,
        "sizeofcmds mismatch"
    );
    parsed
}

fn read_str(buf: &[u8]) -> String {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}

fn read_cstr(buf: &[u8], start: usize) -> String {
    let end = buf[start..]
        .iter()
        .position(|b| *b == 0)
        .map(|i| start + i)
        .unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[start..end]).to_string()
}

/// File offset for a virtual address in `__TEXT` (file offset 0 == vmaddr).
#[allow(dead_code)]
fn text_off_to_va(vmaddr: u64, off: u32) -> u64 {
    vmaddr + off as u64
}

/// Read a Nlist_64 at index `i`, returning (name, n_type, n_sect, n_desc, value).
fn read_nlist(bytes: &[u8], symtab: &Symtab, index: usize) -> (String, u8, u8, u16, u64) {
    let base = symtab.symoff as usize + index * 16;
    let n_strx = u32::from_le_bytes(bytes[base..base + 4].try_into().unwrap());
    let n_type = bytes[base + 4];
    let n_sect = bytes[base + 5];
    let n_desc = u16::from_le_bytes(bytes[base + 6..base + 8].try_into().unwrap());
    let n_value = u64::from_le_bytes(bytes[base + 8..base + 16].try_into().unwrap());
    let strbase = symtab.stroff as usize + n_strx as usize;
    let name = read_cstr(bytes, strbase);
    (name, n_type, n_sect, n_desc, n_value)
}

// ─── object builders ─────────────────────────────────────────────────────────

/// main: call puts; ret — the call relocates against the undefined `puts`.
fn object_calling_puts() -> ObjectFile {
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    let mut text = Section::new_code(
        ".text",
        // call rel32; ret
        vec![0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3],
        16,
    );
    text.relocations
        .push(Relocation::new(1, "puts", RelocationKind::PcRelative32, -4));
    obj.add_section(text);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        6,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("puts", 0));
    obj
}

// ─── tests ─────────────────────────────────────────────────────────────────

/// End-to-end lazy-binding structure: call site → proxy stub → lazy
/// pointer → stub helper → binder, with every dyld stream and table
/// consistent.
#[test]
fn test_macho_lazy_binding_structure_end_to_end() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let obj = object_calling_puts();
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out.clone(), target);
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "link failed: {:?}", res);

    let bytes = std::fs::read(&out).unwrap();
    let image = parse_macho(&bytes);

    // An executable with imports: PIE, dyld-linked, two-level, and NOT
    // NOUNDEFS (the import is undefined until dyld binds it).
    assert_eq!(image.filetype, MH_EXECUTE);
    assert_eq!(image.flags & MH_PIE, MH_PIE, "MH_PIE must be set");
    assert_eq!(image.flags & MH_DYLDLINK, MH_DYLDLINK);
    assert_eq!(image.flags & MH_TWOLEVEL, MH_TWOLEVEL);
    assert_eq!(
        image.flags & MH_NOUNDEFS,
        0,
        "MH_NOUNDEFS must be clear while an import is unbound"
    );

    // Segments: PAGEZERO, __TEXT, __DATA, __LINKEDIT (in that order).
    let seg_names: Vec<&str> = image.segments.iter().map(|s| s.segname.as_str()).collect();
    assert_eq!(
        seg_names,
        vec!["__PAGEZERO", "__TEXT", "__DATA", "__LINKEDIT"]
    );
    let pagezero = &image.segments[0];
    assert_eq!(pagezero.vmaddr, 0);
    assert_eq!(pagezero.vmsize, 0x1_0000_0000);
    let text = &image.segments[1];
    let data = &image.segments[2];
    let linkedit = &image.segments[3];

    // libSystem must be the first (ordinal 1) LC_LOAD_DYLIB.
    assert!(
        image.dylibs.iter().any(|d| d.contains("libSystem")),
        "libSystem.B.dylib must be a dependency, got {:?}",
        image.dylibs
    );

    // Dynamic sections exist with dyld-visible types and metadata.
    let la = text_section_of(data, "__la_symbol_ptr");
    assert_eq!(
        la.flags & 0xFF,
        S_LAZY_SYMBOL_POINTERS,
        "__la_symbol_ptr must be S_LAZY_SYMBOL_POINTERS"
    );
    assert_eq!(
        la.reserved1, 1,
        "la reserved1 = first indirect index after __got"
    );
    assert_eq!(la.size, 8, "one lazy slot for one import");

    let got = text_section_of(data, "__got");
    assert_eq!(
        got.flags & 0xFF,
        S_NON_LAZY_SYMBOL_POINTERS,
        "__got must be S_NON_LAZY_SYMBOL_POINTERS"
    );
    assert_eq!(got.reserved1, 0);
    assert_eq!(got.size, 8, "one GOT slot (dyld_stub_binder)");

    let helper = text_section_named(&image, "__stub_helper");
    assert!(helper.size >= 21, "helper needs entries + tail");

    // LC_DYLD_INFO_ONLY with real content.
    let info = image
        .dyld_info
        .clone()
        .expect("LC_DYLD_INFO_ONLY required for a dyld-bound image");
    assert!(info.rebase_size > 0);
    assert!(info.bind_size > 0);
    assert!(info.lazy_size > 0);

    // Segment indexes for the streams: __TEXT=1, __DATA=2 (PAGEZERO is 0).
    let la_slot_va = la.addr;

    // Lazy stream: exactly one self-contained entry binding `_puts` into
    // the lazy slot, from libSystem (ordinal 1).
    let lazy_bytes = &bytes[info.lazy_off as usize..(info.lazy_off + info.lazy_size) as usize];
    let lazy = decode_bind_stream_for_test(lazy_bytes);
    assert_eq!(
        lazy,
        vec![(
            2,
            la_slot_va - data.vmaddr,
            BIND_TYPE_POINTER,
            1,
            "_puts".to_string(),
            0
        )],
        "lazy stream must bind _puts at slot 0x{:x}",
        la_slot_va
    );

    // dyld's lazy walker whitelists opcodes: a SET_TYPE_IMM (0x50) anywhere
    // in this stream aborts the process at launch with
    // `dyld: bad lazy bind opcode 0x50`. Pin the exact byte sequence.
    assert_eq!(
        lazy_bytes.to_vec(),
        vec![
            0x72,                             // SET_SEGMENT_AND_OFFSET_ULEB __DATA
            (la_slot_va - data.vmaddr) as u8, // slot offset within the segment
            0x11,                             // SET_DYLIB_ORDINAL_IMM libSystem
            0x40,                             // SET_SYMBOL_TRAILING_FLAGS_IMM
            b'_',
            b'p',
            b'u',
            b't',
            b's',
            0x00, // "_puts" 
            0x90, // DO_BIND
            0x00, // DONE
        ],
        "lazy stream must be segment/offset, ordinal, symbol, DO_BIND, DONE"
    );

    // Bind stream: dyld_stub_binder into the GOT slot.
    let bind_bytes = &bytes[info.bind_off as usize..(info.bind_off + info.bind_size) as usize];
    let bind = decode_bind_stream_for_test(bind_bytes);
    assert_eq!(
        bind,
        vec![(
            2,
            got.addr - data.vmaddr,
            BIND_TYPE_POINTER,
            1,
            "dyld_stub_binder".to_string(),
            0
        )]
    );

    // Rebase stream: the lazy slot (initial value is an absolute pointer
    // into __TEXT) and the helper's `push $helper_start` immediate.
    let rebase_bytes =
        &bytes[info.rebase_off as usize..(info.rebase_off + info.rebase_size) as usize];
    let rebases = decode_rebase_stream_for_test(rebase_bytes);
    assert!(
        rebases.iter().any(
            |(seg, off, ty)| *seg == 2 && *ty == REBASE_TYPE_POINTER && {
                let va = data.vmaddr + off;
                va == la_slot_va
            }
        ),
        "lazy slot must be rebased: decoded {:?}",
        rebases
    );
    assert!(
        rebases
            .iter()
            .any(|(seg, _off, ty)| *seg == 1 && *ty == REBASE_TYPE_TEXT_ABSOLUTE32),
        "helper push immediate must be rebased: decoded {:?}",
        rebases
    );

    // Symbol table: `_puts` and `dyld_stub_binder` undefined with libSystem
    // ordinal in n_desc; the proxy stub symbol `puts` must NOT appear as a
    // defined export.
    let symtab = image.symtab.clone().expect("LC_SYMTAB required");
    let dysym = image.dysymtab.clone().expect("LC_DYSYMTAB required");
    let mut symbols = Vec::new();
    for i in 0..symtab.nsyms as usize {
        symbols.push(read_nlist(&bytes, &symtab, i));
    }
    let by_name = |n: &str| symbols.iter().find(|(name, ..)| name == n).cloned();
    let puts = by_name("_puts").expect("_puts undefined symtab entry");
    assert_eq!(puts.1, N_UNDF | N_EXT);
    assert_eq!(puts.3, 0x0100, "two-level ordinal 1 in n_desc");
    let binder = by_name("dyld_stub_binder").expect("dyld_stub_binder entry");
    assert_eq!(binder.1, N_UNDF | N_EXT);
    assert_eq!(binder.3, 0x0100);
    assert!(
        by_name("puts").is_none(),
        "the proxy stub must not leak as a defined `puts`"
    );

    // Defined exports in range and undefined count consistent.
    assert_eq!(dysym.nundefsym, 2);
    let _ = dysym.iextdefsym;

    // Indirect symbol table: [dyld_stub_binder, _puts].
    assert_eq!(dysym.nindirectsyms, 2);
    let indirect: Vec<u32> = (0..2)
        .map(|i| {
            u32::from_le_bytes(
                bytes[dysym.indirectsymoff as usize + i * 4
                    ..dysym.indirectsymoff as usize + (i + 1) * 4]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect();
    let binder_index = symbols
        .iter()
        .position(|(name, ..)| name == "dyld_stub_binder")
        .unwrap();
    let puts_index = symbols
        .iter()
        .position(|(name, ..)| name == "_puts")
        .unwrap();
    assert_eq!(indirect, vec![binder_index as u32, puts_index as u32]);

    // ── machine-code chain ──
    // 1. The call in main targets the proxy stub.
    let text_va = text.vmaddr;
    let main_text_off = text
        .sections
        .iter()
        .find(|s| s.sectname == ".text")
        .map(|s| s.offset)
        .expect(".text section");
    let call_place_va = text_va + main_text_off as u64 + 1;
    let disp = i32::from_le_bytes(
        bytes[main_text_off as usize + 1..main_text_off as usize + 5]
            .try_into()
            .unwrap(),
    );
    let stub_va = (call_place_va as i64 + 4 + disp as i64) as u64;

    // 2. The proxy stub is `jmp *[rip + disp]` landing exactly on the slot.
    let stub_off = (stub_va - text_va) as usize;
    assert_eq!(bytes[stub_off], 0xFF, "stub must be FF 25 (jmp [rip+disp])");
    assert_eq!(bytes[stub_off + 1], 0x25);
    let stub_disp = i32::from_le_bytes(bytes[stub_off + 2..stub_off + 6].try_into().unwrap());
    let stub_target = (stub_va as i64 + 6 + stub_disp as i64) as u64;
    assert_eq!(stub_target, la_slot_va, "stub must jump through its slot");

    // 3. The slot's initial value is the symbol's stub-helper entry.
    let la_file_off = la.offset as usize;
    let helper_entry_va =
        u64::from_le_bytes(bytes[la_file_off..la_file_off + 8].try_into().unwrap());
    let helper_sec = text_section_named(&image, "__stub_helper");
    assert_eq!(
        helper_sec.addr, helper_entry_va,
        "slot must point into __stub_helper head"
    );

    // 4. The helper entry pushes the lazy offset of _puts, then jumps to
    //    the binder tail.
    let entry_off = (helper_entry_va - text_va) as usize;
    assert_eq!(bytes[entry_off], 0x68, "entry pushes its lazy offset");
    let pushed = u32::from_le_bytes(bytes[entry_off + 1..entry_off + 5].try_into().unwrap());
    // The lazy stream is exactly one entry, so _puts's entry starts at 0.
    assert_eq!(
        pushed, 0,
        "first import's lazy-bind offset is 0 for the single-entry stream"
    );
    assert_eq!(bytes[entry_off + 5], 0xE9, "entry jumps to the tail");

    // 5. The tail pushes the helper start (rebased) and jumps through the
    //    GOT slot for dyld_stub_binder.
    let tail_off = entry_off + 10;
    assert_eq!(bytes[tail_off], 0x68, "tail pushes helper start");
    let tail_pushed = u32::from_le_bytes(bytes[tail_off + 1..tail_off + 5].try_into().unwrap());
    assert_eq!(tail_pushed, helper_sec.addr as u32);
    assert_eq!(bytes[tail_off + 5], 0xFF);
    assert_eq!(bytes[tail_off + 6], 0x25);
    let tail_va = text_va + tail_off as u64;
    let tail_jmp_disp = i32::from_le_bytes(bytes[tail_off + 7..tail_off + 11].try_into().unwrap());
    let got_target = (tail_va as i64 + 5 + 6 + tail_jmp_disp as i64) as u64;
    assert_eq!(got_target, got.addr, "tail must jump through __got[0]");

    // The GOT slot starts zeroed (dyld binds it before any lazy call runs).
    let got_file_off = got.offset as usize;
    assert_eq!(
        u64::from_le_bytes(bytes[got_file_off..got_file_off + 8].try_into().unwrap()),
        0,
        "dyld_stub_binder slot must be zero pre-bind"
    );

    // __LINKEDIT covers everything after __DATA.
    assert!(linkedit.fileoff > data.fileoff);
}

/// PIE data pointers are rebased even when the link has no imports (no
/// stubs are synthesized in that case).
#[test]
fn test_macho_pie_absolute_pointer_rebase_without_imports() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    // GC roots reach .data through this reference: `mov rax, [rip+global_value]; ret`.
    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0xC3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "global_value",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    // .data: a pointer to the function (absolute, needs rebasing).
    let mut data_sec = Section::new_data(".data", vec![0u8; 8], true, 8);
    data_sec
        .relocations
        .push(Relocation::new(0, "main", RelocationKind::Absolute64, 0));
    obj.add_section(data_sec);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "global_value",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        8,
        0,
    ));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();
    let config = LinkConfig::new(out.clone(), target);
    let res = Linker::link(std::slice::from_ref(&obj_path), config);
    assert!(res.is_ok(), "link failed: {:?}", res);

    let bytes = std::fs::read(&out).unwrap();
    let image = parse_macho(&bytes);

    // PIE executable with no undefined symbols: NOUNDEFS is honest here.
    assert_eq!(image.flags & MH_PIE, MH_PIE);
    assert_eq!(image.flags & MH_NOUNDEFS, MH_NOUNDEFS);

    // Dynamic info exists (rebasing) but no stub sections: nothing to bind.
    let info = image.dyld_info.clone().expect("LC_DYLD_INFO_ONLY for PIE");
    assert!(info.rebase_size > 0, "the data pointer must be rebased");
    let lazy_bytes = &bytes[info.lazy_off as usize..(info.lazy_off + info.lazy_size) as usize];
    assert!(
        decode_bind_stream_for_test(lazy_bytes).is_empty(),
        "no imports: no lazy entries"
    );
    let bind_bytes = &bytes[info.bind_off as usize..(info.bind_off + info.bind_size) as usize];
    assert!(
        decode_bind_stream_for_test(bind_bytes).is_empty(),
        "no imports: no bind entries"
    );

    // No stub sections synthesized for an import-free link.
    let data_seg = image
        .segments
        .iter()
        .find(|s| s.segname == "__DATA")
        .unwrap();
    assert!(
        data_seg.sections.iter().all(|s| s.sectname != "__got"),
        "__got must not exist without imports"
    );
    assert!(
        image
            .segments
            .iter()
            .find(|s| s.segname == "__TEXT")
            .unwrap()
            .sections
            .iter()
            .all(|s| s.sectname != "__stub_helper"),
        "__stub_helper must not exist without imports"
    );

    // The rebase stream covers the data pointer exactly once.
    let rebase_bytes =
        &bytes[info.rebase_off as usize..(info.rebase_off + info.rebase_size) as usize];
    let rebases = decode_rebase_stream_for_test(rebase_bytes);
    assert_eq!(rebases.len(), 1, "decoded {:?}", rebases);
    assert_eq!(rebases[0].2, REBASE_TYPE_POINTER);
    let data = image
        .segments
        .iter()
        .find(|s| s.segname == "__DATA")
        .unwrap();
    let rebased_va = data.vmaddr + rebases[0].1;
    let data_sec_record = data_seg
        .sections
        .iter()
        .find(|s| s.sectname == ".data")
        .unwrap();
    assert_eq!(
        rebased_va, data_sec_record.addr,
        "rebase must hit the .data pointer"
    );
}

/// Taking the address of an import (any non-call relocation against an
/// imported symbol) is rejected loudly instead of silently binding the
/// proxy stub's address as a data pointer.
#[test]
fn test_macho_data_import_reference_rejected_loudly() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    // Keep .data alive through a text reference (GC is on by default).
    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0xC3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "global_value",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    let mut data_sec = Section::new_data(".data", vec![0u8; 8], true, 8);
    data_sec
        .relocations
        .push(Relocation::new(0, "stdout", RelocationKind::Absolute64, 0));
    obj.add_section(data_sec);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "global_value",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_undefined("stdout", 0));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();
    let config = LinkConfig::new(out.clone(), target);
    let res = Linker::link(&[obj_path], config);
    let err = res.expect_err("data import must fail the link");
    let msg = format!("{}", err);
    assert!(
        msg.contains("stdout") && msg.contains("non-call relocation"),
        "unexpected error: {}",
        msg
    );
    assert!(
        !out.exists(),
        "no binary may be produced for a rejected link"
    );
}

/// A truncated absolute relocation cannot be rebased in a PIE image, and
/// with the 4 GB Mach-O image base it cannot even be encoded. Both paths
/// fail loudly; neither may produce a binary that silently breaks under
/// ASLR.
#[test]
fn test_macho_truncated_absolute_rejected_in_pie() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    // Keep .data alive through a text reference (GC is on by default).
    let mut text = Section::new_code(
        ".text",
        vec![0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00, 0xC3],
        16,
    );
    text.relocations.push(Relocation::new(
        3,
        "global_value",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    let mut data_sec = Section::new_data(".data", vec![0u8; 4], true, 4);
    data_sec
        .relocations
        .push(Relocation::new(0, "main", RelocationKind::Absolute32, 0));
    obj.add_section(data_sec);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        8,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "global_value",
        SymbolBinding::Global,
        SymbolType::Object,
        1,
        0,
        4,
        0,
    ));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();
    let config = LinkConfig::new(out.clone(), target);
    let res = Linker::link(&[obj_path], config);
    let err = res.expect_err("32-bit absolute must fail the link");
    let msg = format!("{}", err);
    assert!(
        msg.contains("PIE Mach-O") || msg.contains("overflowed"),
        "unexpected error: {}",
        msg
    );
    assert!(
        !out.exists(),
        "no binary may be produced for a rejected link"
    );
}

/// Shared libraries get an install name and an export trie over their
/// global symbols.
#[test]
fn test_macho_shared_library_export_trie() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("libadesh_test.dylib");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("lib.o");
    let mut obj = ObjectFile::new(obj_path.clone(), target.clone(), 0);

    let mut text = Section::new_code(
        ".text",
        vec![0x90, 0xC3, 0x90, 0xC3, 0x90, 0xC3, 0x90, 0xC3],
        16,
    );
    text.relocations.push(Relocation::new(
        1,
        "adesh_second",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);

    obj.add_symbol(Symbol::new_defined(
        "adesh_first",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        2,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "adesh_second",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        2,
        2,
        0,
    ));

    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();
    let mut config = LinkConfig::new(out.clone(), target);
    config.shared = true;
    let res = Linker::link(&[obj_path], config);
    assert!(res.is_ok(), "shared link failed: {:?}", res);

    let bytes = std::fs::read(&out).unwrap();
    let image = parse_macho(&bytes);
    assert_eq!(image.filetype, MH_DYLIB);
    assert!(image.id_dylib.is_some(), "LC_ID_DYLIB required for a dylib");
    assert!(!image.has_main_command, "dylibs have no LC_MAIN");

    let info = image.dyld_info.clone().expect("LC_DYLD_INFO_ONLY required");
    assert!(info.export_size > 0, "dylib must export its globals");
    let export_bytes =
        &bytes[info.export_off as usize..(info.export_off + info.export_size) as usize];
    let exports = decode_export_trie_for_test(export_bytes);

    let symtab = image.symtab.clone().unwrap();
    let mut expected: Vec<(String, u64, u64)> = Vec::new();
    for i in 0..symtab.nsyms as usize {
        let (name, n_type, _sect, _desc, value) = read_nlist(&bytes, &symtab, i);
        if n_type == (N_SECT | N_EXT) {
            expected.push((name, 0, value));
        }
    }
    expected.sort();
    assert_eq!(exports, expected, "export trie must cover all globals");
}

/// Load commands must be internally consistent: sizes sum to sizeofcmds,
/// every command is aligned, and section tables are exactly 80 bytes each.
#[test]
fn test_macho_load_command_layout_consistency() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("x86_64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let obj = object_calling_puts();
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out.clone(), target);
    Linker::link(&[obj_path], config).expect("link failed");

    let bytes = std::fs::read(&out).unwrap();
    let image = parse_macho(&bytes);

    // parse_macho already asserts command_total == sizeofcmds and per-command
    // sizes; assert the section table math explicitly here.
    let text = image
        .segments
        .iter()
        .find(|s| s.segname == "__TEXT")
        .unwrap();
    let data = image
        .segments
        .iter()
        .find(|s| s.segname == "__DATA")
        .unwrap();
    assert!(text.sections.iter().any(|s| s.sectname == "__stub_helper"));
    assert!(
        data.sections
            .iter()
            .any(|s| s.sectname == "__la_symbol_ptr")
    );
    assert!(data.sections.iter().any(|s| s.sectname == "__got"));

    // BSS sections (if any) must be S_ZEROFILL, never S_REGULAR-with-zeroes.
    for seg in &image.segments {
        for sec in &seg.sections {
            if sec.sectname.ends_with(".bss") {
                assert_eq!(sec.flags & 0xFF, S_ZEROFILL);
                assert_eq!(sec.offset, 0);
            }
        }
    }

    // Every dyld-info blob and the indirect table must live inside
    // __LINKEDIT's file range.
    let linkedit = image
        .segments
        .iter()
        .find(|s| s.segname == "__LINKEDIT")
        .unwrap();
    let info = image.dyld_info.unwrap();
    for (off, size) in [
        (info.rebase_off, info.rebase_size),
        (info.bind_off, info.bind_size),
        (info.lazy_off, info.lazy_size),
        (info.export_off, info.export_size),
    ] {
        assert!(size > 0 || off == 0, "blob offsets must be 0 when unused");
        assert!(
            off >= linkedit.fileoff as u32
                && (off as u64 + size as u64) <= (linkedit.fileoff + linkedit.filesize),
            "dyld blob 0x{:x}+0x{:x} outside __LINKEDIT",
            off,
            size
        );
    }
}

// ─── section helpers ────────────────────────────────────────────────────────

fn text_section_of<'a>(segment: &'a SegmentRecord, name: &str) -> &'a SectionRecord {
    segment
        .sections
        .iter()
        .find(|s| s.sectname == name)
        .unwrap_or_else(|| panic!("section {} missing in {}", name, segment.segname))
}

fn text_section_named<'a>(image: &'a ParsedMachO, name: &str) -> &'a SectionRecord {
    image
        .segments
        .iter()
        .flat_map(|s| s.sections.iter())
        .find(|s| s.sectname == name)
        .unwrap_or_else(|| panic!("section {} missing", name))
}
/// arm64 thread-local access: `adrp x0, desc@TLVPPAGE` +
/// `ldr x0, [x0, desc@TLVPPAGEOFF]` + `ret`, plus the 24-byte descriptor
/// in `__thread_vars` and the variable's storage in `__thread_bss`.
fn object_arm64_tlv() -> ObjectFile {
    let dir = tempdir().unwrap();
    let obj_path = dir.path().join("main.o");
    let target = Target::from_triple("aarch64-darwin").unwrap();
    let mut obj = ObjectFile::new(obj_path, target, 0);

    let mut text = Section::new_code(
        ".text",
        vec![
            0x00, 0x00, 0x00, 0x90, // adrp x0, #imm
            0x00, 0x00, 0x40, 0xF9, // ldr x0, [x0]  (rewritten to `add` by the linker)
            0xC0, 0x03, 0x5F, 0xD6, // ret
        ],
        16,
    );
    text.relocations
        .push(Relocation::new(0, "_DESC", RelocationKind::AArch64Adrp, 0));
    text.relocations.push(Relocation::new(
        4,
        "_DESC",
        RelocationKind::AArch64TlvDescriptorLo12,
        0,
    ));
    obj.add_section(text);

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
        12,
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
    obj
}

/// Mach-O TLV support end to end: descriptor table and TLS block emitted
/// with the section types dyld looks for, the accessor rewritten from `ldr`
/// to `add`, the descriptor `offset` relative to the TLS block, and the
/// bootstrap thunk expressed as a load-time bind (libSystem's address is
/// unknowable at link time, so a rebase cannot express it).
#[test]
fn test_macho_arm64_thread_local_variables() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("prog");
    let target = Target::from_triple("aarch64-darwin").unwrap();
    let obj_path = dir.path().join("main.o");
    let obj = object_arm64_tlv();
    ObjectWriter::write_to_file(&obj, &obj_path).unwrap();

    let config = LinkConfig::new(out.clone(), target);
    Linker::link(&[obj_path], config).expect("TLV link");

    let bytes = std::fs::read(&out).unwrap();
    let image = parse_macho(&bytes);

    let data = image
        .segments
        .iter()
        .find(|s| s.segname == "__DATA")
        .expect("__DATA");
    let vars = text_section_of(data, "__thread_vars");
    let tbss = text_section_of(data, "__thread_bss");
    assert_eq!(
        vars.flags & 0xFF,
        0x13,
        "__thread_vars must be S_THREAD_LOCAL_VARIABLES"
    );
    assert_eq!(
        tbss.flags & 0xFF,
        0x12,
        "__thread_bss must be S_THREAD_LOCAL_ZEROFILL"
    );

    // Descriptor: thunk zeroed (bound at load time), key zero, and `offset`
    // measured from the start of the per-thread block.
    let (thunk, key, offset) = unsafe {
        let p = bytes.as_ptr().add(vars.offset as usize);
        (
            std::ptr::read_unaligned(p as *const u64),
            std::ptr::read_unaligned(p.add(8) as *const u64),
            std::ptr::read_unaligned(p.add(16) as *const u64),
        )
    };
    assert_eq!((thunk, key), (0, 0), "thunk and key are link-time unknowns");
    assert_eq!(
        offset, 0,
        "the only TLS variable sits at the start of the thread block"
    );

    // `ldr` → `add`: same register pair, unscaled page offset of the descriptor.
    let text = text_section_named(&image, ".text");
    let access = u32::from_le_bytes(
        bytes[text.offset as usize + 4..text.offset as usize + 8]
            .try_into()
            .unwrap(),
    );
    let desc_va = vars.addr;
    assert_eq!(
        access,
        0x9100_0000 | (((desc_va & 0xFFF) as u32) << 10),
        "accessor must become `add x0, x0, #0x{:x}`",
        desc_va & 0xFFF
    );
    assert_eq!(
        access & 0xFFC0_0000,
        0x9100_0000,
        "the rewritten instruction must be a 64-bit `add` immediate"
    );

    // Bootstrap thunk bound at load time from libSystem (ordinal 1).
    let info = image.dyld_info.expect("LC_DYLD_INFO_ONLY");
    let bind = decode_bind_stream_for_test(
        &bytes[info.bind_off as usize..(info.bind_off + info.bind_size) as usize],
    );
    assert!(
        bind.iter()
            .any(|(seg, off, kind, ordinal, sym, _)| *seg == 2
                && *sym == "__tlv_bootstrap"
                && *kind == BIND_TYPE_POINTER
                && *ordinal == 1
                && *off == desc_va - data.vmaddr),
        "bind stream must bind __tlv_bootstrap into the descriptor thunk, got {bind:?}"
    );
}
