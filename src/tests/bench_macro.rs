//! Macro-like Benchmarks
//!
//! Larger workloads meant to stress the interpreter and memory behavior.
use std::time::Instant;
use adeshlang::{Interpreter, ModuleLoader};

fn run(label: &str, src: &str) {
    let mut interp = Interpreter::new();
    let mut loader = ModuleLoader::new(std::path::Path::new("."));
    let t0 = Instant::now();
    let res = interp.run_module(src, &mut loader, Some(label.to_string()));
    assert!(res.is_ok(), "{} failed: {:?}", label, res);
    println!("{}: {:?}", label, t0.elapsed());
}

#[test]
fn macro_fib_recursive() {
    let src = r#"
    fn fib(n) {
      let a = 0; let b = 1;
      for(i in 0..n) { let t = a + b; a = b; b = t; }
      return a;
    }
    print(fib(10000));
    "#;
    run("macro_fib_recursive", src);
}

#[test]
fn macro_json_build() {
    let src = r#"
    let a = [];
    for(i in 0..20000) { a = a + [i]; }
    print("size", a.length);
    "#;
    run("macro_json_build", src);
}

#[test]
fn macro_regex_like() {
    let src = r#"
    fn count(s:string, ch:string):number {
      let c:number = 0;
      for(i in 0..s.length) { if s[i] == ch[0] { c = c + 1; } }
      return c;
    }
    let s = "x";
    for(i in 0..50000) { s = s + "x"; }
    print(count(s, "x"));
    "#;
    run("macro_regex_like", src);
}
