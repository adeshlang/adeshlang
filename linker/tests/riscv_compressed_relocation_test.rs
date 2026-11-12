use std::path::Path;

use adesh_linker::elf::{header::*, reader::ElfReader, reloc::relocation_kind_to_elf_type};
use adesh_linker::error::ErrorCode;
use adesh_linker::object::{ObjectFile, adob::AdobV2};
use adesh_linker::relocation::{
    DefaultRelocationHandler, Relocation, RelocationHandler, RelocationKind,
};
use adesh_linker::section::Section;
use adesh_linker::target::{Arch, Target};

fn patch(kind: RelocationKind, displacement: i64, initial: u16) -> Vec<u8> {
    let mut bytes = vec![0x5a; 0x8a];
    bytes[0x86..0x88].copy_from_slice(&initial.to_le_bytes());
    let addend = displacement - 32;
    let reloc = Relocation::new(0x86, "target", kind, addend);
    // Distinct S and P plus a nonzero addend verify all terms of S + A - P.
    DefaultRelocationHandler::LITTLE
        .apply(&reloc, 0x4000, 0x4020, addend, &mut bytes)
        .unwrap();
    assert!(bytes[..0x86].iter().all(|&b| b == 0x5a));
    assert_eq!(&bytes[0x88..], &[0x5a, 0x5a]);
    bytes[0x86..0x88].to_vec()
}

