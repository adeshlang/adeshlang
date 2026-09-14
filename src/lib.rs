//! Language Compiler Crate
//!
//! This crate implements the full language toolchain and runtime:
//! - Parsing: `parsing` produces an `AST` and lowers it into `HIR`.
//! - IR: `backends::lir` defines a typed, SSA-style `LIR` used by JIT/AOT.
//! - Lowering: `backends::lir_lower` transforms `HIR` → `LIR` with control-flow.
//! - Execution: `execution::runtime` provides the interpreter; `execution::vm` runs bytecode.
//! - CLI: `cli` parses arguments and builds `RuntimeConfig` to drive execution.
//! - Stdlib: `stdlib` registers builtins and language standard library modules.
//! - Types: `types` contains the type model, checker utilities, and layout engine.
//! - Utils: helper utilities like formatting, interning, timers, and doc generation.
//!
//! Key design notes:
//! - Performance-first: SSA `LIR` enables JIT backends; interpreter favors clarity with profiling hooks.
//! - Portability: Backends include interpreter, bytecode VM, JIT, AOT (Cranelift), and WASM integration points.
//! - Safety model: Deterministic, GC-free execution with compile-time safety validation before running any backend.
//! - Developer UX: Rich CLI, IR dumping, formatter, and documentation generator.
// Allow clippy warnings for intentional design choices and style preferences
#![allow(clippy::result_large_err)] // LangError is intentionally rich with context
#![allow(clippy::collapsible_if)] // Nested ifs are often more readable
#![allow(clippy::collapsible_match)] // Match with if-let inside is often clearer
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match)] // Single match is often clearer than if-let
#![allow(clippy::too_many_arguments)] // Some functions legitimately need many args
#![allow(clippy::type_complexity)] // Complex types in VM internals
#![allow(clippy::ptr_arg)] // &Vec is fine in many cases
#![allow(clippy::new_without_default)] // Not all types need Default
#![allow(clippy::should_implement_trait)] // from_str methods are intentional
#![allow(clippy::inherent_to_string)] // We have custom to_string implementations
#![allow(clippy::arc_with_non_send_sync)] // Arc usage is intentional for shared state
#![allow(clippy::unnecessary_cast)] // Explicit casts improve readability
#![allow(clippy::len_zero)] // .len() == 0 vs .is_empty() is style choice
#![allow(clippy::redundant_closure)] // Explicit closures improve readability
#![allow(clippy::needless_as_bytes)] // as_bytes() calls are intentional
#![allow(clippy::manual_strip)] // Manual strip is often clearer
#![allow(clippy::useless_conversion)] // Explicit conversions improve clarity
#![allow(clippy::needless_borrow)] // Explicit borrows are fine
#![allow(clippy::needless_return)] // Return statements are fine
#![allow(clippy::get_first)] // .get(0) is fine
#![allow(clippy::large_enum_variant)] // Enum variants are sized appropriately
#![allow(clippy::explicit_deref_methods)] // Explicit deref is clearer
#![allow(clippy::clone_on_copy)] // .clone() on Copy types is fine for clarity
#![allow(clippy::manual_range_contains)] // Manual range checks are sometimes clearer
#![allow(clippy::needless_lifetimes)] // Explicit lifetimes are documentation
#![allow(clippy::map_entry)] // contains_key + insert pattern is fine
#![allow(clippy::derivable_impls)] // Manual impls provide flexibility
#![allow(clippy::or_fun_call)] // or_insert_with vs or_insert choice
#![allow(clippy::let_and_return)] // let binding before return improves debugging
#![allow(clippy::only_used_in_recursion)] // Recursive parameters are needed
#![allow(clippy::print_with_newline)] // print!("...\n") is fine
#![allow(clippy::redundant_pattern_matching)] // Explicit pattern matching is clearer
#![allow(clippy::write_with_newline)] // Style preference
#![allow(clippy::wrong_self_convention)] // from_xxx naming is intentional
#![allow(clippy::empty_line_after_doc_comments)] // Style preference
#![allow(clippy::needless_range_loop)] // Explicit index loops are often clearer
#![allow(clippy::vec_init_then_push)] // Init then push pattern is fine
#![allow(clippy::explicit_auto_deref)] // Explicit deref is documentation
#![allow(clippy::chars_next_cmp)] // Explicit char comparison is clearer
#![allow(clippy::to_string_in_format_args)] // Explicit to_string is fine
#![allow(clippy::needless_borrows_for_generic_args)] // Explicit borrows are fine
#![allow(clippy::match_like_matches_macro)] // Match is often clearer than matches!
#![allow(clippy::unwrap_or_default)] // or_insert_with is fine
#![allow(clippy::unnecessary_sort_by)] // Explicit sort_by is often clearer
#![allow(clippy::unnecessary_get_then_check)] // Explicit get check is clearer
#![allow(clippy::iter_overeager_cloned)] // Clone ordering is fine
#![allow(clippy::assign_op_pattern)]
#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::drop_non_drop)]
#![allow(clippy::duplicated_attributes)]
#![allow(clippy::duplicate_mod)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::for_kv_map)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::io_other_error)]
#![allow(clippy::legacy_numeric_constants)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::manual_find)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_map)]
#![allow(clippy::manual_ok_err)]
#![allow(clippy::manual_range_patterns)]
#![allow(clippy::manual_slice_fill)]
#![allow(clippy::manual_unwrap_or_default)]
#![allow(clippy::match_result_ok)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::mem_replace_option_with_none)]
#![allow(clippy::mem_replace_option_with_some)]
#![allow(clippy::mem_replace_with_default)]
#![allow(clippy::missing_const_for_thread_local)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_inception)]
#![allow(clippy::needless_bool)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::option_as_ref_deref)]
#![allow(clippy::print_literal)]
#![allow(clippy::question_mark)]
#![allow(clippy::redundant_guards)]
#![allow(clippy::redundant_locals)]
#![allow(clippy::single_char_add_str)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::unnecessary_to_owned)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::unneeded_wildcard_pattern)]
#![allow(clippy::unused_enumerate_index)]
#![allow(clippy::useless_format)]
#![allow(clippy::vec_box)]
#![allow(clippy::while_let_loop)]
#![allow(clippy::wildcard_in_or_patterns)]
#![allow(clippy::zombie_processes)]
#![allow(clippy::approx_constant)]

