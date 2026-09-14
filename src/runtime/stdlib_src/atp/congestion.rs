//! ATP Congestion Controller
//!
//! Uses a well-understood congestion-control approach (NewReno-style) as the
//! baseline, with room for experimental algorithms later.
//!
//! The controller limits how much data can be in flight at once, based on
//! RTT measurements and loss signals.

/// Congestion state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionState {
    /// Slow start: exponential growth.
    SlowStart,
    /// Congestion avoidance: linear growth.
    CongestionAvoidance,
    /// Fast recovery: after a loss event.
    FastRecovery,
}

/// NewReno-style congestion controller.
pub struct CongestionController {
    /// Congestion window (bytes).
    cwnd: usize,
    /// Slow-start threshold (bytes).
    ssthresh: usize,
    /// Current state.
    state: CongestionState,
    /// Initial window.
    initial_window: usize,
    /// Minimum window (safety floor).
    minimum_window: usize,
    /// Maximum window.
    maximum_window: usize,
    /// Bytes acked since last cwnd increase (for congestion avoidance).
    bytes_acked: usize,
    /// MSS (maximum segment size).
    mss: usize,
}

impl CongestionController {
    pub fn new(mss: usize, max_window: usize) -> Self {
        let initial_window = 10 * mss; // ~14 KB with 1450 MSS
        CongestionController {
            cwnd: initial_window,
            ssthresh: max_window,
            state: CongestionState::SlowStart,
            initial_window,
            minimum_window: 2 * mss,
            maximum_window: max_window,
            bytes_acked: 0,
            mss,
        }
    }

    /// Current congestion window (bytes).
    pub fn cwnd(&self) -> usize {
        self.cwnd
    }

    /// Current state.
    pub fn state(&self) -> CongestionState {
        self.state
    }

    /// Whether the controller allows sending `bytes` more data.
    pub fn can_send(&self, bytes_in_flight: usize) -> bool {
        bytes_in_flight < self.cwnd
    }

    /// Available send budget.
    pub fn available(&self, bytes_in_flight: usize) -> usize {
        self.cwnd.saturating_sub(bytes_in_flight)
    }

    /// Called when `acked_bytes` are newly acknowledged.
    pub fn on_acked(&mut self, acked_bytes: usize) {
        match self.state {
            CongestionState::SlowStart => {
                // Exponential growth: cwnd += acked_bytes.
                self.cwnd = (self.cwnd + acked_bytes).min(self.maximum_window);
                if self.cwnd >= self.ssthresh {
                    self.state = CongestionState::CongestionAvoidance;
                }
            }
            CongestionState::CongestionAvoidance => {
                // Linear growth: cwnd += MSS * (acked_bytes / cwnd).
                self.bytes_acked += acked_bytes;
                if self.bytes_acked >= self.cwnd {
                    self.cwnd = (self.cwnd + self.mss).min(self.maximum_window);
                    self.bytes_acked = 0;
                }
            }
            CongestionState::FastRecovery => {
                // In fast recovery, each new ACK inflates cwnd.
                self.cwnd = (self.cwnd + self.mss).min(self.maximum_window);
            }
        }
    }

    /// Called when a loss is detected (3 duplicate ACKs / gap-based).
    pub fn on_loss(&mut self) {
        // Halve the window and set ssthresh.
        self.ssthresh = (self.cwnd / 2).max(self.minimum_window * 2);
        self.cwnd = self.ssthresh;
        self.state = CongestionState::CongestionAvoidance;
    }

    /// Called when a timeout occurs (RTO expiry).
    pub fn on_timeout(&mut self) {
        self.ssthresh = (self.cwnd / 2).max(self.minimum_window * 2);
        self.cwnd = self.initial_window;
        self.state = CongestionState::SlowStart;
        self.bytes_acked = 0;
    }

    /// Called when entering fast recovery.
    pub fn enter_fast_recovery(&mut self) {
        self.ssthresh = (self.cwnd / 2).max(self.minimum_window * 2);
        self.cwnd = self.ssthresh + 3 * self.mss;
        self.state = CongestionState::FastRecovery;
    }

    /// Called when fast recovery completes.
    pub fn exit_fast_recovery(&mut self) {
        self.cwnd = self.ssthresh;
        self.state = CongestionState::CongestionAvoidance;
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        self.cwnd = self.initial_window;
        self.ssthresh = self.maximum_window;
        self.state = CongestionState::SlowStart;
        self.bytes_acked = 0;
    }
}

impl Default for CongestionController {
    fn default() -> Self {
        CongestionController::new(1450, 4 * 1024 * 1024)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slow_start_to_congestion_avoidance() {
        let mut cc = CongestionController::new(1450, 1_000_000);
        assert_eq!(cc.state(), CongestionState::SlowStart);

        // Ack enough to grow past ssthresh.
        cc.on_acked(100_000);
        assert!(cc.cwnd() > 14_500); // grew from initial
    }

    #[test]
    fn test_loss_halves_window() {
        let mut cc = CongestionController::new(1450, 1_000_000);
        cc.on_acked(100_000);
        let cwnd_before = cc.cwnd();
        cc.on_loss();
        assert!(cc.cwnd() < cwnd_before);
        assert_eq!(cc.state(), CongestionState::CongestionAvoidance);
    }

    #[test]
    fn test_timeout_resets_to_slow_start() {
        let mut cc = CongestionController::new(1450, 1_000_000);
        cc.on_acked(100_000);
        cc.on_timeout();
        assert_eq!(cc.state(), CongestionState::SlowStart);
        assert_eq!(cc.cwnd(), cc.initial_window);
    }

    #[test]
    fn test_can_send() {
        let cc = CongestionController::new(1450, 1_000_000);
        assert!(cc.can_send(0));
        assert!(cc.can_send(cc.cwnd() - 1));
        assert!(!cc.can_send(cc.cwnd() + 1));
    }
}
