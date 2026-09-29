//! Qubit register allocation, quantum gate maps, OpenQASM 3.0 and circuit scheduling.

use std::collections::HashMap;

/// Standard quantum gate types supported in the Adesh quantum compiler.
#[derive(Debug, Clone, PartialEq)]
pub enum QuantumGate {
    H(usize),              // Hadamard on qubit
    X(usize),              // Pauli-X (NOT)
    Y(usize),              // Pauli-Y
    Z(usize),              // Pauli-Z
    S(usize),              // Phase (Z^(1/2))
    T(usize),              // π/8 (Z^(1/4))
    Rx(usize, f64),        // Rotation-X by angle theta
    Ry(usize, f64),        // Rotation-Y by angle theta
    Rz(usize, f64),        // Rotation-Z by angle theta
    CNot(usize, usize),    // Controlled-NOT (control, target)
    CZ(usize, usize),      // Controlled-Z (control, target)
    Swap(usize, usize),    // SWAP gate
    Measure(usize, usize), // Measure qubit into classical bit register
    Barrier(Vec<usize>),   // Execution synchronization barrier
}

/// Quantum topology architecture graph for physical qubit routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QubitTopology {
    Linear(usize),
    Grid2D { rows: usize, cols: usize },
    HeavyHex { distance: usize },
    AllToAll(usize),
}

/// Qubit register descriptor for link-time physical-to-virtual qubit layout.
#[derive(Debug, Clone)]
pub struct QubitRegister {
    pub name: String,
    pub num_qubits: usize,
    pub physical_qubit_ids: Vec<usize>,
}

/// Syndrome measurement readout table for quantum error correction.
#[derive(Debug, Clone, Default)]
pub struct SyndromeTable {
    pub num_syndromes: usize,
    pub classical_feedback_target_symbols: Vec<String>,
}

/// Quantum Circuit instruction stream and qubit allocation manifest.
#[derive(Debug, Clone, Default)]
pub struct QuantumCircuit {
    pub name: String,
    pub num_qubits: usize,
    pub num_clbits: usize,
    pub gates: Vec<QuantumGate>,
    pub metadata: HashMap<String, String>,
}

impl QuantumCircuit {
    pub fn new(name: impl Into<String>, num_qubits: usize, num_clbits: usize) -> Self {
        Self {
            name: name.into(),
            num_qubits,
            num_clbits,
            gates: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn add_gate(&mut self, gate: QuantumGate) {
        self.gates.push(gate);
    }

    /// Serialize the circuit to standard OpenQASM 3.0 format.
    pub fn to_openqasm3(&self) -> String {
        let mut qasm = String::new();
        qasm.push_str("OPENQASM 3.0;\n");
        qasm.push_str("include \"stdgates.inc\";\n\n");
        qasm.push_str(&format!("qubit[{}] q;\n", self.num_qubits));
        qasm.push_str(&format!("bit[{}] c;\n\n", self.num_clbits));

        for gate in &self.gates {
            match gate {
                QuantumGate::H(q) => qasm.push_str(&format!("h q[{}];\n", q)),
                QuantumGate::X(q) => qasm.push_str(&format!("x q[{}];\n", q)),
                QuantumGate::Y(q) => qasm.push_str(&format!("y q[{}];\n", q)),
                QuantumGate::Z(q) => qasm.push_str(&format!("z q[{}];\n", q)),
                QuantumGate::S(q) => qasm.push_str(&format!("s q[{}];\n", q)),
                QuantumGate::T(q) => qasm.push_str(&format!("t q[{}];\n", q)),
                QuantumGate::Rx(q, theta) => qasm.push_str(&format!("rx({}) q[{}];\n", theta, q)),
                QuantumGate::Ry(q, theta) => qasm.push_str(&format!("ry({}) q[{}];\n", theta, q)),
                QuantumGate::Rz(q, theta) => qasm.push_str(&format!("rz({}) q[{}];\n", theta, q)),
                QuantumGate::CNot(c, t) => qasm.push_str(&format!("cx q[{}], q[{}];\n", c, t)),
                QuantumGate::CZ(c, t) => qasm.push_str(&format!("cz q[{}], q[{}];\n", c, t)),
                QuantumGate::Swap(a, b) => qasm.push_str(&format!("swap q[{}], q[{}];\n", a, b)),
                QuantumGate::Measure(q, c) => {
                    qasm.push_str(&format!("c[{}] = measure q[{}];\n", c, q))
                }
                QuantumGate::Barrier(qubits) => {
                    let q_str = qubits
                        .iter()
                        .map(|q| format!("q[{}]", q))
                        .collect::<Vec<_>>()
                        .join(", ");
                    qasm.push_str(&format!("barrier {};\n", q_str));
                }
            }
        }
        qasm
    }
}
