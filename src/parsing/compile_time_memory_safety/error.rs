//! Error types for compile-time memory safety analysis
//!
//! This module defines all error types that can be detected during compile-time
//! memory safety checks, including ownership violations, borrow conflicts,
//! lifetime issues, data races, and memory leaks.
//!
//! Formatting uses clickable `file:line:col` locations and language-accurate
//! help text (AdeshLang uses ARC — prefer `share` / `strong` / `weak` / restructure).

use crate::parsing::error::{
    ErrorKind, LangError, RelatedLocation, format_caret, format_clickable_location, ownership_help,
};

/// Compile-time memory safety error variants
#[derive(Debug, Clone)]
pub enum CompileTimeMemoryError {
    /// Use after move
    UseAfterMove {
        variable: String,
        moved_at: SourceLocation,
        used_at: SourceLocation,
        suggestion: String,
    },

    /// Use after free
    UseAfterFree {
        variable: String,
        freed_at: SourceLocation,
        used_at: SourceLocation,
    },

    /// Double free
    DoubleFree {
        variable: String,
        first_free: SourceLocation,
        second_free: SourceLocation,
    },

    /// Borrow conflict: mutable borrow while already borrowed
    BorrowConflict {
        variable: String,
        existing_borrow: BorrowLocation,
        conflicting_borrow: BorrowLocation,
        suggestion: String,
    },

    /// Free while borrowed
    FreeWhileBorrowed {
        variable: String,
        borrowed_at: Vec<SourceLocation>,
        freed_at: SourceLocation,
    },

    /// Return of borrowed reference that outlives owner
    LifetimeViolation {
        reference: String,
        owner: String,
        return_location: SourceLocation,
        owner_scope_ends: SourceLocation,
    },

    /// Data race: concurrent mutable access
    PotentialDataRace {
        variable: String,
        locations: Vec<SourceLocation>,
        thread_context: String,
    },

    /// Memory leak: potential cycle without weak references
    PotentialMemoryLeak {
        cycle: Vec<String>,
        location: SourceLocation,
        suggestion: String,
    },

    /// Uninitialized variable use
    UninitializedVariable {
        variable: String,
        used_at: SourceLocation,
    },

    /// Moved value in loop without reset
    MovedInLoop {
        variable: String,
        moved_at: SourceLocation,
        loop_location: SourceLocation,
    },

    /// Invalid pointer arithmetic
    InvalidPointerArithmetic {
        operation: String,
        location: SourceLocation,
        reason: String,
    },

    /// Missing drop implementation
    MissingDrop {
        type_name: String,
        location: SourceLocation,
    },
}

/// Source code location for error reporting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub context: String, // Line of code or descriptive context for display
}

impl SourceLocation {
    pub fn new(
        file: impl Into<String>,
        line: usize,
        column: usize,
        context: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            context: context.into(),
        }
    }

    /// Clickable `file:line:col`
    pub fn clickable(&self) -> String {
        let file = if self.file.is_empty() {
            None
        } else {
            Some(self.file.as_str())
        };
        format_clickable_location(file, self.line, self.column)
    }

    fn file_opt(&self) -> Option<String> {
        if self.file.is_empty() {
            None
        } else {
            Some(self.file.clone())
        }
    }
}

/// Borrow location tracking
#[derive(Debug, Clone)]
pub struct BorrowLocation {
    pub location: SourceLocation,
    pub is_mutable: bool,
    pub borrow_count: usize,
}

