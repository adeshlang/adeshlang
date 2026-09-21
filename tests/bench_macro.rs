use adeshlang::{Interpreter, ModuleLoader};
use std::time::Instant;

fn run(label: &str, src: &str) {
    let label = label.to_string();
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut interp = Interpreter::new();
            let mut loader = ModuleLoader::new(std::path::Path::new("."));
            let t0 = Instant::now();
            let res = interp.run_module(&src, &mut loader, Some(label.clone()));
            assert!(res.is_ok(), "{} failed: {:?}", label, res);
            println!("{}: {:?}", label, t0.elapsed());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn macro_fib_recursive() {
    let src = r#"
    fn fib(n) {
      let a = 0; let b = 1;
      for(i in 0..n) { let t = a + b; a = b; b = t; }
      return a;
    }
    let res = fib(10);
    if (res != 55) {
        throw "Fib(10) mismatch: expected 55, got " + string(res);
    }
    "#;
    run("macro_fib_recursive", src);
}

#[test]
fn macro_json_build() {
    let src = r#"
    let a = [];
    for(i in 0..1000) { a = a + [i]; }
    if (a.length != 1000) {
        throw "Array size mismatch: expected 1000, got " + string(a.length);
    }
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
    for(i in 0..1000) { s = s + "x"; }
    let cnt = count(s, "x");
    if (cnt != 1001) {
        throw "Count mismatch: expected 1001, got " + string(cnt);
    }
    "#;
    run("macro_regex_like", src);
}
