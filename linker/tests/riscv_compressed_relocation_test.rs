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
