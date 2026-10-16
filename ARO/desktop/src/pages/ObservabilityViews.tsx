import React, { useMemo } from "react";
import { useObservatory } from "../context/ObservatoryContext";
import { SessionDetail, SessionSummary } from "../types/observatory";
import { Activity, Box, Cpu, GitBranch, MemoryStick, Network } from "lucide-react";
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

const mb = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);
const ms = (micros: number) => micros >= 1000 ? `${(micros / 1000).toFixed(2)} ms` : `${micros} µs`;

const Empty: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="rounded-xl border border-slate-800 bg-slate-900/70 p-10 text-center text-xs text-slate-400">{children}</div>
);

export const ProcessExplorer: React.FC = () => {
  const { state, selectSessionById } = useObservatory();
  const roots = useMemo(() => state.sessions.filter(s => !s.parent_pid || !state.sessions.some(p => p.pid === s.parent_pid)), [state.sessions]);
  const children = (parent: SessionSummary) => state.sessions.filter(s => s.parent_pid === parent.pid);
  return <div className="p-6 space-y-5 overflow-y-auto h-full">
    <div><h1 className="text-lg font-bold text-slate-100">Process Tree & VM Sessions</h1><p className="text-xs text-slate-400 mt-1">Parent/child telemetry topology and bytecode VM activity in one distinct view.</p></div>
    {roots.length ? <div className="space-y-2">{roots.map(root => <div key={root.session_id} className="rounded-xl border border-slate-800 bg-slate-900/70 overflow-hidden">
      <button onClick={() => selectSessionById(root.session_id, false)} className="w-full text-left p-4 hover:bg-slate-800/50 flex items-center gap-3"><GitBranch className="w-4 h-4 text-sky-400"/><span className="font-mono text-xs text-sky-300">{root.pid}</span><span className="font-semibold text-sm">{root.tool_name}</span><span className="text-xs text-slate-400">{root.execution_mode}</span><span className="ml-auto text-xs text-slate-500">{root.status}</span></button>
      {children(root).map(child => <button key={child.session_id} onClick={() => selectSessionById(child.session_id, false)} className="w-full border-t border-slate-800 pl-12 pr-4 py-3 text-left hover:bg-slate-800/50 flex items-center gap-3"><Network className="w-3.5 h-3.5 text-indigo-400"/><span className="font-mono text-xs text-indigo-300">{child.pid}</span><span className="text-xs">{child.tool_name}</span><span className="text-xs text-slate-400">{child.execution_mode}</span></button>)}
    </div>)}</div> : <Empty>No tracked process trees yet. Run an Adesh command to populate this view.</Empty>}
    {state.selectedDetail && <SessionDetails detail={state.selectedDetail} />}
  </div>;
};

export const LinkerDiagnostics: React.FC = () => {
  const { state, selectSessionById } = useObservatory();
  const links = state.sessions.filter(s => s.execution_mode === "linker" || s.execution_mode === "aot_native" || s.execution_mode === "native_compiler");
  const stages = state.selectedDetail?.linker_stages || [];
  return <div className="p-6 space-y-5 overflow-y-auto h-full"><div><h1 className="text-lg font-bold text-slate-100">Linker Diagnostics</h1><p className="text-xs text-slate-400 mt-1">Recorded link artifacts, target formats, symbols, sections, and failures.</p></div>
    {links.length ? <div className="rounded-xl border border-slate-800 overflow-hidden"><table className="w-full text-xs"><thead className="bg-slate-900 text-slate-400"><tr><th className="p-3 text-left">Session</th><th className="p-3 text-left">Target / command</th><th className="p-3 text-left">Status / exit</th><th className="p-3 text-right">Duration</th></tr></thead><tbody>{links.map(s => <tr key={s.session_id} onClick={() => selectSessionById(s.session_id, false)} className="border-t border-slate-800 hover:bg-slate-900/70 cursor-pointer"><td className="p-3 font-mono text-sky-300">{s.session_id.slice(0, 12)}</td><td className="p-3 max-w-[28rem] truncate" title={s.command_line}>{s.command_line}</td><td className={s.status === "completed" ? "p-3 text-emerald-400" : "p-3 text-rose-400"}>{s.status}{s.exit_code != null ? ` · ${s.exit_code}` : ""}</td><td className="p-3 text-right">{s.duration_ms ?? 0} ms</td></tr>)}</tbody></table></div> : <Empty>No AOT or linker sessions have been recorded.</Empty>}
    {state.selectedDetail && <div className="rounded-xl border border-slate-800 bg-slate-900/70 p-4 space-y-3"><h2 className="font-semibold text-sm flex gap-2 items-center"><Box className="w-4 h-4 text-violet-400"/>Selected session link stages</h2>{stages.length ? stages.map((s,i) => <div key={i} className="grid grid-cols-2 md:grid-cols-5 gap-3 text-xs border-t border-slate-800 pt-3"><span className="font-mono text-slate-200">{s.stage_name}</span><span>{s.target_triple}</span><span>{s.output_format}</span><span>{s.symbol_count} symbols / {s.section_count} sections</span><span className={s.success ? "text-emerald-400" : "text-rose-400"}>{s.success ? `${s.output_size_bytes ?? 0} bytes` : s.error_message}</span></div>) : <p className="text-xs text-slate-500">Select an `adeshlink` session to inspect its recorded stages.</p>}</div>}
    {state.selectedDetail && <SessionDetails detail={state.selectedDetail} />}
  </div>;
};

