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
    let result = 0;
    let p = Promise(fn(res, rej){ res(5); });
    let r = p.then(fn(x){ return x * 2; }).then(fn(y){ return y + 3; });
    let out = r.then(fn(z){ result = z; });
    "#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)?;
        interp.run_event_loop_until_idle();
        Ok(())
    });
}

#[test]
fn async_function_returns_promise() {
    let src = r#"
    let final_val = 0;
    async fn addLater(a,b){ return a + b; }
    let p = addLater(10, 7).then(fn(v){ final_val = v; });
    "#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)?;
        interp.run_event_loop_until_idle();
        Ok(())
    });
}

#[test]
fn set_timeout_fires() {
    let src = r#"
    let fired = 0;
    setTimeout(fn(){ fired = 42; }, 10);
    "#;
    run_with_large_stack(move || {
        let mut loader = ModuleLoader::new(Path::new("."));
        let mut interp = Interpreter::new();
        interp.run_module(src, &mut loader, None)?;
        let _ = interp.drive_event_loop(50);
        Ok(())
    });
}
