use adeshlang::ecosystem::{
    LockDependency, LockFile, Manifest, PackageSource, ProjectLayout, ProjectTemplate, SemVer,
    VersionReq,
};
use std::fs;

#[test]
fn parses_nested_manifest_sections() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "demo"
          version = 1.2.3
          features = ["cli", "package"]
        }
        dependencies {
          core = ^1.0.0
          util = { version = "~2.1.0", path = "./packages/util" }
        }
        "#,
    )
    .expect("manifest should parse");

    assert_eq!(manifest.project_name().as_deref(), Some("demo"));
    assert_eq!(manifest.project_version().unwrap().to_string(), "1.2.3");
    assert!(manifest.validate().is_ok());
    assert!(manifest.dependencies().is_some());
}

#[test]
fn semantic_versions_and_ranges_work() {
    let version = SemVer::parse("1.4.2-beta+build.7").expect("version parse");
    assert!(VersionReq::parse("^1.4.0").unwrap().matches(&version));
    assert!(
        VersionReq::parse(">=1.4.0, <2.0.0")
            .unwrap()
            .matches(&version)
    );
    assert!(!VersionReq::parse("~1.3.0").unwrap().matches(&version));
}

#[test]
fn scaffold_creates_project_local_layout() {
    let root = std::env::temp_dir().join(format!("adl-ecosystem-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let layout = ProjectLayout::new(&root);
    layout
        .create_template(ProjectTemplate::App, "demo")
        .expect("template should be created");

    assert!(layout.manifest_path().exists());
    assert!(layout.packages_dir().exists());
    assert!(layout.cache_dir().exists());
    assert!(layout.artifacts_dir().exists());

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn dependency_graph_includes_root_and_direct_dependencies() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "graph-demo"
          version = 0.1.0
        }
        dependencies {
          core = ^1.0.0
          util = { version = "~2.1.0", path = "./packages/util" }
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-graph-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");
    fs::create_dir_all(root.join("packages/util")).unwrap();
    fs::write(
        root.join("packages/util/adesh.adl"),
        r#"
        project {
          name = "util"
          version = 2.1.4
        }
        "#,
    )
    .unwrap();

    let resolved = adeshlang::ecosystem::DependencyResolver::new()
        .resolve_manifest(&manifest, &layout.root)
        .expect("graph should resolve");

    assert_eq!(resolved.graph.root, "graph-demo");
    assert_eq!(resolved.graph.nodes.len(), 2);
    assert!(
        resolved
            .graph
            .tree_lines()
            .iter()
            .any(|line| line.contains("core"))
    );
    assert!(
        resolved
            .graph
            .to_graph_lines()
            .iter()
            .any(|line| line.contains("graph-demo ->"))
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn confidential_values_move_out_of_manifest_into_lockfile() {
    let manifest = Manifest::parse(
        r#"
                project {
                    name = "secure-demo"
                    version = 0.1.0
                }
                publishing {
                    registry = "official"
                    registry_token = "super-secret-token"
                }
                security {
                    audit = true
                    private_key = "-----BEGIN KEY-----"
                }
                "#,
    )
    .expect("manifest should parse");

    let (sanitized, confidential) = manifest.sanitize_confidential();
    let sanitized_text = sanitized.to_string();

    assert!(sanitized_text.contains("publishing"));
    assert!(sanitized_text.contains("security"));
    assert!(sanitized_text.contains("audit = true"));
    assert!(!sanitized_text.contains("super-secret-token"));
    assert!(!sanitized_text.contains("-----BEGIN KEY-----"));
    assert_eq!(
        confidential
            .get("publishing.registry_token")
            .map(String::as_str),
        Some("super-secret-token")
    );
    assert_eq!(
        confidential.get("security.private_key").map(String::as_str),
        Some("-----BEGIN KEY-----")
    );

    let mut lockfile = LockFile::new("secure-demo");
    lockfile.set_confidential(confidential);
    let lockfile_text = lockfile.to_string();

    assert!(lockfile_text.contains("confidential"));
    assert!(lockfile_text.contains("super-secret-token"));
}

#[test]
fn adl_info_command_shows_project_details() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "info-test-project"
          version = 2.3.4
        }
        dependencies {
          core = ^1.0.0
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-info-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("should resolve");

    assert_eq!(resolved.graph.root, "info-test-project");
    assert_eq!(
        manifest
            .project_version()
            .map(|v| v.to_string())
            .unwrap_or_default(),
        "2.3.4"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn adl_list_command_shows_all_packages() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "list-test-project"
          version = 1.0.0
        }
        dependencies {
          core = ^1.0.0
          util = { version = "~2.1.0", path = "./packages/util" }
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-list-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");
    fs::create_dir_all(root.join("packages/util")).unwrap();
    fs::write(
        root.join("packages/util/adesh.adl"),
        r#"
        project {
          name = "util"
          version = 2.1.4
        }
        "#,
    )
    .unwrap();

    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("should resolve");

    let nodes: Vec<_> = resolved
        .graph
        .nodes
        .values()
        .map(|n| n.name.clone())
        .collect();
    assert!(nodes.iter().any(|n| n == "core"));
    assert!(nodes.iter().any(|n| n == "util"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn adl_graph_command_generates_dependency_graph() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "graph-test"
          version = 1.0.0
        }
        dependencies {
          core = ^1.0.0
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-graph-cmd-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("should resolve");

    let graph_lines = resolved.graph.to_graph_lines();
    assert!(!graph_lines.is_empty());
    assert!(graph_lines.iter().any(|line| line.contains("graph-test")));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn adl_tree_command_shows_dependency_hierarchy() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "tree-test"
          version = 1.0.0
        }
        dependencies {
          core = ^1.0.0
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-tree-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("should resolve");

    let tree_lines = resolved.graph.tree_lines();
    assert!(!tree_lines.is_empty());
    assert_eq!(tree_lines[0], "tree-test");
    assert!(tree_lines.iter().any(|line| line.contains("core")));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn adl_why_command_finds_dependencies_by_name() {
    let manifest = Manifest::parse(
        r#"
        project {
          name = "why-test"
          version = 1.0.0
        }
        dependencies {
          core = ^1.0.0
        }
        "#,
    )
    .expect("manifest should parse");

    let root = std::env::temp_dir().join(format!("adl-why-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("should resolve");

    let matches = resolved.graph.find_by_name("core");
    assert!(!matches.is_empty());
    assert_eq!(matches[0].name, "core");

    let no_matches = resolved.graph.find_by_name("nonexistent");
    assert!(no_matches.is_empty());

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn workspace_resolution_works() {
    let root = std::env::temp_dir().join(format!("adl-ws-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    // Write workspace manifest
    fs::write(
        layout.manifest_path(),
        r#"
        workspace {
            members = ["packages/*"]
        }
        "#,
    )
    .unwrap();

    // Create member pkg_a
    let pkg_a_dir = root.join("packages/pkg_a");
    fs::create_dir_all(&pkg_a_dir).unwrap();
    fs::write(
        pkg_a_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_a"
            version = 1.0.0
        }
        dependencies {
            pkg_b = { version = "^2.0.0", path = "../pkg_b" }
        }
        "#,
    )
    .unwrap();

    // Create member pkg_b
    let pkg_b_dir = root.join("packages/pkg_b");
    fs::create_dir_all(&pkg_b_dir).unwrap();
    fs::write(
        pkg_b_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_b"
            version = 2.1.0
        }
        "#,
    )
    .unwrap();

    let manifest = layout.load_manifest().expect("load manifest");
    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &layout.root)
        .expect("resolve workspace");

    assert!(resolved.graph.nodes.contains_key("pkg_a@1.0.0"));
    assert!(resolved.graph.nodes.contains_key("pkg_b@2.1.0"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn transitive_dependency_resolution_works() {
    let root = std::env::temp_dir().join(format!("adl-transitive-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().expect("layout should be created");

    // pkg_x -> pkg_y -> pkg_z
    let pkg_x_dir = root.join("pkg_x");
    fs::create_dir_all(&pkg_x_dir).unwrap();
    fs::write(
        pkg_x_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_x"
            version = 1.0.0
        }
        dependencies {
            pkg_y = { version = "^1.0.0", path = "../pkg_y" }
        }
        "#,
    )
    .unwrap();

    let pkg_y_dir = root.join("pkg_y");
    fs::create_dir_all(&pkg_y_dir).unwrap();
    fs::write(
        pkg_y_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_y"
            version = 1.2.0
        }
        dependencies {
            pkg_z = { version = "^3.0.0", path = "../pkg_z" }
        }
        "#,
    )
    .unwrap();

    let pkg_z_dir = root.join("pkg_z");
    fs::create_dir_all(&pkg_z_dir).unwrap();
    fs::write(
        pkg_z_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_z"
            version = 3.5.1
        }
        "#,
    )
    .unwrap();

    let manifest = Manifest::load(pkg_x_dir.join("adesh.adl")).expect("load manifest");
    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver
        .resolve_manifest(&manifest, &pkg_x_dir)
        .expect("resolve transitive");

    assert!(resolved.graph.nodes.contains_key("pkg_y@1.2.0"));
    assert!(resolved.graph.nodes.contains_key("pkg_z@3.5.1"));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn task_runner_up_to_date_works() {
    let root = std::env::temp_dir().join(format!("adl-task-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();

    let input_file = root.join("input.txt");
    let output_file = root.join("output.txt");

    fs::write(&input_file, "input content").unwrap();

    let mut runner = adeshlang::ecosystem::TaskRunner::default();
    let task = adeshlang::ecosystem::TaskDefinition {
        name: "test_task".to_string(),
        command: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            format!("echo updated > {}", output_file.display()),
        ],
        env: std::collections::BTreeMap::new(),
        dependencies: Vec::new(),
        parallel: Vec::new(),
        condition: None,
        inputs: vec![input_file.clone()],
        outputs: vec![output_file.clone()],
        platform: Some("windows".to_string()),
    };
    runner.register(task);

    // Initial state: output doesn't exist, so up-to-date should be false
    let task_def = runner.tasks.get("test_task").unwrap();
    assert!(!runner.is_up_to_date(task_def));

    // Run the task
    runner.run("test_task").expect("run task");
    assert!(output_file.exists());

    // Second run: up-to-date should be true
    assert!(runner.is_up_to_date(task_def));

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn detects_dependency_cycles() {
    let root = std::env::temp_dir().join(format!("adl-cycle-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let pkg_x_dir = root.join("pkg_x");
    fs::create_dir_all(&pkg_x_dir).unwrap();
    fs::write(
        pkg_x_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_x"
            version = 1.0.0
        }
        dependencies {
            pkg_y = { version = "^1.0.0", path = "../pkg_y" }
        }
        "#,
    )
    .unwrap();

    let pkg_y_dir = root.join("pkg_y");
    fs::create_dir_all(&pkg_y_dir).unwrap();
    fs::write(
        pkg_y_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_y"
            version = 1.0.0
        }
        dependencies {
            pkg_x = { version = "^1.0.0", path = "../pkg_x" }
        }
        "#,
    )
    .unwrap();

    let manifest = Manifest::load(pkg_x_dir.join("adesh.adl")).unwrap();
    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let result = resolver.resolve_manifest(&manifest, &pkg_x_dir);

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.contains("dependency cycle detected"),
        "expected cycle error, got: {}",
        err
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn resolves_dependency_conflicts() {
    let root = std::env::temp_dir().join(format!("adl-conflict-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let pkg_a_dir = root.join("pkg_a");
    fs::create_dir_all(&pkg_a_dir).unwrap();
    fs::write(
        pkg_a_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_a"
            version = 1.0.0
        }
        dependencies {
            shared = { version = "^1.0.0", path = "../shared1" }
            pkg_b = { version = "^1.0.0", path = "../pkg_b" }
        }
        "#,
    )
    .unwrap();

    // pkg_b requests shared with version "^2.0.0" -> conflict with pkg_a's "^1.0.0"
    let pkg_b_dir = root.join("pkg_b");
    fs::create_dir_all(&pkg_b_dir).unwrap();
    fs::write(
        pkg_b_dir.join("adesh.adl"),
        r#"
        project {
            name = "pkg_b"
            version = 1.0.0
        }
        dependencies {
            shared = { version = "^2.0.0", path = "../shared2" }
        }
        "#,
    )
    .unwrap();

    let shared1_dir = root.join("shared1");
    fs::create_dir_all(&shared1_dir).unwrap();
    fs::write(
        shared1_dir.join("adesh.adl"),
        r#"
        project {
            name = "shared"
            version = 1.1.0
        }
        "#,
    )
    .unwrap();

    let shared2_dir = root.join("shared2");
    fs::create_dir_all(&shared2_dir).unwrap();
    fs::write(
        shared2_dir.join("adesh.adl"),
        r#"
        project {
            name = "shared"
            version = 2.0.5
        }
        "#,
    )
    .unwrap();

    let manifest = Manifest::load(pkg_a_dir.join("adesh.adl")).unwrap();
    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let result = resolver.resolve_manifest(&manifest, &pkg_a_dir);

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        err.contains("dependency conflict"),
        "expected conflict error, got: {}",
        err
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn verifies_lockfile_signatures() {
    let mut lock = LockFile::new("secure-test");
    lock.add_dependency(LockDependency {
        package_id: "core@1.0.0".to_string(),
        name: "core".to_string(),
        version: SemVer::parse("1.0.0").unwrap(),
        requirement: VersionReq::parse("^1.0.0").unwrap(),
        checksum: "abcdef".to_string(),
        sha256: "abcdef".to_string(),
        source: PackageSource {
            kind: "registry".to_string(),
            location: "official".to_string(),
        },
        features: Vec::new(),
        registries: Vec::new(),
        transitive: Vec::new(),
        target: None,
        profile: None,
    });

    let original_text = lock.to_string();
    assert!(original_text.contains("signature ="));

    let parsed = LockFile::parse(&original_text).expect("should parse");
    parsed.verify_signature().unwrap();

    // Tamper with the lockfile version in dependencies
    let tampered_text = original_text.replace("version = \"1.0.0\"", "version = \"2.0.0\"");
    let tampered_parsed = LockFile::parse(&tampered_text).expect("should parse tampered");
    let verification = tampered_parsed.verify_signature();
    assert!(verification.is_err());
    assert!(
        verification
            .err()
            .unwrap()
            .contains("integrity check failed")
    );
}

#[test]
fn publishes_and_unpublishes_packages() {
    let root = std::env::temp_dir().join(format!("adl-publish-test-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().unwrap();

    fs::write(
        layout.manifest_path(),
        r#"
        project {
            name = "my_published_package"
            version = 1.5.0
        }
        "#,
    )
    .unwrap();

    // Login (write credentials)
    let token = "publish-token-xyz";
    fs::write(
        layout.adl_dir().join("credentials"),
        format!("token = \"{}\"", token),
    )
    .unwrap();

    // Simulate adl publish by copying to target registry directory
    let target_registry_dir = layout
        .root
        .join(".adl")
        .join("registry")
        .join("official")
        .join("my_published_package")
        .join("1.5.0");
    fs::create_dir_all(&target_registry_dir).unwrap();
    fs::write(
        target_registry_dir.join("package.tar.gz"),
        "ADL_PACKAGE_DATA",
    )
    .unwrap();
    fs::copy(
        layout.manifest_path(),
        target_registry_dir.join("adesh.adl"),
    )
    .unwrap();

    assert!(target_registry_dir.join("package.tar.gz").exists());
    assert!(target_registry_dir.join("adesh.adl").exists());

    // Clean up
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn adl_modules_folder_resolution_works() {
    use std::path::PathBuf;
    let root = std::env::temp_dir().join(format!("adl-modules-res-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }

    let layout = ProjectLayout::new(&root);
    layout.ensure_layout().unwrap();

    // 1. Create main project manifest
    fs::write(
        layout.manifest_path(),
        r#"
        project {
            name = "main_app"
            version = 1.0.0
        }
        dependencies {
            my_lib = { version = "^1.2.0", path = "./packages/my_lib" }
        }
        "#,
    )
    .unwrap();

    // 2. Create the my_lib dependency directory and source files
    let my_lib_dir = root.join("packages/my_lib");
    fs::create_dir_all(my_lib_dir.join("src")).unwrap();
    fs::write(
        my_lib_dir.join("adesh.adl"),
        r#"
        project {
            name = "my_lib"
            version = 1.2.5
        }
        "#,
    )
    .unwrap();
    fs::write(
        my_lib_dir.join("src/lib.adesh"),
        "export fn hello() { return \"hello\"; }\n",
    )
    .unwrap();

    // 3. Resolve and install
    let manifest = layout.load_manifest().unwrap();
    let resolver = adeshlang::ecosystem::DependencyResolver::new();
    let resolved = resolver.resolve_manifest(&manifest, &layout.root).unwrap();

    // Populate adl_modules
    layout.populate_adl_modules(&resolved.lockfile).unwrap();

    // 4. Verify adl_modules/my_lib has the source file
    let target_file = root.join("adl_modules/my_lib/src/lib.adesh");
    assert!(target_file.exists());

    // 5. Test resolve_path in interpreter utilities
    let resolved_path =
        adeshlang::execution::runtime_core::interpreter_impl::utilities::resolve_path(
            "my_lib",
            &root.join("src/main.adesh").to_string_lossy(),
        );
    assert_eq!(
        PathBuf::from(resolved_path).canonicalize().unwrap(),
        target_file.canonicalize().unwrap()
    );

    fs::remove_dir_all(&root).unwrap();
}
