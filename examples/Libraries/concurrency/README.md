# Concurrency and Parallelism

AdeshLang provides **native OS threads** (`thread` / `Thread` / `std:thread`)
plus the async `spawn`/`await` runtime. They are different:

- `thread.spawn` — OS thread, blocking join, Send closures
- `spawn` (async) — task/promise on the async runtime

**Production / soak examples (full write-up):**
[`PRODUCTION.md`](PRODUCTION.md) — pipeline topology, every primitive those
files use, shutdown order, and why `--timeout` / `thread.watchdog` exist.

## Threading examples

Import first (`thread`, `Thread`, or `std:thread`), then use `thread.` / `Thread.`.

```bash
adeshlang run examples/concurrency/01_basic_thread.adesh --interpreter --timeout 15s
adeshlang run examples/concurrency/03_thread_return_value.adesh --interpreter --timeout 15s
adeshlang run examples/concurrency/04_named_threads.adesh --interpreter --timeout 15s
adeshlang run examples/concurrency/12_channels.adesh --interpreter --timeout 15s
adeshlang run examples/concurrency/14_thread_pool.adesh --interpreter --timeout 15s
```

Numbered files `01`–`24` each isolate one primitive. Run them with the
**interpreter**. `--timeout` is a process watchdog (exit **124** if the run
never finishes). See [PRODUCTION.md](PRODUCTION.md#why---timeout-exists).

## Production / soak (native thread count)

These keep a **stable** set of named OS threads alive so you can count them in
Task Manager, Resource Monitor, or Process Explorer.

```bash
# Full pipeline: types/enums/classes + thread primitives. Completes in ~2s.
adeshlang run examples/concurrency/production_order_pipeline.adesh --interpreter --timeout 20s

# Soak: ~90 seconds, prints thread.list() every 3s. Watchdog at 110s.
adeshlang run examples/concurrency/production_thread_soak.adesh --interpreter --timeout 120s
```

Expected count is printed at startup (`hardware_concurrency` pool workers plus
named ingress/pipeline/heartbeat threads). `thread.list()` is the language
registry; the OS view may be a few higher (JIT/runtime helpers, watchdog).

## Async examples

```bash
adeshlang run examples/concurrency/async_basics.adesh --jit
```
