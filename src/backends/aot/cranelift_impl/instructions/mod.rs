//! Instruction handler modules
//!
//! This module coordinates instruction handling and re-exports handler functions.

pub(crate) mod calls;

pub(crate) use calls::builtins::handle_call_builtin;
pub(crate) use calls::{handle_call, handle_call_builtin_generic, handle_tail_call};