#[test]
fn uleb_pair_encodes_only_final_difference_and_preserves_span() {
    use adesh_linker::relocation::apply_riscv_uleb128_pair;
    let set = Relocation::new(1, "end", RelocationKind::RiscvSetUleb128, 3);
    let sub = Relocation::new(1, "start", RelocationKind::RiscvSubUleb128, -2);
    for (field, difference, expected) in [
        (vec![0], 127, vec![127]),
        (vec![0x80, 0], 128, vec![0x80, 1]),
        (vec![0xff, 0x80, 0], 127, vec![0xff, 0x80, 0]),
        (vec![0x80, 0x80, 0], 0, vec![0x80, 0x80, 0]),
    ] {
        let mut bytes = vec![0x5a];
        bytes.extend_from_slice(&field);
        bytes.push(0x5a);
        apply_riscv_uleb128_pair(
            &set,
            &sub,
            u64::MAX - 1000 + difference - 5,
            u64::MAX - 1000,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(&bytes[1..1 + field.len()], expected);
        assert_eq!(bytes[0], 0x5a);
        assert_eq!(*bytes.last().unwrap(), 0x5a);
    }
    for (bytes, end, start) in [
        (vec![0x5a, 0], 128, 5),  // 128 overflows one byte
        (vec![0x5a, 0], 0, 6),    // negative
        (vec![0x5a, 0x80], 0, 5), // unterminated
        (vec![0x5a], 0, 5),       // absent
    ] {
        let mut data = bytes.clone();
        assert!(apply_riscv_uleb128_pair(&set, &sub, end, start, &mut data).is_err());
        assert_eq!(data, bytes);
    }
    for (first, second) in [
        (sub.clone(), set.clone()),
        (
            set.clone(),
            Relocation::new(2, "start", RelocationKind::RiscvSubUleb128, 0),
        ),
        (
            set.clone(),
            Relocation::new(1, "start", RelocationKind::RiscvAdd8, 0),
        ),
    ] {
        let mut data = vec![0; 4];
        assert!(apply_riscv_uleb128_pair(&first, &second, 1, 0, &mut data).is_err());
        assert_eq!(data, vec![0; 4]);
    }
    // Both S+A terms exceed u64 without overflowing the wide subtraction.
    let wide_set = Relocation::new(0, "end", RelocationKind::RiscvSetUleb128, 127);
    let wide_sub = Relocation::new(0, "start", RelocationKind::RiscvSubUleb128, 1);
    let mut data = vec![0];
    apply_riscv_uleb128_pair(&wide_set, &wide_sub, u64::MAX, u64::MAX, &mut data).unwrap();
    assert_eq!(data, vec![126]);
    for r in [&set, &sub] {
        let mut data = vec![0; 4];
        assert!(
            DefaultRelocationHandler::LITTLE
                .apply(r, 0, 0, 0, &mut data)
                .is_err()
        );
        assert_eq!(data, vec![0; 4]);
    }
}

#[test]
fn uleb_elf_pairs_require_raw_adjacency_and_bounded_fields() {
    let valid = elf_fixture(&[60, 61]);
    let obj = ElfReader::read(&valid, Path::new("uleb.o"), 7).unwrap();
    for (i, kind) in [
        RelocationKind::RiscvSetUleb128,
        RelocationKind::RiscvSubUleb128,
    ]
    .into_iter()
    .enumerate()
    {
        let r = &obj.sections[0].relocations[i];
        assert_eq!(r.kind, kind);
        assert_eq!(r.symbol_index, Some(1));
        assert_eq!(r.addend, -2);
        assert_eq!(kind.size_in_bytes(), 1);
        for arch in [Arch::Riscv32, Arch::Riscv64] {
            assert_eq!(relocation_kind_to_elf_type(arch, kind), 60 + i as u32);
        }
    }
    assert_eq!(
        AdobV2::encode(&obj).unwrap_err().code,
        ErrorCode::UnsupportedRelocation
    );
    for types in [
        vec![60],
        vec![61],
        vec![61, 60],
        vec![60, 0, 61],
        vec![60, 51, 61],
        vec![60, 33, 61],
    ] {
        assert!(ElfReader::read(&elf_fixture(&types), Path::new("bad.o"), 0).is_err());
    }
    let text = 64 + 5 * 64;
    let rela = text + 0x88;
    let mut mismatch = valid.clone();
    mismatch[rela + 24..rela + 32].copy_from_slice(&0x87u64.to_le_bytes());
    assert!(ElfReader::read(&mismatch, Path::new("bad.o"), 0).is_err());
    let mut unterminated = valid.clone();
    unterminated[text + 0x86..text + 0x88].fill(0x80);
    assert!(ElfReader::read(&unterminated, Path::new("bad.o"), 0).is_err());
    for kind in [
        adesh_linker::section::SectionKind::Bss,
        adesh_linker::section::SectionKind::TBss,
    ] {
        let mut bss = obj.clone();
        bss.sections[0].kind = kind;
        assert!(bss.validate().is_err());
    }
    for offset in [0x88, u64::MAX] {
        let mut bad = obj.clone();
        for r in &mut bad.sections[0].relocations {
            r.offset = offset;
        }
        assert!(bad.validate().is_err());
    }
}

#[test]
fn uleb_elf_rejects_implicit_addends() {
    for types in [vec![60, 61], vec![61]] {
        let mut bytes = elf_fixture(&types);
        let header = 64 + 2 * 64;
        let rel = 64 + 5 * 64 + 0x88;
        // Compact RELA entries to actual ELF64 REL entries, without addends.
        for i in 0..types.len() {
            bytes.copy_within(rel + i * 24..rel + i * 24 + 16, rel + i * 16);
        }
        bytes[header + 4..header + 8].copy_from_slice(&SHT_REL.to_le_bytes());
        bytes[header + 32..header + 40].copy_from_slice(&((types.len() * 16) as u64).to_le_bytes());
        bytes[header + 56..header + 64].copy_from_slice(&16u64.to_le_bytes());
        let err = ElfReader::read(&bytes, Path::new("implicit.o"), 0).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
        assert!(
            err.to_string()
                .contains("SHT_REL implicit addends are unsupported")
        );
    }
}

#[test]
fn uleb_pair_links_real_elf() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("uleb.o");
    let output = dir.path().join("uleb-app");
    let mut bytes = elf_fixture(&[60, 61]);
    let text = 64 + 5 * 64;
    let rela = text + 0x88;
    let symtab = rela + 48;
    // A section-defined symbol cancels out; addends alone give a small difference.
    bytes[symtab + 28] = 0; // STB_LOCAL
    bytes[symtab + 30..symtab + 32].copy_from_slice(&1u16.to_le_bytes());
    bytes[rela + 16..rela + 24].copy_from_slice(&126i64.to_le_bytes());
    bytes[text + 0x86..text + 0x88].copy_from_slice(&[0x80, 0]);
    check_uleb_link(&input, &output, &bytes);

    // Two same-named local section symbols must resolve by raw symbol index,
    // not a name map: end - start = 128, independent of their output VA.
    let mut precise = elf_fixture(&[60, 61]);
    let second = precise[symtab + 24..symtab + 48].to_vec();
    precise.splice(symtab + 48..symtab + 48, second);
    precise[64 + 3 * 64 + 32..64 + 3 * 64 + 40].copy_from_slice(&72u64.to_le_bytes());
    precise[64 + 4 * 64 + 24..64 + 4 * 64 + 32]
        .copy_from_slice(&((symtab + 72) as u64).to_le_bytes());
    for (symbol, value) in [(symtab + 24, 128u64), (symtab + 48, 0)] {
        precise[symbol + 4] = 3; // STB_LOCAL / STT_SECTION
        precise[symbol + 6..symbol + 8].copy_from_slice(&1u16.to_le_bytes());
        precise[symbol + 8..symbol + 16].copy_from_slice(&value.to_le_bytes());
    }
    precise[rela + 32..rela + 40].copy_from_slice(&((2u64 << 32) | 61).to_le_bytes());
    precise[text + 0x86..text + 0x88].copy_from_slice(&[0x80, 0]);
    check_uleb_link(&input, &output, &precise);
}

