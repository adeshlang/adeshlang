import React, { createContext, useContext, useReducer, useEffect, ReactNode, useRef } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { SessionSummary, CollectorStats, SessionDetail, ProcessMetrics, SecurityReport } from "../types/observatory";

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

export interface GpuInfo {
  name: string;
  vendor?: string | null;
  vram_bytes?: number | null;
  driver_version?: string | null;
  status?: string | null;
}

export interface HardwareCaps {
  os_name: string;
  arch: string;
  cpu_name?: string | null;
  logical_cores: number;
  physical_cores?: number | null;
  cpu_max_clock_mhz?: number | null;
  total_memory_bytes?: number | null;
  available_memory_bytes?: number | null;
  gpus?: GpuInfo[] | null;
  gpu_detection_method?: string | null;
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
  selectedMetrics: ProcessMetrics[];
  stats: CollectorStats;
  hardware: HardwareCaps;
  searchQuery: string;
  filterMode: string;
  ipcStatus: "connected" | "disconnected" | "connecting";
  autoRefresh: boolean;
  refreshIntervalMs: number;
  lastUpdated: Date;
  securityReport: SecurityReport | null;
}

type ObservatoryAction =
  | { type: "SET_TAB"; payload: NavTab }
  | { type: "SET_SESSIONS"; payload: SessionSummary[] }
  | { type: "SELECT_SESSION"; payload: string }
  | { type: "SET_SELECTED_SESSION"; payload: string }
  | { type: "SET_SESSION_DETAIL"; payload: SessionDetail }
  | { type: "SET_SESSION_METRICS"; payload: ProcessMetrics[] }
  | { type: "SET_STATS"; payload: CollectorStats }
  | { type: "SET_HARDWARE"; payload: HardwareCaps }
  | { type: "SET_SEARCH_QUERY"; payload: string }
  | { type: "SET_FILTER_MODE"; payload: string }
  | { type: "SET_IPC_STATUS"; payload: "connected" | "disconnected" | "connecting" }
  | { type: "TOGGLE_AUTO_REFRESH" }
  | { type: "SET_REFRESH_INTERVAL"; payload: number }
  | { type: "CLEAR_SELECTION" }
  | { type: "SET_SECURITY_REPORT"; payload: SecurityReport };

const initialState: ObservatoryState = {
  currentTab: "overview",
  sessions: [],
  selectedSessionId: null,
  selectedMetrics: [],
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
    cpu_name: null,
    logical_cores: 0,
    physical_cores: null,
    cpu_max_clock_mhz: null,
    total_memory_bytes: 0,
    available_memory_bytes: null,
    gpus: [],
    gpu_detection_method: null,
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
  securityReport: null,
};

function observatoryReducer(state: ObservatoryState, action: ObservatoryAction): ObservatoryState {
  switch (action.type) {
    case "SET_TAB":
      return { ...state, currentTab: action.payload };
    case "SET_SESSIONS":
      return { ...state, sessions: action.payload, lastUpdated: new Date() };
    case "SELECT_SESSION":
      return { ...state, selectedSessionId: action.payload };
    case "SET_SELECTED_SESSION":
      return { ...state, selectedSessionId: action.payload };
    case "SET_SESSION_DETAIL":
      return { ...state, selectedDetail: action.payload };
    case "SET_SESSION_METRICS":
      return { ...state, selectedMetrics: action.payload };
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
      return { ...state, selectedSessionId: null, selectedDetail: undefined, selectedMetrics: [] };
    case "SET_SECURITY_REPORT":
      return { ...state, securityReport: action.payload };
    default:
      return state;
  }
}

interface ObservatoryContextType {
  state: ObservatoryState;
  dispatch: React.Dispatch<ObservatoryAction>;
  refreshNow: () => Promise<void>;
  purgeHistory: () => Promise<void>;
  selectSessionById: (sessionId: string, navigate?: boolean) => Promise<void>;
  rescanHistory: () => Promise<void>;
}

const ObservatoryContext = createContext<ObservatoryContextType | undefined>(undefined);

