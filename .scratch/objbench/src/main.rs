//! Object-map micro benchmark: current FxHashMap (interpreter/NJIT), BTreeMap
//! (AOT runtime) and IndexMap with the same Fx hasher (proposed).
use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHasher};
type FxBuildHasher = std::hash::BuildHasherDefault<FxHasher>;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

type Fx = FxHashMap<String, u64>;
type Ix = IndexMap<String, u64, FxBuildHasher>;
type Bt = BTreeMap<String, u64>;

trait Map: Default + Clone {
    fn put(&mut self, k: String, v: u64);
    fn read(&self, k: &str) -> Option<u64>;
    fn del(&mut self, k: &str);
    fn sum(&self) -> u64;
}
impl Map for Fx {
    fn put(&mut self, k: String, v: u64) { self.insert(k, v); }
    fn read(&self, k: &str) -> Option<u64> { self.get(k).copied() }
    fn del(&mut self, k: &str) { self.remove(k); }
    fn sum(&self) -> u64 { self.iter().map(|(k, v)| k.len() as u64 + v).sum() }
}
impl Map for Ix {
    fn put(&mut self, k: String, v: u64) { self.insert(k, v); }
    fn read(&self, k: &str) -> Option<u64> { self.get(k).copied() }
    fn del(&mut self, k: &str) { self.shift_remove(k); }
    fn sum(&self) -> u64 { self.iter().map(|(k, v)| k.len() as u64 + v).sum() }
}
impl Map for Bt {
    fn put(&mut self, k: String, v: u64) { self.insert(k, v); }
    fn read(&self, k: &str) -> Option<u64> { self.get(k).copied() }
    fn del(&mut self, k: &str) { self.remove(k); }
    fn sum(&self) -> u64 { self.iter().map(|(k, v)| k.len() as u64 + v).sum() }
}

fn time<F: FnMut() -> u64>(iters: usize, mut f: F) -> f64 {
    let mut acc = 0u64;
    for _ in 0..iters / 10 { acc = acc.wrapping_add(f()); }
    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..iters { acc = acc.wrapping_add(f()); }
        best = best.min(start.elapsed().as_nanos() as f64 / iters as f64);
    }
    black_box(acc);
    best
}

fn bench<M: Map>(keys: &[String], iters: usize) -> [f64; 6] {
    let build = || {
        let mut m = M::default();
        for (i, k) in keys.iter().enumerate() { m.put(k.clone(), i as u64); }
        m
    };
    let base = build();
    [
        time(iters, || build().sum()),
        time(iters, || keys.iter().filter_map(|k| base.read(k)).sum()),
        time(iters, || {
            let mut m = base.clone();
            for k in keys { m.put(k.clone(), 7); }
            m.read(&keys[0]).unwrap()
        }),
        time(iters, || {
            let mut m = base.clone();
            m.del(&keys[0]);
            m.sum()
        }),
        time(iters, || base.sum()),
        time(iters, || base.clone().sum()),
    ]
}

fn main() {
    println!("size,map,create_ns,lookup_all_ns,update_all_ns,delete_one_ns,iterate_ns,clone_ns");
    for &n in &[4usize, 16, 64, 1024] {
        let keys: Vec<String> = (0..n).map(|i| format!("field_{i}")).collect();
        let iters = (200_000 / n).max(200);
        for (name, r) in [
            ("FxHashMap", bench::<Fx>(&keys, iters)),
            ("IndexMap<Fx>", bench::<Ix>(&keys, iters)),
            ("BTreeMap", bench::<Bt>(&keys, iters)),
        ] {
            println!("{n},{name},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0}", r[0], r[1], r[2], r[3], r[4], r[5]);
        }
    }
}
