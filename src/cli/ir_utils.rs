//! IR dumping utilities
//!
//! This module provides utilities for dumping intermediate representations
//! (AST, HIR, LIR, CFG) for debugging and analysis.

use crate::cli::RuntimeConfig;
use crate::toolchain::config::OptLevel;
use std::fs;
use std::path::PathBuf;

/// Dump IR representations
pub fn dump_ir(src: &str, config: &RuntimeConfig) {
    use crate::backends::lir_lower::hir_to_lir;
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    let body = super::directives::strip_compile_directive(src);

    // Parse to AST
    let mut lexer = Lexer::new(&body);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Lexer error: {}", e);
            return;
        }
    };
    let mut parser = Parser::new(tokens, None);
    let ast = match parser.parse_program() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Parser error: {}", e);
            return;
        }
    };

    if config.dump.should_dump_ast() {
        let ast_output = format!("{:#?}", ast);
        if let Some(ref file) = config.dump.write_ast_file {
            let path = if file.is_empty() {
                PathBuf::from("output.ast.adesh")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &ast_output) {
                eprintln!("Error writing AST to file: {}", e);
            } else {
                eprintln!("AST written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ AST ═══");
            eprintln!("{}", ast_output);
        }
    }

    // Lower to HIR
    let hir = match ast_to_hir(&ast, false) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("HIR lowering error: {}", e);
            return;
        }
    };

    if config.dump.should_dump_hir() {
        let hir_output = format!("{:#?}", hir);
        if let Some(ref file) = config.dump.write_hir_file {
            let path = if file.is_empty() {
                PathBuf::from("output.hir.adesh")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &hir_output) {
                eprintln!("Error writing HIR to file: {}", e);
            } else {
                eprintln!("HIR written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ HIR (High-Level IR) ═══");
            eprintln!("{}", hir_output);
        }
    }

    // Lower to LIR
    let lir = match hir_to_lir(&hir) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("LIR lowering error: {}", e);
            return;
        }
    };

    if config.dump.should_dump_lir() {
        let lir_output = format!("{:#?}", lir);
        if let Some(ref file) = config.dump.write_lir_file {
            let path = if file.is_empty() {
                PathBuf::from("output.lir.adesh")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &lir_output) {
                eprintln!("Error writing LIR to file: {}", e);
            } else {
                eprintln!("LIR written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ LIR (Low-Level SSA IR) ═══");
            eprintln!("{}", lir_output);
        }
    }

    // Generate and dump CFG if requested
    if config.dump.should_dump_cfg() {
        use crate::parsing::cfg_borrow::cfg::CfgBuilder;

        // CFG is built per function. We iterate over functions in HIR.
        let mut full_cfg_output = String::new();

        for func in &hir.functions {
            let cfg = CfgBuilder::build(func);
            full_cfg_output.push_str(&format_cfg_to_string(&cfg));
            full_cfg_output.push_str("\n\n");
        }

        if let Some(ref file) = config.dump.write_cfg_file {
            let path = if file.is_empty() {
                PathBuf::from("output.cfg.adesh")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &full_cfg_output) {
                eprintln!("Error writing CFG to file: {}", e);
            } else {
                eprintln!("CFG written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ CFG (Control Flow Graph) ═══");
            eprintln!("{}", full_cfg_output);
        }
    }

    // Generate and dump VIR (Value Intermediate Representation) if requested
    if config.dump.should_dump_vir() {
        // Convert HIR to VIR
        let vir = match convert_hir_to_vir(&hir) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("VIR generation error: {}", e);
                return;
            }
        };

        // Generate pretty-printed VIR output
        let vir_output = format!("{}", vir);

        if let Some(ref file) = config.dump.write_vir_file {
            let path = if file.is_empty() {
                PathBuf::from("output.vir")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &vir_output) {
                eprintln!("Error writing VIR to file: {}", e);
            } else {
                eprintln!("VIR written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ VIR (Value Intermediate Representation) ═══");
            eprintln!("{}", vir_output);
        }
    }

    // Generate and dump MLIR (Multi-Level IR) if requested
    if config.dump.should_dump_mlir() {
        // Generate MLIR representation
        let mlir_output = generate_mlir_from_hir(&hir, config);

        if let Some(ref file) = config.dump.write_mlir_file {
            let path = if file.is_empty() {
                PathBuf::from("output.mlir")
            } else {
                PathBuf::from(file)
            };
            if let Err(e) = fs::write(&path, &mlir_output) {
                eprintln!("Error writing MLIR to file: {}", e);
            } else {
                eprintln!("MLIR written to: {}", path.display());
            }
        } else {
            eprintln!("\n═══ MLIR (Multi-Level Intermediate Representation) ═══");
            eprintln!("{}", mlir_output);
        }
    }

    eprintln!("═══════════════════════════════\n");
}

