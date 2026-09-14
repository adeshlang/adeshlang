# Production thread examples (`production_*`)

This folder demonstrates **native OS threads**, not the async `spawn` / `await`
runtime. The two `production_*` programs are the end-to-end pieces: they keep a
**stable, named set of OS threads** alive long enough to inspect in Task Manager
or Resource Monitor, then shut down with **cancellation, timeouts, and a process
watchdog**.

| File | Role | Nominal run | In-script watchdog | Suggested CLI |
| --- | --- | --- | --- | --- |
| `production_order_pipeline.adesh` | Full ingest → compute → flush pipeline plus almost every `thread` primitive | ~2 s of work (4 × 500 ms ticks) | `thread.watchdog(30000)` | `--timeout 20s` |
| `production_thread_soak.adesh` | Hold a fixed thread set and print `thread.list()` every 3 s | 90 s | `thread.watchdog(RUN_MS + 20000)` → 110 s | `--timeout 120s` |
| `test_pipeline_minimal.adesh` | Smaller cousin of the order pipeline (3 workers + pool) | ~2 s | `thread.watchdog(15000)` | `--timeout 15s` |

Always run these with the **interpreter**. Closures moved onto OS threads are
executed by the interpreter `Value` API. JIT/AOT backends do not yet run this
full thread surface the same way.

```bash
adeshlang run examples/concurrency/production_order_pipeline.adesh --interpreter --timeout 20s
adeshlang run examples/concurrency/production_thread_soak.adesh --interpreter --timeout 120s
```

On Windows, while a program is in its live phase:

- Task Manager → Details → `adeshlang.exe` → right-click column header → enable **Threads**
- Resource Monitor → CPU → Threads → filter `adeshlang`
- Process Explorer / wait-chain analysis on the process

`thread.list()` is the **language registry** (named threads the runtime knows
about). The OS thread count is usually a few higher (interpreter stack thread,
JIT helpers, I/O, the watchdog itself).

---

## Two different “spawn”s

| API | What it is | Join | Typical use |
| --- | --- | --- | --- |
| `thread.spawn` / `thread.spawn_named` / `thread.builder()` | Real OS thread | `join` / `join_timeout` | Blocking workers, pools, pipelines |
| `spawn` + `await` (async) | Task on the async event loop | promise / await | I/O concurrency without extra OS threads |

The numbered files `01_*.adesh` … `24_*.adesh` teach one primitive at a time.
The `production_*` files compose those primitives the way a service would.

`thread` / `Thread` / `std:thread` are the same import-only namespace.

---

## Why `--timeout` exists

### The failure mode

AdeshLang thread workers are **OS threads**. On Windows (and most Unix
runtimes), when `main` returns the process still waits for every non-exited
thread. That means:

1. A worker stuck in `Barrier.wait`, `WaitGroup.wait`, `Channel.recv`, or
   `ThreadPool.join` **never returns**.
2. A `ThreadPool` worker that is still running after `pool.shutdown()` **keeps
   the process alive** even if the AdeshLang script printed “complete”.
3. Dropping a join handle that still owns a live `JoinHandle` would otherwise
   **block process teardown**.

Those hangs look like “the example froze and never stopped.” They are not
Ctrl+C-friendly in the same way a single-threaded script is: extra threads
ignore the fact that the script’s main function is done.

Language-level `join_timeout` / `wait_timeout` fix the *cooperative* case
(workers that poll cancellation and wake from timed waits). They cannot stop a
thread that is wedged inside native work or that never sees `cancel()`.

### What `--timeout` does

`--timeout` / `--timeout-ms` is a **process watchdog**, installed in the CLI
**before** the large-stack `adesh-main` thread starts:

1. Parse a duration: `20s`, `15000ms`, `2m`, or a bare millisecond number
   (`20000`).
2. Spawn a native thread named `adesh-watchdog`.
3. If the deadline is reached and nobody called `disarm`, print
   `[adeshlang] watchdog: process exceeded deadline; killing hung threads (exit 124)`
   and `std::process::exit(124)`.

