/// WASM Module Linker
///
/// Loads WASM modules and links exported functions using Wasmtime.
/// Enables Adesh programs to import and call WASM functions seamlessly.
///
/// Features:
/// - Full WASM module loading and validation
/// - Function export discovery and type checking
/// - Safe sandboxed execution via wasmtime
/// - WASM-compatible type mapping (i32, i64, f32, f64)
use crate::backends::ffi_import::{AbiType, AdeshType, ForeignFunction, ForeignFunctionSignature};
use crate::parsing::ast::Value;
use std::path::Path;
use wasmtime::{Caller, Engine, Instance, Linker, Module, Store, Val, ValType};

/// WASM execution context (heap state)
pub struct WasmContext {
    pub heap_ptr: u32,
}

/// WASM module loader and linker using Wasmtime
pub struct WasmLinker {
    /// Wasmtime engine (shared across all modules)
    engine: Engine,
    /// Currently loaded WASM module
    module: Option<Module>,
    /// Module instance for calling functions
    instance: Option<Instance>,
    /// Store for managing WASM state
    store: Option<Store<WasmContext>>,
    /// Loaded module path
    module_path: Option<String>,
    /// Exported function metadata cache
    exported_functions: Vec<WasmExportInfo>,
}

/// Information about an exported WASM function
#[derive(Debug, Clone)]
pub struct WasmExportInfo {
    /// Function name
    pub name: String,
    /// Parameter types (WASM types)
    pub params: Vec<WasmType>,
    /// Return types (WASM types)
    pub returns: Vec<WasmType>,
}

/// WASM value types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmType {
    /// Convert from wasmtime ValType
    pub fn from_valtype(vt: &ValType) -> Option<Self> {
        match vt {
            ValType::I32 => Some(WasmType::I32),
            ValType::I64 => Some(WasmType::I64),
            ValType::F32 => Some(WasmType::F32),
            ValType::F64 => Some(WasmType::F64),
            _ => None, // Unsupported types (funcref, externref, v128)
        }
    }

    /// Convert to AdeshType
    pub fn to_adesh_type(&self) -> AdeshType {
        match self {
            WasmType::I32 => AdeshType::I32,
            WasmType::I64 => AdeshType::I64,
            WasmType::F32 => AdeshType::F32,
            WasmType::F64 => AdeshType::F64,
        }
    }
}

impl WasmLinker {
    /// Create a new WASM linker with a fresh Wasmtime engine
    pub fn new() -> Self {
        let engine = Engine::default();
        WasmLinker {
            engine,
            module: None,
            instance: None,
            store: None,
            module_path: None,
            exported_functions: Vec::new(),
        }
    }

    /// Load a WASM module from a file
    ///
    /// This will:
    /// 1. Read and validate the .wasm file
    /// 2. Compile it using Wasmtime/Cranelift
    /// 3. Instantiate the module
    /// 4. Discover and cache exported functions
    pub fn load_module(&mut self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Err(format!("WASM module not found: {:?}", path));
        }

        // Read the WASM bytes
        let wasm_bytes =
            std::fs::read(path).map_err(|e| format!("Failed to read WASM file: {}", e))?;

        // Validate WASM magic number
        if wasm_bytes.len() < 8 {
            return Err("Invalid WASM file: too small".to_string());
        }
        if &wasm_bytes[0..4] != b"\0asm" {
            return Err("Invalid WASM file: bad magic number".to_string());
        }

        // Compile the module
        let module = Module::new(&self.engine, &wasm_bytes)
            .map_err(|e| format!("Failed to compile WASM module: {}", e))?;

        // Create a store for module state
        let mut store = Store::new(&self.engine, WasmContext { heap_ptr: 65536 });

        // Create a Linker and define imports
        let mut linker = Linker::new(&self.engine);

        // Define 'env' imports expected by Adesh's WASM backend

        // print_f64: (f64) -> ()
        linker
            .func_wrap("env", "print_f64", |v: f64| {
                println!("{}", v);
            })
            .unwrap();

