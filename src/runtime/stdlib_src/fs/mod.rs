use crate::parsing::ast::{BuiltinEnv, NativeEffect, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use rustc_hash::FxHashMap as HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

static WATCH_STATE: once_cell::sync::Lazy<Mutex<HashMap<String, f64>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::default()));

fn builtin_read_text(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.read(path)")?;
    let path = expect_str(&args[0], "path")?;
    let s = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    Ok(Value::Str(s))
}

fn builtin_write_text(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.write(path, text)")?;
    let path = expect_str(&args[0], "path")?;
    let text = expect_str(&args[1], "text")?;
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {}", path, e))?;
    Ok(Value::Null)
}

fn builtin_write_atomic(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.writeAtomic(path, text)")?;
    let path = expect_str(&args[0], "path")?;
    let text = expect_str(&args[1], "text")?;
    let tmp = format!("{}.tmp-{}", path, rand::random::<u64>());
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write temp {}: {}", tmp, e))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| format!("cannot rename {} -> {}: {}", tmp, path, e))?;
    Ok(Value::Null)
}

fn builtin_remove(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.delete(path)")?;
    let path = expect_str(&args[0], "path")?;
    let p = Path::new(&path);
    if p.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("cannot remove dir {}: {}", path, e))?
    } else {
        std::fs::remove_file(&path).map_err(|e| format!("cannot remove file {}: {}", path, e))?
    }
    Ok(Value::Null)
}

fn builtin_copy(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.copy(src, dst)")?;
    let src = expect_str(&args[0], "src")?;
    let dst = expect_str(&args[1], "dst")?;
    let n =
        std::fs::copy(&src, &dst).map_err(|e| format!("cannot copy {} -> {}: {}", src, dst, e))?;
    Ok(Value::Number(n as f64))
}

fn builtin_rename(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.move(src, dst)")?;
    let src = expect_str(&args[0], "src")?;
    let dst = expect_str(&args[1], "dst")?;
    std::fs::rename(&src, &dst).map_err(|e| format!("cannot move {} -> {}: {}", src, dst, e))?;
    Ok(Value::Null)
}

fn builtin_exists(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.exists(path)")?;
    let path = expect_str(&args[0], "path")?;
    Ok(Value::Bool(Path::new(&path).exists()))
}

fn builtin_is_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.isFile(path)")?;
    let path = expect_str(&args[0], "path")?;
    Ok(Value::Bool(Path::new(&path).is_file()))
}

fn builtin_is_dir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.isDir(path)")?;
    let path = expect_str(&args[0], "path")?;
    Ok(Value::Bool(Path::new(&path).is_dir()))
}

fn builtin_read_dir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.readDir(path)")?;
    let path = expect_str(&args[0], "path")?;
    let mut out = Vec::new();
    let iter = std::fs::read_dir(&path).map_err(|e| format!("cannot readDir {}: {}", path, e))?;
    for ent in iter.filter_map(|r| r.ok()) {
        if let Some(name) = ent.file_name().to_str() {
            out.push(Value::Str(name.to_string()))
        }
    }
    Ok(Value::Array(out))
}

fn builtin_mkdir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.mkdir(path)")?;
    let path = expect_str(&args[0], "path")?;
    std::fs::create_dir_all(&path).map_err(|e| format!("cannot mkdir {}: {}", path, e))?;
    Ok(Value::Null)
}

fn builtin_temp_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.tempFile(prefix)")?;
    let prefix = expect_str(&args[0], "prefix")?;
    let dir = std::env::temp_dir();
    let p = dir.join(format!("{}-{}", prefix, rand::random::<u64>()));
    Ok(Value::Str(p.to_string_lossy().to_string()))
}

fn builtin_temp_dir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.tempDir(prefix)")?;
    let prefix = expect_str(&args[0], "prefix")?;
    let dir = std::env::temp_dir().join(format!("{}-{}", prefix, rand::random::<u64>()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot temp dir {}: {}", dir.to_string_lossy(), e))?;
    Ok(Value::Str(dir.to_string_lossy().to_string()))
}

fn builtin_lock(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.lock(path)")?;
    let path = expect_str(&args[0], "path")?;
    let lockp = format!("{}.lock", path);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lockp);
    match f {
        Ok(_) => Ok(Value::Str(lockp)),
        Err(e) => Err(format!("cannot lock {}: {}", path, e)),
    }
}

