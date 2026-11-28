//! Phase 19.8 — Runtime boundary hardening integration tests:
//! pointer provenance, checked allocation safety, handle representation &
//! stale/negative-word isolation, and runtime type/enum tag safety.

use adesh_codegen::targets::create_backend;
use adesh_linker::{LinkConfig, Target};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::path::Path;
use std::process::Command;

fn can_execute_elf() -> bool {
    cfg!(target_os = "linux")
}

fn execute_elf(path: &Path) -> std::io::Result<std::process::Output> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        Command::new(path).output()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ELF execution requires Linux",
        ))
    }
}

fn compile_link_and_run_linux(src: &str, test_name: &str) -> std::process::Output {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");
    let mut backend = create_backend(target).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("phase19_8_tests")
        .join(test_name);
    let _ = std::fs::remove_dir_all(&test_dir);
    std::fs::create_dir_all(&test_dir).expect("mkdir");

    let obj_path = test_dir.join("main.adob");
    let exe_path = test_dir.join("main.elf");
    std::fs::write(&obj_path, bytes).expect("write adob");

    let link_target = Target::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut cfg = LinkConfig::new(exe_path.clone(), link_target);
    cfg.dynamic = true;

    adesh_linker::Linker::link(&[obj_path], cfg).expect("linking must succeed");
    assert!(exe_path.exists(), "linked ELF missing for {test_name}");

    execute_elf(&exe_path).expect("execute ELF")
}

#[test]
fn native_negative_integers_and_dynamic_collections_do_not_collide_with_handle_tags() {
    if !can_execute_elf() {
        return;
    }

    let src = r#"
fn main() {
    let neg1 = -1;
    let neg42 = -42;
    let min_ish = -1000000000;
    let arr = [neg1, neg42, min_ish, 7];
    println(arr[0]);
    println(arr[1]);
    println(arr[2]);
    println(arr[3]);
    println(neg1 + neg42);
}
"#;
    let out = compile_link_and_run_linux(src, "neg_ints_vs_handles");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["-1", "-42", "-1000000000", "7", "-43"]);
}

#[test]
fn native_enum_variant_tags_and_payloads_roundtrip_cleanly() {
    if !can_execute_elf() {
        return;
    }

    let src = r#"
enum Shape {
    Circle(int),
    Square(int),
    Point,
}

fn area(s: Shape): int {
    return match s {
        Shape::Circle(r) => r * r * 3,
        Shape::Square(side) => side * side,
        Shape::Point => 1,
    };
}

fn main() {
    let a = area(Shape::Circle(4));
    let b = area(Shape::Square(5));
    let c = area(Shape::Point);
    println(a);
    println(b);
    println(c);
}
"#;
    let out = compile_link_and_run_linux(src, "enum_tags_and_payloads");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["48", "25", "1"]);
}

#[test]
fn runtime_boundary_api_provenance_and_bounds_checks() {
    use adesh_runtime::{
        HANDLE_TAG, RuntimeValue, abi, adesh_rt_alloc_tracked, adesh_rt_check_bounds,
        adesh_rt_enum_check_tag, adesh_rt_free_tracked, adesh_rt_is_valid_handle,
        aot_free_handle, aot_get_value, aot_store_value, aot_unbox, aot_update_value,
        is_handle_tagged, native_abi,
    };

    // Negative words never appear handle-tagged.
    assert!(!is_handle_tagged((-1_i64) as u64));
    assert!(!is_handle_tagged(0));

    // Stale handle cannot be resurrected or unboxed to tag bits.
    let h = aot_store_value(RuntimeValue::Int(88));
    assert_eq!(adesh_rt_is_valid_handle(h), 1);
    aot_free_handle(h);
    assert_eq!(adesh_rt_is_valid_handle(h), 0);
    aot_update_value(h, RuntimeValue::Int(99));
    assert_eq!(aot_get_value(h), None);
    assert_eq!(unsafe { aot_unbox(h) }, 0);
    assert_eq!(unsafe { aot_unbox(HANDLE_TAG | 999_999) }, 0);

    // Cross-allocator and double-free safety.
    let p_aot = native_abi::aot_alloc(16);
    assert!(!p_aot.is_null());
    assert_eq!(adesh_rt_check_bounds(p_aot as i64, 0, 16), 1);
    assert_eq!(adesh_rt_check_bounds(p_aot as i64, 12, 8), 0);
    abi::adesh_mem_free(p_aot as *mut u8, 16, 8); // ignored
    assert_eq!(adesh_rt_check_bounds(p_aot as i64, 0, 16), 1);
    native_abi::aot_free(p_aot);
    native_abi::aot_free(p_aot);
    assert_eq!(adesh_rt_check_bounds(p_aot as i64, 0, 1), 0);

    let p_tracked = adesh_rt_alloc_tracked(16, 0);
    assert_ne!(p_tracked, 0);
    assert_eq!(adesh_rt_check_bounds(p_tracked, 8, 8), 1);
    assert_eq!(adesh_rt_free_tracked(p_tracked, 0), 1);

    // Enum tag validation rejects garbage pointers safely.
    assert_eq!(unsafe { adesh_rt_enum_check_tag(std::ptr::null(), 3) }, -1);
    assert_eq!(
        unsafe { adesh_rt_enum_check_tag(0x20000 as *const i64, 3) },
        -1
    );
}
