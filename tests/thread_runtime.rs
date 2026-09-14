//! Interpreter tests for AdeshLang threading.

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_thread_test".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_str, &mut loader, None)
        })
        .unwrap();
    h.join().unwrap().map_err(|e| e.to_string())
}

#[test]
fn thread_current_and_cpu_count() {
    let r = run_code(
        r#"
import thread;
let n = thread.hardware_concurrency();
print(n);
let id = thread.id();
print(id);
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_spawn_join() {
    let r = run_code(
        r#"
import thread;
let h = thread.spawn(fn() {
    return 41 + 1;
});
let v = h.join();
if (v != 42) { throw "join result"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_spawn_uses_native_methods() {
    let r = run_code(
        r#"
import thread;
let h = thread.spawn(fn() {
    thread.sleep(1);
    return thread.id();
});
let v = h.join();
if (v == null) { throw "id failed on worker"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_mutex_shared_across_spawn() {
    let r = run_code(
        r#"
import thread;
let m = thread.Mutex.new(0);
let t1 = thread.spawn(fn() {
    m.with(fn(v) { return v + 1; });
});
let t2 = thread.spawn(fn() {
    m.with(fn(v) { return v + 1; });
});
t1.join();
t2.join();
let g = m.lock();
if (!g.ok) { throw "lock failed"; }
if (g.value.get() != 2) { throw "mutex value"; }
g.value.unlock();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_mutex_with() {
    let r = run_code(
        r#"
import thread;
let m = thread.Mutex.new(0);
m.with(fn(v) { return v + 1; });
m.with(fn(v) { return v + 1; });
let g = m.lock();
if (!g.ok) { throw "lock failed"; }
if (g.value.get() != 2) { throw "mutex value"; }
g.value.unlock();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_channel_bounded() {
    let r = run_code(
        r#"
import thread;
let pair = thread.Channel.bounded(4);
let tx = pair[0];
let rx = pair[1];
tx.send(7);
let got = rx.recv();
if (!got.ok) { throw "recv"; }
if (got.value != 7) { throw "payload"; }
tx.close();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_atomic_fetch_add() {
    let r = run_code(
        r#"
import thread;
let a = thread.Atomic.AtomicI64.new(10);
let prev = a.fetch_add(5);
if (prev != 10) { throw "fetch_add prev"; }
if (a.load() != 15) { throw "fetch_add load"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_waitgroup_once() {
    let r = run_code(
        r#"
import thread;
let wg = thread.WaitGroup.new();
wg.add(1);
wg.done();
wg.wait();
let o = thread.Once.new();
let n = 0;
o.call_once(fn() { n = 1; return 9; });
o.call_once(fn() { n = 2; return 8; });
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_waitgroup_wait_timeout() {
    let r = run_code(
        r#"
import thread;
let wg = thread.WaitGroup.new();
wg.add(1);
let timed = wg.wait_timeout(20);
if (timed) { throw "expected timeout"; }
wg.done();
if (!wg.wait_timeout(200)) { throw "expected wait to succeed"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_pool_join_timeout_after_shutdown() {
    let r = run_code(
        r#"
import thread;
let pool = thread.ThreadPool.new(2);
pool.execute(fn() { return 1; });
thread.sleep(20);
pool.shutdown();
let j = pool.join_timeout(2000);
if (j != null && j.ok == false && j.kind == "timeout") { throw "pool join timed out"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_import_thread_alias() {
    let r = run_code(
        r#"
import Thread;
let n = Thread.hardware_concurrency();
print(n);
let m = thread.hardware_concurrency();
print(m);
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn thread_sleep_yield() {
    let r = run_code(
        r#"
import thread;
thread.yield();
thread.sleep(1);
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn native_thread_id_unique() {
    use adeshlang::runtime::thread::ThreadId;
    let a = ThreadId::current();
    let b = std::thread::spawn(ThreadId::current).join().unwrap();
    assert_ne!(a, b);
}

#[test]
fn native_mutex_and_barrier_smoke() {
    use std::sync::Arc;
    let n = 4;
    let barrier = Arc::new(std::sync::Barrier::new(n));
    let mut hs = vec![];
    for _ in 0..n {
        let b = barrier.clone();
        hs.push(std::thread::spawn(move || {
            b.wait();
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
}
