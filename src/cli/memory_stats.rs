//! Memory statistics reporting module
//!
//! This module provides memory usage statistics and formatting
//! for both interpreter and JIT backends.

use crate::backends::jit::JitMemoryStats;

/// Unified memory stats that can hold either interpreter or JIT stats
pub enum MemoryStats {
    Interpreter(crate::ProgramMemoryStats),
    Jit(JitMemoryStats),
    None,
}

/// Print memory usage statistics
pub fn print_memory_stats(program_stats: MemoryStats) {
    match program_stats {
        MemoryStats::Interpreter(stats) => {
            eprintln!("\n📁 Program Memory Usage (Interpreter):");
            eprintln!("   ─────────────────────────────────────────");

            if stats.variable_count == 0 {
                eprintln!("   No user variables defined");
            } else {
                eprintln!("   Total variables:      {}", stats.variable_count);
                eprintln!(
                    "   Total variable memory: {} bytes",
                    stats.total_variable_bytes
                );
                eprintln!();

                // Print by-type breakdown
                if !stats.by_type.is_empty() {
                    eprintln!("   📊 Memory by Type:");
                    let mut types: Vec<_> = stats.by_type.iter().collect();
                    types.sort_by(|a, b| b.1.1.cmp(&a.1.1));
                    for (type_name, (count, bytes)) in types {
                        eprintln!(
                            "      {:12} {:3} items, {:6} bytes",
                            type_name, count, bytes
                        );
                    }
                    eprintln!();
                }

                // Print individual variables
                if !stats.variables.is_empty() {
                    eprintln!("   📋 Variables (by size):");
                    let max_show = 15.min(stats.variables.len());
                    for var in stats.variables.iter().take(max_show) {
                        let const_marker = if var.is_const { " (const)" } else { "" };
                        eprintln!(
                            "      {:20} {:10} {:6} bytes{}",
                            var.name, var.type_name, var.size_bytes, const_marker
                        );
                    }
                    if stats.variables.len() > max_show {
                        eprintln!(
                            "      ... and {} more variables",
                            stats.variables.len() - max_show
                        );
                    }
                }

                // Print array metadata details
                if !stats.arrays.is_empty() {
                    eprintln!();
                    eprintln!("   📐 Array Metadata Details:");
                    eprintln!(
                        "      Total metadata overhead: {} bytes",
                        stats.total_array_metadata_bytes
                    );
                    eprintln!(
                        "      Total data storage:      {} bytes",
                        stats.total_array_data_bytes
                    );
                    eprintln!();
                    let max_show = 10.min(stats.arrays.len());
                    for arr in stats.arrays.iter().take(max_show) {
                        eprintln!(
                            "      {:20} [{:6}] len={:4} cap={:4} meta={:3}B data={:6}B total={:6}B",
                            arr.name,
                            arr.element_type,
                            arr.length,
                            arr.capacity,
                            arr.metadata_bytes,
                            arr.data_bytes,
                            arr.total_bytes
                        );
                    }
                    if stats.arrays.len() > max_show {
                        eprintln!(
                            "      ... and {} more arrays",
                            stats.arrays.len() - max_show
                        );
                    }
                }
            }

            eprintln!();
            eprintln!("   🏛️ Memory Layout Categories:");
            eprintln!("      Compiler Memory:     {} bytes", stats.compiler_memory);
            eprintln!("      Runtime Memory:      {} bytes", stats.runtime_memory);
            eprintln!("      Heap Memory:         {} bytes", stats.heap_memory);
            eprintln!("      Stack Memory:        {} bytes", stats.stack_memory);
            eprintln!("      Static Data:         {} bytes", stats.static_data);
            eprintln!("      Object Memory:       {} bytes", stats.object_memory);
            eprintln!("      Array Memory:        {} bytes", stats.array_memory);
            eprintln!("      String Memory:       {} bytes", stats.string_memory);
            eprintln!(
                "      Function Metadata:   {} bytes",
                stats.function_metadata
            );
            eprintln!("      Type Metadata:       {} bytes", stats.type_metadata);
            eprintln!("      Symbol Metadata:     {} bytes", stats.symbol_metadata);

            eprintln!();
            eprintln!("   🔧 Runtime State:");
            eprintln!("      Scopes/Environments: {}", stats.scope_count);
            eprintln!("      Active Promises:     {}", stats.promise_count);
            eprintln!("      Active Timers:       {}", stats.timer_count);
            eprintln!(
                "      Method Cache:        {} entries",
                stats.method_cache_count
            );
            if stats.method_cache_lookups > 0 {
                let hit_ratio =
                    (stats.method_cache_hits as f64 / stats.method_cache_lookups as f64) * 100.0;
                eprintln!(
                    "      Method Cache Hits:   {} / {} ({:.1}%)",
                    stats.method_cache_hits, stats.method_cache_lookups, hit_ratio
                );
            } else {
                eprintln!("      Method Cache Hits:   0 / 0 (0.0%)");
            }
            eprintln!();
            eprintln!("   ─────────────────────────────────────────\n");
        }
        MemoryStats::Jit(stats) => {
            eprintln!("\n📁 Program Memory Usage (JIT):");
            eprintln!("   ─────────────────────────────────────────");

            if stats.variable_count == 0 {
                eprintln!("   No user variables defined");
            } else {
                eprintln!("   Total variables:      {}", stats.variable_count);
                eprintln!(
                    "   Total variable memory: {} bytes",
                    stats.total_variable_bytes
                );
                eprintln!();

                // Print by-type breakdown
                if !stats.by_type.is_empty() {
                    eprintln!("   📊 Memory by Type:");
                    let mut types: Vec<_> = stats.by_type.iter().collect();
                    types.sort_by(|a, b| b.1.1.cmp(&a.1.1));
                    for (type_name, (count, bytes)) in types {
                        eprintln!(
                            "      {:12} {:3} items, {:6} bytes",
                            type_name, count, bytes
                        );
                    }
                    eprintln!();
                }

                // Print individual variables
                if !stats.variables.is_empty() {
                    eprintln!("   📋 Variables (by size):");
                    let max_show = 15.min(stats.variables.len());
                    for var in stats.variables.iter().take(max_show) {
                        eprintln!(
                            "      {:20} {:10} {:6} bytes",
                            var.name, var.type_name, var.size_bytes
                        );
                    }
                    if stats.variables.len() > max_show {
                        eprintln!(
                            "      ... and {} more variables",
                            stats.variables.len() - max_show
                        );
                    }
                }
            }

            eprintln!();
            eprintln!("   🔧 JIT Runtime State:");
            eprintln!("      Compiled Functions:  {}", stats.function_count);
            eprintln!("      Active Promises:     {}", stats.promise_count);
            eprintln!("      Memo Cache Entries:  {}", stats.memo_cache_entries);
            eprintln!();
            eprintln!("   ─────────────────────────────────────────\n");
        }
        MemoryStats::None => {}
    }
}
