//! Phase 12 exit criterion 1, RISC-V half: the `AUIPC`/`ADDI` pair that
//! `adesh-codegen` emits for a symbol address must reconstruct that address in a
//! *linked image*, not merely in an object file.
//!
//! The hazard is the LO12 addend. RISC-V defines `%pcrel_lo(sym)` relative to the
//! `AUIPC`, but `linker/src/relocation.rs` computes `S + A - place_va` using each
//! relocation's own address. For an adjacent, uncompressed pair the `ADDI` sits
//! four bytes past its `AUIPC`, so the codegen passes `addend = 4` on
//! `RiscV_PcrelLo12I` to cancel that gap. Nothing in the object format enforces
//! the compensation, and a wrong pair still links — it just loads the wrong
//! address at run time. These tests read the patched immediates back out of the
//! executable and do the arithmetic the hardware would do.
//!
//! The codegen half of the same contract is pinned in
//! `tests/native_phase11_codegen_loudness_test.rs`
//! (`riscv64_materializes_a_symbol_address_with_auipc_and_addi`).
//!
//! No runtime archive is needed: `riscv64gc-unknown-none-elf` links statically and
//! the linker synthesizes `_start`, so the image is produced entirely from the
//! hand-built object below.
//!
//! The object is written with `adesh_object::AdobWriter` — the same encoder the
//! RISC-V backend's `emit_object` feeds — so the relocations travel the production
//! path (`AdobReader` → `AdobV2::decode_universal`) rather than the linker's own
//! `ObjectWriter`. That distinction matters: `AdobV2::encode` maps every
//! relocation kind it does not know to id 0 (`Absolute64`), and its kind table has
//! no RISC-V entries at all, so a RISC-V object written through it arrives with
//! 8-byte absolute relocations instead of 4-byte PC-relative ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use adesh_linker::config::LinkConfig;
use adesh_linker::linker::Linker;
use adesh_linker::target::Target;
use adesh_object::{
    AdobObject, AdobRelocation, AdobSection, AdobSymbol, AdobWriter, RelocationKind, SectionKind,
    SymbolBinding, SymbolKind, TargetDescriptor, section_flags,
};

/// `auipc t0, 0` — the relocation patches bits 31:12 and must leave rd = x5.
const AUIPC_T0: u32 = 0x0000_0297;
/// `addi t0, t0, 0` — the relocation patches bits 31:20 and must leave rd/rs1.
const ADDI_T0_T0: u32 = 0x0002_8293;
/// `addi x0, x0, 0`, used as padding so `.text` can be grown to a known size.
const RV_NOP: u32 = 0x0000_0013;

const PAIR_STRIDE: usize = 8;
/// Data symbols are spaced further apart than the pairs that reference them, so
/// the set of `S - P` values spans more than 4096 bytes and therefore covers
/// every possible low-12 residue — including the ones with bit 11 set, where
/// `hi20` has to round up and the hardware sign-extends `lo12` to a negative.
const SYMBOL_STRIDE: usize = 0x110;
const PAIR_COUNT: usize = 17;

const TRIPLE: &str = "riscv64gc-unknown-none-elf";

fn target() -> Target {
    Target::from_triple(TRIPLE).expect("riscv64 bare-metal target")
}

