use crate::utils::collections::FastMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global map from pointer handle → debug label (variable name)
/// Updated by the interpreter whenever a pointer is assigned to a named variable.
static PTR_LABELS: Lazy<Mutex<HashMap<u64, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Tag a pointer handle with a human-readable label (usually the variable name).
/// Overwrites any previous label for the same pointer.
pub fn tag_ptr(ptr: u64, label: &str) {
    if let Ok(mut map) = PTR_LABELS.lock() {
        map.insert(ptr, label.to_string());
    }
}

/// Retrieve a label for a pointer (for error messages).
fn ptr_label(ptr: u64) -> String {
    PTR_LABELS
        .lock()
        .ok()
        .and_then(|m| m.get(&ptr).cloned())
        .map(|name| format!("'{}'", name))
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Pointer state tracking for memory safety
#[derive(Debug, Clone)]
pub enum PtrState {
    /// Pointer is valid: size in bytes, element size in bytes, and owner thread ID
    Alive {
        size: usize,
        elem_size: usize,
        owner_thread: u64,
    },
    /// Pointer has been freed (use-after-free detection)
    Freed,
}

/// Allocation entry with state tracking and optional debug info
#[derive(Debug, Clone)]
struct AllocEntry {
    state: PtrState,
    data: Vec<u8>,
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    created_at: String,
}

static GLOBAL_ALLOCATIONS: Lazy<Mutex<FastMap<u64, AllocEntry>>> =
    Lazy::new(|| Mutex::new(FastMap::default()));
static NEXT_PTR_ID: AtomicU64 = AtomicU64::new(1);

// Per-process secret to obfuscate pointer handles so they cannot be guessed.
// Handle exposed to users = raw_id ^ POINTER_SECRET.
static POINTER_SECRET: Lazy<u64> = Lazy::new(|| {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seed = nanos ^ 0xA5A5_A5A5_A5A5_A5A5u64;
    if seed == 0 {
        0x1F42_7D38_4A93_BC27u64
    } else {
        seed
    }
});

#[inline]
fn decode_handle(handle: u64) -> u64 {
    handle ^ *POINTER_SECRET
}

#[inline]
fn encode_handle(raw_id: u64) -> u64 {
    raw_id ^ *POINTER_SECRET
}

/// Region context for arena-style scoped allocations
#[derive(Debug, Clone)]
struct RegionCtx {
    name: String,
    allocations_raw: Vec<u64>,
}

/// Per-thread region stacks
static REGION_STACKS: Lazy<Mutex<HashMap<u64, Vec<RegionCtx>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Poison pattern for freed memory in debug builds
#[cfg(debug_assertions)]
const POISON_PATTERN: u8 = 0xDE;

/// Type size table for typed pointer arithmetic
#[inline]
pub fn elem_size_of(type_name: &str) -> usize {
    match type_name {
        "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" | "f32" => 4,
        "u64" | "i64" | "f64" => 8,
        "u128" | "i128" => 16,
        _ => 1, // default to byte
    }
}

/// Get current thread ID for tracking
fn current_thread_id() -> u64 {
    let thread_id = thread::current().id();
    // Hash the thread ID to a u64
    // Use a simple approach: convert the debug string representation to a hash
    format!("{:?}", thread_id).len() as u64
        | (format!("{:?}", thread_id)
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64)))
}

