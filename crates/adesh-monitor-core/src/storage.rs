//! SQLite storage engine for ARO

use adesh_monitor_protocol::*;
use rusqlite::{Connection, Result, params};
use std::collections::HashMap;
use std::path::Path;

const AUDIT_META_LAST_RESCAN: &str = "last_rescan";
const AUDIT_META_RESCAN_COUNT: &str = "rescan_count";

/// Older CLI binaries can report their default interpreter backend for a build
/// command. The invocation itself is authoritative: builds/compiles use the
/// native compiler, ADOB codegen and the platform linker.
fn effective_execution_mode(mode: ExecutionMode, command_args: &[String]) -> ExecutionMode {
    let command = command_args.iter().map(String::as_str).collect::<Vec<_>>();
    if command
        .windows(2)
        .any(|pair| pair[0] == "build" && pair[1] == "run")
        || command
            .iter()
            .any(|arg| matches!(*arg, "build" | "compile" | "compile-aot" | "compile-native"))
    {
        return ExecutionMode::NativeCompiler;
    }
    mode
}

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn default_db_path() -> std::path::PathBuf {
        let base = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let adesh_dir = base.join(".adesh");
        let _ = std::fs::create_dir_all(&adesh_dir);
        adesh_dir.join("observatory.db")
    }

    pub fn default_store() -> Result<Self> {
        Self::open(Self::default_db_path())
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                pid INTEGER NOT NULL,
                parent_pid INTEGER,
                tool_name TEXT NOT NULL,
                execution_mode TEXT NOT NULL,
                command_line TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                target_file TEXT,
                status TEXT NOT NULL,
                exit_code INTEGER,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_ms INTEGER,
                peak_rss_bytes INTEGER DEFAULT 0,
                cpu_peak_percent REAL DEFAULT 0.0,
                error_message TEXT,
                redaction_count INTEGER NOT NULL DEFAULT 0,
                thread_count INTEGER NOT NULL DEFAULT 1,
                peak_threads INTEGER NOT NULL DEFAULT 1,
                cores_used REAL NOT NULL DEFAULT 1.0,
                memory_static_bytes INTEGER NOT NULL DEFAULT 0,
                memory_heap_bytes INTEGER NOT NULL DEFAULT 0,
                memory_dynamic_bytes INTEGER NOT NULL DEFAULT 0,
                memory_reused_bytes INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS redaction_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                rule_id TEXT NOT NULL,
                rule_label TEXT NOT NULL,
                masked_preview TEXT NOT NULL,
                redacted_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(session_id)
            );

            CREATE TABLE IF NOT EXISTS security_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                cpu_percent REAL NOT NULL,
                rss_bytes INTEGER NOT NULL,
                peak_rss_bytes INTEGER NOT NULL,
                vms_bytes INTEGER NOT NULL,
                thread_count INTEGER NOT NULL,
                elapsed_micros INTEGER NOT NULL,
                timestamp_utc TEXT NOT NULL,
                memory_static_bytes INTEGER NOT NULL DEFAULT 0,
                memory_heap_bytes INTEGER NOT NULL DEFAULT 0,
                memory_dynamic_bytes INTEGER NOT NULL DEFAULT 0,
                memory_reused_bytes INTEGER NOT NULL DEFAULT 0,
                cores_used REAL NOT NULL DEFAULT 1.0,
                FOREIGN KEY(session_id) REFERENCES sessions(session_id)
            );

            CREATE TABLE IF NOT EXISTS compiler_stages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                stage_name TEXT NOT NULL,
                input_file TEXT,
                duration_micros INTEGER NOT NULL,
                success INTEGER NOT NULL,
                error_message TEXT,
                metadata_json TEXT,
                FOREIGN KEY(session_id) REFERENCES sessions(session_id)
            );

            CREATE TABLE IF NOT EXISTS linker_stages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                stage_name TEXT NOT NULL,
                target_triple TEXT NOT NULL,
                output_format TEXT NOT NULL,
                input_objects_count INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL,
                section_count INTEGER NOT NULL,
                output_size_bytes INTEGER,
                duration_micros INTEGER NOT NULL,
                success INTEGER NOT NULL,
                error_message TEXT,
                FOREIGN KEY(session_id) REFERENCES sessions(session_id)
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time DESC);
            CREATE INDEX IF NOT EXISTS idx_metrics_session ON metrics(session_id);
            CREATE INDEX IF NOT EXISTS idx_compiler_stages_session ON compiler_stages(session_id);
            CREATE INDEX IF NOT EXISTS idx_linker_stages_session ON linker_stages(session_id);
            CREATE INDEX IF NOT EXISTS idx_redaction_events_session ON redaction_events(session_id);
            "#,
        )?;
        self.migrate_schema()?;
        Ok(())
    }

    /// Forward migrations for databases created before a column existed
    fn migrate_schema(&self) -> Result<()> {
        let has_column = |table: &str, name: &str| -> Result<bool> {
            let mut stmt = self
                .conn
                .prepare(&format!("PRAGMA table_info({})", table))?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(row) = rows.next()? {
                let col: String = row.get(1)?;
                if col == name {
                    found = true;
                    break;
                }
            }
            Ok(found)
        };

        if !has_column("sessions", "redaction_count")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN redaction_count INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("sessions", "thread_count")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN thread_count INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        if !has_column("sessions", "peak_threads")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN peak_threads INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        if !has_column("sessions", "cores_used")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN cores_used REAL NOT NULL DEFAULT 1.0",
                [],
            )?;
        }
        if !has_column("sessions", "memory_static_bytes")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN memory_static_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("sessions", "memory_heap_bytes")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN memory_heap_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("sessions", "memory_dynamic_bytes")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN memory_dynamic_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("sessions", "memory_reused_bytes")? {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN memory_reused_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        // Metrics table migrations
        if !has_column("metrics", "memory_static_bytes")? {
            self.conn.execute(
                "ALTER TABLE metrics ADD COLUMN memory_static_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("metrics", "memory_heap_bytes")? {
            self.conn.execute(
                "ALTER TABLE metrics ADD COLUMN memory_heap_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("metrics", "memory_dynamic_bytes")? {
            self.conn.execute(
                "ALTER TABLE metrics ADD COLUMN memory_dynamic_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("metrics", "memory_reused_bytes")? {
            self.conn.execute(
                "ALTER TABLE metrics ADD COLUMN memory_reused_bytes INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column("metrics", "cores_used")? {
            self.conn.execute(
                "ALTER TABLE metrics ADD COLUMN cores_used REAL NOT NULL DEFAULT 1.0",
                [],
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_session_start(
        &self,
        session_id: &str,
        pid: u32,
        parent_pid: Option<u32>,
        tool_name: &str,
        command_args: &[String],
        working_dir: &str,
        execution_mode: ExecutionMode,
        target_file: Option<&str>,
        start_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        // Mask secrets before anything touches disk
        let outcome = crate::redaction::redact_text(&command_args.join(" "));
        let command_line = outcome.text;
        let redaction_count = outcome.findings.len() as i64;
        let execution_mode = effective_execution_mode(execution_mode, command_args);
        let mode_str = serde_json::to_string(&execution_mode).unwrap_or_default();
        let mode_trimmed = mode_str.trim_matches('"');

        self.conn.execute(
            r#"
            INSERT INTO sessions (
                session_id, pid, parent_pid, tool_name, execution_mode,
                command_line, working_dir, target_file, status, start_time, redaction_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(session_id) DO UPDATE SET
                status = excluded.status,
                redaction_count = excluded.redaction_count
            "#,
            params![
                session_id,
                pid,
                parent_pid,
                tool_name,
                mode_trimmed,
                command_line,
                working_dir,
                target_file,
                "running",
                start_time.to_rfc3339(),
                redaction_count
            ],
        )?;

        self.conn.execute(
            "DELETE FROM redaction_events WHERE session_id = ?1",
            params![session_id],
        )?;
        if !outcome.findings.is_empty() {
            let mut stmt = self.conn.prepare(
                r#"
                INSERT INTO redaction_events (
                    session_id, rule_id, rule_label, masked_preview, redacted_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )?;
            for finding in &outcome.findings {
                stmt.execute(params![
                    session_id,
                    finding.rule_id,
                    finding.rule_label,
                    finding.masked_preview,
                    start_time.to_rfc3339()
                ])?;
            }
        }
        Ok(())
    }

    pub fn record_session_end(
        &self,
        session_id: &str,
        exit_code: i32,
        duration_micros: u64,
        status: SessionStatus,
        error_message: Option<&str>,
        end_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let status_str = match status {
            SessionStatus::Running => "running",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
            SessionStatus::Terminated => "terminated",
        };
        let duration_ms = duration_micros / 1000;

        self.conn.execute(
            r#"
            UPDATE sessions SET
                status = ?1,
                exit_code = ?2,
                duration_ms = ?3,
                end_time = ?4,
                error_message = ?5
            WHERE session_id = ?6
            "#,
            params![
                status_str,
                exit_code,
                duration_ms,
                end_time.to_rfc3339(),
                error_message,
                session_id
            ],
        )?;
        Ok(())
    }

    pub fn record_metric(&self, session_id: &str, metric: &ProcessMetrics) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO metrics (
                session_id, cpu_percent, rss_bytes, peak_rss_bytes,
                vms_bytes, thread_count, elapsed_micros, timestamp_utc,
                memory_static_bytes, memory_heap_bytes, memory_dynamic_bytes,
                memory_reused_bytes, cores_used
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                session_id,
                metric.cpu_percent,
                metric.memory_rss_bytes,
                metric.memory_peak_rss_bytes,
                metric.memory_vms_bytes,
                metric.thread_count,
                metric.elapsed_micros,
                metric.timestamp_utc.to_rfc3339(),
                metric.memory_static_bytes,
                metric.memory_heap_bytes,
                metric.memory_dynamic_bytes,
                metric.memory_reused_bytes,
                metric.cores_used
            ],
        )?;

        // Update peak values and live resource counts in sessions table
        let effective_peak_rss = metric.memory_peak_rss_bytes.max(metric.memory_rss_bytes);
        self.conn.execute(
            r#"
            UPDATE sessions SET
                peak_rss_bytes = MAX(peak_rss_bytes, ?1),
                cpu_peak_percent = MAX(cpu_peak_percent, ?2),
                thread_count = ?3,
                peak_threads = MAX(peak_threads, ?4),
                cores_used = ?5,
                memory_static_bytes = MAX(memory_static_bytes, ?6),
                memory_heap_bytes = MAX(memory_heap_bytes, ?7),
                memory_dynamic_bytes = MAX(memory_dynamic_bytes, ?8),
                memory_reused_bytes = MAX(memory_reused_bytes, ?9)
            WHERE session_id = ?10
            "#,
            params![
                effective_peak_rss,
                metric.cpu_percent,
                metric.thread_count,
                metric.peak_thread_count.max(metric.thread_count),
                metric.cores_used,
                metric.memory_static_bytes,
                metric.memory_heap_bytes,
                metric.memory_dynamic_bytes,
                metric.memory_reused_bytes,
                session_id
            ],
        )?;
        Ok(())
    }

    pub fn record_compiler_stage(
        &self,
        session_id: &str,
        stage: &CompilerStageEvent,
    ) -> Result<()> {
        let meta_json = serde_json::to_string(&stage.metadata).unwrap_or_else(|_| "{}".to_string());
        self.conn.execute(
            r#"
            INSERT INTO compiler_stages (
                session_id, stage_name, input_file, duration_micros, success, error_message, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                session_id,
                stage.stage_name,
                stage.input_file,
                stage.duration_micros,
                if stage.success { 1 } else { 0 },
                stage.error_message,
                meta_json
            ],
        )?;
        Ok(())
    }

    pub fn record_linker_stage(&self, session_id: &str, stage: &LinkerStageEvent) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO linker_stages (
                session_id, stage_name, target_triple, output_format,
                input_objects_count, symbol_count, section_count,
                output_size_bytes, duration_micros, success, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                session_id,
                stage.stage_name,
                stage.target_triple,
                stage.output_format,
                stage.input_objects_count as i64,
                stage.symbol_count as i64,
                stage.section_count as i64,
                stage.output_size_bytes.map(|b| b as i64),
                stage.duration_micros,
                if stage.success { 1 } else { 0 },
                stage.error_message
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT session_id, pid, parent_pid, tool_name, execution_mode,
                   command_line, status, exit_code, start_time, end_time,
                   duration_ms, peak_rss_bytes, cpu_peak_percent, redaction_count,
                   thread_count, peak_threads, cores_used,
                   memory_static_bytes, memory_heap_bytes, memory_dynamic_bytes, memory_reused_bytes
            FROM sessions
            ORDER BY start_time DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let mode_str: String = row.get(4)?;
            let mode = match mode_str.as_str() {
                "interpreter" => ExecutionMode::Interpreter,
                "bytecode_vm" => ExecutionMode::BytecodeVm,
                "jit" => ExecutionMode::Jit,
                "tiered_jit" => ExecutionMode::TieredJit,
                "adaptive_jit" => ExecutionMode::AdaptiveJit,
                "native_compiler" => ExecutionMode::NativeCompiler,
                "aot_native" => ExecutionMode::AotNative,
                "web_assembly" => ExecutionMode::WebAssembly,
                "linker" => ExecutionMode::Linker,
                _ => ExecutionMode::Unknown,
            };

            let status_str: String = row.get(6)?;
            let status = match status_str.as_str() {
                "running" => SessionStatus::Running,
                "completed" => SessionStatus::Completed,
                "failed" => SessionStatus::Failed,
                "terminated" => SessionStatus::Terminated,
                _ => SessionStatus::Running,
            };

            let start_time_str: String = row.get(8)?;
            let start_time = chrono::DateTime::parse_from_rfc3339(&start_time_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let end_time_str: Option<String> = row.get(9)?;
            let end_time = end_time_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok()
            });

            let command_line: String = row.get(5)?;
            let command_args = command_line
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mode = effective_execution_mode(mode, &command_args);
            Ok(SessionSummary {
                session_id: row.get(0)?,
                pid: row.get(1)?,
                parent_pid: row.get(2)?,
                tool_name: row.get(3)?,
                execution_mode: mode,
                command_line,
                status,
                exit_code: row.get(7)?,
                start_time,
                end_time,
                duration_ms: row.get(10)?,
                peak_rss_bytes: row.get::<_, i64>(11)? as u64,
                cpu_peak_percent: row.get::<_, f64>(12)? as f32,
                redaction_count: row.get::<_, i64>(13)?.max(0) as u32,
                thread_count: row.get::<_, i64>(14).unwrap_or(1).max(1) as u32,
                peak_threads: row.get::<_, i64>(15).unwrap_or(1).max(1) as u32,
                cores_used: row.get::<_, f64>(16).unwrap_or(1.0) as f32,
                memory_static_bytes: row.get::<_, i64>(17).unwrap_or(0).max(0) as u64,
                memory_heap_bytes: row.get::<_, i64>(18).unwrap_or(0).max(0) as u64,
                memory_dynamic_bytes: row.get::<_, i64>(19).unwrap_or(0).max(0) as u64,
                memory_reused_bytes: row.get::<_, i64>(20).unwrap_or(0).max(0) as u64,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_session_detail(&self, session_id: &str) -> Result<Option<SessionDetail>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT session_id, pid, parent_pid, tool_name, execution_mode,
                   command_line, working_dir, target_file, status, exit_code,
                   start_time, end_time, duration_ms, peak_rss_bytes, cpu_peak_percent,
                   redaction_count, thread_count, peak_threads, cores_used,
                   memory_static_bytes, memory_heap_bytes, memory_dynamic_bytes, memory_reused_bytes
            FROM sessions
            WHERE session_id = ?1
            "#,
        )?;

        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            let mode_str: String = row.get(4)?;
            let mode = match mode_str.as_str() {
                "interpreter" => ExecutionMode::Interpreter,
                "bytecode_vm" => ExecutionMode::BytecodeVm,
                "jit" => ExecutionMode::Jit,
                "tiered_jit" => ExecutionMode::TieredJit,
                "adaptive_jit" => ExecutionMode::AdaptiveJit,
                "native_compiler" => ExecutionMode::NativeCompiler,
                "aot_native" => ExecutionMode::AotNative,
                "web_assembly" => ExecutionMode::WebAssembly,
                "linker" => ExecutionMode::Linker,
                _ => ExecutionMode::Unknown,
            };

            let status_str: String = row.get(8)?;
            let status = match status_str.as_str() {
                "running" => SessionStatus::Running,
                "completed" => SessionStatus::Completed,
                "failed" => SessionStatus::Failed,
                "terminated" => SessionStatus::Terminated,
                _ => SessionStatus::Running,
            };

            let start_time_str: String = row.get(10)?;
            let start_time = chrono::DateTime::parse_from_rfc3339(&start_time_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let end_time_str: Option<String> = row.get(11)?;
            let end_time = end_time_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok()
            });

            let command_line: String = row.get(5)?;
            let command_args = command_line
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mode = effective_execution_mode(mode, &command_args);
            let summary = SessionSummary {
                session_id: row.get(0)?,
                pid: row.get(1)?,
                parent_pid: row.get(2)?,
                tool_name: row.get(3)?,
                execution_mode: mode,
                command_line,
                status,
                exit_code: row.get(9)?,
                start_time,
                end_time,
                duration_ms: row.get(12)?,
                peak_rss_bytes: row.get::<_, i64>(13)? as u64,
                cpu_peak_percent: row.get::<_, f64>(14)? as f32,
                redaction_count: row.get::<_, i64>(15)?.max(0) as u32,
                thread_count: row.get::<_, i64>(16).unwrap_or(1).max(1) as u32,
                peak_threads: row.get::<_, i64>(17).unwrap_or(1).max(1) as u32,
                cores_used: row.get::<_, f64>(18).unwrap_or(1.0) as f32,
                memory_static_bytes: row.get::<_, i64>(19).unwrap_or(0).max(0) as u64,
                memory_heap_bytes: row.get::<_, i64>(20).unwrap_or(0).max(0) as u64,
                memory_dynamic_bytes: row.get::<_, i64>(21).unwrap_or(0).max(0) as u64,
                memory_reused_bytes: row.get::<_, i64>(22).unwrap_or(0).max(0) as u64,
            };

            let working_dir: String = row.get(6)?;
            let target_file: Option<String> = row.get(7)?;

            let compiler_stages = self.get_compiler_stages(session_id)?;
            let linker_stages = self.get_linker_stages(session_id)?;

            let metrics_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM metrics WHERE session_id = ?1",
                params![session_id],
                |r| r.get(0),
            )?;

            let latest_metrics = self.get_metrics(session_id, 1)?.into_iter().last();

            Ok(Some(SessionDetail {
                summary,
                working_dir,
                target_file,
                compiler_stages,
                linker_stages,
                runtime_events: Vec::new(),
                metrics_count: metrics_count as usize,
                latest_metrics,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_compiler_stages(&self, session_id: &str) -> Result<Vec<CompilerStageEvent>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT stage_name, input_file, duration_micros, success, error_message, metadata_json
            FROM compiler_stages
            WHERE session_id = ?1
            ORDER BY id ASC
            "#,
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            let meta_str: String = row.get(5)?;
            let meta = serde_json::from_str(&meta_str).unwrap_or_default();
            Ok(CompilerStageEvent {
                stage_name: row.get(0)?,
                input_file: row.get(1)?,
                duration_micros: row.get(2)?,
                success: row.get::<_, i32>(3)? != 0,
                error_message: row.get(4)?,
                metadata: meta,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_linker_stages(&self, session_id: &str) -> Result<Vec<LinkerStageEvent>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT stage_name, target_triple, output_format, input_objects_count,
                   symbol_count, section_count, output_size_bytes, duration_micros,
                   success, error_message
            FROM linker_stages
            WHERE session_id = ?1
            ORDER BY id ASC
            "#,
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            Ok(LinkerStageEvent {
                stage_name: row.get(0)?,
                target_triple: row.get(1)?,
                output_format: row.get(2)?,
                input_objects_count: row.get::<_, i64>(3)? as usize,
                symbol_count: row.get::<_, i64>(4)? as usize,
                section_count: row.get::<_, i64>(5)? as usize,
                output_size_bytes: row.get::<_, Option<i64>>(6)?.map(|b| b as u64),
                duration_micros: row.get(7)?,
                success: row.get::<_, i32>(8)? != 0,
                error_message: row.get(9)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_metrics(&self, session_id: &str, limit: usize) -> Result<Vec<ProcessMetrics>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT cpu_percent, rss_bytes, peak_rss_bytes, vms_bytes,
                   thread_count, elapsed_micros, timestamp_utc,
                   memory_static_bytes, memory_heap_bytes, memory_dynamic_bytes,
                   memory_reused_bytes, cores_used
            FROM metrics
            WHERE session_id = ?1
            ORDER BY id ASC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            let ts_str: String = row.get(6)?;
            let ts = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(ProcessMetrics {
                cpu_percent: row.get::<_, f64>(0)? as f32,
                memory_rss_bytes: row.get::<_, i64>(1)? as u64,
                memory_peak_rss_bytes: row.get::<_, i64>(2)? as u64,
                memory_vms_bytes: row.get::<_, i64>(3)? as u64,
                thread_count: row.get::<_, i32>(4)? as u32,
                peak_thread_count: row.get::<_, i32>(4)? as u32,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                cpu_user_ms: 0,
                cpu_sys_ms: 0,
                timestamp_utc: ts,
                elapsed_micros: row.get(5)?,
                memory_static_bytes: row.get::<_, i64>(7).unwrap_or(0).max(0) as u64,
                memory_heap_bytes: row.get::<_, i64>(8).unwrap_or(0).max(0) as u64,
                memory_dynamic_bytes: row.get::<_, i64>(9).unwrap_or(0).max(0) as u64,
                memory_reused_bytes: row.get::<_, i64>(10).unwrap_or(0).max(0) as u64,
                cores_used: row.get::<_, f64>(11).unwrap_or(1.0) as f32,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn purge_history(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM metrics;
            DELETE FROM compiler_stages;
            DELETE FROM linker_stages;
            DELETE FROM redaction_events;
            DELETE FROM sessions;
            DELETE FROM security_meta;
            VACUUM;
            "#,
        )?;
        Ok(())
    }

    /// Aggregate the redaction posture of the whole audit database
    pub fn security_report(&self, audit_limit: usize) -> Result<SecurityReport> {
        let count_row = |sql: &str| -> Result<i64> { self.conn.query_row(sql, [], |r| r.get(0)) };

        let total_sessions_scanned = count_row("SELECT COUNT(*) FROM sessions")? as usize;
        let sessions_with_redactions =
            count_row("SELECT COUNT(*) FROM sessions WHERE redaction_count > 0")? as usize;
        let total_redactions = count_row("SELECT COUNT(*) FROM redaction_events")? as u64;

        let mut findings_by_rule = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT rule_id, COUNT(*) FROM redaction_events GROUP BY rule_id")?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
            })?;
            for row in rows {
                let (rule_id, count) = row?;
                findings_by_rule.insert(rule_id, count);
            }
        }

        let mut redaction_counts = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT session_id, redaction_count FROM sessions WHERE redaction_count > 0",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
            })?;
            for row in rows {
                let (session_id, count) = row?;
                redaction_counts.insert(session_id, count);
            }
        }

        let mut audit_entries = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT e.id, e.session_id, COALESCE(s.tool_name, ''), COALESCE(s.command_line, ''),
                       e.rule_id, e.rule_label, e.masked_preview, e.redacted_at
                FROM redaction_events e
                LEFT JOIN sessions s ON s.session_id = e.session_id
                ORDER BY e.id DESC
                LIMIT ?1
                "#,
            )?;
            let rows = stmt.query_map(params![audit_limit as i64], |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tool_name: row.get(2)?,
                    command_line: row.get(3)?,
                    rule_id: row.get(4)?,
                    rule_label: row.get(5)?,
                    masked_preview: row.get(6)?,
                    redacted_at: row.get(7)?,
                })
            })?;
            for row in rows {
                audit_entries.push(row?);
            }
        }

        let meta_value = |key: &str| -> Option<String> {
            self.conn
                .query_row(
                    "SELECT value FROM security_meta WHERE key = ?1",
                    params![key],
                    |r| r.get::<_, String>(0),
                )
                .ok()
        };

        Ok(SecurityReport {
            rules: crate::redaction::builtin_rules(),
            findings_by_rule,
            redaction_counts,
            total_sessions_scanned,
            sessions_with_redactions,
            total_redactions,
            audit_entries,
            last_rescan: meta_value(AUDIT_META_LAST_RESCAN),
            rescan_count: meta_value(AUDIT_META_RESCAN_COUNT)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            db_path: Self::default_db_path().display().to_string(),
            ingest_redaction_enabled: true,
        })
    }

    /// Re-run the redaction engine over every stored command line. This
    /// protects rows persisted by older builds that wrote secrets verbatim.
    /// Returns the number of sessions whose command line was updated.
    pub fn rescan_history(&self) -> Result<u64> {
        let stored = {
            let mut stmt = self
                .conn
                .prepare("SELECT session_id, command_line FROM sessions")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };

        let mut updated = 0u64;
        for (session_id, command_line) in stored {
            let outcome = crate::redaction::redact_text(&command_line);
            if outcome.text == command_line {
                continue;
            }
            let count = outcome.findings.len() as i64;
            self.conn.execute(
                "UPDATE sessions SET command_line = ?1, redaction_count = ?2 WHERE session_id = ?3",
                params![outcome.text, count, session_id],
            )?;
            self.conn.execute(
                "DELETE FROM redaction_events WHERE session_id = ?1",
                params![session_id],
            )?;
            if !outcome.findings.is_empty() {
                let mut stmt = self.conn.prepare(
                    r#"
                    INSERT INTO redaction_events (
                        session_id, rule_id, rule_label, masked_preview, redacted_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                )?;
                let now = chrono::Utc::now().to_rfc3339();
                for finding in &outcome.findings {
                    stmt.execute(params![
                        session_id,
                        finding.rule_id,
                        finding.rule_label,
                        finding.masked_preview,
                        now
                    ])?;
                }
            }
            updated += 1;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let rescan_count: u64 = self
            .conn
            .query_row(
                "SELECT value FROM security_meta WHERE key = ?1",
                params![AUDIT_META_RESCAN_COUNT],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            + 1;
        self.conn.execute(
            r#"
            INSERT INTO security_meta (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![AUDIT_META_LAST_RESCAN, now],
        )?;
        self.conn.execute(
            r#"
            INSERT INTO security_meta (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![AUDIT_META_RESCAN_COUNT, rescan_count.to_string()],
        )?;
        Ok(updated)
    }

    pub fn ingest_envelope(&self, envelope: &TelemetryEnvelope) -> Result<()> {
        match &envelope.payload {
            EventPayload::SessionStart {
                tool_name,
                command_args,
                working_dir,
                parent_pid,
                execution_mode,
                target_file,
            } => {
                self.record_session_start(
                    &envelope.session_id,
                    envelope.pid,
                    *parent_pid,
                    tool_name,
                    command_args,
                    working_dir,
                    *execution_mode,
                    target_file.as_deref(),
                    envelope.timestamp_utc,
                )?;
            }
            EventPayload::SessionEnd {
                exit_code,
                duration_micros,
                status,
                error_message,
            } => {
                self.record_session_end(
                    &envelope.session_id,
                    *exit_code,
                    *duration_micros,
                    *status,
                    error_message.as_deref(),
                    envelope.timestamp_utc,
                )?;
            }
            EventPayload::MetricsSample(metrics) => {
                self.record_metric(&envelope.session_id, metrics)?;
            }
            EventPayload::CompilerStage(stage) => {
                self.record_compiler_stage(&envelope.session_id, stage)?;
            }
            EventPayload::LinkerStage(stage) => {
                self.record_linker_stage(&envelope.session_id, stage)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn get_stats(&self) -> Result<CollectorStats> {
        let active_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE status = 'running'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let stored_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap_or(0);

        let stages_count: i64 = self
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM compiler_stages) + (SELECT COUNT(*) FROM linker_stages)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let metrics_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM metrics", [], |r| r.get(0))
            .unwrap_or(0);

        let db_path = Self::default_db_path();
        let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

        Ok(CollectorStats {
            uptime_secs: 0,
            total_events_ingested: (stored_count + stages_count + metrics_count) as u64,
            total_events_dropped: 0,
            active_sessions_count: active_count as usize,
            stored_sessions_count: stored_count as usize,
            database_size_bytes: db_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_at_ingest_and_builds_report() {
        let store = HistoryStore::in_memory().unwrap();
        store
            .record_session_start(
                "s1",
                1,
                None,
                "adesh",
                &[
                    "run".to_string(),
                    "--password=hunter2".to_string(),
                    "main.adesh".to_string(),
                ],
                "D:\\proj",
                ExecutionMode::Jit,
                Some("main.adesh"),
                chrono::Utc::now(),
            )
            .unwrap();

        let sessions = store.list_sessions(10).unwrap();
        assert_eq!(sessions[0].command_line, "run --password=*** main.adesh");
        assert_eq!(sessions[0].redaction_count, 1);

        let report = store.security_report(10).unwrap();
        assert_eq!(report.total_sessions_scanned, 1);
        assert_eq!(report.sessions_with_redactions, 1);
        assert_eq!(report.total_redactions, 1);
        assert_eq!(report.audit_entries.len(), 1);
        assert_eq!(report.audit_entries[0].rule_id, "cred_flags");
        assert_eq!(report.audit_entries[0].masked_preview, "--password=***");
        assert!(report.ingest_redaction_enabled);
        assert_eq!(report.rules.len(), crate::redaction::builtin_rules().len());
    }

    #[test]
    fn rescan_masks_legacy_raw_rows() {
        let store = HistoryStore::in_memory().unwrap();
        // Simulate a row written by an older build (raw secret at rest)
        store
            .conn
            .execute(
                "INSERT INTO sessions (
                    session_id, pid, tool_name, execution_mode, command_line,
                    working_dir, status, start_time
                 ) VALUES ('legacy', 42, 'adesh', 'jit', 'build --token=rawsecret', '.', 'completed', '2024-01-01T00:00:00Z')",
                [],
            )
            .unwrap();

        assert_eq!(store.rescan_history().unwrap(), 1);

        let sessions = store.list_sessions(10).unwrap();
        assert_eq!(sessions[0].command_line, "build --token=***");
        assert_eq!(sessions[0].redaction_count, 1);

        // Second rescan is a no-op (idempotent over masked text)
        assert_eq!(store.rescan_history().unwrap(), 0);

        let report = store.security_report(10).unwrap();
        assert_eq!(report.rescan_count, 2);
        assert!(report.last_rescan.is_some());
        assert_eq!(report.total_redactions, 1);
    }
}
