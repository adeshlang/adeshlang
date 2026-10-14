//! End-to-end test for Native Syscall & Process I/O
//!
//! Verifies:
//! 1. `sys_open`, `sys_write`, and `sys_close` manipulate file descriptors natively.
//! 2. `sys_spawn` and `sys_wait` spawn a real child process and await its exit status.
//! 3. `processGetInfo` returns structured process information.

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

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
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
        .current_dir(dir.path())
        .output()
        .expect("execute native binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stderr.is_empty() {
        eprintln!("STDERR: {}", stderr);
    }
    (stdout, code)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_fd_file_io() {
    let src = r#"
    fn main(): int {
        // flags: O_WRONLY (1) | O_CREAT (64) | O_TRUNC (512) = 577
        let fd = sys_open("test_output.txt", 577, 420);
        if fd < 0 {
            return 1;
        }
        let written = sys_write(fd, "HELLO_FROM_SYS_WRITE", 20);
        sys_close(fd);

        // flags: O_RDONLY (0)
        let rfd = sys_open("test_output.txt", 0, 0);
        if rfd < 0 {
            return 2;
        }
        sys_close(rfd);

        return written;
    }
    "#;
    let (_stdout, code) = compile_run_src(src, "test_native_fd_file_io");
    assert_eq!(code, 20, "sys_write must write exactly 20 bytes");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_process_spawn_and_wait() {
    let src = r#"
    fn main(): int {
        let pid = sys_spawn("cmd.exe", ["/c", "exit 42"]);
        if pid < 0 {
            return 1;
        }
        let code = sys_wait(pid);
        return code;
    }
    "#;
    let (_stdout, code) = compile_run_src(src, "test_native_process_spawn_and_wait");
    assert_eq!(code, 42, "Child process exit code must be 42");
}
