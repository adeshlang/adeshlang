//! Interpreter submodules - modular structure for maintainability.
//!
//! This module organizes the interpreter implementation into focused,
//! single-responsibility modules.

pub mod arc_bridge;
pub mod class_access;
pub mod construction_helpers;
pub mod context;
pub mod env;
pub mod error_types;
pub mod eval;
pub mod globals;
pub mod helpers;
pub mod io_helpers;
pub mod memory_stats;
pub mod module_loader;
pub mod promises;
pub mod property_access;
pub mod state;
pub mod type_checking;
pub mod value_utils;
pub mod visitors;

// Re-export commonly used types for public API
pub use class_access::select_best_overload;
pub use context::{AsyncContext, ExecutionContext, MemoryContext, TimerEntry};
pub use error_types::{IndiaError, RunErr};
pub use globals::{
    GENERIC_TYPE_CONTEXT, get_program_args, get_program_name, runtime_env_all, runtime_env_clear,
    runtime_env_get, runtime_env_has, runtime_env_load, runtime_env_remove, runtime_env_set,
    set_program_args,
};
pub use memory_stats::{ArrayMemoryInfo, ProgramMemoryStats, VariableMemoryInfo};
pub use module_loader::ModuleLoader;
pub use state::{CallStack, ClosureManager, ScopeStack, VariableManager};
pub use value_utils::{json_to_value, public_equals};
pub use visitors::{ExpressionVisitor, StatementVisitor};

// Re-export for internal use within runtime_core
pub(super) use arc_bridge::{UNSAFE_DEPTH, arc_manager, in_unsafe_context};
pub(super) use class_access::{
    CONSTRUCTOR_SLOT, find_method_in_class_chain, find_method_with_visibility, is_field_accessible,
    is_method_accessible,
};
pub(super) use construction_helpers::{create_user_class, create_user_fn, err, err_with_span};
pub(super) use env::Env;
pub use globals::INPUT_PLAYBACK;
pub(super) use globals::{INPUT_RECORD, PROGRAM_ARGS, PROMISE_COUNTER, RUNTIME_ENV};
pub(super) use helpers::{format_input_type_error, wrap_foreign_function};
#[allow(unused_imports)]
pub(super) use io_helpers::{ct_disable_raw, ct_enable_raw, ct_poll, ct_read};
pub(super) use promises::{PromiseEntry, PromiseState, ThenHandler};
pub(super) use property_access::{get_prop, new_instance, set_prop};
pub(super) use type_checking::{
    ann_matches_value, coerce_and_apply_defaults, coerce_to_fixed_width, element_matches_type,
    infer_numeric_type,
};
pub(super) use value_utils::{is_copy_value, sizeof_value, value_type_name};
