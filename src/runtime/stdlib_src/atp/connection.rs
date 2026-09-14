//! ATP Connection Management

use super::config::AtpConfig;
use super::congestion::CongestionController;
use super::errors::{AtpError, AtpResult};
use super::flow::ConnectionFlowControl;
use super::handshake::{ClientHandshake, ServerHandshake};
use super::id::{ConnectionHandle, ConnectionId, WireConnectionId};
use super::memory::{ConnectionMemoryConfig, MemoryBudget};
use super::message::{OrderedDelivery, ReliabilityMode, SendMessage, fragment_payload};
use super::path::PathManager;
use super::reliability::{AckTracker, ReliabilityTracker, RetransmittableRef};
use super::scheduler::PacketScheduler;
use super::security::{PacketNumberSpace, SecurityContext};
use super::stream::{AtpStream, Priority};
use super::timer::{TimerKind, TimerWheel};
use super::wire::{DATA_FLAG_FIN, DATA_FLAG_FIRST, Frame};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Initial,
    Handshaking,
    Established,
    Migrating,
    Closing,
    Draining,
    Closed,
}

#[derive(Debug, Clone)]
pub struct TransportParams {
    pub max_streams: u64,
    pub max_data: u64,
    pub max_stream_data: u64,
    pub max_message_size: u64,
    pub idle_timeout: Duration,
    pub max_packet_size: u64,
}

impl Default for TransportParams {
    fn default() -> Self {
        let cfg = AtpConfig::default();
        TransportParams {
            max_streams: cfg.max_streams,
            max_data: cfg.max_connection_data,
            max_stream_data: cfg.max_stream_data,
            max_message_size: cfg.max_message_size,
            idle_timeout: cfg.idle_timeout,
            max_packet_size: cfg.max_packet_size as u64,
        }
    }
}

impl TransportParams {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        super::wire::push_varint(&mut out, self.max_streams);
        super::wire::push_varint(&mut out, self.max_data);
        super::wire::push_varint(&mut out, self.max_stream_data);
        super::wire::push_varint(&mut out, self.max_message_size);
        super::wire::push_varint(&mut out, self.max_packet_size);
        super::wire::push_varint(&mut out, self.idle_timeout.as_millis() as u64);
        out
    }

    pub fn decode(buf: &[u8]) -> AtpResult<Self> {
        let mut off = 0;
        macro_rules! read_v {
            () => {{
                let (v, l) = super::wire::decode_varint(buf, off)?;
                off += l;
                v
            }};
        }
        let params = TransportParams {
            max_streams: read_v!(),
            max_data: read_v!(),
            max_stream_data: read_v!(),
            max_message_size: read_v!(),
            max_packet_size: read_v!(),
            idle_timeout: Duration::from_millis(read_v!()),
        };
        let _ = off;
        Ok(params)
    }
}

pub struct AtpConnection {
    pub internal_id: ConnectionId,
    pub handle: ConnectionHandle,
    pub local_cid: WireConnectionId,
    pub remote_cid: Option<WireConnectionId>,
    pub state: ConnectionState,
    pub is_client: bool,
    pub peer_addr: SocketAddr,
    pub security: Option<SecurityContext>,
    pub client_handshake: Option<ClientHandshake>,
    pub server_handshake: Option<ServerHandshake>,
    pub streams: BTreeMap<u64, AtpStream>,
    next_stream_id: u64,
    pub handshake_reliability: ReliabilityTracker,
    pub app_reliability: ReliabilityTracker,
    pub ack_tracker: AckTracker,
    pub congestion: CongestionController,
    pub flow: ConnectionFlowControl,
    pub mem_config: ConnectionMemoryConfig,
    pub memory_budget: MemoryBudget,
    pub reassembly: BTreeMap<(u64, u64), super::message::MessageReassembly>,
    pub ordered_delivery: BTreeMap<u64, OrderedDelivery>,
    pub timers: TimerWheel,
    pub local_params: TransportParams,
    pub peer_params: TransportParams,
    pub last_activity: Instant,
    pub scheduler: PacketScheduler,
    pub paths: PathManager,
    pub config: AtpConfig,
    pub metrics: ConnectionMetrics,
    pub close_reason: Option<String>,
    /// Retry token received from server (client handshake).
    pub retry_token: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
}

