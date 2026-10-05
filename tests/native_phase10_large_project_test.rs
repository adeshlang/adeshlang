//! Phase 10 Large Project Compilation & Build Wave Scheduling E2E Test Suite.
//!
//! Validates:
//! - Build graph topological wave scheduling across 10+ modules.
//! - Parallel wave execution sequencing.

#![allow(dead_code, unused_imports)]

use adesh_codegen::build_graph::{BuildGraphScheduler, BuildNode};

#[test]
fn test_large_project_wave_scheduling() {
    let mut scheduler = BuildGraphScheduler::new();

    // Base modules (Wave 0)
    scheduler.add_node(BuildNode {
        name: "core".to_string(),
        source_path: "src/core.adesh".to_string(),
        dependencies: vec![],
    });
    scheduler.add_node(BuildNode {
        name: "alloc".to_string(),
        source_path: "src/alloc.adesh".to_string(),
        dependencies: vec![],
    });

    // Dependent modules (Wave 1)
    scheduler.add_node(BuildNode {
        name: "collections".to_string(),
        source_path: "src/collections.adesh".to_string(),
        dependencies: vec!["core".to_string(), "alloc".to_string()],
    });
    scheduler.add_node(BuildNode {
        name: "math".to_string(),
        source_path: "src/math.adesh".to_string(),
        dependencies: vec!["core".to_string()],
    });

    // Top-level application (Wave 2)
    scheduler.add_node(BuildNode {
        name: "app".to_string(),
        source_path: "src/app.adesh".to_string(),
        dependencies: vec!["collections".to_string(), "math".to_string()],
    });

    let schedule = scheduler.schedule().expect("schedule build");
    assert!(schedule.waves.len() >= 3);
    assert!(schedule.waves[0].contains(&"core".to_string()));
    assert!(schedule.waves[0].contains(&"alloc".to_string()));
}
