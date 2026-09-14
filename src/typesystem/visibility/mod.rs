//! Visibility Enforcement
//!
//! Runtime enforcement of visibility modifiers (private, protected, public).
//! This module provides:
//! - Visibility checking for class members
//! - Runtime access control
//! - Error messages with proper context
//!
//! Visibility Rules:
//! - public: accessible from anywhere
//! - protected: accessible from class and subclasses
//! - private: accessible only within the defining class

use std::collections::HashMap;

/// Visibility levels for class members
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Public
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Protected => write!(f, "protected"),
            Visibility::Private => write!(f, "private"),
        }
    }
}

/// Result of visibility check
#[derive(Debug, Clone)]
pub struct VisibilityError {
    pub member_name: String,
    pub member_visibility: Visibility,
    pub defining_class: String,
    pub accessing_class: Option<String>,
    pub message: String,
}

impl std::fmt::Display for VisibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Access context for visibility checking
#[derive(Debug, Clone)]
pub struct AccessContext {
    /// The class where access is occurring (None if global scope)
    pub current_class: Option<String>,
    /// The class hierarchy for the current class (for protected access)
    pub class_hierarchy: Vec<String>,
    /// Whether we're inside a method of the class
    pub is_method_context: bool,
}

impl AccessContext {
    /// Create a global (non-class) context
    pub fn global() -> Self {
        AccessContext {
            current_class: None,
            class_hierarchy: Vec::new(),
            is_method_context: false,
        }
    }

    /// Create a class method context
    pub fn class_method(class_name: &str, hierarchy: Vec<String>) -> Self {
        AccessContext {
            current_class: Some(class_name.to_string()),
            class_hierarchy: hierarchy,
            is_method_context: true,
        }
    }

    /// Check if current context is inside the given class
    pub fn is_inside_class(&self, class_name: &str) -> bool {
        self.current_class
            .as_ref()
            .map(|c| c == class_name)
            .unwrap_or(false)
    }

    /// Check if current context is inside a subclass of the given class
    pub fn is_subclass_of(&self, class_name: &str) -> bool {
        self.class_hierarchy.contains(&class_name.to_string())
    }
}

/// Information about a class member's visibility
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub name: String,
    pub visibility: Visibility,
    pub defining_class: String,
    pub is_static: bool,
}

/// Visibility checker for a class
#[derive(Debug, Clone)]
pub struct ClassVisibility {
    pub class_name: String,
    pub parent_class: Option<String>,
    pub members: HashMap<String, MemberInfo>,
}

impl ClassVisibility {
    /// Create a new class visibility tracker
    pub fn new(name: &str, parent: Option<String>) -> Self {
        ClassVisibility {
            class_name: name.to_string(),
            parent_class: parent,
            members: HashMap::new(),
        }
    }

    /// Register a member with visibility
    pub fn register_member(&mut self, name: &str, visibility: Visibility, is_static: bool) {
        self.members.insert(
            name.to_string(),
            MemberInfo {
                name: name.to_string(),
                visibility,
                defining_class: self.class_name.clone(),
                is_static,
            },
        );
    }

    /// Get visibility of a member
    pub fn get_visibility(&self, name: &str) -> Option<Visibility> {
        self.members.get(name).map(|m| m.visibility)
    }
}

/// Global visibility registry for all classes
#[derive(Debug, Default)]
pub struct VisibilityRegistry {
    classes: HashMap<String, ClassVisibility>,
}

