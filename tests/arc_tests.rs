use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src_owned, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Test A — single share owner reports strong_count == 1 (no dispatch clone).
#[test]
fn test_arc_introspection_basic_strong_count() {
    let code = r#"
        share x = { id: 1 };
        assert_eq(x.strong_count(), 1);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Test B — weak ref does not increase strong count.
#[test]
fn test_arc_introspection_weak_does_not_increase_strong() {
    let code = r#"
        share x = { id: 1 };
        weak w = x;
        assert_eq(x.strong_count(), 1);
        assert_eq(x.weak_count(), 1);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Test C — explicit strong references increase count.
#[test]
fn test_arc_introspection_explicit_strong_refs() {
    let code = r#"
        share x = { id: 1 };
        strong a = x;
        assert_eq(x.strong_count(), 2);
        strong b = x;
        assert_eq(x.strong_count(), 3);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Test D — weak reference introspection.
#[test]
fn test_arc_introspection_via_weak_ref() {
    let code = r#"
        share x = { id: 1 };
        weak w = x;
        assert_eq(w.strong_count(), 1);
        assert_eq(w.weak_count(), 1);
        assert(w.is_alive());
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Test E — after all strong owners released, weak sees dead object.
#[test]
fn test_arc_introspection_after_strong_release() {
    let code = r#"
        fn make_orphan_weak() {
            share x = { id: 1 };
            weak w = x;
            strong a = x;
            return w;
        }
        weak w = make_orphan_weak();
        assert(w.is_alive() == false);
        assert(w.upgrade() == null);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Nested expression receiver: function return value must not gain a dispatch clone.
#[test]
fn test_arc_introspection_nested_function_return() {
    let code = r#"
        fn get_share() {
            share x = { id: 1 };
            return x;
        }
        assert_eq(get_share().strong_count(), 1);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Function parameter binding is a language-level owner when passed as share.
#[test]
fn test_arc_introspection_function_parameter_owner() {
    let code = r#"
        fn inspect(obj) {
            return obj.strong_count();
        }
        share outer = { id: 1 };
        assert_eq(inspect(outer), 2);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

/// Upgrade temporary must be scoped — module-level `let live = w.upgrade()` keeps an extra owner.
#[test]
fn test_arc_upgrade_temporary_released_after_scope() {
    let code = r#"
        share x = { id: 1 };
        weak w = x;
        strong a = x;
        strong b = x;
        strong c = x;
        {
            let maybe = w.upgrade();
            if (maybe != null) {
                assert_eq(x.strong_count(), 5);
            }
        }
        assert_eq(x.strong_count(), 4);
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_deep_nested_scope_counts_no_leak() {
    let code = r#"
        share deep = { level: 0 };
        assert_eq(deep.strong_count(), 1);
        {
            strong l1 = deep;
            {
                strong l2 = deep;
                {
                    strong l3 = deep;
                    {
                        strong l4 = deep;
                        assert_eq(deep.strong_count(), 5);
                    }
                    assert_eq(deep.strong_count(), 4);
                }
                assert_eq(deep.strong_count(), 3);
            }
            assert_eq(deep.strong_count(), 2);
        }
        assert_eq(deep.strong_count(), 1);
        print("PASS: deep nested scope counts");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_nested_scope_counts_no_leak() {
    let code = r#"
        share deep = { level: 0 };
        assert_eq(deep.strong_count(), 1);
        {
            strong l1 = deep;
            {
                strong l2 = deep;
                assert_eq(deep.strong_count(), 3);
            }
            assert_eq(deep.strong_count(), 2);
        }
        assert_eq(deep.strong_count(), 1);
        print("PASS: nested scope counts");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_upgrade_fails_after_owner_released() {
    let code = r#"
        fn orphan_weak() {
            share ephemeral = { id: 7 };
            weak watcher = ephemeral;
            return watcher;
        }
        weak orphan = orphan_weak();
        if (orphan.is_alive() != false) {
            print("FAIL: is_alive should be false");
        }
        let result = orphan.upgrade();
        if (result != null) {
            print("FAIL: upgrade should return null");
        } else {
            print("PASS: upgrade returned null after owner released");
        }
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_weak_to_weak_assignment_clones() {
    let code = r#"
        share holder = { id: 1 };
        weak a = holder;
        assert_eq(a.weak_count(), 1);
        weak b = a;
        assert_eq(a.weak_count(), 2);
        assert_eq(b.weak_count(), 2);
        print("PASS: weak-to-weak assignment clones");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_weak_scope_independence() {
    let code = r#"
        share holder = { id: 1 };
        weak outer = holder;
        assert_eq(outer.weak_count(), 1);
        {
            weak inner = holder;
            weak inner2 = inner;
            assert_eq(inner.weak_count(), 3);
        }
        assert_eq(outer.weak_count(), 1);
        assert(outer.is_alive());
        print("PASS: weak scope independence");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_cycle_breaking_weak_back_ref() {
    let code = r#"
        share parent = { name: "parent" };
        share child = { name: "child" };
        strong parent_holds_child = child;
        weak child_sees_parent = parent;
        let p = child_sees_parent.upgrade();
        if (p == null || p.name != "parent") {
            print("FAIL: weak back-ref upgrade");
        }
        print("PASS: cycle-breaking weak back-ref");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_class_instance_identity() {
    let code = std::fs::read_to_string("examples/arc/09_arc_with_classes.adesh")
        .expect("Failed to read examples/arc/09_arc_with_classes.adesh");
    let res = run_code(&code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_strong_count_semantics() {
    let code = r#"
        share counter = { v: 0 };
        assert_eq(counter.strong_count(), 1);
        strong a = counter;
        strong b = counter;
        assert_eq(counter.strong_count(), 3);
        {
            strong tmp = counter;
            assert_eq(counter.strong_count(), 4);
        }
        assert_eq(counter.strong_count(), 3);
        print("PASS: strong_count semantics");
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_strong_cloning_and_reassignment() {
    let code = r#"
        share user = { name: "Ajay" };
        strong a = user;
        print(user.strong_count());
        {
            strong b = user;
            print(user.strong_count());
        }
        print(user.strong_count());
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_arc_examples_interpreter() {
    let examples = [
        "examples/arc/01_basic_share.adesh",
        "examples/arc/02_strong_references.adesh",
        "examples/arc/03_weak_references.adesh",
        "examples/arc/04_scope_cleanup.adesh",
        "examples/arc/05_upgrade_and_failure.adesh",
        "examples/arc/06_reference_counts.adesh",
        "examples/arc/07_arc_in_functions.adesh",
        "examples/arc/08_breaking_cycles.adesh",
        "examples/arc/09_arc_with_classes.adesh",
        "examples/arc/10_edge_cases.adesh",
    ];

    for path_str in examples {
        let code = std::fs::read_to_string(path_str)
            .unwrap_or_else(|_| panic!("Failed to read {}", path_str));
        let res = run_code(&code);
        assert!(res.is_ok(), "Failed on {}: {:?}", path_str, res.err());
    }
}