fn builtin_unlock(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.unlock(path)")?;
    let lockp = expect_str(&args[0], "path")?;
    std::fs::remove_file(&lockp).map_err(|e| format!("cannot unlock {}: {}", lockp, e))?;
    Ok(Value::Null)
}

fn builtin_mmap_read(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.mmapRead(path)")?;
    let path = expect_str(&args[0], "path")?;
    let mut buf = Vec::new();
    let mut f = std::fs::File::open(&path).map_err(|e| format!("cannot open {}: {}", path, e))?;
    use std::io::Read;
    f.read_to_end(&mut buf)
        .map_err(|e| format!("cannot read {}: {}", path, e))?;
    let vals = buf.into_iter().map(|b| Value::U8(b)).collect::<Vec<_>>();
    Ok(Value::RawArray("u8".into(), vals))
}

fn builtin_crc32(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("fs.crc32(data)".into());
    }
    let mut table = [0u32; 256];
    for i in 0..256 {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB88320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        table[i] = c;
    }
    let mut crc: u32 = 0xFFFF_FFFF;
    match &args[0] {
        Value::Str(s) => {
            for b in s.as_bytes() {
                crc = table[((crc ^ (*b as u32)) & 0xFF) as usize] ^ (crc >> 8);
            }
        }
        Value::RawArray(_, a) | Value::Array(a) => {
            for v in a {
                let b = to_u8(v)?;
                crc = table[((crc ^ (b as u32)) & 0xFF) as usize] ^ (crc >> 8);
            }
        }
        _ => return Err("fs.crc32 expects string or bytes".into()),
    }
    let out = (!crc) as u32;
    Ok(Value::U32(out))
}

fn builtin_sha256(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("fs.sha256(data)".into());
    }
    let mut data: Vec<u8> = match &args[0] {
        Value::Str(s) => s.as_bytes().to_vec(),
        Value::RawArray(_, a) | Value::Array(a) => {
            a.iter().map(|v| to_u8(v)).collect::<Result<_, _>>()?
        }
        _ => return Err("fs.sha256 expects string or bytes".into()),
    };
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let l = (data.len() as u64) * 8;
    data.push(0x80);
    while (data.len() % 64) != 56 {
        data.push(0);
    }
    data.extend_from_slice(&l.to_be_bytes());
    for chunk in data.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::new();
    for i in 0..8 {
        out.push_str(&format!("{:08x}", h[i]));
    }
    Ok(Value::Str(out))
}

fn builtin_symlink_file(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.symlinkFile(src, link)")?;
    let src = expect_str(&args[0], "src")?;
    let link = expect_str(&args[1], "link")?;
    #[cfg(target_os = "windows")]
    {
        std::os::windows::fs::symlink_file(&src, &link)
            .map_err(|e| format!("symlinkFile {} -> {}: {}", src, link, e))?;
        Ok(Value::Null)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, &link)
            .map_err(|e| format!("symlinkFile {} -> {}: {}", src, link, e))?;
        Ok(Value::Null)
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (&src, &link);
        Err("symlinks not supported on this platform".to_string())
    }
}

fn builtin_symlink_dir(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.symlinkDir(src, link)")?;
    let src = expect_str(&args[0], "src")?;
    let link = expect_str(&args[1], "link")?;
    #[cfg(target_os = "windows")]
    {
        std::os::windows::fs::symlink_dir(&src, &link)
            .map_err(|e| format!("symlinkDir {} -> {}: {}", src, link, e))?;
        Ok(Value::Null)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, &link)
            .map_err(|e| format!("symlinkDir {} -> {}: {}", src, link, e))?;
        Ok(Value::Null)
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (&src, &link);
        Err("symlinks not supported on this platform".to_string())
    }
}

fn builtin_read_link(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.readLink(path)")?;
    let path = expect_str(&args[0], "path")?;
    let t = std::fs::read_link(&path).map_err(|e| format!("readLink {}: {}", path, e))?;
    Ok(Value::Str(t.to_string_lossy().to_string()))
}

