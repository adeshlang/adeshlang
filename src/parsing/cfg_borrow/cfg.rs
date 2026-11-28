//! Control Flow Graph Construction and Structural Validation
//!
//! Builds a CFG from HIR functions, methods, and statement lists where:
//! - Nodes are basic blocks (sequences of non-branching statements)
//! - Edges represent control flow transitions
//!
//! Handles: if/else, while, for-in, break, continue, jump, return, try/catch,
//! throw, defer (LIFO cleanup on scope/control exits), unsafe, region, and
//! control-flow expressions (conditional ternary, match, short-circuit &&/||/??).

use super::{BorrowStateMap, CfgBorrowState, SourceSpan};
use crate::parsing::hir::{BinOp, HirExpr, HirFunction, HirStmt, HirType};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Block identifier
pub type BlockId = usize;

/// Kind of basic block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Function entry block
    Entry,
    /// Normal code block
    Normal,
    /// Block ends with conditional branch
    Conditional,
    /// Loop header (entry point for while/for)
    LoopHeader,
    /// Block after loop (exit point)
    LoopExit,
    /// Block ends with return
    Return,
    /// Block ends with break
    Break,
    /// Block ends with continue (or loop jump)
    Continue,
    /// Block ends with throw
    Throw,
    /// Block runs deferred cleanup statements on an exit path
    Defer,
    /// Function exit block (cleanup/return)
    Exit,
    /// Try block
    Try,
    /// Catch block
    Catch,
}

/// A basic block in the CFG
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique identifier for this block
    pub id: BlockId,

    /// Kind of block (for special handling)
    pub kind: BlockKind,

    /// Statements in this block (sequential, no internal control flow)
    pub statements: Vec<HirStmt>,

    /// Predecessor block IDs
    pub predecessors: Vec<BlockId>,

    /// Successor block IDs
    pub successors: Vec<BlockId>,

    /// Borrow state at block entry (computed during analysis)
    pub in_state: BorrowStateMap,

    /// Borrow state at block exit (computed during analysis)
    pub out_state: BorrowStateMap,

    /// Source span for this block (for error messages)
    pub span: SourceSpan,

    /// Label for debugging/error messages
    pub label: String,

    /// True if this block is inside an `unsafe` block/function
    pub in_unsafe: bool,
}

impl BasicBlock {
    fn new(id: BlockId, kind: BlockKind) -> Self {
        Self {
            id,
            kind,
            statements: Vec::new(),
            predecessors: Vec::new(),
            successors: Vec::new(),
            in_state: HashMap::new(),
            out_state: HashMap::new(),
            span: SourceSpan::default(),
            label: format!("block_{}", id),
            in_unsafe: false,
        }
    }

    /// Public constructor for unit tests and external graph construction
    pub fn new_with_kind(id: BlockId, kind: BlockKind) -> Self {
        Self::new(id, kind)
    }

    /// Check if this block terminates (no fall-through to sequential code)
    pub fn is_terminated(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Return | BlockKind::Break | BlockKind::Continue | BlockKind::Throw
        )
    }

    /// Set a descriptive label for this block
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// Structural invariant violation in a `ControlFlowGraph`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgValidationError {
    pub function_name: String,
    pub block_id: Option<BlockId>,
    pub edge: Option<(BlockId, BlockId)>,
    pub expected: String,
    pub actual: String,
}

impl fmt::Display for CfgValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CFG[{}]", self.function_name)?;
        if let Some(id) = self.block_id {
            write!(f, " block {}", id)?;
        }
        if let Some((from, to)) = self.edge {
            write!(f, " edge {} -> {}", from, to)?;
        }
        write!(f, ": expected {}; actual {}", self.expected, self.actual)
    }
}

