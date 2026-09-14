//! Production OOP, Class Inheritance, Multiple Interfaces & Struct Semantics Tests

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_test_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_oop_test".into())
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
fn test_multilevel_class_inheritance() {
    let code = r#"
        class Animal {
            fn init(name: string) {
                this.name = name;
                this.health = 100;
            }

            fn speak() -> string {
                return this.name + " makes a sound";
            }
        }

        class Dog extends Animal {
            fn init(name: string, breed: string) {
                this.name = name;
                this.health = 100;
                this.breed = breed;
            }

            fn speak() -> string {
                return this.name + " barks loudly";
            }

            fn fetch() -> string {
                return this.name + " fetched the ball";
            }
        }

        class GuardDog extends Dog {
            fn init(name: string, breed: string, security_level: i32) {
                this.name = name;
                this.health = 100;
                this.breed = breed;
                this.security_level = security_level;
            }

            fn speak() -> string {
                return this.name + " growls in defense";
            }
        }

        let dog = new Dog("Buddy", "Golden Retriever");
        assert_eq(dog.speak(), "Buddy barks loudly");
        assert_eq(dog.fetch(), "Buddy fetched the ball");

        let guard = new GuardDog("Rex", "German Shepherd", 5);
        assert_eq(guard.speak(), "Rex growls in defense");
        assert_eq(guard.fetch(), "Rex fetched the ball");
        assert_eq(guard.security_level, 5);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_multiple_interface_implementation() {
    let code = r#"
        interface Printable {
            fn print_repr() -> string;
        }

        interface Serializable {
            fn serialize() -> string;
        }

        class TransactionReport implements Printable, Serializable {
            fn init(tx_id: string, amount: f64) {
                this.tx_id = tx_id;
                this.amount = amount;
            }

            fn print_repr() -> string {
                return "TX[" + this.tx_id + "]: $" + this.amount;
            }

            fn serialize() -> string {
                return "{\"id\":\"" + this.tx_id + "\",\"val\":" + this.amount + "}";
            }
        }

        fn format_as_printable(p: Printable) -> string {
            return p.print_repr();
        }

        fn format_as_serialized(s: Serializable) -> string {
            return s.serialize();
        }

        let report = new TransactionReport("TX-9901", 1540.50);
        assert_eq(format_as_printable(report), "TX[TX-9901]: $1540.5");
        assert_eq(format_as_serialized(report), "{\"id\":\"TX-9901\",\"val\":1540.5}");
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_struct_value_semantics() {
    let code = r#"
        struct Vector3D {
            x: f64,
            y: f64,
            z: f64,
        }

        fn dot_product(v1: Vector3D, v2: Vector3D) -> f64 {
            return v1.x * v2.x + v1.y * v2.y + v1.z * v2.z;
        }

        let v1: Vector3D = { x: 1.0, y: 2.0, z: 3.0 };
        let v2: Vector3D = { x: 4.0, y: -2.0, z: 1.0 };

        assert_eq(dot_product(v1, v2), 3.0);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}
