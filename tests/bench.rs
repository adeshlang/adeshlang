use adeshlang::{Interpreter, ModuleLoader};
use std::time::Instant;

fn run_with_large_stack<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("failed to spawn test thread");
    handle.join().expect("test thread panicked");
}

fn run_src(label: &str, src: &str) {
    let label = label.to_string();
    let src = src.to_string();
    run_with_large_stack(move || {
        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(std::path::Path::new("."));
        let start = Instant::now();
        if let Err(e) = interp.run_module(&src, &mut loader, Some(label.clone())) {
            panic!("run error: {}", e);
        }
        let dur = start.elapsed();
        println!("{}: {:?}", label, dur);
    });
}

#[test]
fn bench_loops_arrays_strings() {
    let src = r#"
    let acc = "";
    let xs = [];
    for(i in 0..1000) {
      xs = xs + [i];
      acc = acc + "x";
    }
    if (xs.length != 1000 || acc.length != 1000) {
        throw "Loop length mismatch: xs=" + string(xs.length) + ", acc=" + string(acc.length);
    }
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
    for(i in 0..1000) { c.inc(); }
    if (c.val() != 1000) {
        throw "Object counter mismatch: expected 1000, got " + string(c.val());
    }
    "#;
    run_src("bench_objects_props_methods", src);
}

#[test]
fn bench_generics_calls() {
    let src = r#"
    fn id<T>(x:T):T { return x; }
    let a:number = 0;
    for(i in 0..1000) { a = id(i); }
    if (a != 999) {
        throw "Generics call mismatch: expected 999, got " + string(a);
    }
    "#;
    run_src("bench_generics_calls", src);
}

#[test]
fn bench_import_default_named() {
    let src = r#"
    let sum = fn(a, b) { return a + b; };
    let res = sum(1, 2);
    if (res != 3) {
        throw "Sum mismatch: expected 3, got " + string(res);
    }
    "#;
    run_src("bench_import_default_named", src);
}
