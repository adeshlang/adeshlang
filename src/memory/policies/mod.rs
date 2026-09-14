use std::sync::OnceLock;

/// Threading mode for ARC semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadMode {
    /// Single-threaded ARC (Rc-style, non-atomic refs)
    SingleThread,
    /// Multi-threaded ARC (Arc-style, atomic refs)
    MultiThread,
}

/// Global memory policy enforced by the runtime.
#[derive(Debug, Clone, Copy)]
pub struct MemoryPolicy {
    /// Embedded mode disables heap + ARC/weak.
    pub embedded: bool,
    /// Whether ARC is allowed (opt-in via `share`).
    pub allow_arc: bool,
    /// Whether weak references are allowed.
    pub allow_weak: bool,
    /// Whether heap allocation is allowed (arena + stack are always allowed).
    pub allow_heap: bool,
    /// Whether debug poisoning is enabled for freed memory.
    pub debug_poison: bool,
    /// Threading mode for ARC.
    pub thread_mode: ThreadMode,
}

impl MemoryPolicy {
    /// Default policy: desktop, heap + ARC enabled.
    pub const fn standard() -> Self {
        Self {
            embedded: false,
            allow_arc: true,
            allow_weak: true,
            allow_heap: true,
            debug_poison: cfg!(debug_assertions),
            thread_mode: ThreadMode::SingleThread,
        }
    }

    /// Embedded policy: heap + ARC/weak disabled.
    pub const fn embedded() -> Self {
        Self {
            embedded: true,
            allow_arc: false,
            allow_weak: false,
            allow_heap: false,
            debug_poison: cfg!(debug_assertions),
            thread_mode: ThreadMode::SingleThread,
        }
    }
}

static MEMORY_POLICY: OnceLock<MemoryPolicy> = OnceLock::new();

/// Set the process-wide memory policy. Subsequent calls are ignored.
pub fn set_memory_policy(policy: MemoryPolicy) {
    let _ = MEMORY_POLICY.set(policy);
}

/// Get the currently active memory policy.
pub fn memory_policy() -> MemoryPolicy {
    *MEMORY_POLICY.get().unwrap_or(&MemoryPolicy::standard())
}
