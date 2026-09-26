//! Module cache management for the tiered JIT compiler.
//!
//! Handles loading, compiling, and caching of imported modules. Supports both
//! AdeshLang source modules and WASM modules with efficient Arc-based sharing.

use super::types::{CachedModule, FunctionProfile};
use crate::backends::jit::builtins::{CallableFunction, RuntimeValue};
use crate::backends::jit::lir::{LirFunction, LirInst, LirModule, LirType};
use crate::utils::collections::FastMap;
use std::sync::Arc;

/// Load an LIR module into the JIT context.
///
/// Extracts functions and creates profiling data for tier promotion decisions.
pub fn load_module(
    module: &LirModule,
    functions: &mut FastMap<String, LirFunction>,
    profiles: &mut FastMap<String, FunctionProfile>,
    import_aliases: &mut FastMap<String, String>,
) {
    // Store import aliases from the module
    for (alias, path) in &module.imports {
        import_aliases.insert(alias.clone(), path.clone());
    }

    for func in &module.functions {
        // Count instructions for profiling
        let inst_count: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();

        // Create profile with instruction count
        let mut profile = FunctionProfile::new(inst_count);

        // Detect recursive functions (simple heuristic: calls itself)
        for block in &func.blocks {
            for inst in &block.instructions {
                if let LirInst::Call(_, name, _) = inst {
                    if name == &func.name {
                        profile.is_recursive = true;
                    }
                }
            }
        }

        profiles.insert(func.name.clone(), profile);
        functions.insert(func.name.clone(), func.clone());
    }
}

/// Load an imported module and return its namespace object.
///
/// Uses caching with Arc for efficient sharing. Supports both AdeshLang source
/// modules and WASM modules.
pub fn load_imported_module(
    alias: &str,
    module_cache: &mut FastMap<String, CachedModule>,
    import_aliases: &FastMap<String, String>,
    functions: &mut FastMap<String, LirFunction>,
    profiles: &mut FastMap<String, FunctionProfile>,
    dynamic_builtins: &mut FastMap<
        String,
        Box<dyn Fn(&[RuntimeValue]) -> RuntimeValue + Send + Sync>,
    >,
) -> Result<RuntimeValue, String> {
    // Check if already cached
    if let Some(cached) = module_cache.get(alias) {
        let namespace = (*cached.namespace).clone();
        return Ok(RuntimeValue::Object(namespace));
    }

    // Get the module path from alias
    let path = import_aliases
        .get(alias)
        .ok_or_else(|| format!("Import alias '{}' not found", alias))?
        .clone();

    if path == "fs"
        || path == "std:fs"
        || path == "FS"
        || path == "std:FS"
        || path == "time"
        || path == "std:time"
        || path == "Regex"
        || path == "std:Regex"
        || path == "Math"
        || path == "std:Math"
        || path == "cmath"
        || path == "std:cmath"
        || path == "thread"
        || path == "std:thread"
        || path == "Thread"
        || path == "std:Thread"
        || path == "concurrency"
        || path == "std:concurrency"
    {
        let namespace = FastMap::default();
        return Ok(RuntimeValue::Object(namespace));
    }

    // Resolve path - try multiple strategies
    let resolved_path = std::path::Path::new(&path);
    let mut absolute_path = None;

    if resolved_path.is_absolute() {
        absolute_path = Some(resolved_path.to_path_buf());
    } else {
        if let Ok(base_dir) = std::env::var("ADESH_BASE_DIR") {
            let base_path = std::path::Path::new(&base_dir).join(resolved_path);
            if base_path.exists() {
                absolute_path = Some(base_path);
            }
        }
        if absolute_path.is_none() {
            if let Ok(curr_file) = std::env::var("ADESH_CURRENT_FILE") {
                if let Some(parent) = std::path::Path::new(&curr_file).parent() {
                    let p = parent.join(resolved_path);
                    if p.exists() {
                        absolute_path = Some(p);
                    }
                }
            }
        }
        if absolute_path.is_none() {
            let cwd_path = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
                .join(resolved_path);
            if cwd_path.exists() {
                absolute_path = Some(cwd_path);
            } else {
                if path.starts_with("../") {
                    let from_examples = std::env::current_dir()
                        .map_err(|e| format!("Failed to get current directory: {}", e))?
                        .join("examples/async")
                        .join(resolved_path);
                    if from_examples.exists() {
                        absolute_path = Some(from_examples);
                    }
                }
                if absolute_path.is_none() {
                    let fallback =
                        crate::execution::runtime_core::interpreter_impl::utilities::resolve_path(
                            &path, ".",
                        );
                    let fb_path = std::path::PathBuf::from(&fallback);
                    if fb_path.exists() {
                        absolute_path = Some(fb_path);
                    }
                }
            }
        }
    }

    let absolute_path = absolute_path.ok_or_else(|| {
        format!(
            "Module not found: '{}' (tried base dir, CWD, and search paths)",
            path
        )
    })?;

    // Detect WASM module
    let is_wasm = if let Ok(mut file) = std::fs::File::open(&absolute_path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).is_ok() && &magic == b"\0asm"
    } else {
        false
    };

    if is_wasm {
        return load_wasm_module_impl(
            alias,
            &absolute_path,
            module_cache,
            functions,
            profiles,
            dynamic_builtins,
        );
    }

    // Parse and compile as Source module
    load_source_module_impl(
        alias,
        &path,
        &absolute_path,
        module_cache,
        functions,
        profiles,
    )
}

