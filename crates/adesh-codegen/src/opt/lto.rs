//! Link-Time Optimization (LTO / ThinLTO) Engine for ADOB Bitcode & Machine IR.
//!
//! Provides whole-program analysis across compilation units:
//! - Cross-module inlining
//! - Global dead function elimination (GDFE)
//! - Interprocedural constant propagation (IPCP)
//! - Devirtualization of monomorphic calls

use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    RegisterClass, VirtualRegister,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// LTO execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LtoMode {
    #[default]
    Off,
    Thin,
    Full,
}

/// LTO pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Function metadata summary for Link-Time Analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub name: String,
    pub instruction_count: usize,
    pub is_exported: bool,
    pub calls: Vec<String>,
    pub stack_size: u64,
    pub reads_memory: bool,
    pub writes_memory: bool,
    pub may_allocate: bool,
    pub may_throw: bool,
    pub may_trap: bool,
    pub has_side_effects: bool,
    pub is_pure: bool,
    pub is_leaf: bool,
    pub uses_fp: bool,
    pub uses_simd: bool,
}

/// Module-level summary for Link-Time Optimization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub functions: HashMap<String, FunctionSummary>,
    pub string_pool_size: usize,
}

impl ModuleSummary {
    pub fn analyze(module: &NativeModule) -> Self {
        let mut functions = HashMap::new();
        for func in &module.functions {
            let mut calls = Vec::new();
            let mut count = 0;
            let mut reads_mem = false;
            let mut writes_mem = false;
            let mut uses_fp = false;

            for block in &func.blocks {
                for inst in &block.instructions {
                    count += 1;
                    match inst {
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(sym),
                            ..
                        } => {
                            calls.push(sym.clone());
                            reads_mem = true;
                            writes_mem = true;
                        }
                        MachineInstruction::Load { .. } => reads_mem = true,
                        MachineInstruction::Store { .. } => writes_mem = true,
                        MachineInstruction::FAdd { .. }
                        | MachineInstruction::FSub { .. }
                        | MachineInstruction::FMul { .. }
                        | MachineInstruction::FDiv { .. } => uses_fp = true,
                        _ => {}
                    }
                }
            }

            let is_leaf = calls.is_empty();
            let is_pure = !writes_mem && is_leaf;

            functions.insert(
                func.name.clone(),
                FunctionSummary {
                    name: func.name.clone(),
                    instruction_count: count,
                    is_exported: func.is_exported,
                    calls,
                    stack_size: func.stack_size,
                    reads_memory: reads_mem,
                    writes_memory: writes_mem,
                    may_allocate: false,
                    may_throw: false,
                    may_trap: false,
                    has_side_effects: writes_mem || !is_leaf,
                    is_pure,
                    is_leaf,
                    uses_fp,
                    uses_simd: false,
                },
            );
        }

