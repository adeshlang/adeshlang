//! Phase 19.10–19.12: Failure/Panic Safety, Concurrency/Performance, and Security/Fuzz Tests.
//!
//! 19.10: FFI panic containment & invalid/corrupt input handling in `sys_io`, `stream_io`, `unwind_abi`.
//! 19.11: Multi-threaded handle table & ARC/Weak concurrency stress + throughput sanity checks.
//! 19.12: Deterministic fuzzing & boundary probes across Lexer -> Parser -> HIR -> TypeVerify ->
//!        CFG -> BorrowChecker, ADOB/ADBB parsers, ALSD LSDA parser, and runtime handle/pointer guards.

use std::sync::{Arc, Barrier};
use std::time::Instant;

use adesh_object::bundle::AdobBundle;
use adesh_object::reader::AdobReader;
use adesh_runtime::stream_io::{
    io_bytes_concat, io_pack_number, io_slice_bytes, io_unpack_number, io_utf8_decode,
    io_utf8_encode,
};
use adesh_runtime::sys_io::{
    processGetInfo, processKill, processReadBytes, processReadLine, processReadStderr,
    processReadStdout, processSpawn, processTerminate, processWait, processWaitTimeout,
    processWriteLine, processWriteStdin, sys_bind, sys_close, sys_connect, sys_open, sys_read,
    sys_seek, sys_spawn, sys_wait, sys_write,
};
use adesh_runtime::unwind_abi::parse_alsd_entries;
use adesh_runtime::{
    HANDLE_TAG, HANDLE_TAG_MASK, RuntimeValue, adesh_rt_arc_clone, adesh_rt_arc_drop,
    adesh_rt_arc_get, adesh_rt_arc_new, adesh_rt_arc_set, adesh_rt_arc_strong_count,
    adesh_rt_arc_weak_count, adesh_rt_check_bounds, adesh_rt_enum_check_tag, adesh_rt_weak_clone,
    adesh_rt_weak_drop, adesh_rt_weak_new, adesh_rt_weak_upgrade, aot_free_handle, aot_get_value,
    aot_store_value, aot_update_value, is_handle_tagged, try_cstr_abi_string,
};
use adeshlang::parsing::cfg_borrow::{CfgBorrowChecker, CfgBuilder, validate_cfg};
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::hir_type_verify::{verify_hir_module, verify_typed_hir_strict};
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;

#[derive(Clone, Copy)]
struct Rng64(u64);

impl Rng64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

// ============================================================================
// 19.10: Failure Modes, FFI Panic Containment, and Invalid Input Guards
// ============================================================================

#[test]
fn phase19_10_sys_io_and_process_ffi_invalid_inputs_never_panic() {
    // Empty / invalid paths, bad fds, huge counts (OOM cap), negative seek offsets.
    assert_eq!(sys_open(0, 0, 0), -1);
    assert_eq!(sys_read(-999, 0, 64), -1);
    assert_eq!(sys_read(0, 0, usize::MAX), -1);
    assert_eq!(sys_read(-999, 0, usize::MAX), -1);
    assert_eq!(sys_write(-999, 0, 16), -1);
    assert_eq!(sys_close(-999), -1);
    assert_eq!(sys_seek(-999, 0, 0), -1);
    assert_eq!(sys_seek(-999, -10, 0), -1);
    assert_eq!(sys_seek(-999, 0, 99), -1);
    assert_eq!(sys_bind(-999, 0, 0), -1);
    assert_eq!(sys_connect(-999, 0, 1), -1);
    assert_eq!(sys_spawn(0, 0), -1);
    assert_eq!(sys_wait(-999), -1);

    // Non-existent command & invalid process handles must return sentinel errors cleanly.
    let bad_cmd = aot_store_value(RuntimeValue::String(
        "/nonexistent/adesh_phase19_binary_xyz".to_string(),
    ));
    assert_eq!(sys_spawn(bad_cmd, 0), -1);
    assert_eq!(processSpawn(bad_cmd, 0, 0, 0, 0), 0);
    aot_free_handle(bad_cmd);

    assert_eq!(processKill(9_999_999, 15), 0);
    assert_eq!(processTerminate(9_999_999), 0);
    assert_eq!(processReadStdout(9_999_999), 0);
    assert_eq!(processReadStderr(9_999_999), 0);
    assert_eq!(processWriteStdin(9_999_999, 0), 0);
    assert_eq!(processWriteLine(9_999_999, 0), 0);
    assert_eq!(processReadLine(9_999_999), 0);
    assert_eq!(processReadBytes(9_999_999, usize::MAX as u64), 0);

    let wait_res = processWait(9_999_999);
    assert_ne!(wait_res, 0);
    aot_free_handle(wait_res);

    let wait_to_res = processWaitTimeout(9_999_999, 1);
    assert_ne!(wait_to_res, 0);
    aot_free_handle(wait_to_res);

    let info = processGetInfo(0);
    assert_ne!(info, 0);
    aot_free_handle(info);
}