/// Load a WASM module and create LIR wrappers for its exports.
fn load_wasm_module_impl(
    alias: &str,
    absolute_path: &std::path::Path,
    module_cache: &mut FastMap<String, CachedModule>,
    functions: &mut FastMap<String, LirFunction>,
    profiles: &mut FastMap<String, FunctionProfile>,
    dynamic_builtins: &mut FastMap<
        String,
        Box<dyn Fn(&[RuntimeValue]) -> RuntimeValue + Send + Sync>,
    >,
) -> Result<RuntimeValue, String> {
    use crate::backends::wasm_linker::{call_wasm_function, list_wasm_exports, load_wasm_module};

    // Load WASM module
    load_wasm_module(absolute_path)?;

    let exports = list_wasm_exports();
    let mut namespace = FastMap::default();

    // For each export, create a builtin and a LIR wrapper
    for export in exports {
        let func_name = export.name.clone();
        let builtin_name = format!("__wasm_{}_{}", alias, func_name);
        let builtin_name_captured = builtin_name.clone();
        let func_name_captured = func_name.clone();

        // Register builtin
        dynamic_builtins.insert(
            builtin_name.clone(),
            Box::new(move |args: &[RuntimeValue]| -> RuntimeValue {
                use crate::parsing::ast::Value as AstValue;
                let wasm_args: Vec<AstValue> = args
                    .iter()
                    .map(|v| match v {
                        RuntimeValue::Int(i) => AstValue::I64(*i),
                        RuntimeValue::F64(f) => AstValue::F64(*f),
                        RuntimeValue::Bool(b) => AstValue::Bool(*b),
                        RuntimeValue::String(s) => AstValue::Str(s.clone()),
                        RuntimeValue::Null => AstValue::Null,
                        _ => AstValue::Null,
                    })
                    .collect();

                match call_wasm_function(&func_name_captured, &wasm_args) {
                    Ok(v) => match v {
                        AstValue::Number(n) => RuntimeValue::F64(n),
                        AstValue::Bool(b) => RuntimeValue::Bool(b),
                        AstValue::Str(s) => RuntimeValue::String(s),
                        AstValue::Null => RuntimeValue::Null,
                        AstValue::I64(n) => RuntimeValue::Int(n),
                        AstValue::I32(n) => RuntimeValue::Int(n as i64),
                        AstValue::F64(n) => RuntimeValue::F64(n),
                        AstValue::F32(n) => RuntimeValue::F64(n as f64),
                        _ => RuntimeValue::Null,
                    },
                    Err(e) => RuntimeValue::String(format!("WASM Error: {}", e)),
                }
            }),
        );

        // Create LIR wrapper function
        // fn wrapper(a, b, ...) { return builtin(a, b, ...); }
        let params: Vec<(String, LirType)> = (0..export.params.len())
            .map(|i| (format!("arg{}", i), LirType::Ptr))
            .collect();

        let mut wrapper = LirFunction::new_exported(func_name.clone(), params, LirType::Ptr);

        let entry_id = wrapper.entry_block;

        // Load args
        let mut arg_indices = Vec::new();
        for i in 0..export.params.len() {
            let var = format!("arg{}", i);
            let dst = wrapper.alloc_value();
            wrapper.push_to_block(entry_id, LirInst::LoadVar(dst, var));
            arg_indices.push(dst);
        }

        let result_dst = wrapper.alloc_value();
        wrapper.push_to_block(
            entry_id,
            LirInst::Call(result_dst, builtin_name_captured, arg_indices),
        );
        wrapper.push_to_block(entry_id, LirInst::Return(Some(result_dst)));

        // Register wrapper function
        functions.insert(func_name.clone(), wrapper);
        profiles.insert(
            func_name.clone(),
            FunctionProfile::new(2 + export.params.len()),
        ); // rough count

        // Add to namespace
        let callable = CallableFunction {
            name: func_name.clone(),
            params: (0..export.params.len())
                .map(|i| format!("arg{}", i))
                .collect(),
            captures: FastMap::default(),
            is_async: false,
        };
        namespace.insert(func_name.clone(), RuntimeValue::Function(callable));
    }

    // Cache (only namespace for now, LIR is dummy)
    // Ideally we'd cache LirModule but we just added to functions directly
    // Creating a dummy LirModule to satisfy cache type
    let dummy_module = LirModule::new();

    let cached = CachedModule {
        lir: Arc::new(dummy_module),
        namespace: Arc::new(namespace.clone()),
    };
    module_cache.insert(alias.to_string(), cached);

    Ok(RuntimeValue::Object(namespace))
}