const SessionDetails: React.FC<{ detail: SessionDetail }> = ({ detail }) => {
  const { summary } = detail;
  const fields = [
    ["Command", summary.command_line], ["Working directory", detail.working_dir], ["Target file", detail.target_file || "—"],
    ["PID / parent", `${summary.pid}${summary.parent_pid ? ` / ${summary.parent_pid}` : ""}`], ["Mode", summary.execution_mode],
    ["Exit code", summary.exit_code == null ? "still running" : String(summary.exit_code)], ["Started", new Date(summary.start_time).toLocaleString()],
    ["Finished", summary.end_time ? new Date(summary.end_time).toLocaleString() : "—"], ["Duration", summary.duration_ms == null ? "—" : `${summary.duration_ms} ms`],
    ["Peak RSS", `${mb(summary.peak_rss_bytes)} MB`], ["Peak CPU", `${summary.cpu_peak_percent.toFixed(1)}%`], ["Metric samples", String(detail.metrics_count)],
  ];
  return <div className="rounded-xl border border-slate-800 bg-slate-900/70 p-4 space-y-3"><h2 className="text-sm font-semibold text-slate-100">Execution details</h2><div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 text-xs">{fields.map(([label, value]) => <div key={label} className="rounded-lg border border-slate-800 bg-slate-950/50 p-3"><div className="text-slate-500 mb-1">{label}</div><div className="font-mono text-slate-200 break-all">{value}</div></div>)}</div></div>;
};