#[test]
fn phase19_10_stream_io_overflow_and_invalid_type_guards() {
    let bytes_h = aot_store_value(RuntimeValue::Array(vec![
        RuntimeValue::U8(1),
        RuntimeValue::U8(2),
        RuntimeValue::U8(3),
    ]));
    // Integer-overflow start + len must saturate safely and return an empty array.
    let sliced = io_slice_bytes(bytes_h, (usize::MAX - 4) as u64, 100);
    match aot_get_value(sliced) {
        Some(RuntimeValue::Array(arr)) => assert!(arr.is_empty()),
        other => panic!("expected empty array, got {other:?}"),
    }
    aot_free_handle(sliced);

    let sliced_ok = io_slice_bytes(bytes_h, 1, 100);
    match aot_get_value(sliced_ok) {
        Some(RuntimeValue::Array(arr)) => assert_eq!(arr.len(), 2),
        other => panic!("expected 2-element slice, got {other:?}"),
    }
    aot_free_handle(sliced_ok);

    let concat_h = io_bytes_concat(bytes_h, 0);
    match aot_get_value(concat_h) {
        Some(RuntimeValue::Array(arr)) => assert_eq!(arr.len(), 3),
        other => panic!("expected 3-element concat, got {other:?}"),
    }
    aot_free_handle(concat_h);

    let bad_ty = aot_store_value(RuntimeValue::String("unknown_ty".to_string()));
    let le = aot_store_value(RuntimeValue::String("le".to_string()));
    let bad_endian = aot_store_value(RuntimeValue::String("middle".to_string()));

    let packed_bad_ty = io_pack_number(bad_ty, 42, le);
    let packed_bad_endian = io_pack_number(bad_ty, 42, bad_endian);
    assert_eq!(io_unpack_number(bad_ty, bytes_h, le), 0);
    assert_eq!(io_unpack_number(bad_ty, bytes_h, bad_endian), 0);

    let s = aot_store_value(RuntimeValue::String("hello".to_string()));
    let utf8_bytes = io_utf8_encode(s);
    let decoded = io_utf8_decode(utf8_bytes);
    assert_eq!(
        aot_get_value(decoded).map(|v| v.as_string()),
        Some("hello".to_string())
    );

    for h in [
        bytes_h,
        bad_ty,
        le,
        bad_endian,
        packed_bad_ty,
        packed_bad_endian,
        s,
        utf8_bytes,
        decoded,
    ] {
        aot_free_handle(h);
    }
}

// ============================================================================
// 19.11: Concurrency, Atomic Stress & Performance Sanity
// ============================================================================

