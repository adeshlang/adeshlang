use adesh_linker::metadata::{flags, AdeshMetadata};

#[test]
fn test_metadata_encode_decode_roundtrip() {
    let original = AdeshMetadata {
        abi_version: 1,
        compiler_version: (0, 3, 0),
        runtime_abi_version: 1,
        feature_flags: flags::FEATURE_GC | flags::FEATURE_HARDENED | flags::FEATURE_ASYNC,
        target_triple: "x86_64-linux".to_string(),
        source_hash: [42u8; 32],
    };

    let encoded = original.encode();
    let decoded = AdeshMetadata::decode(&encoded).expect("failed to decode metadata");

    assert_eq!(original.abi_version, decoded.abi_version);
    assert_eq!(original.compiler_version, decoded.compiler_version);
    assert_eq!(original.runtime_abi_version, decoded.runtime_abi_version);
    assert_eq!(original.feature_flags, decoded.feature_flags);
    assert_eq!(original.target_triple, decoded.target_triple);
    assert_eq!(original.source_hash, decoded.source_hash);
}

#[test]
fn test_metadata_compatibility_mismatch() {
    let meta1 = AdeshMetadata {
        abi_version: 1,
        runtime_abi_version: 1,
        ..Default::default()
    };
    let meta2 = AdeshMetadata {
        abi_version: 2,
        runtime_abi_version: 1,
        ..Default::default()
    };

    let res = meta1.validate_compatibility(&meta2, "file1.o", "file2.o");
    assert!(res.is_err());
}
