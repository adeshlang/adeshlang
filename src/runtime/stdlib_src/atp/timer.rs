//! ATP Timer Wheel — fixed-slot timer wheel with reschedule support.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TimerEntry {
    pub deadline: Instant,
    pub kind: TimerKind,
    pub stream_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerKind {
    AckDelay,
    Retransmission,
    Handshake,
    Idle,
    Keepalive,
    PathValidation,
    KeyUpdate,
    CloseDrain,
}

pub struct TimerWheel {
    slots: Vec<Vec<TimerEntry>>,
    current_tick: usize,
    tick_duration: Duration,
    last_advance: Instant,
    timer_count: usize,
}

impl TimerWheel {
    pub fn new(num_slots: usize, tick_duration: Duration, now: Instant) -> Self {
        assert!(num_slots > 0, "timer wheel requires num_slots > 0");
        assert!(
            tick_duration > Duration::ZERO,
            "timer wheel requires tick_duration > 0"
        );
        let mut slots = Vec::with_capacity(num_slots);
        for _ in 0..num_slots {
            slots.push(Vec::new());
        }
        TimerWheel {
            slots,
            current_tick: 0,
            tick_duration,
            last_advance: now,
            timer_count: 0,
        }
    }

    pub fn schedule(
        &mut self,
        deadline: Instant,
        kind: TimerKind,
        stream_id: u64,
        message_id: u64,
    ) {
        let entry = TimerEntry {
            deadline,
            kind,
            stream_id,
            message_id,
        };
        let slot = self.slot_for(deadline);
        self.slots[slot].push(entry);
        self.timer_count += 1;
    }

    fn slot_for(&self, deadline: Instant) -> usize {
        let now = self.last_advance;
        if deadline <= now {
            return self.current_tick;
        }
        let ticks_ahead = (deadline - now).as_millis() / self.tick_duration.as_millis().max(1);
        let ticks_ahead = ticks_ahead as usize;
        (self.current_tick + ticks_ahead) % self.slots.len()
    }

    pub fn advance(&mut self, now: Instant) -> Vec<TimerEntry> {
        let mut expired = Vec::new();
        let tick_ms = self.tick_duration.as_millis().max(1);
        let elapsed = now.duration_since(self.last_advance);
        let ticks_elapsed = (elapsed.as_millis() / tick_ms) as usize;

        for _ in 0..ticks_elapsed.min(self.slots.len()) {
            self.current_tick = (self.current_tick + 1) % self.slots.len();
            let slot_timers = std::mem::take(&mut self.slots[self.current_tick]);
            for timer in slot_timers {
                if timer.deadline <= now {
                    expired.push(timer);
                    self.timer_count = self.timer_count.saturating_sub(1);
                } else {
                    let slot = self.slot_for(timer.deadline);
                    self.slots[slot].push(timer);
                }
            }
        }

        self.last_advance = now;
        expired
    }

    pub fn count(&self) -> usize {
        self.timer_count
    }

    pub fn cancel_where<F>(&mut self, predicate: F)
    where
        F: Fn(&TimerEntry) -> bool,
    {
        for slot in &mut self.slots {
            let before = slot.len();
            slot.retain(|t| !predicate(t));
            self.timer_count -= before - slot.len();
        }
    }

    pub fn cancel_stream(&mut self, stream_id: u64) {
        self.cancel_where(|t| t.stream_id == stream_id);
    }

    pub fn cancel_message(&mut self, stream_id: u64, message_id: u64) {
        self.cancel_where(|t| t.stream_id == stream_id && t.message_id == message_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_fires() {
        let now = Instant::now();
        let mut wheel = TimerWheel::new(100, Duration::from_millis(10), now);
        wheel.schedule(now + Duration::from_millis(50), TimerKind::Keepalive, 0, 0);
        let expired = wheel.advance(now + Duration::from_millis(60));
        assert!(!expired.is_empty());
    }

    #[test]
    fn test_boundary_deadlines() {
        let now = Instant::now();
        let mut wheel = TimerWheel::new(100, Duration::from_millis(10), now);
        for ms in [1, 9, 10, 11, 19, 20] {
            wheel.schedule(now + Duration::from_millis(ms), TimerKind::Idle, 0, 0);
        }
        let expired = wheel.advance(now + Duration::from_millis(25));
        assert!(!expired.is_empty());
    }

    #[test]
    #[should_panic(expected = "num_slots")]
    fn test_reject_zero_slots() {
        let _ = TimerWheel::new(0, Duration::from_millis(10), Instant::now());
    }
}
