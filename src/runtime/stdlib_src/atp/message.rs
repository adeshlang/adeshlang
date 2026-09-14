//! ATP Message and Fragmentation

use super::config::MAX_FRAGMENT_COUNT;
use super::errors::{AtpError, AtpResult};
use super::memory::MemoryBudget;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliabilityMode {
    Unreliable,
    Reliable,
    ReliableUnordered,
    ReliableOrdered,
    OrderedBestEffort,
}

impl ReliabilityMode {
    pub fn is_reliable(&self) -> bool {
        matches!(
            self,
            ReliabilityMode::Reliable
                | ReliabilityMode::ReliableUnordered
                | ReliabilityMode::ReliableOrdered
        )
    }

    pub fn is_ordered(&self) -> bool {
        matches!(
            self,
            ReliabilityMode::ReliableOrdered | ReliabilityMode::OrderedBestEffort
        )
    }

    pub fn from_flags(flags: u8) -> Self {
        let reliable = flags & super::wire::DATA_FLAG_RELIABLE != 0;
        let ordered = flags & super::wire::DATA_FLAG_ORDERED != 0;
        match (reliable, ordered) {
            (true, true) => ReliabilityMode::ReliableOrdered,
            (true, false) => ReliabilityMode::ReliableUnordered,
            (false, true) => ReliabilityMode::OrderedBestEffort,
            (false, false) => ReliabilityMode::Unreliable,
        }
    }

    pub fn to_flags(&self) -> u8 {
        let mut flags = 0u8;
        if self.is_reliable() {
            flags |= super::wire::DATA_FLAG_RELIABLE;
        }
        if self.is_ordered() {
            flags |= super::wire::DATA_FLAG_ORDERED;
        }
        flags
    }
}

#[derive(Debug, Clone)]
pub struct Fragment {
    pub message_id: u64,
    pub fragment_num: u64,
    pub total_fragments: u64,
    pub payload: Vec<u8>,
    pub flags: u8,
}

#[derive(Debug, Clone)]
pub struct SendMessage {
    pub message_id: u64,
    pub stream_id: u64,
    pub total_fragments: u64,
    pub reliability: ReliabilityMode,
    pub acked_fragments: BTreeMap<u64, bool>,
    pub total_size: usize,
    pub complete: bool,
    pub cancelled: bool,
    /// Stored fragment payloads for retransmission.
    pub fragments: BTreeMap<u64, Vec<u8>>,
    pub flags: u8,
}

impl SendMessage {
    pub fn new(
        message_id: u64,
        stream_id: u64,
        total_fragments: u64,
        reliability: ReliabilityMode,
        total_size: usize,
    ) -> Self {
        SendMessage {
            message_id,
            stream_id,
            total_fragments,
            reliability,
            acked_fragments: BTreeMap::new(),
            total_size,
            complete: false,
            cancelled: false,
            fragments: BTreeMap::new(),
            flags: reliability.to_flags(),
        }
    }

    pub fn store_fragment(&mut self, fragment_num: u64, payload: Vec<u8>, flags: u8) {
        self.fragments.insert(fragment_num, payload);
        if fragment_num == 0 {
            self.flags = flags;
        }
    }

    pub fn ack_fragment(&mut self, fragment_num: u64) {
        self.acked_fragments.insert(fragment_num, true);
        if self.reliability.is_reliable()
            && self.acked_fragments.len() == self.total_fragments as usize
        {
            self.complete = true;
            self.fragments.clear();
        }
    }

    pub fn unacked_fragments(&self) -> Vec<u64> {
        (0..self.total_fragments)
            .filter(|f| !self.acked_fragments.get(f).copied().unwrap_or(false))
            .collect()
    }

    pub fn all_acked(&self) -> bool {
        self.reliability.is_reliable()
            && self.acked_fragments.len() == self.total_fragments as usize
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.fragments.clear();
    }
}

pub struct MessageReassembly {
    pub message_id: u64,
    pub stream_id: u64,
    pub total_fragments: u64,
    pub total_size: usize,
    pub reliability: ReliabilityMode,
    fragments: BTreeMap<u64, Vec<u8>>,
    budget: MemoryBudget,
    pub complete: bool,
    created_at: std::time::Instant,
}

