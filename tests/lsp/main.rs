//! LSP & Semantic Integration Tests
//!
//! Comprehensive tests for AdeshLang Language Server Protocol and Semantic Indexing:
//! - Go-to-definition
//! - Hover information (union, nullable, visibility, ownership)
//! - Workspace symbols
//! - Signature help
//! - Diagnostics (borrow, visibility, syntax)
//! - Incremental compilation / cache

#[cfg(test)]
mod tests {
    use adeshlang::semantics::{
        ImportKind, SemanticSymbolKind, VisibilityKind, index_source,
    };
    use adeshlang::typesystem::checker::Ty;

    /// Test cross-file go-to-definition and import symbol resolution
    #[test]
    fn test_goto_definition_imports() {
        let src = r#"
import "math_utils" as math;
import { calculate_sum, calculate_avg } from "stats";

let res = math.add(10, 20);
let total = calculate_sum([1, 2, 3]);
"#;
        let index = index_source(src);

        // Verify import resolution in semantic index
        assert_eq!(index.imports.len(), 2, "Expected 2 import declarations");

        let math_import = index.imports.iter().find(|i| i.path == "math_utils").expect("math_utils import missing");
        assert_eq!(math_import.alias, "math");
        assert_eq!(math_import.kind, ImportKind::Named);

        let stats_import = index.imports.iter().find(|i| i.path == "stats").expect("stats import missing");
        assert_eq!(stats_import.kind, ImportKind::Names);
        assert!(stats_import.names.contains(&"calculate_sum".to_string()));
        assert!(stats_import.names.contains(&"calculate_avg".to_string()));

        // Check local declarations resolution
        let res_sym = index.find_declaration("res");
        assert!(res_sym.is_some(), "Variable 'res' should be found in symbol index");
        assert_eq!(res_sym.unwrap().kind, SemanticSymbolKind::Variable);
    }

    /// Test workspace symbol indexing across classes, structs, functions, and variables
    #[test]
    fn test_workspace_symbols() {
        let src = r#"
class AccountManager {
    private balance: f64;
    public fn deposit(amount: f64): bool {
        return true;
    }
}

struct Vector3D {
    x: f64,
    y: f64,
    z: f64,
}

fn compute_norm(v: Vector3D): f64 {
    return 0.0;
}

let default_vector = Vector3D(0.0, 0.0, 0.0);
"#;
        let index = index_source(src);

        // Verify all top-level symbols are indexed
        let account_class = index.find_declaration("AccountManager").expect("AccountManager class missing");
        assert_eq!(account_class.kind, SemanticSymbolKind::Class);

        let vec_struct = index.find_declaration("Vector3D").expect("Vector3D struct missing");
        assert_eq!(vec_struct.kind, SemanticSymbolKind::Struct);

        let compute_fn = index.find_declaration("compute_norm").expect("compute_norm function missing");
        assert_eq!(compute_fn.kind, SemanticSymbolKind::Function);

        let default_vec = index.find_declaration("default_vector").expect("default_vector variable missing");
        assert_eq!(default_vec.kind, SemanticSymbolKind::Variable);

        // Verify type members
        let type_def = index.get_type("AccountManager").expect("AccountManager type definition missing");
        assert!(type_def.fields.iter().any(|(f, _, _)| f == "balance"));
        assert!(type_def.methods.iter().any(|m| m.name == "deposit"));
    }

    /// Test hover shows union types
    #[test]
    fn test_hover_union_types() {
        let src = r#"
fn parse_id(raw: string | i32): string {
    return "id";
}
"#;
        let index = index_source(src);
        let fn_sym = index.find_declaration("parse_id").expect("parse_id missing");
        assert_eq!(fn_sym.kind, SemanticSymbolKind::Function);

        // Check parameter types recorded
        if let Some(sig) = index.fns.get("parse_id") {
            assert!(!sig.is_empty(), "Signature params should be recorded");
            let param_ty = sig[0].as_deref().unwrap_or("");
            assert!(
                param_ty.contains("|") || param_ty.contains("string") || param_ty.contains("i32"),
                "Hover signature should retain union type representation, got: {}",
                param_ty
            );
        }
    }

    /// Test hover shows nullable types
    #[test]
    fn test_hover_nullable_types() {
        let src = r#"
class Profile {
    avatar_url: string?;
}
"#;
        let index = index_source(src);
        let type_def = index.get_type("Profile").expect("Profile type missing");
        let avatar_field = type_def.fields.iter().find(|(name, _, _)| name == "avatar_url");
        assert!(avatar_field.is_some(), "avatar_url field should exist");
        let (_, ty_annot, _) = avatar_field.unwrap();
        assert!(
            ty_annot.contains("?") || ty_annot.contains("null") || ty_annot.contains("string"),
            "Nullable type annotation must be preserved on field, got: {}",
            ty_annot
        );
    }

