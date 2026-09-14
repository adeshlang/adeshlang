use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_with_large_stack<F>(f: F)
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("failed to spawn test thread");
    let res = handle.join().expect("test thread panicked");
    assert!(res.is_ok(), "{}", res.unwrap_err());
}

#[test]
fn promise_then_chaining() {
    let src = r#"
let p = Promise(fn(res, rej){ res(5); });
let r = p.then(fn(x){ return x * 2; }).then(fn(y){ return y + 3; });
let out = r.then(fn(z){ print(z); });
"#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)
    });
}

#[test]
fn async_function_returns_promise() {
    let src = r#"
async fn addLater(a,b){ return a + b; }
let p = addLater(10, 7).then(fn(v){ print(v); });
"#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)
    });
}

#[test]
fn set_timeout_fires() {
    let src = r#"
let fired = 0;
setTimeout(fn(){ fired = 42; }, 10);
// drive event loop for ~50ms to allow timer to fire
"#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)?;
        // run event loop
        let _ = interp.drive_event_loop(50);
        Ok(())
    });
}
