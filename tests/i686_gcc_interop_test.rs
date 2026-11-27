//! Phase 2 / Milestone 1: i686 correctness, ELF32 validation, and bidirectional
//! GCC (`i686-linux-gnu-gcc`) interoperability executed under `qemu-i386`.
#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use adesh_linker::{Linker, config::LinkConfig, target::Target};
use foreign_exec::{LinuxTools, run};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

const TRIPLE: &str = "i686-unknown-linux-gnu";

fn i686_runtime_archive(repo: &Path) -> Option<PathBuf> {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let path = repo
        .join("target")
        .join(TRIPLE)
        .join(profile)
        .join("libadesh_runtime.a");
    if path.is_file() { Some(path) } else { None }
}

fn have_gcc_and_qemu() -> Option<LinuxTools> {
    LinuxTools::discover("i686-linux-gnu-gcc", "qemu-i386")
}

fn compile_adesh_to_adob(
    repo: &Path,
    src_path: &Path,
    adob_path: &Path,
    library_mode: bool,
    fast: bool,
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
    cmd.current_dir(repo)
        .arg("build")
        .arg(src_path)
        .args(["--target", TRIPLE, "--emit=adob", "-o"])
        .arg(adob_path);
    if library_mode {
        cmd.arg("--library-mode");
    }
    if fast {
        cmd.arg("--fast");
    }
    run(&mut cmd)
}

