//! ELF Note sections (e.g. `.note.gnu.build-id`).

pub const NT_GNU_BUILD_ID: u32 = 3;

/// Construct a standard `.note.gnu.build-id` section payload.
pub fn create_gnu_build_id_note(build_id: &[u8]) -> Vec<u8> {
    let name = b"GNU\0";
    let namesz = name.len() as u32;
    let descsz = build_id.len() as u32;

    let mut note = Vec::with_capacity(16 + 4 + build_id.len());
    note.extend_from_slice(&namesz.to_le_bytes());
    note.extend_from_slice(&descsz.to_le_bytes());
    note.extend_from_slice(&NT_GNU_BUILD_ID.to_le_bytes());
    note.extend_from_slice(name); // 4 bytes ("GNU\0")
    note.extend_from_slice(build_id);

    // 4-byte align desc
    while (note.len() & 3) != 0 {
        note.push(0);
    }

    note
}
