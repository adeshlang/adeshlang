//! Phase 19.9 — Differential Consistency Tests across Interpreter, NJIT, AOT, and WASM.
//!
//! Runs both curated semantic parity programs (control flow, recursion, integer
//! arithmetic including negative values, short-circuiting/booleans, strings,
//! arrays, closures, enums/match, defer/try-catch) and seeded property-generated
//! programs across:
//! 1. Interpreter (`adesh run --quiet`)
//! 2. Native JIT (`adesh run --quiet --njit`) [x86_64]
//! 3. Native AOT ELF (`adesh build -o ... --quiet` + direct execution) [linux x86_64]
//! 4. WASM WASI (`adesh build --target=wasm32-unknown-wasi` + `wasmtime` execution)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::tempdir;
use wasmtime::{Caller, Engine, Extern, Linker, Memory, Module, Store};

fn adesh_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_adesh"))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> (String, String, i32) {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn command");
    let start = Instant::now();
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                let output = child.wait_with_output().expect("wait_with_output");
                let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout).replace('\r', ""));
                let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr).replace('\r', ""));
                return (stdout, stderr, status.code().unwrap_or(-1));
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("command timed out after {:?}", timeout);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
}

fn run_interpreter(src_path: &Path) -> (String, String, i32) {
    let mut cmd = Command::new(adesh_bin());
    cmd.args(["run", "--quiet"]).arg(src_path);
    run_with_timeout(&mut cmd, Duration::from_secs(30))
}