fn compile_gcc_object(tools: &LinuxTools, c_path: &Path, obj_path: &Path, pic: bool, opt: &str) {
    let _ = std::fs::write(obj_path, b"");
    let c_arg = tools.path(c_path);
    let obj_arg = tools.path(obj_path);
    let pic_flag = if pic { "-fPIC" } else { "-fno-pic" };
    let pie_flag = if pic { "-fPIC" } else { "-fno-pie" };
    let out = tools.tool(
        "i686-linux-gnu-gcc",
        &[
            OsStr::new(opt),
            OsStr::new("-c"),
            OsStr::new("-Wall"),
            OsStr::new("-Wextra"),
            OsStr::new("-fno-stack-protector"),
            OsStr::new("-ffreestanding"),
            OsStr::new(pic_flag),
            OsStr::new(pie_flag),
            OsStr::new(&c_arg),
            OsStr::new("-o"),
            OsStr::new(&obj_arg),
        ],
    );
    assert!(
        out.status.success(),
        "i686-linux-gnu-gcc failed (pic={pic}, opt={opt}):\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn i686_elf32_validation_and_heavy_regalloc_pressure_execute() {
    let Some(tools) = LinuxTools::discover("readelf", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    if i686_runtime_archive(repo).is_none()
        && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
    {
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    }

    let dir = tempfile::Builder::new()
        .prefix("adesh i686 regalloc ")
        .tempdir()
        .unwrap();
    let src_path = dir.path().join("stress.adesh");
    let src = r#"
fn fib(n: i32): i32 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn pressure_nested(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32): i32 {
    let t1: i32 = a * 3 + b;
    let t2: i32 = c * 5 - d;
    let t3: i32 = (e + f) * 2;
    let t4: i32 = t1 ^ t2;
    let t5: i32 = (t1 + t3) / 3;
    let t6: i32 = (t2 + t4) % 7;
    let t7: i32 = t1 + t2 + t3 + t4;
    let t8: i32 = t5 + t6 + t7;
    let rec: i32 = fib(6);
    let check: i32 = 0;
    if t8 > rec {
        check = 1;
    }
    return (t8 + rec + check + (t1 - t2) + (t3 + t4) + (t5 - t6)) & 255;
}

fn main(): i32 {
    let s1: i32 = pressure_nested(2, 3, 5, 1, 4, 6);
    let s2: i32 = fib(8);
    print("ok");
    return (s1 + s2) & 127;
}
"#;
    std::fs::write(&src_path, src).unwrap();

    for fast in [false, true] {
        let exe_path = dir.path().join(format!("stress_{fast}.elf"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
        cmd.current_dir(repo)
            .arg("build")
            .arg(&src_path)
            .args(["--target", TRIPLE, "--emit=exe", "-o"])
            .arg(&exe_path);
        if fast {
            cmd.arg("--fast");
        }
        let compiled = run(&mut cmd);
        assert!(
            compiled.status.success(),
            "fast={fast}: {}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );

        let readelf = run(Command::new("readelf")
            .args(["-h", "-l", "-S", "-s"])
            .arg(&exe_path));
        assert!(readelf.status.success());
        let report = String::from_utf8_lossy(&readelf.stdout);
        assert!(report.contains("ELF32"), "{report}");
        assert!(report.contains("Intel 80386"), "{report}");
        assert!(report.contains("EXEC (Executable file)"), "{report}");
        assert!(report.contains("GNU_STACK"), "{report}");
        assert!(
            !report.contains("RWE"),
            "stack/segments must not be RWE: {report}"
        );

        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_path))]);
        assert_eq!(executed.stdout, b"ok\n", "fast={fast}: {executed:?}");
        // Verify deterministic computation:
        // t1=9, t2=24, t3=20, t4=17, t5=9, t6=6, t7=70, t8=85, rec=8, check=1
        // s1 = (85 + 8 + 1 + (9-24) + (20+17) + (9-6)) & 255 = (94 - 15 + 37 + 3) & 255 = 119
        // s2 = fib(8) = 21; (119 + 21) & 127 = 140 & 127 = 12
        assert_eq!(
            executed.status.code(),
            Some(12),
            "fast={fast}: {executed:?}"
        );
    }
}

#[test]
fn i686_adesh_calls_gcc_functions_pic_and_nopic() {
    let Some(tools) = have_gcc_and_qemu() else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(runtime_a) = i686_runtime_archive(repo) else {
        if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
            panic!("missing i686 runtime archive");
        }
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    };

    let dir = tempfile::Builder::new()
        .prefix("adesh i686 interop adesh_to_gcc ")
        .tempdir()
        .unwrap();

    let c_path = dir.path().join("c_lib.c");
    std::fs::write(
        &c_path,
        r#"
#include <stdint.h>
#include <stdarg.h>

struct Pair {
    int32_t a;
    int32_t b;
};

static int32_t s_counter = 10;
int32_t g_scale = 3;
int32_t g_bss_acc;
static const int32_t k_lut[4] = { 2, 4, 6, 8 };

int32_t c_signed_arith(int32_t a, int32_t b) {
    return (a * 3) - (b / 2) + (a % 5);
}

uint32_t c_unsigned_shr(uint32_t x, uint32_t shift) {
    return x >> shift;
}

int32_t c_bump_static(int32_t delta) {
    s_counter += delta;
    g_bss_acc += s_counter;
    return s_counter * g_scale;
}

int32_t c_array_lut(int32_t idx) {
    return k_lut[idx & 3] + g_bss_acc;
}

static int32_t leaf_mul(int32_t x, int32_t y) {
    return x * y + 1;
}

int32_t c_nested_calls(int32_t a, int32_t b, int32_t c) {
    return leaf_mul(a, b) + leaf_mul(b, c);
}

int32_t c_ptr_swap_sum(int32_t *p, int32_t *q) {
    int32_t tmp = *p;
    *p = *q + 1;
    *q = tmp + 2;
    return *p + *q;
}

struct Pair c_make_pair(int32_t x, int32_t y) {
    struct Pair p;
    p.a = x + 5;
    p.b = y * 2;
    return p;
}

int32_t c_sum_va(int32_t count, ...) {
    va_list ap;
    va_start(ap, count);
    int32_t total = 0;
    for (int32_t i = 0; i < count; i++) {
        total += va_arg(ap, int32_t);
    }
    va_end(ap);
    return total;
}
"#,
    )
    .unwrap();

    let adesh_path = dir.path().join("driver.adesh");
    std::fs::write(
        &adesh_path,
        r#"
struct Pair { a: i32; b: i32; }

extern "C" fn c_signed_arith(a: i32, b: i32) -> i32;
extern "C" fn c_unsigned_shr(x: u32, shift: u32) -> u32;
extern "C" fn c_bump_static(delta: i32) -> i32;
extern "C" fn c_array_lut(idx: i32) -> i32;
extern "C" fn c_nested_calls(a: i32, b: i32, c: i32) -> i32;
extern "C" fn c_make_pair(x: i32, y: i32) -> Pair;
extern "C" fn c_sum_va(count: i32, ...) -> i32;

fn main(): i32 {
    let r1: i32 = c_signed_arith(14, -6);          // 42 - (-3) + 4 = 49
    let r2: u32 = c_unsigned_shr(1 << 31, 29);     // 4
    let r3: i32 = c_bump_static(5);                // s=15, bss=15, ret=45
    let r4: i32 = c_array_lut(2);                  // lut[2]=6 + 15 = 21
    let r5: i32 = c_nested_calls(2, 3, 4);         // 7 + 13 = 20
    let pair = c_make_pair(3, 7);                  // a=8, b=14
    let r6: i32 = pair.a + pair.b;                 // 22
    let r7: i32 = c_sum_va(4, 1, 2, 3, 4);         // 10
    // 49 + 4 + 45 + 21 + 20 + 22 + 10 = 171; 171 & 127 = 43
    return (r1 + r2 + r3 + r4 + r5 + r6 + r7) & 127;
}
"#,
    )
    .unwrap();

    let adob_path = dir.path().join("driver.adob");
    let compiled = compile_adesh_to_adob(repo, &adesh_path, &adob_path, false, false);
    assert!(
        compiled.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );

    for (pic, opt) in [(false, "-O2"), (true, "-O2"), (true, "-O0")] {
        let c_obj = dir.path().join(format!("c_lib_{pic}_{opt}.o"));
        compile_gcc_object(&tools, &c_path, &c_obj, pic, opt);

        let exe_path = dir.path().join(format!("interop_{pic}_{opt}.elf"));
        let mut config = LinkConfig::new(exe_path.clone(), Target::from_triple(TRIPLE).unwrap());
        config.dynamic = false;
        Linker::link(&[adob_path.clone(), c_obj, runtime_a.clone()], config)
            .unwrap_or_else(|e| panic!("link failed (pic={pic}, opt={opt}): {e}"));

        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_path))]);
        assert_eq!(
            executed.status.code(),
            Some(43),
            "pic={pic}, opt={opt}: {executed:?}"
        );
    }
}