impl VisibilityRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        VisibilityRegistry::default()
    }

    /// Register a class
    pub fn register_class(&mut self, class: ClassVisibility) {
        self.classes.insert(class.class_name.clone(), class);
    }

    /// Check if access is allowed
    pub fn check_access(
        &self,
        class_name: &str,
        member_name: &str,
        context: &AccessContext,
    ) -> Result<(), VisibilityError> {
        let class = match self.classes.get(class_name) {
            Some(c) => c,
            None => return Ok(()), // Unknown class - allow access
        };

        let member = match class.members.get(member_name) {
            Some(m) => m,
            None => return Ok(()), // Unknown member - allow access (might be dynamic)
        };

        match member.visibility {
            Visibility::Public => Ok(()),

            Visibility::Protected => {
                // Allow access from same class or subclasses
                if context.is_inside_class(class_name) || context.is_subclass_of(class_name) {
                    Ok(())
                } else {
                    Err(VisibilityError {
                        member_name: member_name.to_string(),
                        member_visibility: Visibility::Protected,
                        defining_class: class_name.to_string(),
                        accessing_class: context.current_class.clone(),
                        message: format!(
                            "Cannot access protected member '{}' of class '{}' from {}",
                            member_name,
                            class_name,
                            context.current_class.as_deref().unwrap_or("global scope")
                        ),
                    })
                }
            }

            Visibility::Private => {
                // Allow access only from same class
                if context.is_inside_class(class_name) {
                    Ok(())
                } else {
                    Err(VisibilityError {
                        member_name: member_name.to_string(),
                        member_visibility: Visibility::Private,
                        defining_class: class_name.to_string(),
                        accessing_class: context.current_class.clone(),
                        message: format!(
                            "Cannot access private member '{}' of class '{}'",
                            member_name, class_name
                        ),
                    })
                }
            }
        }
    }

    /// Get class inheritance hierarchy
    pub fn get_hierarchy(&self, class_name: &str) -> Vec<String> {
        let mut hierarchy = vec![class_name.to_string()];
        let mut current = class_name;

        while let Some(class) = self.classes.get(current) {
            if let Some(parent) = &class.parent_class {
                hierarchy.push(parent.clone());
                current = parent;
            } else {
                break;
            }
        }

        hierarchy
    }
}

/// Parse visibility from AST visibility enum
pub fn parse_visibility(vis: Option<&crate::parsing::ast::Visibility>) -> Visibility {
    match vis {
        Some(crate::parsing::ast::Visibility::Pub) | None => Visibility::Public,
        Some(crate::parsing::ast::Visibility::Priv) => Visibility::Private,
        Some(crate::parsing::ast::Visibility::Protected) => Visibility::Protected,
    }
}

// ============================================
// NULLABLE TYPES SUPPORT
// ============================================

/// Represents a nullable type (?Type)
#[derive(Debug, Clone, PartialEq)]
pub enum NullableType {
    /// Non-nullable type
    NonNull(String),
    /// Nullable type (?Type)
    Nullable(String),
    /// Union type (Type1 | Type2)
    Union(Vec<String>),
}

impl NullableType {
    /// Parse a type annotation string
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim();

        // Check for nullable syntax
        if trimmed.ends_with('?') {
            let base = trimmed[..trimmed.len() - 1].trim();
            return NullableType::Nullable(base.to_string());
        }

        // Check for union syntax
        if trimmed.contains('|') {
            let parts: Vec<String> = trimmed
                .split('|')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            if parts.len() > 1 {
                return NullableType::Union(parts);
            }
        }

        NullableType::NonNull(trimmed.to_string())
    }

    /// Check if a value matches this type
    pub fn matches(&self, value_type: &str, is_null: bool) -> bool {
        match self {
            NullableType::NonNull(expected) => {
                if is_null {
                    return false; // Non-nullable can't be null
                }
                type_matches(expected, value_type)
            }
            NullableType::Nullable(expected) => {
                if is_null {
                    return true; // Nullable allows null
                }
                type_matches(expected, value_type)
            }
            NullableType::Union(types) => {
                if is_null {
                    // Check if null is part of union
                    return types.iter().any(|t| t.to_lowercase() == "null");
                }
                types.iter().any(|t| type_matches(t, value_type))
            }
        }
    }

    /// Check if this type allows null
    pub fn allows_null(&self) -> bool {
        match self {
            NullableType::NonNull(_) => false,
            NullableType::Nullable(_) => true,
            NullableType::Union(types) => types.iter().any(|t| t.to_lowercase() == "null"),
        }
    }
}

