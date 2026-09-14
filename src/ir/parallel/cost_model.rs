//! Parallel execution cost model

/// Parallel execution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelStrategy {
    Sequential,
    StaticChunks,
    WorkStealing,
    NestedFlatten,
}

pub struct ParallelCostModel {
    pub min_parallel_iterations: u64,
    pub min_chunk_size: u64,
    pub thread_spawn_cost: f64,
    pub steal_cost: f64,
}

impl Default for ParallelCostModel {
    fn default() -> Self {
        ParallelCostModel {
            min_parallel_iterations: 256,
            min_chunk_size: 64,
            thread_spawn_cost: 5000.0,
            steal_cost: 100.0,
        }
    }
}

impl ParallelCostModel {
    pub fn select_strategy(
        &self,
        iterations: u64,
        op_cost: f64,
        num_workers: usize,
        is_nested: bool,
    ) -> ParallelStrategy {
        if iterations < self.min_parallel_iterations {
            return ParallelStrategy::Sequential;
        }

        if is_nested {
            return ParallelStrategy::NestedFlatten;
        }

        let work = iterations as f64 * op_cost;
        let chunk_count = (iterations / self.min_chunk_size).max(1);
        let static_cost = self.thread_spawn_cost + work / num_workers.max(1) as f64;
        let steal_cost = self.thread_spawn_cost
            + work / num_workers.max(1) as f64
            + chunk_count as f64 * self.steal_cost;

        if static_cost <= steal_cost && chunk_count <= num_workers as u64 * 2 {
            ParallelStrategy::StaticChunks
        } else {
            ParallelStrategy::WorkStealing
        }
    }

    pub fn optimal_chunk_size(&self, iterations: u64, num_workers: usize) -> u64 {
        let workers = num_workers.max(1) as u64;
        let base = iterations / workers;
        base.max(self.min_chunk_size)
    }
}