impl AtpConnection {
    pub fn new(
        is_client: bool,
        peer_addr: SocketAddr,
        mem_config: ConnectionMemoryConfig,
        config: AtpConfig,
        now: Instant,
    ) -> Self {
        let internal_id = ConnectionId::new();
        let local_cid = WireConnectionId::random();
        let local_params = TransportParams::default();
        let peer_params = TransportParams::default();
        let mss = config.max_packet_size;

        let total_budget = mem_config.total_budget;
        AtpConnection {
            internal_id,
            handle: ConnectionHandle::new(internal_id),
            local_cid,
            remote_cid: None,
            state: ConnectionState::Initial,
            is_client,
            peer_addr,
            security: None,
            client_handshake: None,
            server_handshake: None,
            streams: BTreeMap::new(),
            next_stream_id: if is_client { 1 } else { 2 },
            handshake_reliability: ReliabilityTracker::new(),
            app_reliability: ReliabilityTracker::new(),
            ack_tracker: AckTracker::new(),
            congestion: CongestionController::new(mss, local_params.max_data as usize),
            flow: ConnectionFlowControl::new(local_params.max_data, local_params.max_data),
            mem_config,
            memory_budget: MemoryBudget::new(total_budget),
            reassembly: BTreeMap::new(),
            ordered_delivery: BTreeMap::new(),
            timers: TimerWheel::new(256, Duration::from_millis(10), now),
            local_params,
            peer_params,
            last_activity: now,
            scheduler: PacketScheduler::new(),
            paths: PathManager::new(peer_addr, now),
            config,
            metrics: ConnectionMetrics::default(),
            close_reason: None,
            retry_token: Vec::new(),
        }
    }

    /// Queue a frame for scheduled transmission.
    pub fn push_frame(&mut self, frame: Frame, stream_id: u64) {
        let priority = self
            .streams
            .get(&stream_id)
            .map(|s| s.priority)
            .unwrap_or(Priority::Medium);
        self.scheduler.enqueue(frame, priority, stream_id);
    }

    /// Queue a control frame (no stream association).
    pub fn push_control_frame(&mut self, frame: Frame) {
        self.scheduler.enqueue(frame, Priority::Critical, 0);
    }

    pub fn begin_migration(&mut self, new_addr: SocketAddr, now: Instant) -> AtpResult<()> {
        if !self.is_established() {
            return Err(AtpError::connection("cannot migrate: not established"));
        }
        let challenge = self.paths.begin_validation(new_addr, now)?;
        self.transition(ConnectionState::Migrating);
        self.push_control_frame(Frame::PathChallenge { data: challenge });
        Ok(())
    }

    pub fn send_addr(&self) -> SocketAddr {
        self.paths.active_addr()
    }

    pub fn transition(&mut self, new_state: ConnectionState) {
        self.state = new_state;
    }

    pub fn is_established(&self) -> bool {
        self.state == ConnectionState::Established
    }

