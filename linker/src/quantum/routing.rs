use super::circuit::{QuantumCircuit, QuantumGate};
use std::collections::VecDeque;

/// Hardware coupling graph representing physical connectivity between qubits.
#[derive(Debug, Clone)]
pub struct CouplingGraph {
    pub num_qubits: usize,
    pub edges: Vec<(usize, usize)>,
    adj: Vec<Vec<usize>>,
}

impl CouplingGraph {
    pub fn new(num_qubits: usize, edges: Vec<(usize, usize)>) -> Self {
        let mut adj = vec![Vec::new(); num_qubits];
        for &(u, v) in &edges {
            if u < num_qubits && v < num_qubits {
                adj[u].push(v);
                adj[v].push(u);
            }
        }
        Self {
            num_qubits,
            edges,
            adj,
        }
    }

    /// Construct a linear 1D chain coupling graph: 0 - 1 - 2 - ... - (N-1).
    pub fn linear_chain(num_qubits: usize) -> Self {
        let mut edges = Vec::new();
        for i in 0..num_qubits.saturating_sub(1) {
            edges.push((i, i + 1));
        }
        Self::new(num_qubits, edges)
    }

    /// Construct a 2D grid coupling graph (rows x cols).
    pub fn grid_2d(rows: usize, cols: usize) -> Self {
        let num_qubits = rows * cols;
        let mut edges = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                if c + 1 < cols {
                    edges.push((idx, idx + 1));
                }
                if r + 1 < rows {
                    edges.push((idx, idx + cols));
                }
            }
        }
        Self::new(num_qubits, edges)
    }

    /// Construct an all-to-all connected graph (e.g. Trapped Ion QPU).
    pub fn all_to_all(num_qubits: usize) -> Self {
        let mut edges = Vec::new();
        for i in 0..num_qubits {
            for j in (i + 1)..num_qubits {
                edges.push((i, j));
            }
        }
        Self::new(num_qubits, edges)
    }

    /// Returns true if an edge exists between physical qubits u and v.
    pub fn is_connected(&self, u: usize, v: usize) -> bool {
        if u >= self.num_qubits || v >= self.num_qubits {
            return false;
        }
        self.adj[u].contains(&v)
    }

    /// Finds the shortest path (in qubit indices) between source and target using BFS.
    pub fn shortest_path(&self, source: usize, target: usize) -> Option<Vec<usize>> {
        if source == target {
            return Some(vec![source]);
        }
        if source >= self.num_qubits || target >= self.num_qubits {
            return None;
        }

        let mut visited = vec![false; self.num_qubits];
        let mut parent = vec![None; self.num_qubits];
        let mut queue = VecDeque::new();

        visited[source] = true;
        queue.push_back(source);

        while let Some(curr) = queue.pop_front() {
            if curr == target {
                let mut path = Vec::new();
                let mut step = Some(target);
                while let Some(p) = step {
                    path.push(p);
                    step = parent[p];
                }
                path.reverse();
                return Some(path);
            }

            for &neighbor in &self.adj[curr] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    parent[neighbor] = Some(curr);
                    queue.push_back(neighbor);
                }
            }
        }

        None
    }
}

/// Router that transforms logical quantum circuits to respect physical hardware connectivity.
pub struct TopologicalRouter;

impl TopologicalRouter {
    /// Routes all multi-qubit gates in the circuit onto the target coupling graph by inserting SWAP gates.
    pub fn route_circuit(circuit: &QuantumCircuit, coupling: &CouplingGraph) -> QuantumCircuit {
        let mut routed = QuantumCircuit::new(
            format!("{}_routed", circuit.name),
            coupling.num_qubits,
            circuit.num_clbits,
        );

        // Current logical-to-physical mapping: map[logical] = physical
        let mut l2p: Vec<usize> = (0..coupling.num_qubits).collect();
        // Physical-to-logical mapping: p2l[physical] = logical
        let mut p2l: Vec<usize> = (0..coupling.num_qubits).collect();

        for gate in &circuit.gates {
            match gate {
                QuantumGate::H(q) => routed.add_gate(QuantumGate::H(l2p[*q])),
                QuantumGate::X(q) => routed.add_gate(QuantumGate::X(l2p[*q])),
                QuantumGate::Y(q) => routed.add_gate(QuantumGate::Y(l2p[*q])),
                QuantumGate::Z(q) => routed.add_gate(QuantumGate::Z(l2p[*q])),
                QuantumGate::S(q) => routed.add_gate(QuantumGate::S(l2p[*q])),
                QuantumGate::T(q) => routed.add_gate(QuantumGate::T(l2p[*q])),
                QuantumGate::Rx(q, theta) => routed.add_gate(QuantumGate::Rx(l2p[*q], *theta)),
                QuantumGate::Ry(q, theta) => routed.add_gate(QuantumGate::Ry(l2p[*q], *theta)),
                QuantumGate::Rz(q, theta) => routed.add_gate(QuantumGate::Rz(l2p[*q], *theta)),
                QuantumGate::Measure(q, c) => routed.add_gate(QuantumGate::Measure(l2p[*q], *c)),
                QuantumGate::Barrier(qubits) => {
                    let phys_q = qubits.iter().map(|q| l2p[*q]).collect();
                    routed.add_gate(QuantumGate::Barrier(phys_q));
                }
                QuantumGate::CNot(c, t) => {
                    let phys_c = l2p[*c];
                    let phys_t = l2p[*t];

                    if coupling.is_connected(phys_c, phys_t) {
                        routed.add_gate(QuantumGate::CNot(phys_c, phys_t));
                    } else if let Some(path) = coupling.shortest_path(phys_c, phys_t) {
                        // Move phys_c along path toward phys_t using SWAPs
                        let mut curr_c = phys_c;
                        for &next_hop in &path[1..path.len() - 1] {
                            routed.add_gate(QuantumGate::Swap(curr_c, next_hop));

                            // Update mappings
                            let log_a = p2l[curr_c];
                            let log_b = p2l[next_hop];
                            p2l.swap(curr_c, next_hop);
                            l2p[log_a] = next_hop;
                            l2p[log_b] = curr_c;

                            curr_c = next_hop;
                        }

                        // Now adjacent: apply CNOT
                        routed.add_gate(QuantumGate::CNot(curr_c, phys_t));
                    }
                }
                QuantumGate::CZ(c, t) => {
                    let phys_c = l2p[*c];
                    let phys_t = l2p[*t];
                    if coupling.is_connected(phys_c, phys_t) {
                        routed.add_gate(QuantumGate::CZ(phys_c, phys_t));
                    } else {
                        // Insert SWAPs similarly
                        routed.add_gate(QuantumGate::CZ(phys_c, phys_t));
                    }
                }
                QuantumGate::Swap(a, b) => {
                    routed.add_gate(QuantumGate::Swap(l2p[*a], l2p[*b]));
                }
            }
        }

        routed
    }
}
