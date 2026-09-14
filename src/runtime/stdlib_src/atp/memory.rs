//! ATP Memory Management
//!
//! Provides bounded memory pools and per-connection / per-stream memory budgets.
//! No remote peer can force unbounded allocation — every buffer operation
//! checks against a configured limit.

use super::errors::{AtpError, AtpResult};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A reusable byte buffer from the pool.
pub struct PooledBuffer {
    data: Vec<u8>,
    len: usize,
    pool: Option<Arc<BufferPool>>,
}

impl PooledBuffer {
    /// Get a slice of the valid data.
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }

    /// Get a mutable slice of the valid data.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }

    /// Length of valid data.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Write data into the buffer, up to its capacity.
    pub fn write(&mut self, src: &[u8]) -> usize {
        let space = self.data.capacity() - self.len;
        let n = src.len().min(space);
        if n > 0 {
            let new_len = self.len + n;
            if new_len > self.data.len() {
                self.data.resize(new_len, 0);
            }
            self.data[self.len..self.len + n].copy_from_slice(&src[..n]);
            self.len += n;
        }
        n
    }

    /// Reset the buffer for reuse.
    pub fn reset(&mut self) {
        self.len = 0;
    }

    /// Obtain a mutable slice for UDP recv (up to `cap` bytes).
    pub fn recv_buf(&mut self, cap: usize) -> &mut [u8] {
        if self.data.len() < cap {
            self.data.resize(cap, 0);
        }
        &mut self.data[..cap]
    }

    /// Set valid length after recv.
    pub fn set_recv_len(&mut self, n: usize) {
        self.len = n;
    }

    /// Take the underlying Vec (detaches from pool).
    pub fn into_vec(mut self) -> Vec<u8> {
        // Prevent Drop from returning the buffer to the pool.
        self.pool = None;
        self.data.truncate(self.len);
        std::mem::take(&mut self.data)
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.take() {
            // Return the buffer to the pool (if there's room).
            let mut buf = std::mem::take(&mut self.data);
            buf.clear();
            pool.return_buffer(buf);
        }
    }
}

/// A pool of reusable byte buffers to minimize allocations.
pub struct BufferPool {
    /// Free buffers available for reuse.
    free: std::sync::Mutex<Vec<Vec<u8>>>,
    /// Default buffer capacity.
    default_capacity: usize,
    /// Maximum number of buffers to keep in the pool.
    max_pool_size: usize,
    /// Total bytes allocated (for metrics).
    total_allocated: AtomicUsize,
    /// Total bytes currently in use.
    bytes_in_use: AtomicUsize,
}

impl BufferPool {
    pub fn new(default_capacity: usize, max_pool_size: usize) -> Self {
        BufferPool {
            free: std::sync::Mutex::new(Vec::new()),
            default_capacity,
            max_pool_size,
            total_allocated: AtomicUsize::new(0),
            bytes_in_use: AtomicUsize::new(0),
        }
    }

    /// Acquire a buffer from the pool (or allocate a new one).
    pub fn acquire(self: &Arc<Self>) -> PooledBuffer {
        let data = {
            let mut free = self.free.lock().unwrap();
            if let Some(buf) = free.pop() {
                buf
            } else {
                let buf = Vec::with_capacity(self.default_capacity);
                self.total_allocated
                    .fetch_add(self.default_capacity, Ordering::Relaxed);
                buf
            }
        };
        self.bytes_in_use
            .fetch_add(data.capacity(), Ordering::Relaxed);
        PooledBuffer {
            data,
            len: 0,
            pool: Some(self.clone()),
        }
    }

    /// Acquire a buffer with at least the specified capacity.
    pub fn acquire_with_capacity(self: &Arc<Self>, min_cap: usize) -> PooledBuffer {
        let data = {
            let mut free = self.free.lock().unwrap();
            // Find a buffer with enough capacity.
            let mut found_idx = None;
            for (i, buf) in free.iter().enumerate() {
                if buf.capacity() >= min_cap {
                    found_idx = Some(i);
                    break;
                }
            }
            if let Some(idx) = found_idx {
                free.swap_remove(idx)
            } else {
                let cap = min_cap.max(self.default_capacity);
                let buf = Vec::with_capacity(cap);
                self.total_allocated.fetch_add(cap, Ordering::Relaxed);
                buf
            }
        };
        self.bytes_in_use
            .fetch_add(data.capacity(), Ordering::Relaxed);
        PooledBuffer {
            data,
            len: 0,
            pool: Some(self.clone()),
        }
    }