impl MessageReassembly {
    pub fn new(
        message_id: u64,
        stream_id: u64,
        total_fragments: u64,
        total_size: usize,
        reliability: ReliabilityMode,
        memory_limit: usize,
    ) -> Self {
        MessageReassembly {
            message_id,
            stream_id,
            total_fragments,
            total_size,
            reliability,
            fragments: BTreeMap::new(),
            budget: MemoryBudget::new(memory_limit),
            complete: false,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn add_fragment(
        &mut self,
        fragment_num: u64,
        total_fragments: u64,
        flags: u8,
        payload: Vec<u8>,
        max_fragments: u64,
        max_message_size: u64,
    ) -> AtpResult<bool> {
        if self.complete {
            return Ok(true);
        }
        if fragment_num >= total_fragments || total_fragments > max_fragments {
            return Err(AtpError::protocol("invalid fragment bounds"));
        }
        if total_fragments != self.total_fragments {
            return Err(AtpError::protocol("inconsistent total_fragments"));
        }
        if fragment_num == 0 && flags & super::wire::DATA_FLAG_FIRST == 0 {
            return Err(AtpError::protocol("fragment 0 missing FIRST flag"));
        }
        if fragment_num == total_fragments - 1 && flags & super::wire::DATA_FLAG_FIN == 0 {
            return Err(AtpError::protocol("last fragment missing FIN flag"));
        }
        if self.fragments.contains_key(&fragment_num) {
            return Ok(false);
        }

        let projected = self.budget.used() + payload.len();
        if projected as u64 > max_message_size {
            return Err(AtpError::memory("reassembly exceeds max message size"));
        }
        self.budget.try_reserve(payload.len())?;
        self.fragments.insert(fragment_num, payload);

        if self.fragments.len() == self.total_fragments as usize {
            self.complete = true;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn reassemble(&self) -> AtpResult<Vec<u8>> {
        if !self.complete {
            return Err(AtpError::message("reassembly incomplete"));
        }
        for i in 0..self.total_fragments {
            if !self.fragments.contains_key(&i) {
                return Err(AtpError::message(format!("missing fragment {}", i)));
            }
        }
        let mut result = Vec::new();
        for i in 0..self.total_fragments {
            result.extend_from_slice(&self.fragments[&i]);
        }
        if self.total_size > 0 && result.len() != self.total_size {
            return Err(AtpError::message("reassembled size mismatch"));
        }
        Ok(result)
    }

    pub fn memory_used(&self) -> usize {
        self.budget.used()
    }

    pub fn is_expired(&self, timeout: std::time::Duration) -> bool {
        self.created_at.elapsed() > timeout
    }
}

/// Ordered delivery state per stream.
#[derive(Debug, Default)]
pub struct OrderedDelivery {
    pub next_expected_message_id: u64,
    pub pending: BTreeMap<u64, Vec<u8>>,
}

impl OrderedDelivery {
    pub fn new() -> Self {
        OrderedDelivery {
            next_expected_message_id: 1,
            pending: BTreeMap::new(),
        }
    }

    /// Insert a completed message; returns messages ready for delivery in order.
    pub fn deliver(&mut self, message_id: u64, data: Vec<u8>, ordered: bool) -> Vec<(u64, Vec<u8>)> {
        if !ordered {
            return vec![(message_id, data)];
        }
        self.pending.insert(message_id, data);
        let mut ready = Vec::new();
        while let Some(data) = self.pending.remove(&self.next_expected_message_id) {
            ready.push((self.next_expected_message_id, data));
            self.next_expected_message_id += 1;
        }
        ready
    }
}

pub fn fragment_payload(payload: &[u8], max_fragment_size: usize) -> (Vec<Vec<u8>>, u64) {
    if payload.is_empty() {
        return (vec![Vec::new()], 1);
    }
    let max_frag = max_fragment_size.max(64);
    let chunks: Vec<Vec<u8>> = payload.chunks(max_frag).map(|c| c.to_vec()).collect();
    let total = chunks.len() as u64;
    if total > MAX_FRAGMENT_COUNT {
        return (vec![], 0);
    }
    (chunks, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordered_delivery() {
        let mut od = OrderedDelivery::new();
        let r1 = od.deliver(2, b"B".to_vec(), true);
        assert!(r1.is_empty());
        let r2 = od.deliver(1, b"A".to_vec(), true);
        assert_eq!(r2.len(), 2);
        assert_eq!(r2[0].1, b"A");
        assert_eq!(r2[1].1, b"B");
    }

    #[test]
    fn test_reassembly_validation() {
        let mut r = MessageReassembly::new(1, 1, 2, 0, ReliabilityMode::Reliable, 1024);
        assert!(r
            .add_fragment(0, 2, super::super::wire::DATA_FLAG_FIRST, vec![0; 10], 100, 1_000_000)
            .is_ok());
        assert!(r
            .add_fragment(2, 2, 0, vec![0; 10], 100, 1_000_000)
            .is_err()); // invalid fragment num
    }
}
