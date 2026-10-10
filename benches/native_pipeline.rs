//! Native compile/link scaling benchmarks.
//!
//! Build/check with `cargo check --bench native_pipeline`. CI runs short
//! measurements with `cargo bench --profile dev --bench native_pipeline` so
//! benchmark builds do not require the release profile.
#[allow(unused_imports)]
use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister, RegisterClass,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::register_alloc::{LinearScanAllocator, RegisterFile};
use adesh_codegen::targets::create_backend;
use adesh_codegen::targets::x86_64::X86_64RegisterFile;
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
    source.push_str("fn regalloc_pressure(): int { return 0; }\n");
    source.push_str("fn regalloc_call_pressure(): int { return 0; }\n");
    source.push_str("fn zero_arg(): int { return 0; }\n");
    source.push_str(
        "fn main(): int { return bench_0() + regalloc_pressure() + regalloc_call_pressure(); }\n",
    );
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

#[derive(Clone, Copy, Default)]
struct AllocatorMetrics {
    machine_instructions: usize,
    explicit_load_store_instructions: usize,
    spill_slots: usize,
    spill_bytes: u32,
    stack_frame_bytes: u64,
}

fn measure_allocator(
    module: &NativeModule,
    reg_file: &X86_64RegisterFile,
    split_calls: bool,
) -> AllocatorMetrics {
    let allocator = LinearScanAllocator::with_local_call_splitting(reg_file, split_calls);
    let mut metrics = AllocatorMetrics::default();
    for function in &module.functions {
        let mut allocated = function.clone();
        let result = allocator
            .allocate(&mut allocated)
            .expect("standalone allocation metrics should verify");
        metrics.spill_slots += result.spill_map.len();
        metrics.spill_bytes += result.total_spill_bytes;
        metrics.stack_frame_bytes += allocated.stack_size;
        for instruction in allocated
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
        {
            metrics.machine_instructions += 1;
            if matches!(
                instruction,
                adesh_codegen::machine_ir::MachineInstruction::Load { .. }
                    | adesh_codegen::machine_ir::MachineInstruction::Store { .. }
            ) {
                metrics.explicit_load_store_instructions += 1;
            }
        }
    }
    metrics
}