fn check_uleb_link(input: &Path, output: &Path, bytes: &[u8]) {
    use adesh_linker::{config::LinkConfig, linker::Linker};
    // Supply a real entry so startup synthesis cannot introduce unrelated imports.
    let mut bytes = bytes.to_vec();
    let sym_header = 64 + 3 * 64;
    let str_header = 64 + 4 * 64;
    let str_offset =
        u64::from_le_bytes(bytes[str_header + 24..str_header + 32].try_into().unwrap()) as usize;
    let sym_size = u64::from_le_bytes(bytes[sym_header + 32..sym_header + 40].try_into().unwrap());
    let mut entry = vec![0; 24];
    entry[..4].copy_from_slice(&34u32.to_le_bytes());
    entry[4] = 0x12; // STB_GLOBAL / STT_FUNC
    entry[6..8].copy_from_slice(&1u16.to_le_bytes());
    bytes.splice(str_offset..str_offset, entry);
    bytes[sym_header + 32..sym_header + 40].copy_from_slice(&(sym_size + 24).to_le_bytes());
    bytes[str_header + 24..str_header + 32]
        .copy_from_slice(&((str_offset + 24) as u64).to_le_bytes());
    bytes[str_offset + 24 + 34..str_offset + 24 + 40].copy_from_slice(b"_start");
    std::fs::write(input, bytes).unwrap();
    let mut config = LinkConfig::new(
        output.to_path_buf(),
        Target::from_triple("riscv64gc-unknown-linux-gnu").unwrap(),
    );
    config.entry_point = Some("_start".into());
    config.gc_sections = false;
    Linker::link(&[input.to_path_buf()], config).unwrap();
    let image = std::fs::read(output).unwrap();
    // Locate .text itself: executable PT_LOAD may also contain ELF headers.
    let shoff = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
    let shnum = u16::from_le_bytes(image[60..62].try_into().unwrap()) as usize;
    let shsize = u16::from_le_bytes(image[58..60].try_into().unwrap()) as usize;
    let stridx = u16::from_le_bytes(image[62..64].try_into().unwrap()) as usize;
    let strings = shoff + stridx * shsize;
    let strings =
        u64::from_le_bytes(image[strings + 24..strings + 32].try_into().unwrap()) as usize;
    let section = (0..shnum)
        .map(|i| shoff + i * shsize)
        .find(|&s| {
            let name = u32::from_le_bytes(image[s..s + 4].try_into().unwrap()) as usize;
            image[strings + name..].starts_with(b".text\0")
        })
        .unwrap();
    let off = u64::from_le_bytes(image[section + 24..section + 32].try_into().unwrap()) as usize;
    assert_eq!(&image[off + 0x86..off + 0x88], &[0x80, 1]);
}

fn additive_kinds() -> [(RelocationKind, RelocationKind, usize); 4] {
    use RelocationKind::*;
    [
        (RiscvAdd8, RiscvSub8, 1),
        (RiscvAdd16, RiscvSub16, 2),
        (RiscvAdd32, RiscvSub32, 4),
        (RiscvAdd64, RiscvSub64, 8),
    ]
}

