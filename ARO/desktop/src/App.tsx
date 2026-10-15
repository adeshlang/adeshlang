import React from "react";
import { ObservatoryProvider, useObservatory } from "./context/ObservatoryContext";
import { TopBar, ModernSidebar } from "./components/Navigation";
import { CockpitOverview } from "./pages/CockpitOverview";
import { StagesTimeline } from "./pages/StagesTimeline";
import { SecurityRedaction } from "./pages/SecurityRedaction";
import {
  ShieldCheck,
  Cpu,
  HardDrive,
  Terminal,
  Settings,
  Radio,
  Timer,
  ShieldEllipsis,
  Database,
  Network,
  Activity,
  Gauge,
  Sparkles,
  Lock,
  MemoryStick,
  Monitor,
  Layers,
  CheckCircle2,
  XCircle,
  Zap,
} from "lucide-react";

const formatBytes = (bytes?: number | null): string => {
  if (!bytes || bytes <= 0) return "—";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 ** 2;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${(bytes / 1024).toFixed(1)} KB`;
};

const MainLayout: React.FC = () => {
  const { state, dispatch, refreshNow, purgeHistory } = useObservatory();
  const {
    currentTab,
    sessions,
    selectedDetail,
    searchQuery,
    filterMode,
    ipcStatus,
    hardware,
    stats,
  } = state;

  const activeCount = sessions.filter((s) => s.status === "running").length;

  const gpus = hardware.gpus ?? [];
  const memTotal = hardware.total_memory_bytes ?? 0;
  const memAvail = hardware.available_memory_bytes ?? null;
  const memUsedPct =
    memAvail !== null && memTotal > 0 ? ((memTotal - memAvail) / memTotal) * 100 : null;

  return (
    <div className="flex flex-col h-screen w-screen bg-slate-950 text-slate-100 overflow-hidden font-sans">
      {/* Top Bar with Omni Search, Filter & Status */}
      <TopBar
        currentTab={currentTab}
        onSelectTab={(tab) => dispatch({ type: "SET_TAB", payload: tab })}
        activeCount={activeCount}
        ipcStatus={ipcStatus}
        onRefresh={refreshNow}
        onPurgeHistory={purgeHistory}
        searchQuery={searchQuery}
        onSearchChange={(q) => dispatch({ type: "SET_SEARCH_QUERY", payload: q })}
        filterMode={filterMode}
        onFilterModeChange={(m) => dispatch({ type: "SET_FILTER_MODE", payload: m })}
      />

      {/* Main Split Layout: Modern Sidebar + Dynamic View Cockpit */}
      <div className="flex flex-1 overflow-hidden">
        <ModernSidebar
          currentTab={currentTab}
          onSelectTab={(tab) => dispatch({ type: "SET_TAB", payload: tab })}
        />

        <main className="flex-1 overflow-hidden bg-slate-950/40">
          {currentTab === "overview" && <CockpitOverview />}

          {currentTab === "stage_timeline" && (
            <StagesTimeline
              sessionDetail={selectedDetail}
              onBack={() => dispatch({ type: "SET_TAB", payload: "overview" })}
            />
          )}

          {currentTab === "process_explorer" && <CockpitOverview />}

          {currentTab === "audit_logs" && <SecurityRedaction />}

          {currentTab === "linker_analytics" && (
            <div className="p-8 space-y-4">
              <h2 className="text-base font-bold text-slate-100 flex items-center space-x-2">
                <Terminal className="w-4 h-4 text-sky-400" />
                <span>Linker Diagnostics (`adeshlink`)</span>
              </h2>
              <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl space-y-3 text-xs">
                <p className="text-slate-300">
                  Direct PE/COFF, ELF, and Mach-O inspection. Zero external dependencies on LLVM or MSVC.
                </p>
                <div className="grid grid-cols-3 gap-3 pt-2 font-mono">
                  <div className="p-3 bg-slate-950 rounded border border-slate-800">
                    <div className="text-[10px] text-slate-500 uppercase">PE Relocations</div>
                    <div className="text-base text-sky-400 font-bold">Safe Folding</div>
                  </div>
                  <div className="p-3 bg-slate-950 rounded border border-slate-800">
                    <div className="text-[10px] text-slate-500 uppercase">Section GC</div>
                    <div className="text-base text-emerald-400 font-bold">Enabled</div>
                  </div>
                  <div className="p-3 bg-slate-950 rounded border border-slate-800">
                    <div className="text-[10px] text-slate-500 uppercase">ICF Fold Deduplication</div>
                    <div className="text-base text-indigo-400 font-bold">Active</div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {currentTab === "flame_graph" && (
            <div className="p-8 space-y-4">
              <h2 className="text-base font-bold text-slate-100">Live CPU & Allocation Flame Visualization</h2>
              <div className="bg-slate-900 border border-slate-800 p-8 rounded-xl text-center space-y-3">
                <p className="text-xs text-slate-400">
                  Folded stack sampling active. Allocator snapshots captured via non-blocking ring buffer.
                </p>
                <div className="w-full h-40 bg-slate-950 rounded-lg border border-slate-800 flex items-center justify-center font-mono text-xs text-sky-400">
                  [ Monitored Function Stacks: main -&gt; eval -&gt; parse_program -&gt; check_memory_safety ]
                </div>
              </div>
            </div>
          )}

          {currentTab === "hardware_caps" && (
            <div className="p-6 space-y-6 overflow-y-auto h-full">
              {/* Page Header */}
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-1 border-b border-slate-800/80">
                <div>
                  <h1 className="text-lg font-bold text-slate-100 tracking-tight flex items-center space-x-2 flex-wrap">
                    <Cpu className="w-5 h-5 text-sky-400" />
                    <span>Hardware &amp; Platform Discovery</span>
                    <span className="text-[10px] px-2 py-0.5 rounded bg-sky-500/10 text-sky-400 border border-sky-500/30 font-mono">
                      {hardware.os_name} · {hardware.arch}
                    </span>
                  </h1>
                  <p className="text-xs text-slate-400 mt-0.5">
                    Auto-discovered host capabilities feeding collector tuning decisions
                  </p>
                </div>
                <span className="flex items-center space-x-1.5 px-2.5 py-1 rounded-md bg-violet-500/10 border border-violet-500/30 text-violet-300 text-xs font-semibold shrink-0">
                  <Monitor className="w-3.5 h-3.5" />
                  <span>GPU Auto-Detection Active</span>
                </span>
              </div>

              {/* Top Metric Tiles */}
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                {/* Processor */}
                <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-amber-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
                  <div className="flex items-center justify-between text-slate-400 mb-2">
                    <span className="text-[11px] uppercase tracking-wider font-semibold">Processor</span>
                    <span className="p-1 rounded-md bg-amber-500/10 text-amber-400">
                      <Cpu className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
                    {hardware.physical_cores
                      ? `${hardware.physical_cores}C / ${hardware.logical_cores}T`
                      : `${hardware.logical_cores || "—"} Threads`}
                  </div>
                  <div className="text-xs text-slate-400 mt-2 truncate" title={hardware.cpu_name ?? ""}>
                    {hardware.cpu_name || "Detecting..."}
                  </div>
                </div>

                {/* System Memory */}
                <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-sky-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
                  <div className="flex items-center justify-between text-slate-400 mb-2">
                    <span className="text-[11px] uppercase tracking-wider font-semibold">System Memory</span>
                    <span className="p-1 rounded-md bg-sky-500/10 text-sky-400">
                      <MemoryStick className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
                    {formatBytes(memTotal)}
                  </div>
                  <div className="text-xs text-slate-400 mt-2">
                    {memAvail !== null ? (
                      <>
                        Available:{" "}
                        <span className="font-mono text-sky-400">{formatBytes(memAvail)}</span>
                      </>
                    ) : (
                      "Live sampling each refresh tick"
                    )}
                  </div>
                </div>

                {/* GPU Adapters */}
                <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-violet-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
                  <div className="flex items-center justify-between text-slate-400 mb-2">
                    <span className="text-[11px] uppercase tracking-wider font-semibold">GPU Adapters</span>
                    <span className="p-1 rounded-md bg-violet-500/10 text-violet-400">
                      <Monitor className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-3xl font-extrabold text-slate-100 font-mono tracking-tight">
                    {gpus.length}
                  </div>
                  <div className="text-xs text-slate-400 mt-2 truncate">
                    {gpus.length > 0 ? (
                      <>
                        Detected:{" "}
                        <span className="text-violet-300">
                          {gpus.map((g) => g.vendor || "GPU").join(", ")}
                        </span>
                      </>
                    ) : (
                      "No adapters reported"
                    )}
                  </div>
                </div>

                {/* Platform */}
                <div className="bg-gradient-to-br from-slate-900 via-slate-900 to-indigo-950/30 border border-slate-800 p-4 rounded-xl shadow-sm">
                  <div className="flex items-center justify-between text-slate-400 mb-2">
                    <span className="text-[11px] uppercase tracking-wider font-semibold">Platform</span>
                    <span className="p-1 rounded-md bg-indigo-500/10 text-indigo-400">
                      <Layers className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-2xl font-extrabold text-slate-100 font-mono tracking-tight">
                    {hardware.os_name}
                  </div>
                  <div className="text-xs text-slate-400 mt-2">
                    {hardware.arch === "x86_64"
                      ? "64-bit x86"
                      : hardware.arch === "aarch64"
                      ? "64-bit ARM"
                      : hardware.arch}{" "}
                    · <span className="font-mono text-indigo-400">{hardware.logical_cores}</span> threads
                  </div>
                </div>
              </div>

              {/* Two-Column Detail Grid */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start pb-4">
                {/* ===== LEFT COLUMN ===== */}
                <div className="space-y-6">
                  {/* Processor Details */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-amber-950/30 border-b border-slate-800 flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-amber-500/10 text-amber-400 border border-amber-500/30">
                        <Cpu className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">Processor Details</div>
                        <div className="text-[11px] text-slate-400">Topology as reported by the OS</div>
                      </div>
                    </div>
                    <div className="divide-y divide-slate-800/60 text-xs">
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Brand String</span>
                        <span className="font-semibold text-slate-200 text-right max-w-[60%] truncate" title={hardware.cpu_name ?? ""}>
                          {hardware.cpu_name || "Detecting..."}
                        </span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Physical Cores</span>
                        <span className="font-semibold text-slate-200 font-mono">
                          {hardware.physical_cores ?? "—"}
                        </span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Logical Threads</span>
                        <span className="font-semibold text-slate-200 font-mono">{hardware.logical_cores}</span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Max Clock Speed</span>
                        <span className="font-semibold text-sky-300 font-mono">
                          {hardware.cpu_max_clock_mhz
                            ? `${(hardware.cpu_max_clock_mhz / 1000).toFixed(2)} GHz`
                            : "Not exposed"}
                        </span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">ISA / Architecture</span>
                        <span className="font-semibold text-slate-200 font-mono">{hardware.arch}</span>
                      </div>
                    </div>
                  </div>

                  {/* System Memory */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-sky-950/30 border-b border-slate-800 flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-sky-500/10 text-sky-400 border border-sky-500/30">
                        <MemoryStick className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">System Memory</div>
                        <div className="text-[11px] text-slate-400">Physical RAM &amp; live availability</div>
                      </div>
                    </div>
                    <div className="divide-y divide-slate-800/60 text-xs">
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Total Capacity</span>
                        <span className="font-semibold text-slate-200 font-mono">{formatBytes(memTotal)}</span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">Available</span>
                        <span className="font-semibold text-sky-300 font-mono">
                          {memAvail !== null ? formatBytes(memAvail) : "—"}
                        </span>
                      </div>
                      <div className="px-4 py-3 flex justify-between items-center">
                        <span className="text-slate-400">In Use</span>
                        <span className="font-semibold text-slate-200 font-mono">
                          {memUsedPct !== null ? `${memUsedPct.toFixed(1)}%` : "—"}
                        </span>
                      </div>
                      {memUsedPct !== null && (
                        <div className="px-4 py-3">
                          <div className="flex items-center justify-between text-[10px] text-slate-500 mb-1.5">
                            <span>Memory Pressure</span>
                            <span
                              className={
                                memUsedPct >= 85
                                  ? "text-rose-400 font-semibold"
                                  : memUsedPct >= 65
                                  ? "text-amber-400 font-semibold"
                                  : "text-emerald-400 font-semibold"
                              }
                            >
                              {memUsedPct >= 85 ? "High" : memUsedPct >= 65 ? "Moderate" : "Healthy"}
                            </span>
                          </div>
                          <div className="h-2 rounded-full bg-slate-950 border border-slate-800 overflow-hidden">
                            <div
                              className={`h-full rounded-full transition-all duration-700 ${
                                memUsedPct >= 85
                                  ? "bg-gradient-to-r from-amber-500 to-rose-500"
                                  : memUsedPct >= 65
                                  ? "bg-gradient-to-r from-sky-500 to-amber-500"
                                  : "bg-gradient-to-r from-emerald-500 to-sky-500"
                              }`}
                              style={{ width: `${Math.min(100, memUsedPct).toFixed(1)}%` }}
                            ></div>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                </div>

                {/* ===== RIGHT COLUMN ===== */}
                <div className="space-y-6">
                  {/* GPU Accelerators */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-violet-950/30 border-b border-slate-800 flex items-center justify-between">
                      <div className="flex items-center space-x-3">
                        <span className="p-2 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/30">
                          <Monitor className="w-5 h-5" />
                        </span>
                        <div>
                          <div className="text-sm font-bold text-slate-100">GPU Accelerators</div>
                          <div className="text-[11px] text-slate-400">
                            Auto-detected display &amp; compute adapters
                          </div>
                        </div>
                      </div>
                      <span className="text-[10px] font-mono px-2 py-0.5 rounded-full bg-violet-500/10 border border-violet-500/30 text-violet-300">
                        {gpus.length} {gpus.length === 1 ? "device" : "devices"}
                      </span>
                    </div>

                    {gpus.length > 0 ? (
                      <div className="divide-y divide-slate-800/60">
                        {gpus.map((g, i) => (
                          <div
                            key={`${g.name}-${i}`}
                            className="p-4 flex items-start justify-between gap-3 hover:bg-slate-800/30 transition-colors group"
                          >
                            <div className="flex items-start space-x-3 min-w-0">
                              <span className="p-1.5 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/30 group-hover:scale-110 transition-transform shrink-0">
                                <Monitor className="w-4 h-4" />
                              </span>
                              <div className="min-w-0">
                                <div className="font-semibold text-slate-200 text-[13px] truncate" title={g.name}>
                                  {g.name}
                                </div>
                                <div className="flex items-center flex-wrap gap-1.5 mt-1.5">
                                  {g.vendor && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 border border-slate-700 font-semibold">
                                      {g.vendor}
                                    </span>
                                  )}
                                  {g.status && (
                                    <span className="flex items-center space-x-1 text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 font-mono">
                                      <span className="w-1 h-1 rounded-full bg-emerald-400 animate-pulse"></span>
                                      <span>{g.status}</span>
                                    </span>
                                  )}
                                </div>
                              </div>
                            </div>
                            <div className="text-right shrink-0">
                              <div className="font-mono text-[11px] text-sky-300 font-semibold">
                                {g.vram_bytes ? formatBytes(g.vram_bytes) : "Unified / Shared"}
                              </div>
                              <div className="text-[10px] text-slate-500 font-mono">
                                {g.driver_version ? `Driver ${g.driver_version}` : "Kernel driver N/A"}
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="p-6">
                        <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/40 p-6 text-center space-y-2">
                          <Monitor className="w-6 h-6 text-slate-500 mx-auto" />
                          <div className="text-xs font-semibold text-slate-300">
                            No GPU adapters reported by the host
                          </div>
                          <p className="text-[11px] text-slate-500 leading-relaxed">
                            The collector will rely on CPU &amp; memory metrics only. On VMs or
                            headless servers this is expected behaviour.
                          </p>
                        </div>
                      </div>
                    )}

                    <div className="px-4 py-2.5 bg-slate-950/60 border-t border-slate-800 flex items-center justify-between gap-3">
                      <span className="flex items-center space-x-1.5 text-[10px] text-slate-400 min-w-0">
                        <Activity className="w-3 h-3 text-violet-400 shrink-0" />
                        <span className="truncate" title={hardware.gpu_detection_method ?? ""}>
                          {hardware.gpu_detection_method || "Detection pending..."}
                        </span>
                      </span>
                    </div>
                  </div>

                  {/* Platform Capability Matrix */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-emerald-950/30 border-b border-slate-800 flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/30">
                        <ShieldCheck className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">Collector Capability Matrix</div>
                        <div className="text-[11px] text-slate-400">
                          Native OS providers available to the monitor
                        </div>
                      </div>
                    </div>
                    <div className="p-4 grid grid-cols-1 sm:grid-cols-2 gap-3">
                      {[
                        {
                          label: "Process CPU Sampling",
                          desc: "Per-tool CPU% via OS timers",
                          supported: hardware.supports_process_cpu,
                          icon: Cpu,
                          tone: "sky",
                        },
                        {
                          label: "Process Memory (RSS)",
                          desc: "Working-set & peak tracking",
                          supported: hardware.supports_process_memory,
                          icon: MemoryStick,
                          tone: "violet",
                        },
                        {
                          label: "Disk I/O Counters",
                          desc: "Read/write byte throughput",
                          supported: hardware.supports_disk_io,
                          icon: HardDrive,
                          tone: "amber",
                        },
                        {
                          label: "GPU / Accelerators",
                          desc: "Adapters discovered on host",
                          supported: hardware.supports_accelerators,
                          icon: Zap,
                          tone: "emerald",
                        },
                      ].map((cap) => {
                        const Icon = cap.icon;
                        const toneClasses: Record<string, string> = {
                          sky: "bg-sky-500/10 text-sky-400 border-sky-500/30",
                          violet: "bg-violet-500/10 text-violet-400 border-violet-500/30",
                          amber: "bg-amber-500/10 text-amber-400 border-amber-500/30",
                          emerald: "bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
                        };
                        return (
                          <div
                            key={cap.label}
                            className={`rounded-lg border p-3 flex items-center justify-between gap-2 transition-colors ${
                              cap.supported
                                ? "border-slate-800 bg-slate-950/40 hover:border-slate-700"
                                : "border-slate-800/60 bg-slate-950/20 opacity-70"
                            }`}
                          >
                            <div className="flex items-center space-x-2.5 min-w-0">
                              <span
                                className={`p-1.5 rounded-lg border shrink-0 ${toneClasses[cap.tone]}`}
                              >
                                <Icon className="w-4 h-4" />
                              </span>
                              <div className="min-w-0">
                                <div className="text-xs font-semibold text-slate-200 truncate">
                                  {cap.label}
                                </div>
                                <div className="text-[10px] text-slate-500 truncate">{cap.desc}</div>
                              </div>
                            </div>
                            {cap.supported ? (
                              <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                            ) : (
                              <XCircle className="w-4 h-4 text-slate-600 shrink-0" />
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {currentTab === "settings" && (
            <div className="p-6 space-y-6 overflow-y-auto h-full">
              {/* Page Header */}
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-1 border-b border-slate-800/80">
                <div>
                  <h1 className="text-lg font-bold text-slate-100 tracking-tight flex items-center space-x-2">
                    <Settings className="w-5 h-5 text-sky-400" />
                    <span>Collector Tuning</span>
                    <span className="text-[10px] px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono">
                      Zero-Config Defaults
                    </span>
                  </h1>
                  <p className="text-xs text-slate-400 mt-0.5">
                    Tune IPC transport, sampling cadence & privacy redaction for the observatory collector
                  </p>
                </div>
                <div className="flex items-center space-x-2">
                  <span
                    className={`flex items-center space-x-1.5 px-2.5 py-1 rounded-md border text-xs font-mono ${
                      ipcStatus === "connected"
                        ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-400"
                        : ipcStatus === "connecting"
                        ? "bg-amber-500/10 border-amber-500/30 text-amber-400"
                        : "bg-rose-500/10 border-rose-500/30 text-rose-400"
                    }`}
                  >
                    <Radio
                      className={`w-3.5 h-3.5 ${
                        ipcStatus === "connected" ? "animate-pulse" : ""
                      }`}
                    />
                    <span className="capitalize">{ipcStatus}</span>
                  </span>
                </div>
              </div>

              {/* Two-Column Grid */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 items-start">
                {/* ============ COLUMN 1: Collector Tuning Card ============ */}
                <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                  {/* Card Header Banner */}
                  <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-sky-950/30 border-b border-slate-800 flex items-center justify-between">
                    <div className="flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-sky-500/10 text-sky-400 border border-sky-500/30">
                        <Gauge className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">Active Configuration</div>
                        <div className="text-[11px] text-slate-400">
                          Production-grade defaults tuned for low-overhead capture
                        </div>
                      </div>
                    </div>
                    <Sparkles className="w-4 h-4 text-sky-400/60" />
                  </div>

                  {/* Setting Rows */}
                  <div className="divide-y divide-slate-800/60">
                    {/* IPC Endpoint */}
                    <div className="p-4 flex items-center justify-between hover:bg-slate-800/30 transition-colors group">
                      <div className="flex items-center space-x-3">
                        <span className="p-1.5 rounded-lg bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 group-hover:scale-110 transition-transform">
                          <Network className="w-4 h-4" />
                        </span>
                        <div>
                          <div className="font-semibold text-slate-200 text-[13px]">IPC Endpoint</div>
                          <div className="text-[10px] text-slate-500">
                            Windows Named Pipe &amp; Unix Domain Sockets
                          </div>
                        </div>
                      </div>
                      <span className="font-mono text-[11px] text-indigo-300 px-2.5 py-1 rounded-md bg-indigo-500/10 border border-indigo-500/30">
                        Auto-Discovered
                      </span>
                    </div>

                    {/* Sampling Rate */}
                    <div className="p-4 flex items-center justify-between hover:bg-slate-800/30 transition-colors group">
                      <div className="flex items-center space-x-3">
                        <span className="p-1.5 rounded-lg bg-sky-500/10 text-sky-400 border border-sky-500/30 group-hover:scale-110 transition-transform">
                          <Timer className="w-4 h-4" />
                        </span>
                        <div>
                          <div className="font-semibold text-slate-200 text-[13px]">Sampling Rate</div>
                          <div className="text-[10px] text-slate-500">
                            Resource metrics polling frequency
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center space-x-2">
                        <span className="font-mono text-[11px] text-sky-300 px-2.5 py-1 rounded-md bg-sky-500/10 border border-sky-500/30">
                          250 ms
                        </span>
                        <div className="flex space-x-0.5">
                          <span className="w-1 h-4 rounded-sm bg-sky-500/80"></span>
                          <span className="w-1 h-4 rounded-sm bg-sky-500/60"></span>
                          <span className="w-1 h-4 rounded-sm bg-sky-500/40"></span>
                          <span className="w-1 h-4 rounded-sm bg-sky-500/25"></span>
                          <span className="w-1 h-4 rounded-sm bg-sky-500/15"></span>
                        </div>
                      </div>
                    </div>

                    {/* Privacy & Token Redaction */}
                    <div className="p-4 flex items-center justify-between hover:bg-slate-800/30 transition-colors group">
                      <div className="flex items-center space-x-3">
                        <span className="p-1.5 rounded-lg bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 group-hover:scale-110 transition-transform">
                          <ShieldEllipsis className="w-4 h-4" />
                        </span>
                        <div>
                          <div className="font-semibold text-slate-200 text-[13px]">
                            Privacy &amp; Token Redaction
                          </div>
                          <div className="text-[10px] text-slate-500">
                            Mask sensitive CLI credentials automatically
                          </div>
                        </div>
                      </div>
                      <span className="flex items-center space-x-1.5 font-mono text-[11px] text-emerald-400 px-2.5 py-1 rounded-md bg-emerald-500/10 border border-emerald-500/30">
                        <Lock className="w-3 h-3" />
                        <span>Enforced</span>
                      </span>
                    </div>
                  </div>

                  {/* Card Footer Status Strip */}
                  <div className="px-4 py-2.5 bg-slate-950/60 border-t border-slate-800 flex items-center justify-between">
                    <span className="flex items-center space-x-1.5 text-[10px] text-slate-400">
                      <Activity className={`w-3 h-3 ${ipcStatus === "connected" ? "text-emerald-400" : "text-amber-400"}`} />
                      <span>Collector heartbeat</span>
                    </span>
                    <span className="text-[10px] font-mono text-slate-500">
                      {(stats.uptime_secs / 60).toFixed(1)} min uptime
                    </span>
                  </div>
                </div>

                {/* ============ COLUMN 2: Live Health & Platform Cards ============ */}
                <div className="space-y-6">
                  {/* Live Collector Health */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-indigo-950/30 border-b border-slate-800 flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-indigo-500/10 text-indigo-400 border border-indigo-500/30">
                        <Database className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">Live Collector Health</div>
                        <div className="text-[11px] text-slate-400">Real-time ingestion pipeline metrics</div>
                      </div>
                    </div>
                    <div className="grid grid-cols-2 divide-x divide-y divide-slate-800/60">
                      <div className="p-4">
                        <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1">
                          Events Ingested
                        </div>
                        <div className="text-xl font-extrabold text-slate-100 font-mono">
                          {stats.total_events_ingested.toLocaleString()}
                        </div>
                        <div className="text-[10px] text-emerald-400 mt-1">Zero-loss pipeline</div>
                      </div>
                      <div className="p-4">
                        <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1">
                          Events Dropped
                        </div>
                        <div className="text-xl font-extrabold text-slate-100 font-mono">
                          {stats.total_events_dropped}
                        </div>
                        <div className="text-[10px] text-slate-400 mt-1">Non-blocking ring buffers</div>
                      </div>
                      <div className="p-4">
                        <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1">
                          Active Sessions
                        </div>
                        <div className="text-xl font-extrabold text-sky-400 font-mono">
                          {stats.active_sessions_count}
                        </div>
                        <div className="text-[10px] text-slate-400 mt-1">Concurrent tool processes</div>
                      </div>
                      <div className="p-4">
                        <div className="text-[10px] text-slate-500 uppercase tracking-wider font-semibold mb-1">
                          Database Size
                        </div>
                        <div className="text-xl font-extrabold text-slate-100 font-mono">
                          {(stats.database_size_bytes / (1024 * 1024)).toFixed(2)} MB
                        </div>
                        <div className="text-[10px] text-slate-400 mt-1">SQLite WAL mode</div>
                      </div>
                    </div>
                  </div>

                  {/* Platform Context */}
                  <div className="bg-slate-900/80 border border-slate-800 rounded-xl overflow-hidden shadow-sm">
                    <div className="p-4 bg-gradient-to-r from-slate-950/80 via-slate-900/60 to-violet-950/30 border-b border-slate-800 flex items-center space-x-3">
                      <span className="p-2 rounded-lg bg-violet-500/10 text-violet-400 border border-violet-500/30">
                        <Cpu className="w-5 h-5" />
                      </span>
                      <div>
                        <div className="text-sm font-bold text-slate-100">Platform Context</div>
                        <div className="text-[11px] text-slate-400">Host capabilities influencing collector tuning</div>
                      </div>
                    </div>
                    <div className="p-4 space-y-3 text-xs">
                      <div className="flex justify-between items-center">
                        <span className="text-slate-400">Operating System</span>
                        <span className="font-semibold text-slate-200">{hardware.os_name}</span>
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-slate-400">Architecture</span>
                        <span className="font-semibold text-slate-200 font-mono">{hardware.arch}</span>
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-slate-400">Logical CPU Cores</span>
                        <span className="font-semibold text-slate-200 font-mono">{hardware.logical_cores}</span>
                      </div>
                      <div className="flex justify-between items-center">
                        <span className="text-slate-400">Collector Uptime</span>
                        <span className="font-semibold text-emerald-400 font-mono">
                          {(stats.uptime_secs / 60).toFixed(1)} min
                        </span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
};

export const App: React.FC = () => {
  return (
    <ObservatoryProvider>
      <MainLayout />
    </ObservatoryProvider>
  );
};

export default App;
