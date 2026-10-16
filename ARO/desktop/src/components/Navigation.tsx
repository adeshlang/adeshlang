import React, { useState } from "react";
import {
  Activity,
  Layers,
  Terminal,
  Cpu,
  Database,
  Settings,
  Flame,
  Search,
  Filter,
  RefreshCw,
  SlidersHorizontal,
  Download,
  Shield,
  Zap,
  Radio,
  FileCode,
  Box,
  Compass,
  BarChart3,
  Trash2,
} from "lucide-react";

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

interface NavigationProps {
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  activeCount: number;
  ipcStatus: "connected" | "disconnected" | "connecting";
  onRefresh: () => void;
  onPurgeHistory: () => void;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  filterMode: string;
  onFilterModeChange: (m: string) => void;
}

export const TopBar: React.FC<NavigationProps> = ({
  currentTab,
  onSelectTab,
  activeCount,
  ipcStatus,
  onRefresh,
  onPurgeHistory,
  searchQuery,
  onSearchChange,
  filterMode,
  onFilterModeChange,
}) => {
  return (
    <header className="h-14 border-b border-border/80 bg-slate-950/80 backdrop-blur-md px-4 flex items-center justify-between select-none z-30 sticky top-0">
      <div className="flex items-center space-x-3">
        <div className="w-8 h-8 rounded-lg overflow-hidden shadow-lg shadow-sky-500/20 border border-sky-500/30 flex items-center justify-center bg-slate-900">
          <img src="/logo.png" alt="Adesh Logo" className="w-7 h-7 object-contain" />
        </div>
        <div>
          <div className="flex items-center space-x-2">
            <span className="font-bold text-sm tracking-tight text-slate-100 font-sans">
              Adesh Observatory
            </span>
            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-sky-500/10 text-sky-400 border border-sky-500/30">
              v0.1.0-ULTRA
            </span>
          </div>
          <p className="text-[10px] text-slate-400">Universal Runtime & Toolchain Telemetry</p>
        </div>
      </div>

      {/* Global omni-search & mode filters */}
      <div className="hidden sm:flex items-center space-x-2 md:space-x-3 w-1/3 max-w-xs md:max-w-md">
        <div className="relative w-full">
          <Search className="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="Search PID, Session UUID, Tool..."
            className="w-full pl-9 pr-3 py-1.5 bg-slate-900/90 border border-slate-800 rounded-lg text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 transition-all"
          />
        </div>

        <select
          value={filterMode}
          onChange={(e) => onFilterModeChange(e.target.value)}
          className="hidden md:block bg-slate-900 border border-slate-800 text-xs text-slate-300 rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-sky-500 cursor-pointer"
        >
          <option value="all">All Modes</option>
          <option value="jit">JIT / Native JIT</option>
          <option value="interpreter">Interpreter</option>
          <option value="aot_native">AOT Native</option>
          <option value="native_compiler">Native Compiler</option>
          <option value="linker">adeshlink</option>
          <option value="web_assembly">Wasm</option>
        </select>
      </div>

      {/* System Status Indicators */}
      <div className="flex items-center space-x-2 md:space-x-3">
        <div className="hidden lg:flex items-center space-x-2 px-2.5 py-1 rounded-md bg-slate-900 border border-slate-800 text-xs">
          <Radio
            className={`w-3.5 h-3.5 ${
              ipcStatus === "connected" ? "text-emerald-400 animate-pulse" : "text-amber-400"
            }`}
          />
          <span className="font-mono text-slate-300 capitalize">{ipcStatus}</span>
        </div>

        <div className="flex items-center space-x-1 px-2.5 py-1 rounded-md bg-sky-500/10 border border-sky-500/20 text-xs text-sky-400">
          <Zap className="w-3.5 h-3.5" />
          <span className="font-semibold">{activeCount} <span className="hidden sm:inline">Active</span></span>
        </div>

        <button
          onClick={onPurgeHistory}
          className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-300 hover:text-rose-200 transition-colors text-xs font-medium"
          title="Clear all sessions and wipe observatory database history"
        >
          <Trash2 className="w-3.5 h-3.5" />
          <span className="hidden md:inline">Clear Dashboard</span>
        </button>

        <button
          onClick={onRefresh}
          className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-lg bg-sky-500/10 hover:bg-sky-500/20 border border-sky-500/30 text-sky-300 hover:text-white transition-colors text-xs font-medium"
          title="Refresh Telemetry Now"
        >
          <RefreshCw className="w-3.5 h-3.5" />
          <span className="hidden md:inline">Refresh</span>
        </button>
      </div>
    </header>
  );
};

export const ModernSidebar: React.FC<{
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
}> = ({ currentTab, onSelectTab }) => {
  const navItems: { id: NavTab; label: string; icon: any; badge?: string }[] = [
    { id: "overview", label: "Cockpit Overview", icon: Activity },
    { id: "process_explorer", label: "Process Trees & VM", icon: Terminal },
    { id: "stage_timeline", label: "Compiler Stage Pipeline", icon: Layers, badge: "Deep" },
    { id: "linker_analytics", label: "Linker Diagnostics", icon: Box },
    { id: "flame_graph", label: "CPU & Memory Flame", icon: Flame, badge: "Live" },
    { id: "benchmarks", label: "Benchmark Regressions", icon: BarChart3 },
    { id: "hardware_caps", label: "Hardware & Platform", icon: Cpu },
    { id: "audit_logs", label: "Security & Redaction", icon: Shield },
    { id: "settings", label: "Collector Tuning", icon: Settings },
  ];

  return (
    <aside className="w-14 md:w-56 lg:w-60 bg-slate-950/70 border-r border-border/80 flex flex-col justify-between select-none transition-all duration-200 shrink-0">
      <div className="p-2 md:p-3 space-y-1">
        <div className="hidden md:block text-[11px] font-semibold text-slate-400 uppercase tracking-wider px-3 py-2">
          Observability
        </div>
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = currentTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onSelectTab(item.id)}
              title={item.label}
              className={`w-full flex items-center justify-center md:justify-between px-2 md:px-3 py-2 rounded-lg text-xs font-medium transition-all ${
                active
                  ? "bg-gradient-to-r from-sky-500/20 to-indigo-500/10 text-sky-300 border border-sky-500/40 shadow-sm"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-900/60"
              }`}
            >
              <div className="flex items-center space-x-2.5">
                <Icon className={`w-4 h-4 shrink-0 ${active ? "text-sky-400" : "text-slate-400"}`} />
                <span className="hidden md:inline truncate">{item.label}</span>
              </div>
              {item.badge && (
                <span className="hidden lg:inline text-[9px] px-1.5 py-0.2 rounded bg-sky-500/20 text-sky-300 font-mono border border-sky-500/30">
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div className="hidden md:block p-3 border-t border-border/60 bg-slate-900/30">
        <div className="rounded-lg p-2.5 bg-gradient-to-br from-slate-900 to-indigo-950/40 border border-slate-800 text-xs">
          <div className="flex items-center justify-between font-semibold text-slate-200 mb-1">
            <span>Clock Discipline</span>
            <span className="text-[10px] text-emerald-400">Monotonic</span>
          </div>
          <p className="text-[10px] text-slate-400 leading-relaxed">
            Zero-overhead microsecond spans with non-blocking IPC rings.
          </p>
        </div>
      </div>
    </aside>
  );
};
