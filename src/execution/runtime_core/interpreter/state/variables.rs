//! Variable Management
//!
//! This module provides high-level APIs for variable operations including
//! declaration, lookup, assignment, and scope resolution. It works with
//! the ScopeStack to provide convenient variable management.
//!
//! ## Key Features
//!
//! - **Variable Declaration**: Type-safe variable creation
//! - **Scope Resolution**: Automatic scope chain traversal
//! - **Const Enforcement**: Immutable variable protection
//! - **Export Tracking**: Module export management
//! - **Type Annotations**: Optional type tracking
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::state::{VariableManager, ScopeStack};
//! use crate::parsing::ast::Value;
//!
//! let mut scopes = ScopeStack::new();
//! let mut vars = VariableManager::new();
//!
//! let scope = scopes.push(None).unwrap();
//!
//! // Declare variable
//! vars.declare(&mut scopes, scope, "x", Value::Number(42.0), false)?;
//!
//! // Lookup variable
//! let value = vars.get(&scopes, scope, "x")?;
//!
//! // Assign new value
//! vars.set(&mut scopes, scope, "x", Value::Number(43.0))?;
//! ```

#[allow(unused_imports)]
use super::scope_stack::{Scope, ScopeStack};
use crate::parsing::ast::Value;

/// Manages variable operations across scopes.
///
/// Provides high-level APIs for variable declaration, lookup, and assignment
/// with proper error handling and const enforcement.
pub struct VariableManager {
    // Future: Can add caching or optimization state here
}

impl VariableManager {
    /// Create a new variable manager.
    #[inline]
    pub fn new() -> Self {
        Self {}
    }

    /// Declare a new variable in the given scope.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Scope to declare in
    /// * `name` - Variable name
    /// * `value` - Initial value
    /// * `is_const` - Whether the variable is immutable
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if scope invalid or variable already exists
    pub fn declare(
        &mut self,
        scopes: &mut ScopeStack,
        scope_id: usize,
        name: &str,
        value: Value,
        is_const: bool,
    ) -> Result<(), String> {
        if let Some(scope) = scopes.get_mut(scope_id) {
            // Check if variable already declared in this scope
            if scope.values.contains_key(name) {
                return Err(format!(
                    "Variable '{}' already declared in this scope",
                    name
                ));
            }

            scope.values.insert(name.to_string(), value);

            if is_const {
                scope.consts.insert(name.to_string(), true);
            }

            Ok(())
        } else {
            Err(format!("Invalid scope ID: {}", scope_id))
        }
    }

    /// Get a variable from the scope chain.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// The variable value, or Err if not found
    pub fn get(&self, scopes: &ScopeStack, scope_id: usize, name: &str) -> Result<Value, String> {
        scopes
            .get_variable(scope_id, name)
            .cloned()
            .ok_or_else(|| format!("Variable '{}' not found", name))
    }

