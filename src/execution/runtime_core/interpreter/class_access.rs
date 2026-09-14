//! Method and class access utilities for the interpreter.
//!
//! Provides utilities for:
//! - Visibility checking (public, private, protected)
//! - Method lookup in class hierarchies
//! - Overload resolution
//! - Property access with visibility checks

use crate::parsing::ast::{UserClass, UserFn, Value, Visibility};

pub const CONSTRUCTOR_SLOT: &str = "__ctor__";

/// Check if a method is accessible from the current context given its visibility.
///
/// Implements visibility rules:
/// - Public: accessible everywhere
/// - Private: only accessible within the defining class
/// - Protected: accessible in defining class and subclasses
pub(in crate::execution::runtime_core) fn is_method_accessible(
    method: &UserFn,
    defining_class: &str,
    instance_class: &UserClass,
    current_context: Option<&str>,
) -> bool {
    match &method.visibility {
        Some(Visibility::Pub) | None => true, // Public or unspecified (default public)
        Some(Visibility::Priv) => {
            // Private: only accessible within the defining class
            current_context == Some(defining_class)
        }
        Some(Visibility::Protected) => {
            // Protected: accessible in defining class and subclasses
            if let Some(ctx) = current_context {
                let mut curr = Some(instance_class);
                let mut found_ctx = false;
                let mut found_defining = false;
                while let Some(p) = curr {
                    if p.name == ctx {
                        found_ctx = true;
                    }
                    if p.name == defining_class {
                        found_defining = true;
                        if !found_ctx {
                            return false;
                        }
                    }
                    curr = p.parent.as_ref().map(|b| &**b);
                }
                found_ctx && found_defining
            } else {
                false
            }
        }
    }
}

/// Check if a field is accessible based on visibility and current context.
///
/// Implements visibility rules:
/// - Public: accessible everywhere
/// - Private: only accessible within the defining class
/// - Protected: accessible in defining class and subclasses
pub(in crate::execution::runtime_core) fn is_field_accessible(
    visibility: &Option<Visibility>,
    defining_class: &str,
    instance_class: &UserClass,
    current_context: Option<&str>,
) -> bool {
    match visibility {
        Some(Visibility::Pub) | None => true, // Public or unspecified (default public)
        Some(Visibility::Priv) => {
            // Private: only accessible within the defining class
            current_context == Some(defining_class)
        }
        Some(Visibility::Protected) => {
            // Protected: accessible in defining class and subclasses
            if let Some(ctx) = current_context {
                let mut curr = Some(instance_class);
                let mut found_ctx = false;
                let mut found_defining = false;
                while let Some(p) = curr {
                    if p.name == ctx {
                        found_ctx = true;
                    }
                    if p.name == defining_class {
                        found_defining = true;
                        if !found_ctx {
                            return false;
                        }
                    }
                    curr = p.parent.as_ref().map(|b| &**b);
                }
                found_ctx && found_defining
            } else {
                false
            }
        }
    }
}

/// Find method in class without visibility checking.
///
/// Returns the first method with the given name, if it exists.
pub(in crate::execution::runtime_core) fn find_method_in_class_chain(
    c: &UserClass,
    key: &str,
) -> Option<UserFn> {
    c.methods
        .get(key)
        .and_then(|methods| methods.first().cloned())
}

/// Find method in class chain with visibility checking.
///
/// Searches through the class hierarchy for a method with the given name,
/// checking visibility at each level. Returns an error if the method exists
/// but is not accessible.
pub(in crate::execution::runtime_core) fn find_method_with_visibility(
    c: &UserClass,
    key: &str,
    current_context: Option<&str>,
) -> Result<Option<UserFn>, String> {
    if let Some(methods) = c.methods.get(key) {
        if let Some(method) = methods.first() {
            let defining_class = method.defining_class.as_deref().unwrap_or(&c.name);
            if is_method_accessible(method, defining_class, c, current_context) {
                return Ok(Some(method.clone()));
            }

            let vis_str = match &method.visibility {
                Some(Visibility::Priv) => "private",
                Some(Visibility::Protected) => "protected",
                _ => "public",
            };
            return Err(format!(
                "Cannot access {} method '{}' of class '{}'",
                vis_str, key, defining_class
            ));
        }
    }

    Ok(None)
}

/// Select best overload based on argument types.
///
/// Returns the best matching function or an error if ambiguous or no match.
/// Uses type distance scoring to find the most specific overload.
pub fn select_best_overload(methods: &[UserFn], args: &[Value]) -> Result<UserFn, String> {
    // Filter methods that match arity
    let arity_matches: Vec<_> = methods
        .iter()
        .filter(|m| m.matches_signature(args.len()))
        .collect();

    if arity_matches.is_empty() {
        return Err(format!("No overload matches {} arguments", args.len()));
    }

    // If only one arity match, use it
    if arity_matches.len() == 1 {
        return Ok(arity_matches[0].clone());
    }

    // Calculate type distances for each overload
    let mut candidates: Vec<_> = arity_matches
        .iter()
        .filter_map(|m| {
            m.matches_signature_with_types(args)
                .map(|distance| (*m, distance))
        })
        .collect();

    if candidates.is_empty() {
        // No type-compatible overloads
        return Err(format!(
            "No overload matches argument types: {}",
            args.iter()
                .map(|v| UserFn::value_type_name(v))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Sort by distance (lower is better)
    candidates.sort_by_key(|(_, distance)| *distance);

    // Check for ambiguity (multiple candidates with same best distance)
    let best_distance = candidates[0].1;
    let best_matches: Vec<_> = candidates
        .iter()
        .filter(|(_, d)| *d == best_distance)
        .collect();

    if best_matches.len() > 1 {
        return Err(format!(
            "Ambiguous method call: {} overloads match equally well",
            best_matches.len()
        ));
    }

    Ok(candidates[0].0.clone())
}
