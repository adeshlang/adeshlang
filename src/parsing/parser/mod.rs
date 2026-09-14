//! Parser module
//!
//! This module is organized into several submodules for maintainability:
//! - core: Parser struct and utility methods
//! - declarations: Top-level declarations (import, let/const, etc.)
//! - extern_ffi: FFI and extern declarations
//! - functions: Function and decorator declarations
//! - classes: Class declarations with methods and fields
//! - type_decls: Type-related declarations (extend, interface, struct, enum, type alias)
//! - statements: Statement parsing (if, while, for, return, try-catch, etc.)
//! - expressions: Expression entry and binary operators
//! - expressions_unary: Unary operators and ranges
//! - expressions_primary: Primary expressions (literals, identifiers, etc.)
//! - literals: Template literals and match expressions
//! - type_annotations: Type annotation parsing

mod classes;
mod core;
mod declarations;
mod expressions;
mod expressions_primary;
mod expressions_unary;
mod extern_ffi;
mod functions;
mod literals;
mod statements;
mod type_annotations;
mod type_decls;

// Re-export the main types
pub use core::Parser;

// Helper function for deriving import aliases
pub fn derive_alias(p: &str) -> String {
    // Handle namespace:module format like "std:math" -> "math"
    if p.contains(':') {
        // Extract the last part after the colon
        let parts: Vec<&str> = p.split(':').collect();
        if let Some(last) = parts.last() {
            return last.to_string();
        }
    }

    let s = p.replace("\\", "/");
    let parts: Vec<&str> = s.split('/').collect();
    let last_ref = if let Some(last) = parts.last() {
        *last
    } else {
        s.as_str()
    };
    let stem = if let Some(idx) = last_ref.rfind('.') {
        &last_ref[..idx]
    } else {
        last_ref
    };
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ast::{ExprKind, StmtKind, Value};
    use crate::parsing::lexer::Lexer;

    #[test]
    fn parse_factorial_example() {
        let src = r#"
fn factorial(n) {
    if (n <= 1) {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}

let result = factorial(5);
print("Factorial of 5 =", result);
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        // should contain at least one function stmt
        assert!(
            prog.iter()
                .any(|s| matches!(s.kind, StmtKind::Function(_, _)))
        );
    }

    #[test]
    fn parse_class_and_import_examples() {
        let src = r#"
class Person {
    fn Person(name) { this.name = name; }
    fn greet() { print("Hello", this.name); }
}

import utils as math;
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        assert!(prog.iter().any(|s| matches!(s.kind, StmtKind::Class(_, _))));
        assert!(
            prog.iter()
                .any(|s| matches!(s.kind, StmtKind::Import { .. }))
        );
    }

    #[test]
    fn parse_extend_examples() {
        let src = r#"
extend on Person {
    fn salute() { print("Salute " + this.name); }
}

extend HelloExt on Person {
    fn hello() { print("Hello " + this.name); }
}
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        assert!(
            prog.iter()
                .any(|s| matches!(s.kind, StmtKind::Extend(_, _, _, _)))
        );
    }

    #[test]
    fn parse_arrow_examples() {
        let src = r#"
let inc = x => x + 1;
let add = (a, b = 2) => a + b;
let greet = (name) => { return "Hi " + name; };
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        // should contain lets with function literals as initializers
        assert!(prog.iter().any(|s| matches!(
            s.kind,
            StmtKind::Let(
                _,
                Some(crate::parsing::ast::Expr {
                    kind: ExprKind::Fn(_, _, _),
                    ..
                }),
                _,
                _,
                _,
                _
            )
        )));
    }

    #[test]
    fn diagnostic_includes_file_line_col_and_snippet() {
        use regex::Regex;
        let src = "fn main() {\n  let x = ;\n}\n";
        let mut lx = crate::parsing::lexer::Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, Some("examples/test_file.adesh".to_string()));
        let res = p.parse_program();
        assert!(res.is_err());
        let e = res.err().unwrap();
        let out = e.to_string();
        let re = Regex::new("\\x1B\\[[0-9;]*m").unwrap();
        let clean = re.replace_all(&out, "");
        let s = clean.to_string();
        assert!(s.contains("examples/test_file.adesh:2:"));
        assert!(s.contains("Expect expression"));
        assert!(s.contains("let x = ;"));
        assert!(s.contains("^"));
    }

    #[test]
    fn parse_ternary_and_bitwise() {
        let src = r#"let a = 1; let b = 2; let x = a ? b : 3; let y = 1 << 2; let z = 1 & 3;"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        assert!(prog.len() >= 4);
    }

    #[test]
    fn parse_typed_numeric_literals() {
        let src = r#"
let a = 10u8;
let b = 255u16;
let c = 1000i32;
let d = 3.14f32;
let e = 2.718f64;
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        // Should contain 5 let statements
        assert_eq!(prog.len(), 5);

        // Verify the literals are parsed correctly
        for stmt in &prog {
            if let StmtKind::Let(_name, Some(init), _, _, _, _) = &stmt.kind {
                if let ExprKind::Literal(val) = &init.kind {
                    // All should be typed numeric literals
                    match val {
                        Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::I32(_)
                        | Value::I64(_)
                        | Value::F32(_)
                        | Value::F64(_)
                        | Value::Number(_) => {}
                        _ => panic!("Expected typed numeric literal"),
                    }
                }
            }
        }
    }

    #[test]
    fn parse_unsuffixed_integer_defaults_to_i32() {
        let src = r#"let x = 10;"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        assert_eq!(prog.len(), 1);

        if let StmtKind::Let(_name, Some(init), _, _, _, _) = &prog[0].kind {
            if let ExprKind::Literal(val) = &init.kind {
                assert!(
                    matches!(val, Value::Number(n) if (*n - 10.0).abs() < f64::EPSILON),
                    "expected neutral numeric literal, got {:?}",
                    val
                );
            } else {
                panic!("Expected literal initializer");
            }
        } else {
            panic!("Expected let statement");
        }
    }

    #[test]
    fn parse_typed_let_with_annotation() {
        let src = r#"
let a: u8 = 42;
let b: i32 = -100;
let c: f32 = 3.14;
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");
        // Should contain 3 let statements with type annotations
        assert_eq!(prog.len(), 3);

        // Check that type annotations are preserved
        for stmt in &prog {
            if let StmtKind::Let(_name, _, type_ann, _, _, _) = &stmt.kind {
                assert!(type_ann.is_some(), "Expected type annotation");
            }
        }
    }

    #[test]
    fn parse_array_type_annotations() {
        let src = r#"
let a: [u8] = [1, 2, 3];
let b: [u8;4] = [1, 2, 3, 4];
let c: [u8;raw] = [1, 2, 3];
let d: [u8;4;raw] = [1, 2, 3, 4];
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        // Should contain 4 let statements
        assert_eq!(prog.len(), 4);

        // Check type annotations
        let expected_types = ["[u8]", "[u8;4]", "[u8;raw]", "[u8;4;raw]"];
        for (i, stmt) in prog.iter().enumerate() {
            if let StmtKind::Let(_, _, Some(type_ann), _, _, _) = &stmt.kind {
                assert_eq!(type_ann, expected_types[i], "Mismatch at index {}", i);
            } else {
                panic!("Expected Let statement with type annotation at index {}", i);
            }
        }
    }

    #[test]
    fn parse_slice_type_annotation() {
        let src = r#"
let slice: &[u8] = arr[0..10];
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        assert_eq!(prog.len(), 1);
        if let StmtKind::Let(_, _, Some(type_ann), _, _, _) = &prog[0].kind {
            assert_eq!(type_ann, "&[u8]");
        } else {
            panic!("Expected Let statement with slice type annotation");
        }
    }

    #[test]
    fn parse_readonly_declarations() {
        let src = r#"
readonly let a = 10;
readonly let arr = [1, 2, 3];
let b: [u8] readonly = [1, 2, 3];
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        // Should contain 3 let statements, all readonly
        assert_eq!(prog.len(), 3);
        for stmt in &prog {
            if let StmtKind::Let(_, _, _, _, _, is_readonly) = &stmt.kind {
                assert!(*is_readonly, "Expected readonly declaration");
            } else {
                panic!("Expected Let statement");
            }
        }
    }

    #[test]
    fn parse_pointer_type_annotation() {
        let src = r#"
let ptr: *u8 = null;
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        assert_eq!(prog.len(), 1);
        if let StmtKind::Let(_, _, Some(type_ann), _, _, _) = &prog[0].kind {
            assert_eq!(type_ann, "*u8");
        } else {
            panic!("Expected Let statement with pointer type annotation");
        }
    }

    #[test]
    fn parse_vec_type_annotation() {
        // Note: vec is a keyword, followed by a number and generic
        let src = r#"
let v: vec4<f32> = [1.0, 2.0, 3.0, 4.0];
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        assert_eq!(prog.len(), 1);
        if let StmtKind::Let(_, _, Some(type_ann), _, _, _) = &prog[0].kind {
            assert_eq!(type_ann, "vec4<f32>");
        } else {
            panic!("Expected Let statement with vec type annotation");
        }
    }

    #[test]
    fn parse_optional_type_annotation() {
        let src = r#"
let maybe: u8? = null;
"#;
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("lex failed");
        let mut p = Parser::new(toks, None);
        let prog = p.parse_program().expect("parse failed");

        assert_eq!(prog.len(), 1);
        if let StmtKind::Let(_, _, Some(type_ann), _, _, _) = &prog[0].kind {
            assert_eq!(type_ann, "u8?");
        } else {
            panic!("Expected Let statement with optional type annotation");
        }
    }
}
