//! Unified Stdlib Bridge for JIT/NJIT/AOT/WASM Backends
//!
//! Provides conversion between `RuntimeValue` (backend ABI) and `ast::Value` (stdlib ABI)
//! to allow ALL execution backends to call standard library modules directly.

use crate::backends::common::builtins::RuntimeValue;
use crate::parsing::ast::{NoopEnv, Value};
use crate::utils::collections::FastMap;

/// Convert a backend `RuntimeValue` to an AST `Value` for stdlib consumption
pub fn runtime_val_to_ast_val(rv: &RuntimeValue) -> Value {
    match rv {
        RuntimeValue::Int(n) => Value::Number(*n as f64),
        RuntimeValue::Float(n) => Value::Number(*n),
        RuntimeValue::Bool(b) => Value::Bool(*b),
        RuntimeValue::Char(c) => Value::Char(*c),
        RuntimeValue::String(s) => Value::Str(s.clone()),
        RuntimeValue::Array(arr) => {
            Value::Array(arr.iter().map(runtime_val_to_ast_val).collect())
        }
        RuntimeValue::Object(map) => {
            let mut res = std::collections::HashMap::default();
            for (k, v) in map.iter() {
                res.insert(k.clone(), runtime_val_to_ast_val(v));
            }
            Value::Object(std::sync::Arc::new(res))
        }
        RuntimeValue::Tuple(t) => {
            Value::Tuple(t.iter().map(runtime_val_to_ast_val).collect())
        }
        RuntimeValue::Set(s) => {
            Value::Set(s.iter().map(runtime_val_to_ast_val).collect())
        }
        RuntimeValue::U8(n) => Value::U8(*n),
        RuntimeValue::U16(n) => Value::U16(*n),
        RuntimeValue::U32(n) => Value::U32(*n),
        RuntimeValue::U64(n) => Value::U64(*n),
        RuntimeValue::U128(n) => Value::U128(*n),
        RuntimeValue::I8(n) => Value::I8(*n),
        RuntimeValue::I16(n) => Value::I16(*n),
        RuntimeValue::I32(n) => Value::I32(*n),
        RuntimeValue::I64(n) => Value::I64(*n),
        RuntimeValue::I128(n) => Value::I128(*n),
        RuntimeValue::F32(n) => Value::F32(*n),
        RuntimeValue::F64(n) => Value::F64(*n),
        RuntimeValue::Null => Value::Null,
        _ => Value::Null,
    }
}

/// Convert an AST `Value` returned from stdlib back to a `RuntimeValue`
pub fn ast_val_to_runtime_val(v: &Value) -> RuntimeValue {
    match v {
        Value::Number(n) => RuntimeValue::Float(*n),
        Value::Bool(b) => RuntimeValue::Bool(*b),
        Value::Char(c) => RuntimeValue::Char(*c),
        Value::Str(s) => RuntimeValue::String(s.clone()),
        Value::Array(arr) => {
            RuntimeValue::Array(arr.iter().map(ast_val_to_runtime_val).collect())
        }
        Value::Object(map) => {
            let mut res = FastMap::default();
            for (k, val) in map.iter() {
                res.insert(k.clone(), ast_val_to_runtime_val(val));
            }
            RuntimeValue::Object(res)
        }
        Value::Tuple(t) => {
            RuntimeValue::Tuple(t.iter().map(ast_val_to_runtime_val).collect())
        }
        Value::Set(s) => {
            RuntimeValue::Set(s.iter().map(ast_val_to_runtime_val).collect())
        }
        Value::U8(n) => RuntimeValue::U8(*n),
        Value::U16(n) => RuntimeValue::U16(*n),
        Value::U32(n) => RuntimeValue::U32(*n),
        Value::U64(n) => RuntimeValue::U64(*n),
        Value::U128(n) => RuntimeValue::U128(*n),
        Value::I8(n) => RuntimeValue::I8(*n),
        Value::I16(n) => RuntimeValue::I16(*n),
        Value::I32(n) => RuntimeValue::I32(*n),
        Value::I64(n) => RuntimeValue::I64(*n),
        Value::I128(n) => RuntimeValue::I128(*n),
        Value::F32(n) => RuntimeValue::F32(*n),
        Value::F64(n) => RuntimeValue::F64(*n),
        Value::Null => RuntimeValue::Null,
        _ => RuntimeValue::Null,
    }
}

