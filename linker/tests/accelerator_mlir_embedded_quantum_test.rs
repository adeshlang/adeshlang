//! Comprehensive test suite for MLIR, GPU, NPU/TPU, Quantum, and Embedded Baremetal toolchain systems.

use adesh_linker::accelerators::gpu::{
    AmdGpuCodeObjectWriter, CudaComputeArch, CudaFatbinWriter, CudaKernelPayload, MetalLibWriter,
    SpirvBinaryWriter,
};
use adesh_linker::accelerators::npu::{
    EthosCommandPacket, EthosNpuWriter, EthosOpcode, TpuBundleWriter, TpuMxuConfig,
};
use adesh_linker::embedded::flash::{FlashBinaryWriter, IntelHexWriter, MotorolaSrecWriter};
use adesh_linker::embedded::ivt::{ArmCortexVectorTable, RiscvTrapVectorTable};
use adesh_linker::embedded::script::LinkerScript;
use adesh_linker::mlir::bytecode::{MlirBytecodeReader, MlirBytecodeWriter, MlirModule};
use adesh_linker::mlir::dialect::{DialectKind, DialectOp, PolyhedralSchedule, TensorShape};
use adesh_linker::quantum::calibration::{PulseRecord, QpuCalibration, WaveformType};
use adesh_linker::quantum::circuit::{QuantumCircuit, QuantumGate};
use adesh_linker::quantum::qir::QuantumPackageWriter;
use adesh_linker::section::MergedSection;
use tempfile::tempdir;

#[test]
fn test_mlir_bytecode_roundtrip() {
    let mut module = MlirModule::new("test_linalg_matmul");

    let mut matmul_op = DialectOp::new(DialectKind::Linalg, "matmul");
    matmul_op.operands = vec!["%A".to_string(), "%B".to_string()];
    matmul_op.results = vec!["%C".to_string()];
    matmul_op.attributes.insert(
        "indexing_maps".to_string(),
        "affine_map<(d0, d1, d2) -> (d0, d2)>".to_string(),
    );
    matmul_op.tensor_shape = Some(TensorShape::new(vec![128, 128], "f32"));
    module.operations.push(matmul_op);

    let mut schedule = PolyhedralSchedule::new(
        vec!["i".to_string(), "j".to_string(), "k".to_string()],
        vec![(0, 128), (0, 128), (0, 128)],
    );
    schedule.tile_sizes = vec![32, 32, 16];
    module
        .schedules
        .insert("matmul_kernel".to_string(), schedule);

    // Encode to bytecode
    let encoded = MlirBytecodeWriter::encode(&module).expect("Failed to encode MLIR bytecode");
    assert_eq!(&encoded[0..4], &[0x4D, 0x4C, 0xEF, 0x52]); // MLïR magic

    // Decode from bytecode
    let decoded = MlirBytecodeReader::decode(&encoded).expect("Failed to decode MLIR bytecode");
    assert_eq!(decoded.name, "test_linalg_matmul");
    assert_eq!(decoded.operations.len(), 1);
    assert_eq!(decoded.operations[0].dialect, DialectKind::Linalg);
    assert_eq!(decoded.operations[0].op_name, "matmul");
    assert_eq!(
        decoded.operations[0].tensor_shape.as_ref().unwrap().dims,
        vec![128, 128]
    );
    assert_eq!(decoded.schedules.len(), 1);
    assert_eq!(
        decoded.schedules["matmul_kernel"].tile_sizes,
        vec![32, 32, 16]
    );
}