pub mod backends;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
// pub mod core; // removed: no core module present
pub mod ecosystem;
pub mod execution;
pub mod frontend;
pub mod ir;
pub mod memory;
pub mod parsing;
pub mod runtime;
pub mod semantics;
pub mod stdlib;
pub mod testing;
pub mod toolchain;
pub mod types;
pub mod typesystem;
pub mod update;
pub mod utils;

// Re-export layered stdlib for easy access
pub use stdlib::{adesh_alloc, adesh_core, adesh_std};

pub use execution::runtime::{Interpreter, ModuleLoader, ProgramMemoryStats, VariableMemoryInfo};
pub use toolchain::{ExecutionBackend, OptLevel, ParsedArgs, RuntimeConfig};

// Use mimalloc as the global allocator to reduce allocation overhead
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Re-export AOT runtime functions for static library linking
// These are needed by AOT-compiled code
pub use execution::runtime_core::arc_bridge::{
    adesh_rt_arc_clone, adesh_rt_arc_drop, adesh_rt_arc_get, adesh_rt_arc_new, adesh_rt_arc_set,
    adesh_rt_arc_strong_count, adesh_rt_arc_weak_count, adesh_rt_assert_heap_allowed,
    adesh_rt_weak_drop, adesh_rt_weak_new,
};

#[cfg(not(target_arch = "wasm32"))]
pub use backends::aot::runtime_bridge::{
    aot_free_handle, aot_get_value, aot_print_with_options, aot_store_value,
};
