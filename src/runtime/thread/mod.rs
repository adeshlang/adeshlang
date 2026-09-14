//! Native OS threading core for AdeshLang.
//!
//! No GC. Lifecycle is RAII + `Arc`/`Weak`. Single-threaded programs pay
//! almost nothing: no background threads until spawn/pool is used.
//!
//! Happens-before:
//! - spawn: parent writes happen-before child execution starts
//! - join: child completion happens-before join returns in the parent
//! - mutex unlock happens-before a later lock of the same mutex

pub mod platform;
pub use platform::logical_cpu_count;

use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::Thread as StdThread;
use std::time::{Duration, Instant};

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CURRENT_ID: AtomicU64 = const { AtomicU64::new(0) };
    static CURRENT_NAME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Portable thread id. Monotonic, never reused, cheap to compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn allocate() -> Self {
        ThreadId(NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn current() -> Self {
        CURRENT_ID.with(|slot| {
            let mut id = slot.load(Ordering::Relaxed);
            if id == 0 {
                id = ThreadId::allocate().0;
                slot.store(id, Ordering::Relaxed);
            }
            ThreadId(id)
        })
    }

    pub fn install(id: ThreadId) {
        CURRENT_ID.with(|slot| slot.store(id.0, Ordering::Relaxed));
    }
}

pub fn current_name() -> Option<String> {
    CURRENT_NAME.with(|n| n.borrow().clone())
}

pub fn set_current_name(name: Option<String>) {
    CURRENT_NAME.with(|n| *n.borrow_mut() = name.clone());
    if let Some(ref n) = name {
        platform::set_os_thread_name(n);
    }
}

/// Diagnostics-only registry. Not on the spawn/join/lock fast path.
struct RegistryEntry {
    std_thread: StdThread,
    name: Option<String>,
}

fn registry() -> &'static Mutex<FxHashMap<u64, RegistryEntry>> {
    static REG: OnceLock<Mutex<FxHashMap<u64, RegistryEntry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(FxHashMap::default()))
}

pub fn register_current(id: ThreadId, name: Option<String>) {
    let entry = RegistryEntry {
        std_thread: std::thread::current(),
        name: name.clone(),
    };
    if let Ok(mut g) = registry().lock() {
        g.insert(id.as_u64(), entry);
    }
    set_current_name(name);
}

pub fn unregister(id: ThreadId) {
    if let Ok(mut g) = registry().lock() {
        g.remove(&id.as_u64());
    }
}

pub fn unpark_id(id: u64) -> Result<(), String> {
    let g = registry()
        .lock()
        .map_err(|_| "thread registry poisoned".to_string())?;
    match g.get(&id) {
        Some(e) => {
            e.std_thread.unpark();
            Ok(())
        }
        None => Err(format!("unknown thread id {id}")),
    }
}

pub fn list_thread_ids() -> Vec<(u64, Option<String>)> {
    registry()
        .lock()
        .map(|g| g.iter().map(|(k, v)| (*k, v.name.clone())).collect())
        .unwrap_or_default()
}

/// Cooperative cancellation. SAFE, THREAD-SAFE, NON-BLOCKING poll.
#[derive(Debug)]
pub struct CancellationInner {
    pub cancelled: std::sync::atomic::AtomicBool,
}

impl CancellationInner {
    pub fn new() -> Self {
        Self {
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub fn park() {
    std::thread::park();
}

pub fn park_timeout(dur: Duration) {
    std::thread::park_timeout(dur);
}

pub fn yield_now() {
    std::thread::yield_now();
}

pub fn sleep(dur: Duration) {
    if dur.is_zero() {
        yield_now();
        return;
    }
    std::thread::sleep(dur);
}

pub fn sleep_until(deadline: Instant) {
    let now = Instant::now();
    if deadline > now {
        std::thread::sleep(deadline - now);
    }
}

pub fn hardware_concurrency() -> usize {
    platform::logical_cpu_count()
}

pub fn duration_from_millis_f64(ms: f64) -> Result<Duration, String> {
    if !ms.is_finite() || ms < 0.0 {
        return Err("duration must be a non-negative finite number of milliseconds".into());
    }
    if ms > (u64::MAX / 2) as f64 {
        return Err("duration overflow".into());
    }
    Ok(Duration::from_secs_f64(ms / 1000.0))
}

/// Maximum time `JoinOnDrop` will block process teardown waiting for a worker.
pub const DROP_JOIN_TIMEOUT: Duration = Duration::from_secs(3);

struct WatchdogInner {
    deadline: Mutex<Option<Instant>>,
    disarmed: AtomicBool,
    cvar: Condvar,
    started: AtomicBool,
}

fn watchdog_state() -> &'static WatchdogInner {
    static STATE: OnceLock<WatchdogInner> = OnceLock::new();
    STATE.get_or_init(|| WatchdogInner {
        deadline: Mutex::new(None),
        disarmed: AtomicBool::new(false),
        cvar: Condvar::new(),
        started: AtomicBool::new(false),
    })
}

/// Kill the process if it is still running after `timeout`.
/// Multiple installs keep the earliest deadline. `disarm_watchdog` cancels it.
pub fn install_process_watchdog(timeout: Duration) {
    if timeout.is_zero() {
        return;
    }
    let st = watchdog_state();
    st.disarmed.store(false, Ordering::Release);
    {
        let mut dl = st.deadline.lock().unwrap_or_else(|p| p.into_inner());
        let next = Instant::now() + timeout;
        *dl = Some(match *dl {
            Some(old) => old.min(next),
            None => next,
        });
    }
    st.cvar.notify_all();
    if st
        .started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("adesh-watchdog".into())
        .spawn(watchdog_loop);
}

fn watchdog_loop() {
    let st = watchdog_state();
    loop {
        if st.disarmed.load(Ordering::Acquire) {
            return;
        }
        let guard = st.deadline.lock().unwrap_or_else(|p| p.into_inner());
        let Some(deadline) = *guard else {
            drop(st.cvar.wait(guard));
            continue;
        };
        let now = Instant::now();
        if now >= deadline {
            drop(guard);
            if st.disarmed.load(Ordering::Acquire) {
                return;
            }
            eprintln!(
                "[adeshlang] watchdog: process exceeded deadline; killing hung threads (exit 124)"
            );
            std::process::exit(124);
        }
        let remain = deadline.saturating_duration_since(now);
        let (g, _) = st
            .cvar
            .wait_timeout(guard, remain.min(Duration::from_millis(200)))
            .unwrap_or_else(|e| e.into_inner());
        drop(g);
    }
}

/// Stop the watchdog so a finished program can exit immediately.
pub fn disarm_watchdog() {
    let st = watchdog_state();
    st.disarmed.store(true, Ordering::Release);
    if let Ok(mut dl) = st.deadline.lock() {
        *dl = None;
    }
    st.cvar.notify_all();
}

/// CLI / `ADESH_RUN_TIMEOUT_MS` process watchdog. Safe to call more than once.
pub fn boot_cli_watchdog(cli: Option<Duration>) {
    let from_env = std::env::var("ADESH_RUN_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis);
    if let Some(d) = cli.or(from_env) {
        install_process_watchdog(d);
    }
}