#[test]
fn test_gpu_cuda_fatbin_and_spirv_emission() {
    let dir = tempdir().expect("tempdir");
    let fatbin_path = dir.path().join("kernel.fatbin");
    let spirv_path = dir.path().join("shader.spv");
    let hsaco_path = dir.path().join("kernel.hsaco");
    let metallib_path = dir.path().join("library.metallib");

    // 1. CUDA Fatbin
    let kernel = CudaKernelPayload {
        kernel_name: "vector_add".to_string(),
        arch: CudaComputeArch::Sm80,
        ptx_assembly: Some(".version 7.0\n.target sm_80\n.entry vector_add { ret; }".to_string()),
        cubin_binary: Some(vec![0x7f, b'E', b'L', b'F', 0x02, 0x01]),
        shared_memory_bytes: 4096,
        register_count: 32,
    };
    CudaFatbinWriter::write_fatbin(&fatbin_path, &[kernel], &[]).expect("Write fatbin");
    assert!(fatbin_path.exists());
    let fatbin_bytes = std::fs::read(&fatbin_path).unwrap();
    assert_eq!(&fatbin_bytes[0..4], &0xBA55ED50u32.to_le_bytes());

    // 2. Vulkan SPIR-V 1.6
    SpirvBinaryWriter::write_spirv_module(&spirv_path, "main", 0, 0).expect("Write SPIR-V");
    assert!(spirv_path.exists());
    let spv_bytes = std::fs::read(&spirv_path).unwrap();
    assert_eq!(&spv_bytes[0..4], &0x07230203u32.to_le_bytes()); // SPIR-V Magic
    assert_eq!(&spv_bytes[4..8], &0x00010600u32.to_le_bytes()); // SPIR-V 1.6

    // 3. AMD ROCm HSACO
    AmdGpuCodeObjectWriter::write_hsaco(
        &hsaco_path,
        "amdgpu_kernel",
        64,
        32,
        64,
        &[0xbf, 0x81, 0x00, 0x00],
    )
    .expect("Write HSACO");
    assert!(hsaco_path.exists());

    // 4. Apple MetalLib
    MetalLibWriter::write_metallib(
        &metallib_path,
        &["compute_kernel".to_string()],
        b"AIR_BITCODE_PAYLOAD",
    )
    .expect("Write MetalLib");
    assert!(metallib_path.exists());
}

#[test]
fn test_npu_ethos_and_tpu_bundle_emission() {
    let dir = tempdir().expect("tempdir");
    let ethos_path = dir.path().join("ethos.bin");
    let tpu_path = dir.path().join("tpu.bundle");

    // 1. Arm Ethos-U MicroNPU
    let cmd = EthosCommandPacket {
        opcode: EthosOpcode::Conv2D,
        weight_offset: 0,
        weight_size: 1024,
        input_sram_offset: 0x2000_0000,
        output_sram_offset: 0x2000_1000,
        ifm_shape: [1, 28, 28, 1],
        ofm_shape: [1, 28, 28, 32],
    };
    EthosNpuWriter::write_ethos_stream(&ethos_path, &[cmd], &[0xAA; 1024])
        .expect("Write Ethos stream");
    assert!(ethos_path.exists());
    let ethos_bytes = std::fs::read(&ethos_path).unwrap();
    assert_eq!(&ethos_bytes[0..4], b"ETHU");

    // 2. Google TPU V5 Bundle
    let mut weight_sec =
        MergedSection::new(".weights", adesh_linker::section::SectionKind::Data, 0, 128);
    weight_sec.data = vec![0xBB; 256];
    let config = TpuMxuConfig {
        version: 5,
        mxu_tile_dim: 128,
        hbm_size_gb: 32,
        vpu_vector_lanes: 128,
    };
    TpuBundleWriter::write_tpu_executable(&tpu_path, &config, "transformer_layer", &[weight_sec])
        .expect("Write TPU bundle");
    assert!(tpu_path.exists());
    let tpu_bytes = std::fs::read(&tpu_path).unwrap();
    assert_eq!(&tpu_bytes[0..4], b"ADTP");
}

