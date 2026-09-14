// Comprehensive benchmarks for AdeshLang
// Run with: cargo bench

use criterion::{Criterion, black_box, criterion_group, criterion_main};

use adeshlang::parsing::ast::Value;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::hir_passes::{fold_module_constants, run_phase3_passes, run_safety_passes};
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use adeshlang::parsing::unified_safety_pass::UnifiedSafetyPass;
use adeshlang::utils::collections::FastMap;
use adeshlang::utils::interner::intern;

// ============================================
// Value operations
// ============================================

fn bench_value_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_clone");

    // Number (inline, should be very cheap)
    let num = Value::Number(42.0);
    group.bench_function("number", |b| b.iter(|| black_box(num.clone())));

    // Bool (inline)
    let boolean = Value::Bool(true);
    group.bench_function("bool", |b| b.iter(|| black_box(boolean.clone())));

    // String (heap-allocated, clone copies the String)
    let s = Value::Str("hello world this is a longer string".to_string());
    group.bench_function("string", |b| b.iter(|| black_box(s.clone())));

    // Small array (10 elements)
    let arr_small = Value::Array(vec![Value::Number(1.0); 10]);
    group.bench_function("array_10", |b| b.iter(|| black_box(arr_small.clone())));

    // Large array (1000 elements)
    let arr_large = Value::Array(vec![Value::Number(1.0); 1000]);
    group.bench_function("array_1000", |b| b.iter(|| black_box(arr_large.clone())));

    // Object with 10 properties (Arc clone, should be cheap)
    let mut obj = FastMap::default();
    for i in 0..10 {
        obj.insert(format!("key{}", i), Value::Number(i as f64));
    }
    let obj_val = Value::Object(std::sync::Arc::new(obj));
    group.bench_function("object_10_props", |b| b.iter(|| black_box(obj_val.clone())));

    group.finish();
}

// ============================================
// String interning
// ============================================

fn bench_string_interning(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interning");

    // Cache miss (unique string each iteration)
    group.bench_function("intern_miss", |b| {
        let counter = std::sync::atomic::AtomicUsize::new(0);
        b.iter(|| {
            let val = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            black_box(intern(&format!("unique_{}", val)))
        })
    });

    // Cache hit (same string repeatedly)
    intern("cached_string_benchmark");
    group.bench_function("intern_hit", |b| {
        b.iter(|| black_box(intern("cached_string_benchmark")))
    });

    // Interned string equality (pointer comparison)
    let s1 = intern("equality_test");
    let s2 = intern("equality_test");
    group.bench_function("interned_equality", |b| {
        b.iter(|| black_box(std::sync::Arc::ptr_eq(&s1, &s2)))
    });

    group.finish();
}

// ============================================
// HashMap comparison
// ============================================

fn bench_hashmap_comparison(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("hashmap_comparison");

    // Standard HashMap (SipHash)
    let mut std_map: HashMap<String, i32> = HashMap::new();
    for i in 0..1000 {
        std_map.insert(format!("key{}", i), i);
    }
    group.bench_function("std_hashmap_lookup", |b| {
        b.iter(|| black_box(std_map.get("key500")))
    });

    // FastMap (FxHashMap - faster hash)
    let mut fast_map: FastMap<String, i32> = FastMap::default();
    for i in 0..1000 {
        fast_map.insert(format!("key{}", i), i);
    }
    group.bench_function("fast_map_lookup", |b| {
        b.iter(|| black_box(fast_map.get("key500")))
    });

    // FastMap insert
    group.bench_function("fast_map_insert_100", |b| {
        b.iter(|| {
            let mut m: FastMap<String, i32> = FastMap::default();
            for i in 0..100 {
                m.insert(format!("key{}", i), i);
            }
            black_box(m)
        })
    });

    // Standard HashMap insert (for comparison)
    group.bench_function("std_hashmap_insert_100", |b| {
        b.iter(|| {
            let mut m: HashMap<String, i32> = HashMap::new();
            for i in 0..100 {
                m.insert(format!("key{}", i), i);
            }
            black_box(m)
        })
    });

    group.finish();
}

// ============================================
// Lexer
// ============================================

fn bench_lexer(c: &mut Criterion) {
    let small_source = "let x = 42;";
    let medium_source = r#"
        fn fibonacci(n) {
            if (n <= 1) return n;
            return fibonacci(n - 1) + fibonacci(n - 2);
        }
    "#;
    let large_source = r#"
        fn quicksort(arr, lo, hi) {
            if (lo < hi) {
                let pivot = arr[hi];
                let i = lo - 1;
                for (let j = lo; j < hi; j = j + 1) {
                    if (arr[j] <= pivot) {
                        i = i + 1;
                        let temp = arr[i];
                        arr[i] = arr[j];
                        arr[j] = temp;
                    }
                }
                let temp = arr[i + 1];
                arr[i + 1] = arr[hi];
                arr[hi] = temp;
                let pi = i + 1;
                quicksort(arr, lo, pi - 1);
                quicksort(arr, pi + 1, hi);
            }
        }
        fn main() {
            let arr = [3, 6, 8, 10, 1, 2, 1];
            quicksort(arr, 0, 6);
            for (let i = 0; i < 7; i = i + 1) {
                print(arr[i]);
            }
        }
    "#;

    let mut group = c.benchmark_group("lexer");

    group.bench_function("small_10_tokens", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(small_source));
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.bench_function("medium_50_tokens", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(medium_source));
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.bench_function("large_200_tokens", |b| {
        b.iter(|| {
            let mut lexer = Lexer::new(black_box(large_source));
            black_box(lexer.tokenize().unwrap())
        })
    });

    group.finish();
}