        // print_str: (ptr, len) -> ()
        linker
            .func_wrap(
                "env",
                "print_str",
                |_caller: Caller<'_, WasmContext>, _ptr: i32, _len: i32| {
                    // In a real impl, we'd read memory here. For now, just print a marker.
                    println!("[str output]");
                },
            )
            .unwrap();

        // alloc: (size) -> ptr
        // Simple bump allocator stub returning high memory
        // In a real system, we'd track this in Store
        linker
            .func_wrap(
                "env",
                "alloc",
                |mut caller: Caller<'_, WasmContext>, sz: i32| -> i32 {
                    let ctx = caller.data_mut();
                    let ptr = ctx.heap_ptr;
                    ctx.heap_ptr += sz as u32;
                    ptr as i32
                },
            )
            .unwrap();

        // concat2: (p1, l1, p2, l2) -> ptr
        linker
            .func_wrap(
                "env",
                "concat2",
                |mut caller: Caller<'_, WasmContext>, p1: i32, l1: i32, p2: i32, l2: i32| -> i32 {
                    // 1. Allocate memory for result
                    let total_len = l1 + l2;
                    let size = total_len + 1; // +1 for null terminator

                    let res_ptr = {
                        let ctx = caller.data_mut();
                        let ptr = ctx.heap_ptr;
                        ctx.heap_ptr += size as u32;
                        ptr
                    };

                    // 2. Perform copy
                    // We need to look up memory. Wasm module must export "memory".
                    // Using get_export on caller is the standard way.
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        // Read first string
                        let mut buf1 = vec![0u8; l1 as usize];
                        let _ = mem.read(&caller, p1 as usize, &mut buf1);

                        // Read second string
                        let mut buf2 = vec![0u8; l2 as usize];
                        let _ = mem.read(&caller, p2 as usize, &mut buf2);

                        // Write concatenated
                        let _ = mem.write(&mut caller, res_ptr as usize, &buf1);
                        let _ = mem.write(&mut caller, (res_ptr + l1 as u32) as usize, &buf2);
                        // Null terminator
                        let _ = mem.write(&mut caller, (res_ptr + total_len as u32) as usize, &[0]);
                    }

                    res_ptr as i32
                },
            )
            .unwrap();

        // alloc_typed: (size, type) -> ptr
        linker
            .func_wrap("env", "alloc_typed", |_sz: i32, _ty: i32| -> i32 { 65536 })
            .unwrap();

        // input_str: (prompt_ptr, prompt_len) -> ptr
        linker
            .func_wrap(
                "env",
                "input_str",
                |mut caller: Caller<'_, WasmContext>, p: i32, l: i32| -> i32 {
                    let mut prompt = String::new();
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        if l > 0 {
                            let mut buf = vec![0u8; l as usize];
                            let _ = mem.read(&caller, p as usize, &mut buf);
                            if let Ok(s) = String::from_utf8(buf) {
                                prompt = s;
                            }
                        }
                    }
                    let input_val = crate::runtime::tui_input::prompt_input(
                        &prompt,
                        &crate::runtime::tui_input::InputOptions::default(),
                    )
                    .unwrap_or_default();
                    let bytes = input_val.as_bytes();
                    let size = bytes.len() + 1;
                    let res_ptr = {
                        let ctx = caller.data_mut();
                        let ptr = ctx.heap_ptr;
                        ctx.heap_ptr += size as u32;
                        ptr
                    };
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let _ = mem.write(&mut caller, res_ptr as usize, bytes);
                        let _ =
                            mem.write(&mut caller, (res_ptr + bytes.len() as u32) as usize, &[0]);
                    }
                    res_ptr as i32
                },
            )
            .unwrap();

        // input_f64: (prompt_ptr, prompt_len) -> f64
        linker
            .func_wrap(
                "env",
                "input_f64",
                |mut caller: Caller<'_, WasmContext>, p: i32, l: i32| -> f64 {
                    let mut prompt = String::new();
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        if l > 0 {
                            let mut buf = vec![0u8; l as usize];
                            let _ = mem.read(&caller, p as usize, &mut buf);
                            if let Ok(s) = String::from_utf8(buf) {
                                prompt = s;
                            }
                        }
                    }
                    let input_val = crate::runtime::tui_input::prompt_input(
                        &prompt,
                        &crate::runtime::tui_input::InputOptions::default(),
                    )
                    .unwrap_or_default();
                    input_val.trim().parse::<f64>().unwrap_or(0.0)
                },
            )
            .unwrap();

        // input_slider: (prompt_ptr, prompt_len, min, max, step, default) -> f64
        linker
            .func_wrap(
                "env",
                "input_slider",
                |mut caller: Caller<'_, WasmContext>,
                 p: i32,
                 l: i32,
                 min: f64,
                 max: f64,
                 step: f64,
                 default: f64|
                 -> f64 {
                    let mut prompt = String::new();
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        if l > 0 {
                            let mut buf = vec![0u8; l as usize];
                            let _ = mem.read(&caller, p as usize, &mut buf);
                            if let Ok(s) = String::from_utf8(buf) {
                                prompt = s;
                            }
                        }
                    }
                    let config = crate::runtime::tui_input::SliderConfig::default();
                    crate::runtime::tui_input::prompt_slider(
                        &prompt, min, max, step, default, &config,
                    )
                    .unwrap_or(default)
                },
            )
            .unwrap();

        // input_confirm: (prompt_ptr, prompt_len, default_val) -> i32
        linker
            .func_wrap(
                "env",
                "input_confirm",
                |mut caller: Caller<'_, WasmContext>, p: i32, l: i32, default_val: i32| -> i32 {
                    let mut prompt = String::new();
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        if l > 0 {
                            let mut buf = vec![0u8; l as usize];
                            let _ = mem.read(&caller, p as usize, &mut buf);
                            if let Ok(s) = String::from_utf8(buf) {
                                prompt = s;
                            }
                        }
                    }
                    let def_bool = default_val != 0;
                    if crate::runtime::tui_input::prompt_confirm(&prompt, def_bool)
                        .unwrap_or(def_bool)
                    {
                        1
                    } else {
                        0
                    }
                },
            )
            .unwrap();

        // load_typed: (ptr, type) -> i32 (should be i64 or polymorphic, but backend uses i32 signature for now)
        linker
            .func_wrap("env", "load_typed", |_ptr: i32, _ty: i32| -> i32 { 0 })
            .unwrap();

        // store_typed: (ptr, val, type) -> ()
        linker
            .func_wrap("env", "store_typed", |_ptr: i32, _val: i32, _ty: i32| {})
            .unwrap();

        // Instantiate using the linker
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("Failed to instantiate WASM module: {}", e))?;

        // Discover exported functions
        let mut exported_functions = Vec::new();
        for export in module.exports() {
            if let Some(func_type) = export.ty().func() {
                let name = export.name().to_string();

                // Convert parameter types
                let params: Vec<WasmType> = func_type
                    .params()
                    .filter_map(|p| WasmType::from_valtype(&p))
                    .collect();

                // Convert return types
                let returns: Vec<WasmType> = func_type
                    .results()
                    .filter_map(|r| WasmType::from_valtype(&r))
                    .collect();

                // Only include functions with fully supported types
                if params.len() == func_type.params().len()
                    && returns.len() == func_type.results().len()
                {
                    exported_functions.push(WasmExportInfo {
                        name,
                        params,
                        returns,
                    });
                }
            }
        }

        self.module = Some(module);
        self.instance = Some(instance);
        self.store = Some(store);
        self.module_path = Some(path.to_string_lossy().to_string());
        self.exported_functions = exported_functions;

        Ok(())
    }

    /// Get exported function from the loaded WASM module
    ///
    /// Returns a ForeignFunction that can be used with the FFI system.
    pub fn get_export(
        &self,
        name: &str,
        params: Vec<AdeshType>,
        ret_type: AdeshType,
    ) -> Result<ForeignFunction, String> {
        // Validate that all types are WASM-compatible
        for param in &params {
            Self::validate_wasm_type(param)?;
        }
        Self::validate_wasm_type(&ret_type)?;

        // Check if the module is loaded
        if self.module.is_none() {
            return Err("No WASM module loaded. Call load_module() first.".to_string());
        }

        // Find the exported function
        let export_info = self
            .exported_functions
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| {
                let available: Vec<&str> = self
                    .exported_functions
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect();
                format!(
                    "WASM export '{}' not found. Available exports: {:?}",
                    name, available
                )
            })?;

        // Validate parameter count matches
        if export_info.params.len() != params.len() {
            return Err(format!(
                "Parameter count mismatch for '{}': expected {} got {}",
                name,
                export_info.params.len(),
                params.len()
            ));
        }

        // Create ForeignFunction for FFI compatibility
        Ok(ForeignFunction {
            name: name.to_string(),
            abi: AbiType::Wasm,
            ptr: std::ptr::null(), // WASM functions don't use raw pointers
            signature: ForeignFunctionSignature::new(params, ret_type),
        })
    }

    /// Call a WASM function by name with arguments
    pub fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        let store = self.store.as_mut().ok_or("No WASM module loaded")?;
        let instance = self.instance.as_ref().ok_or("No WASM instance available")?;

        // Get the function from the instance
        let func = instance
            .get_func(&mut *store, name)
            .ok_or_else(|| format!("WASM function '{}' not found", name))?;

        // Get function type for conversion
        let func_type = func.ty(&*store);
        let param_types: Vec<ValType> = func_type.params().collect();

        // Convert arguments to WASM values
        if args.len() != param_types.len() {
            return Err(format!(
                "Argument count mismatch for '{}': expected {} got {}",
                name,
                param_types.len(),
                args.len()
            ));
        }

        let mut allocated_ptrs = Vec::new();
        let mut wasm_args = Vec::with_capacity(args.len());

        for (i, (arg, ty)) in args.iter().zip(param_types.iter()).enumerate() {
            if let Value::Str(s) = arg {
                // Special handling for strings: allocate and write to WASM memory
                // Checks for i32 param type (pointer)
                if matches!(*ty, ValType::I32) {
                    let ptr = Self::write_string(store, instance, s)?;
                    allocated_ptrs.push(ptr);
                    wasm_args.push(Val::I32(ptr));
                } else {
                    return Err(format!(
                        "Arg {}: Cannot pass String to non-i32 WASM param",
                        i
                    ));
                }
            } else if let Value::Array(arr) = arg {
                // Arrays: write to WASM memory as contiguous i64 values
                if matches!(*ty, ValType::I32) {
                    let elem_size = 8usize; // i64 elements
                    let total_size = (arr.len() * elem_size) as i32;

                    // Allocate memory
                    let malloc = instance
                        .get_func(&mut *store, "malloc")
                        .ok_or("Module does not export 'malloc' needed for array passing")?;

                    let mut results = [Val::I32(0)];
                    malloc
                        .call(&mut *store, &[Val::I32(total_size)], &mut results)
                        .map_err(|e| format!("Failed to call malloc: {}", e))?;

                    let ptr = match results[0] {
                        Val::I32(p) => p,
                        _ => return Err("malloc returned non-i32".to_string()),
                    };

                    let memory = instance
                        .get_memory(&mut *store, "memory")
                        .ok_or("Module does not export 'memory'")?;

                    // Write each element as i64
                    for (j, val) in arr.iter().enumerate() {
                        let offset = ptr as usize + j * elem_size;
                        let n = Self::value_to_i64(val).unwrap_or(0);
                        memory
                            .write(&mut *store, offset, &n.to_le_bytes())
                            .map_err(|e| format!("Failed to write array element: {}", e))?;
                    }

                    allocated_ptrs.push(ptr);
                    wasm_args.push(Val::I32(ptr));
                } else {
                    return Err(format!(
                        "Arg {}: Cannot pass Array to non-i32 WASM param",
                        i
                    ));
                }
            } else if let Value::DynArray(darr) = arg {
                // DynArray: same as Array but uses .data field
                if matches!(*ty, ValType::I32) {
                    let arr = &darr.data;
                    let elem_size = 8usize;
                    let total_size = (arr.len() * elem_size) as i32;

                    let malloc = instance
                        .get_func(&mut *store, "malloc")
                        .ok_or("Module does not export 'malloc' needed for array passing")?;

                    let mut results = [Val::I32(0)];
                    malloc
                        .call(&mut *store, &[Val::I32(total_size)], &mut results)
                        .map_err(|e| format!("Failed to call malloc: {}", e))?;

                    let ptr = match results[0] {
                        Val::I32(p) => p,
                        _ => return Err("malloc returned non-i32".to_string()),
                    };

                    let memory = instance
                        .get_memory(&mut *store, "memory")
                        .ok_or("Module does not export 'memory'")?;

                    for (j, val) in arr.iter().enumerate() {
                        let offset = ptr as usize + j * elem_size;
                        let n = Self::value_to_i64(val).unwrap_or(0);
                        memory
                            .write(&mut *store, offset, &n.to_le_bytes())
                            .map_err(|e| format!("Failed to write array element: {}", e))?;
                    }

                    allocated_ptrs.push(ptr);
                    wasm_args.push(Val::I32(ptr));
                } else {
                    return Err(format!(
                        "Arg {}: Cannot pass DynArray to non-i32 WASM param",
                        i
                    ));
                }
            } else {
                // Regular primitives
                wasm_args.push(Self::value_to_wasm(arg, ty)?);
            }
        }

        // Prepare result buffer
        let result_types: Vec<ValType> = func_type.results().collect();
        let mut results = vec![Val::I32(0); result_types.len()];

        // Call the function
        let call_result = func.call(&mut *store, &wasm_args, &mut results);

        // Free allocated strings regardless of success
        for ptr in allocated_ptrs {
            // Best effort free, ignore errors to preserve original call result/error
            let _ = Self::free_memory(store, instance, ptr);
        }

        call_result.map_err(|e| format!("WASM function call failed: {}", e))?;

        // Convert result back to Value
        if results.is_empty() {
            Ok(Value::Null)
        } else {
            Self::wasm_to_value(&results[0])
        }
    }

    /// Helper: Allocate and write a string to WASM memory (Null-terminated)
    fn write_string(
        store: &mut Store<WasmContext>,
        instance: &Instance,
        s: &str,
    ) -> Result<i32, String> {
        // 1. Allocate memory directly from context
        let len = s.len() as u32;
        let size = len + 1;

        let ctx = store.data_mut();
        let ptr = ctx.heap_ptr;
        ctx.heap_ptr += size;

        // 2. Find 'memory' export
        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("Module does not export 'memory'")?;

        // 3. Write bytes
        let bytes = s.as_bytes();
        memory
            .write(&mut *store, ptr as usize, bytes)
            .map_err(|e| format!("Failed to write string bytes: {}", e))?;

        // Write null terminator
        memory
            .write(&mut *store, (ptr + len) as usize, &[0])
            .map_err(|e| format!("Failed to write null terminator: {}", e))?;

        Ok(ptr as i32)
    }

    /// Helper: Free allocated memory
    fn free_memory(
        _store: &mut Store<WasmContext>,
        _instance: &Instance,
        _ptr: i32,
    ) -> Result<(), String> {
        // Bump allocator doesn't support free, so this is a no-op
        Ok(())
    }

    /// Read a null-terminated string from WASM memory
    ///
    /// Use this when WASM returns a pointer to a string
    pub fn read_string(&mut self, ptr: i32) -> Result<String, String> {
        let store = self.store.as_mut().ok_or("No WASM module loaded")?;
        let instance = self.instance.as_ref().ok_or("No WASM instance available")?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("Module does not export 'memory'")?;

        // Read bytes until null terminator (max 64KB for safety)
        let max_len = 65536;
        let mut bytes = Vec::new();

        for offset in 0..max_len {
            let mut buf = [0u8; 1];
            memory
                .read(&*store, (ptr as usize) + offset, &mut buf)
                .map_err(|e| format!("Failed to read from WASM memory: {}", e))?;

            if buf[0] == 0 {
                break;
            }
            bytes.push(buf[0]);
        }

        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8 in WASM string: {}", e))
    }

    /// Read a string from WASM memory with known length
    pub fn read_string_len(&mut self, ptr: i32, len: i32) -> Result<String, String> {
        let store = self.store.as_mut().ok_or("No WASM module loaded")?;
        let instance = self.instance.as_ref().ok_or("No WASM instance available")?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("Module does not export 'memory'")?;

        let mut bytes = vec![0u8; len as usize];
        memory
            .read(&*store, ptr as usize, &mut bytes)
            .map_err(|e| format!("Failed to read from WASM memory: {}", e))?;

        String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8 in WASM string: {}", e))
    }

    /// Write an array to WASM memory
    ///
    /// Returns (ptr, len) where ptr is the memory address and len is element count
    pub fn write_array(
        &mut self,
        values: &[Value],
        elem_type: WasmType,
    ) -> Result<(i32, i32), String> {
        let store = self.store.as_mut().ok_or("No WASM module loaded")?;
        let instance = self.instance.as_ref().ok_or("No WASM instance available")?;

        let elem_size = match elem_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
        };

        let total_size = (values.len() * elem_size) as i32;

        // Allocate memory
        let malloc = instance
            .get_func(&mut *store, "malloc")
            .ok_or("Module does not export 'malloc' needed for array passing")?;

        let mut results = [Val::I32(0)];
        malloc
            .call(&mut *store, &[Val::I32(total_size)], &mut results)
            .map_err(|e| format!("Failed to call malloc: {}", e))?;

        let ptr = match results[0] {
            Val::I32(p) => p,
            _ => return Err("malloc returned non-i32".to_string()),
        };

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("Module does not export 'memory'")?;

        // Write each element
        for (i, value) in values.iter().enumerate() {
            let offset = ptr as usize + i * elem_size;

            match elem_type {
                WasmType::I32 => {
                    let n = Self::value_to_i64(value)? as i32;
                    memory
                        .write(&mut *store, offset, &n.to_le_bytes())
                        .map_err(|e| format!("Failed to write array element: {}", e))?;
                }
                WasmType::I64 => {
                    let n = Self::value_to_i64(value)?;
                    memory
                        .write(&mut *store, offset, &n.to_le_bytes())
                        .map_err(|e| format!("Failed to write array element: {}", e))?;
                }
                WasmType::F32 => {
                    let f = Self::value_to_f64(value)? as f32;
                    memory
                        .write(&mut *store, offset, &f.to_le_bytes())
                        .map_err(|e| format!("Failed to write array element: {}", e))?;
                }
                WasmType::F64 => {
                    let f = Self::value_to_f64(value)?;
                    memory
                        .write(&mut *store, offset, &f.to_le_bytes())
                        .map_err(|e| format!("Failed to write array element: {}", e))?;
                }
            }
        }

        Ok((ptr, values.len() as i32))
    }

    /// Read an array from WASM memory
    pub fn read_array(
        &mut self,
        ptr: i32,
        len: i32,
        elem_type: WasmType,
    ) -> Result<Vec<Value>, String> {
        let store = self.store.as_mut().ok_or("No WASM module loaded")?;
        let instance = self.instance.as_ref().ok_or("No WASM instance available")?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("Module does not export 'memory'")?;

        let elem_size = match elem_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
        };

        let mut values = Vec::with_capacity(len as usize);

        for i in 0..(len as usize) {
            let offset = ptr as usize + i * elem_size;

            let value = match elem_type {
                WasmType::I32 => {
                    let mut buf = [0u8; 4];
                    memory
                        .read(&*store, offset, &mut buf)
                        .map_err(|e| format!("Failed to read array element: {}", e))?;
                    Value::I32(i32::from_le_bytes(buf))
                }
                WasmType::I64 => {
                    let mut buf = [0u8; 8];
                    memory
                        .read(&*store, offset, &mut buf)
                        .map_err(|e| format!("Failed to read array element: {}", e))?;
                    Value::I64(i64::from_le_bytes(buf))
                }
                WasmType::F32 => {
                    let mut buf = [0u8; 4];
                    memory
                        .read(&*store, offset, &mut buf)
                        .map_err(|e| format!("Failed to read array element: {}", e))?;
                    Value::F32(f32::from_le_bytes(buf))
                }
                WasmType::F64 => {
                    let mut buf = [0u8; 8];
                    memory
                        .read(&*store, offset, &mut buf)
                        .map_err(|e| format!("Failed to read array element: {}", e))?;
                    Value::F64(f64::from_le_bytes(buf))
                }
            };

            values.push(value);
        }

        Ok(values)
    }

    /// Call a WASM function and interpret result as string
    ///
    /// Useful when you know the function returns a string pointer
    pub fn call_function_get_string(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Result<String, String> {
        let result = self.call_function(name, args)?;

        // Result should be i32 (pointer)
        let ptr = match result {
            Value::I32(p) => p,
            Value::I64(p) => p as i32,
            _ => return Err("Function did not return a pointer".to_string()),
        };

        self.read_string(ptr)
    }

    /// Call a WASM function and interpret result as array
    ///
    /// Requires function to return (ptr, len) via multi-value or separate calls
    pub fn call_function_get_array(
        &mut self,
        name: &str,
        args: &[Value],
        len: i32,
        elem_type: WasmType,
    ) -> Result<Vec<Value>, String> {
        let result = self.call_function(name, args)?;

        let ptr = match result {
            Value::I32(p) => p,
            Value::I64(p) => p as i32,
            _ => return Err("Function did not return a pointer".to_string()),
        };

        self.read_array(ptr, len, elem_type)
    }

    /// List all exported functions
    pub fn list_exports(&self) -> &[WasmExportInfo] {
        &self.exported_functions
    }

    /// Check if a module is loaded
    pub fn is_loaded(&self) -> bool {
        self.module.is_some()
    }

    /// Get the loaded module path
    pub fn module_path(&self) -> Option<&str> {
        self.module_path.as_deref()
    }

    /// Check if a type is valid for WASM FFI
    fn validate_wasm_type(ty: &AdeshType) -> Result<(), String> {
        match ty {
            AdeshType::I32
            | AdeshType::I64
            | AdeshType::U32
            | AdeshType::U64
            | AdeshType::F32
            | AdeshType::F64
            | AdeshType::Void => Ok(()),
            _ => Err(format!(
                "WASM FFI only supports: i32, i64, u32, u64, f32, f64, void. Got: {:?}",
                ty
            )),
        }
    }

    /// Convert Adesh Value to WASM Val
    fn value_to_wasm(value: &Value, target_type: &ValType) -> Result<Val, String> {
        match target_type {
            ValType::I32 => {
                let n = Self::value_to_i64(value)?;
                Ok(Val::I32(n as i32))
            }
            ValType::I64 => {
                let n = Self::value_to_i64(value)?;
                Ok(Val::I64(n))
            }
            ValType::F32 => {
                let f = Self::value_to_f64(value)?;
                Ok(Val::F32((f as f32).to_bits()))
            }
            ValType::F64 => {
                let f = Self::value_to_f64(value)?;
                Ok(Val::F64(f.to_bits()))
            }
            _ => Err(format!("Unsupported WASM type: {:?}", target_type)),
        }
    }

    /// Convert WASM Val to Adesh Value
    fn wasm_to_value(val: &Val) -> Result<Value, String> {
        match val {
            Val::I32(n) => Ok(Value::I32(*n)),
            Val::I64(n) => Ok(Value::I64(*n)),
            Val::F32(bits) => Ok(Value::F32(f32::from_bits(*bits))),
            Val::F64(bits) => Ok(Value::F64(f64::from_bits(*bits))),
            _ => Err(format!("Unsupported WASM result type: {:?}", val)),
        }
    }

    /// Helper: convert Value to i64
    fn value_to_i64(value: &Value) -> Result<i64, String> {
        match value {
            Value::Number(n) => Ok(*n as i64),
            Value::I64(n) => Ok(*n),
            Value::I32(n) => Ok(*n as i64),
            Value::I16(n) => Ok(*n as i64),
            Value::I8(n) => Ok(*n as i64),
            Value::U64(n) => Ok(*n as i64),
            Value::U32(n) => Ok(*n as i64),
            Value::U16(n) => Ok(*n as i64),
            Value::U8(n) => Ok(*n as i64),
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            _ => Err("Expected integer value for WASM i32/i64".to_string()),
        }
    }

    /// Helper: convert Value to f64
    fn value_to_f64(value: &Value) -> Result<f64, String> {
        match value {
            Value::Number(n) => Ok(*n),
            Value::F64(n) => Ok(*n),
            Value::F32(n) => Ok(*n as f64),
            Value::I64(n) => Ok(*n as f64),
            Value::I32(n) => Ok(*n as f64),
            Value::U64(n) => Ok(*n as f64),
            Value::U32(n) => Ok(*n as f64),
            _ => Err("Expected numeric value for WASM f32/f64".to_string()),
        }
    }
}

