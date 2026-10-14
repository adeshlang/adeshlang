import React from "react";
import { Activity, Layers, Terminal, Cpu, Database, Settings } from "lucide-react";

interface SidebarProps {
  currentTab: string;
  onSelectTab: (tab: string) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({ currentTab, onSelectTab }) => {
  const tabs = [
    { id: "overview", label: "Overview", icon: Activity },
    { id: "processes", label: "Process Explorer", icon: Terminal },
    { id: "stages", label: "Compiler & Linker", icon: Layers },
    { id: "performance", label: "Performance", icon: Cpu },
    { id: "history", label: "Session History", icon: Database },
    { id: "settings", label: "Settings", icon: Settings },
  ];

  return (
    <aside className="w-64 bg-surface border-r border-border flex flex-col justify-between select-none">
      <div>
        <div className="p-4 border-b border-border flex items-center space-x-3">
          <div className="w-8 h-8 rounded-lg bg-primary/20 border border-primary/50 flex items-center justify-center text-primary font-bold">
            ARO
          </div>
          <div>
            <h1 className="font-semibold text-sm tracking-wide text-slate-100">Observatory</h1>
            <p className="text-xs text-slate-400">Adesh Ecosystem</p>
          </div>
        </div>

        <nav className="p-3 space-y-1">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const active = currentTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => onSelectTab(tab.id)}
                className={`w-full flex items-center space-x-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
                  active
                    ? "bg-primary/10 text-primary border border-primary/30"
                    : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
                }`}
              >
                <Icon className={`w-4 h-4 ${active ? "text-primary" : "text-slate-400"}`} />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </nav>
      </div>

      <div className="p-4 border-t border-border bg-slate-900/40">
        <div className="flex items-center space-x-2">
          <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
          <span className="text-xs font-mono text-slate-300">IPC Collector: Online</span>
        </div>
        <p className="text-[10px] text-slate-500 mt-1">Cross-platform daemon active</p>
      </div>
    </aside>
  );
};