#[test]
fn phase19_11_concurrent_handle_shards_stress() {
    const THREADS: usize = 8;
    const ITERS: usize = 400;
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|tid| {
            let b = Arc::clone(&barrier);
            std::thread::spawn(move || {
                b.wait();
                for iter in 0..ITERS {
                    let expected = (tid as i64) * 100_000 + (iter as i64);
                    let h = aot_store_value(RuntimeValue::Int(expected));
                    assert!(is_handle_tagged(h), "handle must carry tag");
                    match aot_get_value(h) {
                        Some(RuntimeValue::Int(v)) => assert_eq!(v, expected),
                        other => panic!("unexpected value for handle: {other:?}"),
                    }
                    aot_update_value(h, RuntimeValue::Int(expected + 1));
                    match aot_get_value(h) {
                        Some(RuntimeValue::Int(v)) => assert_eq!(v, expected + 1),
                        other => panic!("unexpected updated value: {other:?}"),
                    }
                    aot_free_handle(h);
                    assert!(aot_get_value(h).is_none());
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("handle stress worker panicked");
    }
}

#[test]
fn phase19_11_concurrent_arc_and_weak_race_stress() {
    for round in 0..40 {
        let init = aot_store_value(RuntimeValue::Int(round));
        let arc_id = adesh_rt_arc_new(init);
        aot_free_handle(init);
        assert_ne!(arc_id, 0);

        const THREADS: usize = 8;
        let barrier = Arc::new(Barrier::new(THREADS));

        let workers: Vec<_> = (0..THREADS)
            .map(|tid| {
                let b = Arc::clone(&barrier);
                // Pre-give each thread its own strong and weak ref so drops are balanced.
                assert_eq!(adesh_rt_arc_clone(arc_id), arc_id);
                let w = adesh_rt_weak_new(arc_id);
                assert_eq!(w, arc_id);
                std::thread::spawn(move || {
                    b.wait();
                    for step in 0..50 {
                        let got = adesh_rt_arc_get(arc_id);
                        aot_free_handle(got);

                        if step % 3 == 0 {
                            let v = aot_store_value(RuntimeValue::Int((tid * 100 + step) as i64));
                            adesh_rt_arc_set(arc_id, v);
                            aot_free_handle(v);
                        }

                        let w2 = adesh_rt_weak_clone(w);
                        if w2 != 0 {
                            let up = adesh_rt_weak_upgrade(w2);
                            if up != 0 {
                                assert_eq!(up, arc_id);
                                adesh_rt_arc_drop(up);
                            }
                            adesh_rt_weak_drop(w2);
                        }
                    }
                    // Drop our pre-acquired strong ref, then try upgrading after potential death.
                    adesh_rt_arc_drop(arc_id);
                    let post = adesh_rt_weak_upgrade(w);
                    if post != 0 {
                        adesh_rt_arc_drop(post);
                    }
                    adesh_rt_weak_drop(w);
                })
            })
            .collect();

        for w in workers {
            w.join().expect("arc/weak worker panicked");
        }

        // Drop the initial strong ref. Once strong == 0, clone is a no-op (never revives) & upgrade returns 0.
        adesh_rt_arc_drop(arc_id);
        assert_eq!(adesh_rt_arc_strong_count(arc_id), 0);
        assert_eq!(adesh_rt_arc_weak_count(arc_id), 0);
        assert_eq!(adesh_rt_arc_clone(arc_id), arc_id);
        assert_eq!(adesh_rt_arc_strong_count(arc_id), 0);
        assert_eq!(adesh_rt_weak_upgrade(arc_id), 0);
    }
}

#[test]
fn phase19_11_handle_and_arc_throughput_sanity() {
    let start = Instant::now();
    for i in 0..10_000i64 {
        let h = aot_store_value(RuntimeValue::Int(i));
        let _ = aot_get_value(h);
        aot_free_handle(h);
    }
    let handle_elapsed = start.elapsed();

    let start_arc = Instant::now();
    for i in 0..5_000i64 {
        let v = aot_store_value(RuntimeValue::Int(i));
        let a = adesh_rt_arc_new(v);
        aot_free_handle(v);
        let c = adesh_rt_arc_clone(a);
        assert_eq!(c, a);
        adesh_rt_arc_drop(c);
        adesh_rt_arc_drop(a);
    }
    let arc_elapsed = start_arc.elapsed();

    assert!(
        handle_elapsed.as_secs_f64() < 5.0,
        "10k handle cycles took too long: {handle_elapsed:?}"
    );
    assert!(
        arc_elapsed.as_secs_f64() < 5.0,
        "5k ARC cycles took too long: {arc_elapsed:?}"
    );
}

// ============================================================================
// 19.12: Security, Fuzzing & Boundary Probes
// ============================================================================

#[test]
fn phase19_12_frontend_pipeline_fuzz_zero_panics() {
    let curated = [
        "",
        "fn main() { let x = 1; }",
        "fn f(x: int) -> int { match x { 0 => 1, _ => 2 } }",
        "enum E { A, B(int), C(int, string) } fn f(e: E) -> int { match e { E::A => 1, E::B(x) => x, _ => 0 } }",
        "fn f() { try { throw \"err\"; } catch (e) { let y = e; } defer { let z = 1; } }",
        "fn f() { let a = 1; let c = fn(x: int) -> int { return x + a; }; }",
        "fn bad( { let x = (((((; if true { else } }",
        "match 1 { => => => }",
        "enum { } struct { } class : { }",
        "\"unterminated \\x \\u{FFFFFFFF} \\0",
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "1e9999999999999999999999999999999999",
        &"(".repeat(128),
        &"{ let x = 1; ".repeat(64),
    ];

    let tokens_vocab = [
        "fn", "let", "mut", "if", "else", "while", "for", "in", "return", "break", "continue",
        "match", "enum", "struct", "class", "try", "catch", "throw", "defer", "move", "true",
        "false", "null", "0", "-1", "3.14", "\"s\"", "(", ")", "{", "}", "[", "]", ";", ":", ",",
        ".", "=>", "->", "=", "==", "+", "-", "*", "/", "&&", "||", "&", "!", "::", "x", "y", "A",
        "int", "string",
    ];

    let mut inputs: Vec<String> = curated.iter().map(|s| (*s).to_string()).collect();
    let mut rng = Rng64(0x1912_CAFE_BEEF_0001);
    for _ in 0..200 {
        let len = 1 + rng.gen_range(28);
        let mut s = String::new();
        for _ in 0..len {
            let tok = tokens_vocab[rng.gen_range(tokens_vocab.len())];
            s.push_str(tok);
            if rng.gen_range(3) != 0 {
                s.push(' ');
            }
        }
        inputs.push(s);
    }

    for (idx, src) in inputs.into_iter().enumerate() {
        let handle = std::thread::Builder::new()
            .name(format!("p19_12_fuzz_{idx}"))
            .spawn(move || {
                let mut lexer = Lexer::new(&src);
                if let Ok(tokens) = lexer.tokenize() {
                    let mut parser = Parser::new(tokens, None);
                    if let Ok(stmts) = parser.parse_program() {
                        if let Ok(hir) = ast_to_hir(&stmts, false) {
                            let _ = verify_hir_module(&hir);
                            let _ = verify_typed_hir_strict(&hir);
                            for func in &hir.functions {
                                let mut cfg = CfgBuilder::build(func);
                                let _ = validate_cfg(&cfg);
                                let mut checker = CfgBorrowChecker::new();
                                let _ = checker.analyze(&mut cfg);
                            }
                        }
                    }
                }
            })
            .expect("spawn fuzz thread");

        handle
            .join()
            .unwrap_or_else(|e| panic!("frontend pipeline panicked on corpus #{idx}: {e:?}"));
    }
}

#[test]
fn phase19_12_adob_adbb_and_alsd_fuzz_zero_panics() {
    let mut rng = Rng64(0xAD0B_ADBB_1912_7777);

    // 1. Corrupt ADOB and ADBB inputs
    let mut base_adob = b"ADOB\x01\x00\x00\x00".to_vec();
    base_adob.extend_from_slice(&[0u8; 64]);

    let mut base_adbb = b"ADBB\x01\x00\x00\x00".to_vec();
    base_adbb.extend_from_slice(&[0u8; 64]);

    for i in 0..200 {
        let mut buf = if i % 2 == 0 {
            base_adob.clone()
        } else {
            base_adbb.clone()
        };
        let mutations = 1 + rng.gen_range(12);
        for _ in 0..mutations {
            let pos = rng.gen_range(buf.len());
            buf[pos] = (rng.next_u64() & 0xff) as u8;
        }
        if rng.gen_range(4) == 0 {
            let trunc = rng.gen_range(buf.len());
            buf.truncate(trunc);
        }
        let _ = AdobReader::read_object(&buf);
        let _ = AdobBundle::read(&buf);
    }

    // 2. Corrupt ALSD LSDA tables including integer overflow counts & invalid kinds
    let mut overflow_alsd = Vec::new();
    overflow_alsd.extend_from_slice(b"ALSD");
    overflow_alsd.extend_from_slice(&1u32.to_le_bytes());
    overflow_alsd.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_alsd_entries(&overflow_alsd).is_none());

    let mut bad_kind_alsd = Vec::new();
    bad_kind_alsd.extend_from_slice(b"ALSD");
    bad_kind_alsd.extend_from_slice(&1u32.to_le_bytes());
    bad_kind_alsd.extend_from_slice(&1u32.to_le_bytes());
    bad_kind_alsd.extend_from_slice(&[0u8; 16]); // kind = 0 (invalid)
    assert!(parse_alsd_entries(&bad_kind_alsd).is_none());

    for _ in 0..200 {
        let len = rng.gen_range(96);
        let mut raw = vec![0u8; len];
        for b in &mut raw {
            *b = (rng.next_u64() & 0xff) as u8;
        }
        if len >= 4 && rng.gen_range(2) == 0 {
            raw[0..4].copy_from_slice(b"ALSD");
        }
        let _ = parse_alsd_entries(&raw);
    }
}

#[test]
fn phase19_12_runtime_boundary_forged_handles_and_pointers() {
    // Forged handle patterns must be rejected cleanly
    assert!(!is_handle_tagged(0));
    assert!(!is_handle_tagged(HANDLE_TAG)); // tag with 0 payload
    assert!(!is_handle_tagged(u64::MAX)); // top bits 11, not 01
    assert!(is_handle_tagged(HANDLE_TAG | 1));
    assert_eq!(HANDLE_TAG_MASK, 0xC000_0000_0000_0000);

    let forged = HANDLE_TAG | 0xDEAD_BEEF_1234;
    assert!(aot_get_value(forged).is_none());
    assert!(try_cstr_abi_string(forged).is_none());
    assert!(try_cstr_abi_string(0).is_none());
    assert!(try_cstr_abi_string(0x1000).is_none());

    // Bounds & enum tag checks on forged/unmapped/overflowing pointers
    assert_eq!(adesh_rt_check_bounds(0, 0, 8), 0);
    assert_eq!(adesh_rt_check_bounds(forged as i64, 0, 8), 0);
    assert_eq!(adesh_rt_check_bounds(i64::MAX, 16, 8), 0);
    assert_eq!(adesh_rt_check_bounds(0x20000, -1, 8), 0);

    assert_eq!(unsafe { adesh_rt_enum_check_tag(std::ptr::null(), 4) }, -1);
    assert_eq!(
        unsafe { adesh_rt_enum_check_tag(forged as *const i64, 4) },
        -1
    );
    assert_eq!(
        unsafe { adesh_rt_enum_check_tag(0x20000 as *const i64, 4) },
        -1
    );
}
