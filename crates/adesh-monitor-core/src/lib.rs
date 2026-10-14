//! Ingest and Collector Service

pub mod ipc;
pub mod storage;

use adesh_monitor_platform::create_platform_provider;
use adesh_monitor_protocol::*;
use crossbeam_channel::{Receiver, Sender, bounded};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use storage::HistoryStore;

pub struct CollectorConfig {
    pub db_path: Option<PathBuf>,
    pub max_queue_capacity: usize,
    pub sampling_interval_ms: u64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        let default_db = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(|h| PathBuf::from(h).join(".adesh").join("observatory.db"))
            .unwrap_or_else(|_| PathBuf::from("observatory.db"));

        Self {
            db_path: Some(default_db),
            max_queue_capacity: 10_000,
            sampling_interval_ms: 250,
        }
    }
}

pub struct ActiveSessionState {
    pub session_id: String,
    pub pid: u32,
    pub tool_name: String,
    pub start_instant: Instant,
    pub latest_metrics: Option<ProcessMetrics>,
}

pub struct Collector {
    config: CollectorConfig,
    running: Arc<AtomicBool>,
    sender: Sender<TelemetryEnvelope>,
    receiver: Receiver<TelemetryEnvelope>,
    store: Arc<Mutex<HistoryStore>>,
    active_sessions: Arc<Mutex<HashMap<String, ActiveSessionState>>>,
    events_ingested: Arc<AtomicU64>,
    events_dropped: Arc<AtomicU64>,
    start_time: Instant,
}

impl Collector {
    pub fn new(config: CollectorConfig) -> Result<Self, String> {
        let store = if let Some(ref path) = config.db_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            HistoryStore::open(path).map_err(|e| format!("Failed to open DB: {}", e))?
        } else {
            HistoryStore::in_memory().map_err(|e| format!("Failed to create memory DB: {}", e))?
        };

        let (sender, receiver) = bounded(config.max_queue_capacity);

