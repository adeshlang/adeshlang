//! Comprehensive tests for AdeshLang's high-speed Parallel library

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_parallel_test".into())
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
fn test_parallel_workers_and_config() {
    let r = run_code(
        r#"
import Parallel;
let w = Parallel.workers();
if (w < 1) { throw "invalid workers count"; }
let nw = Parallel.num_workers();
if (nw != w) { throw "num_workers mismatch"; }
let set_res = Parallel.setWorkers(4);
if (Parallel.workers() != 4) { throw "setWorkers failed"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_map_order_preservation() {
    let r = run_code(
        r#"
import Parallel;
let data = [];
for i in 0..100 {
    data.push(i);
}
let mapped = Parallel.map(data, fn(x) {
    return x * 2 + 1;
});
if (len(mapped) != 100) { throw "length mismatch"; }
for i in 0..100 {
    if (mapped[i] != (i * 2 + 1)) {
        throw "element mismatch at " + str(i);
    }
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_flat_map() {
    let r = run_code(
        r#"
import Parallel;
let data = [1, 2, 3, 4, 5];
let flat = Parallel.flatMap(data, fn(x) {
    return [x, x * 10];
});
if (len(flat) != 10) { throw "flat length"; }
if (flat[0] != 1 || flat[1] != 10 || flat[2] != 2 || flat[3] != 20) {
    throw "flat elements incorrect";
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_filter() {
    let r = run_code(
        r#"
import Parallel;
let data = [];
for i in 0..100 {
    data.push(i);
}
let evens = Parallel.filter(data, fn(x) {
    return x % 2 == 0;
});
if (len(evens) != 50) { throw "filter length"; }
for i in 0..50 {
    if (evens[i] != i * 2) {
        throw "filter element mismatch at " + str(i);
    }
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_reduce_sum() {
    let r = run_code(
        r#"
import Parallel;
let data = [];
let expected = 0;
for i in 1..101 {
    data.push(i);
    expected = expected + i;
}
let sum = Parallel.reduce(data, 0, fn(acc, x) {
    return acc + x;
});
if (sum != expected) {
    throw "reduce sum expected " + str(expected) + " got " + str(sum);
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_for_each_range_and_array() {
    let r = run_code(
        r#"
import Parallel;
import thread;

let m = thread.Mutex.new(0);
Parallel.forEach(0, 100, fn(i) {
    m.with(fn(v) { return v + 1; });
});
let g = m.lock();
if (g.value.get() != 100) { throw "range forEach count"; }
g.value.unlock();

let data = [];
for i in 0..50 { data.push(i); }
let m2 = thread.Mutex.new(0);
Parallel.forEach(data, fn(item) {
    m2.with(fn(v) { return v + item; });
});
let g2 = m2.lock();
if (g2.value.get() != 1225) { throw "array forEach sum"; } // 0+1+..+49 = 49*50/2 = 1225
g2.value.unlock();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_for_step() {
    let r = run_code(
        r#"
import Parallel;
import thread;

let m = thread.Mutex.new(0);
Parallel.for(0, 100, 2, fn(i) {
    m.with(fn(v) { return v + 1; });
});
let g = m.lock();
if (g.value.get() != 50) { throw "step for count"; }
g.value.unlock();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_find_and_predicates() {
    let r = run_code(
        r#"
import Parallel;
let data = [10, 20, 35, 40, 50];

let found = Parallel.find(data, fn(x) { return x % 7 == 0; });
if (found != 35) { throw "find failed"; }

let not_found = Parallel.find(data, fn(x) { return x == 999; });
if (not_found != null) { throw "find should return null"; }

let idx = Parallel.findIndex(data, fn(x) { return x == 40; });
if (idx != 3) { throw "findIndex expected 3, got " + str(idx); }

let has_odd = Parallel.any(data, fn(x) { return x % 2 != 0; });
if (!has_odd) { throw "any failed"; }

let all_positive = Parallel.all(data, fn(x) { return x > 0; });
if (!all_positive) { throw "all failed"; }

let all_even = Parallel.all(data, fn(x) { return x % 2 == 0; });
if (all_even) { throw "all should be false"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_sort_and_sort_by() {
    let r = run_code(
        r#"
import Parallel;
let data = [45, 12, 85, 32, 89, 39, 69, 44, 42, 1, 45, 100];
let sorted = Parallel.sort(data);
if (len(sorted) != len(data)) { throw "sort length"; }
for i in 0..(len(sorted) - 1) {
    if (sorted[i] > sorted[i + 1]) {
        throw "unsorted at " + str(i);
    }
}

let items = [
    { name: "Charlie", age: 30 },
    { name: "Alice", age: 20 },
    { name: "Bob", age: 25 }
];
let sorted_by_age = Parallel.sortBy(items, fn(p) { return p.age; });
if (sorted_by_age[0].name != "Alice" || sorted_by_age[1].name != "Bob" || sorted_by_age[2].name != "Charlie") {
    throw "sortBy failed";
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_math_simd_reductions() {
    let r = run_code(
        r#"
import Parallel;
let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

let s = Parallel.sum(data);
if (s != 55) { throw "sum expected 55, got " + str(s); }

let p = Parallel.product([1, 2, 3, 4, 5]);
if (p != 120) { throw "product expected 120, got " + str(p); }

let mn = Parallel.min(data);
if (mn != 1) { throw "min expected 1, got " + str(mn); }

let mx = Parallel.max(data);
if (mx != 10) { throw "max expected 10, got " + str(mx); }

let avg = Parallel.mean(data);
if (avg != 5.5) { throw "mean expected 5.5, got " + str(avg); }

let a = [1, 2, 3];
let b = [4, 5, 6];
let dot = Parallel.dot(a, b); // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
if (dot != 32) { throw "dot expected 32, got " + str(dot); }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_zip_with_chunk_batch() {
    let r = run_code(
        r#"
import Parallel;
let a = [1, 2, 3];
let b = [10, 20, 30];
let zipped = Parallel.zipWith(a, b, fn(x, y) { return x + y; });
if (zipped[0] != 11 || zipped[1] != 22 || zipped[2] != 33) {
    throw "zipWith failed";
}

let data = [1, 2, 3, 4, 5, 6, 7];
let chunks = Parallel.chunk(data, 3);
if (len(chunks) != 3) { throw "chunk count"; }
if (len(chunks[0]) != 3 || len(chunks[1]) != 3 || len(chunks[2]) != 1) {
    throw "chunk sizes incorrect";
}

let batch_sums = Parallel.batch(data, 3, fn(c) {
    let sum = 0;
    for x in c { sum = sum + x; }
    return sum;
});
if (batch_sums[0] != 6 || batch_sums[1] != 15 || batch_sums[2] != 7) {
    throw "batch sums failed";
}
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_parallel_join_and_scope() {
    let r = run_code(
        r#"
import Parallel;

// Parallel.join
let results = Parallel.join(
    fn() { return 10 * 10; },
    fn() { return 20 * 20; },
    fn() { return 30 * 30; }
);
if (results[0] != 100 || results[1] != 400 || results[2] != 900) {
    throw "join failed";
}

// Parallel.scope
import thread;
let m = thread.Mutex.new(0);
Parallel.scope(fn(s) {
    s.spawn(fn() { m.with(fn(v) { return v + 10; }); });
    s.spawn(fn() { m.with(fn(v) { return v + 20; }); });
    s.spawn(fn() { m.with(fn(v) { return v + 30; }); });
});
let g = m.lock();
if (g.value.get() != 60) { throw "scope join count"; }
g.value.unlock();
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn test_thread_parallel_primitives() {
    let r = run_code(
        r#"
import thread;

let mapped = thread.parallel_map([1, 2, 3, 4, 5], fn(x) { return x * 3; });
if (mapped[0] != 3 || mapped[4] != 15) { throw "thread.parallel_map failed"; }

let reduced = thread.parallel_reduce([1, 2, 3, 4], 10, fn(acc, x) { return acc + x; });
if (reduced != 20) { throw "thread.parallel_reduce failed"; }
"#,
    );
    assert!(r.is_ok(), "{:?}", r.err());
}
