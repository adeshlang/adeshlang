import React from "react";
import { SessionDetail } from "../types/observatory";
import { CheckCircle2, XCircle, ArrowLeft, Layers, Cpu, ShieldCheck } from "lucide-react";

interface StagesTimelineProps {
  sessionDetail?: SessionDetail;
  onBack: () => void;
}

export const StagesTimeline: React.FC<StagesTimelineProps> = ({ sessionDetail, onBack }) => {
  if (!sessionDetail) {
    return (
      <div className="p-8 text-center text-slate-500">
        <p>No session selected. Choose a session from the Overview or Process Explorer to view stage breakdowns.</p>
      </div>
    );
  }

  const { summary, compiler_stages, linker_stages } = sessionDetail;
  const compilerTotal = compiler_stages.reduce((total, stage) => total + stage.duration_micros, 0);
  const linkerTotal = linker_stages.reduce((total, stage) => total + stage.duration_micros, 0);

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      <div className="flex items-center space-x-4 border-b border-border pb-4">
        <button
          onClick={onBack}
          className="p-1.5 rounded-lg bg-surface border border-border hover:bg-slate-800 text-slate-300"
        >
          <ArrowLeft className="w-4 h-4" />
        </button>
        <div>
          <h2 className="text-lg font-bold text-slate-100 flex items-center space-x-2">
            <span>Stage Execution Timeline</span>
            <span className="text-xs px-2 py-0.5 rounded bg-slate-800 text-primary border border-primary/20 font-mono">
              {summary.session_id}
            </span>
          </h2>
          <p className="text-xs text-slate-400">
            Tool: {summary.tool_name} | Mode: {summary.execution_mode} | PID: {summary.pid}
          </p>
        </div>
      </div>

      {/* Compiler Stages */}
      <div className="bg-surface border border-border rounded-xl p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-border/50 pb-3">
          <h3 className="font-semibold text-slate-200 flex items-center space-x-2">
            <Layers className="w-4 h-4 text-primary" />
            <span>Compiler Pipeline Stages</span>
          </h3>
          <span className="text-xs text-slate-400">{compiler_stages.length} recorded · {(compilerTotal / 1000).toFixed(2)} ms</span>
        </div>

        <div className="space-y-3">
          {compiler_stages.map((stage, idx) => (
            <div
              key={idx}
              className="flex items-center justify-between p-3 rounded-lg bg-slate-900/40 border border-border/60 hover:border-primary/40 transition-colors"
            >
              <div className="flex items-center space-x-3">
                {stage.success ? (
                  <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                ) : (
                  <XCircle className="w-4 h-4 text-red-400" />
                )}
                <div>
                  <div className="text-xs font-semibold text-slate-200 font-mono">{stage.stage_name}</div>
                  <div className="text-[10px] text-slate-400">{stage.input_file || "in-memory source"}</div>
                  {stage.error_message && <div className="text-[10px] text-red-300 mt-1">{stage.error_message}</div>}
                  {Object.keys(stage.metadata || {}).length > 0 && (
                    <div className="text-[10px] text-slate-500 mt-1 font-mono">
                      {Object.entries(stage.metadata).map(([key, value]) => `${key}=${value}`).join(" · ")}
                    </div>
                  )}
                </div>
              </div>

              <div className="text-right">
                <div className="text-xs font-mono font-bold text-primary">{stage.duration_micros} µs</div>
                <div className="text-[10px] text-slate-500">
                  {stage.duration_micros > 1000 ? `${(stage.duration_micros / 1000).toFixed(2)} ms` : "sub-millisecond"}
                </div>
              </div>
            </div>
          ))}

          {compiler_stages.length === 0 && (
            <div className="text-center py-6 text-xs text-slate-500">
              No compiler stage telemetry captured for this run.
            </div>
          )}
        </div>
      </div>

      {/* Linker Stages */}
      <div className="bg-surface border border-border rounded-xl p-5 space-y-4">
        <div className="flex items-center justify-between border-b border-border/50 pb-3">
          <h3 className="font-semibold text-slate-200 flex items-center space-x-2">
            <Cpu className="w-4 h-4 text-indigo-400" />
            <span>Native Linker Pipeline (adeshlink)</span>
          </h3>
          <span className="text-xs text-slate-400">{linker_stages.length} recorded · {(linkerTotal / 1000).toFixed(2)} ms</span>
        </div>

        <div className="space-y-3">
          {linker_stages.map((stage, idx) => (
            <div
              key={idx}
              className="flex items-center justify-between p-3 rounded-lg bg-slate-900/40 border border-border/60"
            >
              <div className="flex items-center space-x-3">
                {stage.success ? <CheckCircle2 className="w-4 h-4 text-indigo-400" /> : <XCircle className="w-4 h-4 text-red-400" />}
                <div>
                  <div className="text-xs font-semibold text-slate-200 font-mono">{stage.stage_name}</div>
                  <div className="text-[10px] text-slate-400">
                    Target: {stage.target_triple} | Format: {stage.output_format}
                  </div>
                  {!stage.success && stage.error_message && <div className="text-[10px] text-red-300 mt-1">{stage.error_message}</div>}
                </div>
              </div>

              <div className="text-right">
                <div className="text-xs font-mono font-bold text-indigo-400">{stage.duration_micros} µs</div>
                <div className="text-[10px] text-slate-500">
                  {stage.output_size_bytes ? `${stage.output_size_bytes} bytes emitted` : "resolved"}
                </div>
              </div>
            </div>
          ))}

          {linker_stages.length === 0 && (
            <div className="text-center py-6 text-xs text-slate-500">
              No linker operations recorded for this session.
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
