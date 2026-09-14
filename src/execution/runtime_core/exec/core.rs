//! Core struct definition and scope management utilities for the Exec runtime.
//!
//! This module contains the main `Exec` struct and fundamental operations for
//! scope lifecycle management including acquisition, release, and defer handling.

use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

use crate::execution::runtime_core::{Env, flow::ExecFlow};
use crate::parsing::ast::{NativeEffect, Stmt, UserFn};

/// The main execution context for the language runtime.
///
/// Manages environment scopes, native side effects, and class context for visibility checking.
pub struct Exec {
    pub(in crate::execution::runtime_core) envs: Vec<Env>,
    pub(in crate::execution::runtime_core) current: usize,
    pub(in crate::execution::runtime_core) free_envs: Vec<usize>, // Free list for scope recycling (memory safety)
    pub(in crate::execution::runtime_core) native_side_effects:
        Option<Arc<Mutex<Vec<NativeEffect>>>>,
    // Track current class context for visibility checking
    pub(in crate::execution::runtime_core) current_class_context: Option<String>,
    // Optional method cache for extended methods on enums/structs
    pub(in crate::execution::runtime_core) method_cache:
        Option<FxHashMap<(std::sync::Arc<str>, std::sync::Arc<str>), UserFn>>,
    // Memoization cache for pure recursive numeric function evaluations
    pub(in crate::execution::runtime_core) recursion_memo:
        FxHashMap<(String, i64), crate::parsing::ast::Value>,
    // Purity status cache for user functions
    pub(in crate::execution::runtime_core) pure_fn_cache: FxHashMap<String, bool>,
}

impl Exec {
    /// Acquire a scope - either recycle from free list or create new (Rust-like memory safety)
    #[inline]
    pub(in crate::execution::runtime_core) fn acquire_scope(
        &mut self,
        enclosing: Option<usize>,
    ) -> usize {
        if let Some(idx) = self.free_envs.pop() {
            if idx < self.envs.len() {
                self.envs[idx].reset(enclosing);
                return idx;
            }
        }
        self.envs.push(Env::new(enclosing));
        self.envs.len() - 1
    }

    /// Release a scope back to free list (like Rust's drop - deterministic cleanup)
    #[inline]
    pub(in crate::execution::runtime_core) fn release_scope(&mut self, idx: usize) {
        if idx > 0 && idx < self.envs.len() {
            // Clear immediately to release owned values (deterministic drop)
            self.envs[idx].reset(None);
            self.free_envs.push(idx);
        }
    }

    /// Execute a block of statements with proper flow control and defer handling.
    pub(in crate::execution::runtime_core) fn exec_block(
        &mut self,
        stmts: &Vec<Stmt>,
    ) -> Result<ExecFlow, String> {
        for s in stmts {
            match self.exec_stmt(s)? {
                ExecFlow::Next => {}
                ExecFlow::Return(v) => {
                    // Execute defers before returning
                    self.exec_defers()?;
                    return Ok(ExecFlow::Return(v));
                }
                ExecFlow::Break => {
                    // Execute defers before breaking
                    self.exec_defers()?;
                    return Ok(ExecFlow::Break);
                }
                ExecFlow::Continue => {
                    // Execute defers before continuing
                    self.exec_defers()?;
                    return Ok(ExecFlow::Continue);
                }
                ExecFlow::Jump(n) => {
                    // Execute defers before jumping
                    self.exec_defers()?;
                    return Ok(ExecFlow::Jump(n));
                }
            }
        }
        // Normal scope exit - execute defers
        self.exec_defers()?;
        Ok(ExecFlow::Next)
    }

    /// Execute all deferred blocks in LIFO order for the current scope
    /// Errors in defer blocks are collected but don't prevent other defers from executing
    /// This follows Go's defer behavior where panics in defers are reported but don't stop cleanup
    ///
    /// Note: Currently does not propagate errors to maintain backward compatibility.
    /// Future enhancement: add configuration option to fail-fast or collect all errors.
    pub(in crate::execution::runtime_core) fn exec_defers(&mut self) -> Result<(), String> {
        let mut defer_errors = Vec::new();

        // Pop defers in reverse order (LIFO)
        while let Some(defer_block) = self.envs[self.current].defers.pop() {
            // Execute the defer block, catching any errors
            // Continue executing remaining defers even if one fails (panic-safe behavior)
            if let Err(e) = self.exec_stmt(&defer_block) {
                // Collect error with consistent formatting
                defer_errors.push(format!("Defer error: {}", e));
            }
        }

        // Report any collected errors (in debug builds only for visibility)
        #[cfg(debug_assertions)]
        for error in &defer_errors {
            eprintln!("{}", error);
        }

        // Currently always succeeds to maintain compatibility with all exit paths
        // Defers should not prevent function return/break/continue
        // This matches Go's behavior where defer panics are logged but don't fail the function
        Ok(())
    }
}
