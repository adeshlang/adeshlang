//! Control Flow Graph Construction
//!
//! Builds a CFG from HIR functions where:
//! - Nodes are basic blocks (sequences of non-branching statements)
//! - Edges represent control flow transitions
//!
//! Handles: if/else, while, for-in, break, continue, return, try/catch

use super::{BorrowStateMap, CfgBorrowState, SourceSpan};
use crate::parsing::hir::{HirExpr, HirFunction, HirStmt};
use std::collections::HashMap;

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
    /// Block ends with continue
    Continue,
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
        }
    }

    /// Check if this block terminates (no fall-through to successors)
    pub fn is_terminated(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Return | BlockKind::Break | BlockKind::Continue
        )
    }

    /// Set a descriptive label for this block
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
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
}

/// Builder for constructing CFGs from HIR functions
pub struct CfgBuilder {
    cfg: ControlFlowGraph,
    next_block_id: BlockId,

    /// Stack of loop contexts for break/continue resolution
    loop_stack: Vec<LoopContext>,

    /// Counter for generating unique borrow IDs
    #[allow(dead_code)]
    next_borrow_id: u64,
}

/// Context for resolving break/continue within loops
#[derive(Debug, Clone)]
struct LoopContext {
    /// Loop header block (target for continue)
    #[allow(dead_code)]
    header: BlockId,
    /// Blocks that break out of this loop (need to connect to exit)
    break_blocks: Vec<BlockId>,
    /// Blocks that continue this loop (need to connect to header)
    continue_blocks: Vec<BlockId>,
}

