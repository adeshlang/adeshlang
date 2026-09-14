// End-to-end feature tests for the bytecode emitter + VM

#[cfg(test)]
mod tests {

    #[test]
    fn compile_disassemble_and_run_addition() {
        // Source contains two top-level lets and a print of their sum
        let src = r#"
let a = 12;
let b = 18;
print(a + b);
"#;
        let tmp = std::env::temp_dir().join("india-feature-add.bin");

        // compile to bytecode
        adeshlang::execution::bytecode::compile_to_file(src, &tmp).expect("compile failed");

        // disassemble (smoke; should not error)
        adeshlang::execution::bytecode::disassemble_file(&tmp).expect("disassemble failed");

        // run and capture output
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();

        // cleanup
        let _ = std::fs::remove_file(&tmp);

        // Expect 30 (12 + 18)
        assert!(output.contains('3'));
        assert!(output.contains('0') || output.contains("30"));
    }

    #[test]
    fn compile_and_run_uninitialized_let() {
        // let without initializer should be stored as null and printing it should
        // produce 'null' (our VM writes raw string constants or 'null')
        let src = r#"
let x;
print(x);
"#;
        let tmp = std::env::temp_dir().join("india-feature-null.bin");
        adeshlang::execution::bytecode::compile_to_file(src, &tmp).expect("compile failed");

        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);

        // The emitter stores uninitialized lets as the string "null", so expect that.
        assert!(output.contains("null"));
    }

    #[test]
    fn compile_v2_and_run_addition_print() {
        let src = r#"
let a = 12;
let b = 18;
print(a + b);
"#;
        let tmp = std::env::temp_dir().join("india-feature-add-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("30"));
    }

    #[test]
    fn compile_v2_and_run_arith_print() {
        let src = r#"
let a = 10;
let b = 2;
print(a - b);
print(a * b);
print(a / b);
"#;
        let tmp = std::env::temp_dir().join("india-feature-arith-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("8"));
        assert!(output.contains("20"));
        assert!(output.contains("5"));
    }

    #[test]
    fn compile_v2_and_run_if_else_print() {
        let src = r#"
let a = 0;
if a { print("no"); } else { print("yes"); }
"#;
        let tmp = std::env::temp_dir().join("india-feature-ifelse-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("yes"));
    }

    #[test]
    fn compile_v2_and_run_while_print() {
        let src = r#"
let n = 3;
while n { print(n); n = n - 1; }
"#;
        let tmp = std::env::temp_dir().join("india-feature-while-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("3"));
        assert!(output.contains("2"));
        assert!(output.contains("1"));
    }

    #[test]
    fn compile_v2_and_run_factorial_bigint() {
        let src = r#"
fn factorial(n) {
  if n <= 1 { return 1n; }
  return n * factorial(n - 1);
}
let result = factorial(10n);
print(result);
"#;
        let tmp = std::env::temp_dir().join("india-feature-factorial-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("3628800"));
    }

    #[test]
    fn compile_v2_and_run_basic_example() {
        let path = std::path::Path::new("examples/basic.ind");
        let src = std::fs::read_to_string(path).expect("read example failed");
        let tmp = std::env::temp_dir().join("india-feature-basic-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(&src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("Hello AdeshLang!"));
        assert!(output.contains("5"));
    }

    #[test]
    fn compile_v2_tail_recursion_sum_large() {
        let src = r#"
fn sum(n, acc) {
  if n <= 0 { return acc; }
  return sum(n - 1, acc + n);
}
let result = sum(20000, 0);
print(result);
"#;
        let tmp = std::env::temp_dir().join("india-feature-tco-sum-v2.bin");
        adeshlang::execution::bytecode::compile_to_file_v2(src, &tmp).expect("compile v2 failed");
        let mut out_buf: Vec<u8> = Vec::new();
        adeshlang::execution::vm::run_file_with_writer(&tmp, &mut out_buf).expect("run v2 failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        // 20000*20001/2 = 200010000
        assert!(output.contains("200010000"));
    }
}
