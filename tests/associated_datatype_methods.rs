use adeshlang::execution::runtime::Interpreter;
use adeshlang::execution::runtime_core::ModuleLoader;
use std::path::Path;

fn run(source: &str) -> Result<(), String> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut interpreter = Interpreter::new();
            let mut loader = ModuleLoader::new(Path::new("."));
            interpreter.run_module(&source, &mut loader, None)
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "interpreter thread panicked".to_string())?
}

#[test]
fn associated_methods_cover_native_datatypes() {
    run(
        r#"
        let text = "  hello  ";
        assert(text.trim().toUpperCase() == "HELLO");
        assert(text.trimStart().startsWith("hello"));
        assert("123".isNumeric());

        let values = [3, 1, 2, 2];
        assert(values.distinct().sum() == 6);
        assert(values.first() == 3);
        assert(values.lastIndexOf(2) == 3);

        let point = (10, 20, 30);
        assert(point.get(1) == 20);
        assert(point.toArray().length() == 3);

        let left = {1, 2, 3};
        assert(left.union({3, 4}).length() == 4);
        assert(left.difference({2}).contains(1));

        let config = {port: 8080};
        assert(config.has("port"));
        assert(config.getOr("host", "localhost") == "localhost");
        assert(config.keys().length() == 1);

        assert((-12.5).abs() == 12.5);
        assert(12.isEven());
        assert(15.clamp(0, 10) == 10);

        let z = complex(3, 4);
        assert(z.real() == 3);
        assert(z.magnitude() == 5);
        "#,
    )
    .expect("associated datatype methods should execute");
}

#[test]
fn typed_collections_and_complex_literals_are_supported() {
    run(
        r#"
        let numbers: [i32] = [1, 2, 3];
        let pair: (string, i32) = ("port", 8080);
        let tags: set = {"typed", "native"};
        let config: object = {port: 8080};
        let imaginary: complex = 5j;
        let combined: complex = 5j + 2;
        assert(numbers.length() == 3);
        assert(pair.get(1) == 8080);
        assert(tags.contains("typed"));
        assert(config.get("port") == 8080);
        assert(imaginary.real() == 0);
        assert(imaginary.imag() == 5);
        assert(combined.real() == 2);
        assert(combined.imag() == 5);
        "#,
    )
    .expect("typed datatype and complex literal syntax should execute");
}