#[test]
fn i686_gcc_calls_adesh_functions_pic_and_nopic() {
    let Some(tools) = have_gcc_and_qemu() else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(runtime_a) = i686_runtime_archive(repo) else {
        if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
            panic!("missing i686 runtime archive");
        }
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    };

    let dir = tempfile::Builder::new()
        .prefix("adesh i686 interop gcc_to_adesh ")
        .tempdir()
        .unwrap();

    let adesh_path = dir.path().join("adesh_lib.adesh");
    std::fs::write(
        &adesh_path,
        r#"
extern "C" fn c_helper(x: i32, y: i32) -> i32;

fn adesh_sum6(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32): i32 {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6;
}

fn adesh_rec_fact(n: i32): i32 {
    if n <= 1 {
        return 1;
    }
    return n * adesh_rec_fact(n - 1);
}

fn adesh_roundtrip(x: i32): i32 {
    return c_helper(x + 1, x * 2) + adesh_rec_fact(4);
}
"#,
    )
    .unwrap();

    let c_main_path = dir.path().join("c_main.c");
    std::fs::write(
        &c_main_path,
        r#"
#include <stdint.h>

extern int32_t adesh_sum6(int32_t a, int32_t b, int32_t c, int32_t d, int32_t e, int32_t f);
extern int32_t adesh_rec_fact(int32_t n);
extern int32_t adesh_roundtrip(int32_t x);

int32_t c_helper(int32_t x, int32_t y) {
    return x * 3 + y;
}

int main(void) {
    int32_t s = adesh_sum6(1, 2, 3, 4, 5, 6); // 1 + 4 + 9 + 16 + 25 + 36 = 91
    int32_t f = adesh_rec_fact(5);            // 120
    int32_t r = adesh_roundtrip(4);           // c_helper(5, 8) + 24 = 23 + 24 = 47
    // 91 + 120 + 47 = 258; 258 & 127 = 2
    return (s + f + r) & 127;
}
"#,
    )
    .unwrap();

    let adob_path = dir.path().join("adesh_lib.adob");
    let compiled = compile_adesh_to_adob(repo, &adesh_path, &adob_path, true, false);
    assert!(
        compiled.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );

    for pic in [false, true] {
        let c_obj = dir.path().join(format!("c_main_{pic}.o"));
        compile_gcc_object(&tools, &c_main_path, &c_obj, pic, "-O2");

        let exe_path = dir.path().join(format!("gcc_to_adesh_{pic}.elf"));
        let mut config = LinkConfig::new(exe_path.clone(), Target::from_triple(TRIPLE).unwrap());
        config.dynamic = false;
        Linker::link(&[c_obj, adob_path.clone(), runtime_a.clone()], config)
            .unwrap_or_else(|e| panic!("link failed (pic={pic}): {e}"));

        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_path))]);
        assert_eq!(executed.status.code(), Some(2), "pic={pic}: {executed:?}");
    }
}

