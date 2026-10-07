//! Native-pipeline semantics regression suite:
//! source text -> HIR -> lowering -> x86-64 codegen -> ADOB -> adeshlink PE
//! link (auto-links adesh_runtime.lib) -> execute and compare stdout / exit
//! code.
//!
//! Each case guards a specific miscompile, crash, or missing-runtime-symbol
//! bug fixed in `src/backends/native/lower.rs` and
//! `crates/adesh-runtime/src/native_abi.rs`. Windows x86-64 only, mirroring
//! the other native e2e suites. Requires `adesh_runtime.lib` in the Cargo
//! target directory (built by `cargo build -p adesh-runtime`).

#![allow(dead_code, unused_imports)]

use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::process::Command;
use tempfile::tempdir;

/// Compile source text through the full native pipeline, link, run, and
/// return (stdout, exit_code).
fn compile_run_src(src: &str, test_name: &str) -> (String, i32) {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{test_name}.adob"));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{test_name}.exe"));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    (stdout, code)
}

/// Lower source text and return the native lowering diagnostics (empty if
/// lowering succeeded).
fn lowering_diagnostics(src: &str) -> Vec<String> {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    match lower_hir_module(&hir, &target) {
        Ok(_) => Vec::new(),
        Err(e) => e.diagnostics,
    }
}

/// Imports and `region` blocks used to be dropped silently (the import did
/// nothing; the region lost its bulk free). Both must now be compile errors.
#[test]
fn test_native_rejects_imports_and_regions() {
    let diags = lowering_diagnostics(
        r#"
        import "lib/util.adesh" as util;
        print("x");
        "#,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.contains("import of \"lib/util.adesh\"")),
        "{diags:?}"
    );

    let diags = lowering_diagnostics(
        r#"
        region Scratch {
            let a = 1;
        }
        "#,
    );
    assert!(
        diags.iter().any(|d| d.contains("region Scratch")),
        "{diags:?}"
    );

    assert!(lowering_diagnostics("print(1 + 2);").is_empty());
}

/// Enum variant patterns used to always match and bind nothing; async code
/// ran synchronously; lambdas silently lost captured variables. Each must now
/// be a compile error until the backend implements it.
#[test]
fn test_native_rejects_async() {
    let diags = lowering_diagnostics(
        r#"
        async fn work(): int { return 1; }
        fn main() {
            let x = await work();
            print(x);
        }
        "#,
    );
    assert!(diags.iter().any(|d| d.contains("`async fn`")), "{diags:?}");
    assert!(diags.iter().any(|d| d.contains("`await`")), "{diags:?}");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_capturing_lambda_runs() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let base = 10;
            let add = fn(a) { return a + base; };
            print(add(1));
        }
        "#,
        "sem_lambda_capture",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "11\n");
}

/// Lambdas that only use their own parameters need no environment and must
/// keep compiling and running.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_non_capturing_lambda_runs() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let add = fn(a, b) { return a + b; };
            print(add(2, 3));
        }
        "#,
        "sem_lambda",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "5\n");
}

/// Integer-literal matches with 4+ cases go through `SwitchLowering`
/// (binary search tree for sparse values, compare chain for dense ones).
/// Two sparse matches in one function share pivot values, which used to
/// produce duplicate BST block labels.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_int_match_switch_lowering() {
    let (stdout, code) = compile_run_src(
        r#"
        fn sparse(v: int): int {
            return match v {
                1 => 10,
                100 => 20,
                1000 => 30,
                50000 => 40,
                70000 => 50,
                _ => 99,
            };
        }
        fn sparse_again(v: int): int {
            return match v {
                1 => 11,
                100 => 21,
                1000 => 31,
                50000 => 41,
                70000 => 51,
                _ => 98,
            };
        }
        fn dense(v: int): int {
            return match v {
                0 => 5,
                1 => 6,
                2 => 6,
                3 => 7,
                4 => 8,
                5 => 9,
                other => other * 2,
            };
        }
        fn main() {
            print(sparse(1));
            print(sparse(1000));
            print(sparse(70000));
            print(sparse(7));
            print(sparse(-3));
            print(sparse_again(100));
            print(sparse_again(50000));
            print(sparse_again(2));
            print(dense(0));
            print(dense(2));
            print(dense(5));
            print(dense(21));
        }
        "#,
        "sem_switch",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "10\n30\n50\n99\n99\n21\n41\n98\n5\n6\n9\n42\n");
}

/// Each `print` stores its argument handles in an RBP-relative array. The
/// frame optimizer only counted `StackSlot` operands, shrank the frame below
/// those arrays, and calls overwrote them: from the fifth `print` on, garbage
/// addresses were printed.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_many_prints_keep_frame() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            print(1); print(2); print(3); print(4);
            print(5); print(6); print(7); print(8);
        }
        "#,
        "sem_many_prints",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "1\n2\n3\n4\n5\n6\n7\n8\n");
}

/// `print(70000 + 5)`: large immediate arithmetic used to crash with
/// 0xC0000005 (stack-slot handle misuse); the value must print raw.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_large_int_print() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            print(70000 + 5);
        }
        "#,
        "sem_bigint",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "70005\n");
}

/// Integer division by zero must abort loudly via the runtime guard, not
/// fault the process with 0xC0000094.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_div_by_zero_aborts() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let d = 0;
            print(1 / d);
            print("unreachable");
        }
        "#,
        "sem_divzero",
    );
    assert_eq!(code, 101);
    assert!(
        stdout.contains("division by zero"),
        "stdout was: {stdout:?}"
    );
    assert!(!stdout.contains("unreachable"));
}