impl CompileTimeMemoryError {
    /// Convert to structured `LangError` for unified presentation
    pub fn to_lang_error(&self) -> LangError {
        match self {
            CompileTimeMemoryError::UseAfterMove {
                variable,
                moved_at,
                used_at,
                suggestion,
            } => LangError::located(
                ErrorKind::Ownership,
                format!("use of moved value `{}`", variable),
                used_at.file_opt(),
                used_at.line,
                used_at.column.max(1),
                used_at.context.clone(),
            )
            .with_code("E0382")
            .with_related(RelatedLocation::new(
                moved_at.file_opt(),
                moved_at.line,
                moved_at.column.max(1),
                moved_at.context.clone(),
                format!("value `{}` moved here", variable),
            ))
            .with_help(sanitize_suggestion(suggestion, variable))
            .push_note(
                "ownership is exclusive: after a move, the original name is no longer valid",
            ),

            CompileTimeMemoryError::UseAfterFree {
                variable,
                freed_at,
                used_at,
            } => LangError::located(
                ErrorKind::Ownership,
                format!("use of freed value `{}`", variable),
                used_at.file_opt(),
                used_at.line,
                used_at.column.max(1),
                used_at.context.clone(),
            )
            .with_code("E0416")
            .with_related(RelatedLocation::new(
                freed_at.file_opt(),
                freed_at.line,
                freed_at.column.max(1),
                freed_at.context.clone(),
                format!("`{}` was freed here", variable),
            ))
            .with_help(format!(
                "do not use `{}` after it has been freed; keep it alive until the last use",
                variable
            )),

            CompileTimeMemoryError::DoubleFree {
                variable,
                first_free,
                second_free,
            } => LangError::located(
                ErrorKind::Ownership,
                format!("double free of `{}`", variable),
                second_free.file_opt(),
                second_free.line,
                second_free.column.max(1),
                second_free.context.clone(),
            )
            .with_code("E0384")
            .with_related(RelatedLocation::new(
                first_free.file_opt(),
                first_free.line,
                first_free.column.max(1),
                first_free.context.clone(),
                format!("first free of `{}` here", variable),
            ))
            .with_help("each value may only be freed once; remove the extra free"),

            CompileTimeMemoryError::BorrowConflict {
                variable,
                existing_borrow,
                conflicting_borrow,
                suggestion,
            } => {
                let want_mut = conflicting_borrow.is_mutable;
                let msg = if want_mut {
                    format!(
                        "cannot borrow `{}` as exclusive (mutable) because it is already borrowed",
                        variable
                    )
                } else {
                    format!(
                        "cannot borrow `{}` because it is already exclusively borrowed",
                        variable
                    )
                };
                LangError::located(
                    ErrorKind::Ownership,
                    msg,
                    conflicting_borrow.location.file_opt(),
                    conflicting_borrow.location.line,
                    conflicting_borrow.location.column.max(1),
                    conflicting_borrow.location.context.clone(),
                )
                .with_code("E0502")
                .with_related(RelatedLocation::new(
                    existing_borrow.location.file_opt(),
                    existing_borrow.location.line,
                    existing_borrow.location.column.max(1),
                    existing_borrow.location.context.clone(),
                    if existing_borrow.is_mutable {
                        format!("exclusive borrow of `{}` occurs here", variable)
                    } else {
                        format!("shared borrow of `{}` occurs here", variable)
                    },
                ))
                .with_help(sanitize_suggestion(suggestion, variable))
            }

            CompileTimeMemoryError::FreeWhileBorrowed {
                variable,
                borrowed_at,
                freed_at,
            } => {
                let mut err = LangError::located(
                    ErrorKind::Ownership,
                    format!("cannot free `{}` because it is borrowed", variable),
                    freed_at.file_opt(),
                    freed_at.line,
                    freed_at.column.max(1),
                    freed_at.context.clone(),
                )
                .with_code("E0505")
                .with_help(ownership_help::free_while_borrowed(variable));
                for b in borrowed_at {
                    err = err.with_related(RelatedLocation::new(
                        b.file_opt(),
                        b.line,
                        b.column.max(1),
                        b.context.clone(),
                        format!("borrow of `{}` still active here", variable),
                    ));
                }
                err
            }

            CompileTimeMemoryError::LifetimeViolation {
                reference,
                owner,
                return_location,
                owner_scope_ends,
            } => LangError::located(
                ErrorKind::Ownership,
                format!(
                    "lifetime mismatch: reference `{}` may outlive owner `{}`",
                    reference, owner
                ),
                return_location.file_opt(),
                return_location.line,
                return_location.column.max(1),
                return_location.context.clone(),
            )
            .with_code("E0106")
            .with_related(RelatedLocation::new(
                owner_scope_ends.file_opt(),
                owner_scope_ends.line,
                owner_scope_ends.column.max(1),
                owner_scope_ends.context.clone(),
                format!("owner `{}` scope ends here", owner),
            ))
            .with_help(
                "return an owned value, extend the owner's lifetime, or use `share`/`strong` \
                 for shared ownership instead of a short-lived borrow",
            ),

            CompileTimeMemoryError::PotentialDataRace {
                variable,
                locations,
                thread_context,
            } => {
                let primary = locations.first();
                let mut err = LangError::located(
                    ErrorKind::Ownership,
                    format!("potential data race on `{}`", variable),
                    primary.and_then(|l| l.file_opt()),
                    primary.map(|l| l.line).unwrap_or(1),
                    primary.map(|l| l.column.max(1)).unwrap_or(1),
                    primary
                        .map(|l| l.context.clone())
                        .unwrap_or_else(|| thread_context.clone()),
                )
                .with_code("E0277")
                .with_help(
                    "ensure exclusive access across threads, or use synchronized/shared ownership \
                     (`share`/`strong`) designed for concurrent access",
                )
                .push_note(format!("thread context: {}", thread_context));
                for loc in locations.iter().skip(1) {
                    err = err.with_related(RelatedLocation::new(
                        loc.file_opt(),
                        loc.line,
                        loc.column.max(1),
                        loc.context.clone(),
                        "concurrent access also here",
                    ));
                }
                err
            }

            CompileTimeMemoryError::PotentialMemoryLeak {
                cycle,
                location,
                suggestion,
            } => LangError::located(
                ErrorKind::Ownership,
                format!(
                    "potential memory leak (reference cycle): {}",
                    cycle.join(" -> ")
                ),
                location.file_opt(),
                location.line,
                location.column.max(1),
                location.context.clone(),
            )
            .with_code("E0733")
            .with_help(sanitize_suggestion(suggestion, ""))
            .push_note(ownership_help::use_weak_for_cycle()),

            CompileTimeMemoryError::UninitializedVariable { variable, used_at } => {
                LangError::located(
                    ErrorKind::Ownership,
                    format!("use of possibly uninitialized variable `{}`", variable),
                    used_at.file_opt(),
                    used_at.line,
                    used_at.column.max(1),
                    used_at.context.clone(),
                )
                .with_code("E0381")
                .with_help(format!(
                    "assign a value to `{}` on every path before using it",
                    variable
                ))
            }

            CompileTimeMemoryError::MovedInLoop {
                variable,
                moved_at,
                loop_location,
            } => LangError::located(
                ErrorKind::Ownership,
                format!(
                    "value `{}` is moved inside a loop and reused on later iterations",
                    variable
                ),
                loop_location.file_opt(),
                loop_location.line,
                loop_location.column.max(1),
                loop_location.context.clone(),
            )
            .with_code("E0382")
            .with_related(RelatedLocation::new(
                moved_at.file_opt(),
                moved_at.line,
                moved_at.column.max(1),
                moved_at.context.clone(),
                format!("`{}` moved here in the loop body", variable),
            ))
            .with_help(format!(
                "Options: (1) re-initialize `{v}` each iteration; \
                 (2) avoid moving `{v}` inside the loop; \
                 (3) share ownership with `share {v} = <value>;` then `strong alias = {v};` \
                 so the loop body never consumes the last owner.",
                v = variable
            )),

            CompileTimeMemoryError::InvalidPointerArithmetic {
                operation,
                location,
                reason,
            } => LangError::located(
                ErrorKind::Ownership,
                format!("invalid pointer operation `{}`", operation),
                location.file_opt(),
                location.line,
                location.column.max(1),
                location.context.clone(),
            )
            .with_code("E0308")
            .with_help(reason.clone())
            .push_note("pointer arithmetic must stay within allocated bounds and valid provenance"),

            CompileTimeMemoryError::MissingDrop {
                type_name,
                location,
            } => LangError::located(
                ErrorKind::Ownership,
                format!(
                    "type `{}` requires an explicit Drop implementation",
                    type_name
                ),
                location.file_opt(),
                location.line,
                location.column.max(1),
                location.context.clone(),
            )
            .with_code("E0631")
            .with_help(format!(
                "implement cleanup for `{}` or allocate it in a `region` / RAII scope",
                type_name
            )),
        }
    }

