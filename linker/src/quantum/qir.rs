//! Quantum Intermediate Representation (QIR), OpenQASM 3.0, and QPU circuit binary packaging.

use super::calibration::QpuCalibration;
use super::circuit::QuantumCircuit;
use crate::error::LinkResult;
use crate::section::MergedSection;
use crate::symbol::Symbol;
use std::path::Path;

pub const QIR_MAGIC: [u8; 4] = *b"QIRB"; // Quantum Intermediate Representation Binary

/// QPU Quantum Circuit Package Writer.
pub struct QuantumPackageWriter;

impl QuantumPackageWriter {
    pub fn write_qir_package(
        path: &Path,
        circuit: &QuantumCircuit,
        calibration: Option<&QpuCalibration>,
        extra_sections: &[MergedSection],
    ) -> LinkResult<()> {
        let mut output = Vec::new();

        // 1. Quantum Binary Header (32 bytes)
        output.extend_from_slice(&QIR_MAGIC);
        output.extend_from_slice(&1u32.to_le_bytes()); // QIR ABI version 1
        output.extend_from_slice(&(circuit.num_qubits as u32).to_le_bytes());
        output.extend_from_slice(&(circuit.num_clbits as u32).to_le_bytes());
        output.extend_from_slice(&(circuit.gates.len() as u32).to_le_bytes());
        output.extend_from_slice(&(extra_sections.len() as u32).to_le_bytes());
        output.extend_from_slice(&0u64.to_le_bytes()); // Reserved

        // 2. OpenQASM 3.0 Source Representation
        let qasm_str = circuit.to_openqasm3();
        let qasm_bytes = qasm_str.as_bytes();
        output.extend_from_slice(&(qasm_bytes.len() as u64).to_le_bytes());
        output.extend_from_slice(qasm_bytes);

        // 3. Optional Pulse Calibration Table
        if let Some(calib) = calibration {
            let calib_bytes = calib.encode_binary();
            output.extend_from_slice(&(calib_bytes.len() as u64).to_le_bytes());
            output.extend_from_slice(&calib_bytes);
        } else {
            output.extend_from_slice(&0u64.to_le_bytes());
        }

        // 4. Extra Classical-Quantum Bridge Sections
        for sec in extra_sections {
            let name_bytes = sec.name.as_bytes();
            output.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            output.extend_from_slice(name_bytes);
            output.extend_from_slice(&(sec.data.len() as u64).to_le_bytes());
            output.extend_from_slice(&sec.data);
        }

        std::fs::write(path, output)?;
        Ok(())
    }

    pub fn write_qir_artifact(
        path: &Path,
        merged_sections: &[MergedSection],
        _symbols: &[Symbol],
    ) -> LinkResult<()> {
        let mut output = Vec::new();

        // 1. Quantum Binary Header
        output.extend_from_slice(&QIR_MAGIC);
        output.extend_from_slice(&1u32.to_le_bytes()); // QIR ABI version 1
        output.extend_from_slice(&(merged_sections.len() as u32).to_le_bytes());

        // 2. Quantum Sections (.qir, .qpu_circuits, .qpu_calibration, .qpu_syndromes)
        for sec in merged_sections {
            let name_bytes = sec.name.as_bytes();
            output.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            output.extend_from_slice(name_bytes);
            output.extend_from_slice(&(sec.data.len() as u64).to_le_bytes());
            output.extend_from_slice(&sec.data);
        }

        std::fs::write(path, output)?;
        Ok(())
    }
}