/// Check if an expected type matches an actual type
fn type_matches(expected: &str, actual: &str) -> bool {
    let exp = expected.to_lowercase();
    let act = actual.to_lowercase();

    if exp == act || exp == "any" {
        return true;
    }

    // Handle common aliases
    match (exp.as_str(), act.as_str()) {
        ("int", "number") | ("float", "number") | ("number", "int") | ("number", "float") => true,
        ("str", "string") | ("string", "str") => true,
        ("bool", "boolean") | ("boolean", "bool") => true,
        _ => false,
    }
}

/// Runtime type checking result
#[derive(Debug, Clone)]
pub struct TypeCheckError {
    pub expected: String,
    pub actual: String,
    pub context: String,
}

impl std::fmt::Display for TypeCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Type error in {}: expected {}, got {}",
            self.context, self.expected, self.actual
        )
    }
}

/// Check a value against an expected type annotation at runtime
pub fn check_runtime_type(
    value_type: &str,
    is_null: bool,
    type_annotation: &str,
    context: &str,
) -> Result<(), TypeCheckError> {
    let expected = NullableType::parse(type_annotation);

    if expected.matches(value_type, is_null) {
        Ok(())
    } else {
        Err(TypeCheckError {
            expected: type_annotation.to_string(),
            actual: if is_null {
                "null".to_string()
            } else {
                value_type.to_string()
            },
            context: context.to_string(),
        })
    }
}

// ============================================
// TYPE NARROWING FOR UNIONS
// ============================================

/// Represents a type that has been narrowed through control flow analysis
#[derive(Debug, Clone)]
pub struct NarrowedType {
    /// Original type before narrowing
    pub original: NullableType,
    /// Current narrowed type
    pub narrowed: NullableType,
    /// How the type was narrowed
    pub narrowing_kind: NarrowingKind,
}

/// How a type was narrowed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrowingKind {
    /// Type was narrowed via `typeof` check
    TypeofCheck(String),
    /// Type was narrowed via null check
    NullCheck,
    /// Type was narrowed via truthiness check
    TruthinessCheck,
    /// Type was narrowed via instanceof check
    InstanceofCheck(String),
    /// Type was narrowed via property presence check
    PropertyCheck(String),
    /// No narrowing applied
    None,
}

impl NarrowedType {
    /// Create a new narrowed type
    pub fn new(original: NullableType) -> Self {
        NarrowedType {
            original: original.clone(),
            narrowed: original,
            narrowing_kind: NarrowingKind::None,
        }
    }

    /// Narrow based on typeof check
    /// e.g., `if (typeof x == "number")` narrows Number | String to Number
    pub fn narrow_by_typeof(&mut self, expected_type: &str) {
        self.narrowing_kind = NarrowingKind::TypeofCheck(expected_type.to_string());

        match &self.original {
            NullableType::Union(types) => {
                // Keep only types that match the typeof check
                let matching: Vec<String> = types
                    .iter()
                    .filter(|t| type_matches_typeof(t, expected_type))
                    .cloned()
                    .collect();

                if matching.len() == 1 {
                    self.narrowed = NullableType::NonNull(matching[0].clone());
                } else if !matching.is_empty() {
                    self.narrowed = NullableType::Union(matching);
                }
            }
            NullableType::Nullable(base) => {
                if expected_type != "null" && expected_type != "undefined" {
                    // If checking for non-null type, narrow to non-null
                    if type_matches_typeof(base, expected_type) {
                        self.narrowed = NullableType::NonNull(base.clone());
                    }
                }
            }
            _ => {}
        }
    }

    /// Narrow by null check (removes null from union or nullable)
    pub fn narrow_by_null_check(&mut self, is_null: bool) {
        self.narrowing_kind = NarrowingKind::NullCheck;

        if is_null {
            // x == null -> x is null
            self.narrowed = NullableType::NonNull("null".to_string());
        } else {
            // x != null -> remove null from type
            match &self.original {
                NullableType::Nullable(base) => {
                    self.narrowed = NullableType::NonNull(base.clone());
                }
                NullableType::Union(types) => {
                    let non_null: Vec<String> = types
                        .iter()
                        .filter(|t| t.to_lowercase() != "null")
                        .cloned()
                        .collect();

                    if non_null.len() == 1 {
                        self.narrowed = NullableType::NonNull(non_null[0].clone());
                    } else {
                        self.narrowed = NullableType::Union(non_null);
                    }
                }
                _ => {}
            }
        }
    }

