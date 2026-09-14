use crate::utils::collections::FastMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrioritySchedulingAlgorithm {
    WeightedFair,
    StrictPriority,
    Fifo,
    Adaptive,
}

#[derive(Debug, Clone)]
pub struct PriorityNode {
    pub stream_id: u32,
    pub parent_stream_id: u32,
    pub weight: u8,
    pub exclusive: bool,
    pub urgency: u8,
    pub incremental: bool,
}

impl PriorityNode {
    pub fn new(stream_id: u32, weight: u8) -> Self {
        Self {
            stream_id,
            parent_stream_id: 0,
            weight,
            exclusive: false,
            urgency: 3,
            incremental: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PriorityTree {
    pub nodes: FastMap<u32, PriorityNode>,
    pub algorithm: Option<PrioritySchedulingAlgorithm>,
}

impl PriorityTree {
    pub fn new() -> Self {
        Self {
            nodes: FastMap::default(),
            algorithm: Some(PrioritySchedulingAlgorithm::WeightedFair),
        }
    }

    pub fn insert(&mut self, node: PriorityNode) {
        self.nodes.insert(node.stream_id, node);
    }

    pub fn get(&self, stream_id: u32) -> Option<&PriorityNode> {
        self.nodes.get(&stream_id)
    }
}
