//! In-depth tests for Quantum Gate Decomposition, Topological Routing, and State-Vector Simulation.

use adesh_linker::quantum::circuit::{QuantumCircuit, QuantumGate};
use adesh_linker::quantum::decompose::GateDecomposer;
use adesh_linker::quantum::routing::{CouplingGraph, TopologicalRouter};
use adesh_linker::quantum::sim::StateVectorSimulator;
use std::f64::consts::PI;

#[test]
fn test_state_vector_superposition_hadamard() {
    let mut sim = StateVectorSimulator::new(1);
    let mut circuit = QuantumCircuit::new("hadamard", 1, 1);
    circuit.add_gate(QuantumGate::H(0));

    sim.run_circuit(&circuit);
    let probs = sim.probabilities();
    assert_eq!(probs.len(), 2);
    assert!((probs[0] - 0.5).abs() < 1e-6);
    assert!((probs[1] - 0.5).abs() < 1e-6);
}

#[test]
fn test_state_vector_bell_state_entanglement() {
    let mut sim = StateVectorSimulator::new(2);
    let mut circuit = QuantumCircuit::new("bell_state", 2, 2);
    // Prepare (|00> + |11>) / sqrt(2)
    circuit.add_gate(QuantumGate::H(0));
    circuit.add_gate(QuantumGate::CNot(0, 1));

    sim.run_circuit(&circuit);
    let probs = sim.probabilities();
    assert_eq!(probs.len(), 4);
    assert!((probs[0] - 0.5).abs() < 1e-6); // |00>
    assert!(probs[1].abs() < 1e-6);        // |01>
    assert!(probs[2].abs() < 1e-6);        // |10>
    assert!((probs[3] - 0.5).abs() < 1e-6); // |11>
}

#[test]
fn test_state_vector_ghz_state_3qubits() {
    let mut sim = StateVectorSimulator::new(3);
    let mut circuit = QuantumCircuit::new("ghz_state", 3, 3);
    // Prepare (|000> + |111>) / sqrt(2)
    circuit.add_gate(QuantumGate::H(0));
    circuit.add_gate(QuantumGate::CNot(0, 1));
    circuit.add_gate(QuantumGate::CNot(1, 2));

    sim.run_circuit(&circuit);
    let probs = sim.probabilities();
    assert_eq!(probs.len(), 8);
    assert!((probs[0] - 0.5).abs() < 1e-6); // |000>
    assert!((probs[7] - 0.5).abs() < 1e-6); // |111>
    for i in 1..7 {
        assert!(probs[i].abs() < 1e-6);
    }
}

#[test]
fn test_state_vector_rotations_and_pauli() {
    let mut sim = StateVectorSimulator::new(1);
    let mut circuit = QuantumCircuit::new("rotations", 1, 1);
    // Rx(pi) is equivalent to X gate up to global phase
    circuit.add_gate(QuantumGate::Rx(0, PI));

    sim.run_circuit(&circuit);
    let probs = sim.probabilities();
    assert!(probs[0].abs() < 1e-6); // |0> = 0.0
    assert!((probs[1] - 1.0).abs() < 1e-6); // |1> = 1.0
}

#[test]
fn test_gate_decompositions() {
    // 1. CZ decomposition
    let cz_decomp = GateDecomposer::decompose_cz_to_cnot(0, 1);
    assert_eq!(cz_decomp.len(), 3);
    assert_eq!(cz_decomp[0], QuantumGate::H(1));
    assert_eq!(cz_decomp[1], QuantumGate::CNot(0, 1));
    assert_eq!(cz_decomp[2], QuantumGate::H(1));

    // 2. SWAP decomposition
    let swap_decomp = GateDecomposer::decompose_swap_to_cnot(0, 1);
    assert_eq!(swap_decomp.len(), 3);

    // 3. Toffoli decomposition
    let toffoli_decomp = GateDecomposer::decompose_toffoli(0, 1, 2);
    assert_eq!(toffoli_decomp.len(), 15);
}

#[test]
fn test_topological_qubit_routing_on_linear_chain() {
    // Linear chain: 0 - 1 - 2
    let coupling = CouplingGraph::linear_chain(3);
    assert!(coupling.is_connected(0, 1));
    assert!(coupling.is_connected(1, 2));
    assert!(!coupling.is_connected(0, 2));

    let path = coupling.shortest_path(0, 2).expect("Shortest path");
    assert_eq!(path, vec![0, 1, 2]);

    // Circuit wanting CNOT between distant qubits 0 and 2
    let mut circuit = QuantumCircuit::new("non_adjacent_cnot", 3, 2);
    circuit.add_gate(QuantumGate::CNot(0, 2));

    // Route circuit -> should insert SWAP(0, 1) then CNOT(1, 2)
    let routed = TopologicalRouter::route_circuit(&circuit, &coupling);
    assert_eq!(routed.gates.len(), 2);
    assert_eq!(routed.gates[0], QuantumGate::Swap(0, 1));
    assert_eq!(routed.gates[1], QuantumGate::CNot(1, 2));
}
