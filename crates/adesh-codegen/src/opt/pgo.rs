//! Profile-Guided Optimization (PGO) Infrastructure.
//!
//! Provides execution counters, branch probabilities, hot/cold block identification,
//! profile serialization, instrumentation pass, and profile-guided block layout.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, MachineOperand};
use crate::opt::pass::MachinePass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Execution Profile for a single Basic Block.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockProfile {
    pub execution_count: u64,
}

/// Execution Profile for a CFG Edge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgeProfile {
    pub transition_count: u64,
    pub probability: f64,
}

/// Execution Profile for a Function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionProfile {
    pub name: String,
    pub entry_count: u64,
    pub block_profiles: HashMap<String, BlockProfile>,
    pub edge_profiles: HashMap<String, EdgeProfile>,
}

impl FunctionProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entry_count: 0,
            block_profiles: HashMap::new(),
            edge_profiles: HashMap::new(),
        }
    }

    pub fn add_block_profile(&mut self, block_id: u32, bp: BlockProfile) {
        self.block_profiles.insert(block_id.to_string(), bp);
    }

    pub fn is_hot_block(&self, block_id: u32, threshold_ratio: f64) -> bool {
        if self.entry_count == 0 {
            return false;
        }
        if let Some(bp) = self.block_profiles.get(&block_id.to_string()) {
            (bp.execution_count as f64) >= (self.entry_count as f64) * threshold_ratio
        } else {
            false
        }
    }

    pub fn block_frequency(&self, block_id: u32) -> u64 {
        self.block_profiles
            .get(&block_id.to_string())
            .map(|b| b.execution_count)
            .unwrap_or(0)
    }
}

/// Entire Module/Program Profile Data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileData {
    pub functions: HashMap<String, FunctionProfile>,
}