/// Load a AdeshLang source module and compile it to LIR.
fn load_source_module_impl(
    alias: &str,
    path: &str,
    absolute_path: &std::path::Path,
    module_cache: &mut FastMap<String, CachedModule>,
    functions: &mut FastMap<String, LirFunction>,
    profiles: &mut FastMap<String, FunctionProfile>,
) -> Result<RuntimeValue, String> {
    let source = std::fs::read_to_string(absolute_path).map_err(|e| {
        format!(
            "Failed to read module '{}' (resolved to '{}'): {}",
            path,
            absolute_path.display(),
            e
        )
    })?;

    use crate::backends::lir_lower::hir_to_lir;
    use crate::parsing::hir_lower::ast_to_hir;
    use crate::parsing::lexer::Lexer;
    use crate::parsing::parser::Parser;

    // Tokenize the source
    let mut lexer = Lexer::new(&source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("Failed to tokenize module '{}': {:?}", path, e))?;

    // Parse into AST
    let mut parser = Parser::new(tokens, Some(path.to_string()));
    let ast = parser
        .parse_program()
        .map_err(|e| format!("Failed to parse module '{}': {:?}", path, e))?;

    let hir = ast_to_hir(&ast, false)
        .map_err(|e| format!("Failed to lower module '{}' to HIR: {}", path, e))?;

    let lir =
        hir_to_lir(&hir).map_err(|e| format!("Failed to lower module '{}' to LIR: {}", path, e))?;

    // Load functions from imported module into this context
    for func in &lir.functions {
        if !functions.contains_key(&func.name) {
            functions.insert(func.name.clone(), func.clone());

            // Create profile for the imported function
            let inst_count: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();
            profiles.insert(func.name.clone(), FunctionProfile::new(inst_count));
        }
    }

    // Build namespace object with exported functions
    let mut namespace = FastMap::default();

    // Add all exported functions to namespace
    for func in &lir.functions {
        let callable = CallableFunction {
            name: func.name.clone(),
            params: func.params.iter().map(|(name, _)| name.clone()).collect(),
            captures: FastMap::default(),
            is_async: func.is_async,
        };
        namespace.insert(func.name.clone(), RuntimeValue::Function(callable));
    }

    // Cache the module with Arc for efficient sharing
    let cached = CachedModule {
        lir: Arc::new(lir),
        namespace: Arc::new(namespace.clone()),
    };
    module_cache.insert(alias.to_string(), cached);

    Ok(RuntimeValue::Object(namespace))
}
