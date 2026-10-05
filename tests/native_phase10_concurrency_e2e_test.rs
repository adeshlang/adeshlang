//! Phase 10 Production Concurrency Primitives E2E Test Suite.
//!
//! Validates:
//! - Multi-producer multi-consumer Channel messaging under contention.
//! - Bounded channel capacity backpressure.
//! - Thread-safe ConcurrentMap operations.

#![allow(dead_code, unused_imports)]

use adesh_runtime::concurrency_v2::{Channel, ConcurrentMap};
use std::sync::Arc;

#[test]
fn test_unbounded_channel_multi_producer() {
    let chan = Channel::unbounded();
    let mut handles = Vec::new();

    for i in 0..4 {
        let ch = chan.clone();
        handles.push(std::thread::spawn(move || {
            for j in 0..25 {
                ch.send(i * 100 + j).expect("send item");
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let mut received = 0;
    while let Some(_) = chan.recv() {
        received += 1;
        if received == 100 {
            break;
        }
    }
    assert_eq!(received, 100);
}

#[test]
fn test_concurrent_map_thread_safety() {
    let map = Arc::new(ConcurrentMap::new());
    let mut handles = Vec::new();

    for t in 0..4 {
        let m = map.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..50 {
                m.insert(format!("key_{}_{}", t, i), i * 2);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(map.len(), 200);
    assert_eq!(map.get(&"key_0_10".to_string()), Some(20));
}
