fn main() {
    println!("cargo:rerun-if-changed=src/native_output.c");
    cc::Build::new()
        .file("src/native_output.c")
        .warnings(true)
        .compile("adesh_native_output");
}
