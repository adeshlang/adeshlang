//! Benchmarks
//!
//! Micro-benchmarks for loops/arrays/strings, objects/methods, generics calls,
//! and import resolution, to get quick performance signals.
use std::time::Instant;
use adeshlang::{Interpreter, ModuleLoader};

fn run_src(label: &str, src: &str) {
    let mut interp = Interpreter::new();
    let mut loader = ModuleLoader::new(std::path::Path::new("."));
    let start = Instant::now();
    if let Err(e) = interp.run_module(src, &mut loader, Some(label.to_string())) {
        panic!("run error: {}", e);
    }
    let dur = start.elapsed();
    println!("{}: {:?}", label, dur);
}

#[test]
fn bench_loops_arrays_strings() {
    let src = r#"
    let acc = "";
    let xs = [];
    for(i in 0..10000) {
      xs = xs + [i];
      acc = acc + "x";
    }
    print(xs.length, acc.length);
    "#;
    run_src("bench_loops_arrays_strings", src);
}

#[test]
fn bench_objects_props_methods() {
    let src = r#"
    class C {
      fn init(n){ this.n = n; }
      fn inc(){ this.n = this.n + 1; }
      fn val(){ return this.n; }
    }
    let c = new C(0);
    for(i in 0..100000) { c.inc(); }
    print(c.val());
    "#;
    run_src("bench_objects_props_methods", src);
}

#[test]
fn bench_generics_calls() {
    let src = r#"
    fn id<T>(x:T):T { return x; }
    let a:number = 0;
    for(i in 0..200000) { a = id<number>(i); }
    print(a);
    "#;
    run_src("bench_generics_calls", src);
}

#[test]
fn bench_import_default_named() {
    // reuse examples modules
    let src = r#"
    import "./examples/utils.adesh";
    from "./examples/utils.adesh" import sum, PI;
    import "./examples/server.adesh";
    from "./examples/server.adesh" import http, tcp;
    let s = new server(1);
    print(utils(2,3), sum(1,2), PI, s.info(), http(), tcp());
    "#;
    run_src("bench_import_default_named", src);
}