Exit **124** matches the conventional “timed out” status used by GNU `timeout(1)`.

Equivalent environment variable (milliseconds only):

```bash
# PowerShell
$env:ADESH_RUN_TIMEOUT_MS = "20000"
adeshlang run examples/concurrency/production_order_pipeline.adesh --interpreter
```

If both CLI and env are set, the CLI duration is used (`cli.or(env)`). If a
script also calls `thread.watchdog(ms)`, the runtime keeps the **earliest**
deadline.

### Three layers (why all three)

| Layer | Who installs it | What it can do | What it cannot do |
| --- | --- | --- | --- |
| Cooperative shutdown | The script: `cancel()`, `close()`, `notify_all()`, `wait_timeout`, `join_timeout`, `pool.shutdown()` | Stop well-behaved workers that poll `token.is_cancelled()` and use timed recv/wait | Stop a thread that never wakes |
| In-script `thread.watchdog(ms)` | The `.adesh` file, early after `import thread` | Kill the process if *this program* overruns its own budget, even without CLI flags | Help if the interpreter never starts the script |
| CLI `--timeout` / `ADESH_RUN_TIMEOUT_MS` | `adeshlang` `main` via `boot_cli_watchdog` | Kill even if the script never reaches `thread.watchdog`, or hangs in typecheck/run setup after the watchdog is armed | Distinguish “slow machine” from “deadlock” — pick a limit with headroom |

After a **successful** `run`, the CLI **disarms** the watchdog and
`std::process::exit(0)` so leftover pool threads cannot pin the process.
`thread.disarm_watchdog()` at the end of the production scripts does the same
for the in-script timer so a finished program is not killed a few seconds later.

### How to choose the number

Pick **expected work + generous slack**, never the exact work duration.

- Order pipeline: ~2 s of ticks + shutdown. Script watchdog **30 s**. CLI
  **20 s** is enough on a normal machine and still fails fast if startup
  deadlock happens (`ready.wait_timeout(5000)` already covers that).
- Soak: **90 s** of sampling. Script watchdog **110 s**. CLI **120 s**. A
  `--timeout 20s` here would murder a healthy soak.

`--timeout` is **not** a substitute for graceful shutdown. It is the last
backstop when graceful shutdown fails. Use it in CI and when you are iterating
on threading so a deadlock cannot sit on a core until you kill the terminal.

---

## `production_order_pipeline.adesh` (in depth)

This is a **multi-stage order ingest pipeline** plus a tour of the `thread`
API. It is intentionally “busy”: many primitives are constructed even if a
field is only read (for example `slots.available_permits()`, `cfg` as an
`RwLock`). The point is a production-shaped topology, not a minimal tutorial.

### Constants and live window

```text
RUN_MS     = 2000   # advertised duration (banner); actual live phase is 4 ticks
TICK_MS    = 500    # main thread samples atomics each tick
PRODUCERS  = 3      # ingress-0 .. ingress-2
PIPELINE_WORKERS = 2
```

Main does **four** `thread.sleep(TICK_MS)` samples after startup, so the
pipeline is hot for about **2 seconds**, then shuts down. That is long enough
to see a stable named set in Task Manager, short enough for CI.

`thread.watchdog(30000)` is armed **before** any spawn so a hang on the startup
barrier/latch still dies.

### Language features mixed in

The file is also a stress of ordinary language features on the same heap as the
threads:

- `enum Stage` / `enum Priority` and `st.tag` dispatch (`stage_name`)
- `class PipelineReport` (CPU count, expected thread formula, `clock()` banner)
- Job objects `{ id, payload, kind, priority }` and Result-style `{ ok, value }`
  from `try_send` / `recv_timeout` / `join_timeout`
- Interpolation in names: `` `ingress-${pid}` ``

`PipelineReport.expected_os_threads()` is:

```text
1 (adesh-main)
+ pool_size          (adesh-pool-0 .. N-1, N = max(hardware_concurrency, 2))
+ PRODUCERS          (ingress-*)
+ PIPELINE_WORKERS   (pipeline-*)
+ 2                  (heartbeat + flusher)
```

On a 12-logical-CPU box that prints **~20** expected threads. `thread.list()`
after startup is typically **19** in the registry (main is listed as
`adesh-main` once `thread.name("adesh-main")` runs; pool workers are registered
separately). Treat the formula as a lower bound, not a lockstep OS count.

### Shared state constructed on main (before spawn)

| Object | Type | Role in this program |
| --- | --- | --- |
| `once_log` | `Once` | One-time “logging initialized” print; `call_once` is idempotent |
| `lazy_cpus` | `Lazy` | Caches `hardware_concurrency()` on first `get()` |
| `cfg` | `RwLock` | `{ batch, enabled }` config snapshot (constructed; not on the hot path) |
| `metrics` | `Mutex` | `{ ingested, computed, flushed, overflow }`; flusher uses `metrics.with(...)` |
| `ingested` / `computed` | `AtomicI64` | Hot counters from ingress and pool jobs (`Relaxed` in the loop, `SeqCst` at the end) |
| `accepted` | `AtomicBool` | Constructed for the API tour |
| `slots` | `Semaphore(4)` | Permit counter printed at startup |
| `ready` | `CountDownLatch(7)` | Main waits until all 7 workers pass startup |
| `start_bar` | `Barrier(7)` | All 7 workers start the hot loop together |
| `wg` | `WaitGroup` | `add(7)`; each worker `done()` before return |
| `cancel_src` / `token` | `CancellationSource` + token | Cooperative stop; workers poll `!token.is_cancelled()` |
| `batch_ev` | `Condvar` | Pipeline workers `notify_one` every 16 jobs; flusher `wait_timeout(120)` |
| `tls` | `ThreadLocal` | Each worker `tls.set("ingress-0")` etc. (per-OS-thread slots) |
| `overflow` | `ConcurrentQueue` | Jobs that failed `try_send` on a full ingress channel |
| `pool` | `ThreadPool` | `hardware_concurrency` (min 2) workers named `adesh-pool-i` |
| `ingress` | `Channel.bounded(32)` | MPMC queue: producers → pipeline workers |
| `compute_ch` | `Channel.bounded(32)` | Pipeline workers → flusher (`try_send` of `{ id, score }`) |

**Bounded channels + `try_send`:** producers never block forever on a full
queue. If `try_send` fails (`ok == false`), the job is pushed to `overflow`
instead. That is the production pattern: **backpressure without deadlocking
ingress**.

### Startup protocol (barrier then latch)

Seven workers (3 ingress + 2 pipeline + heartbeat + flusher) each:

1. Set TLS
2. `start_bar.wait()` — last arriver releases everyone
3. `ready.count_down()`

Main then `ready.wait_timeout(5000)`. If the latch does not reach zero (a
spawn failed or a worker died before `count_down`), main **cancels** instead of
blocking forever on `ready.wait()`.

This two-phase start avoids “half the pipeline is already sending while others
are still constructing handles.”

### Data path (hot loop)

```text
ingress-*  --try_send-->  [bounded 32]  --recv_timeout(50)-->  pipeline-*
                                                              |  score_job
                                                              |  pool.execute(LCG + computed++)
                                                              +--try_send-->  [bounded 32]  --try_recv-->  flusher
overflow queue <---------------------------------------------- (failed ingress try_send)
```

**Ingress (`ingress-0..2`)**

- Loop until `token.is_cancelled()`
- Build a job with a stable id `pid * 100000 + seq`
- `tx.try_send(job)`; on success `ingested.fetch_add(1, "Relaxed")`
- `thread.sleep(8)` so the process is inspectable (not a tight 100% CPU spin)

