import React, { useMemo, useState } from "react";
import { useObservatory } from "../context/ObservatoryContext";
import { RedactionRuleInfo } from "../types/observatory";
import {
  ShieldCheck,
  ShieldEllipsis,
  ScrollText,
  KeyRound,
  ScanSearch,
  RefreshCw,
  Database,
  Lock,
  AlertTriangle,
  CheckCircle2,
  Terminal,
  Clock,
  EyeOff,
} from "lucide-react";

const SEVERITY_STYLES: Record<string, { label: string; chip: string; dot: string }> = {
  critical: {
    label: "Critical",
    chip: "bg-rose-500/10 text-rose-300 border-rose-500/30",
    dot: "bg-rose-400",
  },
  high: {
    label: "High",
    chip: "bg-amber-500/10 text-amber-300 border-amber-500/30",
    dot: "bg-amber-400",
  },
  medium: {
    label: "Medium",
    chip: "bg-sky-500/10 text-sky-300 border-sky-500/30",
    dot: "bg-sky-400",
  },
};

const STATUS_STYLES: Record<string, string> = {
  running: "bg-sky-500/10 text-sky-300 border-sky-500/30",
  completed: "bg-emerald-500/10 text-emerald-300 border-emerald-500/30",
  failed: "bg-rose-500/10 text-rose-300 border-rose-500/30",
  terminated: "bg-slate-500/10 text-slate-300 border-slate-500/30",
};

const timeAgo = (iso: string): string => {
  const secs = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
};

