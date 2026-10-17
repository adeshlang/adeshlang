import React, { useState, useMemo } from "react";
import { useObservatory } from "../context/ObservatoryContext";
import { SessionSummary } from "../types/observatory";
import {
  Activity,
  Cpu,
  Database,
  Flame,
  Zap,
  CheckCircle2,
  Clock,
  Terminal,
  TrendingUp,
  AlertTriangle,
  ArrowUpRight,
  Trash2,
  RefreshCw,
  Layers,
  ListFilter,
  ChevronDown,
  ChevronRight,
  Box,
  X,
} from "lucide-react";
import {
  AreaChart,
  Area,
  ResponsiveContainer,
  XAxis,
  YAxis,
  Tooltip,
} from "recharts";

const RUNTIME_METADATA: Record<
  string,
  { label: string; color: string; badgeBg: string; border: string; icon: React.ComponentType<{ className?: string }> }
> = {
  interpreter: {
    label: "Interpreter Runtime",
    color: "text-amber-400",
    badgeBg: "bg-amber-500/10",
    border: "border-amber-500/30",
    icon: Terminal,
  },
  jit: {
    label: "Native JIT (10-232x)",
    color: "text-sky-400",
    badgeBg: "bg-sky-500/10",
    border: "border-sky-500/30",
    icon: Zap,
  },
  tiered_jit: {
    label: "Tiered JIT",
    color: "text-blue-400",
    badgeBg: "bg-blue-500/10",
    border: "border-blue-500/30",
    icon: Cpu,
  },
  adaptive_jit: {
    label: "Adaptive JIT",
    color: "text-teal-400",
    badgeBg: "bg-teal-500/10",
    border: "border-teal-500/30",
    icon: Activity,
  },
  native_compiler: {
    label: "Adesh Native Compiler",
    color: "text-emerald-300",
    badgeBg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
    icon: Layers,
  },
  bytecode_vm: {
    label: "Bytecode VM",
    color: "text-purple-400",
    badgeBg: "bg-purple-500/10",
    border: "border-purple-500/30",
    icon: Database,
  },
  aot_native: {
    label: "AOT Native Binary",
    color: "text-emerald-400",
    badgeBg: "bg-emerald-500/10",
    border: "border-emerald-500/30",
    icon: CheckCircle2,
  },
  web_assembly: {
    label: "WebAssembly (Wasm)",
    color: "text-orange-400",
    badgeBg: "bg-orange-500/10",
    border: "border-orange-500/30",
    icon: Flame,
  },
  linker: {
    label: "Adesh Linker (ADOB)",
    color: "text-pink-400",
    badgeBg: "bg-pink-500/10",
    border: "border-pink-500/30",
    icon: Layers,
  },
  unknown: {
    label: "Other Tools & Subprocesses",
    color: "text-slate-400",
    badgeBg: "bg-slate-500/10",
    border: "border-slate-500/30",
    icon: Box,
  },
};

