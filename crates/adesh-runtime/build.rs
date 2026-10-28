fn main() {
    // The standalone C output object targets native AOT binaries only.
    // wasm32-unknown-unknown has no libc/POSIX headers (unistd.h), and the
    // aot_* symbols are consumed exclusively by the native codegen path
    // (symbol names emitted into generated assembly), so the object is
    // neither compilable nor needed for WebAssembly builds.
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("wasm") {
        println!("cargo:rerun-if-changed=build.rs");
        return;
    }

    println!("cargo:rerun-if-changed=src/native_output.c");
    let mut build = cc::Build::new();
    build.file("src/native_output.c").warnings(true);
    if build.get_compiler().is_like_msvc() {
        // This standalone runtime object is linked without the MSVC CRT that
        // normally supplies __security_cookie and __security_check_cookie.
        build.flag("/GS-");
    } else {
        build.flag_if_supported("-fno-stack-protector");
    }
    build.compile("adesh_native_output");
}