fn builtin_watch(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.watch(path, fn)")?;
    let path = expect_str(&args[0], "path")?;
    let cb = args[1].clone();
    let id = env.alloc_timer_id()?;
    let mut last = 0.0f64;
    if let Ok(m) = std::fs::metadata(&path) {
        if let Ok(mt) = m.modified() {
            if let Ok(d) = mt.duration_since(std::time::UNIX_EPOCH) {
                last = d.as_secs_f64();
            }
        }
    }
    {
        let mut st = WATCH_STATE.lock().unwrap();
        st.insert(path.clone(), last);
    }
    let cb_exec = Value::Function(NativeFn(Arc::new(move |env2: &mut dyn BuiltinEnv, _| {
        let mut changed = false;
        let cur = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mt| mt.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        {
            let mut st = WATCH_STATE.lock().unwrap();
            if let Some(prev) = st.get(&path) {
                if cur > *prev {
                    changed = true;
                    st.insert(path.clone(), cur);
                }
            }
        }
        if changed {
            match cb.clone() {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(
                        env2,
                        vec![Value::Str("change".into()), Value::Str(path.clone())],
                    );
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = env2.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        let p = path.clone();
                        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
                            let interp = ienv
                                .as_any_mut()
                                .downcast_mut::<crate::execution::runtime::Interpreter>()
                                .unwrap();
                            let _ = interp.run_user_fn_in_interp(
                                u.clone(),
                                vec![Value::Str("change".into()), Value::Str(p.clone())],
                            );
                        })));
                    }
                }
                _ => {}
            }
        }
        Ok(Value::Null)
    })));
    let flag = Arc::new(AtomicBool::new(true));
    env.register_timer(id, cb_exec, flag.clone(), true, 500)?;
    Ok(Value::Number(id as f64))
}

fn builtin_unwatch(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.unwatch(id)")?;
    let id = expect_num(&args[0], "id")? as u64;
    env.cancel_timer(id)?;
    Ok(Value::Null)
}

fn builtin_path_join(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("fs.path.join(a, b, ...)".into());
    }
    let parts: Result<Vec<String>, _> = args.iter().map(|v| expect_str(v, "part")).collect();
    let parts = parts?;
    let mut p = PathBuf::new();
    for s in parts {
        p = p.join(s);
    }
    Ok(Value::Str(p.to_string_lossy().to_string()))
}

fn builtin_path_basename(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.path.basename(path)")?;
    let path = expect_str(&args[0], "path")?;
    let b = Path::new(&path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    Ok(Value::Str(b.to_string()))
}

fn builtin_path_dirname(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.path.dirname(path)")?;
    let path = expect_str(&args[0], "path")?;
    let d = Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or("".into());
    Ok(Value::Str(d))
}

fn builtin_path_extname(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.path.extname(path)")?;
    let path = expect_str(&args[0], "path")?;
    let e = Path::new(&path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    Ok(Value::Str(e.to_string()))
}

fn builtin_path_normalize(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.path.normalize(path)")?;
    let path = expect_str(&args[0], "path")?;
    let mut out = PathBuf::new();
    for comp in Path::new(&path).components() {
        out.push(comp);
    }
    Ok(Value::Str(out.to_string_lossy().to_string()))
}

fn builtin_async_read(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.async.read(path)")?;
    let path = expect_str(&args[0], "path")?;
    env.create_promise_executor(Value::Function(NativeFn(Arc::new(move |benv, a| {
        let resolve = a.get(0).cloned().unwrap_or(Value::Null);
        let reject = a.get(1).cloned().unwrap_or(Value::Null);
        let res = std::fs::read_to_string(&path);
        match res {
            Ok(s) => match resolve {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(benv, vec![Value::Str(s)]);
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = benv.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
                            let interp = ienv
                                .as_any_mut()
                                .downcast_mut::<crate::execution::runtime::Interpreter>()
                                .unwrap();
                            let _ = interp
                                .run_user_fn_in_interp(u.clone(), vec![Value::Str(s.clone())]);
                        })));
                    }
                }
                _ => {}
            },
            Err(e) => match reject {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(benv, vec![Value::Str(e.to_string())]);
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = benv.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
                            let interp = ienv
                                .as_any_mut()
                                .downcast_mut::<crate::execution::runtime::Interpreter>()
                                .unwrap();
                            let _ = interp
                                .run_user_fn_in_interp(u.clone(), vec![Value::Str(e.to_string())]);
                        })));
                    }
                }
                _ => {}
            },
        }
        Ok(Value::Null)
    }))))
}

