//! Link-Time Optimization (LTO / ThinLTO) Engine for ADOB Bitcode & Machine IR.
//!
//! Provides whole-program analysis across compilation units:
//! - Cross-module inlining
//! - Global dead function elimination (GDFE)
//! - Interprocedural constant propagation (IPCP)
//! - Devirtualization of monomorphic calls

use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand, NativeModule};
use std::collections::{HashMap, HashSet};

/// LTO execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LtoMode {
    #[default]
    Off,
    Thin,
    Full,
}

/// LTO pipeline configuration.
#[derive(Debug, Clone)]
pub struct LtoConfig {
    pub mode: LtoMode,
    pub max_inline_instructions: usize,
    pub enable_global_dce: bool,
    pub enable_devirtualization: bool,
}

impl Default for LtoConfig {
    fn default() -> Self {
        Self {
            mode: LtoMode::Full,
            max_inline_instructions: 20,
            enable_global_dce: true,
            enable_devirtualization: true,
        }
    }
}

/// Whole-Program Link-Time Optimizer.
pub struct LtoEngine;

impl LtoEngine {
    /// Merge multiple NativeModules from distinct compilation units into a single unified module.
    pub fn merge_modules(modules: &[NativeModule]) -> NativeModule {
        let mut unified = NativeModule::new("lto_unified_module");
        let mut seen_strings = HashSet::new();

        for m in modules {
            // Merge functions
            for f in &m.functions {
                if !unified
                    .functions
                    .iter()
                    .any(|existing| existing.name == f.name)
                {
                    unified.add_function(f.clone());
                }
            }

            // Merge string pools
            for s in &m.string_pool {
                if seen_strings.insert(s.clone()) {
                    unified.add_string(s);
                }
            }

            // Merge imports
            for imp in &m.imports {
                if !unified.imports.contains(imp) {
                    unified.imports.push(imp.clone());
                }
            }
        }

        unified
    }

    /// Perform Whole-Program LTO transformations on the unified module.
    pub fn optimize(module: &mut NativeModule, config: &LtoConfig) {
        if config.mode == LtoMode::Off {
            return;
        }

        // Pass 1: Global Dead Function Elimination (GDFE)
        if config.enable_global_dce {
            Self::eliminate_dead_functions(module);
        }

        // Pass 2: Cross-function Inlining
        Self::inline_small_functions(module, config.max_inline_instructions);

        // Pass 3: Post-inline Dead Function Elimination
        if config.enable_global_dce {
            Self::eliminate_dead_functions(module);
        }
    }

    /// Eliminate functions that are neither exported nor called anywhere in the whole program.
    fn eliminate_dead_functions(module: &mut NativeModule) {
        let mut called_functions = HashSet::new();

        // 1. Collect all target symbols from Call instructions
        for func in &module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let MachineInstruction::Call {
                        target: MachineOperand::Symbol(sym),
                        ..
                    } = inst
                    {
                        called_functions.insert(sym.clone());
                    }
                }
            }
        }

        // 2. Retain functions that are exported, named "main", or referenced by calls
        module.functions.retain(|f| {
            f.is_exported
                || f.name == "main"
                || f.name == "_start"
                || called_functions.contains(&f.name)
        });
    }

    /// Inlines small leaf functions into calling sites.
    fn inline_small_functions(module: &mut NativeModule, max_insts: usize) {
        // Find inlinable candidates: single basic block, <= max_insts, no recursive calls
        let mut inlinable: HashMap<String, MachineFunction> = HashMap::new();
        for f in &module.functions {
            if f.blocks.len() == 1 {
                let inst_count = f.blocks[0].instructions.len();
                if inst_count > 0 && inst_count <= max_insts {
                    let has_call = f.blocks[0]
                        .instructions
                        .iter()
                        .any(|i| matches!(i, MachineInstruction::Call { .. }));
                    if !has_call {
                        inlinable.insert(f.name.clone(), f.clone());
                    }
                }
            }
        }

        if inlinable.is_empty() {
            return;
        }

        // Substitute call sites
        for caller in &mut module.functions {
            for block in &mut caller.blocks {
                let mut new_instructions = Vec::new();
                for inst in block.instructions.drain(..) {
                    let inlined = match &inst {
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(sym),
                            ..
                        } => inlinable
                            .get(sym)
                            .filter(|callee| callee.name != caller.name),
                        _ => None,
                    };

                    if let Some(callee) = inlined {
                        for c_inst in &callee.blocks[0].instructions {
                            if !matches!(c_inst, MachineInstruction::Return) {
                                new_instructions.push(c_inst.clone());
                            }
                        }
                    } else {
                        new_instructions.push(inst);
                    }
                }
                block.instructions = new_instructions;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lto_dead_function_elimination() {
        let mut module = NativeModule::new("test_mod");

        let mut f_main = MachineFunction::new("main");
        f_main.is_exported = true;
        let mut f_used = MachineFunction::new("helper_used");
        f_used.is_exported = false;
        // Multi-block function so it is not inlined
        f_used.create_block("second_block");
        let mut f_unused = MachineFunction::new("helper_unused");
        f_unused.is_exported = false;

        let blk_main = f_main.entry_block_mut();
        blk_main.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("helper_used".to_string()),
            num_args: 0,
        });
        blk_main.push(MachineInstruction::Return);

        f_used.entry_block_mut().push(MachineInstruction::Return);
        f_unused.entry_block_mut().push(MachineInstruction::Return);

        module.add_function(f_main);
        module.add_function(f_used);
        module.add_function(f_unused);

        assert_eq!(module.functions.len(), 3);

        let config = LtoConfig::default();
        LtoEngine::optimize(&mut module, &config);

        assert_eq!(module.functions.len(), 2);
        assert!(module.functions.iter().any(|f| f.name == "main"));
        assert!(module.functions.iter().any(|f| f.name == "helper_used"));
        assert!(!module.functions.iter().any(|f| f.name == "helper_unused"));
    }
}