#[test]
fn test_quantum_openqasm3_and_qir_package() {
    let dir = tempdir().expect("tempdir");
    let qir_path = dir.path().join("circuit.qir");

    let mut circuit = QuantumCircuit::new("bell_state_teleport", 3, 3);
    circuit.add_gate(QuantumGate::H(0));
    circuit.add_gate(QuantumGate::CNot(0, 1));
    circuit.add_gate(QuantumGate::Barrier(vec![0, 1]));
    circuit.add_gate(QuantumGate::Measure(0, 0));
    circuit.add_gate(QuantumGate::Measure(1, 1));

    let qasm3 = circuit.to_openqasm3();
    assert!(qasm3.contains("OPENQASM 3.0;"));
    assert!(qasm3.contains("h q[0];"));
    assert!(qasm3.contains("cx q[0], q[1];"));
    assert!(qasm3.contains("c[0] = measure q[0];"));

    // Pulse calibration
    let mut calib = QpuCalibration::new();
    calib.add_pulse(PulseRecord {
        channel_id: 0,
        waveform: WaveformType::DragGaussian,
        frequency_ghz: 5.12,
        amplitude: 0.95,
        duration_ns: 20.0,
        phase_rad: 0.0,
        drag_beta: 0.25,
    });

    QuantumPackageWriter::write_qir_package(&qir_path, &circuit, Some(&calib), &[])
        .expect("Write QIR package");
    assert!(qir_path.exists());
    let qir_bytes = std::fs::read(&qir_path).unwrap();
    assert_eq!(&qir_bytes[0..4], b"QIRB");
}

#[test]
fn test_embedded_baremetal_linker_script_and_firmware_formats() {
    let dir = tempdir().expect("tempdir");
    let hex_path = dir.path().join("firmware.hex");
    let bin_path = dir.path().join("firmware.bin");
    let srec_path = dir.path().join("firmware.srec");

    // 1. Linker script memory allocation
    let mut script =
        LinkerScript::standard_cortex_m(0x0800_0000, 512 * 1024, 0x2000_0000, 128 * 1024);
    let flash = script.memory_regions.get_mut("FLASH").unwrap();
    let text_vma = flash.allocate(1024, 4).expect("Allocate flash");
    assert_eq!(text_vma, 0x0800_0000);

    // 2. Vector table synthesis
    let ivt = ArmCortexVectorTable::new(0x2002_0000, 0x0800_0004);
    let ivt_bytes = ivt.encode();
    assert_eq!(ivt_bytes.len(), 64);
    assert_eq!(&ivt_bytes[0..4], &0x2002_0000u32.to_le_bytes()); // Initial SP
    assert_eq!(&ivt_bytes[4..8], &0x0800_0005u32.to_le_bytes()); // Thumb Reset Handler

    // 3. RISC-V Trap Vector
    let riscv_ivt = RiscvTrapVectorTable::direct(0x8000_0000);
    assert_eq!(riscv_ivt.encode().len(), 4);

    // 4. Intel HEX emission
    let test_data = [0x11, 0x22, 0x33, 0x44];
    let sections: &[(u64, &[u8])] = &[(0x0800_0000, &ivt_bytes), (0x0800_0040, &test_data)];
    IntelHexWriter::write_hex(&hex_path, sections).expect("Write HEX");
    assert!(hex_path.exists());
    let hex_content = std::fs::read_to_string(&hex_path).unwrap();
    assert!(hex_content.contains(":020000040800F2")); // Extended linear address 0x0800
    assert!(hex_content.ends_with(":00000001FF\n"));

    // 5. Binary & SREC emission
    FlashBinaryWriter::write_bin(&bin_path, 0x0800_0000, sections).expect("Write BIN");
    assert!(bin_path.exists());
    assert_eq!(std::fs::read(&bin_path).unwrap().len(), 68);

    MotorolaSrecWriter::write_srec(&srec_path, sections).expect("Write SREC");
    assert!(srec_path.exists());
    let srec_content = std::fs::read_to_string(&srec_path).unwrap();
    assert!(srec_content.starts_with("S0"));
    assert!(srec_content.ends_with("S70500000000FA\n"));
}
