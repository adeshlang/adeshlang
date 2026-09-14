//! Deterministic UDP loss/reorder/delay simulation for tests.

use std::collections::VecDeque;
use std::net::SocketAddr;

/// Action to apply to a packet in the simulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAction {
    Deliver,
    Drop,
    /// Hold packet and deliver after `n` other packets are processed.
    Delay(u32),
    /// Buffer packet; deliver on next Deliver action (reorder).
    Reorder,
}

/// Script-driven packet simulator with reorder and delay support.
#[derive(Debug, Default)]
pub struct LossSimulator {
    script: VecDeque<SimAction>,
    delayed: VecDeque<(Vec<u8>, SocketAddr, u32)>,
    reorder_buffer: VecDeque<(Vec<u8>, SocketAddr)>,
    delivered: Vec<(Vec<u8>, SocketAddr)>,
    dropped: usize,
}

impl LossSimulator {
    pub fn new() -> Self {
        LossSimulator::default()
    }

    pub fn drop_next(&mut self, count: usize) {
        for _ in 0..count {
            self.script.push_back(SimAction::Drop);
        }
    }

    pub fn deliver_next(&mut self, count: usize) {
        for _ in 0..count {
            self.script.push_back(SimAction::Deliver);
        }
    }

    pub fn delay_next(&mut self, count: usize, delay: u32) {
        for _ in 0..count {
            self.script.push_back(SimAction::Delay(delay));
        }
    }

    pub fn reorder_next(&mut self, count: usize) {
        for _ in 0..count {
            self.script.push_back(SimAction::Reorder);
        }
    }

    fn tick_delayed(&mut self) {
        let mut ready = Vec::new();
        for (i, (_, _, rem)) in self.delayed.iter_mut().enumerate() {
            if *rem == 0 {
                ready.push(i);
            } else {
                *rem -= 1;
            }
        }
        for i in ready.into_iter().rev() {
            if let Some((data, addr, _)) = self.delayed.remove(i) {
                self.delivered.push((data, addr));
            }
        }
    }

    fn flush_reorder(&mut self) {
        while let Some(pkt) = self.reorder_buffer.pop_front() {
            self.delivered.push(pkt);
        }
    }

    /// Process an outbound packet through the simulator.
    pub fn send(&mut self, data: Vec<u8>, addr: SocketAddr) {
        self.tick_delayed();
        let action = self.script.pop_front().unwrap_or(SimAction::Deliver);
        match action {
            SimAction::Drop => {
                self.dropped += 1;
            }
            SimAction::Deliver => {
                self.delivered.push((data, addr));
            }
            SimAction::Delay(n) => {
                self.delayed.push_back((data, addr, n));
            }
            SimAction::Reorder => {
                self.reorder_buffer.push_back((data, addr));
            }
        }
        self.tick_delayed();
    }

    pub fn drain_delivered(&mut self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.flush_reorder();
        std::mem::take(&mut self.delivered)
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped
    }

    pub fn pending_count(&self) -> usize {
        self.delayed.len() + self.reorder_buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_drop_then_deliver() {
        let mut sim = LossSimulator::new();
        sim.drop_next(1);
        sim.deliver_next(1);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        sim.send(b"p1".to_vec(), addr);
        sim.send(b"p2".to_vec(), addr);
        assert_eq!(sim.dropped_count(), 1);
        let d = sim.drain_delivered();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, b"p2");
    }

    #[test]
    fn test_reorder() {
        let mut sim = LossSimulator::new();
        sim.reorder_next(1);
        sim.deliver_next(1);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        sim.send(b"first".to_vec(), addr);
        sim.send(b"second".to_vec(), addr);
        let d = sim.drain_delivered();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].0, b"second");
        assert_eq!(d[1].0, b"first");
    }
}