    /// Get a reference to a variable (avoids cloning).
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// Reference to the variable value, or Err if not found
    pub fn get_ref<'a>(
        &self,
        scopes: &'a ScopeStack,
        scope_id: usize,
        name: &str,
    ) -> Result<&'a Value, String> {
        scopes
            .get_variable(scope_id, name)
            .ok_or_else(|| format!("Variable '{}' not found", name))
    }

    /// Set a variable in the scope chain.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    /// * `value` - New value
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if variable not found or is const
    pub fn set(
        &mut self,
        scopes: &mut ScopeStack,
        scope_id: usize,
        name: &str,
        value: Value,
    ) -> Result<(), String> {
        // Check if variable is const
        if self.is_const(scopes, scope_id, name) {
            return Err(format!("Cannot assign to const variable '{}'", name));
        }

        scopes.set(scope_id, name, value)
    }

    /// Check if a variable is declared as const.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// true if the variable is const, false otherwise
    pub fn is_const(&self, scopes: &ScopeStack, scope_id: usize, name: &str) -> bool {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            if let Some(scope) = scopes.get(id) {
                if scope.values.contains_key(name) {
                    return scope.consts.get(name).copied().unwrap_or(false);
                }
                current = scope.enclosing;
            } else {
                break;
            }
        }

        false
    }

    /// Check if a variable exists in the scope chain.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// true if the variable exists, false otherwise
    #[inline]
    pub fn exists(&self, scopes: &ScopeStack, scope_id: usize, name: &str) -> bool {
        scopes.has_variable(scope_id, name)
    }

    /// Declare an exported variable.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Scope to declare in
    /// * `name` - Variable name
    /// * `value` - Initial value
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if scope invalid
    pub fn declare_export(
        &mut self,
        scopes: &mut ScopeStack,
        scope_id: usize,
        name: &str,
        value: Value,
    ) -> Result<(), String> {
        if let Some(scope) = scopes.get_mut(scope_id) {
            scope.exports.insert(name.to_string(), value.clone());
            scope.values.insert(name.to_string(), value);
            Ok(())
        } else {
            Err(format!("Invalid scope ID: {}", scope_id))
        }
    }

    /// Get an exported variable.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Scope to check
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// The exported value, or Err if not found
    pub fn get_export(
        &self,
        scopes: &ScopeStack,
        scope_id: usize,
        name: &str,
    ) -> Result<Value, String> {
        if let Some(scope) = scopes.get(scope_id) {
            scope
                .exports
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Export '{}' not found", name))
        } else {
            Err(format!("Invalid scope ID: {}", scope_id))
        }
    }

    /// Set a type annotation for a variable.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Scope containing the variable
    /// * `name` - Variable name
    /// * `type_ann` - Type annotation (optional)
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if scope invalid
    pub fn set_type_annotation(
        &mut self,
        scopes: &mut ScopeStack,
        scope_id: usize,
        name: &str,
        type_ann: Option<String>,
    ) -> Result<(), String> {
        if let Some(scope) = scopes.get_mut(scope_id) {
            scope.type_ann.insert(name.to_string(), type_ann);
            Ok(())
        } else {
            Err(format!("Invalid scope ID: {}", scope_id))
        }
    }

    /// Get a type annotation for a variable.
    ///
    /// # Arguments
    ///
    /// * `scopes` - The scope stack
    /// * `scope_id` - Starting scope for lookup
    /// * `name` - Variable name
    ///
    /// # Returns
    ///
    /// The type annotation if it exists
    pub fn get_type_annotation(
        &self,
        scopes: &ScopeStack,
        scope_id: usize,
        name: &str,
    ) -> Option<Option<String>> {
        let mut current = Some(scope_id);

        while let Some(id) = current {
            if let Some(scope) = scopes.get(id) {
                if scope.values.contains_key(name) {
                    return scope.type_ann.get(name).cloned();
                }
                current = scope.enclosing;
            } else {
                break;
            }
        }

        None
    }
}

impl Default for VariableManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_declaration() {
        let mut scopes = ScopeStack::new();
        let mut vars = VariableManager::new();

        let scope = scopes.push(None);

        vars.declare(&mut scopes, scope, "x", Value::Number(42.0), false)
            .unwrap();

        let value = vars.get(&scopes, scope, "x").unwrap();
        assert!(matches!(value, Value::Number(n) if n == 42.0));
    }

    #[test]
    fn test_const_enforcement() {
        let mut scopes = ScopeStack::new();
        let mut vars = VariableManager::new();

        let scope = scopes.push(None);

        // Declare const variable
        vars.declare(&mut scopes, scope, "x", Value::Number(42.0), true)
            .unwrap();

        // Try to modify it - should fail
        let result = vars.set(&mut scopes, scope, "x", Value::Number(43.0));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("const"));
    }

    #[test]
    fn test_scope_chain_lookup() {
        let mut scopes = ScopeStack::new();
        let mut vars = VariableManager::new();

        // Global scope
        let global = scopes.push(None);
        vars.declare(&mut scopes, global, "x", Value::Number(10.0), false)
            .unwrap();

        // Local scope
        let local = scopes.push(Some(global));

        // Should find x in parent scope
        let value = vars.get(&scopes, local, "x").unwrap();
        assert!(matches!(value, Value::Number(n) if n == 10.0));
    }

    #[test]
    fn test_exports() {
        let mut scopes = ScopeStack::new();
        let mut vars = VariableManager::new();

        let scope = scopes.push(None);

        vars.declare_export(&mut scopes, scope, "PI", Value::Number(3.14159))
            .unwrap();

        let value = vars.get_export(&scopes, scope, "PI").unwrap();
        assert!(matches!(value, Value::Number(n) if (n - 3.14159).abs() < 0.00001));
    }

    #[test]
    fn test_type_annotations() {
        let mut scopes = ScopeStack::new();
        let mut vars = VariableManager::new();

        let scope = scopes.push(None);

        vars.declare(&mut scopes, scope, "x", Value::Number(42.0), false)
            .unwrap();

        vars.set_type_annotation(&mut scopes, scope, "x", Some("number".to_string()))
            .unwrap();

        let type_ann = vars.get_type_annotation(&scopes, scope, "x");
        assert_eq!(type_ann, Some(Some("number".to_string())));
    }
}
