//! Timer Manager
//!
//! Lightweight thread-based timers used by the async runtime:
//! - `spawn_timer(id, flag, is_interval, ms)`: schedules one-shot or interval
//! - `cancel_timer(id)`: cancels active timers
//!
//! Dropping the manager cancels all outstanding timers.
use crate::utils::collections::FastMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

pub struct TimerManager {
    tx: mpsc::Sender<u64>,
    handles: Mutex<FastMap<u64, Arc<AtomicBool>>>,
}

impl TimerManager {
    pub fn new() -> (Arc<Self>, mpsc::Receiver<u64>) {
        let (tx, rx) = mpsc::channel();
        let mgr = TimerManager {
            tx,
            handles: Mutex::new(FastMap::default()),
        };
        (Arc::new(mgr), rx)
    }

    pub fn sender(&self) -> mpsc::Sender<u64> {
        self.tx.clone()
    }

    pub fn spawn_timer(&self, id: u64, flag: Arc<AtomicBool>, is_interval: bool, ms: u64) {
        // record handle
        {
            let mut h = self.handles.lock().unwrap();
            h.insert(id, flag.clone());
        }
        let tx = self.tx.clone();
        thread::spawn(move || {
            if is_interval {
                while flag.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    if !flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let _ = tx.send(id);
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                if flag.load(Ordering::SeqCst) {
                    let _ = tx.send(id);
                }
            }
        });
    }

    pub fn cancel_timer(&self, id: u64) {
        let mut h = self.handles.lock().unwrap();
        if let Some(flag) = h.remove(&id) {
            flag.store(false, Ordering::SeqCst);
        }
    }
}

impl Drop for TimerManager {
    fn drop(&mut self) {
        let mut h = self.handles.lock().unwrap();
        for (_id, flag) in h.drain() {
            flag.store(false, Ordering::SeqCst);
        }
    }
}