/// Write an object whose `.text` materializes `__str_<i>` for `i` in
/// `0..PAIR_COUNT` and whose `.rodata` defines those symbols.
///
/// `lo12_addend` is the value under test: 4 is what codegen emits, 0 is the
/// uncompensated form that must produce a wrong address.
fn written_pair_object(dir: &Path, name: &str, lo12_addend: i64) -> PathBuf {
    let mut obj = AdobObject::new(descriptor());

    let mut text = Vec::with_capacity(PAIR_COUNT * PAIR_STRIDE);
    let mut relocs = Vec::with_capacity(PAIR_COUNT * 2);
    for i in 0..PAIR_COUNT {
        let symbol = format!("__str_{i}");
        let hi_offset = text.len();
        text.extend_from_slice(&AUIPC_T0.to_le_bytes());
        relocs.push(AdobRelocation::new(
            hi_offset as u64,
            0,
            symbol.clone(),
            RelocationKind::RiscV_PcrelHi20,
            0,
        ));

        let lo_offset = text.len();
        text.extend_from_slice(&ADDI_T0_T0.to_le_bytes());
        relocs.push(AdobRelocation::new(
            lo_offset as u64,
            0,
            symbol,
            RelocationKind::RiscV_PcrelLo12I,
            lo12_addend,
        ));
    }
    let mut text_section = AdobSection::new(".text", SectionKind::Text)
        .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
        .with_alignment(16)
        .with_data(text);
    text_section.relocations = relocs;
    obj.add_section(text_section);

    let rodata_size = (PAIR_COUNT - 1) * SYMBOL_STRIDE + 8;
    let rodata: Vec<u8> = (0..rodata_size).map(|i| (i % 251) as u8).collect();
    obj.add_section(
        AdobSection::new(".rodata", SectionKind::Rodata)
            .with_flags(section_flags::READ | section_flags::ALLOC)
            .with_alignment(8)
            .with_data(rodata),
    );

    // `main` roots `.text` for section GC and gives the synthesized `_start`
    // something to call.
    obj.add_symbol(
        AdobSymbol::new_defined(
            0,
            "main",
            SymbolKind::Function,
            0,
            0,
            (PAIR_COUNT * PAIR_STRIDE) as u64,
        )
        .with_binding(SymbolBinding::Global),
    );
    for i in 0..PAIR_COUNT {
        obj.add_symbol(
            AdobSymbol::new_defined(
                0,
                format!("__str_{i}"),
                SymbolKind::Object,
                1,
                (i * SYMBOL_STRIDE) as u64,
                1,
            )
            .with_binding(SymbolBinding::Local),
        );
    }

    write_object(&obj, dir, name)
}

fn descriptor() -> TargetDescriptor {
    TargetDescriptor::from_triple(TRIPLE).expect("riscv64 bare-metal descriptor")
}

fn write_object(obj: &AdobObject, dir: &Path, name: &str) -> PathBuf {
    let bytes = AdobWriter::write(obj).expect("encode ADOB object");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write ADOB object");
    path
}

fn link(dir: &Path, object: PathBuf, out_name: &str) -> Vec<u8> {
    let out_path = dir.join(out_name);
    let config = LinkConfig::new(out_path.clone(), target());
    Linker::link(&[object], config).expect("riscv64 static link must succeed");
    let bytes = std::fs::read(&out_path).expect("read linked image");
    assert_eq!(
        &bytes[0..4],
        b"\x7fELF",
        "{} is not an ELF image",
        out_path.display()
    );
    assert_eq!(bytes[4], 2, "riscv64 must produce ELF64");
    assert_eq!(
        u16le(&bytes, 0x12, "e_machine"),
        243,
        "e_machine must be EM_RISCV"
    );
    bytes
}

/// One materialized address, decoded from the image the way the hardware would.
struct Pair {
    index: usize,
    auipc_va: u64,
    hi20: u32,
    lo12: u32,
    reconstructed: i64,
    expected: i64,
}

impl Pair {
    /// `AUIPC` writes `pc + (hi20 << 12)`; `ADDI` adds the sign-extend of `lo12`.
    fn decode(bytes: &[u8], text: &ElfSection, rodata: &ElfSection, index: usize) -> Self {
        let pair_offset = index * PAIR_STRIDE;
        let auipc = word(bytes, text.offset + pair_offset, "AUIPC");
        let addi = word(bytes, text.offset + pair_offset + 4, "ADDI");

        assert_eq!(
            auipc & 0x0000_0FFF,
            AUIPC_T0 & 0x0000_0FFF,
            "pair {index}: the HI20 fixup clobbered the AUIPC opcode or rd"
        );
        assert_eq!(
            addi & 0x000F_FFFF,
            ADDI_T0_T0 & 0x000F_FFFF,
            "pair {index}: the LO12 fixup clobbered the ADDI opcode, rd or rs1"
        );

        let hi20 = sext(auipc >> 12, 20);
        let lo12 = addi >> 20;
        let auipc_va = text.addr + pair_offset as u64;
        let reconstructed = auipc_va as i64 + (hi20 << 12) + sext(lo12, 12);
        let expected = (rodata.addr + (index * SYMBOL_STRIDE) as u64) as i64;

        Self {
            index,
            auipc_va,
            hi20: hi20 as u32,
            lo12,
            reconstructed,
            expected,
        }
    }

