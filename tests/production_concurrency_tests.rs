//! Production Concurrency, Multi-Threaded Channels, Atomics and ThreadPool Tests

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_test_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_concurrency_test".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_str, &mut loader, None)
        })
        .unwrap();
    h.join().unwrap().map_err(|e| e.to_string())
}

#[test]
fn test_producer_consumer_channel_pipeline() {
    let code = r#"
        import thread;

        let pair = thread.Channel.bounded(8);
        let tx = pair[0];
        let rx = pair[1];

        let producer = thread.spawn(fn() {
            let i = 1;
            while (i <= 5) {
                tx.send(i * 10);
                i = i + 1;
            }
            tx.close();
            return "PRODUCER_DONE";
        });

        let sum = 0;
        while (true) {
            let res = rx.recv();
            if (!res.ok) {
                break;
            }
            sum = sum + res.value;
        }

        let p_res = producer.join();
        assert_eq(p_res, "PRODUCER_DONE");
        assert_eq(sum, 150); // 10 + 20 + 30 + 40 + 50
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_atomic_fetch_operations_across_threads() {
    let code = r#"
        import thread;

        let counter = thread.Atomic.AtomicI64.new(100);

        let t1 = thread.spawn(fn() {
            counter.fetch_add(50);
            return counter.load();
        });

        let t2 = thread.spawn(fn() {
            counter.fetch_sub(25);
            return counter.load();
        });

        t1.join();
        t2.join();

        assert_eq(counter.load(), 125);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_threadpool_and_parallel_reduction() {
    let code = r#"
        import thread;

        let numbers = [1, 2, 3, 4, 5, 6, 7, 8];
        let mapped = thread.parallel_map(numbers, fn(x) {
            return x * 2;
        });

        let total = thread.parallel_reduce(mapped, 0, fn(acc, x) {
            return acc + x;
        });

        assert_eq(total, 72); // 2*(1+2+3+4+5+6+7+8) = 2*36 = 72
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}
