//! Helper functions for constructing runtime objects.
//!
//! Provides utilities for creating UserFn and UserClass instances with
//! all required fields properly initialized.

use crate::parsing::ast::{Expr, Stmt, UserClass, UserFn, Value, Visibility};
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

/// Create a runtime error message.
///
/// Simple string conversion for error handling.
pub(in crate::execution::runtime_core) fn err<T: Into<String>>(m: T) -> String {
    m.into()
}

/// Create a runtime error with location info from span.
///
/// Returns the formatted error string with location details embedded.
pub(in crate::execution::runtime_core) fn err_with_span<T: Into<String>>(
    message: T,
    span: &crate::parsing::ast::Span,
) -> String {
    use crate::parsing::error::{ErrorKind, LangError};
    // If span has line 0, the span info was not set properly - just return plain error
    // Otherwise create a proper LangError with location
    if span.line == 0 {
        message.into()
    } else {
        let error = LangError::new(
            ErrorKind::Runtime,
            message.into(),
            span.line,
            span.col,
            span.line_text.clone(),
        );
        // Return the formatted error which includes line/col info in the message itself
        format!("{}", error)
    }
}

/// Create a runtime error with location, file, and optional help message.
#[allow(dead_code)]
pub(in crate::execution::runtime_core) fn err_with_span_file_help<
    T: Into<String>,
    H: Into<String>,
>(
    message: T,
    span: &crate::parsing::ast::Span,
    file: Option<String>,
    help: Option<H>,
) -> String {
    use crate::parsing::error::{ErrorKind, LangError};
    if span.line == 0 {
        message.into()
    } else {
        let mut error = LangError::new(
            ErrorKind::Runtime,
            message.into(),
            span.line,
            span.col,
            span.line_text.clone(),
        );
        if let Some(f) = file {
            error = error.with_file(f);
        }
        if let Some(h) = help {
            error = error.with_help(h.into());
        }
        format!("{}", error)
    }
}

/// Create a styled, registry-linked type-check error (code `E0308`).
///
/// Renders through `LangError` so the message comes out as a full adesh error
/// table: a colored `error[E0308]` header, the clickable `--> file:line:col`
/// location, the source line with a caret underline, a `help:` note, and the
/// `adesh --explain E0308` pointer into the diagnostic registry.
pub(in crate::execution::runtime_core) fn err_styled_type<M: Into<String>, H: Into<String>>(
    message: M,
    span: &crate::parsing::ast::Span,
    file: &str,
    help: H,
) -> String {
    use crate::parsing::error::{ErrorKind, LangError};
    if span.line == 0 {
        // No usable location: still emit the code-linked header.
        return format!("error[E0308]: {}", message.into());
    }
    let error = LangError::new(
        ErrorKind::Type,
        message.into(),
        span.line,
        span.col,
        span.line_text.clone(),
    )
    .with_file(file.to_string())
    .with_help(help.into())
    .with_code("E0308");
    format!("{}", error)
}

/// Create a UserFn with all required fields.
///
/// Provides a convenient way to construct user functions with default values
/// for optional fields.
pub(in crate::execution::runtime_core) fn create_user_fn(
    name: String,
    type_params: Vec<String>,
    params: Vec<(String, Option<Expr>, Option<String>)>,
    body: Arc<Vec<Stmt>>,
    closure: usize,
    captured: Option<HashMap<String, Value>>,
    visibility: Option<Visibility>,
    ret_type: Option<String>,
    is_async: bool,
    is_unsafe: bool,
) -> UserFn {
    UserFn {
        name,
        type_params,
        params,
        body,
        closure,
        captured,
        visibility,
        ret_type,
        is_async,
        is_static: false,
        is_abstract: false,
        is_constructor: false,
        is_getter: false,
        is_setter: false,
        is_operator: false,
        operator_symbol: None,
        defining_class: None,
        is_unsafe,
    }
}

/// Create a UserClass with all required fields.
///
/// Provides a convenient way to construct user classes with default values
/// for optional fields.
#[allow(dead_code)]
pub(in crate::execution::runtime_core) fn create_user_class(
    name: String,
    methods: HashMap<String, Vec<UserFn>>,
    parent: Option<Box<UserClass>>,
    implements: Vec<String>,
    is_abstract: bool,
) -> UserClass {
    UserClass {
        name,
        methods,
        static_methods: HashMap::default(),
        static_properties: HashMap::default(),
        operators: HashMap::default(),
        getters: HashMap::default(),
        setters: HashMap::default(),
        parent,
        implements,
        is_abstract,
        is_sealed: false,
        no_extend: false,
        field_visibility: HashMap::default(),
        field_owner: HashMap::default(),
        field_types: HashMap::default(),
        field_initializers: HashMap::default(),
    }
}