fn prepare_case(function_count: usize) -> NativeCase {
    let source = generated_source(function_count);
    let hir = parse_and_lower(&source);
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc")
        .expect("Windows x86-64 target is valid");
    let mut module = lower_hir_module(&hir, &target).expect("native lowering should succeed");
    let mut pressure = MachineFunction::new("regalloc_pressure");
    let values: Vec<_> = (0..40)
        .map(|_| pressure.alloc_vreg_with_class(RegisterClass::Gpr))
        .collect();
    let sum = pressure.alloc_vreg();
    {
        let block = pressure.entry_block_mut();
        for (index, vreg) in values.iter().enumerate() {
            block.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
                src: MachineOperand::Immediate(index as i64),
            });
        }
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(sum)),
            src: MachineOperand::Register(MachineRegister::Virtual(values[0])),
        });
        for vreg in values.iter().skip(1) {
            block.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(sum)),
                src: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
            });
        }
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            src: MachineOperand::Register(MachineRegister::Virtual(sum)),
        });
        block.push(MachineInstruction::Return);
    }
    let pressure_index = module
        .functions
        .iter()
        .position(|function| function.name == "regalloc_pressure")
        .expect("generated source has a pressure function");
    module.functions[pressure_index] = pressure;
    let reg_file = X86_64RegisterFile::for_os(target.operating_system);
    let mut call_pressure = MachineFunction::new("regalloc_call_pressure");
    let crossing_count = reg_file.callee_saved_for_class(RegisterClass::Gpr).len() + 2;
    let crossing_values: Vec<_> = (0..crossing_count)
        .map(|_| call_pressure.alloc_vreg_with_class(RegisterClass::Gpr))
        .collect();
    let sum = call_pressure.alloc_vreg();
    {
        let block = call_pressure.entry_block_mut();
        for (index, vreg) in crossing_values.iter().enumerate() {
            block.push(MachineInstruction::Move {
                dst: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
                src: MachineOperand::Immediate(index as i64),
            });
        }
        for _ in 0..16 {
            for vreg in &crossing_values {
                block.push(MachineInstruction::Add {
                    dst: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
                    src: MachineOperand::Immediate(1),
                });
            }
        }
        block.push(MachineInstruction::Call {
            target: MachineOperand::Symbol("zero_arg".to_string()),
            num_args: 0,
        });
        for _ in 0..16 {
            for vreg in &crossing_values {
                block.push(MachineInstruction::Add {
                    dst: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
                    src: MachineOperand::Immediate(1),
                });
            }
        }
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(sum)),
            src: MachineOperand::Register(MachineRegister::Virtual(crossing_values[0])),
        });
        for vreg in crossing_values.iter().skip(1) {
            block.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Virtual(sum)),
                src: MachineOperand::Register(MachineRegister::Virtual(*vreg)),
            });
        }
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister::gpr(0))),
            src: MachineOperand::Register(MachineRegister::Virtual(sum)),
        });
        block.push(MachineInstruction::Return);
    }
    let call_pressure_index = module
        .functions
        .iter()
        .position(|function| function.name == "regalloc_call_pressure")
        .expect("generated source has a call-pressure function");
    module.functions[call_pressure_index] = call_pressure;
    let mut backend = create_backend(target.clone()).expect("backend should be available");
    backend.set_opt_level(OptLevel::O2);
    let object = backend
        .emit_object(&module)
        .expect("native code generation should succeed");
    let object_bytes = AdobWriter::write(&object).expect("ADOB encoding should succeed");
    let allocated_module = backend
        .lower_module(&module)
        .expect("backend lowering and register allocation should succeed");
    let split_metrics = measure_allocator(&module, &reg_file, true);
    let fallback_metrics = measure_allocator(&module, &reg_file, false);
    let machine_instructions: usize = allocated_module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .map(|block| block.instructions.len())
        .sum();
    let explicit_load_store_instructions = allocated_module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction,
                adesh_codegen::machine_ir::MachineInstruction::Load { .. }
                    | adesh_codegen::machine_ir::MachineInstruction::Store { .. }
            )
        })
        .count();
    println!(
        "native_pipeline metrics: functions={function_count}, instructions={machine_instructions}, loads_stores={explicit_load_store_instructions}, split_spill_slots={}, split_spill_bytes={}, split_frame_bytes={}, split_instructions={}, split_loads_stores={}, fallback_spill_slots={}, fallback_spill_bytes={}, fallback_frame_bytes={}, fallback_instructions={}, fallback_loads_stores={}, object_bytes={}",
        split_metrics.spill_slots,
        split_metrics.spill_bytes,
        split_metrics.stack_frame_bytes,
        split_metrics.machine_instructions,
        split_metrics.explicit_load_store_instructions,
        fallback_metrics.spill_slots,
        fallback_metrics.spill_bytes,
        fallback_metrics.stack_frame_bytes,
        fallback_metrics.machine_instructions,
        fallback_metrics.explicit_load_store_instructions,
        object_bytes.len()
    );
    let metrics_dir = std::env::current_dir()
        .expect("benchmark working directory")
        .join("target/criterion/native_pipeline/allocator_metrics");
    fs::create_dir_all(&metrics_dir).expect("create allocator metrics directory");
    fs::write(
        metrics_dir.join(format!("{function_count}.json")),
        format!(
        "{{\"functions\":{function_count},\"enabled\":{{\"machine_instructions\":{},\"explicit_load_store_instructions\":{},\"spill_slots\":{},\"spill_bytes\":{},\"stack_frame_bytes\":{}}},\"fallback\":{{\"machine_instructions\":{},\"explicit_load_store_instructions\":{},\"spill_slots\":{},\"spill_bytes\":{},\"stack_frame_bytes\":{}}},\"object_bytes\":{}}}\n",
            split_metrics.machine_instructions,
            split_metrics.explicit_load_store_instructions,
            split_metrics.spill_slots,
            split_metrics.spill_bytes,
            split_metrics.stack_frame_bytes,
            fallback_metrics.machine_instructions,
            fallback_metrics.explicit_load_store_instructions,
            fallback_metrics.spill_slots,
            fallback_metrics.spill_bytes,
            fallback_metrics.stack_frame_bytes,
            object_bytes.len(),
        ),
    )
    .expect("write allocator metrics artifact");
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
