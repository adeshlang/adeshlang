//! Phase 11 item 6: target tiers are derived from verification status, and the
//! list the CLI prints is that derivation — not a second, hand-maintained claim.
//!
//! Before this, `linker/src/target.rs` labelled AArch64 and i686 Tier 1
//! ("Fully Supported, Production-Verified") with no execution test for either,
//! and the linker help repeated the labels as prose.

use adesh_linker::target::{ADVERTISED_TARGETS, Arch, Target, TargetTier, advertised_by_tier};

fn tier_of(triple: &str) -> TargetTier {
    Target::from_triple(triple)
        .unwrap_or_else(|e| panic!("{triple} must parse: {e}"))
        .tier()
}

/// Tier 1 is reserved for what CI actually runs. Each of these has a checked-in
/// suite that builds the artifact and asserts its output or exit status.
#[test]
fn execution_verified_targets_are_tier_1() {
    for triple in [
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc",
        "wasm32-unknown-wasi",
        "wasm32-unknown-unknown",
    ] {
        assert_eq!(
            tier_of(triple),
            TargetTier::Tier1Supported,
            "{triple} is built and executed by a CI job"
        );
    }
}

/// Nothing that has only been emitted and parsed back may claim Tier 1.
#[test]
fn emission_tested_targets_are_tier_2() {
    for triple in [
        // Codegen and object structure are tested; no qemu or hardware job runs them.
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "aarch64-pc-windows-msvc",
        // No x86-64 macOS runner exists.
        "x86_64-apple-darwin",
        // The PE suites that link-and-run all ask for `-msvc`; nothing requests a
        // MinGW triple, so the same writer is unverified for this flavour.
        "x86_64-pc-windows-gnu",
        // RV64 encodings and ELF structure tested; source builds stop at ParallelMove.
        "riscv64gc-unknown-linux-gnu",
        // musl TLS ran on a developer WSL host; there is no dedicated musl CI job.
        "x86_64-unknown-linux-musl",
    ] {
        assert_eq!(
            tier_of(triple),
            TargetTier::Tier2Experimental,
            "{triple} is emission-tested, never executed in CI"
        );
    }
}

/// Targets with no compiler backend and no CI-run link test are declared only.
/// These were Tier 1 or Tier 2 before the realignment: AArch64 was Tier 1, i686
/// was Tier 1, and RISC-V 32, PowerPC, CUDA, ROCm and QIR were Tier 2.
#[test]
fn unimplemented_targets_are_tier_3() {
    for triple in [
        "i686-unknown-linux-gnu",
        "i686-pc-windows-gnu",
        "riscv32imac-unknown-none-elf",
        "armv7-unknown-linux-gnueabihf",
        "thumbv7em-none-eabihf",
        "powerpc64le-unknown-linux-gnu",
        "s390x-unknown-linux-gnu",
        "nvptx64-nvidia-cuda",
        "amdgcn-amd-amdhsa",
        "quantum-circuit-qasm3",
    ] {
        assert_eq!(
            tier_of(triple),
            TargetTier::Tier3Declared,
            "{triple} has no backend and no CI-run link or execution test"
        );
    }

    // The refusal has to be real, not just a label: none of these select a
    // codegen backend.
    for triple in [
        "i686-unknown-linux-gnu",
        "riscv32imac-unknown-none-elf",
        "armv7-unknown-linux-gnueabihf",
        "powerpc64le-unknown-linux-gnu",
    ] {
        assert!(
            Target::from_triple(triple).unwrap().arch != Arch::X86_64,
            "{triple} must not be folded onto the supported x86-64 architecture"
        );
    }
}

/// The printed list is the table grouped by tier — so it can never disagree with
/// `tier()`, and every entry is a triple the linker accepts.
#[test]
fn advertised_list_is_grouped_by_verification_tier() {
    let groups = advertised_by_tier();
    let tiers: Vec<TargetTier> = groups.iter().map(|(tier, _)| *tier).collect();
    assert_eq!(
        tiers,
        vec![
            TargetTier::Tier1Supported,
            TargetTier::Tier2Experimental,
            TargetTier::Tier3Declared,
        ],
        "the list must read as a verification ladder, most-verified first"
    );

    let advertised: usize = groups.iter().map(|(_, items)| items.len()).sum();
    assert_eq!(
        advertised,
        ADVERTISED_TARGETS.len(),
        "every advertised target lands in exactly one tier group"
    );

    for (tier, items) in &groups {
        for entry in items {
            assert_eq!(
                &tier_of(entry.triple),
                tier,
                "{} is printed under the wrong heading",
                entry.triple
            );
            assert!(
                !entry.summary.contains("Tier"),
                "{}: the heading already states the tier; the summary must not restate a status claim",
                entry.triple
            );
        }
    }
}

/// A target the toolchain refuses to build for is not advertised as supported.
#[test]
fn refused_targets_are_not_advertised_as_supported() {
    for entry in ADVERTISED_TARGETS {
        if matches!(tier_of(entry.triple), TargetTier::Tier3Declared) {
            assert!(
                entry.summary.contains("no compiler backend")
                    || entry.summary.contains("refused")
                    || entry.summary.contains("proof-of-concept")
                    || entry.summary.contains("packaging only")
                    || entry.summary.contains("format declared"),
                "{}: a Tier 3 entry must say plainly what is missing: {}",
                entry.triple,
                entry.summary
            );
        }
    }
}
