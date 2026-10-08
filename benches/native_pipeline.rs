//! Native compile/link scaling benchmarks.
//!
//! Build/check with `cargo check --bench native_pipeline`. Run with
//! `cargo bench --bench native_pipeline` only in an environment where release
//! benchmark builds are permitted.

use adesh_codegen::CodegenBackend;
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use adeshlang::typesystem::type_system::check_module;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

struct NativeCase {
    target: TargetDescriptor,
    module: adesh_codegen::machine_ir::NativeModule,
    allocated_module: adesh_codegen::machine_ir::NativeModule,
    object_bytes: Vec<u8>,
    dir: TempDir,
}

fn generated_source(function_count: usize) -> String {
    let mut source = String::with_capacity(function_count * 42);
    for index in 0..function_count {
        source.push_str(&format!("fn bench_{index}(): int {{ return {index}; }}\n"));
    }
    source.push_str("fn main(): int { return bench_0(); }\n");
    source
}

fn parse_and_lower(source: &str) -> adeshlang::ir::hir::HirModule {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("benchmark source should lex");
    let mut parser = Parser::new(tokens, None);
    let program = parser
        .parse_program()
        .expect("benchmark source should parse");
    ast_to_hir(&program, false).expect("benchmark source should lower to HIR")
}

fn prepare_case(function_count: usize) -> NativeCase {
    let source = generated_source(function_count);
    let hir = parse_and_lower(&source);
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc")
        .expect("Windows x86-64 target is valid");
    let module = lower_hir_module(&hir, &target).expect("native lowering should succeed");
    let mut backend = create_backend(target.clone()).expect("backend should be available");
    backend.set_opt_level(OptLevel::O2);
    let object = backend
        .emit_object(&module)
        .expect("native code generation should succeed");
    let object_bytes = AdobWriter::write(&object).expect("ADOB encoding should succeed");
    let allocated_module = backend
        .lower_module(&module)
        .expect("backend lowering and register allocation should succeed");
    NativeCase {
        target,
        module,
        allocated_module,
        object_bytes,
        dir: tempfile::tempdir().expect("temporary benchmark directory"),
    }
}

fn bench_native_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("native_pipeline");
    for function_count in [10usize, 100, 1_000, 10_000] {
        let source = generated_source(function_count);
        let hir = parse_and_lower(&source);
        let case = prepare_case(function_count);
        let object_path = case.dir.path().join("case.adob");
        let output_path = case.dir.path().join("case.exe");
        fs::write(&object_path, &case.object_bytes).expect("write benchmark ADOB");

        group.bench_function(BenchmarkId::new("parse_hir", function_count), |b| {
            b.iter(|| black_box(parse_and_lower(black_box(&source))))
        });

        group.bench_function(BenchmarkId::new("typecheck", function_count), |b| {
            b.iter(|| {
                black_box(
                    check_module(black_box(&source)).expect("benchmark source should typecheck"),
                )
            })
        });

        group.bench_function(BenchmarkId::new("native_lower", function_count), |b| {
            b.iter(|| {
                black_box(
                    lower_hir_module(black_box(&hir), &case.target)
                        .expect("native lowering should succeed"),
                )
            })
        });

        group.bench_function(BenchmarkId::new("regalloc", function_count), |b| {
            b.iter(|| {
                let mut backend =
                    create_backend(case.target.clone()).expect("backend should be available");
                backend.set_opt_level(OptLevel::O2);
                black_box(
                    backend
                        .lower_module(black_box(&case.module))
                        .expect("backend lowering and register allocation should succeed"),
                )
            })
        });

        group.bench_function(BenchmarkId::new("encode", function_count), |b| {
            b.iter(|| {
                let mut backend =
                    create_backend(case.target.clone()).expect("backend should be available");
                let total_bytes: usize = case
                    .allocated_module
                    .functions
                    .iter()
                    .map(|func| {
                        backend
                            .generate_function(black_box(func))
                            .expect("function encoding should succeed")
                            .len()
                    })
                    .sum();
                black_box(total_bytes)
            })
        });

        group.bench_function(BenchmarkId::new("codegen_adob", function_count), |b| {
            b.iter(|| {
                let mut backend =
                    create_backend(case.target.clone()).expect("backend should be available");
                backend.set_opt_level(OptLevel::O2);
                black_box(
                    backend
                        .emit_object(black_box(&case.module))
                        .expect("native code generation should succeed"),
                )
            })
        });

        group.bench_function(BenchmarkId::new("link", function_count), |b| {
            b.iter(|| {
                adesh_linker::link(
                    &[PathBuf::from(&object_path)],
                    &output_path,
                    Some("x86_64-pc-windows-msvc"),
                )
                .expect("native linking should succeed");
                black_box(
                    fs::metadata(&output_path)
                        .expect("linked output exists")
                        .len(),
                )
            })
        });

        if cfg!(windows) && function_count == 10 {
            for (label, level) in [
                ("o0", OptLevel::O0),
                ("o1", OptLevel::O1),
                ("o2", OptLevel::O2),
                ("o3", OptLevel::O3),
            ] {
                let runtime_path = case.dir.path().join(format!("runtime_{label}.exe"));
                let mut backend =
                    create_backend(case.target.clone()).expect("backend should be available");
                backend.set_opt_level(level);
                let runtime_object = backend
                    .emit_object(&case.module)
                    .expect("runtime case code generation should succeed");
                let runtime_object_path = case.dir.path().join(format!("runtime_{label}.adob"));
                fs::write(
                    &runtime_object_path,
                    AdobWriter::write(&runtime_object).expect("ADOB encoding should succeed"),
                )
                .expect("write runtime benchmark object");
                adesh_linker::link(
                    &[runtime_object_path],
                    &runtime_path,
                    Some("x86_64-pc-windows-msvc"),
                )
                .expect("runtime benchmark linking should succeed");

                group.bench_function(
                    BenchmarkId::new(format!("runtime_{label}"), function_count),
                    |b| {
                        b.iter(|| {
                            let result = Command::new(&runtime_path)
                                .output()
                                .expect("benchmark executable should start");
                            black_box(result.status.code())
                        })
                    },
                );
            }
        }
    }
    group.finish();
}

criterion_group!(benches, bench_native_pipeline);
criterion_main!(benches);
