//! Example demonstrating Adesh's zero-GC, memory-safe ecosystem
//!
//! This example shows:
//! - Using layered stdlib (adesh_core, adesh_alloc, adesh_std)
//! - Zero-GC memory management with ARC
//! - Memory safety without explicit lifetimes
//! - High-performance data structures

use adeshlang::stdlib::{EcosystemMetadata, adesh_alloc, adesh_core};

fn main() {
    // Print ecosystem information
    let metadata = EcosystemMetadata::current();
    println!("{}\n", metadata.display());

    // Example 1: adesh_core - No allocator, no OS
    println!("=== Example 1: adesh_core (No dependencies) ===");
    demonstrate_adesh_core();

    // Example 2: adesh_alloc - ARC-based memory management
    println!("\n=== Example 2: adesh_alloc (Zero-GC) ===");
    demonstrate_adesh_alloc();

    // Example 3: Ownership without lifetimes
    println!("\n=== Example 3: Ownership Without Lifetimes ===");
    demonstrate_ownership();
}

fn demonstrate_adesh_core() {
    // Layout operations
    let layout = adesh_core::Layout::new::<u64>();
    println!(
        "  Layout of u64: size={}, align={}",
        layout.size(),
        layout.align()
    );

    // Alignment operations
    let align = adesh_core::Align::of::<u64>();
    println!("  Alignment: {}", align.as_usize());
    println!("  Align up 10 to 8-byte boundary: {}", align.align_up(10));

    // Intrinsics
    println!(
        "  size_of::<u32>() = {}",
        adesh_core::intrinsics::size_of::<u32>()
    );
    println!(
        "  align_of::<u16>() = {}",
        adesh_core::intrinsics::align_of::<u16>()
    );
}

fn demonstrate_adesh_alloc() {
    // ARC - Atomic Reference Counting (Zero-GC!)
    let arc1 = adesh_alloc::Arc::new(42);
    let arc2 = arc1.clone();
    println!("  ARC value: {}", *arc1);
    println!("  Strong count: {}", adesh_alloc::Arc::strong_count(&arc1));

    // Weak references for cycle prevention
    let weak = adesh_alloc::Arc::downgrade(&arc1);
    println!("  Weak count: {}", adesh_alloc::Arc::weak_count(&arc1));
    drop(arc1);
    drop(arc2);
    println!(
        "  After dropping strong refs, upgrade: {:?}",
        weak.upgrade()
    );

    // Vec - Dynamic array
    let mut vec = adesh_alloc::Vec::new();
    vec.push(1);
    vec.push(2);
    vec.push(3);
    println!(
        "  Vec: {:?} (len={}, cap={})",
        vec.get(0).unwrap(),
        vec.len(),
        vec.capacity()
    );

    // String - UTF-8 string
    let mut s = adesh_alloc::String::new();
    s.push_str("Hello");
    s.push(' ');
    s.push_str("Adesh!");
    println!("  String: {}", s.as_str());

    // HashMap - Key-value store
    let mut map = adesh_alloc::HashMap::new();
    map.insert("language", "Adesh");
    map.insert("memory", "Zero-GC");
    println!(
        "  HashMap: language={:?}, memory={:?}",
        map.get(&"language"),
        map.get(&"memory")
    );

    // Box - Unique heap pointer
    let boxed = adesh_alloc::Box::new([1, 2, 3, 4, 5]);
    println!("  Box: first element = {}", boxed[0]);
}

fn demonstrate_ownership() {
    // Ownership without explicit lifetimes!
    let data = vec![1, 2, 3, 4, 5];
    let first = get_first_element(&data);
    println!("  First element: {}", first);

    // Borrow checker ensures safety
    let mut numbers = vec![10, 20, 30];
    modify_vector(&mut numbers);
    println!("  Modified vector: {:?}", numbers);

    // Move semantics
    let owned = vec![100, 200];
    take_ownership(owned);
    // owned is moved - cannot use here!
}

// No explicit lifetime annotations needed!
fn get_first_element(v: &Vec<i32>) -> &i32 {
    &v[0]
}

fn modify_vector(v: &mut Vec<i32>) {
    v.push(40);
}

fn take_ownership(v: Vec<i32>) {
    println!("  Took ownership of: {:?}", v);
    // v is dropped here
}