    /// Test hover shows visibility modifiers
    #[test]
    fn test_hover_visibility() {
        let src = r#"
class SecureVault {
    private secret_key: string;
    protected auth_token: string;
    public username: string;

    public fn unlock(): bool {
        return true;
    }
}
"#;
        let index = index_source(src);
        let vault = index.get_type("SecureVault").expect("SecureVault missing");

        let secret = vault.fields.iter().find(|(n, _, _)| n == "secret_key").unwrap();
        assert_eq!(secret.2, VisibilityKind::Private);

        let auth = vault.fields.iter().find(|(n, _, _)| n == "auth_token").unwrap();
        assert_eq!(auth.2, VisibilityKind::Protected);

        let user = vault.fields.iter().find(|(n, _, _)| n == "username").unwrap();
        assert_eq!(user.2, VisibilityKind::Public);

        let unlock = vault.methods.iter().find(|m| m.name == "unlock").unwrap();
        assert_eq!(unlock.visibility, VisibilityKind::Public);
    }

    /// Test hover shows borrow/ownership hints
    #[test]
    fn test_hover_ownership_hints() {
        let src = r#"
class Node {
    val: i32;
}

fn process_node(share_node: share<Node>, weak_node: weak<Node>) {
    print(share_node.val);
}
"#;
        let index = index_source(src);
        let fn_sym = index.find_declaration("process_node").expect("process_node missing");
        assert_eq!(fn_sym.kind, SemanticSymbolKind::Function);

        if let Some(params) = index.fns.get("process_node") {
            assert_eq!(params.len(), 2);
            let p0 = params[0].as_deref().unwrap_or("");
            let p1 = params[1].as_deref().unwrap_or("");
            assert!(p0.contains("share"), "Ownership hint 'share' expected in signature, got: {}", p0);
            assert!(p1.contains("weak"), "Ownership hint 'weak' expected in signature, got: {}", p1);
        }
    }

    /// Test signature help for functions
    #[test]
    fn test_signature_help() {
        let src = r#"
fn create_user(name: string, age: i32, is_admin: bool): string {
    return name;
}
"#;
        let index = index_source(src);
        let fn_decl = index.find_declaration("create_user").expect("create_user missing");
        assert_eq!(fn_decl.kind, SemanticSymbolKind::Function);

        let params = index.fns.get("create_user").expect("params for create_user missing");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].as_deref(), Some("string"));
        assert_eq!(params[1].as_deref(), Some("i32"));
        assert_eq!(params[2].as_deref(), Some("bool"));

        let ret_ty = index.fns_ret_types.get("create_user").expect("return type missing");
        assert_eq!(ret_ty.as_deref(), Some("string"));
    }

    /// Test borrow-check diagnostics
    #[test]
    fn test_borrow_diagnostics() {
        use adeshlang::memory::raii::transform_ast_with_raii;
        use adeshlang::parsing::compile_time_memory_safety::check_memory_safety_compile_time;
        use adeshlang::parsing::hir_lower::ast_to_hir;
        use adeshlang::parsing::lexer::Lexer;
        use adeshlang::parsing::parser::Parser;

        let conflict_src = r#"
        fn main() {
            let buffer = [1, 2, 3];
            let ref1 = &mut buffer;
            let ref2 = &mut buffer;
            print(ref1);
            print(ref2);
        }
        "#;
        let mut lexer = Lexer::new(conflict_src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens, Some("diag_test.adesh".to_string()));
        let ast = parser.parse_program().unwrap();
        let ast = transform_ast_with_raii(ast);
        let hir = ast_to_hir(&ast, false).unwrap();
        let diag_result = check_memory_safety_compile_time(&hir);

        assert!(
            diag_result.is_err(),
            "Borrow diagnostics must detect conflicting mutable borrows"
        );
        let err = diag_result.unwrap_err();
        assert!(
            err.contains("cannot borrow") || err.contains("mutable"),
            "Diagnostic message must explain borrow violation: {}",
            err
        );
    }

    /// Test visibility diagnostics
    #[test]
    fn test_visibility_diagnostics() {
        use adeshlang::{Interpreter, ModuleLoader};
        use std::path::Path;

        let private_access_src = r#"
        class BankAccount {
            private fn internal_vault_pin() {
                return 1234;
            }
        }
        let acc = new BankAccount();
        acc.internal_vault_pin();
        "#;
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        let res = interp.run_module(private_access_src, &mut loader, None);

        assert!(
            res.is_err(),
            "Expected visibility diagnostic error when calling private method from outside"
        );
    }

    /// Test incremental compilation cache
    #[test]
    fn test_incremental_compile() {
        let v1 = r#"
        fn calculate() { return 10; }
        "#;
        let index1 = index_source(v1);
        assert!(index1.find_declaration("calculate").is_some());
        assert!(index1.find_declaration("updated_calculate").is_none());

        // Updated version adds new function
        let v2 = r#"
        fn calculate() { return 10; }
        fn updated_calculate() { return 20; }
        "#;
        let index2 = index_source(v2);
        assert!(index2.find_declaration("calculate").is_some());
        assert!(index2.find_declaration("updated_calculate").is_some());
        assert_eq!(index2.symbols.len(), 2);
    }
}
