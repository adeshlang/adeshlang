import React from "react";
import { useObservatory } from "../context/ObservatoryContext";
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
} from "lucide-react";
import {
  AreaChart,
  Area,
  ResponsiveContainer,
  XAxis,
  YAxis,
  Tooltip,
} from "recharts";

export const CockpitOverview: React.FC = () => {
  const { state, selectSessionById, refreshNow, purgeHistory } = useObservatory();
  const { sessions, stats, searchQuery, filterMode } = state;

  // Filter sessions based on search & execution mode
  const filteredSessions = sessions.filter((s) => {
    const matchesSearch =
      s.session_id.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.tool_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.command_line.toLowerCase().includes(searchQuery.toLowerCase()) ||
      s.pid.toString().includes(searchQuery);

    const matchesMode = filterMode === "all" || s.execution_mode === filterMode;
    return matchesSearch && matchesMode;
  });

  const activeSessions = sessions.filter((s) => s.status === "running");
  const completedSessions = sessions.filter((s) => s.status === "completed");

  // Chart telemetry data
  const chartData = sessions.slice(0, 10).map((s, idx) => ({
    name: `S${idx + 1}`,
    duration: s.duration_ms || 10,
    rss: (s.peak_rss_bytes / (1024 * 1024)).toFixed(1),
    cpu: s.cpu_peak_percent,
  }));

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
        <div className="flex items-center space-x-2">
          <button
            onClick={purgeHistory}
            className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-300 hover:text-rose-200 transition-colors text-xs font-semibold"
            title="Clear all recorded sessions from dashboard and database"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Clear Dashboard</span>
            <span className="sm:hidden">Clear</span>
          </button>
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
            <span className="text-[11px] uppercase tracking-wider font-semibold">Collector Daemon</span>
            <span className="p-1 rounded-md bg-indigo-500/10 text-indigo-400">
              <Cpu className="w-4 h-4" />
            </span>
          </div>
          <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
            {stats.uptime_secs}s
          </div>
          <div className="text-xs text-slate-400 mt-2">
            Platform: <span className="font-semibold text-slate-300">All OS Native</span>
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

      {/* Monitored Executions Table */}
      <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-slate-950/40">
          <div>
            <h3 className="text-sm font-semibold text-slate-100 flex items-center space-x-2">
              <Terminal className="w-4 h-4 text-sky-400" />
              <span>Live Monitored Invocations</span>
            </h3>
            <p className="text-xs text-slate-400">Click any row to inspect deep stage timelines and symbol tables</p>
          </div>
          <span className="text-xs text-slate-400 font-mono">
            Showing {filteredSessions.length} of {sessions.length}
          </span>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-slate-800 text-slate-400 bg-slate-950/30">
                <th className="py-3 px-4 font-semibold">Session UUID</th>
                <th className="py-3 px-3 font-semibold">Tool</th>
                <th className="py-3 px-3 font-semibold">PID</th>
                <th className="py-3 px-3 font-semibold">Execution Mode</th>
                <th className="py-3 px-3 font-semibold">Status</th>
                <th className="py-3 px-3 font-semibold">Duration</th>
                <th className="py-3 px-3 font-semibold">Peak Memory</th>
                <th className="py-3 px-4 font-semibold text-right">Inspect</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60">
              {filteredSessions.map((s) => (
                <tr
                  key={s.session_id}
                  onClick={() => selectSessionById(s.session_id)}
                  className="hover:bg-slate-800/50 cursor-pointer transition-colors group"
                >
                  <td className="py-3 px-4 font-mono text-sky-400 font-medium">
                    {s.session_id.substring(0, 13)}...
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
                  <td className="py-3 px-3 font-mono text-slate-300">
                    {(s.peak_rss_bytes / (1024 * 1024)).toFixed(1)} MB
                  </td>
                  <td className="py-3 px-4 text-right">
                    <button className="p-1 rounded bg-slate-800 group-hover:bg-sky-500 group-hover:text-slate-950 text-slate-400 transition-colors">
                      <ArrowUpRight className="w-3.5 h-3.5" />
                    </button>
                  </td>
                </tr>
              ))}
              {filteredSessions.length === 0 && (
                <tr>
                  <td colSpan={8} className="text-center py-12 px-6">
                    <div className="max-w-md mx-auto space-y-3">
                      <div className="w-10 h-10 rounded-full bg-slate-800 text-sky-400 flex items-center justify-center mx-auto">
                        <Terminal className="w-5 h-5" />
                      </div>
                      <div className="text-sm font-bold text-slate-200">No Executions Recorded Yet</div>
                      <p className="text-xs text-slate-400 leading-relaxed">
                        Observatory is actively listening. Run any Adesh program or compilation command in your terminal to view real-time telemetry:
                      </p>
                      <div className="p-3 bg-slate-950 rounded-lg border border-slate-800 font-mono text-left text-xs space-y-1.5 text-sky-300">
                        <div>$ adesh run tests/main.adesh</div>
                        <div>$ adesh run tests/main.adesh --njit</div>
                        <div>$ adeshlink main.adob -o app.exe</div>
                      </div>
                      <button
                        onClick={refreshNow}
                        className="inline-flex items-center space-x-1.5 text-xs text-sky-400 hover:text-sky-300 font-medium pt-1 cursor-pointer"
                      >
                        <RefreshCw className="w-3.5 h-3.5" />
                        <span>Check for new sessions</span>
                      </button>
                    </div>
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
