//! ATP Reliability Engine (RUDP)
//!
//! Frame-level reliability with per-fragment retransmission support.

use super::ack::{decode_ack_ranges, encode_ack_ranges};
use super::config::{INITIAL_RTT_MS, MAX_ACK_RANGES, MAX_RTO_MS, MIN_RTO_MS, REORDER_THRESHOLD};
use super::errors::AtpResult;
use super::wire::Frame;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// RTT estimator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RttEstimator {
    pub smoothed_rtt: Duration,
    pub rttvar: Duration,
    pub min_rtt: Duration,
    pub max_ack_delay: Duration,
    pub latest_rtt: Duration,
    pub initialized: bool,
    rto_backoff: u32,
}

impl RttEstimator {
    pub fn new() -> Self {
        RttEstimator {
            smoothed_rtt: Duration::from_millis(INITIAL_RTT_MS),
            rttvar: Duration::from_millis(INITIAL_RTT_MS / 2),
            min_rtt: Duration::from_millis(INITIAL_RTT_MS),
            max_ack_delay: Duration::from_millis(25),
            latest_rtt: Duration::ZERO,
            initialized: false,
            rto_backoff: 0,
        }
    }

    pub fn update(&mut self, rtt_sample: Duration, ack_delay: Duration) {
        self.latest_rtt = rtt_sample;
        let adjusted = rtt_sample.checked_sub(ack_delay).unwrap_or(Duration::ZERO);
        if !self.initialized {
            self.min_rtt = adjusted;
            self.smoothed_rtt = adjusted;
            self.rttvar = adjusted / 2;
            self.initialized = true;
        } else {
            self.min_rtt = self.min_rtt.min(adjusted);
            let diff = adjusted.abs_diff(self.smoothed_rtt);
            self.rttvar = self.rttvar * 3 / 4 + diff / 4;
            self.smoothed_rtt = self.smoothed_rtt * 7 / 8 + adjusted / 8;
        }
        self.rto_backoff = 0;
    }

    pub fn rto(&self) -> Duration {
        let base = self.smoothed_rtt + self.rttvar * 4 + self.max_ack_delay;
        let base = base
            .max(Duration::from_millis(MIN_RTO_MS))
            .min(Duration::from_millis(MAX_RTO_MS));
        if self.rto_backoff == 0 {
            base
        } else {
            let shift = self.rto_backoff.min(10);
            base.saturating_mul(1u32 << shift)
        }
    }

    pub fn on_timeout(&mut self) {
        self.rto_backoff = self.rto_backoff.saturating_add(1);
    }
}

impl Default for RttEstimator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Retransmittable reference
// ---------------------------------------------------------------------------

/// Identifies a retransmittable unit within a sent packet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RetransmittableRef {
    pub stream_id: u64,
    pub message_id: u64,
    pub fragment_num: u64,
}

/// Metadata for a sent packet awaiting acknowledgement.
#[derive(Debug, Clone)]
pub struct SentPacket {
    pub packet_number: u64,
    pub sent_time: Instant,
    pub bytes_sent: usize,
    pub ack_eliciting: bool,
    pub tx_count: u32,
    pub refs: Vec<RetransmittableRef>,
}

// ---------------------------------------------------------------------------
// ACK tracker (receive side)
// ---------------------------------------------------------------------------

pub struct AckTracker {
    received: BTreeSet<u64>,
    largest: u64,
    largest_time: Option<Instant>,
    pub ack_queued: bool,
    packets_since_ack: u32,
    ack_frequency: u32,
}

impl AckTracker {
    pub fn new() -> Self {
        AckTracker {
            received: BTreeSet::new(),
            largest: 0,
            largest_time: None,
            ack_queued: false,
            packets_since_ack: 0,
            ack_frequency: 10,
        }
    }

    pub fn on_packet_received(&mut self, pn: u64, now: Instant) {
        if self.received.is_empty() || pn > self.largest {
            self.largest = pn;
            self.largest_time = Some(now);
        }
        self.received.insert(pn);
        self.packets_since_ack += 1;
        if self.packets_since_ack >= self.ack_frequency {
            self.ack_queued = true;
        }
    }

    pub fn force_ack(&mut self) {
        self.ack_queued = true;
    }

    pub fn build_ack_frame(&mut self, now: Instant) -> Option<Frame> {
        if !self.ack_queued || self.received.is_empty() {
            return None;
        }
        self.ack_queued = false;
        self.packets_since_ack = 0;

        let ack_delay = self
            .largest_time
            .map(|t| now.duration_since(t).as_millis() as u64)
            .unwrap_or(0);

        let (largest_acked, ranges) = encode_ack_ranges(&self.received).ok()?;
        if ranges.len() > MAX_ACK_RANGES {
            return None;
        }

        Some(Frame::Ack {
            largest_acked,
            ack_delay,
            ranges,
        })
    }

    pub fn prune(&mut self, keep_below: u64) {
        self.received.retain(|&pn| pn >= keep_below);
    }
}

impl Default for AckTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Reliability tracker (send side)
// ---------------------------------------------------------------------------

pub struct ReliabilityTracker {
    pub in_flight: BTreeMap<u64, SentPacket>,
    pub lost: Vec<RetransmittableRef>,
    pub rtt: RttEstimator,
    pub next_packet_number: u64,
    pub bytes_in_flight: usize,
    pub bytes_acked_total: usize,
    pub packets_lost: u64,
}