Senders are **cloned** (`tx_in.clone()`) so three producers share one channel
(true MPMC). Clone increments the sender count; `tx_in.close()` on shutdown
wakes receivers.

**Pipeline (`pipeline-0..1`)**

- `rx.recv_timeout(50)` so cancel can be observed even with an empty channel
- On a message: `score_job`, then `pool.execute` of a small LCG (linear
  congruential) step and `computed.fetch_add`
- `tx.try_send({ id, score })` toward the flusher
- Every 16 jobs: `batch_ev.notify_one()`
- `thread.yield()` when idle

Pool work is **unowned fire-and-forget** (`execute`, not `submit`/`get`). That
matches a production ingest path: do not join every micro-job on the pipeline
thread.

**Heartbeat**

- Built with `thread.builder().name("heartbeat").spawn(...)`
- Sleeps 80 ms until cancel
- Shutdown also `heartbeat.unpark()` in case a future version uses `park`
  instead of sleep

**Flusher**

- `batch_ev.wait_timeout(120)` then `reset()` (event-style condvar: notify sets
  ready; reset clears it)
- Drains `rx_cmp.try_recv()` and increments `metrics.flushed` under `Mutex.with`
- Pops at most one overflow job per wake and increments `metrics.overflow`

Mutex `with` takes the lock, runs the closure, stores the returned object back.
The closure **must not** re-enter the same mutex.

### Shutdown sequence (order matters)

```text
cancel_src.cancel()     # workers exit their while !token.is_cancelled() loops
tx_in.close()           # recv_timeout / recv see closed / empty
tx_cmp.close()
batch_ev.notify_all()   # flusher may be in wait_timeout
heartbeat.unpark()
wg.wait_timeout(5000)   # all seven should have called done()
join_timeout(2000)      # each named handle
pool.shutdown()         # stop = true, wake pool workers
pool.join_timeout(3000)
print thread.list()     # expect registry 0 (or only stragglers)
metrics.lock() + atomics SeqCst
thread.disarm_watchdog()
```

Using **`join_timeout` instead of `join`** is the production rule: a stuck
worker prints a timeout object `{ ok: false, kind: "timeout", ... }` and the
script continues. Unbounded `join` is how examples used to never stop.

`WaitGroup.wait` is still available and now **polls every 50 ms** (lost wakeup
cannot hang forever if the counter already hit zero). `wait_timeout` still
returns `false` if workers never call `done()`.

### What a healthy run looks like

```text
tick 1  ingested=…  pool_compute=…
tick 4  ingested=…  pool_compute=…
shutting down: ...
wg.wait_timeout ok=true
pool.join_timeout=null          # null means join succeeded (not a timeout object)
-- after join ... registry=0
final atomics ingested=N computed=M
pipeline complete.
```

`ingested` and `computed` should be close (pool jobs are tiny). `metrics.flushed`
via the mutex may stay 0 if the flusher mostly sees empty `try_recv` after
cancel; atomics are the source of truth for throughput.

---

## `production_thread_soak.adesh` (in depth)

This program is for **counting threads**, not for exercising every primitive.

### Layout

```text
adesh-main
adesh-pool-0 .. adesh-pool-(N-1)     N = hardware_concurrency()
soak-worker-0 .. soak-worker-3
soak-heartbeat
```

Expected ≈ `1 + N + 4 + 1`. Banner prints that formula.

### Live phase (90 seconds)

- Four named workers: each loop `pool.execute` of 200 LCG iterations, then
  `thread.sleep(25)`, until `tok.is_cancelled()`.
- Heartbeat: `ticks_atomic.fetch_add(1)` then `thread.park(200)` (timed park,
  not an unbounded park).
- Main: until `clock() + 90s`, every 3 s print `thread.list()`, `work_units`,
  and heartbeat ticks.

The set of names must stay **stable** for the whole 90 s so you can screenshot
Task Manager. Do not spawn/join extra threads in the loop.

### Shutdown

