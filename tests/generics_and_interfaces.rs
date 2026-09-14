//! End-to-end tests for class generics and interfaces.

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_test_thread".into())
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
fn test_generic_class_instantiation() {
    let src = r#"
class Box<T> {
    fn init(val: T) {
        this.value = val;
    }
    fn get_value(): T {
        return this.value;
    }
}
let b = new Box(100);
print(b.get_value());
"#;
    let res = run_code(src);
    assert!(
        res.is_ok(),
        "Expected Box to compile and run, got error: {:?}",
        res.err()
    );
}

#[test]
fn test_interface_compliance() {
    let src = r#"
interface Speaker {
    fn speak();
}

class Parrot implements Speaker {
    fn speak(): string {
        return "Polly wants a cracker";
    }
}

fn make_sound(s: Speaker) {
    print(s.speak());
}

let p = new Parrot();
make_sound(p);
"#;
    let res = run_code(src);
    assert!(
        res.is_ok(),
        "Expected interface implementation to pass, got error: {:?}",
        res.err()
    );
}

#[test]
fn test_generic_interface_implementation() {
    let src = r#"
interface Cache<T> {
    fn get();
}

class ValueCache<T> implements Cache {
    fn init(v: T) {
        this.value = v;
    }
    fn get(): T {
        return this.value;
    }
}

let c = new ValueCache("secret");
print(c.get());
"#;
    let res = run_code(src);
    assert!(
        res.is_ok(),
        "Expected generic interface implementation to pass, got error: {:?}",
        res.err()
    );
}
