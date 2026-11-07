use adesh_codegen::targets::create_backend;
use adesh_object::target::{Endianness, TargetDescriptor};
use adeshlang::backends::unsafe_heap;
use adeshlang::runtime::stdlib_src::encoding::binary;

#[test]
fn test_heap_pack_unpack_typed_endianness() {
    // 1. Test Little-Endian heap packing/unpacking
    unsafe_heap::set_target_endianness(false);
    assert!(!unsafe_heap::is_target_big_endian());

    // 1 byte
    let p1 = unsafe_heap::pack_typed_val(0x42, 1);
    assert_eq!(p1, vec![0x42]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p1), 0x42);

    // 2 bytes
    let p2_le = unsafe_heap::pack_typed_val(0x1234, 2);
    assert_eq!(p2_le, vec![0x34, 0x12]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p2_le), 0x1234);

    // 4 bytes
    let p4_le = unsafe_heap::pack_typed_val(0x12345678, 4);
    assert_eq!(p4_le, vec![0x78, 0x56, 0x34, 0x12]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p4_le), 0x12345678);

    // 8 bytes
    let val64: i64 = 0x0102030405060708;
    let p8_le = unsafe_heap::pack_typed_val(val64, 8);
    assert_eq!(p8_le, vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p8_le), val64);

    // 2. Test Big-Endian heap packing/unpacking
    unsafe_heap::set_target_endianness(true);
    assert!(unsafe_heap::is_target_big_endian());

    // 1 byte
    let p1_be = unsafe_heap::pack_typed_val(0x42, 1);
    assert_eq!(p1_be, vec![0x42]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p1_be), 0x42);

    // 2 bytes
    let p2_be = unsafe_heap::pack_typed_val(0x1234, 2);
    assert_eq!(p2_be, vec![0x12, 0x34]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p2_be), 0x1234);

    // 4 bytes
    let p4_be = unsafe_heap::pack_typed_val(0x12345678, 4);
    assert_eq!(p4_be, vec![0x12, 0x34, 0x56, 0x78]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p4_be), 0x12345678);

    // 8 bytes
    let p8_be = unsafe_heap::pack_typed_val(val64, 8);
    assert_eq!(p8_be, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    assert_eq!(unsafe_heap::unpack_typed_bytes(&p8_be), val64);

    // Reset back to Little-Endian for other tests
    unsafe_heap::set_target_endianness(false);
}

#[test]
fn test_encoding_binary_128bit() {
    let u128_val: u128 = 0xA1B2C3D4E5F60718293A4B5C6D7E8F90;
    let i128_val: i128 = -0x123456789ABCDEF0123456789ABCDEF;

    // LE roundtrips
    let le_bytes = binary::i128_to_bytes_le(i128_val);
    assert_eq!(le_bytes, i128_val.to_le_bytes());
    assert_eq!(binary::bytes_to_i128_le(&le_bytes).unwrap(), i128_val);

    // BE roundtrips
    let be_bytes = binary::i128_to_bytes_be(i128_val);
    assert_eq!(be_bytes, i128_val.to_be_bytes());
    assert_eq!(binary::bytes_to_i128_be(&be_bytes).unwrap(), i128_val);

    // Slice reading
    let u128_le_buf = u128_val.to_le_bytes().to_vec();
    assert_eq!(binary::read_u128_le(&u128_le_buf, 0).unwrap(), u128_val);

    let u128_be_buf = u128_val.to_be_bytes().to_vec();
    assert_eq!(binary::read_u128_be(&u128_be_buf, 0).unwrap(), u128_val);

    let i128_le_buf = i128_val.to_le_bytes().to_vec();
    assert_eq!(binary::read_i128_le(&i128_le_buf, 0).unwrap(), i128_val);

    let i128_be_buf = i128_val.to_be_bytes().to_vec();
    assert_eq!(binary::read_i128_be(&i128_be_buf, 0).unwrap(), i128_val);

    // Out of bounds checks
    assert!(binary::read_u128_le(&u128_le_buf, 1).is_err());
    assert!(binary::read_i128_be(&i128_be_buf, 1).is_err());
}

#[test]
fn test_codegen_rejects_unsupported_big_endian_backends() {
    // 1. x86_64 with Big Endian
    let mut x86_target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    x86_target.endianness = Endianness::Big;
    match create_backend(x86_target) {
        Err(err) => {
            assert!(
                err.reason
                    .contains("x86-64 architecture does not support big-endian byte order")
            );
        }
        Ok(_) => panic!("Expected x86_64 big-endian backend creation to fail"),
    }

    // 2. WASM with Big Endian
    let mut wasm_target = TargetDescriptor::from_triple("wasm32-unknown-unknown").unwrap();
    wasm_target.endianness = Endianness::Big;
    match create_backend(wasm_target) {
        Err(err) => {
            assert!(
                err.reason
                    .contains("WebAssembly specification mandates little-endian memory semantics")
            );
        }
        Ok(_) => panic!("Expected wasm32 big-endian backend creation to fail"),
    }
}

#[test]
fn test_aarch64_big_endian_codegen_support() {
    let mut arm_target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap();
    arm_target.endianness = Endianness::Big;
    let mut backend =
        create_backend(arm_target).expect("aarch64 big-endian backend creation succeeds");
    assert_eq!(backend.target().endianness, Endianness::Big);

    let mut module = adesh_codegen::machine_ir::NativeModule::new("test_mod");
    let mut func = adesh_codegen::machine_ir::MachineFunction::new("main");
    func.blocks[0].push(adesh_codegen::machine_ir::MachineInstruction::Return);
    module.add_function(func);

    let obj = backend
        .emit_object(&module)
        .expect("emits AArch64 BE object");
    assert_eq!(obj.target.endianness, Endianness::Big);
    assert_eq!(
        obj.target.architecture,
        adesh_object::target::Architecture::AArch64
    );
    assert!(!obj.sections.is_empty());
}

#[test]
fn test_riscv64_big_endian_codegen_support() {
    let mut riscv_target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").unwrap();
    riscv_target.endianness = Endianness::Big;
    let mut backend =
        create_backend(riscv_target).expect("riscv64 big-endian backend creation succeeds");
    assert_eq!(backend.target().endianness, Endianness::Big);

    let mut module = adesh_codegen::machine_ir::NativeModule::new("test_riscv_mod");
    let mut func = adesh_codegen::machine_ir::MachineFunction::new("main");
    func.blocks[0].push(adesh_codegen::machine_ir::MachineInstruction::Return);
    module.add_function(func);

    let obj = backend
        .emit_object(&module)
        .expect("emits RISC-V 64 BE object");
    assert_eq!(obj.target.endianness, Endianness::Big);
    assert_eq!(
        obj.target.architecture,
        adesh_object::target::Architecture::RiscV64
    );
    assert!(!obj.sections.is_empty());
}