    pub fn is_closed(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Closed | ConnectionState::Closing | ConnectionState::Draining
        )
    }

    pub fn open_stream(&mut self, priority: Priority) -> AtpResult<u64> {
        if !self.is_established() {
            return Err(AtpError::connection("cannot open stream: not established"));
        }
        if (self.streams.len() as u64) >= self.peer_params.max_streams {
            return Err(AtpError::stream("max streams exceeded"));
        }
        let stream_id = self.next_stream_id;
        self.next_stream_id += 2;

        let mut stream = AtpStream::new(
            stream_id,
            self.peer_params.max_stream_data,
            self.local_params.max_stream_data,
        );
        stream.priority = priority;
        stream.open();
        self.streams.insert(stream_id, stream);
        self.push_control_frame(Frame::StreamOpen { stream_id });
        Ok(stream_id)
    }

    pub fn close_stream(&mut self, stream_id: u64) -> AtpResult<()> {
        let stream = self
            .streams
            .get_mut(&stream_id)
            .ok_or_else(|| AtpError::stream(format!("stream {} not found", stream_id)))?;
        stream.close_local();
        self.push_control_frame(Frame::StreamClose { stream_id });
        Ok(())
    }

    pub fn send_message(
        &mut self,
        stream_id: u64,
        payload: &[u8],
        reliability: ReliabilityMode,
        now: Instant,
    ) -> AtpResult<u64> {
        if !self.is_established() {
            return Err(AtpError::connection("cannot send: not established"));
        }
        let stream = self
            .streams
            .get_mut(&stream_id)
            .ok_or_else(|| AtpError::stream(format!("stream {} not found", stream_id)))?;
        if !stream.can_send() {
            return Err(AtpError::stream("stream closed for sending"));
        }
        if payload.len() as u64 > self.peer_params.max_message_size {
            return Err(AtpError::message("message exceeds max size"));
        }
        if !self
            .congestion
            .can_send(self.app_reliability.bytes_in_flight)
        {
            return Err(AtpError::connection("congestion window full"));
        }

        self.flow.send.try_consume(payload.len() as u64)?;
        stream.flow.send.try_consume(payload.len() as u64)?;
        self.memory_budget.try_reserve(payload.len())?;

        let max_frag = self.config.max_fragment_payload();
        let (fragments, total_fragments) = fragment_payload(payload, max_frag);
        if total_fragments == 0 {
            return Err(AtpError::message("too many fragments"));
        }

        let message_id = stream.allocate_message_id();
        let flags_base = reliability.to_flags();
        let mut msg = SendMessage::new(
            message_id,
            stream_id,
            total_fragments,
            reliability,
            payload.len(),
        );

        let mut outbound_frames = Vec::new();
        for (i, frag_payload) in fragments.into_iter().enumerate() {
            let mut flags = flags_base;
            if i == 0 {
                flags |= DATA_FLAG_FIRST;
            }
            if i == total_fragments as usize - 1 {
                flags |= DATA_FLAG_FIN;
            }
            msg.store_fragment(i as u64, frag_payload.clone(), flags);
            outbound_frames.push(Frame::Data {
                stream_id,
                message_id,
                fragment_num: i as u64,
                total_fragments,
                flags,
                payload: frag_payload,
            });
        }
        if !reliability.is_reliable() {
            msg.complete = true;
        }
        stream.register_message(msg);
        for frame in outbound_frames {
            self.push_frame(frame, stream_id);
        }
        self.metrics.messages_sent += 1;
        self.last_activity = now;
        Ok(message_id)
    }

    pub fn send_datagram(&mut self, payload: Vec<u8>) -> AtpResult<()> {
        if !self.is_established() {
            return Err(AtpError::connection("cannot send: not established"));
        }
        if payload.len() > self.config.max_datagram_size {
            return Err(AtpError::message("datagram too large"));
        }
        self.memory_budget.try_reserve(payload.len())?;
        self.push_control_frame(Frame::Datagram { payload });
        Ok(())
    }

    pub fn cancel_message(&mut self, stream_id: u64, message_id: u64) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.cancel_message(message_id);
        }
        self.reassembly.remove(&(stream_id, message_id));
        self.timers.cancel_message(stream_id, message_id);
        self.push_control_frame(Frame::Cancel {
            stream_id,
            message_id,
        });
    }

    pub fn on_data_frame(
        &mut self,
        stream_id: u64,
        message_id: u64,
        fragment_num: u64,
        total_fragments: u64,
        flags: u8,
        payload: Vec<u8>,
        now: Instant,
    ) -> AtpResult<Vec<ReceivedData>> {
        if !self.is_established() {
            return Err(AtpError::protocol("data before established"));
        }
        self.last_activity = now;
        let reliability = ReliabilityMode::from_flags(flags);

        self.flow.recv.try_consume(payload.len() as u64)?;

        let stream = self.streams.entry(stream_id).or_insert_with(|| {
            let mut s = AtpStream::new(
                stream_id,
                self.peer_params.max_stream_data,
                self.local_params.max_stream_data,
            );
            s.open();
            s
        });
        stream.flow.recv.try_consume(payload.len() as u64)?;

        let key = (stream_id, message_id);
        let reassembly = self.reassembly.entry(key).or_insert_with(|| {
            super::message::MessageReassembly::new(
                message_id,
                stream_id,
                total_fragments,
                0,
                reliability,
                self.mem_config.max_reassembly_memory,
            )
        });

        let complete = reassembly.add_fragment(
            fragment_num,
            total_fragments,
            flags,
            payload,
            self.config.max_fragment_count,
            self.peer_params.max_message_size,
        )?;

        if complete {
            let data = reassembly.reassemble()?;
            self.reassembly.remove(&key);
            self.memory_budget.release(data.len());
            self.metrics.messages_received += 1;

            let od = self
                .ordered_delivery
                .entry(stream_id)
                .or_insert_with(OrderedDelivery::new);
            let deliveries = od.deliver(message_id, data, reliability.is_ordered());
            return Ok(deliveries
                .into_iter()
                .map(|(mid, d)| ReceivedData::Message {
                    stream_id,
                    message_id: mid,
                    data: d,
                })
                .collect());
        }
        Ok(vec![])
    }

    pub fn on_ack_frame(
        &mut self,
        largest_acked: u64,
        ack_delay: u64,
        ranges: &[(u64, u64)],
        now: Instant,
    ) {
        if let Ok((_, acked_bytes, acked_refs)) =
            self.app_reliability
                .process_ack(largest_acked, ack_delay, ranges, now)
        {
            if acked_bytes > 0 {
                self.congestion.on_acked(acked_bytes);
            }
            for rf in acked_refs {
                if let Some(stream) = self.streams.get_mut(&rf.stream_id) {
                    let _ = stream.ack_fragment(rf.message_id, rf.fragment_num);
                }
            }
        }
    }

    pub fn requeue_lost_fragments(&mut self, refs: Vec<RetransmittableRef>) {
        for rf in refs {
            if let Some(stream) = self.streams.get(&rf.stream_id) {
                if let Some(msg) = stream.send_messages.get(&rf.message_id) {
                    if msg.cancelled || msg.complete {
                        continue;
                    }
                    if let Some(payload) = msg.fragments.get(&rf.fragment_num) {
                        let mut flags = msg.flags;
                        if rf.fragment_num == 0 {
                            flags |= DATA_FLAG_FIRST;
                        }
                        if rf.fragment_num == msg.total_fragments - 1 {
                            flags |= DATA_FLAG_FIN;
                        }
                        self.push_frame(
                            Frame::Data {
                                stream_id: rf.stream_id,
                                message_id: rf.message_id,
                                fragment_num: rf.fragment_num,
                                total_fragments: msg.total_fragments,
                                flags,
                                payload: payload.clone(),
                            },
                            rf.stream_id,
                        );
                    }
                }
            }
        }
    }

    pub fn process_frames(
        &mut self,
        frames: Vec<Frame>,
        from_addr: SocketAddr,
        now: Instant,
    ) -> AtpResult<Vec<ReceivedData>> {
        let mut received = Vec::new();
        for frame in frames {
            match frame {
                Frame::Data {
                    stream_id,
                    message_id,
                    fragment_num,
                    total_fragments,
                    flags,
                    payload,
                } => {
                    received.extend(self.on_data_frame(
                        stream_id,
                        message_id,
                        fragment_num,
                        total_fragments,
                        flags,
                        payload,
                        now,
                    )?);
                }
                Frame::Datagram { payload } => {
                    received.push(ReceivedData::Datagram { data: payload });
                }
                Frame::Ack {
                    largest_acked,
                    ack_delay,
                    ranges,
                } => self.on_ack_frame(largest_acked, ack_delay, &ranges, now),
                Frame::StreamOpen { stream_id } => {
                    self.streams.entry(stream_id).or_insert_with(|| {
                        let mut s = AtpStream::new(
                            stream_id,
                            self.peer_params.max_stream_data,
                            self.local_params.max_stream_data,
                        );
                        s.open();
                        s
                    });
                }
                Frame::StreamClose { stream_id } => {
                    if let Some(s) = self.streams.get_mut(&stream_id) {
                        s.close_remote();
                    }
                }
                Frame::StreamReset {
                    stream_id,
                    reason: _,
                } => {
                    if let Some(s) = self.streams.get_mut(&stream_id) {
                        s.reset();
                    }
                }
                Frame::MaxData { max_data } => self.flow.send.update_max(max_data),
                Frame::MaxStreamData {
                    stream_id,
                    max_data,
                } => {
                    if let Some(s) = self.streams.get_mut(&stream_id) {
                        s.flow.send.update_max(max_data);
                    }
                }
                Frame::Ping => self.push_control_frame(Frame::Pong),
                Frame::Pong => {}
                Frame::ConnectionClose {
                    error_code: _,
                    reason,
                } => {
                    self.transition(ConnectionState::Closing);
                    self.close_reason = Some(reason);
                }
                Frame::KeyUpdate { epoch } => {
                    if let Some(sec) = &mut self.security {
                        if epoch > sec.epoch {
                            sec.rotate_keys();
                        }
                    }
                }
                Frame::Cancel {
                    stream_id,
                    message_id,
                } => {
                    if let Some(s) = self.streams.get_mut(&stream_id) {
                        s.cancel_message(message_id);
                    }
                    self.reassembly.remove(&(stream_id, message_id));
                }
                Frame::PathChallenge { data } => {
                    self.push_control_frame(Frame::PathResponse { data });
                }
                Frame::PathResponse { data } => {
                    if let Ok(Some(new_addr)) = self.paths.on_path_response(data, from_addr, now) {
                        self.peer_addr = new_addr;
                        self.paths.confirm_migration();
                        if self.state == ConnectionState::Migrating {
                            self.transition(ConnectionState::Established);
                        }
                    }
                }
                Frame::Retry { token } => {
                    self.retry_token = token;
                }
                Frame::ConnectionInit { .. }
                | Frame::ConnectionInitAck { .. }
                | Frame::Handshake { .. } => {}
            }
        }
        Ok(received)
    }

    pub fn drain_pending_frames(&mut self) -> Vec<Frame> {
        self.scheduler.drain(64)
    }

    pub fn check_timeouts(&mut self, now: Instant) -> Vec<RetransmittableRef> {
        let expired = self.timers.advance(now);
        let mut lost = Vec::new();

        for timer in expired {
            match timer.kind {
                TimerKind::Retransmission => {
                    lost.extend(self.app_reliability.detect_timeouts(now));
                }
                TimerKind::Idle => {
                    if now.duration_since(self.last_activity) > self.local_params.idle_timeout {
                        self.transition(ConnectionState::Closed);
                    }
                }
                TimerKind::Keepalive => {
                    self.push_control_frame(Frame::Ping);
                }
                TimerKind::Handshake => {
                    if self.state == ConnectionState::Handshaking {
                        self.transition(ConnectionState::Closed);
                    }
                }
                _ => {}
            }
        }
        lost
    }

    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    pub fn memory_used(&self) -> usize {
        self.memory_budget.used()
    }

    pub fn schedule_keepalive(&mut self, now: Instant) {
        let deadline = now + self.local_params.idle_timeout / 2;
        self.timers.schedule(deadline, TimerKind::Keepalive, 0, 0);
    }

    pub fn schedule_idle_check(&mut self, now: Instant) {
        let deadline = now + self.local_params.idle_timeout;
        self.timers.schedule(deadline, TimerKind::Idle, 0, 0);
    }

    pub fn schedule_handshake_timeout(&mut self, now: Instant) {
        let deadline = now + self.config.handshake_timeout;
        self.timers.schedule(deadline, TimerKind::Handshake, 0, 0);
    }

    pub fn reliability_for_space(&self, space: PacketNumberSpace) -> &ReliabilityTracker {
        match space {
            PacketNumberSpace::Handshake => &self.handshake_reliability,
            PacketNumberSpace::Application => &self.app_reliability,
        }
    }

    pub fn reliability_for_space_mut(
        &mut self,
        space: PacketNumberSpace,
    ) -> &mut ReliabilityTracker {
        match space {
            PacketNumberSpace::Handshake => &mut self.handshake_reliability,
            PacketNumberSpace::Application => &mut self.app_reliability,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReceivedData {
    Message {
        stream_id: u64,
        message_id: u64,
        data: Vec<u8>,
    },
    Datagram {
        data: Vec<u8>,
    },
}