    /// Format error with rich, clickable diagnostics
    pub fn format_error(&self) -> String {
        self.to_lang_error().to_string()
    }
}

/// Strip incorrect `.clone()` advice and replace with AdeshLang-accurate help.
fn sanitize_suggestion(suggestion: &str, var: &str) -> String {
    let lower = suggestion.to_lowercase();
    if lower.contains("clone") || lower.contains(".clone()") {
        if !var.is_empty() {
            return ownership_help::use_after_move(var);
        }
        return ownership_help::use_after_move("value");
    }
    // Also fix Rust-style &borrow suggestions that are not Adesh syntax
    if lower.contains("borrowing with `&") || lower.contains("borrow with &") {
        if !var.is_empty() {
            return ownership_help::avoid_move(var);
        }
    }
    suggestion.to_string()
}

/// Plain-text multi-location block (no colors) useful for tests/logs
#[allow(dead_code)]
pub fn format_location_block(primary_label: &str, loc: &SourceLocation) -> String {
    let mut out = format!("{}\n  --> {}\n", primary_label, loc.clickable());
    if !loc.context.is_empty() {
        out.push_str(&format!(
            "{:>4} | {}\n     | {}\n",
            loc.line.max(1),
            loc.context,
            format_caret(loc.column.max(1), 1)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_after_move_mentions_locations_not_clone() {
        let err = CompileTimeMemoryError::UseAfterMove {
            variable: "x".to_string(),
            moved_at: SourceLocation::new("app.adesh", 3, 9, "let y = x;".to_string()),
            used_at: SourceLocation::new("app.adesh", 4, 11, "print(x);".to_string()),
            suggestion: "consider cloning with x.clone()".to_string(),
        };
        let s = err.format_error();
        let re = regex::Regex::new("\\x1B\\[[0-9;]*m").unwrap();
        let clean = re.replace_all(&s, "");
        assert!(clean.contains("app.adesh:4:11") || clean.contains("app.adesh:4:"));
        assert!(clean.contains("moved"));
        assert!(clean.contains("share") || clean.contains("help:"));
    }
}