Same pattern as the pipeline: `cancel`, `unpark` the heartbeat (parked threads
need a wake), `wg.wait_timeout(5000)`, `join_timeout` on workers and beat,
`pool.shutdown` + `join_timeout(3000)`, `disarm_watchdog`.

### Watchdog vs soak duration

`thread.watchdog(RUN_MS + 20000)` is **110 s**. That is 20 s of slack after the
90 s sample window for shutdown. CLI `--timeout 120s` is slightly above that.
Do not use the pipeline’s `--timeout 20s` on the soak.

---

## How the rest of the folder maps onto `production_*`

Read these first if a production file looks opaque. Each is a single idea;
`production_order_pipeline` uses almost all of them together.

| Example | Primitive | Where it appears in production |
| --- | --- | --- |
| `01_basic_thread` | `spawn` + `join` | Ingress / pipeline / flusher handles |
| `02_thread_join` | join after sleep | Shutdown joins |
| `03_thread_return_value` | `join` returns worker value | Ingress returns `seq`; heartbeat returns `thread.id()` |
| `04_named_threads` | `spawn_named` | `ingress-*`, `pipeline-*`, `soak-worker-*` |
| `06_mutex` | `Mutex` / `with` / `lock` | `metrics` |
| `07_rwlock` | `RwLock` | `cfg` |
| `08_condvar` | `Condvar` wait/notify | `batch_ev` |
| `09_semaphore` | `Semaphore` | `slots` |
| `10_barrier` | `Barrier.wait` | `start_bar` |
| `11_atomic_counter` | `AtomicI64` | `ingested`, `computed`, soak `work_units` |
| `12_channels` / `13_multi_producer` | bounded channel, clone sender | `ingress`, `compute_ch` |
| `14_thread_pool` / `22_work_stealing` | `ThreadPool.execute` / `join_timeout` | Compute offload |
| `15_scoped_threads` | `thread.scope` | Not used in production_* (joins are explicit) |
| `16_thread_local` | `ThreadLocal` | `tls.set` per worker |
| `17_cancellation` / `24_graceful_shutdown` | cancel token + waitgroup | Hot-loop stop + `wg.done` |
| `20_producer_consumer` / `21_pipeline` | staged channels | The order pipeline topology |
| `test_pipeline_minimal` | Reduced 3-thread + pool pipeline | Debug hang without enums/classes |

Numbered examples that spawn threads also call `thread.watchdog(15000)` so a
forgotten `join` cannot wedge a `adeshlang run` indefinitely.

---

## Operational checklist

1. Use `--interpreter` for anything that `thread.spawn`s language closures.
2. Put `--timeout` **above** the expected runtime (20 s pipeline, 120 s soak).
3. Prefer `try_send` / `recv_timeout` / `wait_timeout` / `join_timeout` in
   anything that must exit.
4. Shutdown order: **cancel → close/notify → waitgroup timeout → join timeout →
   pool shutdown/join timeout → disarm**.
5. If you see exit **124**, the watchdog fired: the script did not finish and
   disarm. Capture the last printed line (`waiting for workers…` vs
   `joining pool…`) to see which phase deadlocked.
6. `thread.list()` going to `0` after join is the success criterion for “no
   leaked named workers.” OS tools may still show the process until the CLI
   `process::exit(0)`.

---

## Related runtime behavior (for reading the implementation)

- Watchdog: `src/runtime/thread/mod.rs` (`install_process_watchdog`,
  `boot_cli_watchdog`, exit 124).
- CLI flag: `src/toolchain/cli/args.rs` (`--timeout`, `--timeout-ms`).
- CLI install + process exit after run: `src/main.rs`.
- Thread stdlib (spawn, pool, channels, waitgroup): `src/runtime/stdlib_src/concurrency/`.
- Join handles that are dropped without a successful join wait at most
  `DROP_JOIN_TIMEOUT` (3 s) then detach, so interpreter teardown cannot hang
  forever on one stuck worker.