#[test]
fn additive_data_fields_wrap_and_preserve_neighbors_in_both_byte_orders() {
    for handler in [
        DefaultRelocationHandler::LITTLE,
        DefaultRelocationHandler::BIG,
    ] {
        for (add, sub, width) in additive_kinds() {
            assert_eq!(add.size_in_bytes(), width);
            assert_eq!(sub.size_in_bytes(), width);
            let encode = |value: u64| -> Vec<u8> {
                if handler.endianness.is_big() {
                    value.to_be_bytes()[8 - width..].to_vec()
                } else {
                    value.to_le_bytes()[..width].to_vec()
                }
            };
            // Paired difference: nonzero V + (end - 7) - (start - 3).
            // Also cover over/underflow, negative S+A, and full u64 wrapping.
            for (initial, operations, expected) in [
                (0x1234, vec![(add, 0x4200, -7), (sub, 0x4100, -3)], 0x1330),
                (u64::MAX, vec![(add, 5, -3)], 1),
                (1, vec![(sub, 5, -2)], u64::MAX - 1),
                (1, vec![(add, 0, -2)], u64::MAX),
                (1, vec![(sub, 0, -2)], 3),
                (9, vec![(add, u64::MAX, 2)], 10),
                (9, vec![(sub, u64::MAX, 2)], 8),
            ] {
                let mut bytes = vec![0x5a; width + 2];
                bytes[1..1 + width].copy_from_slice(&encode(initial));
                for (kind, symbol, addend) in operations {
                    let reloc = Relocation::new(1, "target", kind, addend);
                    handler
                        .apply(&reloc, 0x9000, symbol, addend, &mut bytes)
                        .unwrap();
                }
                assert_eq!(&bytes[1..1 + width], encode(expected));
                assert_eq!(bytes[0], 0x5a);
                assert_eq!(bytes[width + 1], 0x5a);
            }
            for kind in [add, sub] {
                let mut bytes = vec![0x5a; width];
                let reloc = Relocation::new(1, "target", kind, 0);
                assert_eq!(
                    handler.apply(&reloc, 0, 0, 0, &mut bytes).unwrap_err().code,
                    ErrorCode::RelocationOverflow
                );
                assert_eq!(bytes, vec![0x5a; width]);
            }
        }
    }
}

#[test]
fn additive_elf_types_roundtrip_without_invented_adob_ids() {
    let target = Target::from_triple("riscv64gc-unknown-linux-gnu").unwrap();
    for (i, (add, sub, width)) in additive_kinds().into_iter().enumerate() {
        for (ty, kind) in [(33 + i as u32, add), (37 + i as u32, sub)] {
            let mut fixture = elf_fixture(&[ty]);
            // Place the field safely inside the section even for ADD64/SUB64.
            let rela = 64 + 5 * 64 + 0x88;
            fixture[rela..rela + 8].copy_from_slice(&0x78u64.to_le_bytes());
            let decoded = ElfReader::read(&fixture, Path::new("additive.o"), 7).unwrap();
            let reloc = &decoded.find_section(".text").unwrap().relocations[0];
            assert_eq!(reloc.kind, kind);
            assert_eq!(reloc.addend, -2);
            assert_eq!(reloc.offset, 0x78);
            assert_eq!(reloc.symbol_index, Some(1));
            for arch in [Arch::Riscv32, Arch::Riscv64] {
                assert_eq!(relocation_kind_to_elf_type(arch, reloc.kind), ty);
            }
            let mut obj = ObjectFile::new("additive.adob".into(), target.clone(), 0);
            let mut section = Section::new_code(".text", vec![0; width], 1);
            section
                .relocations
                .push(Relocation::new(0, "target", kind, -2));
            obj.add_section(section);
            let err = AdobV2::encode(&obj).unwrap_err();
            assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
            assert!(err.to_string().contains(kind.name()));
        }
    }
}

