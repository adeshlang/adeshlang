//! Packet/frame scheduler with two-level deficit round-robin.
//!
//! Level 1: priority classes (Critical > High > Medium > Low)
//! Level 2: per-stream fairness within each priority class

use super::stream::Priority;
use super::wire::Frame;
use std::collections::{HashMap, VecDeque};

const WEIGHT_CRITICAL: i64 = 4096;
const WEIGHT_HIGH: i64 = 2048;
const WEIGHT_MEDIUM: i64 = 1024;
const WEIGHT_LOW: i64 = 512;
const STREAM_WEIGHT: i64 = 1024;

#[derive(Debug, Clone)]
struct ScheduledFrame {
    frame: Frame,
    byte_cost: i64,
}

#[derive(Debug, Default)]
struct StreamQueue {
    frames: VecDeque<ScheduledFrame>,
    deficit: i64,
}

#[derive(Debug, Default)]
struct PriorityBucket {
    streams: HashMap<u64, StreamQueue>,
    order: VecDeque<u64>,
    deficit: i64,
}

/// Priority-aware frame scheduler with weighted DRR across priorities and streams.
pub struct PacketScheduler {
    buckets: [PriorityBucket; 4],
    control: VecDeque<Frame>,
    max_queue: usize,
    /// Round-robin starting priority for weighted fairness across classes.
    rr_cursor: usize,
}

impl PacketScheduler {
    pub fn new() -> Self {
        PacketScheduler {
            buckets: Default::default(),
            control: VecDeque::new(),
            max_queue: 4096,
            rr_cursor: 0,
        }
    }

    pub fn with_max_queue(max_queue: usize) -> Self {
        PacketScheduler {
            buckets: Default::default(),
            control: VecDeque::new(),
            max_queue,
            rr_cursor: 0,
        }
    }

    fn priority_index(p: Priority) -> usize {
        match p {
            Priority::Low => 0,
            Priority::Medium => 1,
            Priority::High => 2,
            Priority::Critical => 3,
        }
    }

    fn priority_weight(idx: usize) -> i64 {
        match idx {
            0 => WEIGHT_LOW,
            1 => WEIGHT_MEDIUM,
            2 => WEIGHT_HIGH,
            _ => WEIGHT_CRITICAL,
        }
    }

    /// Queue a frame for transmission.
    pub fn enqueue(&mut self, frame: Frame, priority: Priority, stream_id: u64) -> bool {
        if self.len() >= self.max_queue {
            return false;
        }
        if Self::is_control(&frame) {
            self.control.push_back(frame);
            return true;
        }
        let byte_cost = frame_byte_cost(&frame);
        let idx = Self::priority_index(priority);
        let bucket = &mut self.buckets[idx];
        if !bucket.streams.contains_key(&stream_id) {
            bucket.order.push_back(stream_id);
        }
        bucket.streams
            .entry(stream_id)
            .or_default()
            .frames
            .push_back(ScheduledFrame { frame, byte_cost });
        true
    }

    fn is_control(frame: &Frame) -> bool {
        matches!(
            frame,
            Frame::Ack { .. }
                | Frame::Ping
                | Frame::Pong
                | Frame::ConnectionClose { .. }
                | Frame::MaxData { .. }
                | Frame::MaxStreamData { .. }
                | Frame::KeyUpdate { .. }
                | Frame::PathChallenge { .. }
                | Frame::PathResponse { .. }
        )
    }

    fn pop_from_bucket(bucket: &mut PriorityBucket) -> Option<Frame> {
        if bucket.order.is_empty() {
            return None;
        }

        let rotations = bucket.order.len();
        for _ in 0..rotations {
            let stream_id = bucket.order.pop_front()?;
            let sq = bucket.streams.get_mut(&stream_id)?;
            if sq.frames.is_empty() {
                bucket.streams.remove(&stream_id);
                continue;
            }
            sq.deficit += STREAM_WEIGHT;
            if sq.deficit >= sq.frames.front().unwrap().byte_cost {
                let entry = sq.frames.pop_front().unwrap();
                sq.deficit -= entry.byte_cost;
                if sq.frames.is_empty() {
                    bucket.streams.remove(&stream_id);
                } else {
                    bucket.order.push_back(stream_id);
                }
                return Some(entry.frame);
            }
            bucket.order.push_back(stream_id);
        }

        // Fallback: any frame to prevent stall.
        for stream_id in bucket.order.drain(..).collect::<Vec<_>>() {
            if let Some(sq) = bucket.streams.get_mut(&stream_id) {
                if let Some(entry) = sq.frames.pop_front() {
                    if sq.frames.is_empty() {
                        bucket.streams.remove(&stream_id);
                    }
                    return Some(entry.frame);
                }
            }
        }
        None
    }

    fn bucket_min_cost(bucket: &PriorityBucket) -> i64 {
        bucket
            .streams
            .values()
            .filter_map(|sq| sq.frames.front())
            .map(|f| f.byte_cost)
            .min()
            .unwrap_or(STREAM_WEIGHT)
    }

    fn priority_quota(idx: usize) -> usize {
        (Self::priority_weight(idx) / WEIGHT_LOW) as usize
    }

