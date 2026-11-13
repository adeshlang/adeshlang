//! Real CLI -> native backend/linker -> host loader, in default and fast modes.
//! Build the host runtime first: `cargo build -p adesh-runtime`.
//! Windows can additionally exercise a staged Linux runtime through WSL:
//! ADESH_DESKTOP_TEST_TARGET=x86_64-unknown-linux-gnu
//! ADESH_TEST_WSL_DISTRO=<distro>
//! Missing tools/archives are failures in this suite, never silent skips.
#![cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]

#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

fn host_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("linux", "x86_64") if cfg!(target_env = "musl") => "x86_64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        host => panic!("desktop execution conformance has no target for {host:?}"),
    }
}

fn check_image(path: &Path, target: &str) {
    let bytes = std::fs::read(path).expect("successful CLI build must write the requested image");
    let u16_at = |p| u16::from_le_bytes(bytes[p..p + 2].try_into().unwrap());
    let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    if target.contains("windows") {
        assert_eq!(&bytes[..2], b"MZ");
        let pe = u32_at(0x3c) as usize;
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
        assert_eq!(u16_at(pe + 4), 0x8664, "AMD64");
        assert_eq!(u16_at(pe + 24), 0x20b, "PE32+");
    } else if target.contains("linux") {
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(&bytes[4..6], &[2, 1], "ELF64, little endian");
        assert_eq!(
            u16_at(18),
            if target.starts_with("aarch64") {
                183
            } else {
                62
            },
            "image architecture must match the requested target"
        );
    } else {
        assert_eq!(u32_at(0), 0xfeedfacf, "Mach-O 64");
        assert_eq!(
            u32_at(4),
            if target.starts_with("aarch64") {
                0x0100000c
            } else {
                0x01000007
            }
        );
        assert_eq!(u32_at(12), 2, "MH_EXECUTE");
        assert_ne!(u32_at(24) & 0x00200000, 0, "MH_PIE");
        // Do not re-sign the artifact with Apple's tool, which would hide a
        // broken native signer. Verify the exact image the CLI produced.
        let verified = run(Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(path));
        assert!(
            verified.status.success(),
            "codesign rejected {path:?}: {verified:?}"
        );
    }
}

fn source_case(source: &str, stdout: &str, exit: i32) {
    let target =
        std::env::var("ADESH_DESKTOP_TEST_TARGET").unwrap_or_else(|_| host_target().to_owned());
    let wsl = if target != host_target() {
        assert!(
            cfg!(windows)
                && matches!(
                    target.as_str(),
                    "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl"
                ),
            "only host execution or an explicit Windows-to-WSL Linux target is supported: {target}"
        );
        Some(
            LinuxTools::discover("as", "qemu-x86_64")
                .expect("requested WSL execution tools missing"),
        )
    } else {
        None
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for fast in [false, true] {
        // Paths with spaces catch accidental shell interpolation in the driver.
        let dir = tempfile::Builder::new()
            .prefix("adesh desktop execution ")
            .tempdir_in(repo.join("target"))
            .unwrap();
        let input = dir.path().join("desktop source.adesh");
        let image = dir.path().join(if target.contains("windows") {
            "desktop app.exe"
        } else {
            "desktop app"
        });
        std::fs::write(&input, source).unwrap();
        let mut build = Command::new(env!("CARGO_BIN_EXE_adesh"));
        build
            .current_dir(repo)
            .arg("build")
            .arg(&input)
            .args(["--target", &target, "--emit=exe", "-o"])
            .arg(&image);
        if fast {
            build.arg("--fast");
        }
        let built = run(&mut build);
        assert!(built.status.success(), "{target}, fast={fast}: {built:?}");
        check_image(&image, &target);
        let executed = if let Some(tools) = &wsl {
            let path = tools.path(&image);
            let chmod = tools.tool("chmod", &[OsStr::new("+x"), OsStr::new(&path)]);
            assert!(chmod.status.success(), "chmod: {chmod:?}");
            tools.tool("qemu-x86_64", &[OsStr::new(&path)])
        } else {
            run(Command::new(&image).current_dir(dir.path()))
        };
        assert_eq!(
            executed.status.code(),
            Some(exit),
            "{target}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout,
            stdout.as_bytes(),
            "{target}, fast={fast}: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{target}, fast={fast}: {executed:?}"
        );
    }
}

#[test]
fn hello_uses_the_native_runtime() {
    source_case(
        "print(\"desktop native runtime\");",
        "desktop native runtime\n",
        0,
    );
}

#[test]
fn main_return_is_the_process_exit_code() {
    source_case("fn main(): int { return 42; }", "", 42);
}

#[test]
fn nested_calls_and_arithmetic() {
    source_case(
        include_str!("fixtures/phase13_aarch64/functions.adesh"),
        "",
        50,
    );
}

#[test]
fn loops_and_comparisons() {
    source_case(include_str!("fixtures/phase13_aarch64/loop.adesh"), "", 55);
}

#[test]
fn repeated_runtime_calls_preserve_the_frame() {
    source_case(
        "fn main() { print(1); print(2); print(3); print(4); print(5); print(6); print(7); print(8); print(70000 + 5); }",
        "1\n2\n3\n4\n5\n6\n7\n8\n70005\n",
        0,
    );
}

#[test]
fn capturing_closure_preserves_its_environment() {
    source_case(
        "fn main() { let base = 10; let add = fn(a) { return a + base; }; print(add(1)); print(add(7)); }",
        "11\n17\n",
        0,
    );
}

#[test]
fn short_circuit_preserves_side_effect_order() {
    source_case(
        r#"
        fn noisy(x: int) -> int { print("eval"); return x; }
        fn main() {
            print(noisy(0) && noisy(7));
            print(noisy(6) || noisy(9));
            print(noisy(3) && noisy(8));
            print(noisy(0) || noisy(9));
        }
        "#,
        "eval\n0\neval\n6\neval\neval\n8\neval\neval\n9\n",
        0,
    );
}

#[test]
fn deferred_call_preserves_the_return_value() {
    source_case(
        r#"
        fn value() -> int { defer { print("cleanup"); } return 42; }
        fn main() { print(value()); }
        "#,
        "cleanup\n42\n",
        0,
    );
}

#[test]
fn stack_arguments_survive_nested_calls() {
    source_case(
        r#"
        fn sum(a:int,b:int,c:int,d:int,e:int,f:int,g:int,h:int,i:int,j:int): int {
            return a+b+c+d+e+f+g+h+i+j;
        }
        fn call(): int { return sum(1,2,3,4,5,6,7,8,9,10); }
        fn main() { print(call()); }
        "#,
        "55\n",
        0,
    );
}