#[test]
fn i686_unsupported_by_value_large_struct_param_is_refused_loudly() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh i686 struct param diag ")
        .tempdir()
        .unwrap();
    let src_path = dir.path().join("bad_struct_arg.adesh");
    let adob_path = dir.path().join("bad_struct_arg.adob");
    std::fs::write(
        &src_path,
        r#"
struct Pair { a: i32; b: i32; }
extern "C" fn take_pair(p: Pair) -> i32;
extern "C" fn make_pair() -> Pair;
fn main(): i32 {
    let p = make_pair();
    return take_pair(p);
}
"#,
    )
    .unwrap();

    let compiled = compile_adesh_to_adob(repo, &src_path, &adob_path, false, false);
    assert!(
        !compiled.status.success(),
        "passing an 8-byte struct by value on i686 must refuse loudly rather than miscompiling"
    );
    let diag = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        diag.contains("larger than 4 bytes") || diag.contains("value-struct"),
        "expected explicit i686 value-struct diagnostic, got:\n{diag}"
    );
}

#[test]
fn i686_adesh_and_gcc_bidirectional_double_abi_interop() {
    let Some(tools) = have_gcc_and_qemu() else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(runtime_a) = i686_runtime_archive(repo) else {
        if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
            panic!("missing i686 runtime archive");
        }
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    };

    let dir = tempfile::Builder::new()
        .prefix("adesh_i686_f64_gcc_")
        .tempdir()
        .unwrap();

    // 1. Adesh calls GCC double functions (0-arg, multi-double, mixed int+double, multi-return).
    let c_lib = dir.path().join("fp_lib.c");
    std::fs::write(
        &c_lib,
        r#"
#include <stdint.h>

double c_zero_arg(void) {
    return 1.25;
}

double c_mixed_args(int32_t a, double x, int32_t b, double y) {
    if (a < 0) {
        return y - x;
    }
    return ((double)a * x) + ((double)b / y);
}

double c_four_doubles(double a, double b, double c, double d) {
    return (a + b) * (c - d);
}
"#,
    )
    .unwrap();

    let adesh_driver = dir.path().join("fp_driver.adesh");
    std::fs::write(
        &adesh_driver,
        r#"
extern "C" fn c_zero_arg() -> f64;
extern "C" fn c_mixed_args(a: i32, x: f64, b: i32, y: f64) -> f64;
extern "C" fn c_four_doubles(a: f64, b: f64, c: f64, d: f64) -> f64;

fn main() -> i32 {
    let z: f64 = c_zero_arg();                        // 1.25
    let m1: f64 = c_mixed_args(3, 2.5, 9, 2.0);       // 7.5 + 4.5 = 12.0
    let m2: f64 = c_mixed_args(-1, 2.0, 0, 5.5);      // 3.5
    let q: f64 = c_four_doubles(1.5, 2.5, 6.0, 1.0);  // 4.0 * 5.0 = 20.0
    print(z, m1, m2, q);
    return (z + m1 + m2 + q) as i32;                  // 36
}
"#,
    )
    .unwrap();

    let driver_adob = dir.path().join("fp_driver.adob");
    let compiled = compile_adesh_to_adob(repo, &adesh_driver, &driver_adob, false, false);
    assert!(
        compiled.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );

    let c_lib_obj = dir.path().join("fp_lib.o");
    compile_gcc_object(&tools, &c_lib, &c_lib_obj, false, "-O2");
    let exe_a2c = dir.path().join("fp_a2c.elf");
    let mut cfg = LinkConfig::new(exe_a2c.clone(), Target::from_triple(TRIPLE).unwrap());
    cfg.dynamic = false;
    Linker::link(&[driver_adob, c_lib_obj, runtime_a.clone()], cfg).unwrap();

    let run_a2c = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_a2c))]);
    assert_eq!(run_a2c.stdout, b"1.25 12 3.5 20\n", "{run_a2c:?}");
    assert_eq!(run_a2c.status.code(), Some(36), "{run_a2c:?}");

    // 2. GCC calls Adesh f64 functions (recursion, mixed i32+f64, multi-return, roundtrip).
    let adesh_lib = dir.path().join("fp_adesh_lib.adesh");
    std::fs::write(
        &adesh_lib,
        r#"
extern "C" fn c_f64_callback(x: f64, k: i32) -> f64;

fn adesh_f64_zero() -> f64 {
    return 2.5;
}

fn adesh_f64_mixed(tag: i32, a: f64, b: f64, scale: i32) -> f64 {
    if tag < 0 {
        return a - b;
    }
    if tag == 0 {
        return (a + b) * 0.5;
    }
    return (a * b) + (scale as f64);
}

fn adesh_f64_pow(base: f64, exp: i32) -> f64 {
    if exp <= 0 {
        return 1.0;
    }
    return base * adesh_f64_pow(base, exp - 1);
}

fn adesh_f64_roundtrip(x: f64, n: i32) -> f64 {
    return c_f64_callback(x + 1.0, n) + adesh_f64_zero();
}
"#,
    )
    .unwrap();

    let c_main = dir.path().join("fp_c_main.c");
    std::fs::write(
        &c_main,
        r#"
#include <stdint.h>

extern double adesh_f64_zero(void);
extern double adesh_f64_mixed(int32_t tag, double a, double b, int32_t scale);
extern double adesh_f64_pow(double base, int32_t exp);
extern double adesh_f64_roundtrip(double x, int32_t n);

double c_f64_callback(double x, int32_t k) {
    return x * (double)k;
}

int main(void) {
    double a = adesh_f64_zero();                     // 2.5
    double b = adesh_f64_mixed(1, 2.0, 3.5, 4);      // 7.0 + 4.0 = 11.0
    double c = adesh_f64_mixed(-1, 9.5, 4.0, 0);     // 5.5
    double d = adesh_f64_pow(1.5, 3);                // 3.375
    double e = adesh_f64_roundtrip(2.0, 3);          // 9.0 + 2.5 = 11.5
    // Total = 2.5 + 11.0 + 5.5 + 3.375 + 11.5 = 33.875 -> 33
    double total = a + b + c + d + e;
    return (total > 33.87 && total < 33.88) ? 33 : 99;
}
"#,
    )
    .unwrap();

    let adesh_lib_adob = dir.path().join("fp_adesh_lib.adob");
    let compiled2 = compile_adesh_to_adob(repo, &adesh_lib, &adesh_lib_adob, true, false);
    assert!(
        compiled2.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compiled2.stdout),
        String::from_utf8_lossy(&compiled2.stderr)
    );
    let c_main_obj = dir.path().join("fp_c_main.o");
    compile_gcc_object(&tools, &c_main, &c_main_obj, false, "-O2");
    let exe_c2a = dir.path().join("fp_c2a.elf");
    let mut cfg2 = LinkConfig::new(exe_c2a.clone(), Target::from_triple(TRIPLE).unwrap());
    cfg2.dynamic = false;
    Linker::link(&[c_main_obj, adesh_lib_adob, runtime_a], cfg2).unwrap();

    let run_c2a = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_c2a))]);
    assert_eq!(run_c2a.status.code(), Some(33), "{run_c2a:?}");
}