export const FlameTelemetry: React.FC = () => {
  const { state } = useObservatory();
  const metrics = state.selectedMetrics;
  const data = metrics.map((m, i) => ({ sample: i + 1, cpu: m.cpu_percent, rss: Number(mb(m.memory_rss_bytes)), threads: m.thread_count }));
  const maxCpu = Math.max(0, ...metrics.map(m => m.cpu_percent));
  const maxThreads = Math.max(0, ...metrics.map(m => m.peak_thread_count || m.thread_count));
  const maxVms = Math.max(0, ...metrics.map(m => m.memory_vms_bytes));
  const coresUsed = maxCpu / 100;
  const memoryInventory = state.selectedDetail?.compiler_stages.find(stage => stage.stage_name === "memory_efficiency_inventory")?.metadata;
  return <div className="p-6 space-y-5 overflow-y-auto h-full"><div><h1 className="text-lg font-bold text-slate-100">CPU & Memory Diagnostics</h1><p className="text-xs text-slate-400 mt-1">Actual OS process telemetry plus compile-time allocation, borrowing, and RAII analysis for the selected session.</p></div>
    {state.selectedDetail ? <><div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-3 text-xs"><Metric icon={<Cpu/>} label="CPU peak" value={`${maxCpu.toFixed(1)}%`}/><Metric icon={<Cpu/>} label="Cores used" value={`${coresUsed.toFixed(2)} / ${state.hardware.logical_cores || "—"}`}/><Metric icon={<MemoryStick/>} label="Total RSS used" value={`${mb(Math.max(0,...metrics.map(m => m.memory_peak_rss_bytes)))} MB`}/><Metric icon={<MemoryStick/>} label="Virtual memory" value={`${mb(maxVms)} MB`}/><Metric icon={<Activity/>} label="Peak threads" value={`${maxThreads}`}/><Metric icon={<Activity/>} label="Samples" value={`${metrics.length}`}/></div>
      {data.length ? <div className="rounded-xl border border-slate-800 bg-slate-900/70 p-4 h-72"><ResponsiveContainer width="100%" height="100%"><AreaChart data={data}><XAxis dataKey="sample"/><YAxis/><Tooltip/><Area dataKey="cpu" stroke="#38bdf8" fill="#0ea5e955"/><Area dataKey="rss" stroke="#a78bfa" fill="#8b5cf655"/></AreaChart></ResponsiveContainer></div> : <Empty>No process samples were captured for this session. Longer-running processes are sampled by the collector.</Empty>}</> : <Empty>Select a session from Cockpit or Process Tree to inspect its actual CPU and memory telemetry.</Empty>}
      {memoryInventory && <div className="rounded-xl border border-emerald-500/20 bg-emerald-950/10 p-5 space-y-3"><div><h2 className="text-sm font-semibold text-emerald-200">Ownership, Borrowing & RAII Efficiency</h2><p className="text-xs text-slate-400 mt-1">Static compiler inventory. RSS/virtual memory above are measured from the operating system.</p></div><div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs"><Metric icon={<MemoryStick/>} label="Static source" value={`${memoryInventory.static_source_bytes || "0"} B`}/><Metric icon={<MemoryStick/>} label="Dynamic request sites" value={memoryInventory.dynamic_allocation_sites || "0"}/><Metric icon={<Activity/>} label="Borrow operations" value={memoryInventory.borrow_operations || "0"}/><Metric icon={<Activity/>} label="RAII cleanup" value={memoryInventory.raii_transform_applied === "true" ? "Applied" : "Unavailable"}/></div><p className="text-[11px] text-slate-400">Borrowing avoids ownership copies where valid; RAII injects deterministic cleanup at scope exit. Allocation reuse and saved bytes are reported only when the runtime allocator supplies measured counters, never guessed from source text.</p></div>}
  </div>;
};

const Metric: React.FC<{icon: React.ReactNode; label: string; value: string}> = ({icon,label,value}) => <div className="rounded-xl border border-slate-800 bg-slate-900 p-4"><div className="flex gap-2 text-slate-400 [&_svg]:w-4 [&_svg]:h-4">{icon}<span>{label}</span></div><div className="text-xl font-mono font-bold mt-2">{value}</div></div>;

export const BenchmarkRegressions: React.FC = () => {
  const { state } = useObservatory();
  const comparisons = useMemo(() => Object.entries(state.sessions.reduce<Record<string, SessionSummary[]>>((a,s) => { const key=`${s.tool_name}:${s.execution_mode}`; (a[key] ||= []).push(s); return a; }, {})).map(([key, runs]) => {
    const completed = runs.filter(r => r.duration_ms != null).sort((a,b) => new Date(b.start_time).getTime()-new Date(a.start_time).getTime());
    const current=completed[0]?.duration_ms; const baseline=completed.slice(1).map(r=>r.duration_ms!).sort((a,b)=>a-b); const median=baseline.length ? baseline[Math.floor(baseline.length/2)] : undefined;
    return { key, current, median, delta: current != null && median ? ((current-median)/median)*100 : undefined, samples: completed.length };
  }), [state.sessions]);
  return <div className="p-6 space-y-5 overflow-y-auto h-full"><div><h1 className="text-lg font-bold text-slate-100">Benchmark Regressions</h1><p className="text-xs text-slate-400 mt-1">Latest completed invocation compared with the median of prior runs in its tool/runtime cohort.</p></div>
    {comparisons.length ? <div className="rounded-xl border border-slate-800 overflow-hidden"><table className="w-full text-xs"><thead className="bg-slate-900 text-slate-400"><tr><th className="p-3 text-left">Cohort</th><th className="p-3 text-right">Latest</th><th className="p-3 text-right">Baseline median</th><th className="p-3 text-right">Change</th><th className="p-3 text-right">Runs</th></tr></thead><tbody>{comparisons.map(c => <tr key={c.key} className="border-t border-slate-800"><td className="p-3 font-mono">{c.key}</td><td className="p-3 text-right">{c.current ?? "—"} ms</td><td className="p-3 text-right">{c.median ?? "—"} ms</td><td className={`p-3 text-right font-semibold ${c.delta != null && c.delta > 5 ? "text-rose-400" : "text-emerald-400"}`}>{c.delta == null ? "needs baseline" : `${c.delta > 0 ? "+" : ""}${c.delta.toFixed(1)}%`}</td><td className="p-3 text-right">{c.samples}</td></tr>)}</tbody></table></div> : <Empty>Run at least one program to begin collecting benchmark history.</Empty>}
  </div>;
};
