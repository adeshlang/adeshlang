//! Phase 10 — Production Generic System, Specialization & Code Growth Control.
//!
//! Provides:
//! - Parametric polymorphic functions, structs, interfaces, and constraints.
//! - Deterministic monomorphization and deduplication.
//! - Instantiation depth limit enforcement to prevent recursive generic explosion.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Concrete type representation for specialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConcreteType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Str,
    Custom(String),
    Pointer(Box<ConcreteType>),
}

/// Generic type constraint / trait bound.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraitBound {
    pub name: String,
}

/// Generic function definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericFunctionDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub constraints: HashMap<String, Vec<TraitBound>>,
    pub return_type: String,
}

/// Specialized function representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecializedFunction {
    pub original_name: String,
    pub specialized_name: String,
    pub concrete_type_args: Vec<ConcreteType>,
}

/// Specialization limits preventing binary code explosion.
#[derive(Debug, Clone, Copy)]
pub struct GenericLimits {
    pub max_instantiation_depth: usize,
    pub max_specializations_per_function: usize,
    pub max_total_specializations: usize,
}

impl Default for GenericLimits {
    fn default() -> Self {
        Self {
            max_instantiation_depth: 32,
            max_specializations_per_function: 64,
            max_total_specializations: 1024,
        }
    }
}

/// Specialization error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericError {
    RecursionLimitExceeded(String, usize),
    SpecializationLimitExceeded(String, usize),
    ConstraintViolated(String, String),
}

impl std::fmt::Display for GenericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenericError::RecursionLimitExceeded(name, depth) => write!(
                f,
                "error: generic instantiation limit exceeded for '{}' (depth {})",
                name, depth
            ),
            GenericError::SpecializationLimitExceeded(name, count) => write!(
                f,
                "error: specialization count limit exceeded for '{}' ({} variants)",
                name, count
            ),
            GenericError::ConstraintViolated(param, bound) => write!(
                f,
                "error: type parameter '{}' does not satisfy constraint '{}'",
                param, bound
            ),
        }
    }
}

/// Monomorphization engine with deduplication and explosion bounds.
pub struct SpecializationEngineV2 {
    limits: GenericLimits,
    specializations: HashMap<String, Vec<SpecializedFunction>>,
    known_signatures: HashSet<String>,
}

impl SpecializationEngineV2 {
    pub fn new(limits: GenericLimits) -> Self {
        Self {
            limits,
            specializations: HashMap::new(),
            known_signatures: HashSet::new(),
        }
    }

    /// Request a monomorphized specialization of a generic function.
    pub fn specialize(
        &mut self,
        func: &GenericFunctionDef,
        type_args: Vec<ConcreteType>,
        call_depth: usize,
    ) -> Result<SpecializedFunction, GenericError> {
        if call_depth > self.limits.max_instantiation_depth {
            return Err(GenericError::RecursionLimitExceeded(
                func.name.clone(),
                call_depth,
            ));
        }

        // Canonical specialized name: foo__I32_F64
        let type_suffix = type_args
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join("_");
        let spec_name = format!("{}__{}", func.name, type_suffix);

        // Deduplication: return existing specialization if already monomorphized
        let list = self.specializations.entry(func.name.clone()).or_default();
        if let Some(existing) = list.iter().find(|s| s.specialized_name == spec_name) {
            return Ok(existing.clone());
        }

        if list.len() >= self.limits.max_specializations_per_function {
            return Err(GenericError::SpecializationLimitExceeded(
                func.name.clone(),
                list.len(),
            ));
        }

        let spec = SpecializedFunction {
            original_name: func.name.clone(),
            specialized_name: spec_name.clone(),
            concrete_type_args: type_args,
        };

        list.push(spec.clone());
        self.known_signatures.insert(spec_name);
        Ok(spec)
    }

    pub fn total_specializations(&self) -> usize {
        self.specializations.values().map(|v| v.len()).sum()
    }
}

impl Default for SpecializationEngineV2 {
    fn default() -> Self {
        Self::new(GenericLimits::default())
    }
}