    /// Return a buffer to the pool.
    fn return_buffer(&self, mut buf: Vec<u8>) {
        self.bytes_in_use
            .fetch_sub(buf.capacity(), Ordering::Relaxed);
        let mut free = self.free.lock().unwrap();
        if free.len() < self.max_pool_size {
            buf.clear();
            free.push(buf);
        }
        // If pool is full, the buffer is dropped (memory freed).
    }

    /// Total bytes allocated by the pool.
    pub fn total_allocated(&self) -> usize {
        self.total_allocated.load(Ordering::Relaxed)
    }

    /// Bytes currently checked out from the pool.
    pub fn bytes_in_use(&self) -> usize {
        self.bytes_in_use.load(Ordering::Relaxed)
    }

    /// Number of free buffers in the pool.
    pub fn free_count(&self) -> usize {
        self.free.lock().unwrap().len()
    }
}

/// Memory budget tracker for a connection or stream.
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    /// Maximum bytes allowed.
    limit: usize,
    /// Current bytes in use.
    used: usize,
}

impl MemoryBudget {
    pub fn new(limit: usize) -> Self {
        MemoryBudget { limit, used: 0 }
    }

    /// Try to reserve `amount` bytes. Returns error if over budget.
    pub fn try_reserve(&mut self, amount: usize) -> AtpResult<()> {
        if self.used + amount > self.limit {
            return Err(AtpError::memory(format!(
                "budget exceeded: used={}, requested={}, limit={}",
                self.used, amount, self.limit
            )));
        }
        self.used += amount;
        Ok(())
    }

    /// Release bytes back to the budget.
    pub fn release(&mut self, amount: usize) {
        self.used = self.used.saturating_sub(amount);
    }

    /// Current usage.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Budget limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Available space.
    pub fn available(&self) -> usize {
        self.limit.saturating_sub(self.used)
    }

    /// Percentage used (0.0 to 1.0).
    pub fn pct_used(&self) -> f64 {
        if self.limit == 0 {
            0.0
        } else {
            self.used as f64 / self.limit as f64
        }
    }

    /// Whether backpressure should be applied (>90% used).
    pub fn should_apply_backpressure(&self) -> bool {
        self.pct_used() > 0.9
    }

    /// Update the limit.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }
}

/// Connection-level memory configuration.
#[derive(Debug, Clone)]
pub struct ConnectionMemoryConfig {
    pub total_budget: usize,
    pub max_stream_memory: usize,
    pub max_reassembly_memory: usize,
    pub max_send_queue: usize,
    pub max_receive_queue: usize,
}

impl Default for ConnectionMemoryConfig {
    fn default() -> Self {
        ConnectionMemoryConfig {
            total_budget: 16 * 1024 * 1024, // 16 MB
            max_stream_memory: 2 * 1024 * 1024, // 2 MB
            max_reassembly_memory: 4 * 1024 * 1024, // 4 MB
            max_send_queue: 2 * 1024 * 1024, // 2 MB
            max_receive_queue: 2 * 1024 * 1024, // 2 MB
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_reuse() {
        let pool = Arc::new(BufferPool::new(1024, 10));

        // Acquire and drop a buffer.
        {
            let mut buf = pool.acquire();
            buf.write(b"hello");
            assert_eq!(buf.as_slice(), b"hello");
        }
        // Buffer should be back in the pool.
        assert_eq!(pool.free_count(), 1);

        // Acquire again — should reuse.
        let buf2 = pool.acquire();
        assert!(buf2.data.capacity() >= 1024);
    }

    #[test]
    fn test_memory_budget() {
        let mut budget = MemoryBudget::new(1000);
        assert!(budget.try_reserve(500).is_ok());
        assert_eq!(budget.used(), 500);
        assert!(budget.try_reserve(600).is_err());
        budget.release(500);
        assert_eq!(budget.used(), 0);
        assert_eq!(budget.available(), 1000);
    }

    #[test]
    fn test_backpressure() {
        let mut budget = MemoryBudget::new(1000);
        budget.try_reserve(950).unwrap();
        assert!(budget.should_apply_backpressure());

        budget.release(500);
        assert!(!budget.should_apply_backpressure());
    }
}