/// Allocate memory with element type tracking
/// elem_size = 1 for byte arrays (*u8)
#[inline]
pub fn alloc_typed(size: usize, elem_size: usize) -> Result<u64, String> {
    // Check if heap allocation is forbidden (embedded mode)
    if !crate::memory::memory_policy().allow_heap {
        return Err("Pointer allocation forbidden in embedded mode".to_string());
    }

    if size == 0 {
        return Err("Cannot allocate 0 bytes".to_string());
    }
    if elem_size == 0 {
        return Err("Element size must be > 0".to_string());
    }

    let raw_id = NEXT_PTR_ID.fetch_add(1, Ordering::Relaxed);
    let handle = raw_id ^ *POINTER_SECRET;
    let owner_thread = current_thread_id();

    let entry = AllocEntry {
        state: PtrState::Alive {
            size,
            elem_size,
            owner_thread,
        },
        data: vec![0u8; size],
        #[cfg(debug_assertions)]
        created_at: format!("ptr:{} (thread:{})", raw_id, owner_thread),
    };

    GLOBAL_ALLOCATIONS.lock().unwrap().insert(raw_id, entry);

    // If inside a region, record this allocation for auto-free on region_end
    {
        let tid = current_thread_id();
        let mut stacks = REGION_STACKS.lock().unwrap();
        if let Some(stack) = stacks.get_mut(&tid) {
            if let Some(top) = stack.last_mut() {
                top.allocations_raw.push(raw_id);
            }
        }
    }
    Ok(handle)
}

/// Allocate memory (default to byte-level for backward compatibility)
#[inline]
pub fn alloc(size: usize) -> Result<u64, String> {
    alloc_typed(size, 1)
}

/// Free a pointer and mark as Freed (poison memory in debug mode)
#[inline]
pub fn free(ptr: u64) -> Result<(), String> {
    let raw_id = decode_handle(ptr);
    let mut guard = GLOBAL_ALLOCATIONS.lock().unwrap();

    if let Some(entry) = guard.get_mut(&raw_id) {
        match &entry.state {
            PtrState::Freed => {
                let label = ptr_label(ptr);
                return Err(format!("Double-free detected on variable {}", label));
            }
            PtrState::Alive { .. } => {
                // Zeroize to prevent data remnants from being read after free
                for byte in &mut entry.data {
                    *byte = 0;
                }

                // Poison memory in debug builds
                #[cfg(debug_assertions)]
                {
                    for byte in &mut entry.data {
                        *byte = POISON_PATTERN;
                    }
                }

                // Mark as freed but keep entry for use-after-free detection
                // Keep label too so double-free errors can show the variable name
                entry.state = PtrState::Freed;
                return Ok(());
            }
        }
    } else {
        let label = ptr_label(ptr);
        return Err(format!("Attempt to free invalid pointer {}", label));
    }
}

/// Load byte from pointer with state validation
#[inline]
pub fn load_u8(ptr: u64, offset: usize) -> Result<u8, String> {
    let raw_id = decode_handle(ptr);
    let guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();

    if let Some(entry) = guard.get(&raw_id) {
        match &entry.state {
            PtrState::Freed => Err(format!("Use-after-free: pointer {} was already freed", ptr)),
            PtrState::Alive {
                size, owner_thread, ..
            } => {
                // Check for cross-thread read (warning, but allowed)
                if *owner_thread != current_thread {
                    eprintln!(
                        "Warning: read from pointer {} owned by thread {} from thread {}",
                        ptr, owner_thread, current_thread
                    );
                }

                if offset < *size {
                    #[cfg(debug_assertions)]
                    {
                        let val = entry.data[offset];
                        if val == POISON_PATTERN {
                            return Err(format!(
                                "Poisoned memory read at ptr {} offset {}",
                                ptr, offset
                            ));
                        }
                    }
                    Ok(entry.data[offset])
                } else {
                    Err(format!(
                        "Pointer load out of bounds: offset {} >= size {}",
                        offset, size
                    ))
                }
            }
        }
    } else {
        Err(format!("Invalid pointer: {}", ptr))
    }
}

/// Store byte to pointer with state validation and cross-thread write detection
#[inline]
pub fn store_u8(ptr: u64, offset: usize, value: u8) -> Result<(), String> {
    let raw_id = decode_handle(ptr);
    let mut guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();

    if let Some(entry) = guard.get_mut(&raw_id) {
        match &entry.state {
            PtrState::Freed => Err(format!("Use-after-free: pointer {} was already freed", ptr)),
            PtrState::Alive {
                size, owner_thread, ..
            } => {
                // Detect unsynchronized cross-thread write
                if *owner_thread != current_thread {
                    return Err(format!(
                        "Unsynchronized cross-thread write detected: \
                         pointer {} owned by thread {} written from thread {}. \
                         Use synchronization primitives (mutex, atomics) for shared access.",
                        ptr, owner_thread, current_thread
                    ));
                }

                if offset < *size {
                    entry.data[offset] = value;
                    Ok(())
                } else {
                    Err(format!(
                        "Pointer store out of bounds: offset {} >= size {}",
                        offset, size
                    ))
                }
            }
        }
    } else {
        Err("Invalid pointer".to_string())
    }
}