#[test]
fn compressed_exact_boundary_bytes_and_preserved_fields() {
    use RelocationKind::{RiscvRvcBranch, RiscvRvcJump};
    for (kind, initial, cases) in [
        (
            RiscvRvcJump,
            0xbffd,
            vec![
                (-2048, [0x01, 0xb0]),
                (2046, [0xfd, 0xaf]),
                (-2, [0xfd, 0xbf]),
                (0, [0x01, 0xa0]),
            ],
        ),
        (
            RiscvRvcBranch,
            0xfffd,
            vec![
                (-256, [0x81, 0xf3]),
                (254, [0xfd, 0xef]),
                (-2, [0xfd, 0xff]),
                (0, [0x81, 0xe3]),
            ],
        ),
    ] {
        assert_eq!(kind.size_in_bytes(), 2);
        for (displacement, expected) in cases {
            assert_eq!(
                patch(kind, displacement, initial),
                expected,
                "{kind:?}: {displacement}"
            );
        }
    }
}

#[test]
fn compressed_immediate_bit_permutations() {
    // Independent ISA bit positions catch permutations hidden by all-ones boundaries.
    for (kind, base, positions) in [
        (
            RelocationKind::RiscvRvcJump,
            0x2001u16,
            vec![3, 4, 5, 11, 2, 7, 6, 9, 10, 8],
        ),
        (
            RelocationKind::RiscvRvcBranch,
            0xc281u16,
            vec![3, 4, 10, 11, 2, 5, 6],
        ),
    ] {
        for (index, position) in positions.into_iter().enumerate() {
            assert_eq!(
                patch(kind, 1 << (index + 1), base),
                (base | (1 << position)).to_le_bytes()
            );
        }
    }
}

#[test]
fn compressed_rejects_misalignment_overflow_and_short_buffers_without_writing() {
    for (kind, min, max) in [
        (RelocationKind::RiscvRvcJump, -2048, 2046),
        (RelocationKind::RiscvRvcBranch, -256, 254),
    ] {
        for displacement in [
            min - 2,
            max + 2,
            -1,
            1,
            min + 1,
            max - 1,
            i64::MIN,
            i64::MAX,
        ] {
            let mut bytes = [0xaa; 4];
            let reloc = Relocation::new(1, "target", kind, displacement);
            let error = DefaultRelocationHandler::LITTLE
                .apply(&reloc, 0x4000, 0x4000, displacement, &mut bytes)
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::RelocationOverflow);
            assert_eq!(bytes, [0xaa; 4]);
        }
        for (place, symbol, addend) in [(0, u64::MAX, 1), (u64::MAX, 0, -1)] {
            let mut bytes = [0xaa; 2];
            let reloc = Relocation::new(0, "target", kind, addend);
            assert_eq!(
                DefaultRelocationHandler::LITTLE
                    .apply(&reloc, place, symbol, addend, &mut bytes)
                    .unwrap_err()
                    .code,
                ErrorCode::RelocationOverflow
            );
            assert_eq!(bytes, [0xaa; 2]);
        }
        let reloc = Relocation::new(0, "target", kind, 0);
        assert_eq!(
            DefaultRelocationHandler::LITTLE
                .apply(&reloc, 0, 0, 0, &mut [0])
                .unwrap_err()
                .code,
            ErrorCode::RelocationOverflow
        );
    }
}

