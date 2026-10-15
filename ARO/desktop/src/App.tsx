import React from "react";
import { ObservatoryProvider, useObservatory } from "./context/ObservatoryContext";
import { TopBar, ModernSidebar } from "./components/Navigation";
import { CockpitOverview } from "./pages/CockpitOverview";
import { StagesTimeline } from "./pages/StagesTimeline";
import { ShieldCheck, Cpu, HardDrive, Terminal } from "lucide-react";

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
  } = state;

  const activeCount = sessions.filter((s) => s.status === "running").length;

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
            <div className="p-8 space-y-4 max-w-2xl">
              <h2 className="text-base font-bold text-slate-100 flex items-center space-x-2">
                <Cpu className="w-4 h-4 text-sky-400" />
                <span>Hardware & Platform Discovery</span>
              </h2>
              <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl space-y-3 text-xs">
                <div className="flex justify-between border-b border-slate-800 pb-2">
                  <span className="text-slate-400">Operating System</span>
                  <span className="font-semibold text-slate-200">{hardware.os_name}</span>
                </div>
                <div className="flex justify-between border-b border-slate-800 pb-2">
                  <span className="text-slate-400">Architecture</span>
                  <span className="font-semibold text-slate-200">{hardware.arch}</span>
                </div>
                <div className="flex justify-between border-b border-slate-800 pb-2">
                  <span className="text-slate-400">Logical CPU Cores</span>
                  <span className="font-semibold text-slate-200">{hardware.logical_cores}</span>
                </div>
                <div className="flex justify-between border-b border-slate-800 pb-2">
                  <span className="text-slate-400">Process Memory (RSS/Working Set)</span>
                  <span className="text-emerald-400 font-semibold">Native OS Provider</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-400">Hardware Accelerators</span>
                  <span className="text-sky-400 font-semibold">Pluggable Provider Architecture</span>
                </div>
              </div>
            </div>
          )}

          {currentTab === "settings" && (
            <div className="p-8 space-y-4 max-w-xl">
              <h2 className="text-base font-bold text-slate-100">Observatory Collector Settings</h2>
              <div className="bg-slate-900 border border-slate-800 p-5 rounded-xl space-y-4 text-xs">
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-semibold text-slate-200">IPC Endpoint</div>
                    <div className="text-[10px] text-slate-500">Windows Named Pipe & Unix Sockets</div>
                  </div>
                  <span className="font-mono text-sky-400 px-2 py-0.5 rounded bg-slate-950 border border-slate-800">
                    Auto-Discovered
                  </span>
                </div>
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-semibold text-slate-200">Sampling Rate</div>
                    <div className="text-[10px] text-slate-500">Resource metrics polling frequency</div>
                  </div>
                  <span className="font-mono text-slate-300">250 ms</span>
                </div>
                <div className="flex justify-between items-center">
                  <div>
                    <div className="font-semibold text-slate-200">Privacy & Token Redaction</div>
                    <div className="text-[10px] text-slate-500">Mask sensitive CLI credentials automatically</div>
                  </div>
                  <span className="text-emerald-400 font-semibold">Enforced</span>
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
