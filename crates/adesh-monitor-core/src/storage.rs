//! SQLite storage engine for ARO

use adesh_monitor_protocol::*;
use rusqlite::{Connection, Result, params};
use std::path::Path;

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
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
                error_message TEXT
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
            "#,
        )?;
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
        let command_line = command_args.join(" ");
        let mode_str = serde_json::to_string(&execution_mode).unwrap_or_default();
        let mode_trimmed = mode_str.trim_matches('"');

        self.conn.execute(
            r#"
            INSERT INTO sessions (
                session_id, pid, parent_pid, tool_name, execution_mode,
                command_line, working_dir, target_file, status, start_time
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(session_id) DO UPDATE SET
                status = excluded.status
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
                start_time.to_rfc3339()
            ],
        )?;
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
                vms_bytes, thread_count, elapsed_micros, timestamp_utc
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                session_id,
                metric.cpu_percent,
                metric.memory_rss_bytes,
                metric.memory_peak_rss_bytes,
                metric.memory_vms_bytes,
                metric.thread_count,
                metric.elapsed_micros,
                metric.timestamp_utc.to_rfc3339()
            ],
        )?;

        // Update peak values in sessions table
        self.conn.execute(
            r#"
            UPDATE sessions SET
                peak_rss_bytes = MAX(peak_rss_bytes, ?1),
                cpu_peak_percent = MAX(cpu_peak_percent, ?2)
            WHERE session_id = ?3
            "#,
            params![metric.memory_rss_bytes, metric.cpu_percent, session_id],
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
                   duration_ms, peak_rss_bytes, cpu_peak_percent
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

            Ok(SessionSummary {
                session_id: row.get(0)?,
                pid: row.get(1)?,
                parent_pid: row.get(2)?,
                tool_name: row.get(3)?,
                execution_mode: mode,
                command_line: row.get(5)?,
                status,
                exit_code: row.get(7)?,
                start_time,
                end_time,
                duration_ms: row.get(10)?,
                peak_rss_bytes: row.get::<_, i64>(11)? as u64,
                cpu_peak_percent: row.get::<_, f64>(12)? as f32,
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
                   start_time, end_time, duration_ms, peak_rss_bytes, cpu_peak_percent
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

            let summary = SessionSummary {
                session_id: row.get(0)?,
                pid: row.get(1)?,
                parent_pid: row.get(2)?,
                tool_name: row.get(3)?,
                execution_mode: mode,
                command_line: row.get(5)?,
                status,
                exit_code: row.get(9)?,
                start_time,
                end_time,
                duration_ms: row.get(12)?,
                peak_rss_bytes: row.get::<_, i64>(13)? as u64,
                cpu_peak_percent: row.get::<_, f64>(14)? as f32,
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

            Ok(Some(SessionDetail {
                summary,
                working_dir,
                target_file,
                compiler_stages,
                linker_stages,
                runtime_events: Vec::new(),
                metrics_count: metrics_count as usize,
                latest_metrics: None,
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
                   thread_count, elapsed_micros, timestamp_utc
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
            DELETE FROM sessions;
            VACUUM;
            "#,
        )?;
        Ok(())
    }
}
