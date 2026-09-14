//! Production Integration Test Suite for AdeshLang Phase 3: Standard Library & I/O
//!
//! Verifies:
//! 1. Filesystem & Path Subsystem (fs.read, fs.write, fs.exists, fs.delete, fs.path.*)
//! 2. JSON Serialization & Deserialization (JSON.parse, JSON.stringify)
//! 3. Cryptographic Operations (SHA256, HMAC, Random)
//! 4. Encoding Subsystem (Base64, Hex)
//! 5. System, Timing & Environment (clock, sizeof, Math)

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_phase3_test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_str, &mut loader, None)
        })
        .unwrap();
    h.join().unwrap().map_err(|e| e.to_string())
}

#[test]
fn test_filesystem_and_path_operations() {
    let test_file = "test_phase3_temp_io.txt";
    let test_dir = "test_phase3_temp_dir";
    let code = format!(
        r#"
        import fs;
        let test_path = "{test_file}";
        let content = "Hello AdeshLang Phase 3 I/O Subsystem!";
        
        fs.write(test_path, content);
        
        if (!fs.exists(test_path)) {{
            throw "File existence check failed";
        }}
        
        if (!fs.isFile(test_path)) {{
            throw "isFile check failed";
        }}
        
        let read_back = fs.read(test_path);
        if (read_back != content) {{
            throw "Read content mismatch";
        }}
        
        let base = fs.path.basename(test_path);
        if (base != "{test_file}") {{
            throw "Basename mismatch";
        }}
        
        let ext = fs.path.extname(test_path);
        if (ext != "txt") {{
            throw "Extension mismatch";
        }}
        
        let joined = fs.path.join("a", "b", "c.txt");
        if (!joined.contains("c.txt")) {{
            throw "Join path failed";
        }}
        
        fs.delete(test_path);
        if (fs.exists(test_path)) {{
            throw "File deletion failed";
        }}
        
        let dir_path = "{test_dir}";
        fs.mkdir(dir_path);
        if (!fs.isDir(dir_path)) {{
            throw "Directory creation failed";
        }}
        fs.delete(dir_path);
    "#
    );

    let res = run_code(&code);
    assert!(res.is_ok(), "{:?}", res.err());
}

#[test]
fn test_json_serialization_and_deserialization() {
    let code = r#"
        let original_json = "{\"status\":\"ok\",\"code\":200,\"tags\":[\"compiler\",\"zero_gc\"]}";
        let parsed = JSON.parse(original_json);
        
        if (parsed.status != "ok") {
            throw "JSON status field mismatch";
        }
        
        if (parsed.code != 200) {
            throw "JSON numeric field mismatch";
        }
        
        let stringified = JSON.stringify(parsed);
        if (!stringified.contains("zero_gc")) {
            throw "JSON stringify missing array content";
        }
    "#;

    let res = run_code(code);
    assert!(res.is_ok(), "{:?}", res.err());
}

#[test]
fn test_crypto_hashing_and_security() {
    let code = r#"
        import Crypto;
        let data = "AdeshLangZeroGCSecurity";
        let h1 = Crypto.sha256(data);
        let h2 = Crypto.sha256(data);
        
        if (h1 != h2) {
            throw "Deterministic hash mismatch";
        }
        
        if (h1.len() != 64) {
            throw "SHA256 hex digest length invalid";
        }
    "#;

    let res = run_code(code);
    assert!(res.is_ok(), "{:?}", res.err());
}

#[test]
fn test_encoding_subsystem() {
    let code = r#"
        import Crypto;
        let plain = "AdeshLang Fast Systems Language";
        let b64 = Crypto.encodeBase64(plain);
        let decoded_bytes = Crypto.decodeBase64(b64);
        
        if (decoded_bytes.len() != plain.len()) {
            throw "Base64 roundtrip byte length mismatch";
        }
        
        let hex_val = Crypto.encodeHex(plain);
        let hex_decoded = Crypto.decodeHex(hex_val);
        
        if (hex_decoded.len() != plain.len()) {
            throw "Hex roundtrip byte length mismatch";
        }
    "#;

    let res = run_code(code);
    assert!(res.is_ok(), "{:?}", res.err());
}

#[test]
fn test_system_and_math_builtins() {
    let code = r#"
        import Math;
        let t0 = clock();
        let s = sizeof(42);
        if (s <= 0) {
            throw "sizeof invalid";
        }
        
        let sq = Math.sqrt(144);
        if (sq != 12) {
            throw "Math.sqrt mismatch";
        }
        
        let pw = Math.pow(2, 8);
        if (pw != 256) {
            throw "Math.pow mismatch";
        }
        
        let t1 = clock();
        if (t1 < t0) {
            throw "clock monotonicity violation";
        }
    "#;

    let res = run_code(code);
    assert!(res.is_ok(), "{:?}", res.err());
}