export const ObservatoryProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [state, dispatch] = useReducer(observatoryReducer, initialState);
  const lastHardwareFetch = useRef(0);
  const lastDetailFetch = useRef(0);

  const isTauriEnv = isTauri() || typeof (window as any).__TAURI_INTERNALS__ !== "undefined";

  const refreshNow = async () => {
    try {
      if (isTauriEnv) {
        let sessions: SessionSummary[] = [];
        let stats: CollectorStats | null = null;
        let hardware: HardwareCaps | null = null;

        try {
          sessions = await invoke<SessionSummary[]>("get_sessions", { limit: 200 });
        } catch (e) {
          console.error("get_sessions error:", e);
        }

        try {
          stats = await invoke<CollectorStats>("get_collector_stats");
        } catch (e) {
          console.error("get_collector_stats error:", e);
        }

        // Hardware discovery can invoke OS probes. It does not need to run on
        // every telemetry refresh, so keep it to a low-frequency cache.
        if (Date.now() - lastHardwareFetch.current > 30_000) {
          try {
            hardware = await invoke<HardwareCaps>("get_hardware_info");
            lastHardwareFetch.current = Date.now();
          } catch (e) {
            console.error("get_hardware_info error:", e);
          }
        }

        // The security report only matters on the Security & Redaction tab
        if (state.currentTab === "audit_logs") {
          try {
            const report = await invoke<SecurityReport>("get_security_report");
            if (report) {
              dispatch({ type: "SET_SECURITY_REPORT", payload: report });
            }
          } catch (e) {
            console.error("get_security_report error:", e);
          }
        }

        dispatch({ type: "SET_SESSIONS", payload: sessions || [] });

        if (stats) {
          dispatch({ type: "SET_STATS", payload: stats });
        }
        if (hardware) {
          dispatch({ type: "SET_HARDWARE", payload: hardware });
        }
        dispatch({ type: "SET_IPC_STATUS", payload: "connected" });

        // If a session is currently selected, refresh its details too
        if (state.selectedSessionId && Date.now() - lastDetailFetch.current > 5_000) {
          try {
            const detail = await invoke<SessionDetail>("get_session_detail", {
              session_id: state.selectedSessionId,
              sessionId: state.selectedSessionId,
            });
            if (detail) {
              dispatch({ type: "SET_SESSION_DETAIL", payload: detail });
            }
            const metrics = await invoke<ProcessMetrics[]>("get_session_metrics", { sessionId: state.selectedSessionId, limit: 500 });
            dispatch({ type: "SET_SESSION_METRICS", payload: metrics || [] });
            lastDetailFetch.current = Date.now();
          } catch (_) {}
        }
      } else {
        // Outside Tauri (standalone web inspector)
        dispatch({ type: "SET_IPC_STATUS", payload: "connected" });
      }
    } catch (err) {
      dispatch({ type: "SET_IPC_STATUS", payload: "disconnected" });
    }
  };

  const purgeHistory = async () => {
    try {
      if (isTauriEnv) {
        await invoke("purge_history");
      }
      dispatch({ type: "SET_SESSIONS", payload: [] });
      dispatch({ type: "CLEAR_SELECTION" });
      await refreshNow();
    } catch (e) {
      console.error("Failed to purge history:", e);
    }
  };

  const rescanHistory = async () => {
    try {
      if (isTauriEnv) {
        const report = await invoke<SecurityReport>("rescan_history");
        if (report) {
          dispatch({ type: "SET_SECURITY_REPORT", payload: report });
        }
      }
      await refreshNow();
    } catch (e) {
      console.error("Failed to rescan history:", e);
    }
  };

  const selectSessionById = async (sessionId: string, navigate = true) => {
    dispatch({ type: navigate ? "SELECT_SESSION" : "SET_SELECTED_SESSION", payload: sessionId });
    try {
      if (isTauriEnv) {
        const detail = await invoke<SessionDetail>("get_session_detail", {
          session_id: sessionId,
          sessionId,
        });
        dispatch({ type: "SET_SESSION_DETAIL", payload: detail });
        const metrics = await invoke<ProcessMetrics[]>("get_session_metrics", { sessionId, limit: 500 });
        dispatch({ type: "SET_SESSION_METRICS", payload: metrics || [] });
      } else {
        const summary = state.sessions.find((s) => s.session_id === sessionId);
        if (summary) {
          dispatch({
            type: "SET_SESSION_DETAIL",
            payload: {
              summary,
              working_dir: ".",
              compiler_stages: [],
              linker_stages: [],
              runtime_events: [],
              metrics_count: 0,
            },
          });
        }
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
  }, [state.autoRefresh, state.refreshIntervalMs, state.selectedSessionId, state.currentTab]);

  return (
    <ObservatoryContext.Provider
      value={{ state, dispatch, refreshNow, purgeHistory, selectSessionById, rescanHistory }}
    >
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
