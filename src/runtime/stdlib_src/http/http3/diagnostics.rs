use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub static TOTAL_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
pub static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
pub static TOTAL_STREAMS: AtomicU64 = AtomicU64::new(0);
pub static ACTIVE_STREAMS: AtomicU64 = AtomicU64::new(0);
pub static COMPLETED_STREAMS: AtomicU64 = AtomicU64::new(0);
pub static RESET_STREAMS: AtomicU64 = AtomicU64::new(0);
pub static CANCELLED_STREAMS: AtomicU64 = AtomicU64::new(0);
pub static CONNECTION_ERRORS: AtomicU64 = AtomicU64::new(0);
pub static STREAM_ERRORS: AtomicU64 = AtomicU64::new(0);
pub static BYTES_SENT: AtomicU64 = AtomicU64::new(0);
pub static BYTES_RECEIVED: AtomicU64 = AtomicU64::new(0);
pub static MAX_CONCURRENT_STREAMS: AtomicU64 = AtomicU64::new(0);

pub static HTTP3_DEBUG: AtomicBool = AtomicBool::new(false);

pub fn is_debug_enabled() -> bool {
    HTTP3_DEBUG.load(Ordering::Relaxed) || std::env::var_os("ADESH_HTTP3_DEBUG").is_some()
}

pub fn set_http3_debug(enable: bool) {
    HTTP3_DEBUG.store(enable, Ordering::SeqCst);
}

pub fn reset_http3_stats() {
    TOTAL_CONNECTIONS.store(0, Ordering::SeqCst);
    ACTIVE_CONNECTIONS.store(0, Ordering::SeqCst);
    TOTAL_STREAMS.store(0, Ordering::SeqCst);
    ACTIVE_STREAMS.store(0, Ordering::SeqCst);
    COMPLETED_STREAMS.store(0, Ordering::SeqCst);
    RESET_STREAMS.store(0, Ordering::SeqCst);
    CANCELLED_STREAMS.store(0, Ordering::SeqCst);
    CONNECTION_ERRORS.store(0, Ordering::SeqCst);
    STREAM_ERRORS.store(0, Ordering::SeqCst);
    BYTES_SENT.store(0, Ordering::SeqCst);
    BYTES_RECEIVED.store(0, Ordering::SeqCst);
    MAX_CONCURRENT_STREAMS.store(0, Ordering::SeqCst);
}

fn update_max_concurrent_streams(current_active: u64) {
    let mut prev = MAX_CONCURRENT_STREAMS.load(Ordering::Relaxed);
    while current_active > prev {
        match MAX_CONCURRENT_STREAMS.compare_exchange_weak(
            prev,
            current_active,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => prev = actual,
        }
    }
}

pub fn on_connection_created(conn_id: u64) {
    TOTAL_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
    ACTIVE_CONNECTIONS.fetch_add(1, Ordering::SeqCst);
    if is_debug_enabled() {
        println!("HTTP/3 Connection {} created", conn_id);
    }
}

pub fn on_connection_closed(conn_id: u64) {
    ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
    if is_debug_enabled() {
        println!("HTTP/3 Connection {} closed", conn_id);
    }
}

pub fn on_stream_opened(conn_id: u64, stream_id: u64) {
    TOTAL_STREAMS.fetch_add(1, Ordering::SeqCst);
    let active = ACTIVE_STREAMS.fetch_add(1, Ordering::SeqCst) + 1;
    update_max_concurrent_streams(active);
    if is_debug_enabled() {
        println!(
            "HTTP/3 Stream {} opened on Connection {}",
            stream_id, conn_id
        );
    }
}

pub fn on_stream_completed(stream_id: u64) {
    ACTIVE_STREAMS.fetch_sub(1, Ordering::SeqCst);
    COMPLETED_STREAMS.fetch_add(1, Ordering::SeqCst);
    if is_debug_enabled() {
        println!("HTTP/3 Stream {} completed", stream_id);
    }
}

pub fn on_stream_reset(stream_id: u64) {
    ACTIVE_STREAMS.fetch_sub(1, Ordering::SeqCst);
    RESET_STREAMS.fetch_add(1, Ordering::SeqCst);
    if is_debug_enabled() {
        println!("HTTP/3 Stream {} reset", stream_id);
    }
}

pub fn on_stream_cancelled(_stream_id: u64) {
    ACTIVE_STREAMS.fetch_sub(1, Ordering::SeqCst);
    CANCELLED_STREAMS.fetch_add(1, Ordering::SeqCst);
}

pub fn on_connection_error(_conn_id: u64, _msg: &str) {
    CONNECTION_ERRORS.fetch_add(1, Ordering::SeqCst);
}

pub fn on_stream_error(_stream_id: u64, _msg: &str) {
    STREAM_ERRORS.fetch_add(1, Ordering::SeqCst);
}

pub fn add_bytes_sent(bytes: usize) {
    BYTES_SENT.fetch_add(bytes as u64, Ordering::SeqCst);
}

pub fn add_bytes_received(bytes: usize) {
    BYTES_RECEIVED.fetch_add(bytes as u64, Ordering::SeqCst);
}

pub fn get_http3_stats_value() -> Value {
    let mut map = FastMap::default();
    let total_conn = TOTAL_CONNECTIONS.load(Ordering::SeqCst);
    let active_conn = ACTIVE_CONNECTIONS.load(Ordering::SeqCst);

    map.insert("connections".to_string(), Value::I64(total_conn as i64));
    map.insert(
        "activeConnections".to_string(),
        Value::I64(active_conn as i64),
    );
    map.insert(
        "totalConnections".to_string(),
        Value::I64(total_conn as i64),
    );
    map.insert(
        "totalStreams".to_string(),
        Value::I64(TOTAL_STREAMS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "activeStreams".to_string(),
        Value::I64(ACTIVE_STREAMS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "completedStreams".to_string(),
        Value::I64(COMPLETED_STREAMS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "resetStreams".to_string(),
        Value::I64(RESET_STREAMS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "cancelledStreams".to_string(),
        Value::I64(CANCELLED_STREAMS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "connectionErrors".to_string(),
        Value::I64(CONNECTION_ERRORS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "streamErrors".to_string(),
        Value::I64(STREAM_ERRORS.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "bytesSent".to_string(),
        Value::I64(BYTES_SENT.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "bytesReceived".to_string(),
        Value::I64(BYTES_RECEIVED.load(Ordering::SeqCst) as i64),
    );
    map.insert(
        "maxConcurrentStreams".to_string(),
        Value::I64(MAX_CONCURRENT_STREAMS.load(Ordering::SeqCst) as i64),
    );

    Value::Object(Arc::new(map))
}
