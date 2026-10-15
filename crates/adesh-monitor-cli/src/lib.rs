//! `adesh-monitor-cli`
//!
//! Production-grade scriptable CLI for Adesh Observatory (ARO).

use adesh_monitor_core::ipc::send_collector_rpc;
use adesh_monitor_core::storage::HistoryStore;
use adesh_monitor_platform::create_platform_provider;
use adesh_monitor_protocol::*;

pub fn run_cli(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        print_help();
        return Ok(());
    }

    let cmd = &args[0];
    let subargs = &args[1..];

    match cmd.as_str() {
        "list" => cmd_list(subargs),
        "show" => cmd_show(subargs),
        "stats" => cmd_stats(subargs),
        "trace" => cmd_trace(subargs),
        "export" => cmd_export(subargs),
        "doctor" => cmd_doctor(subargs),
        "version" => cmd_version(),
        "purge" => cmd_purge(),
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        unknown => Err(format!(
            "Unknown monitor command '{}'. Run 'aro --help' or 'adesh monitor --help' for usage.",
            unknown
        )),
    }
}

pub fn print_help() {
    println!(
        r#"Adesh Runtime Observatory (ARO) CLI v{}
Cross-platform production monitor and diagnostic tool for AdeshLang.

USAGE:
    aro <COMMAND> [OPTIONS]
    adesh monitor <COMMAND> [OPTIONS]

COMMANDS:
    list [--limit <N>] [--json]       List active and recorded execution sessions
    show <session-id> [--json]        Show complete session details & telemetry
    trace <session-id> [--json]       Inspect compiler and linker stage timings
    stats [--json]                    Display collector throughput and resource stats
    export <session-id> [--format json|csv] [--out <file>]  Export session reports
    doctor                            Check OS platform capabilities, IPC, and storage
    purge                             Purge local monitoring database history
    version                           Display observatory version and schema compatibility
    help                              Display this help message
"#,
        PROTOCOL_VERSION
    );
}

fn cmd_list(args: &[String]) -> Result<(), String> {
    let mut limit = 50;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--limit" && i + 1 < args.len() {
            limit = args[i + 1].parse().unwrap_or(50);
            i += 2;
        } else if args[i] == "--json" {
            json_mode = true;
            i += 1;
        } else {
            i += 1;
        }
    }

    let sessions = match send_collector_rpc(&CollectorRequest::ListSessions { limit }) {
        Ok(CollectorResponse::SessionsList { sessions }) => sessions,
        _ => {
            let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
            store.list_sessions(limit).map_err(|e| e.to_string())?
        }
    };

    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "{:<36} {:<8} {:<12} {:<12} {:<10} {:<10} {:<10}",
            "SESSION ID", "PID", "TOOL", "MODE", "STATUS", "DURATION", "PEAK RSS"
        );
        println!("{:-<105}", "");
        for s in &sessions {
            let dur = s
                .duration_ms
                .map(|d| format!("{}ms", d))
                .unwrap_or_else(|| "running".to_string());
            let mem_mb = format!("{:.1}MB", s.peak_rss_bytes as f64 / (1024.0 * 1024.0));
            let mode = format!("{:?}", s.execution_mode);
            let status = format!("{:?}", s.status);
            println!(
                "{:<36} {:<8} {:<12} {:<12} {:<10} {:<10} {:<10}",
                s.session_id, s.pid, s.tool_name, mode, status, dur, mem_mb
            );
        }
    }
    Ok(())
}

fn cmd_show(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("Missing <session-id>. Usage: aro show <session-id> [--json]".to_string());
    }
    let session_id = args[0].clone();
    let json_mode = args.iter().any(|a| a == "--json");

    let session = match send_collector_rpc(&CollectorRequest::GetSessionDetail {
        session_id: session_id.clone(),
    }) {
        Ok(CollectorResponse::SessionDetail { session }) => *session,
        _ => {
            let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
            store
                .get_session_detail(&session_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Session {} not found", session_id))?
        }
    };

    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?
        );
    } else {
        println!("=== Session Detail: {} ===", session.summary.session_id);
        println!("Tool:            {}", session.summary.tool_name);
        println!("PID:             {}", session.summary.pid);
        println!("Mode:            {:?}", session.summary.execution_mode);
        println!("Status:          {:?}", session.summary.status);
        println!("Exit Code:       {:?}", session.summary.exit_code);
        println!("Working Dir:     {}", session.working_dir);
        println!(
            "Target File:     {}",
            session.target_file.as_deref().unwrap_or("none")
        );
        println!("Command:         {}", session.summary.command_line);
        println!("Compiler Stages: {}", session.compiler_stages.len());
        println!("Linker Stages:   {}", session.linker_stages.len());
        println!("Metrics Samples: {}", session.metrics_count);
    }
    Ok(())
}

fn cmd_stats(args: &[String]) -> Result<(), String> {
    let json_mode = args.iter().any(|a| a == "--json");
    let stats = match send_collector_rpc(&CollectorRequest::GetCollectorStats) {
        Ok(CollectorResponse::Stats(s)) => s,
        _ => {
            let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
            store.get_stats().map_err(|e| e.to_string())?
        }
    };

    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&stats).map_err(|e| e.to_string())?
        );
    } else {
        println!("=== Adesh Observatory Collector Stats ===");
        println!("Uptime:           {}s", stats.uptime_secs);
        println!("Events Ingested:  {}", stats.total_events_ingested);
        println!("Events Dropped:   {}", stats.total_events_dropped);
        println!("Active Sessions:  {}", stats.active_sessions_count);
        println!("Stored Sessions:  {}", stats.stored_sessions_count);
        println!(
            "Database Size:    {:.2} MB",
            stats.database_size_bytes as f64 / (1024.0 * 1024.0)
        );
    }
    Ok(())
}

