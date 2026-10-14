import React, { createContext, useContext, useReducer, useEffect, ReactNode } from "react";
import { SessionSummary, CollectorStats, SessionDetail, CompilerStageEvent, LinkerStageEvent } from "../types/observatory";

export type NavTab =
  | "overview"
  | "process_explorer"
  | "stage_timeline"
  | "flame_graph"
  | "linker_analytics"
  | "benchmarks"
  | "hardware_caps"
  | "audit_logs"
  | "settings";

export interface HardwareCaps {
  os_name: string;
  arch: string;
  logical_cores: number;
  supports_process_cpu: boolean;
  supports_process_memory: boolean;
  supports_disk_io: boolean;
  supports_accelerators: boolean;
}

export interface ObservatoryState {
  currentTab: NavTab;
  sessions: SessionSummary[];
  selectedSessionId: string | null;
  selectedDetail?: SessionDetail;
  stats: CollectorStats;
  hardware: HardwareCaps;
  searchQuery: string;
  filterMode: string;
  ipcStatus: "connected" | "disconnected" | "connecting";
  autoRefresh: boolean;
  refreshIntervalMs: number;
  lastUpdated: Date;
}

type ObservatoryAction =
  | { type: "SET_TAB"; payload: NavTab }
  | { type: "SET_SESSIONS"; payload: SessionSummary[] }
  | { type: "SELECT_SESSION"; payload: string }
  | { type: "SET_SESSION_DETAIL"; payload: SessionDetail }
  | { type: "SET_STATS"; payload: CollectorStats }
  | { type: "SET_HARDWARE"; payload: HardwareCaps }
  | { type: "SET_SEARCH_QUERY"; payload: string }
  | { type: "SET_FILTER_MODE"; payload: string }
  | { type: "SET_IPC_STATUS"; payload: "connected" | "disconnected" | "connecting" }
  | { type: "TOGGLE_AUTO_REFRESH" }
  | { type: "SET_REFRESH_INTERVAL"; payload: number }
  | { type: "CLEAR_SELECTION" };

const initialState: ObservatoryState = {
  currentTab: "overview",
  sessions: [],
  selectedSessionId: null,
  stats: {
    uptime_secs: 0,
    total_events_ingested: 0,
    total_events_dropped: 0,
    active_sessions_count: 0,
    stored_sessions_count: 0,
    database_size_bytes: 0,
  },
  hardware: {
    os_name: "Detecting...",
    arch: "...",
    logical_cores: 0,
    supports_process_cpu: true,
    supports_process_memory: true,
    supports_disk_io: true,
    supports_accelerators: false,
  },
  searchQuery: "",
  filterMode: "all",
  ipcStatus: "connecting",
  autoRefresh: true,
  refreshIntervalMs: 1000,
  lastUpdated: new Date(),
};

function observatoryReducer(state: ObservatoryState, action: ObservatoryAction): ObservatoryState {
  switch (action.type) {
    case "SET_TAB":
      return { ...state, currentTab: action.payload };
    case "SET_SESSIONS":
      return { ...state, sessions: action.payload, lastUpdated: new Date() };
    case "SELECT_SESSION":
      return { ...state, selectedSessionId: action.payload, currentTab: "stage_timeline" };
    case "SET_SESSION_DETAIL":
      return { ...state, selectedDetail: action.payload };
    case "SET_STATS":
      return { ...state, stats: action.payload };
    case "SET_HARDWARE":
      return { ...state, hardware: action.payload };
    case "SET_SEARCH_QUERY":
      return { ...state, searchQuery: action.payload };
    case "SET_FILTER_MODE":
      return { ...state, filterMode: action.payload };
    case "SET_IPC_STATUS":
      return { ...state, ipcStatus: action.payload };
    case "TOGGLE_AUTO_REFRESH":
      return { ...state, autoRefresh: !state.autoRefresh };
    case "SET_REFRESH_INTERVAL":
      return { ...state, refreshIntervalMs: action.payload };
    case "CLEAR_SELECTION":
      return { ...state, selectedSessionId: null, selectedDetail: undefined };
    default:
      return state;
  }
}

interface ObservatoryContextType {
  state: ObservatoryState;
  dispatch: React.Dispatch<ObservatoryAction>;
  refreshNow: () => Promise<void>;
  selectSessionById: (sessionId: string) => Promise<void>;
}

const ObservatoryContext = createContext<ObservatoryContextType | undefined>(undefined);

