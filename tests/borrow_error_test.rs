use adeshlang::memory::raii::transform_ast_with_raii;
use adeshlang::parsing::compile_time_memory_safety::check_memory_safety_compile_time;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;

#[test]
fn test_borrow_checker_rejects_move_while_borrowed() {
    let src = r#"
        fn main() {
            let data = [1, 2, 3];
            let ref1 = &mut data;
            let ref2 = &mut data; // Conflicting borrow!
            print(ref1);
            print(ref2);
        }
    "#;

    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().expect("failed to tokenize");
    let mut parser = Parser::new(tokens, Some("test_borrow.adesh".to_string()));
    let ast = parser.parse_program().expect("failed to parse");
    let ast = transform_ast_with_raii(ast);
    let hir = ast_to_hir(&ast, false).expect("failed to lower to HIR");

    let safety_result = check_memory_safety_compile_time(&hir);

    assert!(
        safety_result.is_err(),
        "Expected borrow checker to reject code with borrow conflict violation"
    );

    let err_msg = safety_result.unwrap_err();
    assert!(
        err_msg.contains("cannot borrow") && err_msg.contains("mutable"),
        "Expected error message to mention borrow conflict, got: {}",
        err_msg
    );
}