impl Default for WasmLinker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global WASM linker instance for easy access
static WASM_LINKER: std::sync::OnceLock<std::sync::Mutex<WasmLinker>> = std::sync::OnceLock::new();

/// Get the global WASM linker
pub fn get_wasm_linker() -> &'static std::sync::Mutex<WasmLinker> {
    WASM_LINKER.get_or_init(|| std::sync::Mutex::new(WasmLinker::new()))
}

/// Load a WASM module globally
pub fn load_wasm_module(path: &Path) -> Result<(), String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.load_module(path)
}

/// Call a WASM function globally
pub fn call_wasm_function(name: &str, args: &[Value]) -> Result<Value, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.call_function(name, args)
}

/// List exported WASM functions globally
pub fn list_wasm_exports() -> Vec<WasmExportInfo> {
    let linker = get_wasm_linker();
    if let Ok(guard) = linker.lock() {
        guard.list_exports().to_vec()
    } else {
        Vec::new()
    }
}

/// Read a null-terminated string from WASM memory globally
pub fn read_wasm_string(ptr: i32) -> Result<String, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.read_string(ptr)
}

/// Read a string with known length from WASM memory globally
pub fn read_wasm_string_len(ptr: i32, len: i32) -> Result<String, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.read_string_len(ptr, len)
}

