//! Phase 10 — First-Class Metadata & Attribute System.
//!
//! Provides:
//! - Validation, parsing, and propagation of compiler attributes.
//! - Supported standard attributes: `#[inline]`, `#[cold]`, `#[no_mangle]`,
//!   `#[export]`, `#[ffi("C")]`, `#[target_feature("avx2")]`.
//! - Diagnostics for unrecognized or misplaced attributes.

use serde::{Deserialize, Serialize};

/// Inline strategy hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineHint {
    Never,
    Always,
    Default,
}

/// Standardized compiler attribute representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeKind {
    Inline(InlineHint),
    Cold,
    NoMangle,
    Export,
    Ffi(String),
    TargetFeature(String),
    Custom(String, Option<String>),
}

/// Where an attribute may be legally placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeTarget {
    Function,
    Struct,
    Field,
    Module,
}

/// Attribute validator ensuring attributes are applied to valid language constructs.
pub struct AttributeValidator;

impl AttributeValidator {
    pub fn validate(
        attr: &AttributeKind,
        target: AttributeTarget,
    ) -> Result<(), String> {
        match attr {
            AttributeKind::Inline(_) | AttributeKind::Cold => {
                if target != AttributeTarget::Function {
                    return Err(format!(
                        "Attribute can only be applied to functions, found on {:?}",
                        target
                    ));
                }
            }
            AttributeKind::NoMangle | AttributeKind::Export | AttributeKind::Ffi(_) => {
                if target != AttributeTarget::Function && target != AttributeTarget::Struct {
                    return Err(format!(
                        "Linkage attribute can only be applied to functions or structs, found on {:?}",
                        target
                    ));
                }
            }
            AttributeKind::TargetFeature(_) => {
                if target != AttributeTarget::Function && target != AttributeTarget::Module {
                    return Err(format!(
                        "target_feature attribute can only be applied to functions or modules, found on {:?}",
                        target
                    ));
                }
            }
            AttributeKind::Custom(_, _) => {}
        }
        Ok(())
    }
}