    fn assert_exact(&self) {
        assert_eq!(
            self.reconstructed,
            self.expected,
            "pair {} at {:#x}: AUIPC hi20={:#07x} + ADDI lo12={:#05x} reconstructs \
             {:#x}, but __str_{} is at {:#x}. The LO12 half must be relative to the \
             AUIPC, so the codegen's +4 addend is what makes this exact.",
            self.index,
            self.auipc_va,
            self.hi20,
            self.lo12,
            self.reconstructed,
            self.index,
            self.expected
        );
    }
}

fn decode_pairs(bytes: &[u8]) -> Vec<Pair> {
    let text = elf_section(bytes, ".text");
    let rodata = elf_section(bytes, ".rodata");
    assert!(
        text.size >= (PAIR_COUNT * PAIR_STRIDE) as u64,
        ".text is {} bytes, too small for {PAIR_COUNT} pairs — section GC dropped the code",
        text.size
    );
    assert!(
        rodata.size >= ((PAIR_COUNT - 1) * SYMBOL_STRIDE + 1) as u64,
        ".rodata is {} bytes, too small for the string pool — section GC dropped the data \
         the relocations reference",
        rodata.size
    );
    (0..PAIR_COUNT)
        .map(|i| Pair::decode(bytes, &text, &rodata, i))
        .collect()
}

#[test]
fn riscv64_pcrel_pair_reconstructs_the_symbol_address_in_a_linked_image() {
    let dir = tempfile::tempdir().unwrap();
    let object = written_pair_object(dir.path(), "pairs.o", 4);
    let bytes = link(dir.path(), object, "pairs-app");

    let pairs = decode_pairs(&bytes);
    for pair in &pairs {
        pair.assert_exact();
    }
    if std::env::var_os("ADESH_MEASURE_RISCV_PAIRS").is_some() {
        let text = elf_section(&bytes, ".text");
        let rodata = elf_section(&bytes, ".rodata");
        eprintln!(
            "MEASURE .text addr={:#x} size={} .rodata addr={:#x} size={} delta={:#x}",
            text.addr,
            text.size,
            rodata.addr,
            rodata.size,
            rodata.addr as i64 - text.addr as i64
        );
        for p in &pairs {
            eprintln!(
                "MEASURE pair {:2} auipc_va={:#x} hi20={:#07x} lo12={:#05x} recon={:#x} sym={:#x} neg_low={}",
                p.index, p.auipc_va, p.hi20, p.lo12, p.reconstructed, p.expected, p.lo12 & 0x800 != 0
            );
        }
    }

    // The span is only interesting if it actually reached the sign-extended
    // half of the low-12 range; otherwise the `+0x800` rounding in the HI20
    // fixup is never exercised and this suite would pass on a broken linker.
    let negative_low: Vec<usize> = pairs
        .iter()
        .filter(|p| p.lo12 & 0x800 != 0)
        .map(|p| p.index)
        .collect();
    assert!(
        !negative_low.is_empty(),
        "no pair had lo12 bit 11 set, so the HI20 round-up and the hardware's \
         sign-extension of LO12 were never exercised: {:?}",
        pairs.iter().map(|p| p.lo12).collect::<Vec<_>>()
    );

    let rounded_up: Vec<usize> = pairs
        .iter()
        .filter(|p| {
            let delta = p.expected - p.auipc_va as i64;
            (sext(p.hi20, 20)) << 12 > delta
        })
        .map(|p| p.index)
        .collect();
    assert!(
        !rounded_up.is_empty(),
        "no pair needed hi20 to round up past the true delta, so the +0x800 in the \
         HI20 fixup is untested"
    );
}