    /// Narrow by truthiness check
    pub fn narrow_by_truthiness(&mut self, is_truthy: bool) {
        self.narrowing_kind = NarrowingKind::TruthinessCheck;

        if is_truthy {
            // Truthy check removes null/undefined and falsy values
            match &self.original {
                NullableType::Nullable(base) => {
                    self.narrowed = NullableType::NonNull(base.clone());
                }
                NullableType::Union(types) => {
                    let truthy_types: Vec<String> = types
                        .iter()
                        .filter(|t| {
                            let lower = t.to_lowercase();
                            lower != "null" && lower != "undefined"
                        })
                        .cloned()
                        .collect();

                    if truthy_types.len() == 1 {
                        self.narrowed = NullableType::NonNull(truthy_types[0].clone());
                    } else if !truthy_types.is_empty() {
                        self.narrowed = NullableType::Union(truthy_types);
                    }
                }
                _ => {}
            }
        }
    }

    /// Get the narrowed type
    pub fn get_narrowed(&self) -> &NullableType {
        &self.narrowed
    }

    /// Check if type was narrowed
    pub fn is_narrowed(&self) -> bool {
        self.narrowing_kind != NarrowingKind::None
    }
}

/// Check if a type name matches a typeof result
fn type_matches_typeof(type_name: &str, typeof_result: &str) -> bool {
    let name = type_name.to_lowercase();
    let result = typeof_result.to_lowercase();

    match result.as_str() {
        "number" => matches!(name.as_str(), "number" | "int" | "float"),
        "string" => matches!(name.as_str(), "string" | "str"),
        "boolean" => matches!(name.as_str(), "boolean" | "bool"),
        "object" => matches!(name.as_str(), "object" | "array" | "null"),
        "function" => name == "function",
        "undefined" => name == "undefined",
        _ => name == result,
    }
}

// ============================================
// NULLABLE PROPAGATION WARNINGS
// ============================================

/// Warning generated during nullable propagation analysis
#[derive(Debug, Clone)]
pub struct NullableWarning {
    pub variable: String,
    pub operation: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl std::fmt::Display for NullableWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Warning: {} on nullable variable '{}': {}",
            self.operation, self.variable, self.message
        )?;
        if let Some(suggestion) = &self.suggestion {
            write!(f, "\n  Suggestion: {}", suggestion)?;
        }
        Ok(())
    }
}

/// Tracks nullable propagation through code
#[derive(Debug, Default)]
pub struct NullablePropagation {
    /// Variables and their nullable status
    variables: std::collections::HashMap<String, bool>,
    /// Warnings accumulated during analysis
    warnings: Vec<NullableWarning>,
}

impl NullablePropagation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a variable with nullable status
    pub fn declare(&mut self, name: &str, is_nullable: bool) {
        self.variables.insert(name.to_string(), is_nullable);
    }

    /// Check if a variable is nullable
    pub fn is_nullable(&self, name: &str) -> bool {
        self.variables.get(name).copied().unwrap_or(false)
    }

    /// Record a property access on a variable
    /// Generates warning if accessing property on nullable without null check
    pub fn record_property_access(&mut self, variable: &str, property: &str) {
        if self.is_nullable(variable) {
            self.warnings.push(NullableWarning {
                variable: variable.to_string(),
                operation: format!(".{}", property),
                message: format!(
                    "Accessing property '{}' on nullable variable '{}' without null check",
                    property, variable
                ),
                suggestion: Some(format!(
                    "Add a null check: if ({} != null) {{ {}.{} }}",
                    variable, variable, property
                )),
            });
        }
    }

    /// Record a method call on a variable
    pub fn record_method_call(&mut self, variable: &str, method: &str) {
        if self.is_nullable(variable) {
            self.warnings.push(NullableWarning {
                variable: variable.to_string(),
                operation: format!(".{}()", method),
                message: format!(
                    "Calling method '{}' on nullable variable '{}' without null check",
                    method, variable
                ),
                suggestion: Some(format!(
                    "Add a null check: if ({} != null) {{ {}.{}() }}",
                    variable, variable, method
                )),
            });
        }
    }

    /// Mark a variable as checked (not nullable in this scope)
    pub fn mark_checked(&mut self, name: &str) {
        if self.variables.contains_key(name) {
            self.variables.insert(name.to_string(), false);
        }
    }

    /// Get accumulated warnings
    pub fn warnings(&self) -> &[NullableWarning] {
        &self.warnings
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Clear warnings
    pub fn clear_warnings(&mut self) {
        self.warnings.clear();
    }
}

