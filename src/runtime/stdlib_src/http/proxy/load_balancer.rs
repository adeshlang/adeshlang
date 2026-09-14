use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct UpstreamTarget {
    pub url: String,
    pub weight: usize,
    pub healthy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalanceAlgorithm {
    RoundRobin,
    Random,
    ConsistentHash,
}

#[derive(Clone)]
pub struct LoadBalancer {
    pub targets: Vec<UpstreamTarget>,
    pub algorithm: LoadBalanceAlgorithm,
    counter: Arc<AtomicUsize>,
}

impl LoadBalancer {
    pub fn new(targets: Vec<UpstreamTarget>, algorithm: LoadBalanceAlgorithm) -> Self {
        Self {
            targets,
            algorithm,
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn next_target(&self) -> Option<&UpstreamTarget> {
        let healthy_targets: Vec<&UpstreamTarget> =
            self.targets.iter().filter(|t| t.healthy).collect();
        if healthy_targets.is_empty() {
            return None;
        }

        match self.algorithm {
            LoadBalanceAlgorithm::RoundRobin => {
                let idx = self.counter.fetch_add(1, Ordering::SeqCst) % healthy_targets.len();
                Some(healthy_targets[idx])
            }
            LoadBalanceAlgorithm::Random => {
                let idx = rand::random::<usize>() % healthy_targets.len();
                Some(healthy_targets[idx])
            }
            LoadBalanceAlgorithm::ConsistentHash => Some(healthy_targets[0]),
        }
    }
}