/// Write an array to WASM memory globally
pub fn write_wasm_array(values: &[Value], elem_type: WasmType) -> Result<(i32, i32), String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.write_array(values, elem_type)
}

/// Read an array from WASM memory globally
pub fn read_wasm_array(ptr: i32, len: i32, elem_type: WasmType) -> Result<Vec<Value>, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.read_array(ptr, len, elem_type)
}

/// Call a WASM function and get string result globally
pub fn call_wasm_function_get_string(name: &str, args: &[Value]) -> Result<String, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.call_function_get_string(name, args)
}

/// Call a WASM function and get array result globally
pub fn call_wasm_function_get_array(
    name: &str,
    args: &[Value],
    len: i32,
    elem_type: WasmType,
) -> Result<Vec<Value>, String> {
    let linker = get_wasm_linker();
    let mut guard = linker
        .lock()
        .map_err(|e| format!("Failed to lock WASM linker: {}", e))?;
    guard.call_function_get_array(name, args, len, elem_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_linker_creation() {
        let linker = WasmLinker::new();
        assert!(!linker.is_loaded());
        assert!(linker.module_path().is_none());
    }

    #[test]
    fn test_wasm_type_validation() {
        assert!(WasmLinker::validate_wasm_type(&AdeshType::I32).is_ok());
        assert!(WasmLinker::validate_wasm_type(&AdeshType::F64).is_ok());
        assert!(WasmLinker::validate_wasm_type(&AdeshType::Void).is_ok());
        assert!(WasmLinker::validate_wasm_type(&AdeshType::String).is_err());
        assert!(WasmLinker::validate_wasm_type(&AdeshType::VoidPtr).is_err());
    }

    #[test]
    fn test_wasm_type_conversion() {
        assert_eq!(WasmType::I32.to_adesh_type(), AdeshType::I32);
        assert_eq!(WasmType::I64.to_adesh_type(), AdeshType::I64);
        assert_eq!(WasmType::F32.to_adesh_type(), AdeshType::F32);
        assert_eq!(WasmType::F64.to_adesh_type(), AdeshType::F64);
    }

    #[test]
    fn test_value_conversion() {
        // i64 conversion
        assert_eq!(WasmLinker::value_to_i64(&Value::I32(42)).unwrap(), 42);
        assert_eq!(WasmLinker::value_to_i64(&Value::I64(100)).unwrap(), 100);
        assert_eq!(WasmLinker::value_to_i64(&Value::Bool(true)).unwrap(), 1);

        // f64 conversion
        assert!((WasmLinker::value_to_f64(&Value::F64(3.14)).unwrap() - 3.14).abs() < 0.0001);
        assert!((WasmLinker::value_to_f64(&Value::Number(2.5)).unwrap() - 2.5).abs() < 0.0001);
    }
}