fn builtin_async_write(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 2, "fs.async.write(path, text)")?;
    let path = expect_str(&args[0], "path")?;
    let text = expect_str(&args[1], "text")?;
    env.create_promise_executor(Value::Function(NativeFn(Arc::new(move |benv, a| {
        let resolve = a.get(0).cloned().unwrap_or(Value::Null);
        let reject = a.get(1).cloned().unwrap_or(Value::Null);
        let res = std::fs::write(&path, text.clone());
        match res {
            Ok(_) => match resolve {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(benv, vec![Value::Null]);
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = benv.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
                            let interp = ienv
                                .as_any_mut()
                                .downcast_mut::<crate::execution::runtime::Interpreter>()
                                .unwrap();
                            let _ = interp.run_user_fn_in_interp(u.clone(), vec![Value::Null]);
                        })));
                    }
                }
                _ => {}
            },
            Err(e) => match reject {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(benv, vec![Value::Str(e.to_string())]);
                }
                Value::UserFunction(u) => {
                    if let Some(ns) = benv.native_side_effects() {
                        let mut q = ns.lock().unwrap();
                        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
                            let interp = ienv
                                .as_any_mut()
                                .downcast_mut::<crate::execution::runtime::Interpreter>()
                                .unwrap();
                            let _ = interp
                                .run_user_fn_in_interp(u.clone(), vec![Value::Str(e.to_string())]);
                        })));
                    }
                }
                _ => {}
            },
        }
        Ok(Value::Null)
    }))))
}

fn builtin_fsql(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 1 {
        return Err("fs.fsql(path, [ext])".into());
    }
    let path = expect_str(&args[0], "path")?;
    let extf = if args.len() > 1 {
        Some(expect_str(&args[1], "ext")?)
    } else {
        None
    };
    let mut rows: Vec<Value> = Vec::new();
    let iter = std::fs::read_dir(&path).map_err(|e| format!("fsql readDir {}: {}", path, e))?;
    for ent in iter.filter_map(|r| r.ok()) {
        let p = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        let is_file = p.is_file();
        let ex = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if let Some(extwanted) = &extf {
            if &ex != extwanted {
                continue;
            }
        }
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let mut row: HashMap<String, Value> = HashMap::default();
        row.insert("name".into(), Value::Str(name));
        row.insert("ext".into(), Value::Str(ex));
        row.insert("isFile".into(), Value::Bool(is_file));
        row.insert("size".into(), Value::Number(size as f64));
        rows.push(Value::Object(Arc::new(row)));
    }
    Ok(Value::Array(rows))
}

