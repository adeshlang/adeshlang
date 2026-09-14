use super::super::headers::Headers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnidirectionalStreamType {
    Control = 0x00,
    Push = 0x01,
    QpackEncoder = 0x02,
    QpackDecoder = 0x03,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Http3StreamState {
    Open,
    HeadersSent,
    BodyStreaming,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Http3ErrorCode {
    NoError = 0x0100,
    GeneralProtocolError = 0x0101,
    InternalError = 0x0102,
    StreamCreationError = 0x0103,
    ClosedCriticalStream = 0x0104,
    FrameUnexpected = 0x0105,
    FrameError = 0x0106,
    ExcessiveLoad = 0x0107,
    IdError = 0x0108,
    SettingsError = 0x0109,
    MissingSettings = 0x010a,
    RequestRejected = 0x010b,
    RequestCancelled = 0x010c,
    RequestIncomplete = 0x010d,
    MessageError = 0x010e,
    ConnectError = 0x010f,
    VersionFallback = 0x0110,
}

impl Http3ErrorCode {
    pub fn code(&self) -> u64 {
        *self as u64
    }
}

#[derive(Debug, Clone)]
pub struct Http3Stream {
    pub stream_id: u64,
    pub state: Http3StreamState,
    pub incoming_headers: Vec<(String, String)>,
    pub incoming_data: Vec<u8>,
    pub trailers: Option<Headers>,
    pub is_finished: bool,
    pub error_code: Option<u64>,
}

impl Http3Stream {
    pub fn new(stream_id: u64) -> Self {
        Self {
            stream_id,
            state: Http3StreamState::Open,
            incoming_headers: Vec::new(),
            incoming_data: Vec::new(),
            trailers: None,
            is_finished: false,
            error_code: None,
        }
    }

    pub fn mark_headers_sent(&mut self) {
        if self.state == Http3StreamState::Open {
            self.state = Http3StreamState::HeadersSent;
        }
    }

    pub fn mark_body_streaming(&mut self) {
        if matches!(
            self.state,
            Http3StreamState::Open | Http3StreamState::HeadersSent
        ) {
            self.state = Http3StreamState::BodyStreaming;
        }
    }

    pub fn mark_completed(&mut self) {
        self.state = Http3StreamState::Completed;
        self.is_finished = true;
    }

    pub fn mark_cancelled(&mut self, err: Http3ErrorCode) {
        self.state = Http3StreamState::Cancelled;
        self.error_code = Some(err.code());
    }

    pub fn mark_failed(&mut self, err: Http3ErrorCode) {
        self.state = Http3StreamState::Failed;
        self.error_code = Some(err.code());
    }

    pub fn can_reset(&self) -> bool {
        !matches!(self.state, Http3StreamState::Completed)
    }
}