/// Load typed element from pointer (element-based indexing)
/// Computes: address = base_ptr + (index * elem_size)
#[inline]
pub fn load_typed(ptr: u64, index: usize) -> Result<Vec<u8>, String> {
    let raw_id = decode_handle(ptr);
    let guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();

    if let Some(entry) = guard.get(&raw_id) {
        match &entry.state {
            PtrState::Freed => Err(format!("Use-after-free: pointer {} was already freed", ptr)),
            PtrState::Alive {
                size,
                elem_size,
                owner_thread,
            } => {
                // Check for cross-thread read (warning)
                if *owner_thread != current_thread {
                    eprintln!(
                        "Warning: typed read from pointer {} owned by thread {} from thread {}",
                        ptr, owner_thread, current_thread
                    );
                }

                let byte_offset = index
                    .checked_mul(*elem_size)
                    .ok_or_else(|| format!("Index overflow: {} * {}", index, elem_size))?;

                let end_offset = byte_offset
                    .checked_add(*elem_size)
                    .ok_or_else(|| format!("Offset overflow: {} + {}", byte_offset, elem_size))?;

                if end_offset > *size {
                    return Err(format!(
                        "Typed pointer load out of bounds: index {} (offset {}..{}) exceeds size {}",
                        index, byte_offset, end_offset, size
                    ));
                }

                Ok(entry.data[byte_offset..end_offset].to_vec())
            }
        }
    } else {
        Err(format!("Invalid pointer: {}", ptr))
    }
}

/// Store typed element to pointer (element-based indexing) with cross-thread detection
#[inline]
pub fn store_typed(ptr: u64, index: usize, value: &[u8]) -> Result<(), String> {
    let raw_id = decode_handle(ptr);
    let mut guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();

    if let Some(entry) = guard.get_mut(&raw_id) {
        match &entry.state {
            PtrState::Freed => Err(format!("Use-after-free: pointer {} was already freed", ptr)),
            PtrState::Alive {
                size,
                elem_size,
                owner_thread,
            } => {
                // Detect unsynchronized cross-thread write
                if *owner_thread != current_thread {
                    return Err(format!(
                        "Unsynchronized cross-thread write detected: \
                         pointer {} owned by thread {} written from thread {}. \
                         Use synchronization primitives (mutex, atomics) for shared access.",
                        ptr, owner_thread, current_thread
                    ));
                }

                if value.len() != *elem_size {
                    return Err(format!(
                        "Value size mismatch: expected {} bytes, got {}",
                        elem_size,
                        value.len()
                    ));
                }

                let byte_offset = index
                    .checked_mul(*elem_size)
                    .ok_or_else(|| format!("Index overflow: {} * {}", index, elem_size))?;

                let end_offset = byte_offset
                    .checked_add(*elem_size)
                    .ok_or_else(|| format!("Offset overflow: {} + {}", byte_offset, elem_size))?;

                if end_offset > *size {
                    return Err(format!(
                        "Typed pointer store out of bounds: index {} (offset {}..{}) exceeds size {}",
                        index, byte_offset, end_offset, size
                    ));
                }

                entry.data[byte_offset..end_offset].copy_from_slice(value);
                Ok(())
            }
        }
    } else {
        Err("Invalid pointer".to_string())
    }
}

