//! Phase 9 Generic & Monomorphization Infrastructure.
//!
//! Provides:
//! - Generic function and generic type representation
//! - Concrete type specialization
//! - Monomorphization engine with deterministic symbol mangling
//! - Deduplication across modules so identical instantiations share machine code
//! - Integration hooks with inlining and dead-code elimination

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Concrete Type for Generic Instantiation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
    Pointer(Box<ConcreteType>),
    Array(Box<ConcreteType>, usize),
    Struct(String, Vec<ConcreteType>),
}

impl ConcreteType {
    /// Deterministic mangled name for symbol resolution.
    pub fn mangle(&self) -> String {
        match self {
            ConcreteType::I8 => "i8".to_string(),
            ConcreteType::I16 => "i16".to_string(),
            ConcreteType::I32 => "i32".to_string(),
            ConcreteType::I64 => "i64".to_string(),
            ConcreteType::U8 => "u8".to_string(),
            ConcreteType::U16 => "u16".to_string(),
            ConcreteType::U32 => "u32".to_string(),
            ConcreteType::U64 => "u64".to_string(),
            ConcreteType::F32 => "f32".to_string(),
            ConcreteType::F64 => "f64".to_string(),
            ConcreteType::Bool => "b".to_string(),
            ConcreteType::Str => "str".to_string(),
            ConcreteType::Pointer(inner) => format!("P{}", inner.mangle()),
            ConcreteType::Array(inner, len) => format!("A{}_{}", len, inner.mangle()),
            ConcreteType::Struct(name, type_args) => {
                let args = type_args
                    .iter()
                    .map(|t| t.mangle())
                    .collect::<Vec<_>>()
                    .join("_");
                format!("{}_T{}", name, args)
            }
        }
    }
}

/// Generic function signature template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericFunctionTemplate {
    pub name: String,
    pub type_params: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub is_inline: bool,
}

/// Specialized (monomorphized) function instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonomorphizedFunction {
    pub original_name: String,
    pub specialized_symbol: String,
    pub type_arguments: Vec<ConcreteType>,
    pub return_type: ConcreteType,
    pub is_inline: bool,
    pub call_count: usize,
}

/// Monomorphization and Specialization Engine.
pub struct MonomorphizationEngine {
    templates: HashMap<String, GenericFunctionTemplate>,
    specializations: BTreeMap<String, MonomorphizedFunction>,
    symbol_table: HashMap<String, String>, // mangled_symbol -> canonical_symbol (for deduplication)
}

impl MonomorphizationEngine {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            specializations: BTreeMap::new(),
            symbol_table: HashMap::new(),
        }
    }

    /// Register a generic function template.
    pub fn register_template(&mut self, template: GenericFunctionTemplate) {
        self.templates.insert(template.name.clone(), template);
    }

    /// Generate or retrieve an existing monomorphized specialization.
    /// Ensures identical specializations produce the identical canonical symbol (deduplication).
    pub fn specialize(
        &mut self,
        func_name: &str,
        type_args: &[ConcreteType],
    ) -> Result<MonomorphizedFunction, String> {
        let template = self
            .templates
            .get(func_name)
            .ok_or_else(|| format!("Generic template '{}' not found", func_name))?;

        if template.type_params.len() != type_args.len() {
            return Err(format!(
                "Generic template '{}' expects {} type parameters, provided {}",
                func_name,
                template.type_params.len(),
                type_args.len()
            ));
        }

        // Mangle symbol deterministically: `func_name$T_arg1_arg2`
        let mangled_args = type_args
            .iter()
            .map(|t| t.mangle())
            .collect::<Vec<_>>()
            .join("_");
        let mangled_symbol = format!("{}__mono__{}", func_name, mangled_args);

        // Check deduplication
        if let Some(existing) = self.specializations.get_mut(&mangled_symbol) {
            existing.call_count += 1;
            return Ok(existing.clone());
        }

        // Canonical mapping
        let canonical_symbol = mangled_symbol.clone();
        self.symbol_table
            .insert(mangled_symbol.clone(), canonical_symbol.clone());

        // Infer return type based on substitution
        let return_type = if let Some(idx) = template
            .type_params
            .iter()
            .position(|p| p == &template.return_type)
        {
            type_args[idx].clone()
        } else {
            ConcreteType::I64
        };

        let mono = MonomorphizedFunction {
            original_name: func_name.to_string(),
            specialized_symbol: canonical_symbol,
            type_arguments: type_args.to_vec(),
            return_type,
            is_inline: template.is_inline,
            call_count: 1,
        };

        self.specializations
            .insert(mangled_symbol, mono.clone());

        Ok(mono)
    }

    /// Retrieve all unique monomorphized specializations for code generation.
    pub fn all_specializations(&self) -> Vec<MonomorphizedFunction> {
        self.specializations.values().cloned().collect()
    }

    /// Number of unique functions monomorphized (after deduplication).
    pub fn unique_function_count(&self) -> usize {
        self.specializations.len()
    }
}