// Minimal ELF64 relocatable object: NULL, .text, .rela.text, .symtab, .strtab.
// The shared string table also contains section names; symbol 1 is an absolute target.
fn elf_fixture(types: &[u32]) -> Vec<u8> {
    let text = 64 + 5 * 64;
    let rela = text + 0x88;
    let symtab = rela + types.len() * 24;
    let strtab = symtab + 48;
    let strings = b"\0.text\0.rela.text\0.symtab\0.strtab\0target\0";
    let mut bytes = vec![0; strtab + strings.len()];
    bytes[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    bytes[16..18].copy_from_slice(&ET_REL.to_le_bytes());
    bytes[18..20].copy_from_slice(&EM_RISCV.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&64u64.to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[58..60].copy_from_slice(&64u16.to_le_bytes());
    bytes[60..62].copy_from_slice(&5u16.to_le_bytes());
    bytes[62..64].copy_from_slice(&4u16.to_le_bytes());
    for (index, name, ty, flags, offset, size, link, info, align, entsize) in [
        (
            1,
            1u32,
            SHT_PROGBITS,
            6u64,
            text,
            0x88,
            0u32,
            0u32,
            2u64,
            0u64,
        ),
        (2, 7, SHT_RELA, 0, rela, types.len() * 24, 3, 1, 8, 24),
        (3, 18, SHT_SYMTAB, 0, symtab, 48, 4, 1, 8, 24),
        (4, 26, SHT_STRTAB, 0, strtab, strings.len(), 0, 0, 1, 0),
    ] {
        let h = 64 + index * 64;
        bytes[h..h + 4].copy_from_slice(&name.to_le_bytes());
        bytes[h + 4..h + 8].copy_from_slice(&ty.to_le_bytes());
        bytes[h + 8..h + 16].copy_from_slice(&flags.to_le_bytes());
        bytes[h + 24..h + 32].copy_from_slice(&(offset as u64).to_le_bytes());
        bytes[h + 32..h + 40].copy_from_slice(&(size as u64).to_le_bytes());
        bytes[h + 40..h + 44].copy_from_slice(&link.to_le_bytes());
        bytes[h + 44..h + 48].copy_from_slice(&info.to_le_bytes());
        bytes[h + 48..h + 56].copy_from_slice(&align.to_le_bytes());
        bytes[h + 56..h + 64].copy_from_slice(&entsize.to_le_bytes());
    }
    bytes[text + 0x86..text + 0x88].copy_from_slice(&[0x01, 0xa0]);
    for (i, ty) in types.iter().enumerate() {
        let r = rela + i * 24;
        bytes[r..r + 8].copy_from_slice(&0x86u64.to_le_bytes());
        bytes[r + 8..r + 16].copy_from_slice(&((1u64 << 32) | *ty as u64).to_le_bytes());
        bytes[r + 16..r + 24].copy_from_slice(&(-2i64).to_le_bytes());
    }
    bytes[symtab + 24..symtab + 28].copy_from_slice(&34u32.to_le_bytes());
    bytes[symtab + 28] = 0x10; // STB_GLOBAL
    bytes[symtab + 30..symtab + 32].copy_from_slice(&0xfff1u16.to_le_bytes()); // SHN_ABS
    bytes[strtab..].copy_from_slice(strings);
    bytes
}

#[test]
fn elf_decodes_compressed_relocations_at_section_end_and_skips_only_markers() {
    for (ty, kind) in [
        (44, RelocationKind::RiscvRvcBranch),
        (45, RelocationKind::RiscvRvcJump),
    ] {
        let obj = ElfReader::read(
            &elf_fixture(&[R_RISCV_NONE, ty, R_RISCV_RELAX]),
            Path::new("native_output.o"),
            7,
        )
        .unwrap();
        let section = obj.find_section(".text").unwrap();
        assert_eq!(section.data.len(), 0x88);
        assert_eq!(section.relocations.len(), 1);
        let reloc = &section.relocations[0];
        assert_eq!(reloc.kind, kind);
        assert_eq!(reloc.offset, 0x86);
        assert_eq!(reloc.addend, -2);
        assert_eq!(reloc.symbol_index, Some(1));
        assert_eq!(reloc.file_index, Some(7));
        for arch in [Arch::Riscv32, Arch::Riscv64] {
            assert_eq!(relocation_kind_to_elf_type(arch, kind), ty);
        }
        let mut data = section.data.clone();
        DefaultRelocationHandler::LITTLE
            .apply(reloc, 0x1000, 0x1000, -2, &mut data)
            .unwrap();
    }
    // ALIGN is deliberately unsupported: dropping it without relaxing is incorrect.
    for ty in [43, 255] {
        let err =
            ElfReader::read(&elf_fixture(&[ty]), Path::new("native_output.o"), 0).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
        assert!(err.to_string().contains("LNK008"));
        assert!(err.to_string().contains(&format!("type {ty}")));
    }
}

#[test]
fn pcrel32_decodes_type_57_and_applies_signed_difference() {
    let mut fixture = elf_fixture(&[R_RISCV_32_PCREL]);
    // The fixture parks the field at 0x86 for 2-byte RVC kinds; a 4-byte
    // PC-relative field needs room inside the 0x88-byte section.
    let rela = 64 + 5 * 64 + 0x88;
    fixture[rela..rela + 8].copy_from_slice(&0x78u64.to_le_bytes());
    let decoded = ElfReader::read(&fixture, Path::new("pcrel.o"), 7).unwrap();
    let section = decoded.find_section(".text").unwrap();
    assert_eq!(section.relocations.len(), 1);
    let reloc = &section.relocations[0];
    assert_eq!(reloc.kind, RelocationKind::PcRelative32);
    assert_eq!(reloc.offset, 0x78);
    assert_eq!(reloc.addend, -2);
    assert_eq!(reloc.symbol_index, Some(1));
    for arch in [Arch::Riscv32, Arch::Riscv64] {
        assert_eq!(
            relocation_kind_to_elf_type(arch, RelocationKind::PcRelative32),
            R_RISCV_32_PCREL
        );
    }
    let mut data = section.data.clone();
    data.fill(0x5a);
    DefaultRelocationHandler::LITTLE
        .apply(reloc, 0x2000, 0x3000, -2, &mut data)
        .unwrap();
    assert_eq!(&data[0x78..0x7c], &0xffeu32.to_le_bytes());
    assert!(data[..0x78].iter().all(|&b| b == 0x5a));
    assert!(data[0x7c..].iter().all(|&b| b == 0x5a));
    // A difference outside i32 refuses without touching the field.
    let before = data.clone();
    let err = DefaultRelocationHandler::LITTLE
        .apply(reloc, 0x2000, 0x1_0000_0000, -2, &mut data)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::RelocationOverflow);
    assert_eq!(data, before);
}

#[test]
fn set_fields_overwrite_and_six_bit_forms_preserve_top_bits() {
    use RelocationKind::{RiscvSet6, RiscvSet8, RiscvSet16, RiscvSet32, RiscvSub6};
    for handler in [
        DefaultRelocationHandler::LITTLE,
        DefaultRelocationHandler::BIG,
    ] {
        for (kind, width) in [(RiscvSet8, 1), (RiscvSet16, 2), (RiscvSet32, 4)] {
            assert_eq!(kind.size_in_bytes(), width);
            let encode = |value: u64| -> Vec<u8> {
                if handler.endianness.is_big() {
                    value.to_be_bytes()[8 - width..].to_vec()
                } else {
                    value.to_le_bytes()[..width].to_vec()
                }
            };
            // SET overwrites: the initial field value must not leak into the result.
            for (symbol, addend, expected) in [
                (0x4200u64, -7i64, 0x41f9u64),
                (u64::MAX, 2, 1),
                (0, -2, u64::MAX - 1),
            ] {
                let mut bytes = vec![0x5a; width + 2];
                bytes[1..1 + width].copy_from_slice(&encode(0xdead));
                let reloc = Relocation::new(1, "target", kind, addend);
                handler
                    .apply(&reloc, 0x9000, symbol, addend, &mut bytes)
                    .unwrap();
                assert_eq!(&bytes[1..1 + width], encode(expected));
                assert_eq!(bytes[0], 0x5a);
                assert_eq!(bytes[width + 1], 0x5a);
            }
        }
    }
    // The 6-bit forms live in one byte regardless of endianness and must keep
    // the top two bits, which belong to the surrounding LSDA record.
    for handler in [
        DefaultRelocationHandler::LITTLE,
        DefaultRelocationHandler::BIG,
    ] {
        for (kind, initial, symbol, addend, expected) in [
            (RiscvSet6, 0xd2u8, 0x1ffu64, 0i64, 0xffu8), // low6(0x1ff)=0x3f, top=11
            (RiscvSet6, 0x12, 7, 0, 0x07),               // top bits cleared by input
            (RiscvSet6, 0xc0, 0, -1, 0xff),              // S+A wraps to all ones
            (RiscvSub6, 0xc5, 7, 0, 0xfe),               // (5-7) mod 64 = 62, top=11
            (RiscvSub6, 0x45, 0, -2, 0x47),              // 5-(-2)=7
            (RiscvSub6, 0x80, 65, 0, 0xbf),              // operand truncated to 6 bits
        ] {
            let mut bytes = vec![0x5a, initial, 0x5a];
            let reloc = Relocation::new(1, "target", kind, addend);
            handler
                .apply(&reloc, 0x9000, symbol, addend, &mut bytes)
                .unwrap();
            assert_eq!(bytes, vec![0x5a, expected, 0x5a], "{kind:?}");
        }
    }
}

#[test]
fn set_elf_types_decode_and_roundtrip() {
    for (ty, kind, width) in [
        (R_RISCV_SUB6, RelocationKind::RiscvSub6, 1),
        (R_RISCV_SET6, RelocationKind::RiscvSet6, 1),
        (R_RISCV_SET8, RelocationKind::RiscvSet8, 1),
        (R_RISCV_SET16, RelocationKind::RiscvSet16, 2),
        (R_RISCV_SET32, RelocationKind::RiscvSet32, 4),
    ] {
        assert_eq!(kind.size_in_bytes(), width);
        let mut fixture = elf_fixture(&[ty]);
        let rela = 64 + 5 * 64 + 0x88;
        fixture[rela..rela + 8].copy_from_slice(&0x78u64.to_le_bytes());
        let decoded = ElfReader::read(&fixture, Path::new("set.o"), 7).unwrap();
        let reloc = &decoded.find_section(".text").unwrap().relocations[0];
        assert_eq!(reloc.kind, kind);
        assert_eq!(reloc.offset, 0x78);
        assert_eq!(reloc.addend, -2);
        for arch in [Arch::Riscv32, Arch::Riscv64] {
            assert_eq!(relocation_kind_to_elf_type(arch, kind), ty);
        }
        let mut data = vec![0x5a; 0x88];
        DefaultRelocationHandler::LITTLE
            .apply(reloc, 0x1000, 0x1040, -2, &mut data)
            .unwrap();
        assert!(data[..0x78].iter().all(|&b| b == 0x5a));
        assert!(data[0x78 + width..].iter().all(|&b| b == 0x5a));
    }
}

#[test]
fn universal_adob_keeps_compressed_kinds_and_two_byte_widths() {
    use adesh_object::{AdobObject, AdobRelocation, AdobSection, AdobWriter, TargetDescriptor};
    let triple = "riscv64gc-unknown-linux-gnu";
    let mut obj = AdobObject::new(TargetDescriptor::from_triple(triple).unwrap());
    let mut section = AdobSection::new(".text", adesh_object::SectionKind::Text)
        .with_alignment(2)
        .with_data(vec![0x01, 0xa0, 0x81, 0xe3]);
    section.relocations.push(
        AdobRelocation::new(
            0,
            0,
            "target",
            adesh_object::RelocationKind::RiscV_RvcJump,
            -2,
        )
        .with_width(2),
    );
    section.relocations.push(
        AdobRelocation::new(
            2,
            0,
            "target",
            adesh_object::RelocationKind::RiscV_RvcBranch,
            2,
        )
        .with_width(2),
    );
    obj.add_section(section);
    obj.add_symbol(adesh_object::AdobSymbol::new_defined(
        0,
        "target",
        adesh_object::SymbolKind::Function,
        0,
        0,
        4,
    ));
    let bytes = AdobWriter::write(&obj).unwrap();
    let decoded = AdobV2::decode_universal(
        &bytes,
        Path::new("compressed.adob"),
        &Target::from_triple(triple).unwrap(),
        0,
    )
    .unwrap();
    let relocs = &decoded.sections[0].relocations;
    assert_eq!(relocs.len(), 2);
    assert_eq!(relocs[0].kind, RelocationKind::RiscvRvcJump);
    assert_eq!(relocs[1].kind, RelocationKind::RiscvRvcBranch);
    assert_eq!(relocs[0].kind.size_in_bytes(), 2);
    assert_eq!(relocs[1].kind.size_in_bytes(), 2);
}

#[test]
fn compressed_relocations_have_no_invented_persisted_v3_ids() {
    let target = Target::from_triple("riscv64gc-unknown-linux-gnu").unwrap();
    for kind in [RelocationKind::RiscvRvcJump, RelocationKind::RiscvRvcBranch] {
        let mut obj = ObjectFile::new("compressed.adob".into(), target.clone(), 0);
        let mut section = Section::new_code(".text", vec![0x01, 0xa0], 2);
        section
            .relocations
            .push(Relocation::new(0, "target", kind, 0));
        obj.add_section(section);
        let err = AdobV2::encode(&obj).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedRelocation);
        assert!(err.to_string().contains(kind.name()));
    }
}