#[cfg(target_arch = "x86_64")]
fn run_njit(src_path: &Path) -> (String, String, i32) {
    let mut cmd = Command::new(adesh_bin());
    cmd.args(["run", "--quiet", "--njit"]).arg(src_path);
    run_with_timeout(&mut cmd, Duration::from_secs(30))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_aot(dir: &Path, src_path: &Path) -> (String, String, i32) {
    let exe = dir.join("aot_out");
    let mut build_cmd = Command::new(adesh_bin());
    build_cmd
        .arg("build")
        .arg(src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet");
    let (b_out, b_err, b_code) = run_with_timeout(&mut build_cmd, Duration::from_secs(60));
    assert_eq!(
        b_code, 0,
        "AOT build failed (code={b_code}):\nstdout:\n{b_out}\nstderr:\n{b_err}"
    );
    assert!(exe.exists(), "AOT binary missing at {}", exe.display());

    let mut run_cmd = Command::new(&exe);
    run_with_timeout(&mut run_cmd, Duration::from_secs(30))
}

const WASI_STDOUT: i32 = 1;
const WASI_EBADF: i32 = 8;

#[derive(Default)]
struct WasiCapture {
    stdout: Vec<u8>,
}

fn read_u32(memory: &Memory, caller: &Caller<'_, WasiCapture>, addr: usize) -> Option<u32> {
    let mut word = [0u8; 4];
    memory.read(caller, addr, &mut word).ok()?;
    Some(u32::from_le_bytes(word))
}

fn wasi_fd_write(
    mut caller: Caller<'_, WasiCapture>,
    fd: i32,
    iovs: i32,
    iovs_len: i32,
    nwritten: i32,
) -> i32 {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return WASI_EBADF;
    };
    let mut payload = Vec::new();
    for index in 0..iovs_len.max(0) as usize {
        let entry = iovs.max(0) as usize + index * 8;
        let (Some(base), Some(len)) = (
            read_u32(&memory, &caller, entry),
            read_u32(&memory, &caller, entry + 4),
        ) else {
            return WASI_EBADF;
        };
        let mut buffer = vec![0u8; len as usize];
        if memory.read(&caller, base as usize, &mut buffer).is_err() {
            return WASI_EBADF;
        }
        payload.extend_from_slice(&buffer);
    }
    let written = payload.len() as u32;
    if memory
        .write(
            &mut caller,
            nwritten.max(0) as usize,
            &written.to_le_bytes(),
        )
        .is_err()
    {
        return WASI_EBADF;
    }
    if fd == WASI_STDOUT {
        caller.data_mut().stdout.extend_from_slice(&payload);
    }
    0
}

fn run_wasm_wasi(dir: &Path, src_path: &Path) -> String {
    let wasm_path = dir.join("out.wasm");
    let mut build_cmd = Command::new(adesh_bin());
    build_cmd
        .arg("build")
        .arg(src_path)
        .arg("--target=wasm32-unknown-wasi")
        .arg("-o")
        .arg(&wasm_path)
        .arg("--quiet");
    let (b_out, b_err, b_code) = run_with_timeout(&mut build_cmd, Duration::from_secs(60));
    assert_eq!(
        b_code, 0,
        "WASM WASI build failed (code={b_code}):\nstdout:\n{b_out}\nstderr:\n{b_err}"
    );
    assert!(wasm_path.exists(), "WASM file missing");

    let engine = Engine::default();
    let module = Module::from_file(&engine, &wasm_path).expect("wasmtime compile");
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("wasi_snapshot_preview1", "fd_write", wasi_fd_write)
        .expect("wrap fd_write");
    let mut store = Store::new(&engine, WasiCapture::default());
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate wasm");
    let entry = instance
        .get_typed_func::<(), ()>(&mut store, "main")
        .or_else(|_| instance.get_typed_func::<(), ()>(&mut store, "_start"))
        .expect("main or _start export");
    entry.call(&mut store, ()).expect("run wasm entrypoint");
    String::from_utf8_lossy(&store.data().stdout).replace('\r', "")
}

fn normalize_lines(s: &str) -> Vec<String> {
    s.lines().map(|l| l.trim_end().to_string()).collect()
}

fn assert_all_4_backends_match(name: &str, src: &str, expected: &[&str]) {
    let dir = tempdir().expect("tempdir");
    let src_path = dir.path().join(format!("{name}.adesh"));
    std::fs::write(&src_path, src).expect("write source");

    let expected_vec: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();

    let (i_out, i_err, i_code) = run_interpreter(&src_path);
    assert_eq!(i_code, 0, "[{name}] interpreter failed: {i_err}");
    assert_eq!(
        normalize_lines(&i_out),
        expected_vec,
        "[{name}] interpreter output mismatch:\n{i_out}"
    );

    #[cfg(target_arch = "x86_64")]
    {
        let (n_out, n_err, n_code) = run_njit(&src_path);
        assert_eq!(n_code, 0, "[{name}] NJIT failed: {n_err}");
        assert_eq!(
            normalize_lines(&n_out),
            expected_vec,
            "[{name}] NJIT output mismatch vs interpreter:\n{n_out}"
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let (a_out, a_err, a_code) = run_aot(dir.path(), &src_path);
        assert_eq!(a_code, 0, "[{name}] AOT failed: {a_err}");
        assert_eq!(
            normalize_lines(&a_out),
            expected_vec,
            "[{name}] AOT output mismatch vs interpreter:\n{a_out}"
        );
    }

    let w_out = run_wasm_wasi(dir.path(), &src_path);
    assert_eq!(
        normalize_lines(&w_out),
        expected_vec,
        "[{name}] WASM WASI output mismatch vs interpreter:\n{w_out}"
    );
}

fn assert_native_3_backends_match(name: &str, src: &str, expected: &[&str]) {
    let dir = tempdir().expect("tempdir");
    let src_path = dir.path().join(format!("{name}.adesh"));
    std::fs::write(&src_path, src).expect("write source");

    let expected_vec: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();

    let (i_out, i_err, i_code) = run_interpreter(&src_path);
    assert_eq!(i_code, 0, "[{name}] interpreter failed: {i_err}");
    assert_eq!(
        normalize_lines(&i_out),
        expected_vec,
        "[{name}] interpreter output mismatch:\n{i_out}"
    );

    #[cfg(target_arch = "x86_64")]
    {
        let (n_out, n_err, n_code) = run_njit(&src_path);
        assert_eq!(n_code, 0, "[{name}] NJIT failed: {n_err}");
        assert_eq!(
            normalize_lines(&n_out),
            expected_vec,
            "[{name}] NJIT output mismatch vs interpreter:\n{n_out}"
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let (a_out, a_err, a_code) = run_aot(dir.path(), &src_path);
        assert_eq!(a_code, 0, "[{name}] AOT failed: {a_err}");
        assert_eq!(
            normalize_lines(&a_out),
            expected_vec,
            "[{name}] AOT output mismatch vs interpreter:\n{a_out}"
        );
    }
}

#[test]
fn parity_4way_arithmetic_recursion_and_negative_values() {
    let src = r#"
fn fib(n: int): int {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn sum_range(lo: int, hi: int): int {
    let acc = 0;
    let i = lo;
    while i <= hi {
        if i != 3 {
            acc = acc + i;
        }
        i = i + 1;
    }
    return acc;
}

fn main() {
    let a = -15;
    let b = 27;
    println(a + b);
    println(a * -2);
    println(fib(9));
    println(sum_range(1, 7));
}
"#;
    assert_all_4_backends_match("arith_rec_loops", src, &["12", "30", "34", "25"]);
}

#[test]
fn parity_native3_loops_break_continue_floats_and_booleans() {
    let src = r#"
fn scan_sum(limit: int): int {
    let acc = 0;
    let i = 0;
    while i < 20 {
        i = i + 1;
        if i % 2 == 0 {
            continue;
        }
        if i > limit {
            break;
        }
        acc = acc + i;
    }
    return acc;
}

fn main() {
    println(scan_sum(9));
    let x = 2.5;
    let y = 4.0;
    println(x * y + 1.5);
    let ok = (10 > 3) && !(4 == 5);
    println(ok);
}
"#;
    assert_native_3_backends_match("break_continue_floats", src, &["25", "11.5", "true"]);
}

#[test]
fn parity_native3_strings_and_multicapture_closures() {
    let src = r#"
enum Op {
    AddTen(int),
    Scale(int),
}

fn eval_op(op: Op): int {
    return match (op) {
        Op.AddTen(a) => a + 10,
        Op.Scale(x) => x * 3,
    };
}

fn main() {
    let a = -7;
    let b = 4;
    let scale_add = fn(x, y) { return (x + y) * b + a; };
    println(scale_add(2, 3));
    println(eval_op(Op.AddTen(11)));
    println(eval_op(Op.Scale(-6)));
    println(len("ab" + "cde"));
}
"#;
    assert_native_3_backends_match("strings_multicapture_match", src, &["13", "21", "-18", "5"]);
}

#[test]
fn parity_4way_branches_short_circuit_and_nested_calls() {
    let src = r#"
fn clamp(x: int, lo: int, hi: int): int {
    if x < lo {
        return lo;
    }
    if x > hi {
        return hi;
    }
    return x;
}

fn step(n: int): int {
    if n <= 0 {
        return 1;
    }
    if n % 2 == 0 {
        return step(n / 2) + 10;
    }
    return step(n - 1) + 3;
}

fn main() {
    println(clamp(-50, -10, 10));
    println(clamp(5, -10, 10));
    println(clamp(99, -10, 10));
    println(step(11));
}
"#;
    // step(11) -> step(10)+3 -> step(5)+13 -> step(4)+16 -> step(2)+26 -> step(1)+36 -> step(0)+39 = 40
    assert_all_4_backends_match("branches_nested", src, &["-10", "5", "10", "40"]);
}

#[test]
fn parity_native3_arrays_strings_closures_and_enums() {
    let src = r#"
enum ExprVal {
    Num(int),
    Double(int),
    Zero,
}

fn eval_ev(v: ExprVal): int {
    return match (v) {
        ExprVal.Num(n) => n,
        ExprVal.Double(n) => n * 2,
        ExprVal.Zero => 0,
    };
}

fn main() {
    let arr = [10, -3, 7, 20];
    arr[1] = 15;
    let sum = 0;
    let i = 0;
    while i < 4 {
        sum = sum + arr[i];
        i = i + 1;
    }
    println(sum);

    let k = 5;
    let add_k = fn(x) { return x + k; };
    println(add_k(12));

    let e1 = eval_ev(ExprVal.Num(9));
    let e2 = eval_ev(ExprVal.Double(8));
    let e3 = eval_ev(ExprVal.Zero);
    println(e1 + e2 + e3);

    let s = "hello";
    println(s + " world");
}
"#;
    assert_native_3_backends_match(
        "arrays_closures_enums",
        src,
        &["52", "17", "25", "hello world"],
    );
}

#[test]
fn parity_native3_try_catch_and_defer_lifo() {
    let src = r#"
fn risky(x: int): int {
    defer {
        println("defer_risky");
    }
    if x < 0 {
        throw "neg";
    }
    return x * 10;
}

fn main() {
    let r1 = 0;
    try {
        r1 = risky(-5);
    } catch (e) {
        println("caught");
        r1 = -99;
    }
    println(r1);
    let r2 = risky(4);
    println(r2);
}
"#;
    assert_native_3_backends_match(
        "try_catch_defer",
        src,
        &["defer_risky", "caught", "-99", "defer_risky", "40"],
    );
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 32
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next() % span) as i64
    }
}

#[derive(Clone)]
enum PropExpr {
    Lit(i64),
    Var(usize),
    Add(Box<PropExpr>, Box<PropExpr>),
    Sub(Box<PropExpr>, Box<PropExpr>),
    Mul(Box<PropExpr>, Box<PropExpr>),
    CondGt(Box<PropExpr>, Box<PropExpr>, Box<PropExpr>, Box<PropExpr>),
}

impl PropExpr {
    fn generate(rng: &mut Lcg, depth: u32, nvars: usize) -> Self {
        if depth == 0 || rng.range(0, 3) == 0 {
            if rng.range(0, 1) == 0 {
                PropExpr::Lit(rng.range(-15, 15))
            } else {
                PropExpr::Var(rng.range(0, (nvars - 1) as i64) as usize)
            }
        } else {
            let pick = rng.range(0, 3);
            let a = Box::new(Self::generate(rng, depth - 1, nvars));
            let b = Box::new(Self::generate(rng, depth - 1, nvars));
            match pick {
                0 => PropExpr::Add(a, b),
                1 => PropExpr::Sub(a, b),
                2 => PropExpr::Mul(a, b),
                _ => {
                    let t = Box::new(Self::generate(rng, depth - 1, nvars));
                    let f = Box::new(Self::generate(rng, depth - 1, nvars));
                    PropExpr::CondGt(a, b, t, f)
                }
            }
        }
    }

    fn eval(&self, env: &[i64]) -> i64 {
        match self {
            PropExpr::Lit(v) => *v,
            PropExpr::Var(i) => env[*i],
            PropExpr::Add(a, b) => a.eval(env).wrapping_add(b.eval(env)),
            PropExpr::Sub(a, b) => a.eval(env).wrapping_sub(b.eval(env)),
            PropExpr::Mul(a, b) => a.eval(env).wrapping_mul(b.eval(env)),
            PropExpr::CondGt(a, b, t, f) => {
                if a.eval(env) > b.eval(env) {
                    t.eval(env)
                } else {
                    f.eval(env)
                }
            }
        }
    }

    fn render(&self, names: &[&str]) -> String {
        match self {
            PropExpr::Lit(v) if *v < 0 => format!("(0 - {})", -v),
            PropExpr::Lit(v) => v.to_string(),
            PropExpr::Var(i) => names[*i].to_string(),
            PropExpr::Add(a, b) => format!("({} + {})", a.render(names), b.render(names)),
            PropExpr::Sub(a, b) => format!("({} - {})", a.render(names), b.render(names)),
            PropExpr::Mul(a, b) => format!("({} * {})", a.render(names), b.render(names)),
            PropExpr::CondGt(a, b, t, f) => format!(
                "choose_gt({}, {}, {}, {})",
                a.render(names),
                b.render(names),
                t.render(names),
                f.render(names)
            ),
        }
    }
}

#[test]
fn parity_4way_seeded_property_generated_programs() {
    let mut rng = Lcg(0x1909_2026_CAFE_BABE_u64);
    let names = ["v0", "v1", "v2", "v3"];

    for seed_idx in 0..10 {
        let mut env = [
            rng.range(-10, 10),
            rng.range(-10, 10),
            rng.range(-10, 10),
            rng.range(-10, 10),
        ];
        let mut stmts = String::new();
        for (idx, val) in env.iter().enumerate() {
            if *val < 0 {
                stmts.push_str(&format!("    let {} = 0 - {};\n", names[idx], -val));
            } else {
                stmts.push_str(&format!("    let {} = {};\n", names[idx], val));
            }
        }
        let mut expected = Vec::new();
        for step in 0..4 {
            let target = step % names.len();
            let expr = PropExpr::generate(&mut rng, 2, names.len());
            let val = expr.eval(&env);
            env[target] = val;
            let rendered = match expr {
                PropExpr::Var(idx) => format!("({} + 0)", names[idx]),
                other => other.render(&names),
            };
            stmts.push_str(&format!("    {} = {};\n", names[target], rendered));
            stmts.push_str(&format!("    println({});\n", names[target]));
            expected.push(val.to_string());
        }

        let program = format!(
            "fn choose_gt(a: int, b: int, t: int, f: int): int {{\n    if a > b {{\n        return t;\n    }}\n    return f;\n}}\n\nfn main() {{\n{stmts}}}\n"
        );
        let expected_refs: Vec<&str> = expected.iter().map(|s| s.as_str()).collect();
        assert_all_4_backends_match(&format!("prop_seed_{seed_idx}"), &program, &expected_refs);
    }
}

#[test]
fn parity_native3_structs_conversions_arc_and_memory_alloc() {
    let src = r#"
struct Vec2 {
    x: int,
    y: int,
}

fn dot(a: Vec2, b: Vec2): int {
    return a.x * b.x + a.y * b.y;
}

fn main() {
    let p = Vec2 { x: 3, y: -4 };
    let q = Vec2 { x: 5, y: 2 };
    println(dot(p, q));

    // Integer conversions & wrapping/truncation
    let big: int = 300;
    let byte_val: u8 = big as u8;
    let back: int = byte_val as int;
    println(back);
    let signed_i32: i32 = -12 as i32;
    println(signed_i32 as int);

    // ARC share/strong/weak
    share s1: Shared<int> = 77;
    {
        strong s2 = s1;
        println(s2.strong_count());
    }
    println(s1.strong_count());

    // Memory allocation and free
    unsafe {
        let buf = alloc(32);
        println(buf != 0);
        free(buf);
    }
}
"#;
    assert_native_3_backends_match(
        "structs_conv_arc_alloc",
        src,
        &["7", "44", "-12", "2", "1", "true"],
    );
}

#[test]
fn parity_native3_unhandled_throw_exits_nonzero_and_wasm_reports_capability_error() {
    let dir = tempdir().expect("tempdir");
    let src_path = dir.path().join("unhandled_throw.adesh");
    std::fs::write(&src_path, "fn main() {\n    throw \"boom\";\n}\n").expect("write source");

    let (_i_out, _i_err, i_code) = run_interpreter(&src_path);
    assert_ne!(
        i_code, 0,
        "interpreter must exit non-zero on unhandled throw"
    );

    #[cfg(target_arch = "x86_64")]
    {
        let (_n_out, _n_err, n_code) = run_njit(&src_path);
        assert_ne!(n_code, 0, "NJIT must exit non-zero on unhandled throw");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let (_a_out, _a_err, a_code) = run_aot(dir.path(), &src_path);
        assert_ne!(a_code, 0, "AOT must exit non-zero on unhandled throw");
    }

    // Unhandled `throw` on WASM traps via `unreachable`, matching non-zero exit / trap semantics.
    let wasm_throw_out = dir.path().join("unhandled_throw.wasm");
    let mut throw_build_cmd = Command::new(adesh_bin());
    throw_build_cmd
        .arg("build")
        .arg(&src_path)
        .arg("--target=wasm32-unknown-wasi")
        .arg("-o")
        .arg(&wasm_throw_out)
        .arg("--quiet");
    let (wt_out, wt_err, wt_code) = run_with_timeout(&mut throw_build_cmd, Duration::from_secs(60));
    assert_eq!(
        wt_code, 0,
        "WASM WASI build for unhandled throw failed: stdout={wt_out}, stderr={wt_err}"
    );
    let engine = Engine::default();
    let module = Module::from_file(&engine, &wasm_throw_out).expect("wasmtime compile");
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("wasi_snapshot_preview1", "fd_write", wasi_fd_write)
        .expect("wrap fd_write");
    let mut store = Store::new(&engine, WasiCapture::default());
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate wasm");
    let entry = instance
        .get_typed_func::<(), ()>(&mut store, "main")
        .or_else(|_| instance.get_typed_func::<(), ()>(&mut store, "_start"))
        .expect("main or _start export");
    assert!(
        entry.call(&mut store, ()).is_err(),
        "WASM must trap when an unhandled `throw` executes"
    );

    // WASM WASI still explicitly rejects `try`/`catch` unwinding with a capability error.
    let try_catch_path = dir.path().join("try_catch_unsupported.adesh");
    std::fs::write(
        &try_catch_path,
        "fn main() {\n    try {\n        throw \"boom\";\n    } catch (e) {\n        println(1);\n    }\n}\n",
    )
    .expect("write try/catch source");
    let wasm_out = dir.path().join("try_catch_unsupported.wasm");
    let mut build_cmd = Command::new(adesh_bin());
    build_cmd
        .arg("build")
        .arg(&try_catch_path)
        .arg("--target=wasm32-unknown-wasi")
        .arg("-o")
        .arg(&wasm_out)
        .arg("--quiet");
    let (w_out, w_err, w_code) = run_with_timeout(&mut build_cmd, Duration::from_secs(60));
    assert_ne!(
        w_code, 0,
        "WASM must return an explicit capability error for unsupported `try/catch`, stdout={w_out}, stderr={w_err}"
    );
    assert!(
        w_err.contains("unsupported") || w_out.contains("unsupported"),
        "expected explicit capability diagnostic for try/catch, stdout={w_out}, stderr={w_err}"
    );
}
