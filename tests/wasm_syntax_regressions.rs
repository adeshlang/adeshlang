//! Execute generated WASM through the same Node loader as `run --wasm`.
use adeshlang::backends::wasm::{compile_to_file, write_js_loader};
use std::process::Command;

fn run(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let wasm = dir.path().join("main.wasm");
    let loader = dir.path().join("main.cjs");
    compile_to_file(source, &wasm).expect("compile source");
    write_js_loader(&wasm, &loader).unwrap();
    let result = Command::new("node")
        .arg(loader)
        .output()
        .expect("Node.js is required");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn rest_spread_empty_and_repeated_calls() {
    assert_eq!(
        run(r#"
        fn sum(...items) {
            let total = 0;
            let i = 0;
            while (i < len(items)) {
                total = total + items[i];
                i = i + 1;
            }
            return total;
        }
        fn append(first, ...rest) { return [first, ...rest, 99]; }
        let values = [4, 7, 9];
        print(sum(), sum(2, 3), sum(...values), sum(1, ...values, 5));
        print(append(8), append(...values));
        fn describe(first, second = 20, ...rest) {
            print(first, second, rest);
        }
        describe(...[6, 7, 8, 9]);
        describe(3);
    "#),
        "0 5 20 26\n[8, 99] [4, 7, 9, 99]\n6 7 [8, 9]\n3 20 []\n"
    );
}

#[test]
fn extended_method_uses_receiver_not_example_names() {
    assert_eq!(
        run(r#"
        class Visitor { Visitor(name) { this.name = name; } }
        extend on Visitor {
            fn greet() { print("Welcome " + this.name); }
        }
        let one = new Visitor("Mira");
        let two = new Visitor("Dev");
        one.greet();
        two.greet();
        one.name = "Sam";
        one.greet();
    "#),
        "Welcome Mira\nWelcome Dev\nWelcome Sam\n"
    );
}

#[test]
fn array_prints_observe_each_index_assignment() {
    assert_eq!(
        run(r#"
        let values = [10, 20, 30];
        print(values);
        values[1] = values[1] + 3;
        print(values);
        values[1] = values[1] * 2;
        print(values);
        values[0] = values[2] - 5;
        print(values);
    "#),
        "[10, 20, 30]\n[10, 23, 30]\n[10, 46, 30]\n[25, 46, 30]\n"
    );
}

#[test]
fn date_uses_timestamp_and_runtime_clock() {
    let output = run(r#"
        let d = new Date(1700000000123);
        print(d.toISOString(), d.getTime());
        print(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(),
            d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), d.getUTCMilliseconds());
        print(Date.now());
        let current = new Date();
        print(current.getTime());
    "#);
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines[0], "2023-11-14T22:13:20.123Z 1700000000123");
    assert_eq!(lines[1], "2023 10 14 22 13 20 123");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    for line in &lines[2..] {
        assert!((line.parse::<f64>().unwrap() - now).abs() < 10_000.0);
    }
    assert_eq!(lines.len(), 4);
}

#[test]
fn print_style_and_empty_print_have_no_return_value_output() {
    assert_eq!(
        run("print(\"done\", {bold: true, flush: true}); print();"),
        "\u{1b}[1mdone\u{1b}[0m\n\n"
    );
}