/// Validate all structural graph invariants on a `ControlFlowGraph`.
///
/// Enforces:
/// 1. Non-empty graph with valid `Entry` block (`blocks[entry].kind == Entry`).
/// 2. Every block has `block.id == index` (unique stable identity).
/// 3. Valid block IDs on every predecessor/successor edge; no duplicate edges.
/// 4. Strict symmetry: `u -> v` in `u.successors` iff `u` in `v.predecessors`.
/// 5. Non-empty `exits`, no duplicates, valid IDs, `kind == Exit`, and no outgoing edges.
/// 6. Terminated blocks (`Return`, `Break`, `Continue`, `Throw`) have valid targets
///    and cannot have an unintentional fallthrough edge.
/// 7. `Entry` and `Exit` blocks have valid edge directions (no incoming edge to `Entry`
///    unless loop header is separate; no outgoing edge from `Exit`).
pub fn validate_cfg(cfg: &ControlFlowGraph) -> Result<(), Vec<CfgValidationError>> {
    let mut errors = Vec::new();
    let fn_name = &cfg.function_name;

    if cfg.blocks.is_empty() {
        errors.push(CfgValidationError {
            function_name: fn_name.clone(),
            block_id: None,
            edge: None,
            expected: "at least one basic block (entry)".to_string(),
            actual: "0 blocks".to_string(),
        });
        return Err(errors);
    }

    let n = cfg.blocks.len();

    // 1. Entry block invariants
    if cfg.entry >= n {
        errors.push(CfgValidationError {
            function_name: fn_name.clone(),
            block_id: Some(cfg.entry),
            edge: None,
            expected: format!("entry block ID < {}", n),
            actual: format!("entry = {}", cfg.entry),
        });
    } else if cfg.blocks[cfg.entry].kind != BlockKind::Entry {
        errors.push(CfgValidationError {
            function_name: fn_name.clone(),
            block_id: Some(cfg.entry),
            edge: None,
            expected: "entry block kind == BlockKind::Entry".to_string(),
            actual: format!("{:?}", cfg.blocks[cfg.entry].kind),
        });
    }

    // 2. Exit block invariants
    if cfg.exits.is_empty() {
        errors.push(CfgValidationError {
            function_name: fn_name.clone(),
            block_id: None,
            edge: None,
            expected: "at least one exit block".to_string(),
            actual: "exits is empty".to_string(),
        });
    }

    let mut seen_exits = HashSet::new();
    for &exit_id in &cfg.exits {
        if !seen_exits.insert(exit_id) {
            errors.push(CfgValidationError {
                function_name: fn_name.clone(),
                block_id: Some(exit_id),
                edge: None,
                expected: "unique exit block IDs".to_string(),
                actual: format!("duplicate exit {}", exit_id),
            });
            continue;
        }
        if exit_id >= n {
            errors.push(CfgValidationError {
                function_name: fn_name.clone(),
                block_id: Some(exit_id),
                edge: None,
                expected: format!("exit block ID < {}", n),
                actual: format!("exit = {}", exit_id),
            });
        } else {
            if cfg.blocks[exit_id].kind != BlockKind::Exit {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(exit_id),
                    edge: None,
                    expected: "exit block kind == BlockKind::Exit".to_string(),
                    actual: format!("{:?}", cfg.blocks[exit_id].kind),
                });
            }
            if !cfg.blocks[exit_id].successors.is_empty() {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(exit_id),
                    edge: None,
                    expected: "exit block has no outgoing successors".to_string(),
                    actual: format!("successors = {:?}", cfg.blocks[exit_id].successors),
                });
            }
        }
    }

    // 3. Per-block identity, edge validity, duplicates, symmetry, and termination invariants
    for (idx, block) in cfg.blocks.iter().enumerate() {
        if block.id != idx {
            errors.push(CfgValidationError {
                function_name: fn_name.clone(),
                block_id: Some(idx),
                edge: None,
                expected: format!("block.id == {}", idx),
                actual: format!("block.id == {}", block.id),
            });
        }

        // Duplicate successors & valid range & pred symmetry
        let mut seen_succ = HashSet::new();
        for &succ in &block.successors {
            if !seen_succ.insert(succ) {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((idx, succ)),
                    expected: "unique successor edges".to_string(),
                    actual: format!("duplicate edge {} -> {}", idx, succ),
                });
                continue;
            }
            if succ >= n {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((idx, succ)),
                    expected: format!("valid successor block ID < {}", n),
                    actual: format!("out-of-range successor {}", succ),
                });
                continue;
            }
            if !cfg.blocks[succ].predecessors.contains(&idx) {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((idx, succ)),
                    expected: format!("block {} predecessors contains {}", succ, idx),
                    actual: format!("predecessors = {:?}", cfg.blocks[succ].predecessors),
                });
            }
        }

        // Duplicate predecessors & valid range & succ symmetry
        let mut seen_pred = HashSet::new();
        for &pred in &block.predecessors {
            if !seen_pred.insert(pred) {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((pred, idx)),
                    expected: "unique predecessor edges".to_string(),
                    actual: format!("duplicate predecessor {} -> {}", pred, idx),
                });
                continue;
            }
            if pred >= n {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((pred, idx)),
                    expected: format!("valid predecessor block ID < {}", n),
                    actual: format!("out-of-range predecessor {}", pred),
                });
                continue;
            }
            if !cfg.blocks[pred].successors.contains(&idx) {
                errors.push(CfgValidationError {
                    function_name: fn_name.clone(),
                    block_id: Some(idx),
                    edge: Some((pred, idx)),
                    expected: format!("block {} successors contains {}", pred, idx),
                    actual: format!("successors = {:?}", cfg.blocks[pred].successors),
                });
            }
        }

        // Entry block should not have incoming edges (unless invalid)
        if idx == cfg.entry && !block.predecessors.is_empty() {
            errors.push(CfgValidationError {
                function_name: fn_name.clone(),
                block_id: Some(idx),
                edge: None,
                expected: "entry block has no incoming predecessors".to_string(),
                actual: format!("predecessors = {:?}", block.predecessors),
            });
        }

        // Terminated blocks invariants
        match block.kind {
            BlockKind::Return => {
                if block.successors.is_empty() {
                    errors.push(CfgValidationError {
                        function_name: fn_name.clone(),
                        block_id: Some(idx),
                        edge: None,
                        expected: "Return block has at least 1 target (Defer or Exit)".to_string(),
                        actual: "0 successors".to_string(),
                    });
                }
                for &s in &block.successors {
                    if s < n
                        && !matches!(cfg.blocks[s].kind, BlockKind::Exit | BlockKind::Defer)
                    {
                        errors.push(CfgValidationError {
                            function_name: fn_name.clone(),
                            block_id: Some(idx),
                            edge: Some((idx, s)),
                            expected: "Return block targets Exit or Defer".to_string(),
                            actual: format!("target block {} has kind {:?}", s, cfg.blocks[s].kind),
                        });
                    }
                }
            }
            BlockKind::Throw => {
                if block.successors.is_empty() {
                    errors.push(CfgValidationError {
                        function_name: fn_name.clone(),
                        block_id: Some(idx),
                        edge: None,
                        expected: "Throw block has target (Catch, Defer, or Exit)".to_string(),
                        actual: "0 successors".to_string(),
                    });
                }
                for &s in &block.successors {
                    if s < n
                        && !matches!(
                            cfg.blocks[s].kind,
                            BlockKind::Catch | BlockKind::Defer | BlockKind::Exit
                        )
                    {
                        errors.push(CfgValidationError {
                            function_name: fn_name.clone(),
                            block_id: Some(idx),
                            edge: Some((idx, s)),
                            expected: "Throw block targets Catch, Defer, or Exit".to_string(),
                            actual: format!("target block {} has kind {:?}", s, cfg.blocks[s].kind),
                        });
                    }
                }
            }
            BlockKind::Break => {
                if block.successors.len() != 1 {
                    errors.push(CfgValidationError {
                        function_name: fn_name.clone(),
                        block_id: Some(idx),
                        edge: None,
                        expected: "Break block has exactly 1 target (loop exit or Defer)"
                            .to_string(),
                        actual: format!("{} successors: {:?}", block.successors.len(), block.successors),
                    });
                }
            }
            BlockKind::Continue => {
                if block.successors.len() != 1 {
                    errors.push(CfgValidationError {
                        function_name: fn_name.clone(),
                        block_id: Some(idx),
                        edge: None,
                        expected: "Continue block has exactly 1 target (loop header or Defer)"
                            .to_string(),
                        actual: format!("{} successors: {:?}", block.successors.len(), block.successors),
                    });
                }
            }
            BlockKind::Exit => {
                if !cfg.exits.contains(&idx) {
                    errors.push(CfgValidationError {
                        function_name: fn_name.clone(),
                        block_id: Some(idx),
                        edge: None,
                        expected: "Exit block registered in cfg.exits".to_string(),
                        actual: format!("exits = {:?}", cfg.exits),
                    });
                }
            }
            _ => {}
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Control Flow Graph for a function
#[derive(Debug)]
pub struct ControlFlowGraph {
    /// All basic blocks
    pub blocks: Vec<BasicBlock>,

    /// Entry block ID (always 0)
    pub entry: BlockId,

    /// Exit block ID(s) - may have multiple for early returns
    pub exits: Vec<BlockId>,

    /// Function name for error reporting
    pub function_name: String,

    /// All variables in scope with their initial states
    pub variables: HashMap<String, CfgBorrowState>,
}

impl ControlFlowGraph {
    fn new(function_name: String) -> Self {
        Self {
            blocks: Vec::new(),
            entry: 0,
            exits: Vec::new(),
            function_name,
            variables: HashMap::new(),
        }
    }

    /// Public constructor for unit tests and graph construction
    pub fn new_empty(function_name: String) -> Self {
        Self::new(function_name)
    }

    /// Get a block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(id)
    }

    /// Get a mutable block by ID
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(id)
    }

    /// Get predecessors' out_states for a given block
    pub fn get_predecessor_states(&self, block_id: BlockId) -> Vec<&BorrowStateMap> {
        if let Some(block) = self.get_block(block_id) {
            block
                .predecessors
                .iter()
                .filter_map(|&pred_id| self.get_block(pred_id).map(|b| &b.out_state))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Number of blocks in the CFG
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Check if CFG is empty
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Compute the list of unreachable block IDs from `entry`.
    pub fn unreachable_blocks(&self) -> Vec<BlockId> {
        super::dominators::Reachability::compute(self).unreachable
    }

    /// Serialize the CFG to Graphviz DOT format without omitting any blocks or edges.
    pub fn to_dot(&self) -> String {
        let reach = super::dominators::Reachability::compute(self);
        let mut out = format!("digraph \"{}\" {{\n", self.function_name.replace('"', "\\\""));
        out.push_str(&format!(
            "  // entry={}, exits={:?}, blocks={}\n",
            self.entry,
            self.exits,
            self.blocks.len()
        ));
        for block in &self.blocks {
            let unreach_tag = if reach.is_reachable(block.id) {
                ""
            } else {
                " [UNREACHABLE]"
            };
            let unsafe_tag = if block.in_unsafe { " [unsafe]" } else { "" };
            out.push_str(&format!(
                "  b{} [label=\"b{}: {} ({:?}, stmts={}){}{}\"];\n",
                block.id,
                block.id,
                block.label.replace('"', "\\\""),
                block.kind,
                block.statements.len(),
                unsafe_tag,
                unreach_tag,
            ));
        }
        for block in &self.blocks {
            for &succ in &block.successors {
                out.push_str(&format!("  b{} -> b{};\n", block.id, succ));
            }
        }
        out.push_str("}\n");
        out
    }

    /// Produce a complete human-readable summary of every block, edge, and exit.
    pub fn debug_summary(&self) -> String {
        let reach = super::dominators::Reachability::compute(self);
        let mut out = format!(
            "CFG `{}` (entry={}, exits={:?}, blocks={})\n",
            self.function_name,
            self.entry,
            self.exits,
            self.blocks.len()
        );
        for block in &self.blocks {
            out.push_str(&format!(
                "  block {} `{}` kind={:?} reachable={} unsafe={} stmts={} preds={:?} succs={:?}\n",
                block.id,
                block.label,
                block.kind,
                reach.is_reachable(block.id),
                block.in_unsafe,
                block.statements.len(),
                block.predecessors,
                block.successors
            ));
        }
        out
    }
}

/// Builder for constructing CFGs from HIR functions
pub struct CfgBuilder {
    cfg: ControlFlowGraph,
    next_block_id: BlockId,

    /// Stack of loop contexts for break/continue resolution
    loop_stack: Vec<LoopContext>,

    /// Stack of active `try` catch targets (innermost last)
    try_catch_stack: Vec<TryCatchContext>,

    /// Stack of lexical scopes' deferred statements (innermost scope last)
    scope_defers: Vec<Vec<HirStmt>>,

    /// Nesting depth of `unsafe` blocks/functions
    unsafe_depth: usize,
}

/// Context for resolving break/continue within loops
#[derive(Debug, Clone)]
struct LoopContext {
    /// Loop header block (target for continue / jump)
    header: BlockId,
    /// Blocks that break out of this loop (need to connect to exit)
    break_blocks: Vec<BlockId>,
    /// Blocks that continue this loop (need to connect to header)
    continue_blocks: Vec<BlockId>,
    /// Depth of `scope_defers` when entering the loop body
    defer_scope_depth: usize,
}

/// Context for active `try { ... } catch (err) { ... }`
#[derive(Debug, Clone)]
struct TryCatchContext {
    /// Target catch block when an exception is thrown
    catch_block: BlockId,
    /// Depth of `scope_defers` when entering the try block
    defer_scope_depth: usize,
}

impl CfgBuilder {
    /// Build a CFG from an HIR function
    pub fn build(func: &HirFunction) -> ControlFlowGraph {
        let params: Vec<(String, Option<HirType>)> = func
            .params
            .iter()
            .map(|(n, ty, _)| (n.clone(), ty.clone()))
            .collect();
        Self::build_internal(func.name.clone(), &params, &func.body, func.is_unsafe)
    }

    /// Build a CFG from a statement slice (e.g., class method, lambda, or top-level script)
    pub fn build_from_stmts(
        name: String,
        params: &[(String, Option<HirType>)],
        body: &[HirStmt],
    ) -> ControlFlowGraph {
        Self::build_internal(name, params, body, false)
    }

    fn build_internal(
        name: String,
        params: &[(String, Option<HirType>)],
        body: &[HirStmt],
        is_unsafe: bool,
    ) -> ControlFlowGraph {
        let mut builder = Self {
            cfg: ControlFlowGraph::new(name),
            next_block_id: 0,
            loop_stack: Vec::new(),
            try_catch_stack: Vec::new(),
            scope_defers: vec![Vec::new()],
            unsafe_depth: usize::from(is_unsafe),
        };

        // Create entry block
        let entry_id = builder.create_block(BlockKind::Entry);
        builder.cfg.entry = entry_id;
        if let Some(entry) = builder.cfg.get_block_mut(entry_id) {
            entry.label = "entry".to_string();
        }

        // Create exit block (target for all returns)
        let exit_id = builder.create_block(BlockKind::Exit);
        builder.cfg.exits.push(exit_id);
        if let Some(exit) = builder.cfg.get_block_mut(exit_id) {
            exit.label = "exit".to_string();
        }

        // Initialize parameter states
        for (param_name, _) in params {
            builder
                .cfg
                .variables
                .insert(param_name.clone(), CfgBorrowState::Unborrowed);
        }

        // Build blocks from function body
        let mut current_block = entry_id;
        for stmt in body.iter() {
            current_block = builder.process_stmt(stmt, current_block, exit_id);
        }

        // Emit any top-level deferred statements before normal function exit
        if !builder
            .cfg
            .get_block(current_block)
            .is_some_and(BasicBlock::is_terminated)
        {
            current_block = builder.emit_defers_from_depth(0, current_block, exit_id);
        }

        // Connect final block to exit if not already terminated
        if let Some(block) = builder.cfg.get_block(current_block) {
            if !block.is_terminated() && !block.successors.contains(&exit_id) {
                builder.add_edge(current_block, exit_id);
            }
        }

        builder.cfg
    }

    /// Create a new basic block
    fn create_block(&mut self, kind: BlockKind) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        let mut block = BasicBlock::new(id, kind);
        block.in_unsafe = self.unsafe_depth > 0;
        self.cfg.blocks.push(block);
        id
    }

    /// Add an edge between two blocks
    fn add_edge(&mut self, from: BlockId, to: BlockId) {
        if let Some(from_block) = self.cfg.get_block_mut(from) {
            if !from_block.successors.contains(&to) {
                from_block.successors.push(to);
            }
        }
        if let Some(to_block) = self.cfg.get_block_mut(to) {
            if !to_block.predecessors.contains(&from) {
                to_block.predecessors.push(from);
            }
        }
    }

    /// If `current_block` is the function `entry` block, transition to a fresh
    /// `Normal` block so `blocks[entry]` always preserves `BlockKind::Entry`.
    fn ensure_non_entry_block(&mut self, current_block: BlockId) -> BlockId {
        if current_block == self.cfg.entry {
            let next = self.create_block(BlockKind::Normal);
            self.add_edge(current_block, next);
            next
        } else {
            current_block
        }
    }

    /// Check whether any active defer statements exist at or above `from_depth`.
    fn has_defers_from_depth(&self, from_depth: usize) -> bool {
        self.scope_defers
            .iter()
            .skip(from_depth)
            .any(|scope| !scope.is_empty())
    }

    /// Emit deferred statements in LIFO order from the innermost scope down to `from_depth`.
    /// Does not pop `scope_defers` when `consume_top` is false (e.g., on early return inside a branch).
    fn emit_defers_from_depth(
        &mut self,
        from_depth: usize,
        mut current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        if !self.has_defers_from_depth(from_depth) {
            return current_block;
        }
        let deferred_stmts: Vec<HirStmt> = self
            .scope_defers
            .iter()
            .skip(from_depth)
            .rev()
            .flat_map(|scope| scope.iter().rev().cloned())
            .collect();

        for deferred in deferred_stmts {
            let defer_block = self.create_block(BlockKind::Defer);
            if let Some(b) = self.cfg.get_block_mut(defer_block) {
                b.label = format!("defer_{}", defer_block);
            }
            self.add_edge(current_block, defer_block);
            current_block = self.process_stmt(&deferred, defer_block, exit_block);
        }
        current_block
    }

    /// Check if an expression contains internal control flow (Conditional, Match, or short-circuit).
    fn expr_has_control_flow(expr: &HirExpr) -> bool {
        match expr {
            HirExpr::Conditional(..) | HirExpr::Match(..) => true,
            HirExpr::BinaryOp(lhs, op, rhs) => {
                matches!(op, BinOp::And | BinOp::Or | BinOp::NullCoalesce)
                    || Self::expr_has_control_flow(lhs)
                    || Self::expr_has_control_flow(rhs)
            }
            HirExpr::UnaryOp(_, inner)
            | HirExpr::Deref(inner)
            | HirExpr::Share(inner)
            | HirExpr::Downgrade(inner)
            | HirExpr::Move(inner)
            | HirExpr::Borrow(inner, _)
            | HirExpr::BorrowImmut(inner)
            | HirExpr::BorrowMut(inner)
            | HirExpr::Await(inner)
            | HirExpr::Spawn(inner)
            | HirExpr::NonNull(inner)
            | HirExpr::Spread(inner)
            | HirExpr::Update(inner, _, _)
            | HirExpr::Cast(inner, _)
            | HirExpr::Format(inner, _)
            | HirExpr::Free(inner)
            | HirExpr::Alloc(_, inner)
            | HirExpr::MemberAccess(inner, _)
            | HirExpr::OptionalGet(inner, _)
            | HirExpr::StoreVar(_, inner)
            | HirExpr::AssignTuple(_, inner)
            | HirExpr::AssignObject(_, inner) => Self::expr_has_control_flow(inner),
            HirExpr::Index(a, b) | HirExpr::Range(a, b, _) | HirExpr::SetMember(a, _, b) => {
                Self::expr_has_control_flow(a) || Self::expr_has_control_flow(b)
            }
            HirExpr::Call(callee, args, _) => {
                Self::expr_has_control_flow(callee) || args.iter().any(Self::expr_has_control_flow)
            }
            HirExpr::MethodCall(recv, _, args) => {
                Self::expr_has_control_flow(recv) || args.iter().any(Self::expr_has_control_flow)
            }
            HirExpr::NewInstance(_, args)
            | HirExpr::ArrayLiteral(args)
            | HirExpr::SetLiteral(args)
            | HirExpr::TupleLiteral(args) => args.iter().any(Self::expr_has_control_flow),
            HirExpr::DictLiteral(pairs) => pairs
                .iter()
                .any(|(k, v)| Self::expr_has_control_flow(k) || Self::expr_has_control_flow(v)),
            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                fields.iter().any(|(_, v)| Self::expr_has_control_flow(v))
            }
            HirExpr::Literal(_)
            | HirExpr::LoadVar(_)
            | HirExpr::Lambda(..)
            | HirExpr::This
            | HirExpr::Super => false,
        }
    }

    /// Lower an expression that may contain internal control flow (`Conditional`, `Match`,
    /// short-circuit `&&`/`||`/`??`) into CFG branches, returning the join block after the
    /// expression finishes evaluating.
    fn lower_expr_cfg(
        &mut self,
        expr: &HirExpr,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        if !Self::expr_has_control_flow(expr) {
            if let Some(block) = self.cfg.get_block_mut(current_block) {
                block.statements.push(HirStmt::Expr(expr.clone()));
            }
            return current_block;
        }

        match expr {
            HirExpr::Conditional(cond, then_expr, else_expr) => {
                let cond_exit = self.lower_expr_cfg(cond, current_block, exit_block);
                let cond_exit = self.ensure_non_entry_block(cond_exit);
                if let Some(b) = self.cfg.get_block_mut(cond_exit) {
                    b.kind = BlockKind::Conditional;
                    b.label = format!("ternary_cond_{}", cond_exit);
                }

                let then_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(then_block) {
                    b.label = format!("ternary_then_{}", then_block);
                }
                self.add_edge(cond_exit, then_block);
                let then_exit = self.lower_expr_cfg(then_expr, then_block, exit_block);

                let else_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(else_block) {
                    b.label = format!("ternary_else_{}", else_block);
                }
                self.add_edge(cond_exit, else_block);
                let else_exit = self.lower_expr_cfg(else_expr, else_block, exit_block);

                let join_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(join_block) {
                    b.label = format!("ternary_join_{}", join_block);
                }
                if !self.cfg.get_block(then_exit).is_some_and(BasicBlock::is_terminated) {
                    self.add_edge(then_exit, join_block);
                }
                if !self.cfg.get_block(else_exit).is_some_and(BasicBlock::is_terminated) {
                    self.add_edge(else_exit, join_block);
                }
                join_block
            }

            HirExpr::Match(scrutinee, arms) => {
                let scrut_exit = self.lower_expr_cfg(scrutinee, current_block, exit_block);
                let scrut_exit = self.ensure_non_entry_block(scrut_exit);
                if let Some(b) = self.cfg.get_block_mut(scrut_exit) {
                    b.kind = BlockKind::Conditional;
                    b.label = format!("match_scrutinee_{}", scrut_exit);
                }

                if arms.is_empty() {
                    return scrut_exit;
                }

                let join_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(join_block) {
                    b.label = format!("match_join_{}", join_block);
                }

                for (idx, (pattern, arm_expr)) in arms.iter().enumerate() {
                    let arm_block = self.create_block(BlockKind::Normal);
                    if let Some(b) = self.cfg.get_block_mut(arm_block) {
                        b.label = format!("match_arm_{}_{}", idx, arm_block);
                    }
                    self.register_pattern_vars(pattern);
                    self.add_edge(scrut_exit, arm_block);
                    let arm_exit = self.lower_expr_cfg(arm_expr, arm_block, exit_block);
                    if !self.cfg.get_block(arm_exit).is_some_and(BasicBlock::is_terminated) {
                        self.add_edge(arm_exit, join_block);
                    }
                }
                join_block
            }

            HirExpr::BinaryOp(lhs, op, rhs)
                if matches!(op, BinOp::And | BinOp::Or | BinOp::NullCoalesce) =>
            {
                let lhs_exit = self.lower_expr_cfg(lhs, current_block, exit_block);
                let lhs_exit = self.ensure_non_entry_block(lhs_exit);
                if let Some(b) = self.cfg.get_block_mut(lhs_exit) {
                    b.kind = BlockKind::Conditional;
                    b.label = format!("short_circuit_lhs_{}", lhs_exit);
                }

                // Short-circuit path skips rhs
                let skip_rhs = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(skip_rhs) {
                    b.label = format!("short_circuit_skip_{}", skip_rhs);
                }
                self.add_edge(lhs_exit, skip_rhs);

                // Non-short-circuit path evaluates rhs
                let rhs_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(rhs_block) {
                    b.label = format!("short_circuit_rhs_{}", rhs_block);
                }
                self.add_edge(lhs_exit, rhs_block);
                let rhs_exit = self.lower_expr_cfg(rhs, rhs_block, exit_block);

                let join_block = self.create_block(BlockKind::Normal);
                if let Some(b) = self.cfg.get_block_mut(join_block) {
                    b.label = format!("short_circuit_join_{}", join_block);
                }
                self.add_edge(skip_rhs, join_block);
                if !self.cfg.get_block(rhs_exit).is_some_and(BasicBlock::is_terminated) {
                    self.add_edge(rhs_exit, join_block);
                }
                join_block
            }

            HirExpr::BinaryOp(lhs, _, rhs) => {
                // Strict left-to-right evaluation order
                let after_lhs = self.lower_expr_cfg(lhs, current_block, exit_block);
                self.lower_expr_cfg(rhs, after_lhs, exit_block)
            }

            HirExpr::Call(callee, args, _) => {
                let mut cur = self.lower_expr_cfg(callee, current_block, exit_block);
                for arg in args {
                    cur = self.lower_expr_cfg(arg, cur, exit_block);
                }
                cur
            }

            HirExpr::MethodCall(recv, _, args) => {
                let mut cur = self.lower_expr_cfg(recv, current_block, exit_block);
                for arg in args {
                    cur = self.lower_expr_cfg(arg, cur, exit_block);
                }
                cur
            }

            HirExpr::StoreVar(name, inner) => {
                let after_val = self.lower_expr_cfg(inner, current_block, exit_block);
                if let Some(b) = self.cfg.get_block_mut(after_val) {
                    b.statements.push(HirStmt::Expr(HirExpr::StoreVar(
                        name.clone(),
                        Box::new(HirExpr::Literal(crate::parsing::hir::HirLiteral::Null)),
                    )));
                }
                after_val
            }

            HirExpr::UnaryOp(_, inner)
            | HirExpr::Deref(inner)
            | HirExpr::Share(inner)
            | HirExpr::Downgrade(inner)
            | HirExpr::Move(inner)
            | HirExpr::Borrow(inner, _)
            | HirExpr::BorrowImmut(inner)
            | HirExpr::BorrowMut(inner)
            | HirExpr::Await(inner)
            | HirExpr::Spawn(inner)
            | HirExpr::NonNull(inner)
            | HirExpr::Spread(inner)
            | HirExpr::Update(inner, _, _)
            | HirExpr::Cast(inner, _)
            | HirExpr::Format(inner, _)
            | HirExpr::Free(inner)
            | HirExpr::Alloc(_, inner)
            | HirExpr::MemberAccess(inner, _)
            | HirExpr::OptionalGet(inner, _)
            | HirExpr::AssignTuple(_, inner)
            | HirExpr::AssignObject(_, inner) => self.lower_expr_cfg(inner, current_block, exit_block),

            HirExpr::Index(a, b) | HirExpr::Range(a, b, _) | HirExpr::SetMember(a, _, b) => {
                let after_a = self.lower_expr_cfg(a, current_block, exit_block);
                self.lower_expr_cfg(b, after_a, exit_block)
            }

            HirExpr::NewInstance(_, args)
            | HirExpr::ArrayLiteral(args)
            | HirExpr::SetLiteral(args)
            | HirExpr::TupleLiteral(args) => {
                let mut cur = current_block;
                for arg in args {
                    cur = self.lower_expr_cfg(arg, cur, exit_block);
                }
                cur
            }

            HirExpr::DictLiteral(pairs) => {
                let mut cur = current_block;
                for (k, v) in pairs {
                    cur = self.lower_expr_cfg(k, cur, exit_block);
                    cur = self.lower_expr_cfg(v, cur, exit_block);
                }
                cur
            }

            HirExpr::ObjectLiteral(fields) | HirExpr::StructLiteral(_, fields) => {
                let mut cur = current_block;
                for (_, v) in fields {
                    cur = self.lower_expr_cfg(v, cur, exit_block);
                }
                cur
            }

            HirExpr::Literal(_)
            | HirExpr::LoadVar(_)
            | HirExpr::Lambda(..)
            | HirExpr::This
            | HirExpr::Super => current_block,
        }
    }

    fn register_pattern_vars(&mut self, pattern: &crate::parsing::hir::HirPattern) {
        use crate::parsing::hir::HirPattern;
        match pattern {
            HirPattern::Variable(name) => {
                self.cfg
                    .variables
                    .entry(name.clone())
                    .or_insert(CfgBorrowState::Unborrowed);
            }
            HirPattern::Or(a, b) => {
                self.register_pattern_vars(a);
                self.register_pattern_vars(b);
            }
            HirPattern::EnumVariant(_, inner) => {
                for p in inner {
                    self.register_pattern_vars(p);
                }
            }
            HirPattern::Literal(_) | HirPattern::Wildcard => {}
        }
    }

    /// Process a statement and return the current block after processing
    fn process_stmt(
        &mut self,
        stmt: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        match stmt {
            HirStmt::Let { name, init, .. } => {
                self.cfg
                    .variables
                    .insert(name.clone(), CfgBorrowState::Unborrowed);
                if let Some(init_expr) = init {
                    if Self::expr_has_control_flow(init_expr) {
                        let after_init = self.lower_expr_cfg(init_expr, current_block, exit_block);
                        if let Some(block) = self.cfg.get_block_mut(after_init) {
                            block.statements.push(HirStmt::Let {
                                name: name.clone(),
                                ty: None,
                                init: None,
                                is_const: false,
                                is_borrowed: None,
                                is_atomic: false,
                            });
                        }
                        return after_init;
                    }
                }
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::LetTuple { names, init, .. } => {
                for name in names {
                    self.cfg
                        .variables
                        .insert(name.clone(), CfgBorrowState::Unborrowed);
                }
                if let Some(init_expr) = init {
                    if Self::expr_has_control_flow(init_expr) {
                        return self.lower_expr_cfg(init_expr, current_block, exit_block);
                    }
                }
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::Assign { target, value, .. } => {
                if Self::expr_has_control_flow(target) || Self::expr_has_control_flow(value) {
                    let after_target = self.lower_expr_cfg(target, current_block, exit_block);
                    let after_val = self.lower_expr_cfg(value, after_target, exit_block);
                    if let HirExpr::LoadVar(name) = target {
                        if let Some(block) = self.cfg.get_block_mut(after_val) {
                            block.statements.push(HirStmt::Assign {
                                target: HirExpr::LoadVar(name.clone()),
                                value: HirExpr::Literal(crate::parsing::hir::HirLiteral::Null),
                                is_move: false,
                            });
                        }
                    }
                    return after_val;
                }
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::Expr(expr) => {
                if Self::expr_has_control_flow(expr) {
                    return self.lower_expr_cfg(expr, current_block, exit_block);
                }
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::If {
                cond,
                then_branch,
                else_branch,
            } => self.process_if(
                cond,
                then_branch,
                else_branch.as_deref(),
                current_block,
                exit_block,
            ),

            HirStmt::While { cond, body } => {
                self.process_while(cond, body, current_block, exit_block)
            }

            HirStmt::ForIn { var, iter, body } => {
                self.process_for_in(var, iter, body, current_block, exit_block)
            }

            HirStmt::Return(expr) => {
                let ret_block = if let Some(e) = expr {
                    if Self::expr_has_control_flow(e) {
                        self.lower_expr_cfg(e, current_block, exit_block)
                    } else {
                        if let Some(block) = self.cfg.get_block_mut(current_block) {
                            block.statements.push(stmt.clone());
                        }
                        current_block
                    }
                } else {
                    if let Some(block) = self.cfg.get_block_mut(current_block) {
                        block.statements.push(stmt.clone());
                    }
                    current_block
                };

                // Run all active defers before returning to exit_block
                let after_defers = self.emit_defers_from_depth(0, ret_block, exit_block);
                let term_block = self.ensure_non_entry_block(after_defers);
                if term_block == ret_block {
                    if let Some(block) = self.cfg.get_block_mut(term_block) {
                        block.kind = BlockKind::Return;
                    }
                    self.add_edge(term_block, exit_block);
                } else {
                    if let Some(b) = self.cfg.get_block_mut(term_block) {
                        b.kind = BlockKind::Return;
                        b.label = format!("return_{}", term_block);
                    }
                    self.add_edge(term_block, exit_block);
                }

                // Unreachable block for dead code after return
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_return".to_string();
                }
                dead_block
            }

            HirStmt::Throw(expr) => {
                let throw_block = if Self::expr_has_control_flow(expr) {
                    self.lower_expr_cfg(expr, current_block, exit_block)
                } else {
                    if let Some(block) = self.cfg.get_block_mut(current_block) {
                        block.statements.push(stmt.clone());
                    }
                    current_block
                };

                let (target_block, defer_depth) = if let Some(try_ctx) = self.try_catch_stack.last()
                {
                    (try_ctx.catch_block, try_ctx.defer_scope_depth)
                } else {
                    (exit_block, 0)
                };

                let after_defers =
                    self.emit_defers_from_depth(defer_depth, throw_block, exit_block);
                let term_block = self.ensure_non_entry_block(after_defers);
                if let Some(block) = self.cfg.get_block_mut(term_block) {
                    block.kind = BlockKind::Throw;
                    block.label = format!("throw_{}", term_block);
                }
                self.add_edge(term_block, target_block);

                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_throw".to_string();
                }
                dead_block
            }

            HirStmt::Break => {
                let defer_depth = self
                    .loop_stack
                    .last()
                    .map(|ctx| ctx.defer_scope_depth)
                    .unwrap_or(self.scope_defers.len());
                let after_defers =
                    self.emit_defers_from_depth(defer_depth, current_block, exit_block);
                let break_block = self.ensure_non_entry_block(after_defers);
                if let Some(block) = self.cfg.get_block_mut(break_block) {
                    block.kind = BlockKind::Break;
                }
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.break_blocks.push(break_block);
                } else {
                    // Break outside of loop — route to exit so graph stays structurally connected,
                    // though loop semantics would reject this earlier in semantic checking.
                    self.add_edge(break_block, exit_block);
                }
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_break".to_string();
                }
                dead_block
            }

            HirStmt::Continue => {
                let defer_depth = self
                    .loop_stack
                    .last()
                    .map(|ctx| ctx.defer_scope_depth)
                    .unwrap_or(self.scope_defers.len());
                let after_defers =
                    self.emit_defers_from_depth(defer_depth, current_block, exit_block);
                let cont_block = self.ensure_non_entry_block(after_defers);
                if let Some(block) = self.cfg.get_block_mut(cont_block) {
                    block.kind = BlockKind::Continue;
                }
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_blocks.push(cont_block);
                } else {
                    self.add_edge(cont_block, exit_block);
                }
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_continue".to_string();
                }
                dead_block
            }

            HirStmt::Jump(expr) => {
                let after_expr = self.lower_expr_cfg(expr, current_block, exit_block);
                let defer_depth = self
                    .loop_stack
                    .last()
                    .map(|ctx| ctx.defer_scope_depth)
                    .unwrap_or(self.scope_defers.len());
                let after_defers =
                    self.emit_defers_from_depth(defer_depth, after_expr, exit_block);
                let jump_block = self.ensure_non_entry_block(after_defers);
                if let Some(block) = self.cfg.get_block_mut(jump_block) {
                    block.kind = BlockKind::Continue;
                    block.label = format!("jump_{}", jump_block);
                }
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_blocks.push(jump_block);
                } else {
                    self.add_edge(jump_block, exit_block);
                }
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_jump".to_string();
                }
                dead_block
            }

            HirStmt::Block(stmts) => {
                let scope_depth = self.scope_defers.len();
                self.scope_defers.push(Vec::new());
                let mut block = current_block;
                for s in stmts {
                    block = self.process_stmt(s, block, exit_block);
                }
                if !self
                    .cfg
                    .get_block(block)
                    .is_some_and(BasicBlock::is_terminated)
                {
                    block = self.emit_defers_from_depth(scope_depth, block, exit_block);
                }
                self.scope_defers.pop();
                block
            }

            HirStmt::Defer(body) => {
                if let Some(scope) = self.scope_defers.last_mut() {
                    scope.push((**body).clone());
                }
                current_block
            }

            HirStmt::TryCatch {
                try_block,
                error_name,
                catch_block,
            } => self.process_try_catch(
                try_block,
                error_name,
                catch_block,
                current_block,
                exit_block,
            ),

            HirStmt::FunctionDef { .. } => {
                // Nested function definitions have their own CFG scope.
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::Region { name, body } => {
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(HirStmt::Expr(HirExpr::Literal(
                        crate::parsing::hir::HirLiteral::String(format!("region:{}", name)),
                    )));
                }
                self.process_stmt(body, current_block, exit_block)
            }

            HirStmt::Unsafe(body) => {
                // Record the Unsafe marker in current block if body is empty, and recurse
                // through `body` with `unsafe_depth` incremented so CFG edges, exits, and
                // moves inside unsafe blocks are faithfully modeled.
                self.unsafe_depth += 1;
                let unsafe_entry = if self.cfg.get_block(current_block).is_some_and(|b| b.in_unsafe)
                {
                    current_block
                } else {
                    let b = self.create_block(BlockKind::Normal);
                    if let Some(blk) = self.cfg.get_block_mut(b) {
                        blk.label = format!("unsafe_{}", b);
                    }
                    self.add_edge(current_block, b);
                    b
                };
                let unsafe_exit = self.process_stmt(body, unsafe_entry, exit_block);
                self.unsafe_depth -= 1;
                if self
                    .cfg
                    .get_block(unsafe_exit)
                    .is_some_and(BasicBlock::is_terminated)
                {
                    unsafe_exit
                } else {
                    let after_unsafe = self.create_block(BlockKind::Normal);
                    if let Some(blk) = self.cfg.get_block_mut(after_unsafe) {
                        blk.label = format!("after_unsafe_{}", after_unsafe);
                    }
                    self.add_edge(unsafe_exit, after_unsafe);
                    after_unsafe
                }
            }

            HirStmt::ClassDef(_)
            | HirStmt::Extend { .. }
            | HirStmt::Import { .. }
            | HirStmt::ImportDefault { .. }
            | HirStmt::ImportNames { .. } => {
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }
        }
    }

    /// Process an if statement
    fn process_if(
        &mut self,
        cond: &HirExpr,
        then_branch: &HirStmt,
        else_branch: Option<&HirStmt>,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        let cond_block = if Self::expr_has_control_flow(cond) {
            self.lower_expr_cfg(cond, current_block, exit_block)
        } else {
            if let Some(block) = self.cfg.get_block_mut(current_block) {
                block.statements.push(HirStmt::Expr(cond.clone()));
            }
            current_block
        };
        let cond_block = self.ensure_non_entry_block(cond_block);

        if let Some(block) = self.cfg.get_block_mut(cond_block) {
            block.kind = BlockKind::Conditional;
            block.label = format!("if_cond_{}", cond_block);
        }

        // Create then block
        let then_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(then_block) {
            block.label = format!("then_{}", then_block);
        }
        self.add_edge(cond_block, then_block);
        let then_exit = self.process_stmt(then_branch, then_block, exit_block);

        // Create else block (or empty fall-through)
        let else_exit = if let Some(else_stmt) = else_branch {
            let else_block = self.create_block(BlockKind::Normal);
            if let Some(block) = self.cfg.get_block_mut(else_block) {
                block.label = format!("else_{}", else_block);
            }
            self.add_edge(cond_block, else_block);
            self.process_stmt(else_stmt, else_block, exit_block)
        } else {
            let else_block = self.create_block(BlockKind::Normal);
            if let Some(block) = self.cfg.get_block_mut(else_block) {
                block.label = format!("no_else_{}", else_block);
            }
            self.add_edge(cond_block, else_block);
            else_block
        };

        // Create join block
        let join_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(join_block) {
            block.label = format!("if_join_{}", join_block);
        }

        if let Some(block) = self.cfg.get_block(then_exit) {
            if !block.is_terminated() {
                self.add_edge(then_exit, join_block);
            }
        }
        if let Some(block) = self.cfg.get_block(else_exit) {
            if !block.is_terminated() {
                self.add_edge(else_exit, join_block);
            }
        }

        join_block
    }

    /// Process a while loop
    fn process_while(
        &mut self,
        cond: &HirExpr,
        body: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        let header_block = self.create_block(BlockKind::LoopHeader);
        if let Some(block) = self.cfg.get_block_mut(header_block) {
            block.label = format!("while_header_{}", header_block);
        }
        self.add_edge(current_block, header_block);

        let cond_exit = if Self::expr_has_control_flow(cond) {
            self.lower_expr_cfg(cond, header_block, exit_block)
        } else {
            if let Some(block) = self.cfg.get_block_mut(header_block) {
                block.statements.push(HirStmt::Expr(cond.clone()));
            }
            header_block
        };

        let loop_exit = self.create_block(BlockKind::LoopExit);
        if let Some(block) = self.cfg.get_block_mut(loop_exit) {
            block.label = format!("while_exit_{}", loop_exit);
        }
        self.add_edge(cond_exit, loop_exit);

        self.loop_stack.push(LoopContext {
            header: header_block,
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
            defer_scope_depth: self.scope_defers.len(),
        });

        let body_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(body_block) {
            block.label = format!("while_body_{}", body_block);
        }
        self.add_edge(cond_exit, body_block);
        let body_exit = self.process_stmt(body, body_block, exit_block);

        if let Some(block) = self.cfg.get_block(body_exit) {
            if !block.is_terminated() {
                self.add_edge(body_exit, header_block);
            }
        }

        if let Some(loop_ctx) = self.loop_stack.pop() {
            for break_block in loop_ctx.break_blocks {
                self.add_edge(break_block, loop_exit);
            }
            for continue_block in loop_ctx.continue_blocks {
                self.add_edge(continue_block, loop_ctx.header);
            }
        }

        loop_exit
    }

    /// Process a for-in loop
    fn process_for_in(
        &mut self,
        var: &str,
        iter: &HirExpr,
        body: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        self.cfg
            .variables
            .insert(var.to_string(), CfgBorrowState::Unborrowed);

        let iter_eval_block = self.lower_expr_cfg(iter, current_block, exit_block);

        let header_block = self.create_block(BlockKind::LoopHeader);
        if let Some(block) = self.cfg.get_block_mut(header_block) {
            block.label = format!("for_header_{}", header_block);
        }
        self.add_edge(iter_eval_block, header_block);

        let loop_exit = self.create_block(BlockKind::LoopExit);
        if let Some(block) = self.cfg.get_block_mut(loop_exit) {
            block.label = format!("for_exit_{}", loop_exit);
        }
        self.add_edge(header_block, loop_exit);

        self.loop_stack.push(LoopContext {
            header: header_block,
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
            defer_scope_depth: self.scope_defers.len(),
        });

        let body_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(body_block) {
            block.label = format!("for_body_{}", body_block);
        }
        self.add_edge(header_block, body_block);
        let body_exit = self.process_stmt(body, body_block, exit_block);

        if let Some(block) = self.cfg.get_block(body_exit) {
            if !block.is_terminated() {
                self.add_edge(body_exit, header_block);
            }
        }

        if let Some(loop_ctx) = self.loop_stack.pop() {
            for break_block in loop_ctx.break_blocks {
                self.add_edge(break_block, loop_exit);
            }
            for continue_block in loop_ctx.continue_blocks {
                self.add_edge(continue_block, loop_ctx.header);
            }
        }

        loop_exit
    }

    /// Process a try-catch block
    fn process_try_catch(
        &mut self,
        try_block: &HirStmt,
        error_name: &str,
        catch_block: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        if !error_name.is_empty() {
            self.cfg
                .variables
                .entry(error_name.to_string())
                .or_insert(CfgBorrowState::Unborrowed);
        }

        let try_block_id = self.create_block(BlockKind::Try);
        if let Some(block) = self.cfg.get_block_mut(try_block_id) {
            block.label = format!("try_{}", try_block_id);
        }
        self.add_edge(current_block, try_block_id);

        // Pre-allocate catch block so `throw` inside `try_block` can edge directly to it
        let catch_block_id = self.create_block(BlockKind::Catch);
        if let Some(block) = self.cfg.get_block_mut(catch_block_id) {
            block.label = format!("catch_{}", catch_block_id);
        }

        self.try_catch_stack.push(TryCatchContext {
            catch_block: catch_block_id,
            defer_scope_depth: self.scope_defers.len(),
        });
        let try_exit = self.process_stmt(try_block, try_block_id, exit_block);
        self.try_catch_stack.pop();

        // Any block inside the try region can potentially throw (e.g. via calls or runtime checks).
        // Connect try entry and non-terminated try exit to catch so exception paths are conservatively modeled.
        self.add_edge(try_block_id, catch_block_id);
        if try_exit != try_block_id
            && !self
                .cfg
                .get_block(try_exit)
                .is_some_and(BasicBlock::is_terminated)
        {
            self.add_edge(try_exit, catch_block_id);
        }

        let catch_exit = self.process_stmt(catch_block, catch_block_id, exit_block);

        let join_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(join_block) {
            block.label = format!("try_catch_join_{}", join_block);
        }

        if let Some(block) = self.cfg.get_block(try_exit) {
            if !block.is_terminated() {
                self.add_edge(try_exit, join_block);
            }
        }
        if let Some(block) = self.cfg.get_block(catch_exit) {
            if !block.is_terminated() {
                self.add_edge(catch_exit, join_block);
            }
        }

        join_block
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::hir::{HirLiteral, HirPattern, HirType};
    use std::sync::Arc;

    fn make_test_function(body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_string(),
            params: vec![("x".to_string(), Some(HirType::Int), None)],
            body: Arc::new(body),
            ret_type: None,
            is_async: false,
            decorators: Vec::new(),
            is_exported: false,
            move_params: Vec::new(),
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        }
    }

    #[test]
    fn test_empty_function() {
        let func = make_test_function(vec![]);
        let cfg = CfgBuilder::build(&func);

        assert_eq!(cfg.function_name, "test");
        assert_eq!(cfg.blocks.len(), 2); // entry + exit
        assert_eq!(cfg.entry, 0);
        assert!(cfg.exits.contains(&1));
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_simple_let() {
        let func = make_test_function(vec![HirStmt::Let {
            name: "y".to_string(),
            ty: Some(HirType::Int),
            init: Some(HirExpr::Literal(HirLiteral::Int(42))),
            is_const: false,
            is_borrowed: None,
            is_atomic: false,
        }]);
        let cfg = CfgBuilder::build(&func);

        assert_eq!(cfg.blocks[0].statements.len(), 1);
        assert!(cfg.variables.contains_key("y"));
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_if_else() {
        let func = make_test_function(vec![HirStmt::If {
            cond: HirExpr::LoadVar("x".to_string()),
            then_branch: Box::new(HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(1)))),
            else_branch: Some(Box::new(HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(
                2,
            ))))),
        }]);
        let cfg = CfgBuilder::build(&func);

        assert!(cfg.blocks.len() >= 5);
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_while_loop() {
        let func = make_test_function(vec![HirStmt::While {
            cond: HirExpr::LoadVar("x".to_string()),
            body: Box::new(HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(1)))),
        }]);
        let cfg = CfgBuilder::build(&func);

        let has_loop_header = cfg.blocks.iter().any(|b| b.kind == BlockKind::LoopHeader);
        assert!(has_loop_header);
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_return_terminates() {
        let func = make_test_function(vec![
            HirStmt::Return(Some(HirExpr::Literal(HirLiteral::Int(42)))),
            HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(0))),
        ]);
        let cfg = CfgBuilder::build(&func);

        let return_block = cfg.blocks.iter().find(|b| b.kind == BlockKind::Return);
        assert!(return_block.is_some());
        assert!(return_block.unwrap().is_terminated());
        assert!(!cfg.unreachable_blocks().is_empty());
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_break_continue() {
        let func = make_test_function(vec![HirStmt::While {
            cond: HirExpr::LoadVar("x".to_string()),
            body: Box::new(HirStmt::Block(vec![HirStmt::If {
                cond: HirExpr::LoadVar("x".to_string()),
                then_branch: Box::new(HirStmt::Break),
                else_branch: Some(Box::new(HirStmt::Continue)),
            }])),
        }]);
        let cfg = CfgBuilder::build(&func);

        let has_break = cfg.blocks.iter().any(|b| b.kind == BlockKind::Break);
        let has_continue = cfg.blocks.iter().any(|b| b.kind == BlockKind::Continue);
        assert!(has_break);
        assert!(has_continue);
        assert!(validate_cfg(&cfg).is_ok());
    }

    #[test]
    fn test_throw_inside_try_edges_to_catch_and_runs_defer() {
        let func = make_test_function(vec![HirStmt::TryCatch {
            try_block: Box::new(HirStmt::Block(vec![
                HirStmt::Defer(Box::new(HirStmt::Expr(HirExpr::LoadVar(
                    "cleanup".to_string(),
                )))),
                HirStmt::Throw(HirExpr::Literal(HirLiteral::String("boom".to_string()))),
            ])),
            error_name: "err".to_string(),
            catch_block: Box::new(HirStmt::Expr(HirExpr::LoadVar("err".to_string()))),
        }]);
        let cfg = CfgBuilder::build(&func);
        assert!(validate_cfg(&cfg).is_ok());
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Throw));
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Defer));
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Catch));

        let catch_id = cfg
            .blocks
            .iter()
            .find(|b| b.kind == BlockKind::Catch)
            .unwrap()
            .id;
        let throw_block = cfg
            .blocks
            .iter()
            .find(|b| b.kind == BlockKind::Throw)
            .unwrap();
        assert!(throw_block.successors.contains(&catch_id));
    }

    #[test]
    fn test_match_and_conditional_and_short_circuit_split_cfg() {
        let func = make_test_function(vec![
            HirStmt::Expr(HirExpr::Conditional(
                Box::new(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    BinOp::And,
                    Box::new(HirExpr::LoadVar("y".to_string())),
                )),
                Box::new(HirExpr::Literal(HirLiteral::Int(1))),
                Box::new(HirExpr::Literal(HirLiteral::Int(2))),
            )),
            HirStmt::Expr(HirExpr::Match(
                Box::new(HirExpr::LoadVar("x".to_string())),
                vec![
                    (
                        HirPattern::Literal(HirLiteral::Int(0)),
                        HirExpr::Literal(HirLiteral::Int(10)),
                    ),
                    (
                        HirPattern::Variable("other".to_string()),
                        HirExpr::LoadVar("other".to_string()),
                    ),
                ],
            )),
        ]);
        let cfg = CfgBuilder::build(&func);
        assert!(validate_cfg(&cfg).is_ok());
        assert!(cfg.variables.contains_key("other"));
        let cond_blocks = cfg
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Conditional)
            .count();
        assert!(cond_blocks >= 3);
    }

    #[test]
    fn test_unsafe_block_recurses_and_marks_blocks() {
        let func = make_test_function(vec![HirStmt::Unsafe(Box::new(HirStmt::If {
            cond: HirExpr::LoadVar("x".to_string()),
            then_branch: Box::new(HirStmt::Expr(HirExpr::Free(Box::new(HirExpr::LoadVar(
                "x".to_string(),
            ))))),
            else_branch: None,
        }))]);
        let cfg = CfgBuilder::build(&func);
        assert!(validate_cfg(&cfg).is_ok());
        assert!(cfg.blocks.iter().any(|b| b.in_unsafe));
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Conditional && b.in_unsafe));
    }

    #[test]
    fn test_validate_cfg_rejects_malformed_graphs() {
        // 1. Empty CFG
        let empty = ControlFlowGraph::new_empty("empty".to_string());
        assert!(validate_cfg(&empty).is_err());

        // 2. Asymmetric edge
        let mut asym = ControlFlowGraph::new_empty("asym".to_string());
        let mut entry = BasicBlock::new_with_kind(0, BlockKind::Entry);
        let exit = BasicBlock::new_with_kind(1, BlockKind::Exit);
        entry.successors.push(1); // missing exit.predecessors.push(0)
        asym.blocks.push(entry);
        asym.blocks.push(exit);
        asym.entry = 0;
        asym.exits = vec![1];
        let errs = validate_cfg(&asym).unwrap_err();
        assert!(errs.iter().any(|e| e.edge == Some((0, 1))));

        // 3. Return block falling through to Normal block
        let mut bad_ret = ControlFlowGraph::new_empty("bad_ret".to_string());
        let mut b0 = BasicBlock::new_with_kind(0, BlockKind::Entry);
        let mut b1 = BasicBlock::new_with_kind(1, BlockKind::Return);
        let mut b2 = BasicBlock::new_with_kind(2, BlockKind::Normal);
        let mut b3 = BasicBlock::new_with_kind(3, BlockKind::Exit);
        b0.successors.push(1);
        b1.predecessors.push(0);
        b1.successors.push(2); // invalid fallthrough from Return!
        b2.predecessors.push(1);
        b2.successors.push(3);
        b3.predecessors.push(2);
        bad_ret.blocks = vec![b0, b1, b2, b3];
        bad_ret.entry = 0;
        bad_ret.exits = vec![3];
        assert!(validate_cfg(&bad_ret).is_err());
    }

    #[test]
    fn test_dot_and_summary_include_all_blocks_and_unreachable() {
        let func = make_test_function(vec![
            HirStmt::Return(Some(HirExpr::Literal(HirLiteral::Int(1)))),
            HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(2))),
        ]);
        let cfg = CfgBuilder::build(&func);
        let dot = cfg.to_dot();
        let summary = cfg.debug_summary();
        for b in &cfg.blocks {
            assert!(dot.contains(&format!("b{} [", b.id)));
            assert!(summary.contains(&format!("block {} ", b.id)));
        }
        assert!(dot.contains("UNREACHABLE"));
        assert!(summary.contains("reachable=false"));
    }
}
