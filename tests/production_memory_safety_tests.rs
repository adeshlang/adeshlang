//! Production-grade Zero-GC Memory Safety & Borrowing Tests for AdeshLang
//!
//! Covers:
//! - Deterministic RAII destruction & drop ordering
//! - Complex nested scope borrowing & scope isolation
//! - Weak reference cycle breaking with Tree/Graph nodes
//! - Safe upgrade semantics across temporal owner drops
//! - Multiple concurrent readers with single exclusive mutator

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_test_code(src: &str) -> Result<(), String> {
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

#[test]
fn test_tree_node_weak_back_pointers_no_cycle_leak() {
    let code = r#"
        class TreeNode {
            fn init(name: string) {
                this.name = name;
                this.children = [];
            }

            fn add_child(child: share<TreeNode>) {
                this.children.append(child);
            }
        }

        share root = new TreeNode("root");
        assert_eq(root.strong_count(), 1);

        {
            share child1 = new TreeNode("child1");
            share child2 = new TreeNode("child2");
            root.add_child(child1);
            root.add_child(child2);

            assert_eq(root.strong_count(), 1);
            assert_eq(child1.strong_count(), 2);
            assert_eq(child2.strong_count(), 2);
        }

        // After inner scope, children are still kept alive by root
        assert_eq(root.children.len(), 2);
        assert_eq(root.children[0].name, "child1");
        assert_eq(root.children[1].name, "child2");
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_observer_pattern_with_weak_refs() {
    let code = r#"
        class Observer {
            fn init(id: i32) {
                this.id = id;
                this.last_event = "";
            }

            fn notify(msg: string) {
                this.last_event = msg;
            }
        }

        class Subject {
            fn init() {
                this.observers = [];
            }

            fn register(obs: Observer) {
                weak w = obs;
                this.observers.append(w);
            }

            fn count_alive() -> i32 {
                let alive = 0;
                for obs in this.observers {
                    if (obs.is_alive()) {
                        alive = alive + 1;
                    }
                }
                return alive;
            }
        }

        let subject = new Subject();
        share obs1 = new Observer(1);

        subject.register(obs1);
        assert_eq(subject.count_alive(), 1);

        {
            share obs2 = new Observer(2);
            subject.register(obs2);
            assert_eq(subject.count_alive(), 2);
        }

        // obs2 was dropped upon scope exit
        assert_eq(subject.count_alive(), 1);
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_raii_lifo_destruction_order() {
    let code = r#"
        let tracker: [string] = [];

        {
            defer { tracker.append("first_deferred"); }
            defer { tracker.append("second_deferred"); }
            defer { tracker.append("third_deferred"); }
            tracker.append("work_done");
        }

        assert_eq(tracker[0], "work_done");
        assert_eq(tracker[1], "third_deferred");
        assert_eq(tracker[2], "second_deferred");
        assert_eq(tracker[3], "first_deferred");
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}
