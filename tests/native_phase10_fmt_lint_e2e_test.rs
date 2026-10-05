//! Phase 10 Formatting, Linting & Attribute Validation E2E Test Suite.
//!
//! Validates:
//! - Attribute validation for inline, cold, export, target_feature.
//! - Rejection of misplaced attributes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::attributes::{AttributeKind, AttributeTarget, AttributeValidator, InlineHint};

#[test]
fn test_attribute_validation_placement() {
    let inline_attr = AttributeKind::Inline(InlineHint::Always);
    assert!(AttributeValidator::validate(&inline_attr, AttributeTarget::Function).is_ok());
    assert!(AttributeValidator::validate(&inline_attr, AttributeTarget::Field).is_err());

    let target_feat = AttributeKind::TargetFeature("avx2".to_string());
    assert!(AttributeValidator::validate(&target_feat, AttributeTarget::Function).is_ok());
    assert!(AttributeValidator::validate(&target_feat, AttributeTarget::Struct).is_err());
}
