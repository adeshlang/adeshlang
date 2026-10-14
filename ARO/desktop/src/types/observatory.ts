export interface SessionSummary {
  session_id: string;
  pid: number;
  parent_pid?: number;
  tool_name: string;
  execution_mode: "interpreter" | "bytecode_vm" | "jit" | "tiered_jit" | "adaptive_jit" | "aot_native" | "web_assembly" | "linker" | "unknown";
  command_line: string;
  status: "running" | "completed" | "failed" | "terminated";
  exit_code?: number;
  start_time: string;
  end_time?: string;
  duration_ms?: number;
  peak_rss_bytes: number;
  cpu_peak_percent: number;
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
}

export interface CollectorStats {
  uptime_secs: number;
  total_events_ingested: number;
  total_events_dropped: number;
  active_sessions_count: number;
  stored_sessions_count: number;
  database_size_bytes: number;
}
