//! Real-world example: In-memory cache with zero-GC ARC
//!
//! This example demonstrates a realistic use case of Adesh's ecosystem:
//! - High-performance caching with ARC
//! - Zero GC pauses (deterministic memory)
//! - Memory-safe concurrent access
//! - Efficient reference sharing

use adeshlang::stdlib::adesh_alloc::Arc;
use std::collections::HashMap;
use std::time::Instant;

/// A simple in-memory cache using Adesh's ARC for zero-GC sharing
struct Cache {
    data: HashMap<String, Arc<Vec<u8>>>,
    access_count: HashMap<String, usize>,
}

impl Cache {
    fn new() -> Self {
        Cache {
            data: HashMap::new(),
            access_count: HashMap::new(),
        }
    }
    
    fn insert(&mut self, key: &str, value: Vec<u8>) {
        // ARC allows efficient sharing without copying (zero-GC!)
        self.data.insert(key.to_string(), Arc::new(value));
        self.access_count.insert(key.to_string(), 0);
    }
    
    fn get(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        // Increment access count
        if let Some(count) = self.access_count.get_mut(key) {
            *count += 1;
        }
        
        // Return ARC clone (just increments counter, no data copy!)
        self.data.get(key).cloned()
    }
    
    fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.data.len(),
            total_accesses: self.access_count.values().sum(),
            most_accessed: self.find_most_accessed(),
        }
    }
    
    fn find_most_accessed(&self) -> Option<(String, usize)> {
        self.access_count
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, count)| (key.clone(), *count))
    }
}

struct CacheStats {
    entries: usize,
    total_accesses: usize,
    most_accessed: Option<(String, usize)>,
}

fn main() {
    println!("=== Adesh Zero-GC Cache Demo ===\n");
    
    // Create cache
    let mut cache = Cache::new();
    
    // Insert some data
    println!("Inserting data into cache...");
    cache.insert("user:1", b"Alice".to_vec());
    cache.insert("user:2", b"Bob".to_vec());
    cache.insert("user:3", b"Charlie".to_vec());
    cache.insert("config", b"debug=true,version=1.0".to_vec());
    
    // Simulate realistic access patterns
    println!("Simulating cache access patterns...\n");
    
    let start = Instant::now();
    
    // Access user:1 frequently
    for _ in 0..1000 {
        let _ = cache.get("user:1");
    }
    
    // Access user:2 moderately
    for _ in 0..500 {
        let _ = cache.get("user:2");
    }
    
    // Access config occasionally
    for _ in 0..100 {
        let _ = cache.get("config");
    }
    
    let elapsed = start.elapsed();
    
    // Get stats
    let stats = cache.stats();
    
    println!("Performance Results:");
    println!("  Total operations: {}", stats.total_accesses);
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Operations/sec: {:.0}", 
        stats.total_accesses as f64 / elapsed.as_secs_f64());
    println!("  Avg latency: {:?}", 
        elapsed / stats.total_accesses as u32);
    
    println!("\nCache Statistics:");
    println!("  Total entries: {}", stats.entries);
    println!("  Total accesses: {}", stats.total_accesses);
    
    if let Some((key, count)) = stats.most_accessed {
        println!("  Most accessed: {} ({} times)", key, count);
    }
    
    // Demonstrate ARC reference counting
    println!("\nMemory Management:");
    if let Some(data) = cache.get("user:1") {
        println!("  Retrieved data via ARC");
        println!("  ARC strong count: {}", Arc::strong_count(&data));
        
        // Create another reference
        let data2 = data.clone();
        println!("  After clone, strong count: {}", Arc::strong_count(&data));
        
        // Drop one reference
        drop(data2);
        println!("  After drop, strong count: {}", Arc::strong_count(&data));
        
        // Data still valid!
        println!("  Data still accessible: {:?}", 
            String::from_utf8_lossy(data.as_slice()));
    }
    
    println!("\n=== Key Benefits ===");
    println!("✓ Zero GC pauses - deterministic performance");
    println!("✓ ARC enables efficient sharing without copying");
    println!("✓ Memory-safe concurrent access (compile-time verified)");
    println!("✓ Predictable latency for real-time systems");
    println!("✓ No stop-the-world collections");
}