export const SecurityRedaction: React.FC = () => {
  const { state, rescanHistory } = useObservatory();
  const { sessions, securityReport, searchQuery } = state;
  const [rescanning, setRescanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);

  const handleRescan = async () => {
    setRescanning(true);
    setScanError(null);
    try {
      await rescanHistory();
    } catch (e) {
      setScanError("Rescan failed — collector unreachable?");
    } finally {
      setRescanning(false);
    }
  };

  const rules: RedactionRuleInfo[] = securityReport?.rules ?? [];
  const findingsByRule = securityReport?.findings_by_rule ?? {};
  const totalRedactions = securityReport?.total_redactions ?? 0;
  const sessionsScanned = securityReport?.total_sessions_scanned ?? 0;
  const sessionsWithSecrets = securityReport?.sessions_with_redactions ?? 0;
  const auditEntries = securityReport?.audit_entries ?? [];

  const auditedSessions = useMemo(() => {
    const q = searchQuery.toLowerCase();
    return [...sessions]
      .sort((a, b) => new Date(b.start_time).getTime() - new Date(a.start_time).getTime())
      .filter(
        (s) =>
          !q ||
          s.session_id.toLowerCase().includes(q) ||
          s.tool_name.toLowerCase().includes(q) ||
          s.command_line.toLowerCase().includes(q)
      )
      .slice(0, 50);
  }, [sessions, searchQuery]);

  const metricTiles = [
    {
      label: "Secrets Masked",
      value: securityReport ? totalRedactions.toLocaleString() : "—",
      sub: securityReport?.ingest_redaction_enabled ? "Enforced at ingest" : "Passive scan",
      icon: EyeOff,
      tone: "emerald",
    },
    {
      label: "Sessions Scanned",
      value: securityReport ? sessionsScanned.toLocaleString() : "—",
      sub: "Command lines screened",
      icon: ScanSearch,
      tone: "sky",
    },
    {
      label: "Sessions w/ Secrets",
      value: securityReport ? sessionsWithSecrets.toLocaleString() : "—",
      sub: sessionsScanned > 0
        ? `${((sessionsWithSecrets / sessionsScanned) * 100).toFixed(1)}% of history`
        : "No history yet",
      icon: AlertTriangle,
      tone: "rose",
    },
    {
      label: "Active Rules",
      value: securityReport ? rules.length.toString() : "—",
      sub: "Built-in detection patterns",
      icon: ShieldCheck,
      tone: "violet",
    },
  ];

  const toneClasses: Record<string, string> = {
    emerald: "bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
    sky: "bg-sky-500/10 text-sky-400 border-sky-500/30",
    rose: "bg-rose-500/10 text-rose-400 border-rose-500/30",
    violet: "bg-violet-500/10 text-violet-400 border-violet-500/30",
  };

  const gradientTones: Record<string, string> = {
    emerald: "from-slate-900 via-slate-900 to-emerald-950/30",
    sky: "from-slate-900 via-slate-900 to-sky-950/30",
    rose: "from-slate-900 via-slate-900 to-rose-950/30",
    violet: "from-slate-900 via-slate-900 to-violet-950/30",
  };

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      {/* Page Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-1 border-b border-slate-800/80">
        <div>
          <h1 className="text-lg font-bold text-slate-100 tracking-tight flex items-center space-x-2 flex-wrap">
            <ShieldCheck className="w-5 h-5 text-emerald-400" />
            <span>Security &amp; Redaction</span>
            <span className="text-[10px] px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 font-mono">
              {securityReport?.ingest_redaction_enabled
                ? "Enforced at Ingest"
                : "Audit Scanner"}
            </span>
          </h1>
          <p className="text-xs text-slate-400 mt-0.5">
            Secrets are masked before telemetry touches disk — plus a full audit trail of every redaction
          </p>
        </div>
        <div className="flex items-center space-x-2">
          {scanError && (
            <span className="text-[10px] text-rose-400 font-mono">{scanError}</span>
          )}
          <button
            onClick={handleRescan}
            disabled={rescanning}
            className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-lg bg-emerald-500/10 hover:bg-emerald-500/20 border border-emerald-500/30 text-emerald-300 hover:text-emerald-200 transition-colors text-xs font-medium disabled:opacity-60 disabled:cursor-not-allowed"
            title="Re-run the redaction engine over stored history (masks secrets persisted by older builds)"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${rescanning ? "animate-spin" : ""}`} />
            <span>{rescanning ? "Rescanning…" : "Rescan History"}</span>
          </button>
        </div>
      </div>

      {/* Metric Tiles */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {metricTiles.map((tile) => {
          const Icon = tile.icon;
          return (
            <div
              key={tile.label}
              className={`bg-gradient-to-br ${gradientTones[tile.tone]} border border-slate-800 p-4 rounded-xl shadow-sm`}
            >
              <div className="flex items-center justify-between text-slate-400 mb-2">
                <span className="text-[11px] uppercase tracking-wider font-semibold">{tile.label}</span>
                <span className={`p-1 rounded-md border ${toneClasses[tile.tone]}`}>
                  <Icon className="w-4 h-4" />
                </span>
              </div>
              <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
                {tile.value}
              </div>
              <div className="text-xs text-slate-400 mt-2">{tile.sub}</div>
            </div>
          );
        })}
      </div>

      {/* Policy + Audit Trail */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start">
        {/* Redaction Policy */}
        <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
          <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-emerald-950/30 border-b border-slate-800 flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <span className="p-2 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
                <KeyRound className="w-5 h-5" />
              </span>
              <div>
                <div className="text-sm font-bold text-slate-100">Redaction Policy</div>
                <div className="text-[11px] text-slate-400">
                  Built-in secret detection rules, always on
                </div>
              </div>
            </div>
            <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-emerald-500/10 border border-emerald-500/30 text-emerald-300">
              {rules.length} rules
            </span>
          </div>
          {rules.length > 0 ? (
            <div className="divide-y divide-slate-800/60">
              {rules.map((rule) => {
                const severity = SEVERITY_STYLES[rule.severity] ?? SEVERITY_STYLES.medium;
                const matches = findingsByRule[rule.id] ?? 0;
                return (
                  <div
                    key={rule.id}
                    className="p-4 flex items-start justify-between gap-3 hover:bg-slate-800/30 transition-colors group"
                  >
                    <div className="flex items-start space-x-3 min-w-0">
                      <span className="p-1.5 rounded-lg bg-slate-950/60 border border-slate-700 text-emerald-400 group-hover:scale-110 transition-transform shrink-0 mt-0.5">
                        <ShieldEllipsis className="w-4 h-4" />
                      </span>
                      <div className="min-w-0">
                        <div className="flex items-center space-x-2 flex-wrap">
                          <span className="font-semibold text-slate-200 text-[13px]">{rule.label}</span>
                          <span className={`text-[9px] px-1.5 py-0.5 rounded border font-semibold uppercase tracking-wide ${severity.chip}`}>
                            <span className={`inline-block w-1.5 h-1.5 rounded-full mr-1 ${severity.dot}`}></span>
                            {severity.label}
                          </span>
                        </div>
                        <div className="text-[11px] text-slate-500 mt-1 leading-relaxed">
                          {rule.description}
                        </div>
                        <div className="mt-1.5 font-mono text-[10px] text-slate-400 px-1.5 py-0.5 rounded bg-slate-950/60 border border-slate-800 inline-block">
                          e.g. {rule.example}
                        </div>
                      </div>
                    </div>
                    <div className="text-right shrink-0">
                      <div
                        className={`font-mono text-sm font-bold ${
                          matches > 0 ? "text-emerald-300" : "text-slate-600"
                        }`}
                      >
                        {matches}
                      </div>
                      <div className="text-[9px] text-slate-500 uppercase tracking-wide">hits</div>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="p-6">
              <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/40 p-6 text-center space-y-2">
                <KeyRound className="w-6 h-6 text-slate-500 mx-auto" />
                <div className="text-xs font-semibold text-slate-300">
                  Awaiting collector connection
                </div>
                <p className="text-[11px] text-slate-500 leading-relaxed">
                  Redaction rules load once the native backend reports in.
                </p>
              </div>
            </div>
          )}
          <div className="px-4 py-2.5 bg-slate-950/60 border-t border-slate-800 flex items-center justify-between">
            <span className="flex items-center space-x-1.5 text-[10px] text-slate-400">
              <Lock className="w-3 h-3 text-emerald-400" />
              <span>Masking runs before SQLite persistence</span>
            </span>
            <span className="text-[10px] font-mono text-slate-500">zero raw secrets at rest</span>
          </div>
        </div>

        {/* Audit Trail */}
        <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
          <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-rose-950/30 border-b border-slate-800 flex items-center justify-between">
            <div className="flex items-center space-x-3">
              <span className="p-2 rounded-lg bg-rose-500/10 text-rose-400 border border-rose-500/30">
                <ScrollText className="w-5 h-5" />
              </span>
              <div>
                <div className="text-sm font-bold text-slate-100">Redaction Audit Trail</div>
                <div className="text-[11px] text-slate-400">
                  Every secret masked, newest first
                </div>
              </div>
            </div>
            <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-rose-500/10 border border-rose-500/30 text-rose-300">
              {auditEntries.length} shown
            </span>
          </div>
          {auditEntries.length > 0 ? (
            <div className="divide-y divide-slate-800/60 max-h-[420px] overflow-y-auto">
              {auditEntries.map((entry) => (
                <div
                  key={entry.id}
                  className="p-4 flex items-start justify-between gap-3 hover:bg-slate-800/30 transition-colors"
                >
                  <div className="flex items-start space-x-3 min-w-0">
                    <span className="p-1.5 rounded-lg bg-rose-500/10 text-rose-400 border border-rose-500/30 shrink-0 mt-0.5">
                      <EyeOff className="w-4 h-4" />
                    </span>
                    <div className="min-w-0">
                      <div className="flex items-center space-x-2 flex-wrap">
                        <span className="font-mono text-[11px] text-rose-300 px-1.5 py-0.5 rounded bg-rose-500/10 border border-rose-500/30">
                          {entry.masked_preview}
                        </span>
                        <span className="text-[11px] text-slate-400">{entry.rule_label}</span>
                      </div>
                      <div className="flex items-center space-x-2 mt-1.5 text-[10px] text-slate-500">
                        <Terminal className="w-3 h-3 shrink-0" />
                        <span className="font-semibold text-slate-300">{entry.tool_name || "unknown"}</span>
                        <span className="font-mono">· {entry.session_id.slice(0, 8)}</span>
                        <span className="flex items-center space-x-0.5">
                          <Clock className="w-3 h-3" />
                          <span>{timeAgo(entry.redacted_at)}</span>
                        </span>
                      </div>
                    </div>
                  </div>
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0 mt-1" />
                </div>
              ))}
            </div>
          ) : (
            <div className="p-6">
              <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/40 p-6 text-center space-y-2">
                <ScrollText className="w-6 h-6 text-slate-500 mx-auto" />
                <div className="text-xs font-semibold text-slate-300">
                  {sessions.length === 0
                    ? "No telemetry captured yet"
                    : "No secrets detected in captured history"}
                </div>
                <p className="text-[11px] text-slate-500 leading-relaxed">
                  {sessions.length === 0
                    ? "The audit trail fills in as the collector records tool sessions."
                    : "Every stored command line in the current history passed all redaction rules cleanly."}
                </p>
              </div>
            </div>
          )}
          <div className="px-4 py-2.5 bg-slate-950/60 border-t border-slate-800 flex items-center justify-between">
            <span className="flex items-center space-x-1.5 text-[10px] text-slate-400">
              <ScrollText className="w-3 h-3 text-rose-400" />
              <span>Stored in SQLite redaction_events table</span>
            </span>
            <span className="text-[10px] font-mono text-slate-500">
              {securityReport ? `${securityReport.rescan_count} rescans` : "—"}
            </span>
          </div>
        </div>
      </div>

      {/* Session Command Audit */}
      <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-sky-950/30 border-b border-slate-800 flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <span className="p-2 rounded-lg bg-sky-500/10 text-sky-400 border border-sky-500/30">
              <Terminal className="w-5 h-5" />
            </span>
            <div>
              <div className="text-sm font-bold text-slate-100">Session Command Audit</div>
              <div className="text-[11px] text-slate-400">
                Persisted command lines — already masked, safe to share
              </div>
            </div>
          </div>
          {searchQuery && (
            <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-sky-500/10 border border-sky-500/30 text-sky-300">
              filtered by omni-search
            </span>
          )}
        </div>
        {auditedSessions.length > 0 ? (
          <div className="divide-y divide-slate-800/60 max-h-[420px] overflow-y-auto">
            {auditedSessions.map((session) => {
              const masked = session.redaction_count ?? 0;
              return (
                <div
                  key={session.session_id}
                  className="px-4 py-3 flex items-center justify-between gap-3 hover:bg-slate-800/30 transition-colors"
                >
                  <div className="flex items-center space-x-3 min-w-0 flex-1">
                    <span
                      className={`p-1.5 rounded-lg border shrink-0 ${
                        masked > 0
                          ? "bg-rose-500/10 text-rose-400 border-rose-500/30"
                          : "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                      }`}
                    >
                      {masked > 0 ? <EyeOff className="w-4 h-4" /> : <CheckCircle2 className="w-4 h-4" />}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="font-mono text-[11px] text-slate-200 truncate" title={session.command_line}>
                        {session.command_line || "—"}
                      </div>
                      <div className="flex items-center space-x-2 mt-1 text-[10px] text-slate-500">
                        <span className="font-semibold text-slate-400">{session.tool_name}</span>
                        <span className="font-mono">{session.session_id.slice(0, 8)}</span>
                        <span>{timeAgo(session.start_time)}</span>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center space-x-2 shrink-0">
                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded border ${
                        masked > 0
                          ? "bg-rose-500/10 text-rose-300 border-rose-500/30"
                          : "bg-emerald-500/10 text-emerald-300 border-emerald-500/30"
                      }`}
                      title={
                        masked > 0
                          ? `${masked} secret(s) masked in this command line`
                          : "No secrets found in this command line"
                      }
                    >
                      {masked > 0 ? `${masked} masked` : "clean"}
                    </span>
                    <span
                      className={`text-[10px] font-mono px-2 py-0.5 rounded border capitalize ${
                        STATUS_STYLES[session.status] ?? STATUS_STYLES.terminated
                      }`}
                    >
                      {session.status}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="p-6">
            <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/40 p-6 text-center space-y-2">
              <Terminal className="w-6 h-6 text-slate-500 mx-auto" />
              <div className="text-xs font-semibold text-slate-300">
                {searchQuery ? "No sessions match the current search" : "No sessions captured yet"}
              </div>
              <p className="text-[11px] text-slate-500 leading-relaxed">
                {searchQuery
                  ? "Clear the omni-search box in the top bar to see the full audit list."
                  : "Run an Adesh tool under the collector to populate the command audit."}
              </p>
            </div>
          </div>
        )}
      </div>

      {/* Data at Rest */}
      <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
        <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-violet-950/30 border-b border-slate-800 flex items-center space-x-3">
          <span className="p-2 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/30">
            <Database className="w-5 h-5" />
          </span>
          <div>
            <div className="text-sm font-bold text-slate-100">Data at Rest</div>
            <div className="text-[11px] text-slate-400">
              Where the redacted audit database lives on this machine
            </div>
          </div>
        </div>
        <div className="p-4 grid grid-cols-1 sm:grid-cols-3 gap-4 text-xs">
          <div className="sm:col-span-2">
            <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1.5">
              Audit Database Path
            </div>
            <div className="font-mono text-[11px] text-slate-200 px-2.5 py-2 rounded-md bg-slate-950/60 border border-slate-800 break-all">
              {securityReport?.db_path || "Awaiting collector connection…"}
            </div>
          </div>
          <div className="space-y-3">
            <div>
              <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1.5">
                Last Manual Rescan
              </div>
              <div className="font-mono text-[11px] text-emerald-300">
                {securityReport?.last_rescan
                  ? timeAgo(securityReport.last_rescan)
                  : "Never — ingest masking always on"}
              </div>
            </div>
            <div>
              <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1.5">
                Purge Control
              </div>
              <div className="font-mono text-[11px] text-slate-400">
                Use “Clear Dashboard” (top bar) to wipe the audit trail
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
