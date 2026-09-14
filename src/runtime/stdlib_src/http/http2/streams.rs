use super::super::headers::Headers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Http2StreamState {
    Idle,
    ReservedLocal,
    ReservedRemote,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

#[derive(Debug, Clone)]
pub struct Http2Stream {
    pub stream_id: u32,
    pub state: Http2StreamState,
    pub send_window: i32,
    pub recv_window: i32,
    pub incoming_headers: Vec<(String, String)>,
    pub incoming_data: Vec<u8>,
    pub end_stream_received: bool,
    pub trailers: Option<Headers>,
}

impl Http2Stream {
    pub fn new(stream_id: u32, initial_window: u32) -> Self {
        Self {
            stream_id,
            state: Http2StreamState::Idle,
            send_window: initial_window as i32,
            recv_window: initial_window as i32,
            incoming_headers: Vec::new(),
            incoming_data: Vec::new(),
            end_stream_received: false,
            trailers: None,
        }
    }
}
