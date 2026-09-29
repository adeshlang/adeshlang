use adesh_linker::archive::Archive;
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