pub fn format_cfg_to_string(cfg: &crate::parsing::cfg_borrow::cfg::ControlFlowGraph) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "Function '{}':", cfg.function_name).unwrap();
    writeln!(
        &mut s,
        "  Blocks: {}, Entry: {}, Exits: {:?}",
        cfg.blocks.len(),
        cfg.entry,
        cfg.exits
    )
    .unwrap();
    if !cfg.variables.is_empty() {
        writeln!(&mut s, "  Variables:").unwrap();
        let mut vars: Vec<_> = cfg.variables.keys().collect();
        vars.sort();
        for name in vars {
            writeln!(&mut s, "    {}", name).unwrap();
        }
    }

    for block in &cfg.blocks {
        writeln!(
            &mut s,
            "\n  Block {} [Kind: {:?}, Label: \"{}\"]:",
            block.id, block.kind, block.label
        )
        .unwrap();
        if !block.predecessors.is_empty() {
            writeln!(&mut s, "    Predecessors: {:?}", block.predecessors).unwrap();
        }

        if !block.statements.is_empty() {
            writeln!(&mut s, "    Statements:").unwrap();
            for stmt in &block.statements {
                // Formatting statement using Debug
                writeln!(&mut s, "      {:?}", stmt).unwrap();
            }
        } else {
            writeln!(&mut s, "    (empty)").unwrap();
        }

        if !block.successors.is_empty() {
            writeln!(&mut s, "    Successors: {:?}", block.successors).unwrap();
        } else if block.is_terminated() {
            writeln!(&mut s, "    (terminated)").unwrap();
        }
    }
    s
}

/// Convert HIR to VIR (Value Intermediate Representation)
fn convert_hir_to_vir(
    hir: &crate::ir::hir::HirModule,
) -> Result<crate::ir::vir::VirModule, String> {
    let mir = crate::ir::mir::MirModule::from_hir(hir)?;
    crate::ir::vir::VirModule::from_mir(&mir)
}

/// Generate MLIR (Multi-Level Intermediate Representation) from HIR
fn generate_mlir_from_hir(hir: &crate::ir::hir::HirModule, config: &RuntimeConfig) -> String {
    let mir = match crate::ir::mir::MirModule::from_hir(hir) {
        Ok(mir) => mir,
        Err(e) => return format!("// MLIR generation failed (MIR): {}\n", e),
    };

    let vir = match crate::ir::vir::VirModule::from_mir(&mir) {
        Ok(vir) => vir,
        Err(e) => return format!("// MLIR generation failed (VIR): {}\n", e),
    };

    let mlir_config = crate::backends::mlir::MlirConfig {
        enable_gpu: config.dump.mlir_enable_gpu,
        gpu_target: config.gpu_target,
        gpu_kernel_config: crate::backends::mlir::gpu::GpuKernelConfig {
            grid_dims: config.gpu_grid,
            block_dims: config.gpu_block,
            shared_memory_size: config.gpu_shared_mem,
        },
        opt_level: opt_level_to_u8(config.opt_level),
        enable_vector: true,
        target_triple: None,
    };

    match crate::backends::mlir::lowering::lower_module(&vir, &mlir_config) {
        Ok(mlir) => mlir,
        Err(e) => format!("// MLIR generation failed (Lowering): {}\n", e),
    }
}

fn opt_level_to_u8(level: OptLevel) -> u8 {
    match level {
        OptLevel::O0 => 0,
        OptLevel::O1 => 1,
        OptLevel::O2 => 2,
        OptLevel::O3 => 3,
    }
}