// ============================================
// ENHANCED ERROR MESSAGES
// ============================================

/// Generate enhanced type error message with inferred vs expected types
///
/// Prefer passing `file` so the location is clickable as `file:line:col`.
pub fn format_type_error(
    expected: &str,
    actual: &str,
    context: &str,
    variable: Option<&str>,
    line: Option<usize>,
    col: Option<usize>,
) -> String {
    format_type_error_in(expected, actual, context, variable, None, line, col)
}

/// Same as [`format_type_error`] with an optional source file path.
pub fn format_type_error_in(
    expected: &str,
    actual: &str,
    context: &str,
    variable: Option<&str>,
    file: Option<&str>,
    line: Option<usize>,
    col: Option<usize>,
) -> String {
    use crate::parsing::error::{ErrorKind, LangError, format_clickable_location};

    let mut message = String::new();
    if let Some(var) = variable {
        message.push_str(&format!("in {} for '{}': ", context, var));
    } else {
        message.push_str(&format!("in {}: ", context));
    }
    message.push_str(&format!("expected '{}', got '{}'", expected, actual));

    let line = line.unwrap_or(0);
    let col = col.unwrap_or(0);
    LangError::new(ErrorKind::Type, message, line, col, String::new())
        .with_file_opt(file.map(|f| f.to_string()))
        .with_code("E0308")
        .with_help(format!(
            "value has type '{}', but '{}' was required here",
            actual, expected
        ))
        .to_string()
        // Keep a plain clickable prefix for callers that strip colors / parse text
        .replace(
            &format!("line {} col {}", line.max(1), col),
            &format_clickable_location(file, line, col),
        )
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_public() {
        let mut registry = VisibilityRegistry::new();
        let mut class = ClassVisibility::new("MyClass", None);
        class.register_member("publicMethod", Visibility::Public, false);
        registry.register_class(class);

        let context = AccessContext::global();
        assert!(
            registry
                .check_access("MyClass", "publicMethod", &context)
                .is_ok()
        );
    }

    #[test]
    fn test_visibility_private() {
        let mut registry = VisibilityRegistry::new();
        let mut class = ClassVisibility::new("MyClass", None);
        class.register_member("privateField", Visibility::Private, false);
        registry.register_class(class);

        // Access from global scope should fail
        let global_context = AccessContext::global();
        assert!(
            registry
                .check_access("MyClass", "privateField", &global_context)
                .is_err()
        );

        // Access from same class should succeed
        let class_context = AccessContext::class_method("MyClass", vec!["MyClass".to_string()]);
        assert!(
            registry
                .check_access("MyClass", "privateField", &class_context)
                .is_ok()
        );
    }

    #[test]
    fn test_visibility_protected() {
        let mut registry = VisibilityRegistry::new();

        // Parent class with protected member
        let mut parent = ClassVisibility::new("Parent", None);
        parent.register_member("protectedField", Visibility::Protected, false);
        registry.register_class(parent);

        // Child class
        let child = ClassVisibility::new("Child", Some("Parent".to_string()));
        registry.register_class(child);

        // Access from subclass should succeed
        let child_context =
            AccessContext::class_method("Child", vec!["Child".to_string(), "Parent".to_string()]);
        assert!(
            registry
                .check_access("Parent", "protectedField", &child_context)
                .is_ok()
        );

        // Access from unrelated class should fail
        let other_context = AccessContext::class_method("Other", vec!["Other".to_string()]);
        assert!(
            registry
                .check_access("Parent", "protectedField", &other_context)
                .is_err()
        );
    }

    #[test]
    fn test_nullable_type_parsing() {
        assert_eq!(
            NullableType::parse("String?"),
            NullableType::Nullable("String".to_string())
        );
        assert_eq!(
            NullableType::parse("Number"),
            NullableType::NonNull("Number".to_string())
        );
        assert_eq!(
            NullableType::parse("Number | String"),
            NullableType::Union(vec!["Number".to_string(), "String".to_string()])
        );
    }

    #[test]
    fn test_nullable_type_matching() {
        // Non-nullable
        let non_null = NullableType::NonNull("Number".to_string());
        assert!(non_null.matches("number", false));
        assert!(!non_null.matches("string", false));
        assert!(!non_null.matches("number", true)); // null not allowed

        // Nullable
        let nullable = NullableType::Nullable("String".to_string());
        assert!(nullable.matches("string", false));
        assert!(nullable.matches("string", true)); // null allowed
        assert!(!nullable.matches("number", false));

        // Union
        let union = NullableType::Union(vec!["Number".to_string(), "String".to_string()]);
        assert!(union.matches("number", false));
        assert!(union.matches("string", false));
        assert!(!union.matches("boolean", false));
    }

    #[test]
    fn test_runtime_type_check() {
        // Valid check
        assert!(check_runtime_type("number", false, "Number", "test").is_ok());

        // Invalid check
        let err = check_runtime_type("string", false, "Number", "test");
        assert!(err.is_err());

        // Nullable check with null
        assert!(check_runtime_type("", true, "String?", "test").is_ok());

        // Non-nullable check with null
        assert!(check_runtime_type("", true, "String", "test").is_err());
    }

    #[test]
    fn test_type_narrowing_typeof() {
        // Union type
        let union = NullableType::Union(vec!["Number".to_string(), "String".to_string()]);
        let mut narrowed = NarrowedType::new(union);

        // Narrow by typeof "number"
        narrowed.narrow_by_typeof("number");
        assert!(narrowed.is_narrowed());

        // Should narrow to just Number
        if let NullableType::NonNull(t) = narrowed.get_narrowed() {
            assert_eq!(t, "Number");
        } else {
            panic!("Expected NonNull after narrowing");
        }
    }

    #[test]
    fn test_type_narrowing_null_check() {
        // Nullable type
        let nullable = NullableType::Nullable("String".to_string());
        let mut narrowed = NarrowedType::new(nullable);

        // Narrow by != null check
        narrowed.narrow_by_null_check(false);
        assert!(narrowed.is_narrowed());

        // Should narrow to non-null String
        if let NullableType::NonNull(t) = narrowed.get_narrowed() {
            assert_eq!(t, "String");
        } else {
            panic!("Expected NonNull after null check");
        }
    }

    #[test]
    fn test_nullable_propagation() {
        let mut propagation = NullablePropagation::new();

        // Declare nullable variable
        propagation.declare("maybeString", true);
        assert!(propagation.is_nullable("maybeString"));

        // Access property on nullable - should generate warning
        propagation.record_property_access("maybeString", "length");
        assert!(propagation.has_warnings());
        assert_eq!(propagation.warnings().len(), 1);

        // Mark as checked
        propagation.mark_checked("maybeString");
        propagation.clear_warnings();

        // Now accessing should be safe (no new warning)
        propagation.record_property_access("maybeString", "length");
        assert!(!propagation.has_warnings());
    }

    #[test]
    fn test_format_type_error() {
        let msg = format_type_error_in(
            "Number",
            "String",
            "assignment",
            Some("x"),
            Some("<test>.adesh"),
            Some(10),
            Some(5),
        );

        assert!(msg.contains("expected 'Number'"));
        assert!(msg.contains("got 'String'"));
    }
}