/// Generic dispatcher for standard library functions
pub fn dispatch_stdlib_builtin(name: &str, args: &[RuntimeValue]) -> RuntimeValue {
    let stdlib = crate::runtime::stdlib_src::create_stdlib();

    let func = stdlib
        .get(name)
        .or_else(|| {
            if let Some((ns, method)) = name.split_once('.') {
                let alt_name = format!("{}.{}", ns.to_lowercase(), method);
                stdlib.get(&alt_name)
            } else {
                None
            }
        })
        .or_else(|| stdlib.get(&name.to_lowercase()));

    if let Some(func) = func {
        let ast_args: Vec<Value> = args.iter().map(runtime_val_to_ast_val).collect();
        let mut env = NoopEnv;
        match func(&mut env, ast_args) {
            Ok(ret) => ast_val_to_runtime_val(&ret),
            Err(_) => RuntimeValue::Null,
        }
    } else {
        RuntimeValue::Null
    }
}

macro_rules! stdlib_wrapper {
    ($fn_name:ident, $name_str:expr) => {
        pub fn $fn_name(args: &[RuntimeValue]) -> RuntimeValue {
            dispatch_stdlib_builtin($name_str, args)
        }
    };
}

// FS Wrappers
stdlib_wrapper!(runtime_fs_read_file, "fs.readFile");
stdlib_wrapper!(runtime_fs_write_file, "fs.writeFile");
stdlib_wrapper!(runtime_fs_exists, "fs.exists");
stdlib_wrapper!(runtime_fs_unlink, "fs.unlink");
stdlib_wrapper!(runtime_fs_mkdir, "fs.mkdir");
stdlib_wrapper!(runtime_fs_read_dir, "fs.readDir");
stdlib_wrapper!(runtime_fs_stat, "fs.stat");
stdlib_wrapper!(runtime_fs_copy_file, "fs.copyFile");

// Crypto Wrappers
stdlib_wrapper!(runtime_crypto_hash, "crypto.hash");
stdlib_wrapper!(runtime_crypto_sha256, "crypto.sha256");
stdlib_wrapper!(runtime_crypto_md5, "crypto.md5");
stdlib_wrapper!(runtime_crypto_random_bytes, "crypto.randomBytes");
stdlib_wrapper!(runtime_crypto_encrypt, "crypto.encrypt");
stdlib_wrapper!(runtime_crypto_decrypt, "crypto.decrypt");

// Path Wrappers
stdlib_wrapper!(runtime_path_join, "path.join");
stdlib_wrapper!(runtime_path_resolve, "path.resolve");
stdlib_wrapper!(runtime_path_dirname, "path.dirname");
stdlib_wrapper!(runtime_path_basename, "path.basename");
stdlib_wrapper!(runtime_path_extname, "path.extname");
stdlib_wrapper!(runtime_path_is_absolute, "path.isAbsolute");
stdlib_wrapper!(runtime_path_normalize, "path.normalize");

// HTTP & Network Wrappers
stdlib_wrapper!(runtime_http_get, "http.get");
stdlib_wrapper!(runtime_http_post, "http.post");
stdlib_wrapper!(runtime_http_request, "http.request");
stdlib_wrapper!(runtime_http_fetch, "http.fetch");
stdlib_wrapper!(runtime_dns_lookup, "dns.lookup");
stdlib_wrapper!(runtime_dns_resolve, "dns.resolve");

// System Wrappers
stdlib_wrapper!(runtime_system_env, "system.env");
stdlib_wrapper!(runtime_system_args, "system.args");
stdlib_wrapper!(runtime_system_os, "system.os");
stdlib_wrapper!(runtime_system_arch, "system.arch");
stdlib_wrapper!(runtime_system_memory_info, "system.memoryInfo");
stdlib_wrapper!(runtime_system_exit, "system.exit");

// Random & Encoding Wrappers
stdlib_wrapper!(runtime_random_random, "random.random");
stdlib_wrapper!(runtime_random_int, "random.int");
stdlib_wrapper!(runtime_random_float, "random.float");
stdlib_wrapper!(runtime_random_uuid, "random.uuid");

stdlib_wrapper!(runtime_encoding_base64_encode, "encoding.base64Encode");
stdlib_wrapper!(runtime_encoding_base64_decode, "encoding.base64Decode");
stdlib_wrapper!(runtime_encoding_hex_encode, "encoding.hexEncode");
stdlib_wrapper!(runtime_encoding_hex_decode, "encoding.hexDecode");

stdlib_wrapper!(runtime_compression_gzip_compress, "compression.gzipCompress");
stdlib_wrapper!(runtime_compression_gzip_decompress, "compression.gzipDecompress");

stdlib_wrapper!(runtime_url_parse, "url.parse");
stdlib_wrapper!(runtime_url_format, "url.format");
stdlib_wrapper!(runtime_tls_connect, "tls.connect");

stdlib_wrapper!(runtime_simd_vector_add, "simd.vectorAdd");
stdlib_wrapper!(runtime_simd_vector_mul, "simd.vectorMul");