export const CockpitOverview: React.FC = () => {
  const { state, selectSessionById, clearSelection, refreshNow, purgeHistory } = useObservatory();
  const { sessions, stats, searchQuery, filterMode } = state;

  const [groupByRuntime, setGroupByRuntime] = useState<boolean>(true);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  // Filter and sort sessions: NEWEST ALWAYS AT TOP
  const sortedSessions = useMemo(() => {
    const filtered = sessions.filter((s) => {
      const matchesSearch =
        s.session_id.toLowerCase().includes(searchQuery.toLowerCase()) ||
        s.tool_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        s.command_line.toLowerCase().includes(searchQuery.toLowerCase()) ||
        s.pid.toString().includes(searchQuery);

      const matchesMode = filterMode === "all" || s.execution_mode === filterMode;
      return matchesSearch && matchesMode;
    });

    // Descending order by start_time: newest at top always
    return filtered.sort((a, b) => {
      const tA = new Date(a.start_time).getTime();
      const tB = new Date(b.start_time).getTime();
      return tB - tA;
    });
  }, [sessions, searchQuery, filterMode]);

  const activeSessions = sessions.filter((s) => s.status === "running");

  // Telemetry chart from latest 10 executions
  const chartData = useMemo(() => {
    return sortedSessions.slice(0, 10).reverse().map((s, idx) => ({
      name: `S${idx + 1}`,
      duration: s.duration_ms || 10,
      rss: Number((s.peak_rss_bytes / (1024 * 1024)).toFixed(1)),
      cpu: s.cpu_peak_percent,
    }));
  }, [sortedSessions]);

  // Grouped runtime buckets with newest sessions on top
  const groupedSessions = useMemo(() => {
    const groups: Record<string, SessionSummary[]> = {};
    for (const s of sortedSessions) {
      const key = s.execution_mode || "unknown";
      if (!groups[key]) groups[key] = [];
      groups[key].push(s);
    }
    return groups;
  }, [sortedSessions]);

  const toggleGroupCollapse = (key: string) => {
    setCollapsedGroups((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const formatTime = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    } catch {
      return iso;
    }
  };

  const formatDate = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
    } catch {
      return "";
    }
  };

  const formatBytes = (bytes?: number) => {
    if (!bytes || bytes <= 0) return "0 B";
    if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${bytes} B`;
  };

  const handleRowClick = (sessionId: string) => {
    if (state.selectedSessionId === sessionId) {
      clearSelection();
    } else {
      selectSessionById(sessionId, false);
    }
  };

  const renderTable = (list: SessionSummary[]) => (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-xs">
        <thead>
          <tr className="border-b border-slate-800 text-slate-400 bg-slate-950/40">
            <th className="py-3 px-4 font-semibold">Session UUID</th>
            <th className="py-3 px-3 font-semibold">
              <span className="flex items-center space-x-1">
                <Clock className="w-3.5 h-3.5 text-sky-400" />
                <span>Timestamp</span>
              </span>
            </th>
            <th className="py-3 px-3 font-semibold">Tool</th>
            <th className="py-3 px-3 font-semibold">PID</th>
            <th className="py-3 px-3 font-semibold">Runtime Mode</th>
            <th className="py-3 px-3 font-semibold">Status</th>
            <th className="py-3 px-3 font-semibold">Duration</th>
            <th className="py-3 px-3 font-semibold">
              <span className="flex items-center space-x-1">
                <Cpu className="w-3.5 h-3.5 text-emerald-400" />
                <span>CPU / Threads</span>
              </span>
            </th>
            <th className="py-3 px-3 font-semibold">
              <span className="flex items-center space-x-1">
                <Activity className="w-3.5 h-3.5 text-indigo-400" />
                <span>Memory Breakdown</span>
              </span>
            </th>
            <th className="py-3 px-4 font-semibold text-right">Details</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-800/60">
          {list.map((s) => {
            const isSelected = state.selectedSessionId === s.session_id;
            const detail = isSelected ? state.selectedDetail : undefined;
            const latestM = detail?.latest_metrics;
            const staticMem = s.memory_static_bytes || latestM?.memory_static_bytes || 0;
            const heapMem = s.memory_heap_bytes || latestM?.memory_heap_bytes || 0;
            const dynamicMem = s.memory_dynamic_bytes || latestM?.memory_dynamic_bytes || 0;
            const reusedMem = s.memory_reused_bytes || latestM?.memory_reused_bytes || 0;
            const threads = s.thread_count || latestM?.thread_count || 1;
            const peakThreads = s.peak_threads || latestM?.peak_thread_count || threads;
            const cores = s.cores_used || latestM?.cores_used || (s.cpu_peak_percent > 0 ? Number((s.cpu_peak_percent / 100).toFixed(2)) : 1.0);

            return (
              <React.Fragment key={s.session_id}>
                <tr
                  onClick={() => handleRowClick(s.session_id)}
                  className={`cursor-pointer transition-colors group ${
                    isSelected ? "bg-sky-950/30 border-l-2 border-l-sky-400" : "hover:bg-slate-800/50"
                  }`}
                >
                  <td className="py-3 px-4 font-mono text-sky-400 font-medium">
                    {s.session_id.substring(0, 13)}...
                  </td>
                  <td className="py-3 px-3 whitespace-nowrap" title={new Date(s.start_time).toLocaleString()}>
                    <div className="text-[11px] font-semibold text-slate-200">{formatTime(s.start_time)}</div>
                    <div className="text-[10px] text-slate-500">{formatDate(s.start_time)}</div>
                  </td>
                  <td className="py-3 px-3 font-bold text-slate-200">{s.tool_name}</td>
                  <td className="py-3 px-3 font-mono text-slate-400">{s.pid}</td>
                  <td className="py-3 px-3">
                    <span className="px-2 py-0.5 rounded text-[11px] font-mono bg-slate-800 text-slate-300 border border-slate-700">
                      {s.execution_mode}
                    </span>
                  </td>
                  <td className="py-3 px-3">
                    <span
                      className={`inline-flex items-center space-x-1 px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-wider ${
                        s.status === "completed"
                          ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/30"
                          : s.status === "running"
                          ? "bg-sky-500/10 text-sky-400 border border-sky-500/30 animate-pulse"
                          : "bg-red-500/10 text-red-400 border border-red-500/30"
                      }`}
                    >
                      <span className={`w-1 h-1 rounded-full ${s.status === "completed" ? "bg-emerald-400" : "bg-sky-400"}`}></span>
                      <span>{s.status}</span>
                    </span>
                  </td>
                  <td className="py-3 px-3 font-mono font-semibold text-slate-200">
                    {s.duration_ms ? `${s.duration_ms} ms` : "running..."}
                  </td>
                  <td className="py-3 px-3">
                    <div className="flex flex-col text-[11px] font-mono">
                      <span className="text-emerald-400 font-semibold">
                        {s.cpu_peak_percent > 0 ? `${s.cpu_peak_percent.toFixed(1)}% CPU` : "Active"}
                      </span>
                      <span className="text-slate-400 text-[10px]">
                        {threads} {threads === 1 ? "thread" : "threads"} ({cores} cores)
                      </span>
                    </div>
                  </td>
                  <td className="py-3 px-3">
                    <div className="flex flex-col text-[11px] font-mono">
                      <span className="text-slate-200 font-semibold">
                        {(s.peak_rss_bytes / (1024 * 1024)).toFixed(1)} MB <span className="text-slate-500 text-[10px]">RSS</span>
                      </span>
                      <span className="text-slate-400 text-[10px]">
                        Static: {formatBytes(staticMem)} · Heap: {formatBytes(heapMem)}
                      </span>
                    </div>
                  </td>
                  <td className="py-3 px-4 text-right">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleRowClick(s.session_id);
                      }}
                      className={`p-1.5 rounded transition-colors ${
                        isSelected ? "bg-sky-500 text-slate-950 font-bold" : "bg-slate-800 group-hover:bg-sky-500 group-hover:text-slate-950 text-slate-400"
                      }`}
                      title={isSelected ? "Collapse session details" : "Expand session details"}
                      aria-label={isSelected ? "Collapse session details" : "Expand session details"}
                    >
                      {isSelected ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                    </button>
                  </td>
                </tr>

                {/* IN-PLACE UNIFIED PROCESS INSPECTOR PANEL */}
                {isSelected && (
                  <tr>
                    <td colSpan={10} className="p-0 border-b border-sky-500/30 bg-slate-950/90">
                      <div className="p-5 space-y-5 animate-in fade-in duration-200">
                        {/* Process Header & Meta */}
                        <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-slate-800">
                          <div className="space-y-1">
                            <div className="flex items-center space-x-2">
                              <span className="text-xs font-bold text-sky-400 uppercase tracking-wider">Process Inspector</span>
                              <span className="text-xs text-slate-500 font-mono">Session ID: {s.session_id}</span>
                              <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-indigo-500/10 text-indigo-300 border border-indigo-500/30">
                                Mode: {s.execution_mode}
                              </span>
                            </div>
                            <div className="font-mono text-xs text-slate-300 bg-slate-900 px-3 py-1.5 rounded border border-slate-800">
                              <span className="text-slate-500 select-none">$ </span>
                              {s.command_line}
                            </div>
                          </div>
                          <div className="flex items-center gap-3">
                            <div className="text-right text-xs text-slate-400 font-mono space-y-0.5">
                              <div>PID: <span className="text-slate-200">{s.pid}</span> {s.parent_pid ? `(Parent: ${s.parent_pid})` : ""}</div>
                              <div>Start: <span className="text-slate-200">{formatTime(s.start_time)}</span></div>
                              {s.exit_code != null && <div>Exit Code: <span className={s.exit_code === 0 ? "text-emerald-400 font-bold" : "text-rose-400 font-bold"}>{s.exit_code}</span></div>}
                            </div>
                            <button
                              type="button"
                              onClick={(e) => {
                                e.stopPropagation();
                                clearSelection();
                              }}
                              className="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white border border-slate-700 transition-colors flex items-center space-x-1.5 text-xs font-semibold shrink-0 cursor-pointer shadow-sm"
                              title="Collapse opened session panel"
                              aria-label="Collapse session panel"
                            >
                              <X className="w-3.5 h-3.5" />
                              <span>Collapse</span>
                            </button>
                          </div>
                        </div>

                        {/* Real-time Memory Usage & Thread/Core Breakdown */}
                        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
                          {/* 1. Static Memory */}
                          <div className="bg-slate-900/90 border border-slate-800 p-3.5 rounded-xl space-y-1">
                            <div className="text-[10px] text-slate-400 font-semibold uppercase tracking-wider flex items-center justify-between">
                              <span>Static Memory</span>
                              <span className="text-sky-400">Code / RoData</span>
                            </div>
                            <div className="text-lg font-bold font-mono text-slate-100">
                              {formatBytes(staticMem)}
                            </div>
                            <p className="text-[10px] text-slate-500">Fixed binary footprint, string tables, compiled instructions</p>
                          </div>

                          {/* 2. Heap Memory */}
                          <div className="bg-slate-900/90 border border-slate-800 p-3.5 rounded-xl space-y-1">
                            <div className="text-[10px] text-slate-400 font-semibold uppercase tracking-wider flex items-center justify-between">
                              <span>Heap Memory</span>
                              <span className="text-indigo-400">Committed</span>
                            </div>
                            <div className="text-lg font-bold font-mono text-slate-100">
                              {formatBytes(heapMem)}
                            </div>
                            <p className="text-[10px] text-slate-500">Allocated managed heap buffer space for dynamic values</p>
                          </div>

                          {/* 3. Dynamic & Reused Memory */}
                          <div className="bg-slate-900/90 border border-slate-800 p-3.5 rounded-xl space-y-1">
                            <div className="text-[10px] text-slate-400 font-semibold uppercase tracking-wider flex items-center justify-between">
                              <span>Dynamic & Reused</span>
                              <span className="text-emerald-400">Recycled</span>
                            </div>
                            <div className="text-lg font-bold font-mono text-slate-100 flex items-baseline space-x-2">
                              <span>{formatBytes(dynamicMem)}</span>
                              <span className="text-xs text-emerald-400 font-normal">({formatBytes(reusedMem)} reused)</span>
                            </div>
                            <p className="text-[10px] text-slate-500">Active allocations & RAII reused recycled slots</p>
                          </div>

                          {/* 4. Threads & Cores Used */}
                          <div className="bg-slate-900/90 border border-slate-800 p-3.5 rounded-xl space-y-1">
                            <div className="text-[10px] text-slate-400 font-semibold uppercase tracking-wider flex items-center justify-between">
                              <span>Threads & Cores</span>
                              <span className="text-amber-400">Execution</span>
                            </div>
                            <div className="text-lg font-bold font-mono text-slate-100 flex items-baseline space-x-2">
                              <span>{threads} <span className="text-xs font-normal text-slate-400">threads</span></span>
                              <span className="text-xs text-amber-300 font-normal">({cores} cores used)</span>
                            </div>
                            <p className="text-[10px] text-slate-500">Peak threads observed: {peakThreads} · CPU: {s.cpu_peak_percent.toFixed(1)}%</p>
                          </div>
                        </div>

                        {/* Linker Diagnostics & Compiler Stages Accordion */}
                        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                          {/* Linker Diagnostics */}
                          <div className="bg-slate-900/70 border border-slate-800 rounded-xl p-4 space-y-3">
                            <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                              <h4 className="text-xs font-bold text-slate-200 flex items-center space-x-2">
                                <Box className="w-4 h-4 text-violet-400" />
                                <span>Linker Diagnostics (adeshlink)</span>
                              </h4>
                              <span className="text-[11px] font-mono text-slate-400">
                                {detail?.linker_stages.length || 0} stage(s)
                              </span>
                            </div>
                            {detail?.linker_stages && detail.linker_stages.length > 0 ? (
                              <div className="space-y-2 max-h-48 overflow-y-auto">
                                {detail.linker_stages.map((ls, idx) => (
                                  <div key={idx} className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800 text-xs space-y-1">
                                    <div className="flex items-center justify-between font-mono">
                                      <span className="font-semibold text-sky-300">{ls.stage_name}</span>
                                      <span className={ls.success ? "text-emerald-400 font-semibold" : "text-rose-400 font-semibold"}>
                                        {ls.success ? "Success" : "Failed"}
                                      </span>
                                    </div>
                                    <div className="grid grid-cols-2 gap-2 text-[11px] text-slate-400 font-mono">
                                      <div>Target: <span className="text-slate-200">{ls.target_triple}</span></div>
                                      <div>Format: <span className="text-slate-200">{ls.output_format}</span></div>
                                      <div>Symbols: <span className="text-slate-200">{ls.symbol_count}</span></div>
                                      <div>Sections: <span className="text-slate-200">{ls.section_count}</span></div>
                                      <div>Artifact: <span className="text-emerald-300">{ls.output_size_bytes ? formatBytes(ls.output_size_bytes) : "—"}</span></div>
                                      <div>Time: <span className="text-slate-200">{(ls.duration_micros / 1000).toFixed(2)} ms</span></div>
                                    </div>
                                    {ls.error_message && (
                                      <div className="text-[10px] text-rose-300 font-mono mt-1 p-1.5 rounded bg-rose-950/30 border border-rose-900/40">
                                        {ls.error_message}
                                      </div>
                                    )}
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <div className="text-center py-6 text-xs text-slate-500 font-mono">
                                {s.execution_mode === "native_compiler" || s.execution_mode === "aot_native" || s.execution_mode === "linker"
                                  ? "Linker stages loading or finalized directly."
                                  : "Non-linker runtime execution mode."}
                              </div>
                            )}
                          </div>

                          {/* Compiler Stages */}
                          <div className="bg-slate-900/70 border border-slate-800 rounded-xl p-4 space-y-3">
                            <div className="flex items-center justify-between border-b border-slate-800 pb-2">
                              <h4 className="text-xs font-bold text-slate-200 flex items-center space-x-2">
                                <Layers className="w-4 h-4 text-emerald-400" />
                                <span>Compiler Pipeline Stages</span>
                              </h4>
                              <span className="text-[11px] font-mono text-slate-400">
                                {detail?.compiler_stages.length || 0} stage(s)
                              </span>
                            </div>
                            {detail?.compiler_stages && detail.compiler_stages.length > 0 ? (
                              <div className="space-y-2 max-h-48 overflow-y-auto">
                                {detail.compiler_stages.map((cs, idx) => (
                                  <div key={idx} className="p-2.5 rounded-lg bg-slate-950/60 border border-slate-800 text-xs space-y-1">
                                    <div className="flex items-center justify-between font-mono">
                                      <span className="font-semibold text-emerald-300">{cs.stage_name}</span>
                                      <span className="text-slate-400 font-mono">{(cs.duration_micros / 1000).toFixed(2)} ms</span>
                                    </div>
                                    <div className="text-[10px] text-slate-500 font-mono truncate">
                                      {cs.input_file || "in-memory source"}
                                    </div>
                                    {Object.keys(cs.metadata || {}).length > 0 && (
                                      <div className="text-[10px] text-slate-400 font-mono">
                                        {Object.entries(cs.metadata).slice(0, 3).map(([k, v]) => `${k}=${v}`).join(" · ")}
                                      </div>
                                    )}
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <div className="text-center py-6 text-xs text-slate-500 font-mono">
                                No compiler intermediate stages captured for this run.
                              </div>
                            )}
                          </div>
                        </div>

                        {/* Footer Collapse Bar */}
                        <div className="flex justify-end pt-3 border-t border-slate-800/80">
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              clearSelection();
                            }}
                            className="px-3 py-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 text-slate-300 hover:text-white border border-slate-700/80 text-xs font-medium flex items-center space-x-1.5 transition-colors cursor-pointer"
                            title="Collapse opened session panel"
                          >
                            <ChevronDown className="w-3.5 h-3.5 rotate-180" />
                            <span>Collapse Session</span>
                          </button>
                        </div>
                      </div>
                    </td>
                  </tr>
                )}
              </React.Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      {/* Cockpit Header with Action Controls */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-1 border-b border-slate-800/80">
        <div>
          <h1 className="text-lg font-bold text-slate-100 tracking-tight flex items-center space-x-2">
            <span>Observatory Cockpit</span>
            <span className="text-[10px] px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono">
              Live SQLite WAL
            </span>
          </h1>
          <p className="text-xs text-slate-400 mt-0.5">
            Durable telemetry history synchronized at <code className="text-sky-400 font-mono">~/.adesh/observatory.db</code>
          </p>
        </div>
        <div className="flex items-center flex-wrap gap-2">
          {/* Runtime Grouping Toggle */}
          <button
            onClick={() => setGroupByRuntime(!groupByRuntime)}
            className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-lg border transition-colors text-xs font-semibold ${
              groupByRuntime
                ? "bg-indigo-500/20 border-indigo-500/40 text-indigo-300"
                : "bg-slate-800/60 border-slate-700 text-slate-400 hover:text-slate-200"
            }`}
            title="Group executions by runtime mode (Interpreter, JIT, VM, Linker)"
          >
            <Layers className="w-3.5 h-3.5" />
            <span>{groupByRuntime ? "Grouped by Runtime" : "Flat List"}</span>
          </button>

          {/* Clear Dashboard */}
          <button
            onClick={purgeHistory}
            className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-300 hover:text-rose-200 transition-colors text-xs font-semibold"
            title="Clear all recorded sessions from dashboard and database"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Clear Dashboard</span>
            <span className="sm:hidden">Clear</span>
          </button>

          {/* Refresh */}
          <button
            onClick={refreshNow}
            className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-sky-500/10 hover:bg-sky-500/20 border border-sky-500/30 text-sky-300 hover:text-white transition-colors text-xs font-semibold"
            title="Force refresh telemetry from SQLite"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Refresh</span>
          </button>
        </div>
      </div>

      {/* Top Banner Metric Tiles */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-sky-950/30 border border-slate-800 p-4 rounded-xl relative overflow-hidden shadow-sm">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-[11px] uppercase tracking-wider font-semibold">Active Tool Processes</span>
            <span className="p-1 rounded-md bg-emerald-500/10 text-emerald-400">
              <Zap className="w-4 h-4" />
            </span>
          </div>
          <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
            {activeSessions.length}
          </div>
          <div className="flex items-center space-x-1.5 text-xs text-emerald-400 mt-2 font-medium">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
            <span>Real-time polling active (250ms)</span>
          </div>
        </div>

        <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-indigo-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-[11px] uppercase tracking-wider font-semibold">SQLite Telemetry Store</span>
            <span className="p-1 rounded-md bg-sky-500/10 text-sky-400">
              <Database className="w-4 h-4" />
            </span>
          </div>
          <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
            {stats.stored_sessions_count}
          </div>
          <div className="text-xs text-slate-400 mt-2">
            Size: <span className="font-mono text-sky-400">{(stats.database_size_bytes / (1024 * 1024)).toFixed(2)} MB</span> (WAL Mode)
          </div>
        </div>

        <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-amber-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-[11px] uppercase tracking-wider font-semibold">Events Ingested</span>
            <span className="p-1 rounded-md bg-amber-500/10 text-amber-400">
              <Flame className="w-4 h-4" />
            </span>
          </div>
          <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
            {stats.total_events_ingested}
          </div>
          <div className="text-xs text-amber-400/90 mt-2">
            Dropped: <span className="font-mono font-bold text-slate-300">{stats.total_events_dropped}</span> (Zero Backpressure Loss)
          </div>
        </div>

        <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-violet-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-[11px] uppercase tracking-wider font-semibold">Order & Grouping</span>
            <span className="p-1 rounded-md bg-indigo-500/10 text-indigo-400">
              <Clock className="w-4 h-4" />
            </span>
          </div>
          <div className="text-sm font-bold text-slate-100 mt-1">
            Newest First Always
          </div>
          <div className="text-xs text-slate-400 mt-2">
            Mode: <span className="text-indigo-400 font-semibold">{groupByRuntime ? "Grouped" : "Flat"}</span>
          </div>
        </div>
      </div>

      {/* Real-time Telemetry Trend Visualizer */}
      <div className="bg-slate-900/80 border border-slate-800 p-5 rounded-xl shadow-sm space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-sm font-semibold text-slate-200 flex items-center space-x-2">
              <TrendingUp className="w-4 h-4 text-sky-400" />
              <span>Compilation Duration & Memory RSS Trends</span>
            </h2>
            <p className="text-xs text-slate-400">Continuous sampling across recent Adesh tool executions</p>
          </div>
          <div className="flex items-center space-x-4 text-xs font-mono">
            <span className="flex items-center space-x-1.5 text-sky-400">
              <span className="w-2.5 h-2.5 rounded-full bg-sky-500"></span>
              <span>Duration (ms)</span>
            </span>
            <span className="flex items-center space-x-1.5 text-indigo-400">
              <span className="w-2.5 h-2.5 rounded-full bg-indigo-500"></span>
              <span>Peak RSS (MB)</span>
            </span>
          </div>
        </div>

        <div className="h-44 w-full pt-2">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={chartData}>
              <defs>
                <linearGradient id="colorDur" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#38bdf8" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#38bdf8" stopOpacity={0} />
                </linearGradient>
                <linearGradient id="colorRss" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#818cf8" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#818cf8" stopOpacity={0} />
                </linearGradient>
              </defs>
              <XAxis dataKey="name" stroke="#475569" fontSize={10} tickLine={false} />
              <YAxis stroke="#475569" fontSize={10} tickLine={false} />
              <Tooltip
                contentStyle={{ backgroundColor: "#0f172a", borderColor: "#1e293b", fontSize: "12px", borderRadius: "8px" }}
              />
              <Area type="monotone" dataKey="duration" stroke="#38bdf8" strokeWidth={2} fillOpacity={1} fill="url(#colorDur)" />
              <Area type="monotone" dataKey="rss" stroke="#818cf8" strokeWidth={2} fillOpacity={1} fill="url(#colorRss)" />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Monitored Invocations Section */}
      {groupByRuntime ? (
        /* ================= GROUPED RUNTIME VIEW ================= */
        <div className="space-y-4">
          <div className="flex items-center justify-between pb-1">
            <h2 className="text-sm font-bold text-slate-100 flex items-center space-x-2">
              <Layers className="w-4 h-4 text-indigo-400" />
              <span>Runtime Execution Groups</span>
            </h2>
            <span className="text-xs text-slate-400 font-mono">
              Total {sortedSessions.length} recorded executions
            </span>
          </div>

          {Object.entries(groupedSessions).map(([modeKey, groupList]) => {
            const meta = RUNTIME_METADATA[modeKey] || RUNTIME_METADATA.unknown;
            const IconComp = meta.icon;
            const isCollapsed = collapsedGroups[modeKey];

            return (
              <div
                key={modeKey}
                className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm"
              >
                {/* Group Header Banner */}
                <div
                  onClick={() => toggleGroupCollapse(modeKey)}
                  className="p-3.5 border-b border-slate-800 flex items-center justify-between bg-slate-950/60 cursor-pointer hover:bg-slate-950/80 transition-colors select-none"
                >
                  <div className="flex items-center space-x-3">
                    <span className={`p-1.5 rounded-lg ${meta.badgeBg} ${meta.color} border ${meta.border}`}>
                      <IconComp className="w-4 h-4" />
                    </span>
                    <div>
                      <div className="flex items-center space-x-2">
                        <span className="text-sm font-bold text-slate-100">{meta.label}</span>
                        <span className="text-[11px] font-mono px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 font-semibold border border-slate-700">
                          {groupList.length} {groupList.length === 1 ? "execution" : "executions"}
                        </span>
                      </div>
                      <p className="text-[11px] text-slate-400">
                        Latest at {formatTime(groupList[0].start_time)} ({formatDate(groupList[0].start_time)})
                      </p>
                    </div>
                  </div>

                  <div className="flex items-center space-x-2 text-slate-400">
                    <span className="text-xs hidden sm:inline text-slate-500">
                      {isCollapsed ? "Click to expand" : "Click to collapse"}
                    </span>
                    {isCollapsed ? (
                      <ChevronRight className="w-4 h-4 text-slate-400" />
                    ) : (
                      <ChevronDown className="w-4 h-4 text-slate-400" />
                    )}
                  </div>
                </div>

                {/* Group Table (Newest at Top) */}
                {!isCollapsed && renderTable(groupList)}
              </div>
            );
          })}

          {sortedSessions.length === 0 && (
            <div className="bg-slate-900/80 border border-slate-800 rounded-xl p-12 text-center">
              <div className="max-w-md mx-auto space-y-3">
                <div className="w-10 h-10 rounded-full bg-slate-800 text-sky-400 flex items-center justify-center mx-auto">
                  <Terminal className="w-5 h-5" />
                </div>
                <div className="text-sm font-bold text-slate-200">No Executions Recorded Yet</div>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Observatory is actively listening. Run any Adesh program in your terminal to see real-time data:
                </p>
                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800 font-mono text-left text-xs space-y-1.5 text-sky-300">
                  <div>$ adesh run main.adesh</div>
                  <div>$ adesh run main.adesh --njit</div>
                  <div>$ adesh run main.adesh --vm</div>
                </div>
              </div>
            </div>
          )}
        </div>
      ) : (
        /* ================= FLAT LIST VIEW ================= */
        <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
          <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/40">
            <div>
              <h3 className="text-sm font-semibold text-slate-100 flex items-center space-x-2">
                <Terminal className="w-4 h-4 text-sky-400" />
                <span>Live Monitored Invocations (Newest First)</span>
              </h3>
              <p className="text-xs text-slate-400">Click any row to inspect deep stage timelines and symbol tables</p>
            </div>
            <span className="text-xs text-slate-400 font-mono">
              Showing {sortedSessions.length} of {sessions.length}
            </span>
          </div>

          {sortedSessions.length > 0 ? (
            renderTable(sortedSessions)
          ) : (
            <div className="text-center py-12 px-6">
              <div className="max-w-md mx-auto space-y-3">
                <div className="w-10 h-10 rounded-full bg-slate-800 text-sky-400 flex items-center justify-center mx-auto">
                  <Terminal className="w-5 h-5" />
                </div>
                <div className="text-sm font-bold text-slate-200">No Executions Recorded Yet</div>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Observatory is actively listening. Run any Adesh program in your terminal to view real-time telemetry:
                </p>
                <div className="p-3 bg-slate-950 rounded-lg border border-slate-800 font-mono text-left text-xs space-y-1.5 text-sky-300">
                  <div>$ adesh run main.adesh</div>
                  <div>$ adesh run main.adesh --njit</div>
                  <div>$ adeshlink main.adob -o app.exe</div>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
