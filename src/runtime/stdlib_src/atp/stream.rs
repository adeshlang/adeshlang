//! ATP Stream Management
//!
//! Streams provide independent, multiplexed channels within a single
//! ATP connection. Each stream has its own:
//! - Sequence state
//! - Flow-control windows
//! - Message queues
//! - Buffering
//! - Cancellation
//! - Priority

use super::errors::{AtpError, AtpResult};
use super::flow::StreamFlowControl;
use super::message::SendMessage;
use std::collections::BTreeMap;

/// Stream state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Opening,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
    Reset,
}

/// Priority level for a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Medium
    }
}

/// An ATP stream.
pub struct AtpStream {
    pub stream_id: u64,
    pub state: StreamState,
    pub priority: Priority,
    /// Flow control for this stream.
    pub flow: StreamFlowControl,
    /// Outgoing messages (send side).
    pub send_messages: BTreeMap<u64, SendMessage>,
    /// Next message ID to assign for this stream.
    pub next_message_id: u64,
    /// Total bytes queued for sending.
    pub send_queue_bytes: usize,
    /// Total bytes received.
    pub recv_bytes: usize,
}

impl AtpStream {
    pub fn new(stream_id: u64, initial_send_window: u64, initial_recv_window: u64) -> Self {
        AtpStream {
            stream_id,
            state: StreamState::Idle,
            priority: Priority::default(),
            flow: StreamFlowControl::new(initial_send_window, initial_recv_window),
            send_messages: BTreeMap::new(),
            next_message_id: 1,
            send_queue_bytes: 0,
            recv_bytes: 0,
        }
    }

    /// Open the stream.
    pub fn open(&mut self) {
        self.state = StreamState::Open;
    }

    /// Close the local side.
    pub fn close_local(&mut self) {
        self.state = match self.state {
            StreamState::Open => StreamState::HalfClosedLocal,
            StreamState::HalfClosedRemote => StreamState::Closed,
            s => s,
        };
    }

    /// Close the remote side.
    pub fn close_remote(&mut self) {
        self.state = match self.state {
            StreamState::Open => StreamState::HalfClosedRemote,
            StreamState::HalfClosedLocal => StreamState::Closed,
            s => s,
        };
    }

    /// Reset the stream.
    pub fn reset(&mut self) {
        self.state = StreamState::Reset;
        self.send_messages.clear();
        self.send_queue_bytes = 0;
    }

    /// Whether the stream can send data.
    pub fn can_send(&self) -> bool {
        matches!(
            self.state,
            StreamState::Open | StreamState::HalfClosedRemote
        )
    }

    /// Whether the stream is fully closed.
    pub fn is_closed(&self) -> bool {
        matches!(self.state, StreamState::Closed | StreamState::Reset)
    }

    /// Allocate a new message ID.
    pub fn allocate_message_id(&mut self) -> u64 {
        let id = self.next_message_id;
        self.next_message_id += 1;
        id
    }

    /// Register a new outgoing message.
    pub fn register_message(&mut self, msg: SendMessage) {
        self.send_queue_bytes += msg.total_size;
        self.send_messages.insert(msg.message_id, msg);
    }

    /// Mark a fragment as acked in a message.
    pub fn ack_fragment(&mut self, message_id: u64, fragment_num: u64) -> AtpResult<()> {
        let msg = self
            .send_messages
            .get_mut(&message_id)
            .ok_or_else(|| AtpError::message(format!("message {} not found", message_id)))?;
        msg.ack_fragment(fragment_num);
        if msg.complete {
            self.send_queue_bytes = self.send_queue_bytes.saturating_sub(msg.total_size);
        }
        Ok(())
    }

    /// Cancel a message.
    pub fn cancel_message(&mut self, message_id: u64) {
        if let Some(msg) = self.send_messages.get_mut(&message_id) {
            msg.cancel();
            self.send_queue_bytes = self.send_queue_bytes.saturating_sub(msg.total_size);
        }
        self.send_messages.remove(&message_id);
    }

    /// Remove completed messages.
    pub fn cleanup_completed(&mut self) {
        self.send_messages
            .retain(|_, m| !m.complete && !m.cancelled);
    }

    /// Get messages that have unacked fragments (need retransmission).
    pub fn messages_with_unacked(&self) -> Vec<(u64, Vec<u64>)> {
        self.send_messages
            .iter()
            .filter(|(_, m)| m.reliability.is_reliable() && !m.complete && !m.cancelled)
            .map(|(id, m)| (*id, m.unacked_fragments()))
            .filter(|(_, frags)| !frags.is_empty())
            .collect()
    }

    /// Whether we should send a MAX_STREAM_DATA frame to update the peer's send window.
    pub fn should_send_max_stream_data(&self) -> Option<u64> {
        self.flow.should_send_max_stream_data()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::message::ReliabilityMode;
    use super::*;

    #[test]
    fn test_stream_lifecycle() {
        let mut stream = AtpStream::new(1, 1_000_000, 1_000_000);
        assert_eq!(stream.state, StreamState::Idle);
        stream.open();
        assert_eq!(stream.state, StreamState::Open);
        assert!(stream.can_send());
        stream.close_local();
        assert_eq!(stream.state, StreamState::HalfClosedLocal);
        stream.close_remote();
        assert_eq!(stream.state, StreamState::Closed);
        assert!(stream.is_closed());
    }

    #[test]
    fn test_stream_message_management() {
        let mut stream = AtpStream::new(1, 1_000_000, 1_000_000);
        stream.open();

        let msg_id = stream.allocate_message_id();
        assert_eq!(msg_id, 1);

        let msg = SendMessage::new(msg_id, 1, 2, ReliabilityMode::Reliable, 200);
        stream.register_message(msg);

        stream.ack_fragment(msg_id, 0).unwrap();
        assert!(!stream.send_messages.get(&msg_id).unwrap().complete);
        stream.ack_fragment(msg_id, 1).unwrap();
        assert!(stream.send_messages.get(&msg_id).unwrap().complete);
    }

    #[test]
    fn test_stream_reset() {
        let mut stream = AtpStream::new(1, 1_000_000, 1_000_000);
        stream.open();
        let msg = SendMessage::new(1, 1, 1, ReliabilityMode::Reliable, 100);
        stream.register_message(msg);
        assert!(stream.send_queue_bytes > 0);
        stream.reset();
        assert_eq!(stream.state, StreamState::Reset);
        assert_eq!(stream.send_queue_bytes, 0);
    }
}
