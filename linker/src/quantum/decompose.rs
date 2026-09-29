//! Quantum gate decomposition and basis gate synthesis.
//!
//! Provides:
//! - Arbitrary single-qubit unitary Euler decomposition ($U_3(\theta, \phi, \lambda) \to R_z(\phi) R_y(\theta) R_z(\lambda)$).
//! - Decomposition into universal Clifford+T basis set.
//! - Controlled-gate decomposition into CNOT and single-qubit rotations.

use super::circuit::QuantumGate;
use std::f64::consts::PI;

/// Native physical QPU target basis sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetBasisSet {
    /// IBM / Superconducting basis: { Rz, SX, X, CNOT / ECR }
    SuperconductingIbm,
    /// IonQ / Trapped Ion basis: { Rz, GPI, GPI2, MS (Mølmer-Sørensen) }
    TrappedIon,
    /// Rigetti / Superconducting basis: { Rx, Rz, CZ }
    RigettiCz,
    /// Universal Fault-Tolerant: { H, S, T, CNOT }
    CliffordPlusT,
}

pub struct GateDecomposer;

impl GateDecomposer {
    /// Decomposes an arbitrary Euler rotation $U_3(\theta, \phi, \lambda)$ into $R_z(\phi) R_y(\theta) R_z(\lambda)$.
    pub fn decompose_u3(qubit: usize, theta: f64, phi: f64, lambda: f64) -> Vec<QuantumGate> {
        let mut gates = Vec::new();
        if lambda.abs() > 1e-9 {
            gates.push(QuantumGate::Rz(qubit, lambda));
        }
        if theta.abs() > 1e-9 {
            gates.push(QuantumGate::Ry(qubit, theta));
        }
        if phi.abs() > 1e-9 {
            gates.push(QuantumGate::Rz(qubit, phi));
        }
        gates
    }

    /// Decomposes a Hadamard gate into $R_z(\pi/2) R_x(\pi/2) R_z(\pi/2)$ for hardware supporting $\{R_x, R_z\}$.
    pub fn decompose_h_to_rx_rz(qubit: usize) -> Vec<QuantumGate> {
        vec![
            QuantumGate::Rz(qubit, PI / 2.0),
            QuantumGate::Rx(qubit, PI / 2.0),
            QuantumGate::Rz(qubit, PI / 2.0),
        ]
    }

    /// Decomposes a Controlled-Z (CZ) gate into Hadamard + CNOT + Hadamard.
    pub fn decompose_cz_to_cnot(control: usize, target: usize) -> Vec<QuantumGate> {
        vec![
            QuantumGate::H(target),
            QuantumGate::CNot(control, target),
            QuantumGate::H(target),
        ]
    }

    /// Decomposes a SWAP gate into 3 alternating CNOT gates.
    pub fn decompose_swap_to_cnot(q0: usize, q1: usize) -> Vec<QuantumGate> {
        vec![
            QuantumGate::CNot(q0, q1),
            QuantumGate::CNot(q1, q0),
            QuantumGate::CNot(q0, q1),
        ]
    }

    /// Decomposes a Toffoli (CCNOT) gate into 6 CNOTs and Clifford+T single-qubit gates.
    pub fn decompose_toffoli(c0: usize, c1: usize, target: usize) -> Vec<QuantumGate> {
        vec![
            QuantumGate::H(target),
            QuantumGate::CNot(c1, target),
            QuantumGate::Rz(target, -PI / 4.0), // T^\dagger
            QuantumGate::CNot(c0, target),
            QuantumGate::T(target),
            QuantumGate::CNot(c1, target),
            QuantumGate::Rz(target, -PI / 4.0),
            QuantumGate::CNot(c0, target),
            QuantumGate::T(c1),
            QuantumGate::T(target),
            QuantumGate::H(target),
            QuantumGate::CNot(c0, c1),
            QuantumGate::T(c0),
            QuantumGate::Rz(c1, -PI / 4.0),
            QuantumGate::CNot(c0, c1),
        ]
    }
}