fn cmd_trace(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("Missing <session-id>. Usage: aro trace <session-id> [--json]".to_string());
    }
    let session_id = args[0].clone();
    let json_mode = args.iter().any(|a| a == "--json");

    let (c_stages, l_stages) = match send_collector_rpc(&CollectorRequest::GetSessionStages {
        session_id: session_id.clone(),
    }) {
        Ok(CollectorResponse::SessionStages {
            compiler_stages,
            linker_stages,
            ..
        }) => (compiler_stages, linker_stages),
        _ => {
            let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
            let cs = store
                .get_compiler_stages(&session_id)
                .map_err(|e| e.to_string())?;
            let ls = store
                .get_linker_stages(&session_id)
                .map_err(|e| e.to_string())?;
            (cs, ls)
        }
    };

    if json_mode {
        let val = serde_json::json!({
            "session_id": session_id,
            "compiler_stages": c_stages,
            "linker_stages": l_stages,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "=== Compilation & Linker Stage Timeline for {} ===",
            session_id
        );
        if c_stages.is_empty() && l_stages.is_empty() {
            println!("No stage events recorded for this session.");
        }
        for (idx, cs) in c_stages.iter().enumerate() {
            let status = if cs.success { "OK" } else { "FAILED" };
            println!(
                "{:>2}. [Compiler] {:<20} {:>8} µs  [{}]",
                idx + 1,
                cs.stage_name,
                cs.duration_micros,
                status
            );
        }
        for (idx, ls) in l_stages.iter().enumerate() {
            let status = if ls.success { "OK" } else { "FAILED" };
            println!(
                "{:>2}. [Linker]   {:<20} {:>8} µs  [{}] (Target: {})",
                idx + 1,
                ls.stage_name,
                ls.duration_micros,
                status,
                ls.target_triple
            );
        }
    }
    Ok(())
}

fn cmd_export(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("Missing <session-id>. Usage: aro export <session-id> [--format json|csv] [--out <file>]".to_string());
    }
    let session_id = args[0].clone();
    let mut format = "json";
    let mut out_file = None;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--format" && i + 1 < args.len() {
            format = &args[i + 1];
            i += 2;
        } else if args[i] == "--out" && i + 1 < args.len() {
            out_file = Some(&args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    let session = match send_collector_rpc(&CollectorRequest::GetSessionDetail {
        session_id: session_id.clone(),
    }) {
        Ok(CollectorResponse::SessionDetail { session }) => *session,
        _ => {
            let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
            store
                .get_session_detail(&session_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Session {} not found", session_id))?
        }
    };

    let output_str = if format.eq_ignore_ascii_case("csv") {
        let mut csv = String::new();
        csv.push_str("session_id,tool,pid,mode,status,exit_code,duration_ms,peak_rss_bytes\n");
        csv.push_str(&format!(
            "{},{},{},{:?},{:?},{:?},{:?},{}\n",
            session.summary.session_id,
            session.summary.tool_name,
            session.summary.pid,
            session.summary.execution_mode,
            session.summary.status,
            session.summary.exit_code,
            session.summary.duration_ms,
            session.summary.peak_rss_bytes,
        ));
        csv
    } else {
        serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?
    };

    if let Some(dest) = out_file {
        std::fs::write(dest, output_str).map_err(|e| format!("Failed to write export: {}", e))?;
        println!("Exported session {} to {}", session_id, dest);
    } else {
        println!("{}", output_str);
    }
    Ok(())
}

fn cmd_doctor(_args: &[String]) -> Result<(), String> {
    println!("=== Adesh Observatory Doctor ===");
    let provider = create_platform_provider();
    let caps = provider.query_capabilities();

    println!("OS Platform:         {}", caps.os_name);
    println!("Architecture:        {}", caps.arch);
    println!("Logical CPU Cores:   {}", caps.logical_cores);
    println!(
        "Process CPU Metrics: {}",
        if caps.supports_process_cpu {
            "Supported (Native)"
        } else {
            "Unavailable"
        }
    );
    println!(
        "Process Memory:      {}",
        if caps.supports_process_memory {
            "Supported (Native RSS/VMS)"
        } else {
            "Unavailable"
        }
    );
    println!(
        "Disk I/O Metrics:    {}",
        if caps.supports_disk_io {
            "Supported"
        } else {
            "Unavailable"
        }
    );
    println!(
        "Hardware Accel:      {}",
        if caps.supports_accelerators {
            "Available"
        } else {
            "Pluggable"
        }
    );

    // Test IPC connectivity
    match send_collector_rpc(&CollectorRequest::Ping) {
        Ok(CollectorResponse::Pong {
            version,
            uptime_secs,
        }) => {
            println!(
                "ARO Collector IPC:   ONLINE (v{}, uptime {}s)",
                version, uptime_secs
            );
        }
        Err(_) => {
            println!("ARO Collector IPC:   STANDALONE (direct SQLite WAL active)");
        }
        _ => {}
    }
    Ok(())
}

fn cmd_version() -> Result<(), String> {
    println!("Adesh Runtime Observatory (ARO) v{}", PROTOCOL_VERSION);
    println!("Protocol Schema Version: {}", PROTOCOL_VERSION);
    println!("Ecosystem: AdeshLang v0.3.0");
    Ok(())
}

fn cmd_purge() -> Result<(), String> {
    let _ = send_collector_rpc(&CollectorRequest::PurgeHistory);
    let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
    store.purge_history().map_err(|e| e.to_string())?;
    println!("Observatory database history purged successfully.");
    Ok(())
}
