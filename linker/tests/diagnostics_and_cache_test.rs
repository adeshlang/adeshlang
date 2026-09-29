use adesh_linker::cache::LinkCache;
use adesh_linker::config::LinkConfig;
use adesh_linker::diagnostics::{DiagnosticEngine, DiagnosticLevel};
use adesh_linker::error::{ErrorCode, LinkError};
use adesh_linker::incremental::IncrementalState;
use adesh_linker::target::Target;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_diagnostic_engine_error_tracking() {
    let mut engine = DiagnosticEngine::new();
    assert!(!engine.has_errors());

    engine.emit_warning("Relocation truncated in section .text");
    assert!(!engine.has_errors());
    assert_eq!(engine.diagnostics().len(), 1);
    assert_eq!(engine.diagnostics()[0].level, DiagnosticLevel::Warning);

    let err = LinkError::new(ErrorCode::UndefinedSymbol, "Undefined symbol `my_func`");
    engine.emit_error(&err);
    assert!(engine.has_errors());
    assert_eq!(engine.diagnostics().len(), 2);
    assert_eq!(engine.diagnostics()[1].level, DiagnosticLevel::Error);
    assert_eq!(engine.diagnostics()[1].code.as_deref(), Some("LNK001"));
}

#[test]
fn test_incremental_link_cache_store() {
    let dir = tempdir().expect("Failed to create tempdir");
    let cache_dir = dir.path().to_path_buf();

    let cache = LinkCache::new(Some(cache_dir));
    let key = "libmath_obj_hash";
    let data = b"COMPILED_CODE_BLOCK";

    cache
        .put_cached_file(key, data)
        .expect("Failed to store cache");
    let fetched = cache
        .get_cached_file(key)
        .expect("Failed to get cached file");
    assert_eq!(fetched, data);
}

#[test]
fn test_incremental_state_hash() {
    let dir = tempdir().expect("Failed to create tempdir");
    let obj_path = dir.path().join("main.o");
    std::fs::write(&obj_path, b"OBJECT_BYTES_SAMPLE").unwrap();

    let target = Target::x86_64_linux();
    let config = LinkConfig::new(PathBuf::from("a.out"), target);

    let state = IncrementalState::build(&config, &[obj_path]);
    assert!(!state.config_hash.is_empty());
    assert_eq!(state.input_files.len(), 1);
}
