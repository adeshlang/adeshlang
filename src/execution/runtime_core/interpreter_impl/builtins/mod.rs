//! Builtin Methods Module
//!
//! This module organizes all builtin method implementations for the language's core types.
//! Provides clean separation between different method categories while maintaining
//! compatibility with the interpreter.
//!
//! ## Modules
//!
//! - `strings`: String manipulation methods (split, trim, substring, etc.)
//! - `dates`: Date/time methods (getTime, toISOString, etc.)
//! - `sets`: Set operations (union, intersection, add, delete, etc.)
//! - `numbers`: Math namespace methods (sin, cos, sqrt, random, etc.)
//! - `arrays`: Simple array methods (push, pop, slice, join, indexOf, etc.)
//! - `objects`: Object utility methods (placeholder for future extraction)
//! - `testing`: Test assertion functions (assert, assert_eq, assert_ne, etc.)
//!
//! ## Note
//!
//! Array methods with higher-order functions (map, filter, reduce, find, findIndex,
//! forEach, some, every) remain in interpreter_core.rs as they require access to
//! the full interpreter context for function execution.

pub mod arrays;
pub mod associated;
pub mod dates;
pub mod numbers;
pub mod objects;
pub mod sets;
pub mod strings;
pub mod testing;

pub use arrays::call_simple_array_method;
pub use dates::call_date_method;
pub use numbers::{call_math_method, get_math_constant};
pub use objects::call_object_method;
pub use sets::call_set_method;
pub use strings::call_string_method;
pub use testing::{builtin_assert, builtin_assert_eq, builtin_assert_ne};