        Ok(Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
            store: Arc::new(Mutex::new(store)),
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            events_ingested: Arc::new(AtomicU64::new(0)),
            events_dropped: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        })
    }

    pub fn sender(&self) -> Sender<TelemetryEnvelope> {
        self.sender.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn store(&self) -> Arc<Mutex<HistoryStore>> {
        self.store.clone()
    }

    pub fn stats(&self) -> CollectorStats {
        let active_count = self.active_sessions.lock().map(|s| s.len()).unwrap_or(0);
        let stored_count = match self.store.lock() {
            Ok(s) => s.list_sessions(1_000_000).map(|l| l.len()).unwrap_or(0),
            Err(_) => 0,
        };

        let db_size = self
            .config
            .db_path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        CollectorStats {
            uptime_secs: self.start_time.elapsed().as_secs(),
            total_events_ingested: self.events_ingested.load(Ordering::Relaxed),
            total_events_dropped: self.events_dropped.load(Ordering::Relaxed),
            active_sessions_count: active_count,
            stored_sessions_count: stored_count,
            database_size_bytes: db_size,
        }
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let running = self.running.clone();
        let receiver = self.receiver.clone();
        let store = self.store.clone();
        let active_sessions = self.active_sessions.clone();
        let events_ingested = self.events_ingested.clone();

        // Background worker: Ingest queue & persist to SQLite
        thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(envelope) => {
                        events_ingested.fetch_add(1, Ordering::Relaxed);
                        let store_lock = store.lock();
                        if let Ok(st) = store_lock {
                            match envelope.payload {
                                EventPayload::SessionStart {
                                    tool_name,
                                    command_args,
                                    working_dir,
                                    parent_pid,
                                    execution_mode,
                                    target_file,
                                } => {
                                    if let Ok(mut act) = active_sessions.lock() {
                                        act.insert(
                                            envelope.session_id.clone(),
                                            ActiveSessionState {
                                                session_id: envelope.session_id.clone(),
                                                pid: envelope.pid,
                                                tool_name: tool_name.clone(),
                                                start_instant: Instant::now(),
                                                latest_metrics: None,
                                            },
                                        );
                                    }
                                    let _ = st.record_session_start(
                                        &envelope.session_id,
                                        envelope.pid,
                                        parent_pid,
                                        &tool_name,
                                        &command_args,
                                        &working_dir,
                                        execution_mode,
                                        target_file.as_deref(),
                                        envelope.timestamp_utc,
                                    );
                                }
                                EventPayload::SessionEnd {
                                    exit_code,
                                    duration_micros,
                                    status,
                                    error_message,
                                } => {
                                    if let Ok(mut act) = active_sessions.lock() {
                                        act.remove(&envelope.session_id);
                                    }
                                    let _ = st.record_session_end(
                                        &envelope.session_id,
                                        exit_code,
                                        duration_micros,
                                        status,
                                        error_message.as_deref(),
                                        envelope.timestamp_utc,
                                    );
                                }
                                EventPayload::MetricsSample(ref metrics) => {
                                    if let Ok(mut act) = active_sessions.lock()
                                        && let Some(session) = act.get_mut(&envelope.session_id)
                                    {
                                        session.latest_metrics = Some(metrics.clone());
                                    }
                                    let _ = st.record_metric(&envelope.session_id, metrics);
                                }
                                EventPayload::CompilerStage(ref stage) => {
                                    let _ = st.record_compiler_stage(&envelope.session_id, stage);
                                }
                                EventPayload::LinkerStage(ref stage) => {
                                    let _ = st.record_linker_stage(&envelope.session_id, stage);
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        // Background worker: Resource sampler for active sessions
        let running_sampler = self.running.clone();
        let active_for_sampler = self.active_sessions.clone();
        let sender_sampler = self.sender.clone();
        let interval = Duration::from_millis(self.config.sampling_interval_ms);

        thread::spawn(move || {
            let mut provider = create_platform_provider();
            while running_sampler.load(Ordering::SeqCst) {
                thread::sleep(interval);
                let sessions: Vec<(String, u32, u64)> = if let Ok(act) = active_for_sampler.lock() {
                    act.values()
                        .map(|s| {
                            (
                                s.session_id.clone(),
                                s.pid,
                                s.start_instant.elapsed().as_micros() as u64,
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                for (session_id, pid, elapsed) in sessions {
                    if let Some(metrics) = provider.sample_process(pid, elapsed) {
                        let envelope = TelemetryEnvelope::new(
                            session_id,
                            pid,
                            elapsed,
                            EventPayload::MetricsSample(metrics),
                        );
                        let _ = sender_sampler.try_send(envelope);
                    }
                }
            }
        });
    }

    pub fn handle_request(&self, req: CollectorRequest) -> CollectorResponse {
        match req {
            CollectorRequest::Ping => CollectorResponse::Pong {
                version: PROTOCOL_VERSION.to_string(),
                uptime_secs: self.start_time.elapsed().as_secs(),
            },
            CollectorRequest::ListSessions { limit } => match self.store.lock() {
                Ok(s) => match s.list_sessions(limit) {
                    Ok(sessions) => CollectorResponse::SessionsList { sessions },
                    Err(e) => CollectorResponse::Error {
                        message: e.to_string(),
                    },
                },
                Err(e) => CollectorResponse::Error {
                    message: e.to_string(),
                },
            },
            CollectorRequest::GetSessionDetail { session_id } => match self.store.lock() {
                Ok(s) => match s.get_session_detail(&session_id) {
                    Ok(Some(session)) => CollectorResponse::SessionDetail {
                        session: Box::new(session),
                    },
                    Ok(None) => CollectorResponse::Error {
                        message: format!("Session {} not found", session_id),
                    },
                    Err(e) => CollectorResponse::Error {
                        message: e.to_string(),
                    },
                },
                Err(e) => CollectorResponse::Error {
                    message: e.to_string(),
                },
            },
            CollectorRequest::GetSessionMetrics { session_id, limit } => match self.store.lock() {
                Ok(s) => match s.get_metrics(&session_id, limit) {
                    Ok(metrics) => CollectorResponse::SessionMetrics {
                        session_id,
                        metrics,
                    },
                    Err(e) => CollectorResponse::Error {
                        message: e.to_string(),
                    },
                },
                Err(e) => CollectorResponse::Error {
                    message: e.to_string(),
                },
            },
            CollectorRequest::GetSessionStages { session_id } => {
                let store = match self.store.lock() {
                    Ok(s) => s,
                    Err(e) => {
                        return CollectorResponse::Error {
                            message: e.to_string(),
                        };
                    }
                };
                let c_stages = store.get_compiler_stages(&session_id).unwrap_or_default();
                let l_stages = store.get_linker_stages(&session_id).unwrap_or_default();
                CollectorResponse::SessionStages {
                    session_id,
                    compiler_stages: c_stages,
                    linker_stages: l_stages,
                }
            }
            CollectorRequest::GetCollectorStats => CollectorResponse::Stats(self.stats()),
            CollectorRequest::PurgeHistory => match self.store.lock() {
                Ok(s) => match s.purge_history() {
                    Ok(()) => CollectorResponse::Ok,
                    Err(e) => CollectorResponse::Error {
                        message: e.to_string(),
                    },
                },
                Err(e) => CollectorResponse::Error {
                    message: e.to_string(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_pipeline() {
        let config = CollectorConfig {
            db_path: None,
            max_queue_capacity: 100,
            sampling_interval_ms: 100,
        };
        let collector = Collector::new(config).expect("collector created");
        collector.start();

        let sender = collector.sender();
        let env1 = TelemetryEnvelope::new(
            "sess-test".to_string(),
            std::process::id(),
            0,
            EventPayload::SessionStart {
                tool_name: "adesh".to_string(),
                command_args: vec!["run".to_string()],
                working_dir: ".".to_string(),
                parent_pid: None,
                execution_mode: ExecutionMode::Jit,
                target_file: Some("main.adesh".to_string()),
            },
        );
        sender.send(env1).unwrap();

        let stage = CompilerStageEvent {
            stage_name: "parse".to_string(),
            input_file: Some("main.adesh".to_string()),
            duration_micros: 2500,
            success: true,
            error_message: None,
            metadata: HashMap::new(),
        };
        let env2 = TelemetryEnvelope::new(
            "sess-test".to_string(),
            std::process::id(),
            2500,
            EventPayload::CompilerStage(stage),
        );
        sender.send(env2).unwrap();

        let env3 = TelemetryEnvelope::new(
            "sess-test".to_string(),
            std::process::id(),
            10000,
            EventPayload::SessionEnd {
                exit_code: 0,
                duration_micros: 10000,
                status: SessionStatus::Completed,
                error_message: None,
            },
        );
        sender.send(env3).unwrap();

        // Give worker thread a moment to process queue
        thread::sleep(Duration::from_millis(300));

        let res = collector.handle_request(CollectorRequest::ListSessions { limit: 10 });
        if let CollectorResponse::SessionsList { sessions } = res {
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].session_id, "sess-test");
            assert_eq!(sessions[0].status, SessionStatus::Completed);
        } else {
            panic!("Expected SessionsList response");
        }

        let res2 = collector.handle_request(CollectorRequest::GetSessionDetail {
            session_id: "sess-test".to_string(),
        });
        if let CollectorResponse::SessionDetail { session } = res2 {
            assert_eq!(session.compiler_stages.len(), 1);
            assert_eq!(session.compiler_stages[0].stage_name, "parse");
        } else {
            panic!("Expected SessionDetail response");
        }

        collector.stop();
    }
}
