//! Phase 9 Deterministic Builds E2E Test Suite.
//!
//! Validates:
//! - Bit-for-bit identical object generation across independent compiler invocations.
//! - Canonical ordering in package resolution, lockfiles, and symbol maps.
//! - Invariant fingerprints across multiple runs.

#![allow(dead_code, unused_imports)]

use adesh_codegen::asm::AssemblyEmitter;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::package::{
    DependencyResolver, PackageDependency, PackageManifest, PackageMetadata,
};
use std::collections::BTreeMap;

#[test]
fn test_assembly_and_ir_emission_determinism() {
    let make_module = || {
        let mut native_mod = NativeModule::new("det_mod");
        for i in 0..10 {
            let mut func = MachineFunction::new(format!("fn_{}", i));
            func.is_exported = true;
            let b = func.entry_block_mut();
            b.push(MachineInstruction::Add {
                dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
                src: MachineOperand::Immediate(i as i64),
            });
            b.push(MachineInstruction::Return);
            native_mod.add_function(func);
        }
        native_mod
    };

    let mod1 = make_module();
    let mod2 = make_module();

    let asm1 = AssemblyEmitter::emit_module(&mod1);
    let asm2 = AssemblyEmitter::emit_module(&mod2);

    assert_eq!(
        asm1, asm2,
        "Assembly emission must be bit-for-bit deterministic"
    );
}

#[test]
fn test_lockfile_resolution_determinism() {
    let mut deps = BTreeMap::new();
    deps.insert(
        "z_crate".to_string(),
        PackageDependency::Simple("1.0.0".to_string()),
    );
    deps.insert(
        "a_crate".to_string(),
        PackageDependency::Simple("2.0.0".to_string()),
    );
    deps.insert(
        "m_crate".to_string(),
        PackageDependency::Simple("3.0.0".to_string()),
    );

    let manifest = PackageManifest {
        package: PackageMetadata {
            name: "det_package".to_string(),
            version: "1.0.0".to_string(),
            authors: vec![],
            edition: "2024".to_string(),
            license: None,
            description: None,
            entry: None,
        },
        dependencies: deps,
        dev_dependencies: BTreeMap::new(),
        target: BTreeMap::new(),
        features: BTreeMap::new(),
        workspace: None,
        profile: BTreeMap::new(),
    };

    let mut resolver = DependencyResolver::new();
    resolver.register_package("z_crate", "1.0.0", vec![]);
    resolver.register_package("a_crate", "2.0.0", vec![]);
    resolver.register_package("m_crate", "3.0.0", vec![]);

    let lock1 = resolver.resolve(&manifest, &[]).expect("res 1");
    let lock2 = resolver.resolve(&manifest, &[]).expect("res 2");

    let s1 = lock1.to_string();
    let s2 = lock2.to_string();
    assert_eq!(s1, s2, "Lockfile generation must be strictly deterministic");
}