/// The A/B that makes the `+4` load-bearing rather than incidental: the same
/// object with an uncompensated LO12 addend still links, and still produces
/// well-formed instructions, but materializes the wrong address.
#[test]
fn riscv64_lo12_without_the_plus_four_addend_materializes_the_wrong_address() {
    let dir = tempfile::tempdir().unwrap();
    let object = written_pair_object(dir.path(), "uncompensated.o", 0);
    let bytes = link(dir.path(), object, "uncompensated-app");

    let pairs = decode_pairs(&bytes);
    let mut off_by_four = 0;
    for pair in &pairs {
        assert_ne!(
            pair.reconstructed, pair.expected,
            "pair {} reconstructed the right address without the +4 addend, which \
             means this test is not measuring the compensation at all",
            pair.index
        );
        if pair.reconstructed == pair.expected - 4 {
            off_by_four += 1;
        }
    }
    assert!(
        off_by_four > 0,
        "expected at least one pair to land exactly four bytes low — the gap between \
         the AUIPC and its ADDI — but none did: {:?}",
        pairs
            .iter()
            .map(|p| (p.index, p.reconstructed - p.expected))
            .collect::<Vec<_>>()
    );
}

/// A backward reference: the pair sits far above its target in the same section,
/// so `S + A - P` is negative and the HI20 fixup has to arithmetic-shift a
/// negative value. Taking a function's address is a real Adesh operation, and
/// this is the shape it lowers to.
#[test]
fn riscv64_pcrel_pair_handles_a_backward_reference_to_a_function() {
    const BACKWARD_OFFSET: usize = 0x2000;

    let dir = tempfile::tempdir().unwrap();
    let mut obj = AdobObject::new(descriptor());

    let mut text = vec![0u8; BACKWARD_OFFSET + PAIR_STRIDE];
    for chunk in text[..BACKWARD_OFFSET].chunks_exact_mut(4) {
        chunk.copy_from_slice(&RV_NOP.to_le_bytes());
    }
    text[BACKWARD_OFFSET..BACKWARD_OFFSET + 4].copy_from_slice(&AUIPC_T0.to_le_bytes());
    text[BACKWARD_OFFSET + 4..BACKWARD_OFFSET + 8].copy_from_slice(&ADDI_T0_T0.to_le_bytes());

    let mut text_section = AdobSection::new(".text", SectionKind::Text)
        .with_flags(section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC)
        .with_alignment(16)
        .with_data(text);
    text_section.relocations = vec![
        AdobRelocation::new(
            BACKWARD_OFFSET as u64,
            0,
            "main",
            RelocationKind::RiscV_PcrelHi20,
            0,
        ),
        AdobRelocation::new(
            (BACKWARD_OFFSET + 4) as u64,
            0,
            "main",
            RelocationKind::RiscV_PcrelLo12I,
            4,
        ),
    ];
    obj.add_section(text_section);
    obj.add_symbol(
        AdobSymbol::new_defined(
            0,
            "main",
            SymbolKind::Function,
            0,
            0,
            (BACKWARD_OFFSET + PAIR_STRIDE) as u64,
        )
        .with_binding(SymbolBinding::Global),
    );
    let object_path = write_object(&obj, dir.path(), "backward.o");

    let bytes = link(dir.path(), object_path, "backward-app");
    let text = elf_section(&bytes, ".text");
    assert!(
        text.size >= (BACKWARD_OFFSET + PAIR_STRIDE) as u64,
        ".text is {} bytes, so the backward pair was dropped",
        text.size
    );

    let auipc = word(&bytes, text.offset + BACKWARD_OFFSET, "AUIPC");
    let addi = word(&bytes, text.offset + BACKWARD_OFFSET + 4, "ADDI");
    assert_eq!(auipc & 0xFFF, AUIPC_T0 & 0xFFF, "AUIPC opcode/rd clobbered");
    assert_eq!(
        addi & 0x000F_FFFF,
        ADDI_T0_T0 & 0x000F_FFFF,
        "ADDI opcode/rd/rs1 clobbered"
    );

    let hi20 = auipc >> 12;
    let lo12 = addi >> 20;
    let pair_va = text.addr + BACKWARD_OFFSET as u64;
    let reconstructed = pair_va as i64 + (sext(hi20, 20) << 12) + sext(lo12, 12);

    assert_eq!(
        reconstructed, text.addr as i64,
        "a pair at .text+{BACKWARD_OFFSET:#x} referencing `main` at .text+0 \
         reconstructed {reconstructed:#x} (hi20={hi20:#07x} lo12={lo12:#05x}); \
         the negative delta needs an arithmetic shift, not a logical one"
    );
    assert_eq!(lo12, 0, "a -0x2000 delta has a zero low half");
    assert_eq!(
        sext(hi20, 20),
        -2,
        "(-0x2000 + 0x800) >> 12 must be -2, got {hi20:#07x}"
    );
}