impl CfgBuilder {
    /// Build a CFG from an HIR function
    pub fn build(func: &HirFunction) -> ControlFlowGraph {
        let mut builder = Self {
            cfg: ControlFlowGraph::new(func.name.clone()),
            next_block_id: 0,
            loop_stack: Vec::new(),
            next_borrow_id: 0,
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
        for (param_name, _, _) in &func.params {
            builder
                .cfg
                .variables
                .insert(param_name.clone(), CfgBorrowState::Unborrowed);
        }

        // Build blocks from function body
        let mut current_block = entry_id;
        for stmt in func.body.iter() {
            current_block = builder.process_stmt(stmt, current_block, exit_id);
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
        let block = BasicBlock::new(id, kind);
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

    /// Process a statement and return the current block after processing
    fn process_stmt(
        &mut self,
        stmt: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        match stmt {
            HirStmt::Let { name, .. } => {
                // Register variable
                self.cfg
                    .variables
                    .insert(name.clone(), CfgBorrowState::Unborrowed);
                // Add statement to current block
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::LetTuple { names, .. } => {
                // Register all variables
                for name in names {
                    self.cfg
                        .variables
                        .insert(name.clone(), CfgBorrowState::Unborrowed);
                }
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::Assign { .. } | HirStmt::Expr(_) => {
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

            HirStmt::Return(_expr) => {
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                    block.kind = BlockKind::Return;
                }
                self.add_edge(current_block, exit_block);
                // Return a new block that's unreachable (for dead code after return)
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_return".to_string();
                }
                dead_block
            }

            HirStmt::Break => {
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.kind = BlockKind::Break;
                }
                // Record break for later resolution
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.break_blocks.push(current_block);
                }
                // Return a new block that's unreachable
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_break".to_string();
                }
                dead_block
            }

            HirStmt::Continue => {
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.kind = BlockKind::Continue;
                }
                // Record continue for later resolution
                if let Some(loop_ctx) = self.loop_stack.last_mut() {
                    loop_ctx.continue_blocks.push(current_block);
                }
                // Return a new block that's unreachable
                let dead_block = self.create_block(BlockKind::Normal);
                if let Some(block) = self.cfg.get_block_mut(dead_block) {
                    block.label = "unreachable_after_continue".to_string();
                }
                dead_block
            }

            HirStmt::Block(stmts) => {
                let mut block = current_block;
                for s in stmts {
                    block = self.process_stmt(s, block, exit_block);
                }
                block
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
                // Nested function definitions don't affect the current CFG
                // They would be analyzed separately
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            HirStmt::Region { name, body } => {
                // Region blocks are treated as regular blocks for borrow checking
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(HirStmt::Expr(HirExpr::Literal(
                        crate::parsing::hir::HirLiteral::String(format!("region:{}", name)),
                    )));
                }
                self.process_stmt(body, current_block, exit_block)
            }

            HirStmt::Unsafe(_body) => {
                // Unsafe blocks opt out of some borrow checking
                // But we still track the CFG structure
                if let Some(block) = self.cfg.get_block_mut(current_block) {
                    block.statements.push(stmt.clone());
                }
                current_block
            }

            _ => {
                // Other statement types
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
        // Add condition evaluation to current block
        if let Some(block) = self.cfg.get_block_mut(current_block) {
            block.statements.push(HirStmt::Expr(cond.clone()));
            block.kind = BlockKind::Conditional;
            block.label = format!("if_cond_{}", current_block);
        }

        // Create then block
        let then_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(then_block) {
            block.label = format!("then_{}", then_block);
        }
        self.add_edge(current_block, then_block);
        let then_exit = self.process_stmt(then_branch, then_block, exit_block);

        // Create else block (or empty path)
        let else_exit = if let Some(else_stmt) = else_branch {
            let else_block = self.create_block(BlockKind::Normal);
            if let Some(block) = self.cfg.get_block_mut(else_block) {
                block.label = format!("else_{}", else_block);
            }
            self.add_edge(current_block, else_block);
            self.process_stmt(else_stmt, else_block, exit_block)
        } else {
            // No else branch - create empty fall-through
            let else_block = self.create_block(BlockKind::Normal);
            if let Some(block) = self.cfg.get_block_mut(else_block) {
                block.label = format!("no_else_{}", else_block);
            }
            self.add_edge(current_block, else_block);
            else_block
        };

        // Create join block
        let join_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(join_block) {
            block.label = format!("if_join_{}", join_block);
        }

        // Connect non-terminated branches to join
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
        // Create loop header (condition check)
        let header_block = self.create_block(BlockKind::LoopHeader);
        if let Some(block) = self.cfg.get_block_mut(header_block) {
            block.statements.push(HirStmt::Expr(cond.clone()));
            block.label = format!("while_header_{}", header_block);
        }
        self.add_edge(current_block, header_block);

        // Create loop exit block
        let loop_exit = self.create_block(BlockKind::LoopExit);
        if let Some(block) = self.cfg.get_block_mut(loop_exit) {
            block.label = format!("while_exit_{}", loop_exit);
        }
        self.add_edge(header_block, loop_exit);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            header: header_block,
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
        });

        // Create and process loop body
        let body_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(body_block) {
            block.label = format!("while_body_{}", body_block);
        }
        self.add_edge(header_block, body_block);
        let body_exit = self.process_stmt(body, body_block, exit_block);

        // Back-edge to header
        if let Some(block) = self.cfg.get_block(body_exit) {
            if !block.is_terminated() {
                self.add_edge(body_exit, header_block);
            }
        }

        // Pop loop context and resolve break/continue
        if let Some(loop_ctx) = self.loop_stack.pop() {
            // Connect break blocks to loop exit
            for break_block in loop_ctx.break_blocks {
                self.add_edge(break_block, loop_exit);
            }
            // Connect continue blocks to header
            for continue_block in loop_ctx.continue_blocks {
                self.add_edge(continue_block, header_block);
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
        // Register loop variable
        self.cfg
            .variables
            .insert(var.to_string(), CfgBorrowState::Unborrowed);

        // Create loop header
        let header_block = self.create_block(BlockKind::LoopHeader);
        if let Some(block) = self.cfg.get_block_mut(header_block) {
            block.statements.push(HirStmt::Expr(iter.clone()));
            block.label = format!("for_header_{}", header_block);
        }
        self.add_edge(current_block, header_block);

        // Create loop exit
        let loop_exit = self.create_block(BlockKind::LoopExit);
        if let Some(block) = self.cfg.get_block_mut(loop_exit) {
            block.label = format!("for_exit_{}", loop_exit);
        }
        self.add_edge(header_block, loop_exit);

        // Push loop context
        self.loop_stack.push(LoopContext {
            header: header_block,
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
        });

        // Process body
        let body_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(body_block) {
            block.label = format!("for_body_{}", body_block);
        }
        self.add_edge(header_block, body_block);
        let body_exit = self.process_stmt(body, body_block, exit_block);

        // Back-edge to header
        if let Some(block) = self.cfg.get_block(body_exit) {
            if !block.is_terminated() {
                self.add_edge(body_exit, header_block);
            }
        }

        // Resolve break/continue
        if let Some(loop_ctx) = self.loop_stack.pop() {
            for break_block in loop_ctx.break_blocks {
                self.add_edge(break_block, loop_exit);
            }
            for continue_block in loop_ctx.continue_blocks {
                self.add_edge(continue_block, header_block);
            }
        }

        loop_exit
    }

    /// Process a try-catch block
    fn process_try_catch(
        &mut self,
        try_block: &HirStmt,
        _error_name: &str,
        catch_block: &HirStmt,
        current_block: BlockId,
        exit_block: BlockId,
    ) -> BlockId {
        // Create try block
        let try_block_id = self.create_block(BlockKind::Try);
        if let Some(block) = self.cfg.get_block_mut(try_block_id) {
            block.label = format!("try_{}", try_block_id);
        }
        self.add_edge(current_block, try_block_id);
        let try_exit = self.process_stmt(try_block, try_block_id, exit_block);

        // Create catch block
        let catch_block_id = self.create_block(BlockKind::Catch);
        if let Some(block) = self.cfg.get_block_mut(catch_block_id) {
            block.label = format!("catch_{}", catch_block_id);
        }
        // Catch can be reached from try (on exception)
        self.add_edge(try_block_id, catch_block_id);
        let catch_exit = self.process_stmt(catch_block, catch_block_id, exit_block);

        // Create join block
        let join_block = self.create_block(BlockKind::Normal);
        if let Some(block) = self.cfg.get_block_mut(join_block) {
            block.label = format!("try_catch_join_{}", join_block);
        }

        // Connect exits to join
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
    use crate::parsing::hir::{HirLiteral, HirType};
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
    }

    #[test]
    fn test_simple_let() {
        let func = make_test_function(vec![HirStmt::Let {
            name: "y".to_string(),
            ty: Some(HirType::Int),
            init: Some(HirExpr::Literal(HirLiteral::Int(42))),
            is_const: false,
            is_borrowed: None,
        }]);
        let cfg = CfgBuilder::build(&func);

        // Entry should have the let statement
        assert_eq!(cfg.blocks[0].statements.len(), 1);
        // Variable should be registered
        assert!(cfg.variables.contains_key("y"));
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

        // Should have: entry, cond, then, else, join, exit
        assert!(cfg.blocks.len() >= 5);

        // Entry should connect to conditional
        // Cond should connect to then and else
        // Then and else should connect to join
    }

    #[test]
    fn test_while_loop() {
        let func = make_test_function(vec![HirStmt::While {
            cond: HirExpr::LoadVar("x".to_string()),
            body: Box::new(HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(1)))),
        }]);
        let cfg = CfgBuilder::build(&func);

        // Should have header with back-edge
        let has_loop_header = cfg.blocks.iter().any(|b| b.kind == BlockKind::LoopHeader);
        assert!(has_loop_header);
    }

    #[test]
    fn test_return_terminates() {
        let func = make_test_function(vec![
            HirStmt::Return(Some(HirExpr::Literal(HirLiteral::Int(42)))),
            HirStmt::Expr(HirExpr::Literal(HirLiteral::Int(0))), // Dead code
        ]);
        let cfg = CfgBuilder::build(&func);

        // Check that return block is terminated
        let return_block = cfg.blocks.iter().find(|b| b.kind == BlockKind::Return);
        assert!(return_block.is_some());
        assert!(return_block.unwrap().is_terminated());
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

        // Should have break and continue blocks
        let has_break = cfg.blocks.iter().any(|b| b.kind == BlockKind::Break);
        let has_continue = cfg.blocks.iter().any(|b| b.kind == BlockKind::Continue);
        assert!(has_break);
        assert!(has_continue);
    }
}
