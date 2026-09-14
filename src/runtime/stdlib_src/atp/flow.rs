//! ATP Flow Control
//!
//! Two-level flow control:
//! - Connection-level: limits total data the peer can send.
//! - Stream-level: limits data per individual stream.
//!
//! Backpressure is applied by reducing advertised receive windows when
//! the application is not consuming data fast enough.

use super::errors::{AtpError, AtpResult};

/// Flow control window for one direction (send or receive).
#[derive(Debug, Clone)]
pub struct FlowWindow {
    /// Maximum bytes that can be sent/received.
    max: u64,
    /// Bytes consumed so far.
    consumed: u64,
}

impl FlowWindow {
    pub fn new(initial_max: u64) -> Self {
        FlowWindow {
            max: initial_max,
            consumed: 0,
        }
    }

    /// Try to consume `amount` bytes from the send window.
    /// Returns error if over budget.
    pub fn try_consume(&mut self, amount: u64) -> AtpResult<()> {
        if self.consumed + amount > self.max {
            return Err(AtpError::stream(format!(
                "flow control: consumed={}, requested={}, max={}",
                self.consumed, amount, self.max
            )));
        }
        self.consumed += amount;
        Ok(())
    }

    /// Update the maximum (when peer sends MAX_DATA / MAX_STREAM_DATA).
    pub fn update_max(&mut self, new_max: u64) {
        self.max = self.max.max(new_max);
    }

    /// Available bytes.
    pub fn available(&self) -> u64 {
        self.max.saturating_sub(self.consumed)
    }

    /// Current max.
    pub fn max(&self) -> u64 {
        self.max
    }

    /// Current consumption.
    pub fn consumed(&self) -> u64 {
        self.consumed
    }
}

/// Connection-level flow control (both directions).
#[derive(Debug, Clone)]
pub struct ConnectionFlowControl {
    /// How much data we are allowed to send (peer's limit on us).
    pub send: FlowWindow,
    /// How much data the peer is allowed to send (our limit on them).
    pub recv: FlowWindow,
    /// Initial receive window we advertise.
    initial_recv_window: u64,
}

impl ConnectionFlowControl {
    pub fn new(initial_send_window: u64, initial_recv_window: u64) -> Self {
        ConnectionFlowControl {
            send: FlowWindow::new(initial_send_window),
            recv: FlowWindow::new(initial_recv_window),
            initial_recv_window,
        }
    }

    /// Whether we should send a MAX_DATA frame to increase the peer's send window.
    pub fn should_send_max_data(&self) -> Option<u64> {
        // When the peer has consumed more than half of our advertised window,
        // send an update.
        if self.recv.available() < self.initial_recv_window / 2 {
            Some(self.recv.consumed + self.initial_recv_window)
        } else {
            None
        }
    }
}

/// Stream-level flow control.
#[derive(Debug, Clone)]
pub struct StreamFlowControl {
    pub send: FlowWindow,
    pub recv: FlowWindow,
    initial_recv_window: u64,
}

impl StreamFlowControl {
    pub fn new(initial_send: u64, initial_recv: u64) -> Self {
        StreamFlowControl {
            send: FlowWindow::new(initial_send),
            recv: FlowWindow::new(initial_recv),
            initial_recv_window: initial_recv,
        }
    }

    pub fn should_send_max_stream_data(&self) -> Option<u64> {
        if self.recv.available() < self.initial_recv_window / 2 {
            Some(self.recv.consumed + self.initial_recv_window)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_window() {
        let mut w = FlowWindow::new(1000);
        assert!(w.try_consume(600).is_ok());
        assert_eq!(w.available(), 400);
        assert!(w.try_consume(500).is_err());
        w.update_max(2000);
        assert!(w.try_consume(500).is_ok());
    }

    #[test]
    fn test_connection_flow_control() {
        let mut fc = ConnectionFlowControl::new(1_000_000, 1_000_000);
        // Consume most of the receive window.
        fc.recv.try_consume(600_000).unwrap();
        assert!(fc.should_send_max_data().is_some());
    }
}
