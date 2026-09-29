//! Hybrid Classical-Quantum runtime orchestration fixups and symbol resolution.

pub const RUNTIME_QUANTUM_DISPATCH: &str = "__adesh_quantum_circuit_dispatch";
pub const RUNTIME_QPU_MEASURE: &str = "__adesh_qpu_measure";
pub const RUNTIME_QPU_BARRIER: &str = "__adesh_qpu_barrier";

/// Standard Quantum Runtime Symbol definitions
pub const QIR_RT_QUBIT_ALLOCATE: &str = "__quantum__rt__qubit_allocate";
pub const QIR_RT_QUBIT_RELEASE: &str = "__quantum__rt__qubit_release";
pub const QIR_QIS_H: &str = "__quantum__qis__h__body";
pub const QIR_QIS_CNOT: &str = "__quantum__qis__cnot__body";
pub const QIR_QIS_MZ: &str = "__quantum__qis__mz__body";

/// Hybrid execution configuration descriptor.
#[derive(Debug, Clone)]
pub struct HybridExecutionConfig {
    pub max_circuit_depth: usize,
    pub shot_count: usize,
    pub dynamic_decoupling_enabled: bool,
    pub measurement_error_mitigation: bool,
}

impl Default for HybridExecutionConfig {
    fn default() -> Self {
        Self {
            max_circuit_depth: 1000,
            shot_count: 1024,
            dynamic_decoupling_enabled: true,
            measurement_error_mitigation: true,
        }
    }
}
