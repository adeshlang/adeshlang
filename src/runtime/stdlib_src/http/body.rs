use super::errors::{HttpError, HttpErrorKind};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub type StreamChunk = Result<Vec<u8>, HttpError>;
pub type BodyStreamCallback = Arc<dyn Fn() -> Option<StreamChunk> + Send + Sync>;

#[derive(Clone)]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Stream(BodyStreamCallback),
    AsyncStream(AsyncBodyStream),
}

impl Body {
    pub fn empty() -> Self {
        Body::Empty
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Body::Bytes(bytes)
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Body::Bytes(s.into().into_bytes())
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, HttpError> {
        let serialized = serde_json::to_vec(json)
            .map_err(|e| HttpError::new(HttpErrorKind::ProtocolError, e.to_string()))?;
        Ok(Body::Bytes(serialized))
    }

    pub fn from_stream<F>(f: F) -> Self
    where
        F: Fn() -> Option<StreamChunk> + Send + Sync + 'static,
    {
        Body::Stream(Arc::new(f))
    }

    pub fn from_async_stream(stream: AsyncBodyStream) -> Self {
        Body::AsyncStream(stream)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Body::Empty => true,
            Body::Bytes(b) => b.is_empty(),
            Body::Stream(_) | Body::AsyncStream(_) => false,
        }
    }

    pub fn len(&self) -> Option<usize> {
        match self {
            Body::Empty => Some(0),
            Body::Bytes(b) => Some(b.len()),
            Body::Stream(_) | Body::AsyncStream(_) => None,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, HttpError> {
        match self {
            Body::Empty => Ok(Vec::new()),
            Body::Bytes(b) => Ok(b.clone()),
            Body::Stream(cb) => {
                let mut full = Vec::new();
                while let Some(chunk_res) = cb() {
                    let chunk = chunk_res?;
                    full.extend_from_slice(&chunk);
                }
                Ok(full)
            }
            Body::AsyncStream(s) => s.read_to_end(),
        }
    }

    pub fn to_text(&self) -> Result<String, HttpError> {
        let bytes = self.to_bytes()?;
        String::from_utf8(bytes).map_err(|e| {
            HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Failed to decode UTF-8 body: {}", e),
            )
        })
    }

    pub fn to_json(&self) -> Result<serde_json::Value, HttpError> {
        let bytes = self.to_bytes()?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HttpError::new(
                HttpErrorKind::ProtocolError,
                format!("Failed to parse JSON body: {}", e),
            )
        })
    }
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Empty => write!(f, "Body::Empty"),
            Body::Bytes(b) => write!(f, "Body::Bytes({} bytes)", b.len()),
            Body::Stream(_) => write!(f, "Body::Stream(<stream>)"),
            Body::AsyncStream(_) => write!(f, "Body::AsyncStream(<async_stream>)"),
        }
    }
}

/// Backpressure-aware, thread-safe Async Body Stream
#[derive(Clone)]
pub struct AsyncBodyStream {
    inner: Arc<AsyncBodyStreamInner>,
}

struct AsyncBodyStreamInner {
    queue: Mutex<VecDeque<Vec<u8>>>,
    read_cond: Condvar,
    write_cond: Condvar,
    max_bytes: usize,
    current_bytes: Mutex<usize>,
    eof: AtomicBool,
    cancelled: AtomicBool,
    error: Mutex<Option<HttpError>>,
    trailers: Mutex<Option<super::headers::Headers>>,
}

impl AsyncBodyStream {
    pub fn new(max_bytes_capacity: usize) -> Self {
        Self {
            inner: Arc::new(AsyncBodyStreamInner {
                queue: Mutex::new(VecDeque::new()),
                read_cond: Condvar::new(),
                write_cond: Condvar::new(),
                max_bytes: if max_bytes_capacity == 0 { 1024 * 1024 } else { max_bytes_capacity },
                current_bytes: Mutex::new(0),
                eof: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
                error: Mutex::new(None),
                trailers: Mutex::new(None),
            }),
        }
    }

    pub fn default_bounded() -> Self {
        Self::new(512 * 1024)
    }

    /// Push chunk into the stream with backpressure: blocks producer if capacity exceeded.
    pub fn push_chunk(&self, chunk: Vec<u8>) -> Result<(), HttpError> {
        if chunk.is_empty() {
            return Ok(());
        }
        if self.inner.cancelled.load(Ordering::SeqCst) {
            return Err(HttpError::new(HttpErrorKind::Cancelled, "Stream cancelled"));
        }

        let chunk_len = chunk.len();
        let mut cur = self.inner.current_bytes.lock().unwrap();

        // Enforce backpressure limit
        while *cur + chunk_len > self.inner.max_bytes {
            if self.inner.cancelled.load(Ordering::SeqCst) {
                return Err(HttpError::new(HttpErrorKind::Cancelled, "Stream cancelled"));
            }
            cur = self.inner.write_cond.wait_timeout(cur, Duration::from_millis(100)).unwrap().0;
        }

        *cur += chunk_len;
        let mut q = self.inner.queue.lock().unwrap();
        q.push_back(chunk);
        drop(q);
        drop(cur);

        self.inner.read_cond.notify_one();
        Ok(())
    }

    pub fn set_eof(&self) {
        self.inner.eof.store(true, Ordering::SeqCst);
        self.inner.read_cond.notify_all();
        self.inner.write_cond.notify_all();
    }

    pub fn set_error(&self, err: HttpError) {
        if let Ok(mut e) = self.inner.error.lock() {
            *e = Some(err);
        }
        self.inner.read_cond.notify_all();
        self.inner.write_cond.notify_all();
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.read_cond.notify_all();
        self.inner.write_cond.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_trailers(&self, headers: super::headers::Headers) {
        if let Ok(mut t) = self.inner.trailers.lock() {
            *t = Some(headers);
        }
    }

    pub fn get_trailers(&self) -> Option<super::headers::Headers> {
        self.inner.trailers.lock().ok()?.clone()
    }

    /// Read next chunk from the stream. Returns Ok(None) on EOF.
    pub fn read_chunk(&self) -> Result<Option<Vec<u8>>, HttpError> {
        let mut q = self.inner.queue.lock().unwrap();
        loop {
            if self.inner.cancelled.load(Ordering::SeqCst) {
                return Err(HttpError::new(HttpErrorKind::Cancelled, "Stream cancelled"));
            }
            if let Ok(e) = self.inner.error.lock() {
                if let Some(err) = e.clone() {
                    return Err(err);
                }
            }

            if let Some(chunk) = q.pop_front() {
                let len = chunk.len();
                let mut cur = self.inner.current_bytes.lock().unwrap();
                *cur = cur.saturating_sub(len);
                self.inner.write_cond.notify_one();
                return Ok(Some(chunk));
            }

            if self.inner.eof.load(Ordering::SeqCst) {
                return Ok(None);
            }

            q = self.inner.read_cond.wait_timeout(q, Duration::from_millis(100)).unwrap().0;
        }
    }

    pub fn read_to_end(&self) -> Result<Vec<u8>, HttpError> {
        let mut full = Vec::new();
        while let Some(chunk) = self.read_chunk()? {
            full.extend_from_slice(&chunk);
        }
        Ok(full)
    }
}

