//! Phase 10 Compiler Server Daemon Persistence E2E Test Suite.
//!
//! Validates:
//! - Persistent compiler cache survival across successive valid and failed compilations.

#![allow(dead_code, unused_imports)]

use adesh_codegen::compiler_server::CompilerServer;

#[test]
fn test_compiler_server_persistence_and_recovery() {
    let mut server = CompilerServer::new();

    // 1. Initial valid compile
    assert!(server.compile_module("mod_a", "fn a() {}").is_ok());
    assert_eq!(server.cached_modules_count(), 1);

    // 2. Syntax error compile — must fail without wiping existing cache
    assert!(server.compile_module("mod_bad", "syntax_error_intentional").is_err());
    assert_eq!(server.cached_modules_count(), 1);

    // 3. Second valid compile
    assert!(server.compile_module("mod_b", "fn b() {}").is_ok());
    assert_eq!(server.cached_modules_count(), 2);
    assert_eq!(server.total_compilations(), 3);
}
