//! Stress tests for threading primitives (debug profile; no --release required).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[test]
fn stress_atomic_counter() {
    let n = 8usize;
    let ops = 50_000u64;
    let c = Arc::new(AtomicU64::new(0));
    let mut hs = vec![];
    for _ in 0..n {
        let c = c.clone();
        hs.push(std::thread::spawn(move || {
            for _ in 0..ops {
                c.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(c.load(Ordering::Relaxed), n as u64 * ops);
}

#[test]
fn stress_mutex() {
    let m = Arc::new(std::sync::Mutex::new(0u64));
    let mut hs = vec![];
    for _ in 0..8 {
        let m = m.clone();
        hs.push(std::thread::spawn(move || {
            for _ in 0..10_000 {
                *m.lock().unwrap() += 1;
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(*m.lock().unwrap(), 80_000);
}

#[test]
fn bench_uncontended_mutex_debug() {
    let m = std::sync::Mutex::new(0u64);
    let t = std::time::Instant::now();
    for _ in 0..100_000 {
        *m.lock().unwrap() += 1;
    }
    let ns = t.elapsed().as_nanos() / 100_000;
    eprintln!("uncontended mutex lock+add ~{ns} ns (debug build)");
    assert_eq!(*m.lock().unwrap(), 100_000);
}