fn builtin_tx(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.tx(fn)")?;
    let cb = args[0].clone();
    let log = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
    if let Some(ns) = env.native_side_effects() {
        let mut q = ns.lock().unwrap();
        let log2 = log.clone();
        q.push(NativeEffect::EnqueueMicrotask(Box::new(move |ienv| {
            let interp = ienv
                .as_any_mut()
                .downcast_mut::<crate::execution::runtime::Interpreter>()
                .unwrap();
            let mut methods: HashMap<String, Value> = HashMap::default();
            methods.insert(
                "write".into(),
                Value::Function(NativeFn(Arc::new({
                    let log3 = log2.clone();
                    move |_e, a| {
                        let p = expect_str(a.get(0).ok_or("arg")?, "path")?;
                        let t = expect_str(a.get(1).ok_or("arg")?, "text")?;
                        std::fs::write(&p, t).map_err(|e| e.to_string())?;
                        log3.lock().unwrap().push(("write".into(), vec![p]));
                        Ok(Value::Null)
                    }
                }))),
            );
            methods.insert(
                "rename".into(),
                Value::Function(NativeFn(Arc::new({
                    let log3 = log2.clone();
                    move |_e, a| {
                        let s = expect_str(a.get(0).ok_or("arg")?, "src")?;
                        let d = expect_str(a.get(1).ok_or("arg")?, "dst")?;
                        std::fs::rename(&s, &d).map_err(|e| e.to_string())?;
                        log3.lock().unwrap().push(("rename".into(), vec![s, d]));
                        Ok(Value::Null)
                    }
                }))),
            );
            methods.insert(
                "remove".into(),
                Value::Function(NativeFn(Arc::new({
                    let log3 = log2.clone();
                    move |_e, a| {
                        let p = expect_str(a.get(0).ok_or("arg")?, "path")?;
                        let md = std::fs::metadata(&p).ok();
                        let backup = if md.map(|m| m.is_file()).unwrap_or(false) {
                            std::fs::read_to_string(&p).ok()
                        } else {
                            None
                        };
                        if Path::new(&p).is_dir() {
                            std::fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
                        } else {
                            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
                        }
                        let mut rec = vec![p];
                        if let Some(b) = backup {
                            rec.push(b);
                        }
                        log3.lock().unwrap().push(("remove".into(), rec));
                        Ok(Value::Null)
                    }
                }))),
            );
            methods.insert(
                "commit".into(),
                Value::Function(NativeFn(Arc::new(move |_e, _| Ok(Value::Null)))),
            );
            methods.insert(
                "rollback".into(),
                Value::Function(NativeFn(Arc::new({
                    let log4 = log2.clone();
                    move |_e, _| {
                        let ops = log4.lock().unwrap().clone();
                        for (op, params) in ops.into_iter().rev() {
                            match op.as_str() {
                                "write" => {
                                    let _ = std::fs::remove_file(&params[0]);
                                }
                                "rename" => {
                                    if params.len() == 2 {
                                        let _ = std::fs::rename(&params[1], &params[0]);
                                    }
                                }
                                "remove" => {
                                    if params.len() == 2 {
                                        let _ = std::fs::write(&params[0], &params[1]);
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                }))),
            );
            let txobj = Value::Object(Arc::new(methods));
            match cb.clone() {
                Value::Function(NativeFn(f)) => {
                    let _ = (f)(&mut *interp, vec![txobj]);
                }
                Value::UserFunction(u) => {
                    let _ = interp.run_user_fn_in_interp(u.clone(), vec![txobj]);
                }
                _ => {}
            }
        })));
    }
    Ok(Value::Null)
}

fn builtin_snapshot(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.snapshot(name)")?;
    let name = expect_str(&args[0], "name")?;
    let mut map: HashMap<String, Value> = HashMap::default();
    let base = std::env::current_dir().unwrap_or(PathBuf::from("."));
    let mut stack = vec![base];
    while let Some(dir) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.filter_map(|r| r.ok()) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file() {
                    let sz = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    map.insert(p.to_string_lossy().to_string(), Value::Number(sz as f64));
                }
            }
        }
    }
    let mut out: HashMap<String, Value> = HashMap::default();
    out.insert("name".into(), Value::Str(name));
    out.insert("files".into(), Value::Object(Arc::new(map)));
    Ok(Value::Object(Arc::new(out)))
}

fn builtin_batch(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    ensure_arity(args.len(), 1, "fs.batch(ops)")?;
    let ops = match &args[0] {
        Value::Array(v) => v.clone(),
        _ => return Err("fs.batch expects array".into()),
    };
    let mut results: Vec<Value> = Vec::new();
    for opv in ops {
        if let Value::Object(m) = opv {
            let mut kind = String::new();
            let mut path = String::new();
            let mut a2 = String::new();
            if let Some(Value::Str(k)) = m.get("op") {
                kind = k.clone();
            }
            if let Some(Value::Str(p)) = m.get("path") {
                path = p.clone();
            }
            if let Some(Value::Str(d)) = m.get("data") {
                a2 = d.clone();
            }
            let r = match kind.as_str() {
                "write" => std::fs::write(&path, a2)
                    .map(|_| Value::Null)
                    .map_err(|e| e.to_string()),
                "read" => std::fs::read_to_string(&path)
                    .map(Value::Str)
                    .map_err(|e| e.to_string()),
                "delete" => {
                    let p = Path::new(&path);
                    if p.is_dir() {
                        std::fs::remove_dir_all(&path)
                            .map(|_| Value::Null)
                            .map_err(|e| e.to_string())
                    } else {
                        std::fs::remove_file(&path)
                            .map(|_| Value::Null)
                            .map_err(|e| e.to_string())
                    }
                }
                _ => Err("unknown op".into()),
            };
            match r {
                Ok(v) => results.push(v),
                Err(s) => results.push(Value::Str(s)),
            }
        }
    }
    Ok(Value::Array(results))
}

fn ensure_arity(actual: usize, expected: usize, sig: &str) -> Result<(), String> {
    if actual != expected {
        Err(sig.to_string())
    } else {
        Ok(())
    }
}
fn expect_str(v: &Value, name: &str) -> Result<String, String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Instance(inst) => {
            if let Ok(fields) = inst.fields.read() {
                if let Some(val) = fields.get("_raw").or_else(|| fields.get("_path")) {
                    if let Value::Str(s) = val {
                        return Ok(s.clone());
                    }
                }
            }
            Ok(inst.class_name.clone())
        }
        Value::Object(map) => {
            if let Some(val) = map.get("_raw").or_else(|| map.get("_path")) {
                if let Value::Str(s) = val {
                    return Ok(s.clone());
                }
            }
            Err(format!("{} must be string or Path", name))
        }
        _ => Err(format!("{} must be string", name)),
    }
}
fn expect_num(v: &Value, name: &str) -> Result<f64, String> {
    if let Value::Number(n) = v {
        Ok(*n)
    } else {
        Err(format!("{} must be number", name))
    }
}
fn to_u8(v: &Value) -> Result<u8, String> {
    match v {
        Value::U8(b) => Ok(*b),
        Value::Number(n) => Ok(*n as u8),
        Value::I8(b) => Ok(*b as u8),
        Value::Str(s) => {
            if s.len() == 1 {
                Ok(s.as_bytes()[0])
            } else {
                Err("byte from string len!=1".into())
            }
        }
        _ => Err("unsupported byte value".into()),
    }
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register("fs", "fs", "Filesystem namespace", |_env, _| {
        let mut methods = HashMap::default();
        methods.insert(
            "read".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_read_text))),
        );
        methods.insert(
            "readText".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_read_text))),
        );
        methods.insert(
            "write".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_write_text))),
        );
        methods.insert(
            "writeText".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_write_text))),
        );
        methods.insert(
            "writeAtomic".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_write_atomic))),
        );
        methods.insert(
            "delete".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_remove))),
        );
        methods.insert(
            "copy".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_copy))),
        );
        methods.insert(
            "move".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_rename))),
        );
        methods.insert(
            "exists".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_exists))),
        );
        methods.insert(
            "isFile".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_is_file))),
        );
        methods.insert(
            "isDir".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_is_dir))),
        );
        methods.insert(
            "readDir".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_read_dir))),
        );
        methods.insert(
            "mkdir".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_mkdir))),
        );
        methods.insert(
            "tempFile".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_temp_file))),
        );
        methods.insert(
            "tempDir".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_temp_dir))),
        );
        methods.insert(
            "lock".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_lock))),
        );
        methods.insert(
            "unlock".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_unlock))),
        );
        methods.insert(
            "mmapRead".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_mmap_read))),
        );
        methods.insert(
            "crc32".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_crc32))),
        );
        methods.insert(
            "sha256".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_sha256))),
        );
        methods.insert(
            "symlinkFile".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_symlink_file))),
        );
        methods.insert(
            "symlinkDir".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_symlink_dir))),
        );
        methods.insert(
            "readLink".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_read_link))),
        );
        methods.insert(
            "watch".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_watch))),
        );
        methods.insert(
            "unwatch".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_unwatch))),
        );
        methods.insert(
            "tx".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_tx))),
        );
        methods.insert(
            "snapshot".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_snapshot))),
        );
        methods.insert(
            "batch".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_batch))),
        );
        methods.insert(
            "fsql".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_fsql))),
        );

        let mut path_methods = HashMap::default();
        path_methods.insert(
            "join".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_path_join))),
        );
        path_methods.insert(
            "basename".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_path_basename))),
        );
        path_methods.insert(
            "dirname".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_path_dirname))),
        );
        path_methods.insert(
            "extname".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_path_extname))),
        );
        path_methods.insert(
            "normalize".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_path_normalize))),
        );
        methods.insert("path".to_string(), Value::Object(Arc::new(path_methods)));

        let mut async_methods = HashMap::default();
        async_methods.insert(
            "read".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_async_read))),
        );
        async_methods.insert(
            "write".to_string(),
            Value::Function(NativeFn(Arc::new(builtin_async_write))),
        );
        methods.insert("async".to_string(), Value::Object(Arc::new(async_methods)));

        Ok(Value::Object(Arc::new(methods)))
    });
    registry.register("fs.async", "fs", "Async filesystem namespace", |_env, _| {
        Ok(Value::Null)
    });
    registry.register("fs.path", "fs", "Path utilities", |_env, _| Ok(Value::Null));
    registry.register("fs.vfs", "fs", "Virtual filesystem", |_env, _| {
        Ok(Value::Null)
    });
    registry.register("fs.tx", "fs", "Transactional filesystem", builtin_tx);
    registry.register(
        "fs.snapshot",
        "fs",
        "Create filesystem snapshot",
        builtin_snapshot,
    );
    registry.register(
        "fs.batch",
        "fs",
        "Batch filesystem operations",
        builtin_batch,
    );

    registry.register("fs.read", "fs", "Read file as string", builtin_read_text);
    registry.register("fs.write", "fs", "Write string to file", builtin_write_text);
    registry.register(
        "fs.readText",
        "fs",
        "Read file as string",
        builtin_read_text,
    );
    registry.register(
        "fs.writeText",
        "fs",
        "Write string to file",
        builtin_write_text,
    );
    registry.register(
        "fs.writeAtomic",
        "fs",
        "Atomic write string to file",
        builtin_write_atomic,
    );
    registry.register(
        "fs.delete",
        "fs",
        "Delete file or directory",
        builtin_remove,
    );
    registry.register("fs.copy", "fs", "Copy file", builtin_copy);
    registry.register("fs.move", "fs", "Move/rename path", builtin_rename);
    registry.register("fs.exists", "fs", "Path exists", builtin_exists);
    registry.register("fs.isFile", "fs", "Is file", builtin_is_file);
    registry.register("fs.isDir", "fs", "Is directory", builtin_is_dir);
    registry.register(
        "fs.readDir",
        "fs",
        "List directory entries",
        builtin_read_dir,
    );
    registry.register(
        "fs.mkdir",
        "fs",
        "Create directory (recursive)",
        builtin_mkdir,
    );
    registry.register(
        "fs.tempFile",
        "fs",
        "Create temp file path",
        builtin_temp_file,
    );
    registry.register("fs.tempDir", "fs", "Create temp dir path", builtin_temp_dir);
    registry.register("fs.lock", "fs", "Lock file via sidecar", builtin_lock);
    registry.register("fs.unlock", "fs", "Unlock file via sidecar", builtin_unlock);
    registry.register(
        "fs.mmapRead",
        "fs",
        "Pseudo-mmap as bytes",
        builtin_mmap_read,
    );
    registry.register("fs.crc32", "fs", "CRC32 of bytes/string", builtin_crc32);
    registry.register("fs.sha256", "fs", "SHA256 of bytes/string", builtin_sha256);
    registry.register(
        "fs.symlinkFile",
        "fs",
        "Create file symlink",
        builtin_symlink_file,
    );
    registry.register(
        "fs.symlinkDir",
        "fs",
        "Create dir symlink",
        builtin_symlink_dir,
    );
    registry.register(
        "fs.readLink",
        "fs",
        "Read symlink target",
        builtin_read_link,
    );
    registry.register("fs.watch", "fs", "Watch path and callback", builtin_watch);
    registry.register("fs.unwatch", "fs", "Cancel watch by id", builtin_unwatch);

    registry.register("fs.path.join", "fs", "Join paths", builtin_path_join);
    registry.register("fs.path.basename", "fs", "Basename", builtin_path_basename);
    registry.register("fs.path.dirname", "fs", "Dirname", builtin_path_dirname);
    registry.register("fs.path.extname", "fs", "Extname", builtin_path_extname);
    registry.register(
        "fs.path.normalize",
        "fs",
        "Normalize",
        builtin_path_normalize,
    );

    registry.register(
        "fs.async.read",
        "fs",
        "Async read as Promise",
        builtin_async_read,
    );
    registry.register(
        "fs.async.write",
        "fs",
        "Async write as Promise",
        builtin_async_write,
    );

    registry.register("fs.fsql", "fs", "Filesystem query", builtin_fsql);
}
