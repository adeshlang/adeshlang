export interface SessionSummary {
  session_id: string;
  pid: number;
  parent_pid?: number;
  tool_name: string;
  execution_mode: "interpreter" | "bytecode_vm" | "jit" | "tiered_jit" | "adaptive_jit" | "native_compiler" | "aot_native" | "web_assembly" | "linker" | "unknown";
  command_line: string;
  status: "running" | "completed" | "failed" | "terminated";
  exit_code?: number;
  start_time: string;
  end_time?: string;
  duration_ms?: number;
  peak_rss_bytes: number;
  cpu_peak_percent: number;
  /** Secrets masked in command_line before it was persisted */
  redaction_count?: number;
}

export interface CompilerStageEvent {
  stage_name: string;
  input_file?: string;
  duration_micros: number;
  success: boolean;
  error_message?: string;
  metadata: Record<string, string>;
}

export interface LinkerStageEvent {
  stage_name: string;
  target_triple: string;
  output_format: string;
  input_objects_count: number;
  symbol_count: number;
  section_count: number;
  output_size_bytes?: number;
  duration_micros: number;
  success: boolean;
  error_message?: string;
}

export interface SessionDetail {
  summary: SessionSummary;
  working_dir: string;
  target_file?: string;
  compiler_stages: CompilerStageEvent[];
  linker_stages: LinkerStageEvent[];
  metrics_count: number;
  runtime_events?: RuntimeEventPayload[];
  latest_metrics?: ProcessMetrics | null;
}

export interface ProcessMetrics {
  cpu_percent: number;
  memory_rss_bytes: number;
  memory_peak_rss_bytes: number;
  memory_vms_bytes: number;
  thread_count: number;
  peak_thread_count: number;
  disk_read_bytes: number;
  disk_write_bytes: number;
  cpu_user_ms: number;
  cpu_sys_ms: number;
  timestamp_utc: string;
  elapsed_micros: number;
}

export interface RuntimeEventPayload {
  event_name: string;
  component: string;
  duration_micros?: number | null;
  details: Record<string, string>;
}

export interface CollectorStats {
  uptime_secs: number;
  total_events_ingested: number;
  total_events_dropped: number;
  active_sessions_count: number;
  stored_sessions_count: number;
  database_size_bytes: number;
}

export interface RedactionRuleInfo {
  id: string;
  label: string;
  description: string;
  example: string;
  /** "critical" | "high" | "medium" */
  severity: string;
}

export interface AuditEntry {
  id: number;
  session_id: string;
  tool_name: string;
  command_line: string;
  rule_id: string;
  rule_label: string;
  masked_preview: string;
  redacted_at: string;
}

export interface SecurityReport {
  rules: RedactionRuleInfo[];
  findings_by_rule: Record<string, number>;
  redaction_counts: Record<string, number>;
  total_sessions_scanned: number;
  sessions_with_redactions: number;
  total_redactions: number;
  audit_entries: AuditEntry[];
  last_rescan?: string | null;
  rescan_count: number;
  db_path: string;
  ingest_redaction_enabled: boolean;
}