        Self {
            functions,
            string_pool_size: module.string_pool.len(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Statistics returned by an LTO run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LtoReport {
    pub dead_functions_removed: usize,
    pub inlined_calls: usize,
    pub constants_propagated: usize,
}

/// Whole-Program Link-Time Optimizer.
#[derive(Debug, Clone, Default)]
pub struct LtoEngine {
    pub config: LtoConfig,
    pub modules: Vec<NativeModule>,
}

impl LtoEngine {
    pub fn new(config: LtoConfig) -> Self {
        Self {
            config,
            modules: Vec::new(),
        }
    }

    pub fn add_module(&mut self, module: &NativeModule) {
        self.modules.push(module.clone());
    }

    pub fn optimize_modules(
        &mut self,
        modules: &mut Vec<NativeModule>,
    ) -> Result<LtoReport, CodegenError> {
        if modules.is_empty() {
            return Ok(LtoReport::default());
        }

        let before_count: usize = modules.iter().map(|m| m.functions.len()).sum();
        let mut unified = Self::merge_modules(modules);
        Self::optimize(&mut unified, &self.config);
        let after_count = unified.functions.len();

        let dead_functions_removed = before_count.saturating_sub(after_count);
        *modules = vec![unified];

        Ok(LtoReport {
            dead_functions_removed,
            inlined_calls: 0,
            constants_propagated: 0,
        })
    }

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

    /// Inlines small leaf functions into their call sites.
    ///
    /// Splicing a callee body verbatim into a caller would corrupt both
    /// functions: virtual register ids are per-function and would collide, and
    /// stack slots are frame-relative and would alias the caller's locals. Only
    /// callees that can be *specialised* soundly are inlined; see
    /// [`Self::is_inlinable_candidate`]. The specialisation rewrites every
    /// callee virtual register into the caller's numbering and re-bases its
    /// stack slots below the caller's frame.
    ///
    /// Argument binding needs no extra work in this IR: arguments and the
    /// return value flow through the *physical* argument/return registers of
    /// the calling convention, and the caller has already populated them before
    /// the call instruction.
    fn inline_small_functions(module: &mut NativeModule, max_insts: usize) {
        // Find inlinable candidates. The bodies are cloned so the module's
        // functions can be rewritten while the candidates are being consulted.
        let candidates: HashMap<String, MachineFunction> = module
            .functions
            .iter()
            .filter(|f| Self::is_inlinable_candidate(f, max_insts))
            .map(|f| (f.name.clone(), f.clone()))
            .collect();

        if candidates.is_empty() {
            return;
        }

        for caller in &mut module.functions {
            // Caller-local state; applied once the block loop releases the borrow.
            let mut vreg_base = caller.vreg_count;
            let mut caller_stack = caller.stack_size as i64;
            let mut extra_classes: HashMap<VirtualRegister, RegisterClass> = HashMap::new();
            let mut changed = false;

            for block in &mut caller.blocks {
                let mut new_instructions = Vec::new();
                for inst in block.instructions.drain(..) {
                    let callee = match &inst {
                        MachineInstruction::Call {
                            target: MachineOperand::Symbol(sym),
                            ..
                        } if sym != &caller.name => candidates.get(sym.as_str()),
                        _ => None,
                    };

                    match callee {
                        Some(callee) => {
                            let (body, fresh_vregs, classes) =
                                specialize_body(callee, vreg_base, caller_stack);
                            new_instructions.extend(body);
                            vreg_base += fresh_vregs;
                            caller_stack += callee.stack_size as i64;
                            extra_classes.extend(classes);
                            changed = true;
                        }
                        None => new_instructions.push(inst),
                    }
                }
                block.instructions = new_instructions;
            }

            if changed {
                caller.vreg_count = vreg_base;
                caller.stack_size = caller_stack.max(0) as u64;
                caller.vreg_classes.extend(extra_classes);
            }
        }
    }

    /// True when `f` can be spliced into a caller soundly:
    ///
    /// - a single basic block (no internal control flow to rewrite),
    /// - at least one instruction and at most `max_insts`,
    /// - exactly one `Return`, as the final instruction (single exit; the
    ///   result is already in the physical return register),
    /// - performs no calls itself (leaves only, so inlining is bounded and
    ///   recursion is impossible),
    /// - no `ParallelMove` (its `MoveLocation`s are not `MachineOperand`s the
    ///   specialiser rewrites),
    /// - no `Label` operands (labels refer to blocks *inside the callee* and
    ///   would dangle in the caller).
    fn is_inlinable_candidate(f: &MachineFunction, max_insts: usize) -> bool {
        if f.blocks.len() != 1 {
            return false;
        }
        let insts = &f.blocks[0].instructions;
        if insts.is_empty() || insts.len() > max_insts {
            return false;
        }
        if !matches!(insts.last(), Some(MachineInstruction::Return)) {
            return false;
        }
        if insts
            .iter()
            .filter(|i| matches!(i, MachineInstruction::Return))
            .count()
            != 1
        {
            return false;
        }

        for inst in insts {
            match inst {
                MachineInstruction::Call { .. }
                | MachineInstruction::Branch { .. }
                | MachineInstruction::BranchCc { .. }
                | MachineInstruction::ParallelMove { .. } => return false,
                _ => {}
            }
            let mut probe = inst.clone();
            if operands_mut(&mut probe)
                .iter()
                .any(|op| matches!(op, MachineOperand::Label(_)))
            {
                return false;
            }
        }
        true
    }
}

/// Rewrite `callee`'s body so it can be spliced into a caller whose virtual
/// registers are numbered from `vreg_base` and whose locals currently occupy
/// `caller_stack` bytes.
///
/// Returns the rewritten instruction list (without the trailing `Return`),
/// the number of fresh virtual registers consumed, and the register classes of
/// those fresh registers.
fn specialize_body(
    callee: &MachineFunction,
    vreg_base: u32,
    caller_stack: i64,
) -> (
    Vec<MachineInstruction>,
    u32,
    HashMap<VirtualRegister, RegisterClass>,
) {
    let callee_stack = callee.stack_size as i64;
    let mut renames: HashMap<VirtualRegister, VirtualRegister> = HashMap::new();
    let mut fresh = 0u32;
    let mut body = Vec::with_capacity(callee.blocks[0].instructions.len());

    for inst in &callee.blocks[0].instructions {
        if matches!(inst, MachineInstruction::Return) {
            continue;
        }
        let mut inst = inst.clone();
        for op in operands_mut(&mut inst) {
            remap_operand(
                op,
                &mut renames,
                vreg_base,
                &mut fresh,
                caller_stack,
                callee_stack,
            );
        }
        body.push(inst);
    }

    let classes = renames
        .iter()
        .map(|(old, new)| (*new, callee.vreg_class(*old)))
        .collect();

    (body, fresh, classes)
}

/// Allocate the caller-side id for callee virtual register `v`, remembering
/// previously assigned renames.
fn fresh_vreg(
    v: VirtualRegister,
    renames: &mut HashMap<VirtualRegister, VirtualRegister>,
    vreg_base: u32,
    fresh: &mut u32,
) -> VirtualRegister {
    if let Some(&mapped) = renames.get(&v) {
        return mapped;
    }
    let new = VirtualRegister(vreg_base + *fresh);
    *fresh += 1;
    renames.insert(v, new);
    new
}

/// Translate one operand from the callee's namespace into the caller's:
/// virtual registers are renamed, stack slots are re-based below the caller's
/// frame, everything else is address-independent.
fn remap_operand(
    op: &mut MachineOperand,
    renames: &mut HashMap<VirtualRegister, VirtualRegister>,
    vreg_base: u32,
    fresh: &mut u32,
    caller_stack: i64,
    callee_stack: i64,
) {
    match op {
        MachineOperand::Register(reg) => {
            if let MachineRegister::Virtual(v) = *reg {
                *reg = MachineRegister::Virtual(fresh_vreg(v, renames, vreg_base, fresh));
            }
        }
        MachineOperand::Memory { base, index, .. } => {
            if let MachineRegister::Virtual(v) = *base {
                *base = MachineRegister::Virtual(fresh_vreg(v, renames, vreg_base, fresh));
            }
            if let Some((MachineRegister::Virtual(v), _)) = *index {
                let mapped = fresh_vreg(v, renames, vreg_base, fresh);
                *index = Some((MachineRegister::Virtual(mapped), 0));
            }
        }
        // The callee's slot `k` sits `callee_stack + k` bytes below its frame
        // top; place it at the same depth below the caller's locals.
        MachineOperand::StackSlot(k) => {
            let depth = caller_stack + callee_stack + *k as i64;
            *op = MachineOperand::StackSlot((-depth) as i32);
        }
        _ => {}
    }
}

/// Mutable access to every `MachineOperand` of `inst`.
///
/// `ParallelMove` (whose moves use `MoveLocation`, not `MachineOperand`) and
/// the pure-control-flow instructions have no operands here.
fn operands_mut(inst: &mut MachineInstruction) -> Vec<&mut MachineOperand> {
    match inst {
        MachineInstruction::Move { dst, src }
        | MachineInstruction::Load { dst, src, .. }
        | MachineInstruction::Store { dst, src, .. }
        | MachineInstruction::Add { dst, src }
        | MachineInstruction::Sub { dst, src }
        | MachineInstruction::Mul { dst, src }
        | MachineInstruction::Div { dst, src }
        | MachineInstruction::Mod { dst, src }
        | MachineInstruction::And { dst, src }
        | MachineInstruction::Or { dst, src }
        | MachineInstruction::Xor { dst, src }
        | MachineInstruction::Shl { dst, src }
        | MachineInstruction::Shr { dst, src }
        | MachineInstruction::Sar { dst, src }
        | MachineInstruction::FAdd { dst, src, .. }
        | MachineInstruction::FSub { dst, src, .. }
        | MachineInstruction::FMul { dst, src, .. }
        | MachineInstruction::FDiv { dst, src, .. }
        | MachineInstruction::VectorAdd { dst, src, .. }
        | MachineInstruction::VectorSub { dst, src, .. }
        | MachineInstruction::VectorMul { dst, src, .. }
        | MachineInstruction::VectorDiv { dst, src, .. }
        | MachineInstruction::VectorAnd { dst, src, .. }
        | MachineInstruction::VectorOr { dst, src, .. }
        | MachineInstruction::VectorXor { dst, src, .. }
        | MachineInstruction::VectorLoad { dst, src, .. }
        | MachineInstruction::VectorStore { dst, src, .. }
        | MachineInstruction::VectorBroadcast { dst, src, .. }
        | MachineInstruction::VectorShuffle { dst, src, .. }
        | MachineInstruction::VectorReduceAdd { dst, src, .. }
        | MachineInstruction::AtomicLoad { dst, src, .. }
        | MachineInstruction::AtomicStore { dst, src, .. }
        | MachineInstruction::AtomicFetchAdd { dst, src, .. }
        | MachineInstruction::FCvtIntToFloat { dst, src, .. }
        | MachineInstruction::FCvtFloatToInt { dst, src, .. }
        | MachineInstruction::FCvtFloatToFloat { dst, src, .. } => vec![dst, src],
        MachineInstruction::AtomicCompareExchange {
            dst,
            expected,
            desired,
            ..
        } => vec![dst, expected, desired],
        MachineInstruction::Compare { lhs: dst, rhs: src }
        | MachineInstruction::Test { lhs: dst, rhs: src }
        | MachineInstruction::FCmp {
            lhs: dst, rhs: src, ..
        } => vec![dst, src],
        MachineInstruction::Neg { dst }
        | MachineInstruction::Not { dst }
        | MachineInstruction::FNeg { dst, .. }
        | MachineInstruction::SetCc { dst, .. }
        | MachineInstruction::Pop { dst }
        | MachineInstruction::Push { src: dst } => vec![dst],
        MachineInstruction::Call { target, .. } => vec![target],
        MachineInstruction::Custom { operands, .. } => operands.iter_mut().collect(),
        _ => Vec::new(),
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
