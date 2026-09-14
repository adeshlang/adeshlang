fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("wasm32") {
        return;
    }

    // Rebuild when the C helpers change
    println!("cargo:rerun-if-changed=src/backends/jit/native/print_helpers.c");
    println!("cargo:rerun-if-changed=src/runtime/c_runtime/adesh_runtime.c");

    // Build C helpers for the active toolchain (MSVC/clang/gcc)
    cc::Build::new()
        .file("src/backends/jit/native/print_helpers.c")
        .compile("print_helpers");

    // Build the AdeshLang C runtime library (provides adesh_rt_range,
    // adesh_string_concat, array helpers, env/args helpers, etc.)
    // This is linked into the static library so AOT-compiled binaries
    // can resolve runtime symbols on Windows, Linux, macOS, and Android.
    cc::Build::new()
        .file("src/runtime/c_runtime/adesh_runtime.c")
        .compile("adesh_runtime");
}
