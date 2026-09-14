//! Visitor Pattern Infrastructure (Phase 3: PR2)
//!
//! This module provides visitor pattern infrastructure for traversing and
//! evaluating language AST nodes. The visitor pattern offers several benefits:
//!
//! - **Reduced Recursion**: Enables iterative evaluation strategies to prevent stack overflow
//! - **Separation of Concerns**: Separates traversal logic from evaluation logic
//! - **Extensibility**: Easy to add new visitors for different purposes (evaluation, type checking, optimization)
//! - **Testability**: Each visitor can be tested independently
//!
//! ## Architecture
//!
//! The visitor pattern implementation consists of:
//!
//! - `ExpressionVisitor`: Trait for visiting expression AST nodes
//! - `StatementVisitor`: Trait for visiting statement AST nodes
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::visitors::ExpressionVisitor;
//!
//! struct MyEvaluator {
//!     // ... state ...
//! }
//!
//! impl ExpressionVisitor for MyEvaluator {
//!     type Output = Result<Value, String>;
//!
//!     fn visit_literal(&mut self, value: &Value) -> Self::Output {
//!         Ok(value.clone())
//!     }
//!
//!     // ... implement other visit methods ...
//! }
//!
//! // Use the visitor
//! let mut evaluator = MyEvaluator::new();
//! let result = evaluator.visit_expr(&some_expression)?;
//! ```
//!
//! ## Future Enhancements (PR3+)
//!
//! - Iterative binary chain evaluation to eliminate deep recursion
//! - Iterative call chain evaluation  
//! - Type checking visitor implementation
//! - Optimization pass visitors
//! - Code generation visitors

pub mod expression;
pub mod statement;

pub use expression::ExpressionVisitor;
pub use statement::StatementVisitor;
