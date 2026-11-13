//! Cross-build filenames must describe the destination, not the build host.
use adeshlang::cli::build::{BuildConfig, EmitType};
use std::path::PathBuf;

#[test]
fn inferred_extensions_and_library_prefixes_follow_the_target() {
    for (triple, names) in [
        (
            "x86_64-pc-windows-msvc",
            ["demo.exe", "demo.obj", "demo.lib", "demo.dll", "demo.asm"],
        ),
        (
            "x86_64-unknown-linux-gnu",
            ["demo", "demo.o", "libdemo.a", "libdemo.so", "demo.s"],
        ),
        (
            "x86_64-apple-darwin",
            ["demo", "demo.o", "libdemo.a", "libdemo.dylib", "demo.s"],
        ),
        (
            "aarch64-apple-darwin",
            ["demo", "demo.o", "libdemo.a", "libdemo.dylib", "demo.s"],
        ),
    ] {
        for (emit, name) in [
            EmitType::Executable,
            EmitType::Object,
            EmitType::StaticLib,
            EmitType::SharedLib,
            EmitType::Assembly,
        ]
        .into_iter()
        .zip(names)
        {
            let config = BuildConfig {
                input: PathBuf::from("sources/demo.adesh"),
                target: Some(triple.to_owned()),
                emit,
                ..Default::default()
            };
            assert_eq!(
                config.get_output_path(),
                PathBuf::from("sources").join(name),
                "{triple}, {emit:?}"
            );
        }
    }
}

#[test]
fn extensionless_explicit_unix_executable_output_is_preserved() {
    for triple in [
        "x86_64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        let path = PathBuf::from("output with spaces/desktop app");
        let config = BuildConfig {
            target: Some(triple.to_owned()),
            output: Some(path.clone()),
            ..Default::default()
        };
        assert_eq!(config.get_output_path(), path, "{triple}");
    }
}

#[test]
fn an_explicit_suffix_is_not_overwritten() {
    for triple in [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
    ] {
        let path = PathBuf::from("output/custom.binary");
        let config = BuildConfig {
            target: Some(triple.to_owned()),
            output: Some(path.clone()),
            ..Default::default()
        };
        assert_eq!(config.get_output_path(), path);
    }
}
