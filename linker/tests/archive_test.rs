use adesh_linker::archive::Archive;
use adesh_linker::object::{ObjectFile, ObjectWriter};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::Path;

#[test]
fn test_archive_creation_and_parsing() {
    let mut archive = Archive::new();
    let obj1_data = vec![0x7f, b'E', b'L', b'F', 1, 2, 3, 4];
    let obj2_data = vec![0x4d, 0x5a, 0x90, 0x00, 5, 6, 7, 8, 9]; // PE MZ

    archive.add_file("short_name.o", obj1_data.clone());
    archive.add_file(
        "very_long_member_object_file_name_exceeding_15_chars.o",
        obj2_data.clone(),
    );

    let encoded = archive.encode_gnu();
    assert!(encoded.starts_with(b"!<arch>\n"));

    // Parse back
    let parsed = Archive::parse(&encoded, Path::new("libtest.a"))
        .expect("Failed to parse generated archive");
    assert_eq!(parsed.members.len(), 2);
    assert_eq!(parsed.members[0].name, "short_name.o");
    assert_eq!(parsed.members[0].data, obj1_data);
    assert_eq!(
        parsed.members[1].name,
        "very_long_member_object_file_name_exceeding_15_chars.o"
    );
    assert_eq!(parsed.members[1].data, obj2_data);
}

#[test]
fn test_archive_odd_size_alignment_padding() {
    let mut archive = Archive::new();
    // 3 bytes data (odd size)
    archive.add_file("odd.o", vec![1, 2, 3]);
    let encoded = archive.encode_gnu();

    // Total length must be even (excluding magic)
    let parsed = Archive::parse(&encoded, Path::new("odd.a")).expect("Failed to parse odd archive");
    assert_eq!(parsed.members.len(), 1);
    assert_eq!(parsed.members[0].data, vec![1, 2, 3]);
}

#[test]
fn test_archive_index_retains_all_duplicate_definitions() {
    let target = Target::x86_64_linux();
    let mut archive = Archive::new();
    for i in 0..2 {
        let mut obj = ObjectFile::new(format!("member{i}.o").into(), target.clone(), i);
        obj.add_section(Section::new_code(".text", vec![0xC3], 1));
        obj.add_symbol(Symbol::new_defined(
            "duplicate",
            SymbolBinding::Global,
            SymbolType::Function,
            0,
            0,
            1,
            i,
        ));
        archive.add_file(format!("member{i}.o"), ObjectWriter::encode(&obj).unwrap());
    }
    let parsed = Archive::parse(&archive.encode_gnu(), Path::new("duplicates.a")).unwrap();
    assert_eq!(parsed.symbol_index.get("duplicate"), Some(&vec![0, 1]));
}

#[test]
fn test_archive_linker_index_is_lazy_and_resolvable() {
    fn append_member(out: &mut Vec<u8>, name: &str, data: &[u8]) -> u32 {
        let member_offset = out.len() as u32;
        let mut header = [b' '; 60];
        header[..name.len()].copy_from_slice(name.as_bytes());
        let size = data.len().to_string();
        header[48..48 + size.len()].copy_from_slice(size.as_bytes());
        header[58..60].copy_from_slice(b"`\n");
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            out.push(b'\n');
        }
        member_offset
    }

    let target = Target::host();
    let mut obj = ObjectFile::new("lazy.o".into(), target, 0);
    obj.add_section(Section::new_code(".text", vec![0xC3], 1));
    obj.add_symbol(Symbol::new_defined(
        "lazy_symbol",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        1,
        0,
    ));
    let object_bytes = ObjectWriter::encode(&obj).unwrap();

    let index_size = 4 + 4 + "lazy_symbol".len() + 1;
    let object_offset = 8 + 60 + index_size as u32;
    let mut index_data = Vec::with_capacity(index_size);
    index_data.extend_from_slice(&1u32.to_be_bytes());
    index_data.extend_from_slice(&object_offset.to_be_bytes());
    index_data.extend_from_slice(b"lazy_symbol\0");

    let mut bytes = b"!<arch>\n".to_vec();
    append_member(&mut bytes, "/", &index_data);
    append_member(&mut bytes, "lazy.o/", &object_bytes);

    let parsed = Archive::parse(&bytes, Path::new("lazy.a")).unwrap();
    assert_eq!(parsed.symbol_index.get("lazy_symbol"), Some(&vec![0]));
    assert!(
        parsed.members[0].obj.is_none(),
        "indexed members should remain unparsed until extracted"
    );
}
