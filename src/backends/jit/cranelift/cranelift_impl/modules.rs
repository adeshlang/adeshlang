//! Module loading and initialization for JIT execution

use crate::backends::jit::builtins::{CallableFunction, RuntimeValue};
use crate::backends::jit::cranelift::{CachedModule, JitContext};
use crate::utils::collections::FastMap;
use std::sync::Arc;

impl JitContext {
    /// Setup the input namespace object with methods like mock, select, form, etc.
    /// This allows input.mock(), input.select(), etc. to work in JIT
    pub(in crate::backends::jit::cranelift) fn setup_input_namespace(&mut self) {
        // Create a special marker object for input namespace
        // The actual method calls will be intercepted in call_builtin
        let mut input_obj = FastMap::default();

        // Add method markers that indicate these are callable input methods
        input_obj.insert(
            "mock".into(),
            RuntimeValue::String("__input_method:mock".into()),
        );
        input_obj.insert(
            "select".into(),
            RuntimeValue::String("__input_method:select".into()),
        );
        input_obj.insert(
            "selectEnum".into(),
            RuntimeValue::String("__input_method:selectEnum".into()),
        );
        input_obj.insert(
            "selectKey".into(),
            RuntimeValue::String("__input_method:selectKey".into()),
        );
        input_obj.insert(
            "checkbox".into(),
            RuntimeValue::String("__input_method:checkbox".into()),
        );
        input_obj.insert(
            "radio".into(),
            RuntimeValue::String("__input_method:radio".into()),
        );
        input_obj.insert(
            "form".into(),
            RuntimeValue::String("__input_method:form".into()),
        );
        input_obj.insert(
            "fromPipe".into(),
            RuntimeValue::String("__input_method:fromPipe".into()),
        );
        input_obj.insert(
            "voice".into(),
            RuntimeValue::String("__input_method:voice".into()),
        );
        input_obj.insert(
            "record".into(),
            RuntimeValue::String("__input_method:record".into()),
        );
        input_obj.insert(
            "play".into(),
            RuntimeValue::String("__input_method:play".into()),
        );

        // Store as global "input" object
        self.globals
            .insert("input".into(), RuntimeValue::Object(input_obj));
    }

    /// Load an LIR module into the JIT context
    pub(in crate::backends::jit::cranelift) fn load_imported_module(
        &mut self,
        alias: &str,
    ) -> Result<RuntimeValue, String> {
        // Check if already cached
        if let Some(cached) = self.module_cache.get(alias) {
            // Return cached namespace (Arc clone is cheap)
            let namespace = (*cached.namespace).clone();
            return Ok(RuntimeValue::Object(namespace));
        }

        // Get the module path from alias
        let path = self
            .import_aliases
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

        // Resolve path and read module source
        let source = if path == "Time" || path == "std:Time" {
            include_str!("../../../../stdlib/Time.adesh").to_string()
        } else if path == "System" || path == "std:System" || path == "system" || path == "std:system" {
            include_str!("../../../../stdlib/System.adesh").to_string()
        } else if path == "Env" || path == "std:Env" || path == "env" || path == "std:env" {
            include_str!("../../../../stdlib/Env.adesh").to_string()
        } else if path == "Path" || path == "std:Path" || path == "path" || path == "std:path" {
            include_str!("../../../../stdlib/Path.adesh").to_string()
        } else if path == "IO" || path == "std:IO" || path == "io" || path == "std:io" {
            include_str!("../../../../stdlib/IO.adesh").to_string()
        } else if path == "Process"
            || path == "std:Process"
            || path == "process"
            || path == "std:process"
        {
            include_str!("../../../../stdlib/Process.adesh").to_string()
        } else if path == "Crypto"
            || path == "std:Crypto"
            || path == "crypto"
            || path == "std:crypto"
        {
            include_str!("../../../../stdlib/Crypto.adesh").to_string()
        } else if path == "Encoding"
            || path == "std:Encoding"
            || path == "encoding"
            || path == "std:encoding"
        {
            include_str!("../../../../stdlib/Encoding.adesh").to_string()
        } else if path == "Compression"
            || path == "std:Compression"
            || path == "compression"
            || path == "std:compression"
        {
            include_str!("../../../../stdlib/Compression.adesh").to_string()
        } else if path == "URL" || path == "std:URL" || path == "url" || path == "std:url" {
            include_str!("../../../../stdlib/URL.adesh").to_string()
        } else if path == "TLS" || path == "std:TLS" || path == "tls" || path == "std:tls" {
            include_str!("../../../../stdlib/TLS.adesh").to_string()
        } else if path == "WebSocket"
            || path == "std:WebSocket"
            || path == "websocket"
            || path == "std:websocket"
        {
            include_str!("../../../../stdlib/WebSocket.adesh").to_string()
        } else {
            // Resolve path - try multiple strategies
            let resolved_path = std::path::Path::new(&path);
            let mut absolute_path = None;

            // Strategy 1: If absolute, use as-is
            if resolved_path.is_absolute() {
                absolute_path = Some(resolved_path.to_path_buf());
            } else {
                // Strategy 2: Try relative to current working directory
                let cwd_path = std::env::current_dir()
                    .map_err(|e| format!("Failed to get current directory: {}", e))?
                    .join(resolved_path);
                if cwd_path.exists() {
                    absolute_path = Some(cwd_path);
                } else {
                    // Strategy 3: For paths starting with ../, try resolving from examples/async/
                    // This handles the common case where we're running from project root
                    if path.starts_with("../") {
                        let from_examples = std::env::current_dir()
                            .map_err(|e| format!("Failed to get current directory: {}", e))?
                            .join("examples/async")
                            .join(resolved_path);
                        if from_examples.exists() {
                            absolute_path = Some(from_examples);
                        }
                    }
                }
            }

            let absolute_path = absolute_path.ok_or_else(|| {
                format!(
                    "Module not found: '{}' (tried CWD and examples/async)",
                    path
                )
            })?;

            std::fs::read_to_string(&absolute_path).map_err(|e| {
                format!(
                    "Failed to read module '{}' (resolved to '{}'): {}",
                    path,
                    absolute_path.display(),
                    e
                )
            })?
        };

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
        let mut parser = Parser::new(tokens, Some(path.clone()));
        let ast = parser
            .parse_program()
            .map_err(|e| format!("Failed to parse module '{}': {:?}", path, e))?;

        let hir = ast_to_hir(&ast, false)
            .map_err(|e| format!("Failed to lower module '{}' to HIR: {}", path, e))?;

        let lir = hir_to_lir(&hir)
            .map_err(|e| format!("Failed to lower module '{}' to LIR: {}", path, e))?;

        // Load functions from imported module into this context
        for func in &lir.functions {
            if !self.functions.contains_key(&func.name) {
                self.functions.insert(func.name.clone(), func.clone());
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
        self.module_cache.insert(alias.to_string(), cached);

        Ok(RuntimeValue::Object(namespace))
    }
}