    /// Drain up to `max_frames` frames using weighted deficit round-robin.
    pub fn drain(&mut self, max_frames: usize) -> Vec<Frame> {
        let mut out = Vec::with_capacity(max_frames);

        while out.len() < max_frames {
            if let Some(f) = self.control.pop_front() {
                out.push(f);
                continue;
            }

            let mut sent = false;
            for offset in 0..4 {
                let i = (self.rr_cursor + offset) % 4;
                let bucket = &mut self.buckets[i];
                if bucket.streams.is_empty() {
                    continue;
                }
                bucket.deficit += Self::priority_weight(i);
                let quota = Self::priority_quota(i);
                for _ in 0..quota {
                    if out.len() >= max_frames {
                        break;
                    }
                    let min_cost = Self::bucket_min_cost(bucket);
                    if bucket.deficit < min_cost {
                        break;
                    }
                    if let Some(frame) = Self::pop_from_bucket(bucket) {
                        let cost = frame_byte_cost(&frame);
                        bucket.deficit -= cost;
                        out.push(frame);
                        sent = true;
                    } else {
                        break;
                    }
                }
                if sent {
                    self.rr_cursor = (i + 1) % 4;
                    break;
                }
            }

            if !sent {
                break;
            }
        }

        out
    }

    pub fn is_empty(&self) -> bool {
        self.control.is_empty() && self.buckets.iter().all(|b| b.streams.is_empty())
    }

    pub fn len(&self) -> usize {
        self.control.len()
            + self
                .buckets
                .iter()
                .map(|b| b.streams.values().map(|s| s.frames.len()).sum::<usize>())
                .sum::<usize>()
    }
}

impl Default for PacketScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn frame_byte_cost(frame: &Frame) -> i64 {
    frame.encode().len().max(64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datagram(label: &[u8]) -> Frame {
        Frame::Datagram {
            payload: label.to_vec(),
        }
    }

    #[test]
    fn test_control_frames_first() {
        let mut sched = PacketScheduler::new();
        sched.enqueue(
            Frame::Data {
                stream_id: 1,
                message_id: 1,
                fragment_num: 0,
                total_fragments: 1,
                flags: 0,
                payload: vec![0; 100],
            },
            Priority::Low,
            1,
        );
        sched.enqueue(Frame::Ping, Priority::Low, 0);
        let frames = sched.drain(2);
        assert!(matches!(frames[0], Frame::Ping));
    }

    #[test]
    fn test_higher_priority_gets_more_service() {
        let mut sched = PacketScheduler::new();
        for _ in 0..32 {
            sched.enqueue(datagram(b"low"), Priority::Low, 1);
            sched.enqueue(datagram(b"critical"), Priority::Critical, 2);
        }
        let frames = sched.drain(50);
        let critical = frames
            .iter()
            .filter(|f| matches!(f, Frame::Datagram { payload } if payload == b"critical"))
            .count();
        let low = frames
            .iter()
            .filter(|f| matches!(f, Frame::Datagram { payload } if payload == b"low"))
            .count();
        assert!(critical > low, "critical={critical} low={low}");
        assert_eq!(critical + low, 50);
    }

    #[test]
    fn test_per_stream_fairness() {
        let mut sched = PacketScheduler::new();
        for _ in 0..5 {
            sched.enqueue(datagram(b"s1"), Priority::Medium, 1);
            sched.enqueue(datagram(b"s2"), Priority::Medium, 2);
        }
        let frames = sched.drain(4);
        // Alternating streams at same priority.
        assert_eq!(frames.len(), 4);
        let s1_count = frames
            .iter()
            .filter(|f| matches!(f, Frame::Datagram { payload } if payload == b"s1"))
            .count();
        let s2_count = frames
            .iter()
            .filter(|f| matches!(f, Frame::Datagram { payload } if payload == b"s2"))
            .count();
        assert!(s1_count >= 1 && s2_count >= 1);
    }

    #[test]
    fn test_weighted_byte_distribution_under_sustained_backlog() {
        let mut sched = PacketScheduler::new();
        for _ in 0..200 {
            sched.enqueue(datagram(b"c"), Priority::Critical, 1);
            sched.enqueue(datagram(b"h"), Priority::High, 2);
            sched.enqueue(datagram(b"m"), Priority::Medium, 3);
            sched.enqueue(datagram(b"l"), Priority::Low, 4);
        }

        let frames = sched.drain(150);
        assert_eq!(frames.len(), 150);

        let bytes_for = |label: &[u8]| -> usize {
            frames
                .iter()
                .filter(|f| matches!(f, Frame::Datagram { payload } if payload == label))
                .map(|f| f.encode().len())
                .sum()
        };

        let bc = bytes_for(b"c") as f64;
        let bh = bytes_for(b"h") as f64;
        let bm = bytes_for(b"m") as f64;
        let bl = bytes_for(b"l") as f64;
        let total = bc + bh + bm + bl;
        assert!(total > 0.0);

        const TOLERANCE: f64 = 0.15;
        let weights = [8.0_f64, 4.0, 2.0, 1.0];
        let bytes = [bc, bh, bm, bl];
        let total_weight: f64 = weights.iter().sum();
        for (i, &weight) in weights.iter().enumerate() {
            let expected_frac = weight / total_weight;
            let actual_frac = bytes[i] / total;
            let rel_err = (actual_frac - expected_frac).abs() / expected_frac;
            assert!(
                rel_err < TOLERANCE,
                "priority bucket {i}: expected_frac={expected_frac:.3} actual_frac={actual_frac:.3} rel_err={rel_err:.3}"
            );
        }

        let ratio = |a: f64, b: f64| a / b.max(1.0);
        assert!((ratio(bc, bh) - 2.0).abs() / 2.0 < TOLERANCE, "c/h={}", ratio(bc, bh));
        assert!((ratio(bh, bm) - 2.0).abs() / 2.0 < TOLERANCE, "h/m={}", ratio(bh, bm));
        assert!((ratio(bm, bl) - 2.0).abs() / 2.0 < TOLERANCE, "m/l={}", ratio(bm, bl));
    }
}
