//! Cranelift AOT compiler implementation modules
//!
//! This directory contains modularized implementation components for the
//! Cranelift AOT compiler, organized by functionality.

pub mod arithmetic;
pub mod comparisons;
pub mod constants;
pub mod context;
pub mod control_flow;
pub mod conversions;
pub mod execution;
pub mod header_gen;
pub mod helpers;
pub mod instructions;
pub mod linking;
pub mod memory;
pub mod runtime_decl;
pub mod types;
pub mod utilities;
pub mod variables;