export const ObservatoryProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [state, dispatch] = useReducer(observatoryReducer, initialState);

  const refreshNow = async () => {
    try {
      if ((window as any).__TAURI_INTERNALS__) {
        const { invoke } = await import("@tauri-apps/api/core");
        let sessions: SessionSummary[] = [];
        let stats: CollectorStats | null = null;
        try {
          sessions = await invoke<SessionSummary[]>("get_sessions", { limit: 200 });
        } catch (e) {
          console.warn("get_sessions error, falling back:", e);
        }
        try {
          stats = await invoke<CollectorStats>("get_collector_stats");
        } catch (e) {
          console.warn("get_collector_stats error, falling back:", e);
        }
        if (sessions && sessions.length > 0) {
          dispatch({ type: "SET_SESSIONS", payload: sessions });
        }
        if (stats) {
          dispatch({ type: "SET_STATS", payload: stats });
        }
        dispatch({ type: "SET_IPC_STATUS", payload: "connected" });
      } else {
        // High-fidelity development fixtures when inspected outside Tauri window
        const demoSessions: SessionSummary[] = [
          {
            session_id: "sess-8a3b12f4-e7c9-4b21-9d55-1029384756ab",
            pid: 14220,
            tool_name: "adesh",
            execution_mode: "jit",
            command_line: "adesh run tests/main.adesh --njit",
            status: "completed",
            exit_code: 0,
            start_time: new Date(Date.now() - 45000).toISOString(),
            end_time: new Date(Date.now() - 44968).toISOString(),
            duration_ms: 32,
            peak_rss_bytes: 38400000,
            cpu_peak_percent: 18.5,
          },
          {
            session_id: "sess-194fa83c-1120-4df2-bc51-0987654321fe",
            pid: 29810,
            tool_name: "adeshlink",
            execution_mode: "linker",
            command_line: "adeshlink -o dist/app.exe main.adob --hardened",
            status: "completed",
            exit_code: 0,
            start_time: new Date(Date.now() - 15000).toISOString(),
            end_time: new Date(Date.now() - 14986).toISOString(),
            duration_ms: 14,
            peak_rss_bytes: 24500000,
            cpu_peak_percent: 25.0,
          },
          {
            session_id: "sess-5510abcd-3344-9876-1122-334455667788",
            pid: 31044,
            tool_name: "adesh",
            execution_mode: "aot_native",
            command_line: "adesh compile-native app.adesh app.exe",
            status: "running",
            start_time: new Date(Date.now() - 1200).toISOString(),
            peak_rss_bytes: 67200000,
            cpu_peak_percent: 88.4,
          },
        ];

        dispatch({ type: "SET_SESSIONS", payload: demoSessions });
        dispatch({
          type: "SET_STATS",
          payload: {
            uptime_secs: 420,
            total_events_ingested: 3892,
            total_events_dropped: 0,
            active_sessions_count: 1,
            stored_sessions_count: demoSessions.length,
            database_size_bytes: 2457600,
          },
        });
        dispatch({
          type: "SET_HARDWARE",
          payload: {
            os_name: "Windows / Linux / macOS Unified Core",
            arch: "x86_64 / aarch64",
            logical_cores: 16,
            supports_process_cpu: true,
            supports_process_memory: true,
            supports_disk_io: true,
            supports_accelerators: true,
          },
        });
        dispatch({ type: "SET_IPC_STATUS", payload: "connected" });
      }
    } catch (err) {
      dispatch({ type: "SET_IPC_STATUS", payload: "disconnected" });
    }
  };

  const selectSessionById = async (sessionId: string) => {
    dispatch({ type: "SELECT_SESSION", payload: sessionId });
    try {
      if ((window as any).__TAURI_INTERNALS__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const detail = await invoke<SessionDetail>("get_session_detail", { sessionId });
        dispatch({ type: "SET_SESSION_DETAIL", payload: detail });
      } else {
        const summary = state.sessions.find((s) => s.session_id === sessionId) || state.sessions[0];
        dispatch({
          type: "SET_SESSION_DETAIL",
          payload: {
            summary,
            working_dir: "D:\\Projects\\AdeshLang",
            target_file: "tests/main.adesh",
            compiler_stages: [
              { stage_name: "lex_and_parse", duration_micros: 340, success: true, metadata: {} },
              { stage_name: "decorator_phases", duration_micros: 120, success: true, metadata: {} },
              { stage_name: "raii_transformation", duration_micros: 280, success: true, metadata: {} },
              { stage_name: "hir_lower", duration_micros: 410, success: true, metadata: {} },
              { stage_name: "compile_time_memory_safety", duration_micros: 850, success: true, metadata: {} },
              { stage_name: "cranelift_native_lower", duration_micros: 1840, success: true, metadata: {} },
            ],
            linker_stages: [
              {
                stage_name: "read_objects",
                target_triple: "x86_64-windows",
                output_format: "Pe",
                input_objects_count: 2,
                symbol_count: 240,
                section_count: 6,
                duration_micros: 180,
                success: true,
              },
              {
                stage_name: "section_gc",
                target_triple: "x86_64-windows",
                output_format: "Pe",
                input_objects_count: 2,
                symbol_count: 240,
                section_count: 6,
                duration_micros: 240,
                success: true,
              },
              {
                stage_name: "emit_pe_binary",
                target_triple: "x86_64-windows",
                output_format: "Pe",
                input_objects_count: 2,
                symbol_count: 140,
                section_count: 5,
                output_size_bytes: 94208,
                duration_micros: 1420,
                success: true,
              },
            ],
            metrics_count: 28,
          },
        });
      }
    } catch (err) {
      console.error("Failed to load session detail:", err);
    }
  };

  useEffect(() => {
    refreshNow();
    if (!state.autoRefresh) return;
    const timer = setInterval(refreshNow, state.refreshIntervalMs);
    return () => clearInterval(timer);
  }, [state.autoRefresh, state.refreshIntervalMs]);

  return (
    <ObservatoryContext.Provider value={{ state, dispatch, refreshNow, selectSessionById }}>
      {children}
    </ObservatoryContext.Provider>
  );
};

export const useObservatory = (): ObservatoryContextType => {
  const context = useContext(ObservatoryContext);
  if (!context) {
    throw new Error("useObservatory must be used within an ObservatoryProvider");
  }
  return context;
};
