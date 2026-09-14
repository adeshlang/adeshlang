# AdeshLang Memory Management Examples

This directory contains comprehensive examples demonstrating AdeshLang's sophisticated memory management system. AdeshLang uses **manual memory management** with **no garbage collection** or **automatic reference counting**, instead providing a hybrid system with multiple allocation strategies and safety mechanisms.

## Overview

AdeshLang's memory management is based on:
- **Ownership tracking** with Rust-inspired borrowing semantics
- **Smart pointers** (`Shared<T>`, `Unique<T>`, `Weak<T>`)
- **Multiple allocation strategies** (heap, arena, bump, slab)
- **Small object optimizations** (SSO for strings, SAO for arrays)
- **Memory safety** through ownership validation and debug poisoning

## Examples

### 1. `shared_unique_weak.adesh`
Demonstrates the three main smart pointer types:
- **Shared<T>**: Reference-counted shared ownership
- **Unique<T>**: Move-only exclusive ownership  
- **Weak<T>**: Non-owning references to break cycles

**Key concepts:**
- Reference counting behavior
- Move semantics and ownership transfer
- Weak references for parent-child relationships
- Debug poisoning for use-after-free detection

### 2. `sso_sao_examples.adesh`
Shows small object optimizations:
- **SSO (Small String Optimization)**: Strings ≤22 bytes stored inline
- **SAO (Small Array Optimization)**: Arrays ≤8 elements stored inline

**Key concepts:**
- Memory overhead reduction
- Performance benefits of inline storage
- When optimizations apply vs heap allocation
- Memory usage comparison

### 3. `ownership_patterns.adesh`
Demonstrates ownership and borrowing patterns:
- Unique ownership and move semantics
- Borrowing rules (multiple immutable OR single mutable)
- RAII (Resource Acquisition Is Initialization)
- Scope-based lifetime management

**Key concepts:**
- Ownership transfer chains
- Borrow checking at runtime
- Automatic resource cleanup
- Memory safety validation

### 4. `allocation_strategies.adesh`
Shows different memory allocation strategies:
- **Heap**: Long-lived data, general purpose
- **Arena**: Temporary data with bulk deallocation
- **Bump**: Very fast allocation for compilation phases
- **Slab**: Fixed-size objects with predictable patterns

**Key concepts:**
- Choosing appropriate allocation strategy
- Performance characteristics of each allocator
- Memory pool management
- Allocation statistics and monitoring

### 5. `reference_cycles.adesh`
Demonstrates handling reference cycles:
- Parent-child relationships without memory leaks
- Observer patterns with weak references
- Cache implementations that don't prevent cleanup
- Graph structures with mixed strong/weak edges

**Key concepts:**
- Breaking reference cycles with weak pointers
- Safe back-reference patterns
- Observer pattern implementation
- Weak cache that allows garbage collection

### 6. `memory_safety_demo.adesh`
Comprehensive memory safety demonstration:
- Use-after-free detection
- Double-free prevention
- Borrow checking violations
- Memory poisoning in debug mode
- Ownership validation

**Key concepts:**
- Runtime safety checks
- Error detection and prevention
- Comprehensive buffer safety
- Multiple safety mechanisms working together

## Running the Examples

Each example can be run independently:

```bash
# Basic smart pointer usage
adesh run examples/memory/shared_unique_weak.adesh

# Small object optimizations
adesh run examples/memory/sso_sao_examples.adesh

# Ownership patterns
adesh run examples/memory/ownership_patterns.adesh

# Allocation strategies
adesh run examples/memory/allocation_strategies.adesh

# Reference cycle handling
adesh run examples/memory/reference_cycles.adesh

# Memory safety features
adesh run examples/memory/memory_safety_demo.adesh
```

## Memory Management Principles

### 1. No Garbage Collection
AdeshLang explicitly does not use garbage collection. Memory management is manual but assisted by:
- Ownership tracking
- Automatic cleanup through RAII
- Smart pointers for safe sharing

### 2. Performance-First Design
- Multiple specialized allocators for different use cases
- Small object optimizations to avoid heap allocation
- Compact value representation for better cache utilization
- Interned strings for fast equality comparisons

### 3. Safety Through Ownership
- Rust-inspired ownership model prevents common memory errors
- Runtime borrow checking catches aliasing violations
- Debug mode poisoning detects use-after-free bugs
- Ownership validation ensures proper access control

### 4. Flexible Allocation
- Choose allocation strategy based on object lifetime and usage patterns
- Arena allocation for temporary data
- Slab allocation for fixed-size objects
- Bump allocation for compilation phases
- Heap allocation for general-purpose long-lived data

## Best Practices

### Choosing Smart Pointers
- Use `Unique<T>` for exclusive ownership
- Use `Shared<T>` when multiple owners are needed
- Use `Weak<T>` for back-references and to break cycles

### Allocation Strategy Selection
- **Temporary data**: Arena or bump allocators
- **Fixed-size objects**: Slab allocator
- **Long-lived data**: Heap allocator
- **Small strings/arrays**: Rely on SSO/SAO optimizations

### Avoiding Memory Leaks
- Use weak references for parent-child relationships
- Implement proper cleanup in destructors
- Break reference cycles explicitly
- Use RAII patterns for automatic resource management

### Performance Optimization
- Prefer small strings (≤22 chars) and small arrays (≤8 elements)
- Use appropriate allocation strategy for object lifetime
- Minimize ownership transfers when possible
- Leverage interned strings for frequent comparisons

## Memory Safety Guarantees

AdeshLang's memory management provides:
- **Use-after-free prevention** through ownership tracking
- **Double-free prevention** through allocation state tracking
- **Buffer overflow protection** through bounds checking
- **Data race prevention** through borrow checking
- **Memory leak detection** through reference cycle analysis

These examples demonstrate how AdeshLang achieves memory safety without garbage collection, providing both performance and safety through careful design of ownership semantics and allocation strategies.