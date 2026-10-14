import React from "react";
import { SessionSummary, CollectorStats } from "../types/observatory";
import { Activity, Clock, Database, Flame, CheckCircle, AlertTriangle } from "lucide-react";

interface OverviewProps {
  sessions: SessionSummary[];
  stats: CollectorStats;
  onSelectSession: (id: string) => void;
}

export const Overview: React.FC<OverviewProps> = ({ sessions, stats, onSelectSession }) => {
  const activeSessions = sessions.filter((s) => s.status === "running");
  const completedSessions = sessions.filter((s) => s.status === "completed");
  const failedSessions = sessions.filter((s) => s.status === "failed");

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      {/* Top metric cards */}
      <div className="grid grid-cols-4 gap-4">
        <div className="bg-surface border border-border p-4 rounded-xl">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs uppercase font-semibold">Active Sessions</span>
            <Activity className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-2xl font-bold text-slate-100">{activeSessions.length}</div>
          <div className="text-xs text-emerald-400 mt-1">Live tracking enabled</div>
        </div>

        <div className="bg-surface border border-border p-4 rounded-xl">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs uppercase font-semibold">Recorded Executions</span>
            <Database className="w-4 h-4 text-primary" />
          </div>
          <div className="text-2xl font-bold text-slate-100">{stats.stored_sessions_count}</div>
          <div className="text-xs text-slate-400 mt-1">SQLite durable history</div>
        </div>

        <div className="bg-surface border border-border p-4 rounded-xl">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs uppercase font-semibold">Telemetry Ingestion</span>
            <Flame className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-2xl font-bold text-slate-100">{stats.total_events_ingested}</div>
          <div className="text-xs text-slate-400 mt-1">0 events dropped</div>
        </div>

        <div className="bg-surface border border-border p-4 rounded-xl">
          <div className="flex items-center justify-between text-slate-400 mb-2">
            <span className="text-xs uppercase font-semibold">Collector Uptime</span>
            <Clock className="w-4 h-4 text-indigo-400" />
          </div>
          <div className="text-2xl font-bold text-slate-100">{stats.uptime_secs}s</div>
          <div className="text-xs text-slate-400 mt-1">Version 0.1.0 (All OS)</div>
        </div>
      </div>

      {/* Recent executions table */}
      <div className="bg-surface border border-border rounded-xl p-5">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-base font-semibold text-slate-100">Recent Adesh Tool Invocations</h2>
          <span className="text-xs text-slate-400">{sessions.length} sessions</span>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-border text-slate-400">
                <th className="pb-3 font-medium">Session ID</th>
                <th className="pb-3 font-medium">Tool</th>
                <th className="pb-3 font-medium">PID</th>
                <th className="pb-3 font-medium">Mode</th>
                <th className="pb-3 font-medium">Status</th>
                <th className="pb-3 font-medium">Duration</th>
                <th className="pb-3 font-medium">Peak Memory</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {sessions.map((s) => (
                <tr
                  key={s.session_id}
                  onClick={() => onSelectSession(s.session_id)}
                  className="hover:bg-slate-800/40 cursor-pointer transition-colors"
                >
                  <td className="py-3 font-mono text-primary font-medium">{s.session_id.substring(0, 8)}...</td>
                  <td className="py-3 text-slate-200 font-semibold">{s.tool_name}</td>
                  <td className="py-3 font-mono text-slate-400">{s.pid}</td>
                  <td className="py-3">
                    <span className="px-2 py-0.5 rounded bg-slate-800 text-slate-300 border border-slate-700">
                      {s.execution_mode}
                    </span>
                  </td>
                  <td className="py-3">
                    <span
                      className={`inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium ${
                        s.status === "completed"
                          ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20"
                          : s.status === "running"
                          ? "bg-primary/10 text-primary border border-primary/20 animate-pulse"
                          : "bg-red-500/10 text-red-400 border border-red-500/20"
                      }`}
                    >
                      {s.status}
                    </span>
                  </td>
                  <td className="py-3 font-mono text-slate-300">
                    {s.duration_ms ? `${s.duration_ms} ms` : "running..."}
                  </td>
                  <td className="py-3 font-mono text-slate-300">
                    {(s.peak_rss_bytes / (1024 * 1024)).toFixed(1)} MB
                  </td>
                </tr>
              ))}
              {sessions.length === 0 && (
                <tr>
                  <td colSpan={7} className="text-center py-8 text-slate-500">
                    No active or recorded sessions found. Run <code className="text-primary font-mono">adesh run &lt;file&gt;</code> to start monitoring.
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
