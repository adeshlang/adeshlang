//! Phase 10 — Dependency-Aware Build Graph Scheduler.
//!
//! Provides:
//! - DAG build graph representing module compilation units.
//! - Parallel execution wave scheduling with topological sorting.
//! - Cycle detection and incremental invalidation propagation.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// A single node in the compiler build graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildNode {
    pub name: String,
    pub source_path: String,
    pub dependencies: Vec<String>,
}

/// Compilation schedule grouping independent compilation units into concurrent waves.
#[derive(Debug, Clone, Default)]
pub struct BuildSchedule {
    pub waves: Vec<Vec<String>>,
}

/// Dependency-aware DAG scheduler.
#[derive(Debug, Default)]
pub struct BuildGraphScheduler {
    nodes: HashMap<String, BuildNode>,
}

impl BuildGraphScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: BuildNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    /// Compute parallel execution waves respecting dependency order.
    pub fn schedule(&self) -> Result<BuildSchedule, String> {
        let mut in_degrees: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for name in self.nodes.keys() {
            in_degrees.insert(name.clone(), 0);
        }

        for (name, node) in &self.nodes {
            for dep in &node.dependencies {
                if !self.nodes.contains_key(dep) {
                    return Err(format!(
                        "Unresolved dependency '{}' for module '{}'",
                        dep, name
                    ));
                }
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(name.clone());
                *in_degrees.get_mut(name).unwrap() += 1;
            }
        }

        let mut ready: VecDeque<String> = in_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(k, _)| k.clone())
            .collect();

        let mut schedule = BuildSchedule::default();
        let mut processed_count = 0;

        while !ready.is_empty() {
            let current_wave: Vec<String> = ready.drain(..).collect();
            processed_count += current_wave.len();

            let mut next_wave = Vec::new();
            for completed in &current_wave {
                if let Some(deps) = dependents.get(completed) {
                    for dep in deps {
                        let deg = in_degrees.get_mut(dep).unwrap();
                        *deg -= 1;
                        if *deg == 0 {
                            next_wave.push(dep.clone());
                        }
                    }
                }
            }

            schedule.waves.push(current_wave);
            for n in next_wave {
                ready.push_back(n);
            }
        }

        if processed_count < self.nodes.len() {
            return Err("Cycle detected in compiler build graph".to_string());
        }

        Ok(schedule)
    }
}