/// Get allocation size in bytes (also checks ownership)
#[inline]
pub fn size_of(ptr: u64) -> Option<usize> {
    let raw_id = decode_handle(ptr);
    let guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    guard.get(&raw_id).and_then(|entry| match &entry.state {
        PtrState::Alive { size, .. } => Some(*size),
        PtrState::Freed => None,
    })
}

/// Get allocation size with thread check
#[inline]
pub fn size_of_checked(ptr: u64) -> Result<usize, String> {
    let raw_id = decode_handle(ptr);
    let guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();

    guard
        .get(&raw_id)
        .ok_or_else(|| format!("Invalid pointer: {}", ptr))
        .and_then(|entry| match &entry.state {
            PtrState::Alive {
                size, owner_thread, ..
            } => {
                if *owner_thread != current_thread {
                    eprintln!(
                        "Warning: size check on pointer {} owned by thread {} from thread {}",
                        ptr, owner_thread, current_thread
                    );
                }
                Ok(*size)
            }
            PtrState::Freed => Err(format!("Cannot get size of freed pointer: {}", ptr)),
        })
}

/// Get element size in bytes recorded for the pointer
#[inline]
pub fn elem_size_of_ptr(ptr: u64) -> Result<usize, String> {
    let raw_id = decode_handle(ptr);
    let guard = GLOBAL_ALLOCATIONS.lock().unwrap();
    let current_thread = current_thread_id();
    guard
        .get(&raw_id)
        .ok_or_else(|| format!("Invalid pointer: {}", ptr))
        .and_then(|entry| match &entry.state {
            PtrState::Alive {
                elem_size,
                owner_thread,
                ..
            } => {
                if *owner_thread != current_thread {
                    eprintln!(
                        "Warning: elem_size query on pointer {} owned by thread {} from thread {}",
                        ptr, owner_thread, current_thread
                    );
                }
                Ok(*elem_size)
            }
            PtrState::Freed => Err(format!("Cannot get elem_size of freed pointer: {}", ptr)),
        })
}

/// Get pointer state (for debugging/testing)
#[inline]
pub fn get_state(ptr: u64) -> Option<PtrState> {
    let raw_id = decode_handle(ptr);
    GLOBAL_ALLOCATIONS
        .lock()
        .unwrap()
        .get(&raw_id)
        .map(|entry| entry.state.clone())
}

/// Returns true if `ptr` is a valid, live (not-freed) heap allocation handle.
/// Used by the interpreter to decide whether to tag a numeric value as a pointer label.
#[inline]
pub fn is_live_ptr(ptr: u64) -> bool {
    let raw_id = decode_handle(ptr);
    GLOBAL_ALLOCATIONS
        .lock()
        .ok()
        .and_then(|g| {
            g.get(&raw_id)
                .map(|e| matches!(e.state, PtrState::Alive { .. }))
        })
        .unwrap_or(false)
}
/// Begin a region (arena). Nested regions are supported per thread.
#[inline]
pub fn begin_region(name: &str) {
    let tid = current_thread_id();
    let mut stacks = REGION_STACKS.lock().unwrap();
    let stack = stacks.entry(tid).or_insert_with(Vec::new);
    stack.push(RegionCtx {
        name: name.to_string(),
        allocations_raw: Vec::new(),
    });
}

/// End a region and free all allocations recorded within.
#[inline]
pub fn end_region(name: &str) -> Result<(), String> {
    let tid = current_thread_id();
    let mut stacks = REGION_STACKS.lock().unwrap();
    let stack = stacks
        .get_mut(&tid)
        .ok_or_else(|| "No active region stack".to_string())?;
    let ctx = stack.pop().ok_or_else(|| "No active region".to_string())?;
    if ctx.name != name {
        // Put it back to avoid corruption
        stack.push(ctx.clone());
        return Err(format!(
            "Mismatched region end: expected '{}', got '{}'",
            ctx.name, name
        ));
    }
    drop(stacks); // release lock before freeing

    // Free all allocations; ignore errors for already-freed
    for raw_id in ctx.allocations_raw {
        let handle = encode_handle(raw_id);
        let _ = free(handle);
    }
    Ok(())
}