/// Right shift of a negative value is arithmetic (`-8 >> 1 == -4`), not
/// logical.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_signed_shift_right() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            print(-8 >> 1);
        }
        "#,
        "sem_shift",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "-4\n");
}

/// Defers: LIFO within a scope, block-exit scoped, flushed on `break`, and
/// run on fall-through function exit.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_defers_scoped_lifo_break_fallthrough() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            print("start");
            defer { print("cleanup-main"); }
            {
                defer { print("cleanup-block"); }
                print("in-block");
            }
            for i in range(0, 2) {
                defer { print("cleanup-iter"); }
                if i == 1 {
                    print("breaking");
                    break;
                }
                print("iter");
            }
            print("end");
        }
        "#,
        "sem_defer",
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "start\nin-block\ncleanup-block\niter\ncleanup-iter\nbreaking\ncleanup-iter\nend\ncleanup-main\n"
    );
}

/// Deferred calls run on return after the return expression has been
/// evaluated, preserving its value across the cleanup call.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_defer_runs_before_return() {
    let (stdout, code) = compile_run_src(
        r#"
        fn value() -> int {
            defer { print("cleanup"); }
            return 42;
        }
        fn main() {
            print(value());
        }
        "#,
        "sem_defer_return",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "cleanup\n42\n");
}

/// `&&`/`||` short-circuit: the right operand (and its side effects) must
/// not execute when the left operand already decided the result.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_logical_short_circuit() {
    let (stdout, code) = compile_run_src(
        r#"
        fn noisy(x: int) -> int {
            print("eval");
            return x;
        }
        fn main() {
            let a = noisy(0) && noisy(7);
            print(a);
            let b = noisy(6) || noisy(9);
            print(b);
            let c = noisy(3) && noisy(8);
            print(c);
            let d = noisy(0) || noisy(9);
            print(d);
        }
        "#,
        "sem_shortcirc",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "eval\n0\neval\n6\neval\neval\n8\neval\neval\n9\n");
}

/// Block-scoped shadowing: an inner `let` shadows the outer variable, and
/// the outer binding is restored when the block exits.
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_shadowing_restores_outer() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let x = 1;
            if x == 1 {
                let x = 2;
                print(x);
            }
            print(x);
        }
        "#,
        "sem_shadow",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "2\n1\n");
}

/// `**` integer power (previously compiled to 2 for 2**10).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_pow() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            print(2 ** 10);
        }
        "#,
        "sem_pow",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "1024\n");
}

/// `range(...)` materializes an iterable array (previously a missing
/// runtime symbol: link failure).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_range_iteration() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            for i in range(1, 5) {
                print(i);
            }
        }
        "#,
        "sem_range",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "1\n2\n3\n4\n");
}

/// String `+` concatenation (previously a missing runtime symbol).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_string_concat() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let s = "hello" + " " + "world";
            print(s);
        }
        "#,
        "sem_strcat",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "hello world\n");
}

/// `in` membership over arrays (previously a missing runtime symbol).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_in_membership() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let a = [1, 2, 3];
            print(2 in a);
            print(5 in a);
        }
        "#,
        "sem_in",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "true\nfalse\n");
}

/// Float arithmetic through variables, float args/returns (XMM0), and the
/// f64 math dispatcher (previously floats through variables compiled to
/// integer ops).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_float_vars_and_math() {
    let (stdout, code) = compile_run_src(
        r#"
        fn half(x: float) -> float {
            return x / 2.0;
        }
        fn main() {
            let f = 1.5;
            let g = f * 2.0;
            print(g);
            print(half(g));
            print(sqrt(2.0));
        }
        "#,
        "sem_float",
    );
    assert_eq!(code, 0, "stdout before native process exit: {stdout:?}");
    assert!(stdout.contains("3"), "stdout was: {stdout:?}");
    assert!(stdout.contains("1.5"), "stdout was: {stdout:?}");
    assert!(stdout.contains("1.414"), "stdout was: {stdout:?}");
}

/// `clock()` returns epoch seconds as f64 in XMM0 (previously bound to the
/// C library `clock`, wrong semantics entirely).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_clock_epoch() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let t = clock();
            print(t > 1000000000.0);
        }
        "#,
        "sem_clock",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "true\n");
}

/// try/catch: a thrown value routes control to the catch handler (the
/// catch block used to be silently discarded by the backend).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_try_catch_throw() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            try {
                print("try");
                defer { print("cleanup-try"); }
                throw "boom";
                print("unreachable");
            } catch(e) {
                print("caught");
            }
            print("after");
        }
        "#,
        "sem_trycatch",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "try\ncleanup-try\ncaught\nafter\n");
}

/// `arr[i]++` loads, increments, and stores back through the runtime index
/// accessors (previously silently compiled to 0).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_index_increment() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let a = [10, 20, 30];
            a[1]++;
            print(a[1]);
        }
        "#,
        "sem_arrinc",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "21\n");
}

/// `aot_call_method` results must be unboxed: `a.len()` returns a raw int,
/// not a boxed handle (previously the raw handle pointer leaked through).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_method_result_unboxed() {
    let (stdout, code) = compile_run_src(
        r#"
        fn main() {
            let a = [1, 2, 3];
            a.append(4);
            print(a.len());
        }
        "#,
        "sem_unbox",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "4\n");
}

/// A `struct` declaration alongside `fn main` must build (previously both
/// lowered into separate functions named `main` → duplicate symbol at ADOB
/// encode time).
#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_struct_with_fn_main_builds() {
    let (stdout, code) = compile_run_src(
        r#"
        struct Point {
            x: int,
            y: int,
        }
        fn main() {
            print("built");
        }
        "#,
        "sem_struct",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "built\n");
}