/// Sign-extend the low `bits` of an immediate field, as the hardware does.
fn sext(value: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    (((value << shift) as i32) >> shift) as i64
}

struct ElfSection {
    addr: u64,
    offset: usize,
    size: u64,
}

/// Locate a section by name through `e_shstrndx`, and report both its virtual
/// address and its file offset. Reading the symtab instead would not work: the
/// default link strips symbols, and `__str_<n>` is local to begin with.
fn elf_section(bytes: &[u8], name: &str) -> ElfSection {
    let shoff = u64le(bytes, 0x28, "e_shoff") as usize;
    let shentsize = u16le(bytes, 0x3A, "e_shentsize") as usize;
    let shnum = u16le(bytes, 0x3C, "e_shnum") as usize;
    let shstrndx = u16le(bytes, 0x3E, "e_shstrndx") as usize;
    assert!(shoff != 0 && shnum > 0, "image has no section headers");
    assert!(shstrndx < shnum, "e_shstrndx {shstrndx} out of range");

    let strtab_base = shoff + shstrndx * shentsize;
    let strtab_offset = u64le(bytes, strtab_base + 24, "shstrtab sh_offset") as usize;
    let strtab_size = u64le(bytes, strtab_base + 32, "shstrtab sh_size") as usize;

    let mut seen: HashMap<String, usize> = HashMap::new();
    for index in 0..shnum {
        let base = shoff + index * shentsize;
        let name_offset = u32le(bytes, base, "sh_name") as usize;
        let start = strtab_offset + name_offset;
        let end = (strtab_offset + strtab_size).min(bytes.len());
        assert!(start <= end, "section name {index} runs past the section-name table");
        let section_name = String::from_utf8_lossy(&bytes[start..end])
            .split('\0')
            .next()
            .unwrap_or("")
            .to_string();
        seen.insert(section_name.clone(), index);
        if section_name == name {
            return ElfSection {
                addr: u64le(bytes, base + 16, "sh_addr"),
                offset: u64le(bytes, base + 24, "sh_offset") as usize,
                size: u64le(bytes, base + 32, "sh_size"),
            };
        }
    }
    panic!(
        "no section named {name} in the linked image; sections present: {:?}",
        seen.keys().collect::<Vec<_>>()
    );
}

fn word(bytes: &[u8], offset: usize, what: &str) -> u32 {
    u32le(bytes, offset, what)
}

fn u16le(bytes: &[u8], offset: usize, what: &str) -> u16 {
    let slice = bytes
        .get(offset..offset + 2)
        .unwrap_or_else(|| panic!("{what} at {offset:#x} is past the image end"));
    u16::from_le_bytes([slice[0], slice[1]])
}

fn u32le(bytes: &[u8], offset: usize, what: &str) -> u32 {
    let slice = bytes
        .get(offset..offset + 4)
        .unwrap_or_else(|| panic!("{what} at {offset:#x} is past the image end"));
    u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
}

fn u64le(bytes: &[u8], offset: usize, what: &str) -> u64 {
    let slice = bytes
        .get(offset..offset + 8)
        .unwrap_or_else(|| panic!("{what} at {offset:#x} is past the image end"));
    u64::from_le_bytes(slice.try_into().expect("8 bytes"))
}