impl ProfileData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_function_profile(&mut self, profile: FunctionProfile) {
        self.functions.insert(profile.name.clone(), profile);
    }

    pub fn get_function_profile(&self, name: &str) -> Option<&FunctionProfile> {
        self.functions.get(name)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// PGO Instrumentation Pass for `-PGO=generate`.
/// Inserts counter increments for basic block profiling.
pub struct PgoInstrumentationPass {
    pub counter_symbol_prefix: String,
}

impl PgoInstrumentationPass {
    pub fn new() -> Self {
        Self {
            counter_symbol_prefix: "__pgo_counter".to_string(),
        }
    }

    /// Instruments a function with profiling counter increments at the start
    /// of each block.
    ///
    /// Counter symbols are keyed by block **id** (not layout position), and
    /// the entry block (`blocks[0]`, the `PgoOptimizationPass` entry
    /// convention) gets an `__pgo_entry_<fn>_<id>` symbol so the runtime dump
    /// can report `entry_count` alongside per-block counts.
    ///
    /// The encoder preserves EFLAGS around the atomic counter increment,
    /// including when flags are live across block boundaries.
    pub fn instrument_function(&self, func: &mut MachineFunction) {
        for (pos, block) in func.blocks.iter_mut().enumerate() {
            let prefix = if pos == 0 {
                "__pgo_entry"
            } else {
                self.counter_symbol_prefix.as_str()
            };
            let sym = format!("{}_{}_{}", prefix, func.name, block.id);
            let counter_inst = MachineInstruction::Custom {
                name: "pgo_inc".to_string(),
                operands: vec![MachineOperand::Symbol(sym)],
            };
            block.instructions.insert(0, counter_inst);
        }
    }
}

impl Default for PgoInstrumentationPass {
    fn default() -> Self {
        Self::new()
    }
}

/// PGO Consumption & Layout Optimization Pass for `-PGO=use`.
/// Reorders basic blocks to place hot traces sequentially and cold blocks at the end.
pub struct PgoOptimizationPass {
    pub profile_data: ProfileData,
    pub hot_threshold_ratio: f64,
}

impl PgoOptimizationPass {
    pub fn new(profile_data: ProfileData) -> Self {
        Self {
            profile_data,
            hot_threshold_ratio: 0.5,
        }
    }

    /// Calculate hotness-weighted spill cost multiplier for register allocation.
    pub fn spill_weight_for_block(&self, func_name: &str, block_id: u32) -> f64 {
        if let Some(p) = self.profile_data.get_function_profile(func_name) {
            let freq = p.block_frequency(block_id);
            1.0 + (freq as f64) * 10.0
        } else {
            1.0
        }
    }
}

impl MachinePass for PgoOptimizationPass {
    fn name(&self) -> &'static str {
        "pgo_block_layout"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        let profile = match self.profile_data.get_function_profile(&func.name) {
            Some(p) => p,
            None => return Ok(false),
        };

        if func.blocks.len() <= 1 {
            return Ok(false);
        }

        // Preserve implicit fallthrough edges before changing physical layout.
        let labels: Vec<_> = func.blocks.iter().map(|b| b.label.clone()).collect();
        for (i, block) in func.blocks.iter_mut().enumerate() {
            if i + 1 < labels.len()
                && !matches!(
                    block.instructions.last(),
                    Some(MachineInstruction::Branch { .. } | MachineInstruction::Return)
                )
            {
                block.instructions.push(MachineInstruction::Branch {
                    target: labels[i + 1].clone(),
                });
            }
        }

        // Partition blocks into entry block (0), hot blocks, and cold blocks
        let entry = func.blocks.remove(0);
        let mut hot_blocks = Vec::new();
        let mut cold_blocks = Vec::new();

        for block in func.blocks.drain(..) {
            if profile.is_hot_block(block.id, self.hot_threshold_ratio) {
                hot_blocks.push(block);
            } else {
                cold_blocks.push(block);
            }
        }

        // Sort hot blocks by execution count descending
        hot_blocks.sort_by(|a, b| {
            let count_a = profile.block_frequency(a.id);
            let count_b = profile.block_frequency(b.id);
            count_b.cmp(&count_a)
        });

        // Reconstruct function blocks: entry -> hot blocks -> cold blocks
        let mut new_blocks = Vec::with_capacity(1 + hot_blocks.len() + cold_blocks.len());
        new_blocks.push(entry);
        new_blocks.extend(hot_blocks);
        new_blocks.extend(cold_blocks);

        func.blocks = new_blocks;
        func.rebuild_cfg();
        Ok(true)
    }
}

/// Profile-Guided Optimization Operating Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PgoMode {
    #[default]
    Disabled,
    Generate,
    Use,
}

/// Configuration for PGO transformations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgoConfig {
    pub mode: PgoMode,
    pub profile_path: Option<std::path::PathBuf>,
    pub hot_threshold: f64,
}

impl Default for PgoConfig {
    fn default() -> Self {
        Self {
            mode: PgoMode::Disabled,
            profile_path: None,
            hot_threshold: 0.5,
        }
    }
}

/// Profile-Guided Optimization Engine coordinating profile collection and consumption.
pub struct PgoEngine {
    pub config: PgoConfig,
    pub profile: ProfileData,
}

impl PgoEngine {
    pub fn new(config: PgoConfig) -> Self {
        Self {
            config,
            profile: ProfileData::new(),
        }
    }

    pub fn with_profile(config: PgoConfig, profile: ProfileData) -> Self {
        Self { config, profile }
    }

    pub fn optimize_function(&self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        match self.config.mode {
            PgoMode::Use => {
                let mut pass = PgoOptimizationPass::new(self.profile.clone());
                pass.hot_threshold_ratio = self.config.hot_threshold;
                pass.run_on_function(func)
            }
            PgoMode::Generate => {
                let pass = PgoInstrumentationPass::new();
                pass.instrument_function(func);
                Ok(true)
            }
            PgoMode::Disabled => Ok(false),
        }
    }
}
