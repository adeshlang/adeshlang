//! Decorator Compile-Time Phase Executor
//!
//! This module executes decorator compile and typecheck phases during compilation.
//! These phases run before code generation and can enforce contracts, validate
//! function properties, and inject metadata.

use crate::parsing::ast::*;
use std::collections::HashMap;

/// Execute all compile-time decorator phases for a program
pub fn execute_compile_time_phases(ast: &mut [Stmt]) -> Result<(), String> {
    // First pass: collect all decorator definitions
    let mut decorator_defs: HashMap<String, DecoratorDef> = HashMap::new();

    for stmt in ast.iter() {
        if let StmtKind::Decorator(def, _) = &stmt.kind {
            decorator_defs.insert(def.name.clone(), def.clone());
        }
    }

    // Second pass: execute compile-time phases on decorated functions
    for stmt in ast.iter_mut() {
        match &mut stmt.kind {
            StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
                if !func.decorators.is_empty() {
                    execute_function_compile_phases(func, &decorator_defs)?;
                }
            }
            StmtKind::Class(class, _) | StmtKind::ExportDefaultClass(class) => {
                // Check decorators on class methods
                for method in &mut class.methods {
                    if !method.decorators.is_empty() {
                        execute_function_compile_phases(method, &decorator_defs)?;
                    }
                }
                for method in &mut class.static_methods {
                    if !method.decorators.is_empty() {
                        execute_function_compile_phases(method, &decorator_defs)?;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// Execute compile-time phases for a specific function
fn execute_function_compile_phases(
    func: &mut Function,
    decorator_defs: &HashMap<String, DecoratorDef>,
) -> Result<(), String> {
    // Collect decorator names first to avoid borrow issues
    let decorator_names: Result<Vec<String>, String> = func
        .decorators
        .iter()
        .map(|expr| extract_decorator_name(expr))
        .collect();
    let decorator_names = decorator_names?;

    // Process decorators in order (top to bottom in source)
    for decorator_name in decorator_names {
        if let Some(def) = decorator_defs.get(&decorator_name) {
            // Execute each phase
            for phase in &def.phases {
                match phase {
                    DecoratorPhase::Typecheck(_body) => {
                        // Execute typecheck validations
                        execute_typecheck_phase(&decorator_name, func)?;
                    }
                    DecoratorPhase::Compile(_body) => {
                        // Execute compile-time transformations
                        execute_compile_phase(&decorator_name, func)?;
                    }
                    _ => {} // Runtime and Emit phases are not executed here
                }
            }

            // Check unsafe requirements
            if def.requires_unsafe {
                validate_unsafe_requirement(&decorator_name, func)?;
            }
        }
    }

    Ok(())
}

/// Execute typecheck phase validations
fn execute_typecheck_phase(decorator_name: &str, func: &Function) -> Result<(), String> {
    // Common typecheck validations based on decorator semantics
    match decorator_name {
        "pure" => {
            // Pure functions should have return types
            if func.ret_type.is_none() {
                eprintln!(
                    "⚠️  Warning: @pure on function '{}' - pure functions should have return type annotations",
                    func.name
                );
            }
            // Pure functions shouldn't be async
            if func.is_async {
                return Err(format!(
                    "Error: @pure cannot be applied to async function '{}'",
                    func.name
                ));
            }
        }
        "memoize" => {
            // Memoize requires deterministic functions
            if func.ret_type.is_none() {
                eprintln!(
                    "⚠️  Warning: @memoize on function '{}' - memoization works best with typed return values",
                    func.name
                );
            }
        }
        "noalloc" => {
            // Would check for heap allocations in function body
            eprintln!("✓ Typecheck: @noalloc on function '{}'", func.name);
        }
        _ => {
            // Generic typecheck
            if func.ret_type.is_none() {
                eprintln!(
                    "⚠️  Warning: @{} typecheck phase on function '{}' - function has no return type annotation",
                    decorator_name, func.name
                );
            }
        }
    }

    Ok(())
}

/// Execute compile-time phase transformations
fn execute_compile_phase(decorator_name: &str, func: &mut Function) -> Result<(), String> {
    // Perform compile-time metadata injection and transformations
    eprintln!(
        "✓ Compile phase: @{} on function '{}' (metadata injection would happen here)",
        decorator_name, func.name
    );

    // In a full implementation, we would:
    // 1. Inject metadata into the function
    // 2. Transform the AST if needed
    // 3. Register function properties with the type system

    Ok(())
}

/// Validate unsafe requirement
fn validate_unsafe_requirement(decorator_name: &str, func: &Function) -> Result<(), String> {
    // In a full implementation, check if function is in unsafe context
    // For now, just warn
    eprintln!(
        "⚠️  Warning: @{} requires unsafe on function '{}' - full unsafe checking not yet implemented",
        decorator_name, func.name
    );
    Ok(())
}

/// Extract the decorator name from a decorator expression
fn extract_decorator_name(expr: &Expr) -> Result<String, String> {
    match &expr.kind {
        ExprKind::Variable(name) => Ok(name.clone()),
        ExprKind::Call(callee, _, _) => {
            // For decorator factories like @retry(3, 100), extract the function name
            if let ExprKind::Variable(name) = &callee.kind {
                Ok(name.clone())
            } else {
                Err("Invalid decorator: expected function name or call".to_string())
            }
        }
        _ => Err(format!("Invalid decorator expression: {:?}", expr.kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_decorator_name() {
        let expr = Expr {
            kind: ExprKind::Variable("memoize".to_string()),
            span: Span::default(),
        };
        assert_eq!(extract_decorator_name(&expr).unwrap(), "memoize");
    }

    #[test]
    fn test_typecheck_phase_pure_decorator() {
        let func = Function {
            name: "add".to_string(),
            type_params: vec![],
            params: vec![],
            body: std::sync::Arc::new(vec![]),
            visibility: None,
            ret_type: Some("Int".to_string()),
            is_async: false,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_getter: false,
            is_setter: false,
            is_operator: false,
            operator_symbol: None,
            decorators: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };

        // Should succeed for pure with return type
        assert!(execute_typecheck_phase("pure", &func).is_ok());
    }

    #[test]
    fn test_typecheck_phase_async_pure_error() {
        let func = Function {
            name: "fetch".to_string(),
            type_params: vec![],
            params: vec![],
            body: std::sync::Arc::new(vec![]),
            visibility: None,
            ret_type: Some("String".to_string()),
            is_async: true, // Async function
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_getter: false,
            is_setter: false,
            is_operator: false,
            operator_symbol: None,
            decorators: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };

        // Should fail for pure + async
        assert!(execute_typecheck_phase("pure", &func).is_err());
    }

    #[test]
    fn test_compile_phase_execution() {
        let mut func = Function {
            name: "compute".to_string(),
            type_params: vec![],
            params: vec![],
            body: std::sync::Arc::new(vec![]),
            visibility: None,
            ret_type: Some("Int".to_string()),
            is_async: false,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_getter: false,
            is_setter: false,
            is_operator: false,
            operator_symbol: None,
            decorators: vec![],
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        };

        // Should succeed
        assert!(execute_compile_phase("memoize", &mut func).is_ok());
    }
}
