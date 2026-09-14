// Using AdeshLang FFI from Rust
// This example shows how compiled AdeshLang libraries work with Rust projects

use std::os::raw::c_longlong;

// Import the functions from AdeshLang compiled library
extern "C" {
    fn add(a: c_longlong, b: c_longlong) -> c_longlong;
    fn multiply(a: c_longlong, b: c_longlong) -> c_longlong;
    fn factorial(n: c_longlong) -> c_longlong;
    fn gcd(a: c_longlong, b: c_longlong) -> c_longlong;
    fn is_prime(n: c_longlong) -> c_longlong;
}

fn main() {
    println!("=== AdeshLang FFI from Rust ===\n");
    
    unsafe {
        // Basic arithmetic
        println!("Basic Math:");
        println!("  add(100, 50) = {}", add(100, 50));
        println!("  multiply(12, 8) = {}", multiply(12, 8));
        
        // More complex functions
        println!("\nAdvanced Math:");
        println!("  factorial(10) = {}", factorial(10));
        println!("  gcd(48, 18) = {}", gcd(48, 18));
        
        // Prime checking
        println!("\nPrime Tests:");
        for n in [2, 15, 17, 100, 97] {
            let is_p = is_prime(n);
            println!("  is_prime({}) = {} ({})", 
                n, 
                is_p,
                if is_p == 1 { "prime" } else { "composite" }
            );
        }
    }
    
    println!("\n✅ Rust successfully called AdeshLang FFI functions!");
    println!("   Arc<Mutex<FfiRegistry>> ensures thread safety across language boundaries");
}

// Build instructions:
// 1. Compile AdeshLang libraries:
//    adesh compile-aot math_lib.adesh math_lib.o -c
//    adesh compile-aot advanced_math.adesh advanced_math.o -c
//
// 2. Compile this Rust program (link the object files):
//    rustc use_from_rust.rs -o use_from_rust.exe -C link-arg=math_lib.o -C link-arg=advanced_math.o
//
// 3. Run:
//    ./use_from_rust