// ============================================
// Parser
// ============================================

fn bench_parser(c: &mut Criterion) {
    let source = r#"
        fn test(x, y) {
            let z = x + y;
            return z * 2;
        }

        class Point {
            constructor(x, y) {
                this.x = x;
                this.y = y;
            }

            fn distance() {
                return sqrt(this.x * this.x + this.y * this.y);
            }
        }

        fn main() {
            let p = new Point(3, 4);
            let d = p.distance();
            let result = test(10, 20);
            print(result);
        }
    "#;

    c.benchmark_group("parser")
        .bench_function("mixed_constructs", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                black_box(parser.parse_program().unwrap())
            })
        });
}

// ============================================
// HIR lowering and safety passes
// ============================================

fn bench_hir_lowering(c: &mut Criterion) {
    let source = r#"
        fn fibonacci(n) {
            if (n <= 1) { return n; }
            return fibonacci(n - 1) + fibonacci(n - 2);
        }

        fn main() {
            let result = fibonacci(10);
            print(result);
        }
    "#;

    c.benchmark_group("hir")
        .bench_function("lowering_fibonacci", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                black_box(ast_to_hir(&program, false).unwrap())
            })
        });
}

fn bench_safety_passes(c: &mut Criterion) {
    let source = r#"
        fn add(a, b) {
            let result = a + b;
            return result;
        }

        fn main() {
            let x = 10;
            let y = 20;
            let z = add(x, y);
            print(z);
        }
    "#;

    c.benchmark_group("safety")
        .bench_function("unified_safety_pass", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                let hir = ast_to_hir(&program, false).unwrap();
                let mut pass = UnifiedSafetyPass::new();
                black_box(pass.analyze(&hir))
            })
        })
        .bench_function("hir_safety_passes", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                let hir = ast_to_hir(&program, false).unwrap();
                black_box(run_safety_passes(&hir, true, true))
            })
        })
        .bench_function("phase3_analysis", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                let hir = ast_to_hir(&program, false).unwrap();
                black_box(run_phase3_passes(&hir))
            })
        });
}

// ============================================
// Constant folding optimization
// ============================================

fn bench_constant_folding(c: &mut Criterion) {
    let source = r#"
        fn compute() {
            let a = 2 + 3 * 4 - 1;
            let b = 10 * 10 + 5;
            let c = a + b;
            let d = c * 2 - 1;
            return d;
        }
    "#;

    c.benchmark_group("optimization")
        .bench_function("constant_folding", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                let mut hir = ast_to_hir(&program, false).unwrap();
                fold_module_constants(&mut hir);
                black_box(hir)
            })
        });
}

// ============================================
// Full pipeline (lex → parse → lower → safety)
// ============================================

fn bench_full_pipeline(c: &mut Criterion) {
    let source = r#"
        fn quicksort(arr, lo, hi) {
            if (lo < hi) {
                let pivot = arr[hi];
                let i = lo - 1;
                for (let j = lo; j < hi; j = j + 1) {
                    if (arr[j] <= pivot) {
                        i = i + 1;
                        let temp = arr[i];
                        arr[i] = arr[j];
                        arr[j] = temp;
                    }
                }
                let temp = arr[i + 1];
                arr[i + 1] = arr[hi];
                arr[hi] = temp;
                let pi = i + 1;
                quicksort(arr, lo, pi - 1);
                quicksort(arr, pi + 1, hi);
            }
        }

        fn main() {
            let arr = [5, 2, 8, 1, 9, 3, 7, 4, 6];
            quicksort(arr, 0, 8);
            for (let i = 0; i < 9; i = i + 1) {
                print(arr[i]);
            }
        }
    "#;

    c.benchmark_group("pipeline")
        .bench_function("lex_parse_lower_safety", |b| {
            b.iter(|| {
                let mut lexer = Lexer::new(black_box(source));
                let tokens = lexer.tokenize().unwrap();
                let mut parser = Parser::new(tokens, None);
                let program = parser.parse_program().unwrap();
                let hir = ast_to_hir(&program, false).unwrap();
                let mut pass = UnifiedSafetyPass::new();
                let _ = pass.analyze(&hir);
                black_box(hir)
            })
        });
}

criterion_group!(
    benches,
    bench_value_clone,
    bench_string_interning,
    bench_hashmap_comparison,
    bench_lexer,
    bench_parser,
    bench_hir_lowering,
    bench_safety_passes,
    bench_constant_folding,
    bench_full_pipeline,
);

criterion_main!(benches);