#[test]
fn i686_elf32_readelf_structural_tls_and_section_audit() {
    let Some(tools) = LinuxTools::discover("readelf", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    if i686_runtime_archive(repo).is_none()
        && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
    {
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    }

    let dir = tempfile::Builder::new()
        .prefix("adesh_i686_readelf_")
        .tempdir()
        .unwrap();

    // 1. Mixed .tdata + .tbss + .data + .bss + .rodata + .text
    let mixed_src = dir.path().join("mixed_tls.adesh");
    std::fs::write(
        &mixed_src,
        r#"
thread_local let tl_init_i: i32 = 17;
thread_local let tl_init_f: f64 = 2.5;
thread_local let tl_zero: i32 = 0;
let g_rw: i32 = 9;
let g_bss: i32 = 0;

fn main() -> i32 {
    print("rodata-ok", tl_init_i, tl_init_f, tl_zero, g_rw, g_bss);
    return 0;
}
"#,
    )
    .unwrap();
    let mixed_exe = dir.path().join("mixed_tls.elf");
    let out = run(Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .arg("build")
        .arg(&mixed_src)
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&mixed_exe));
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let readelf = run(Command::new("readelf")
        .args(["-h", "-l", "-S", "-s", "-r"])
        .arg(&mixed_exe));
    assert!(readelf.status.success());
    let report = String::from_utf8_lossy(&readelf.stdout);
    assert!(report.contains("TLS"), "missing PT_TLS:\n{report}");
    assert!(report.contains(".tdata"), "missing .tdata:\n{report}");
    assert!(
        report.contains("WAT"),
        "TLS sections must carry SHF_TLS (WAT):\n{report}"
    );
    // Verify non-alloc sections have sh_addr == 00000000 and TLS symbols do not map to ABS.
    for line in report.lines() {
        if (line.contains("SYMTAB") || line.contains("STRTAB")) && !line.contains("Symbol table") {
            assert!(
                line.contains("00000000"),
                "non-alloc section must have Addr=00000000: {line}"
            );
        }
        if line.contains(" TLS ") {
            assert!(
                !line.contains(" ABS "),
                "TLS symbol must reference .tdata/.tbss section index, not ABS: {line}"
            );
        }
    }

    // 2. True SHT_NOBITS .tbss (from GCC `__thread int c_tbss = 0;`) linked into ELF32.
    if let Some(runtime_a) = i686_runtime_archive(repo)
        && LinuxTools::discover("i686-linux-gnu-gcc", "qemu-i386").is_some()
    {
        let c_tbss = dir.path().join("c_tbss.c");
        std::fs::write(
            &c_tbss,
            r#"
__thread int c_tbss_val;
__thread double c_tbss_dbl;
int main(void) {
    c_tbss_val += 4;
    c_tbss_dbl += 1.5;
    return (c_tbss_val == 4 && c_tbss_dbl == 1.5) ? 4 : 99;
}
"#,
        )
        .unwrap();
        let c_tbss_obj = dir.path().join("c_tbss.o");
        compile_gcc_object(&tools, &c_tbss, &c_tbss_obj, false, "-O2");
        let tbss_exe = dir.path().join("c_tbss.elf");
        let mut cfg = LinkConfig::new(tbss_exe.clone(), Target::from_triple(TRIPLE).unwrap());
        cfg.dynamic = false;
        Linker::link(&[c_tbss_obj, runtime_a], cfg).unwrap();

        let readelf2 = run(Command::new("readelf")
            .args(["-l", "-S", "-s"])
            .arg(&tbss_exe));
        assert!(readelf2.status.success());
        let report2 = String::from_utf8_lossy(&readelf2.stdout);
        assert!(
            report2.contains("TLS"),
            "pure .tbss needs PT_TLS:\n{report2}"
        );
        assert!(
            report2.contains(".tbss"),
            "missing .tbss section:\n{report2}"
        );
        assert!(
            report2.contains("NOBITS"),
            ".tbss must be SHT_NOBITS:\n{report2}"
        );
        assert!(
            report2.contains("WAT"),
            ".tbss must carry SHF_TLS (WAT):\n{report2}"
        );

        let tools = LinuxTools::discover("readelf", "qemu-i386").unwrap();
        let run_tbss = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&tbss_exe))]);
        assert_eq!(run_tbss.status.code(), Some(4), "{run_tbss:?}");
    }

    // 3. No-TLS executable must NOT emit PT_TLS.
    let no_tls_src = dir.path().join("no_tls.adesh");
    std::fs::write(&no_tls_src, "fn main() -> i32 { return 3; }\n").unwrap();
    let no_tls_exe = dir.path().join("no_tls.elf");
    let out3 = run(Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .arg("build")
        .arg(&no_tls_src)
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&no_tls_exe));
    assert!(out3.status.success());
    let readelf3 = run(Command::new("readelf").args(["-l"]).arg(&no_tls_exe));
    let report3 = String::from_utf8_lossy(&readelf3.stdout);
    assert!(
        !report3.lines().any(|l| l.trim_start().starts_with("TLS")),
        "no-TLS executable must not emit PT_TLS:\n{report3}"
    );
}
