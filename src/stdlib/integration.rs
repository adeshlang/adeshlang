//! Ecosystem Integration Module
//!
//! This module ensures all Adesh ecosystem components are properly wired together:
//! - Layered stdlib (adesh_core, adesh_alloc, adesh_std)
//! - Compile-time memory safety
//! - ABI versioning and FFI safety
//! - Runtime integration

use crate::parsing::compile_time_memory_safety::check_memory_safety_compile_time;
use crate::parsing::hir::HirModule;

/// Validates that the ecosystem is properly integrated
pub fn validate_ecosystem_integration() -> Result<(), String> {
    // Check that all stdlib layers are accessible
    let _core = crate::stdlib::adesh_core::Layout::new::<u32>();
    let _alloc = crate::stdlib::adesh_alloc::Vec::<i32>::new();

    // Check that ABI versioning is accessible
    let _abi_version = crate::runtime::abi::versioning::AbiVersion::CURRENT;

    // Check that FFI safety is accessible
    let _checker = crate::backends::common::ffi::safety::FfiSafetyChecker::new();

    Ok(())
}

/// Run complete compile-time validation before execution
pub fn validate_before_execution(hir: &HirModule) -> Result<(), String> {
    // Run memory safety checks
    check_memory_safety_compile_time(hir)?;

    // Additional validations can be added here

    Ok(())
}

/// Integration metadata
pub struct EcosystemMetadata {
    pub abi_version: String,
    pub memory_model: String,
    pub safety_guarantees: Vec<String>,
}

impl EcosystemMetadata {
    pub fn current() -> Self {
        EcosystemMetadata {
            abi_version: crate::runtime::abi::versioning::AbiVersion::CURRENT.to_string(),
            memory_model: "Zero-GC, ARC-based".to_string(),
            safety_guarantees: vec![
                "Use-after-free prevention".to_string(),
                "Double-free prevention".to_string(),
                "Data race prevention".to_string(),
                "No garbage collection pauses".to_string(),
                "Deterministic memory management".to_string(),
            ],
        }
    }

    pub fn display(&self) -> String {
        format!(
            "Adesh Ecosystem v{}\nMemory Model: {}\nSafety Guarantees:\n{}",
            self.abi_version,
            self.memory_model,
            self.safety_guarantees
                .iter()
                .map(|g| format!("  ✓ {}", g))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecosystem_validation() {
        // Should not panic - validates all components are accessible
        validate_ecosystem_integration().expect("Ecosystem should be properly integrated");
    }

    #[test]
    fn test_ecosystem_metadata() {
        let metadata = EcosystemMetadata::current();
        assert_eq!(metadata.abi_version, "1.0.0");
        assert_eq!(metadata.memory_model, "Zero-GC, ARC-based");
        assert_eq!(metadata.safety_guarantees.len(), 5);
    }

    #[test]
    fn test_metadata_display() {
        let metadata = EcosystemMetadata::current();
        let display = metadata.display();
        assert!(display.contains("Adesh Ecosystem"));
        assert!(display.contains("Zero-GC"));
        assert!(display.contains("✓"));
    }
}