impl ReliabilityTracker {
    pub fn new() -> Self {
        ReliabilityTracker {
            in_flight: BTreeMap::new(),
            lost: Vec::new(),
            rtt: RttEstimator::new(),
            next_packet_number: 0,
            bytes_in_flight: 0,
            bytes_acked_total: 0,
            packets_lost: 0,
        }
    }

    pub fn allocate_packet_number(&mut self) -> u64 {
        let pn = self.next_packet_number;
        self.next_packet_number += 1;
        pn
    }

    pub fn on_packet_sent(&mut self, packet: SentPacket) {
        self.bytes_in_flight += packet.bytes_sent;
        self.in_flight.insert(packet.packet_number, packet);
    }

    /// Process an ACK. Returns (newly_acked_pns, newly_acked_bytes, acked_refs).
    pub fn process_ack(
        &mut self,
        largest_acked: u64,
        ack_delay_ms: u64,
        ranges: &[(u64, u64)],
        now: Instant,
    ) -> AtpResult<(Vec<u64>, usize, Vec<RetransmittableRef>)> {
        let acked_set = decode_ack_ranges(largest_acked, ranges)?;
        let ack_delay = Duration::from_millis(ack_delay_ms);
        let mut newly_acked = Vec::new();
        let mut acked_bytes = 0usize;
        let mut acked_refs = Vec::new();
        let mut first_rtt = true;

        for pn in &acked_set {
            if let Some(pkt) = self.in_flight.remove(pn) {
                self.bytes_in_flight = self.bytes_in_flight.saturating_sub(pkt.bytes_sent);
                acked_bytes += pkt.bytes_sent;
                self.bytes_acked_total += pkt.bytes_sent;
                if first_rtt {
                    let sample = now.duration_since(pkt.sent_time);
                    self.rtt.update(sample, ack_delay);
                    first_rtt = false;
                }
                acked_refs.extend(pkt.refs.clone());
                newly_acked.push(*pn);
            }
        }

        // Gap-based loss detection.
        let loss_threshold = largest_acked.saturating_sub(REORDER_THRESHOLD);
        let mut to_remove = Vec::new();
        for (&pn, pkt) in &self.in_flight {
            if pn <= loss_threshold && pkt.ack_eliciting {
                to_remove.push(pn);
            }
        }
        for pn in to_remove {
            if let Some(pkt) = self.in_flight.remove(&pn) {
                self.bytes_in_flight = self.bytes_in_flight.saturating_sub(pkt.bytes_sent);
                self.packets_lost += 1;
                self.lost.extend(pkt.refs);
            }
        }

        Ok((newly_acked, acked_bytes, acked_refs))
    }

    pub fn detect_timeouts(&mut self, now: Instant) -> Vec<RetransmittableRef> {
        let rto = self.rtt.rto();
        let mut timed_out_refs = Vec::new();
        let mut to_remove = Vec::new();

        for (&pn, pkt) in &self.in_flight {
            if pkt.ack_eliciting && now.duration_since(pkt.sent_time) > rto {
                to_remove.push(pn);
            }
        }

        if !to_remove.is_empty() {
            self.rtt.on_timeout();
        }

        for pn in to_remove {
            if let Some(pkt) = self.in_flight.remove(&pn) {
                self.bytes_in_flight = self.bytes_in_flight.saturating_sub(pkt.bytes_sent);
                self.packets_lost += 1;
                timed_out_refs.extend(pkt.refs);
            }
        }

        timed_out_refs
    }

    pub fn drain_lost(&mut self) -> Vec<RetransmittableRef> {
        std::mem::take(&mut self.lost)
    }

    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    pub fn has_in_flight(&self) -> bool {
        !self.in_flight.is_empty()
    }
}

impl Default for ReliabilityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_roundtrip_via_tracker() {
        let mut tracker = AckTracker::new();
        let now = Instant::now();
        for &pn in &[1u64, 2, 3, 5, 6, 8] {
            tracker.on_packet_received(pn, now);
        }
        tracker.force_ack();
        let frame = tracker.build_ack_frame(now).unwrap();
        if let Frame::Ack {
            largest_acked,
            ranges,
            ..
        } = frame
        {
            assert_eq!(largest_acked, 8);
            let decoded = decode_ack_ranges(largest_acked, &ranges).unwrap();
            assert_eq!(decoded, tracker.received);
        }
    }

    #[test]
    fn test_reliability_ack_bytes() {
        let mut tracker = ReliabilityTracker::new();
        let now = Instant::now();
        for pn in 0..5 {
            tracker.on_packet_sent(SentPacket {
                packet_number: pn,
                sent_time: now,
                bytes_sent: 100 + pn as usize,
                ack_eliciting: true,
                tx_count: 1,
                refs: vec![RetransmittableRef {
                    stream_id: 1,
                    message_id: 1,
                    fragment_num: pn,
                }],
            });
        }
        let (_, bytes, _) = tracker.process_ack(4, 0, &[(0, 4)], now).unwrap();
        assert_eq!(bytes, 100 + 101 + 102 + 103 + 104);
        assert_eq!(tracker.in_flight_count(), 0);
    }

    #[test]
    fn test_timeout_retransmittable_refs() {
        let mut tracker = ReliabilityTracker::new();
        let now = Instant::now();
        tracker.on_packet_sent(SentPacket {
            packet_number: 0,
            sent_time: now,
            bytes_sent: 200,
            ack_eliciting: true,
            tx_count: 1,
            refs: vec![RetransmittableRef {
                stream_id: 1,
                message_id: 5,
                fragment_num: 2,
            }],
        });
        let future = now + Duration::from_secs(120);
        let refs = tracker.detect_timeouts(future);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].fragment_num, 2);
    }
}
