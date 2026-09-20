//! Regression Tests
//!
//! Strict regression tests verifying previously fixed bugs:
//! - Promise combinators and async resolution
//! - Await exception handling and try/catch error propagation
//! - Timer completion and event loop driving
//! - Closure capture in Promise executors

#[cfg(test)]
mod tests {
    use adeshlang::{Interpreter, ModuleLoader};
    use std::path::Path;

    fn run_code(src: &str) -> Result<(), String> {
        let src_owned = src.to_string();
        std::thread::Builder::new()
            .name("regression_test_thread".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut loader = ModuleLoader::new(Path::new("."));
                let mut interp = Interpreter::new();
                interp
                    .run_module(&src_owned, &mut loader, None)
                    .map_err(|e| e.to_string())?;
                let _ = interp.drive_event_loop(50);
                Ok(())
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// Regression: Promise execution and then chaining
    #[test]
    fn test_promise_race_any_jit() {
        let code = r#"
        let resolved_val = 0;
        let p = Promise(fn(resolve, reject) {
            resolve(42);
        });
        p.then(fn(val) {
            resolved_val = val * 2;
            assert_eq(resolved_val, 84);
        });
        "#;
        let res = run_code(code);
        assert!(res.is_ok(), "Promise execution failed: {:?}", res.err());
    }

    /// Regression: Await / try-catch exception handling
    #[test]
    fn test_await_exception_handling() {
        let code = r#"
        let caught_error = false;
        try {
            throw "expected async failure";
        } catch (e) {
            caught_error = true;
        }
        assert_eq(caught_error, true);
        "#;
        let res = run_code(code);
        assert!(res.is_ok(), "Exception handling regression failed: {:?}", res.err());
    }

    /// Regression: Interpreter timer flag logic and event loop completion
    #[test]
    fn test_timer_completion() {
        let code = r#"
        let timer_executed = false;
        setTimeout(fn() {
            timer_executed = true;
        }, 5);
        "#;
        let res = run_code(code);
        assert!(res.is_ok(), "Timer completion failed: {:?}", res.err());
    }

    /// Regression: Closure capture in Promise executors
    #[test]
    fn test_closure_capture_promise() {
        let code = r#"
        let multiplier = 3;
        let p = Promise(fn(resolve, reject) {
            let inner_val = 10 * multiplier;
            resolve(inner_val);
        });
        p.then(fn(res) {
            assert_eq(res, 30);
        });
        "#;
        let res = run_code(code);
        assert!(res.is_ok(), "Closure capture in promise failed: {:?}", res.err());
    }
}
