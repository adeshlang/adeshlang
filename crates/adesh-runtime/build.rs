fn main() {
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
