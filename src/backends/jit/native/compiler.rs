//! Native JIT Compiler Implementation
//!
//! This module implements the core Native JIT compiler using cranelift-jit.

use cranelift::prelude::*;
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module};
use std::collections::{HashMap, HashSet};

use super::context::NativeJitContext;
use crate::backends::aot::cranelift::AotValueType;
use crate::backends::aot::cranelift_impl::helpers::{
    make_bg_color_escape, make_fg_color_escape, parse_hex_color,
};
use crate::backends::aot::cranelift_impl::{arithmetic, comparisons, conversions};
use crate::backends::common::lir::{BlockId, LirFunction, LirInst, LirModule, ValueId};

/// Native JIT Compiler
///
/// Compiles LIR modules to native machine code at runtime using Cranelift.
pub struct NativeJitCompiler {
    /// Cranelift JIT module
    module: JITModule,
    /// Cranelift code generation context
    ctx: codegen::Context,
    /// Function IDs for compiled functions
    functions: HashMap<String, cranelift_module::FuncId>,
    /// Data IDs for string constants
    string_data: HashMap<usize, cranelift_module::DataId>,
    /// String pool for string constants
    string_pool: Vec<String>,
    /// Data IDs for global variables
    global_vars: HashMap<String, cranelift_module::DataId>,
    /// Global variable types
    global_types: HashMap<String, crate::backends::common::lir::LirType>,
    /// Cached extra string data IDs (format strings, ANSI codes, etc.)
    format_strings: HashMap<String, cranelift_module::DataId>,
    /// Map of function name -> return type (for correct call result typing)
    function_return_types: HashMap<String, crate::backends::common::lir::LirType>,
}
#[allow(unused)]
#[derive(Debug, Clone, Default)]
struct JitPrintOptions {
    sep: Option<String>,
    end: Option<String>,
    color: Option<String>,
    background: Option<String>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    file: Option<String>,
    flush: bool,
    pretty: Option<String>,
}

#[allow(dead_code)]
impl NativeJitCompiler {
    /// Create a new Native JIT compiler
    pub fn new() -> Result<Self, String> {
        // Create Cranelift settings for the native target with optimizations
        let mut flag_builder = settings::builder();

        // Set optimization level to speed for maximum performance
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| format!("Failed to set opt_level: {}", e))?;

        // Disable position-independent code for better performance
        flag_builder
            .set("is_pic", "false")
            .map_err(|e| format!("Failed to set is_pic: {}", e))?;

        // Enable colocated libcalls for direct relative branches
        let _ = flag_builder.set("use_colocated_libcalls", "true");

        // Enable SIMD / vector instructions when supported
        let _ = flag_builder.enable("enable_simd");

        // Enable verifier only when debug assertions are active
        #[cfg(debug_assertions)]
        {
            let _ = flag_builder.enable("enable_verifier");
        }

        // Build ISA with optimizations
        let isa_builder = cranelift_native::builder()
            .map_err(|e| format!("Failed to create ISA builder: {}", e))?;
        let _isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| format!("Failed to create ISA: {}", e))?;

        // Create JIT builder with symbol resolution
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("Failed to create JIT builder: {}", e))?;

        // Register external symbols (C runtime functions)
        builder.symbol("printf", libc::printf as *const u8);
        builder.symbol("malloc", libc::malloc as *const u8);
        builder.symbol("free", libc::free as *const u8);
        builder.symbol("memcpy", libc::memcpy as *const u8);
        builder.symbol("puts", libc::puts as *const u8);

        // Register our print helper functions
        // These are defined in print_helpers.c and compiled into the binary
        unsafe extern "C" {
            fn print_i64(value: i64);
            fn print_i64_nl(value: i64);
            fn print_f64(value: f64);
            fn print_f64_nl(value: f64);
            fn print_f32(value: f32);
            fn print_f32_nl(value: f32);
            fn print_str(s: *const u8);
            fn print_str_nl(s: *const u8);
            fn print_flush();
            fn file_write_str(path: *const u8, s: *const u8);
            fn file_write_i64(path: *const u8, value: i64);
            fn file_write_f64(path: *const u8, value: f64);
        }

        // unsafe {
        builder.symbol("print_i64", print_i64 as *const u8);
        builder.symbol("print_i64_nl", print_i64_nl as *const u8);
        builder.symbol("print_f64", print_f64 as *const u8);
        builder.symbol("print_f64_nl", print_f64_nl as *const u8);
        builder.symbol("print_f32", print_f32 as *const u8);
        builder.symbol("print_f32_nl", print_f32_nl as *const u8);
        builder.symbol("print_str", print_str as *const u8);
        builder.symbol("print_str_nl", print_str_nl as *const u8);
        builder.symbol("print_flush", print_flush as *const u8);
        builder.symbol("file_write_str", file_write_str as *const u8);
        builder.symbol("file_write_i64", file_write_i64 as *const u8);
        builder.symbol("file_write_f64", file_write_f64 as *const u8);
        // }

        // Register runtime bridge functions for complex value handling
        for (name, ptr) in super::runtime_bridge::get_runtime_symbols() {
            builder.symbol(name, ptr);
        }

        // Register ARC FFI functions for native JIT execution
        use crate::execution::arc_bridge::*;
        builder.symbol("adesh_rt_arc_new", adesh_rt_arc_new as *const u8);
        builder.symbol("adesh_rt_arc_clone", adesh_rt_arc_clone as *const u8);
        builder.symbol("adesh_rt_arc_drop", adesh_rt_arc_drop as *const u8);
        builder.symbol("adesh_rt_weak_new", adesh_rt_weak_new as *const u8);
        builder.symbol("adesh_rt_weak_drop", adesh_rt_weak_drop as *const u8);
        builder.symbol("adesh_rt_arc_get", adesh_rt_arc_get as *const u8);
        builder.symbol("adesh_rt_arc_set", adesh_rt_arc_set as *const u8);
        builder.symbol(
            "adesh_rt_arc_strong_count",
            adesh_rt_arc_strong_count as *const u8,
        );
        builder.symbol(
            "adesh_rt_arc_weak_count",
            adesh_rt_arc_weak_count as *const u8,
        );

        // Create the JIT module
        let module = JITModule::new(builder);

        Ok(NativeJitCompiler {
            module,
            ctx: codegen::Context::new(),
            functions: HashMap::new(),
            string_data: HashMap::new(),
            string_pool: Vec::new(),
            global_vars: HashMap::new(),
            global_types: HashMap::new(),
            format_strings: HashMap::new(),
            function_return_types: HashMap::new(),
        })
    }

    /// Compile a LIR module to native code
    pub fn compile_module(&mut self, lir_module: &LirModule) -> Result<NativeJitContext, String> {
        // Phase 0: Collect and declare all globals
        self.collect_globals(lir_module)?;

        // Phase 0: Collect and declare all strings
        self.collect_strings(lir_module)?;

        // Build function return type map for correct call result typing
        self.function_return_types = lir_module
            .functions
            .iter()
            .map(|f| (f.name.clone(), f.ret_type))
            .collect();

        // Phase 1: Declare all functions
        for func in &lir_module.functions {
            self.declare_function(func)?;
        }

        // Phase 2: Compile all function bodies
        for func in &lir_module.functions {
            if let Err(e) = self.compile_function(func) {
                return Err(format!("Failed to compile function {}: {}", func.name, e));
            }
        }

        // Phase 3: Finalize the module
        self.module.finalize_definitions().unwrap();

        // Register compiled functions with runtime bridge
        for (name, func_id) in &self.functions {
            let ptr = self.module.get_finalized_function(*func_id);
            super::runtime_bridge::register_jit_function(name.clone(), ptr as usize);
        }

        // Create execution context
        let context = NativeJitContext::new(self.functions.clone(), &self.module);

        // eprintln!("[Native JIT] Compilation complete");
        Ok(context)
    }

    /// Collect all strings from the LIR module and declare them as data objects
    fn collect_strings(&mut self, lir_module: &LirModule) -> Result<(), String> {
        // Extract string pool from LIR module
        self.string_pool = lir_module.string_pool.clone();

        // Ensure all ConstString literals are present in the pool
        // (Some LIR producers may not populate string_pool.)
        for func in &lir_module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let LirInst::ConstString(_, s) = inst {
                        if !self.string_pool.iter().any(|existing| existing == s) {
                            self.string_pool.push(s.clone());
                        }
                    }
                }
            }
        }

        // Declare each string as a data object
        for (index, string) in self.string_pool.iter().enumerate() {
            let data_name = format!("__string_{}", index);

            // Create data description for the string
            let mut data_desc = DataDescription::new();
            let mut string_bytes = string.as_bytes().to_vec();
            string_bytes.push(0); // Null terminator for C compatibility
            data_desc.define(string_bytes.into_boxed_slice());
            data_desc.set_align(1); // Byte alignment for strings

            // Declare the data in the module
            let data_id = self
                .module
                .declare_data(&data_name, Linkage::Local, true, false)
                .map_err(|e| format!("Failed to declare string data: {}", e))?;

            // Define the data content
            self.module
                .define_data(data_id, &data_desc)
                .map_err(|e| format!("Failed to define string data: {}", e))?;

            // Store the data ID for later reference
            self.string_data.insert(index, data_id);
        }

        Ok(())
    }

    #[allow(unused)]
    fn get_or_create_string_ptr(
        module: &mut JITModule,
        format_strings: &mut HashMap<String, cranelift_module::DataId>,
        builder: &mut FunctionBuilder,
        s: &str,
    ) -> Result<Value, String> {
        let data_id = if let Some(&id) = format_strings.get(s) {
            id
        } else {
            let mut data_desc = DataDescription::new();
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            data_desc.define(bytes.into_boxed_slice());
            data_desc.set_align(1);

            let data_name = format!("__jit_str_{}", format_strings.len());
            let id = module
                .declare_data(&data_name, Linkage::Local, true, false)
                .map_err(|e| format!("Failed to declare string data: {}", e))?;
            module
                .define_data(id, &data_desc)
                .map_err(|e| format!("Failed to define string data: {}", e))?;

            format_strings.insert(s.to_string(), id);
            id
        };

        let gv = module.declare_data_in_func(data_id, builder.func);
        Ok(builder.ins().global_value(types::I64, gv))
    }

    /// Collect and declare all globals from the LIR module
    fn collect_globals(&mut self, lir_module: &LirModule) -> Result<(), String> {
        self.global_types = lir_module
            .globals
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        for (name, ty) in &lir_module.globals {
            let data_name = format!("__global_{}", name);
            let size = Self::global_type_size(*ty).max(8);
            let mut data_desc = DataDescription::new();
            let init_bytes = vec![0u8; size.max(1)];
            data_desc.define(init_bytes.into_boxed_slice());
            data_desc.set_align(8);

            let data_id = self
                .module
                .declare_data(&data_name, Linkage::Local, true, false)
                .map_err(|e| format!("Failed to declare global data {}: {}", name, e))?;

            self.module
                .define_data(data_id, &data_desc)
                .map_err(|e| format!("Failed to define global data {}: {}", name, e))?;

            self.global_vars.insert(name.clone(), data_id);
        }

        Ok(())
    }

    /// Declare a function signature
    fn declare_function(&mut self, func: &LirFunction) -> Result<(), String> {
        // Create function signature
        let mut sig = self.module.make_signature();

        // Add parameters (all as i64 for now - will be refined)
        for _ in &func.params {
            sig.params.push(AbiParam::new(types::I64));
        }

        // Add return type based on LIR type
        // Note: We always add a return type even if LIR says Void,
        // to handle cases where LIR has Return(Some(val)) with Void return type
        match func.ret_type {
            crate::backends::common::lir::LirType::Void => {
                // Check if function actually returns a value by looking at instructions
                let has_value_return = func.blocks.iter().any(|block| {
                    block
                        .instructions
                        .iter()
                        .any(|inst| matches!(inst, LirInst::Return(Some(_))))
                });

                if has_value_return {
                    // LIR says void but returns value - add i64 return type
                    sig.returns.push(AbiParam::new(types::I64));
                }
                // else: truly void, no return value
            }
            crate::backends::common::lir::LirType::I64
            | crate::backends::common::lir::LirType::I32
            | crate::backends::common::lir::LirType::U64
            | crate::backends::common::lir::LirType::U32
            | crate::backends::common::lir::LirType::U8
            | crate::backends::common::lir::LirType::I8 => {
                sig.returns.push(AbiParam::new(types::I64));
            }
            crate::backends::common::lir::LirType::F64
            | crate::backends::common::lir::LirType::F32 => {
                sig.returns.push(AbiParam::new(types::F64));
            }
            crate::backends::common::lir::LirType::Bool => {
                sig.returns.push(AbiParam::new(types::I8));
            }
            _ => {
                sig.returns.push(AbiParam::new(types::I64));
            }
        }

        // Declare the function
        let linkage = if func.is_exported || func.name == "main" {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let func_id = self
            .module
            .declare_function(&func.name, linkage, &sig)
            .map_err(|e| format!("Failed to declare function {}: {}", func.name, e))?;

        self.functions.insert(func.name.clone(), func_id);

        Ok(())
    }

    /// Compile a single function
    fn compile_function(&mut self, func: &LirFunction) -> Result<(), String> {
        let func_id = self
            .functions
            .get(&func.name)
            .ok_or_else(|| format!("Function {} not declared", func.name))?;

        // Clear the context for a new function
        self.ctx.func.clear();

        // Get the function signature
        self.ctx.func.signature = self
            .module
            .declarations()
            .get_function_decl(*func_id)
            .signature
            .clone();

        // Set function name for debugging
        self.ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

        // Clone functions HashMap for use during compilation
        let functions_clone = self.functions.clone();
        let string_data = self.string_data.clone();
        let string_pool = self.string_pool.clone();
        let global_vars = self.global_vars.clone();
        let global_types = self.global_types.clone();

        // Compile the function body
        {
            let mut builder_ctx = FunctionBuilderContext::new();
            let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut builder_ctx);

            // Track value mappings (LIR ValueId -> Cranelift Value)
            let mut value_map: HashMap<ValueId, Value> = HashMap::new();
            let mut value_types: HashMap<ValueId, AotValueType> = HashMap::new();
            let mut const_strings: HashMap<ValueId, String> = HashMap::new();
            let mut const_bools: HashMap<ValueId, bool> = HashMap::new();
            let mut const_nulls: HashSet<ValueId> = HashSet::new();
            let mut object_properties: HashMap<ValueId, HashMap<String, (ValueId, AotValueType)>> =
                HashMap::new();
            let mut var_object_properties: HashMap<
                String,
                HashMap<String, (ValueId, AotValueType)>,
            > = HashMap::new();

            // Track variable namespace (variable name -> Cranelift Value)
            // This is separate from value_map and handles LoadVar/StoreVar
            let mut var_namespace: HashMap<String, Value> = HashMap::new();
            // Track variable types separately (var_name -> AotValueType)
            let mut var_types: HashMap<String, AotValueType> = HashMap::new();
            let mut var_nulls: HashSet<String> = HashSet::new();

            // Track block mappings (LIR BlockId -> Cranelift Block)
            let mut block_map: HashMap<BlockId, Block> = HashMap::new();

            // Create Cranelift blocks for all LIR blocks
            for lir_block in &func.blocks {
                let block = builder.create_block();
                block_map.insert(lir_block.id, block);
            }

            // Declare Cranelift Variable for each local / global variable name to support SSA variable resolution
            let mut cranelift_vars: HashMap<String, (Variable, Variable)> = HashMap::new();
            let mut next_var_id = 0u32;

            // Collect all possible local variable names in the function (skip globals)
            let mut all_var_names = HashSet::new();
            for param in &func.params {
                if !global_vars.contains_key(&param.0) {
                    all_var_names.insert(param.0.clone());
                }
            }
            for (var_name, _) in &func.var_map {
                if !global_vars.contains_key(var_name) {
                    all_var_names.insert(var_name.clone());
                }
            }
            for block in &func.blocks {
                for inst in &block.instructions {
                    match inst {
                        LirInst::LoadVar(_, name) | LirInst::StoreVar(name, _) => {
                            if !global_vars.contains_key(name) {
                                all_var_names.insert(name.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Declare them in Cranelift JIT builder
            for name in all_var_names {
                let var_i64 = Variable::from_u32(next_var_id);
                let var_f64 = Variable::from_u32(next_var_id + 1);
                next_var_id += 2;

                builder.declare_var(var_i64, types::I64);
                builder.declare_var(var_f64, types::F64);
                cranelift_vars.insert(name, (var_i64, var_f64));
            }

            // Declare Cranelift Variable for each LIR Phi node destination value
            let mut phi_vars: HashMap<ValueId, (Variable, Variable)> = HashMap::new();
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let LirInst::Phi(dst, _) = inst {
                        let var_i64 = Variable::from_u32(next_var_id);
                        let var_f64 = Variable::from_u32(next_var_id + 1);
                        next_var_id += 2;

                        builder.declare_var(var_i64, types::I64);
                        builder.declare_var(var_f64, types::F64);
                        phi_vars.insert(*dst, (var_i64, var_f64));
                    }
                }
            }

            // Process the entry block
            let entry_block = *block_map
                .get(&func.entry_block)
                .ok_or_else(|| "Entry block not found".to_string())?;

            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);

            // Map function parameters to LIR values based on var_map and define their Cranelift variables
            for (i, (param_name, param_type)) in func.params.iter().enumerate() {
                let param_value = builder.block_params(entry_block)[i];
                // Determine AotValueType from LirType
                let aot_type = match param_type {
                    crate::backends::common::lir::LirType::I8 => AotValueType::I8,
                    crate::backends::common::lir::LirType::I16 => AotValueType::I16,
                    crate::backends::common::lir::LirType::I32 => AotValueType::I32,
                    crate::backends::common::lir::LirType::I64 => AotValueType::Int,
                    crate::backends::common::lir::LirType::U8 => AotValueType::U8,
                    crate::backends::common::lir::LirType::U16 => AotValueType::U16,
                    crate::backends::common::lir::LirType::U32 => AotValueType::U32,
                    crate::backends::common::lir::LirType::U64 => AotValueType::U64,
                    crate::backends::common::lir::LirType::F32 => AotValueType::F32,
                    crate::backends::common::lir::LirType::F64 => AotValueType::F64,
                    crate::backends::common::lir::LirType::Bool => AotValueType::Bool,
                    crate::backends::common::lir::LirType::Ptr => AotValueType::String, // Ptr params are typically strings
                    _ => AotValueType::Int,
                };
                // Store in value_map using the variable's mapped value ID from var_map
                if let Some(value_id) = func.get_var(param_name) {
                    value_map.insert(value_id, param_value);
                    value_types.insert(value_id, aot_type.clone());
                }
                // Also store in var_namespace for LoadVar/StoreVar
                var_namespace.insert(param_name.clone(), param_value);
                var_types.insert(param_name.clone(), aot_type.clone());

                // Define initial variable value in Cranelift variables
                if let Some(&(v_i64, v_f64)) = cranelift_vars.get(param_name) {
                    if matches!(aot_type, AotValueType::F64 | AotValueType::F32) {
                        let f64_val = Self::to_f64(&mut builder, param_value);
                        builder.def_var(v_f64, f64_val);
                    } else {
                        let i64_val = Self::to_i64_unsigned(&mut builder, param_value);
                        builder.def_var(v_i64, i64_val);
                    }
                }
            }

            // Compile all blocks
            for lir_block in &func.blocks {
                let block = *block_map
                    .get(&lir_block.id)
                    .ok_or_else(|| format!("Block {} not found", lir_block.id))?;

                // Skip entry block as it's already switched
                if lir_block.id != func.entry_block {
                    builder.switch_to_block(block);
                }

                // Track if we've seen a terminator
                let mut seen_terminator = false;

                // Compile instructions in this block
                for inst in &lir_block.instructions {
                    // Skip instructions after a terminator
                    if seen_terminator {
                        continue;
                    }

                    Self::compile_instruction_static(
                        &mut builder,
                        &mut value_map,
                        &mut value_types,
                        &mut const_strings,
                        &mut const_bools,
                        &mut const_nulls,
                        &mut object_properties,
                        &mut var_object_properties,
                        &mut var_namespace,
                        &mut var_types,
                        &mut var_nulls,
                        &cranelift_vars,
                        &phi_vars,
                        lir_block.id,
                        &block_map,
                        func,
                        inst,
                        &functions_clone,
                        &string_data,
                        &string_pool,
                        &global_vars,
                        &global_types,
                        &mut self.module,
                        &mut self.format_strings,
                        &self.function_return_types,
                    )?;

                    // Check if this was a terminator instruction
                    if matches!(
                        inst,
                        LirInst::Return(_)
                            | LirInst::Jump(_)
                            | LirInst::JumpIf(_, _, _)
                            | LirInst::TailCall(_, _)
                    ) {
                        seen_terminator = true;
                    }
                }

                // Ensure block has a terminator - add default return if missing
                if !seen_terminator {
                    if let Some(ret_param) = builder.func.signature.returns.get(0) {
                        let ty = ret_param.value_type;
                        let zero = if ty == types::F64 {
                            builder.ins().f64const(0.0)
                        } else if ty == types::F32 {
                            builder.ins().f32const(0.0)
                        } else {
                            builder.ins().iconst(ty, 0)
                        };
                        builder.ins().return_(&[zero]);
                    } else {
                        // Void function - add empty return
                        builder.ins().return_(&[]);
                    }
                }
            }

            // Seal all blocks after compilation
            for lir_block in &func.blocks {
                let block = *block_map.get(&lir_block.id).unwrap();
                builder.seal_block(block);
            }

            builder.finalize();
        }

        // Define the function in the JIT module
        let define_result = self.module.define_function(*func_id, &mut self.ctx);

        if let Err(e) = define_result {
            // Print the function for debugging
            if cfg!(debug_assertions) {
                eprintln!("Failed to compile function {}. Cranelift IR:", func.name);
                eprintln!("{}", self.ctx.func);
                eprintln!("Cranelift define_function error (debug): {:?}", e);
            }
            return Err(format!(
                "Failed to define function {}: {} | {:?}",
                func.name, e, e
            ));
        }

        // Clear the context
        self.module.clear_context(&mut self.ctx);

        Ok(())
    }

    /// Compile a single LIR instruction (static to avoid borrow issues)
    fn compile_instruction_static(
        builder: &mut FunctionBuilder,
        value_map: &mut HashMap<ValueId, Value>,
        value_types: &mut HashMap<ValueId, AotValueType>,
        const_strings: &mut HashMap<ValueId, String>,
        const_bools: &mut HashMap<ValueId, bool>,
        const_nulls: &mut HashSet<ValueId>,
        object_properties: &mut HashMap<ValueId, HashMap<String, (ValueId, AotValueType)>>,
        var_object_properties: &mut HashMap<String, HashMap<String, (ValueId, AotValueType)>>,
        var_namespace: &mut HashMap<String, Value>,
        var_types: &mut HashMap<String, AotValueType>,
        var_nulls: &mut HashSet<String>,
        cranelift_vars: &HashMap<String, (Variable, Variable)>,
        phi_vars: &HashMap<ValueId, (Variable, Variable)>,
        current_block_id: BlockId,
        block_map: &HashMap<BlockId, Block>,
        func: &LirFunction,
        inst: &LirInst,
        functions: &HashMap<String, cranelift_module::FuncId>,
        string_data: &HashMap<usize, cranelift_module::DataId>,
        string_pool: &Vec<String>,
        global_vars: &HashMap<String, cranelift_module::DataId>,
        global_types: &HashMap<String, crate::backends::common::lir::LirType>,
        module: &mut JITModule,
        format_strings: &mut HashMap<String, cranelift_module::DataId>,
        function_return_types: &HashMap<String, crate::backends::common::lir::LirType>,
    ) -> Result<(), String> {
        // Try arithmetic instructions
        if arithmetic::lower_arithmetic_instruction(builder, value_map, value_types, inst)? {
            return Ok(());
        }

        // Try comparison instructions (doesn't need value_types)
        // Special case: optimize/handle string equality using runtime bridge (Handle/Ptr vs Handle/Ptr)
        if let LirInst::CmpEqI64(dst, a, b) | LirInst::CmpNeI64(dst, a, b) = inst {
            let type_a = value_types.get(a).cloned().unwrap_or(AotValueType::Unknown);
            let type_b = value_types.get(b).cloned().unwrap_or(AotValueType::Unknown);

            let is_str_type = |t| {
                matches!(
                    t,
                    AotValueType::String | AotValueType::Ptr | AotValueType::Handle
                )
            };

            if is_str_type(type_a) || is_str_type(type_b) {
                if let (Some(&va), Some(&vb)) = (value_map.get(a), value_map.get(b)) {
                    let res = Self::call_runtime_fn_2(builder, module, "jit_str_eq", va, vb)?;
                    let final_res = if matches!(inst, LirInst::CmpEqI64(_, _, _)) {
                        // Eq: returns 1 if eq, 0 if ne. Convert to bool (b1) by checking != 0
                        builder.ins().icmp_imm(IntCC::NotEqual, res, 0)
                    } else {
                        // Ne: returns 1 if eq, 0 if ne. Result is true if ne (res == 0)
                        builder.ins().icmp_imm(IntCC::Equal, res, 0)
                    };
                    value_map.insert(*dst, final_res);
                    // value_types.insert(*dst, AotValueType::Bool);
                    return Ok(());
                }
            }
        }

        if comparisons::lower_comparison_instruction(builder, value_map, value_types, inst)? {
            return Ok(());
        }

        // Try conversion instructions
        if conversions::lower_conversion_instruction(builder, value_map, inst)? {
            match inst {
                LirInst::I64ToF64(dst, _) => {
                    value_types.insert(*dst, AotValueType::F64);
                }
                LirInst::F64ToI64(dst, _) => {
                    value_types.insert(*dst, AotValueType::I64);
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle basic constant instructions manually for now
        match inst {
            LirInst::ConstI64(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I64);
                return Ok(());
            }
            LirInst::ConstI32(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I32);
                return Ok(());
            }
            LirInst::ConstI16(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I16);
                return Ok(());
            }
            LirInst::ConstI8(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I8);
                return Ok(());
            }
            LirInst::ConstU64(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U64);
                return Ok(());
            }
            LirInst::ConstU32(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U32);
                return Ok(());
            }
            LirInst::ConstU16(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U16);
                return Ok(());
            }
            LirInst::ConstU8(dst, val) => {
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U8);
                return Ok(());
            }
            LirInst::ConstF64(dst, val) => {
                let v = builder.ins().f64const(*val);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::F64);
                return Ok(());
            }
            LirInst::ConstF32(dst, val) => {
                let v = builder.ins().f32const(*val);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::F32);
                return Ok(());
            }
            LirInst::ConstBool(dst, val) => {
                let v = builder.ins().iconst(types::I8, if *val { 1 } else { 0 });
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::Bool);
                const_bools.insert(*dst, *val);
                return Ok(());
            }
            LirInst::ConstNull(dst) => {
                // Null pointer - use 0
                let v = builder.ins().iconst(types::I64, 0);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::Ptr);
                const_nulls.insert(*dst);
                return Ok(());
            }
            _ => {}
        }

        // Handle control flow instructions
        match inst {
            LirInst::Return(Some(value_id)) => {
                let ret_val = value_map
                    .get(value_id)
                    .copied()
                    .ok_or_else(|| format!("Return value {} not found", value_id))?;
                if let Some(ret_abi) = builder.func.signature.returns.get(0) {
                    let expected_ty = ret_abi.value_type;
                    let actual_ty = builder.func.dfg.value_type(ret_val);
                    let final_val = if actual_ty == expected_ty {
                        ret_val
                    } else if expected_ty == types::I64 {
                        Self::to_i64_signed(builder, ret_val)
                    } else if expected_ty == types::F64 {
                        Self::to_f64(builder, ret_val)
                    } else if expected_ty == types::I8 {
                        let i64_val = Self::to_i64_signed(builder, ret_val);
                        builder.ins().ireduce(types::I8, i64_val)
                    } else {
                        ret_val
                    };
                    builder.ins().return_(&[final_val]);
                } else {
                    builder.ins().return_(&[]);
                }
                Ok(())
            }
            LirInst::Return(None) => {
                // Check if function signature expects a return value
                if let Some(ret_param) = builder.func.signature.returns.get(0) {
                    let ty = ret_param.value_type;
                    let zero = if ty == types::F64 {
                        builder.ins().f64const(0.0)
                    } else if ty == types::F32 {
                        builder.ins().f32const(0.0)
                    } else {
                        builder.ins().iconst(ty, 0)
                    };
                    builder.ins().return_(&[zero]);
                } else {
                    builder.ins().return_(&[]);
                }
                Ok(())
            }
            LirInst::Jump(target_block) => {
                let target = block_map
                    .get(target_block)
                    .copied()
                    .ok_or_else(|| format!("Jump target block {} not found", target_block))?;
                Self::define_phi_incoming_vars(
                    builder,
                    value_map,
                    phi_vars,
                    func,
                    current_block_id,
                    *target_block,
                );
                builder.ins().jump(target, &[]);
                Ok(())
            }
            LirInst::JumpIf(cond_id, true_block, false_block) => {
                let cond = value_map
                    .get(cond_id)
                    .copied()
                    .ok_or_else(|| format!("Condition value {} not found", cond_id))?;
                let cond_ty = builder.func.dfg.value_type(cond);
                let cond_b1 = if cond_ty.is_int() {
                    builder.ins().icmp_imm(IntCC::NotEqual, cond, 0)
                } else {
                    // Fallback: treat non-bool values as truthy if non-zero.
                    builder.ins().icmp_imm(IntCC::NotEqual, cond, 0)
                };
                let then_block = block_map
                    .get(true_block)
                    .copied()
                    .ok_or_else(|| format!("True block {} not found", true_block))?;
                let else_block = block_map
                    .get(false_block)
                    .copied()
                    .ok_or_else(|| format!("False block {} not found", false_block))?;
                Self::define_phi_incoming_vars(
                    builder,
                    value_map,
                    phi_vars,
                    func,
                    current_block_id,
                    *true_block,
                );
                Self::define_phi_incoming_vars(
                    builder,
                    value_map,
                    phi_vars,
                    func,
                    current_block_id,
                    *false_block,
                );
                builder
                    .ins()
                    .brif(cond_b1, then_block, &[], else_block, &[]);
                Ok(())
            }
            LirInst::Call(dst, func_name, args) => {
                // Handle function-to-function calls
                if let Some(&target_func_id) = functions.get(func_name) {
                    // Get argument values
                    let arg_vals: Vec<Value> = args
                        .iter()
                        .filter_map(|a| {
                            value_map.get(a).copied().map(|v| {
                                match value_types.get(a).cloned().unwrap_or(AotValueType::Unknown) {
                                    AotValueType::Bool
                                    | AotValueType::U8
                                    | AotValueType::U16
                                    | AotValueType::U32 => Self::to_i64_unsigned(builder, v),
                                    AotValueType::I8 | AotValueType::I16 | AotValueType::I32 => {
                                        Self::to_i64_signed(builder, v)
                                    }
                                    _ => {
                                        let ty = builder.func.dfg.value_type(v);
                                        if ty.is_int() && ty != types::I64 {
                                            Self::to_i64_unsigned(builder, v)
                                        } else {
                                            v
                                        }
                                    }
                                }
                            })
                        })
                        .collect();

                    // Declare function reference in current function
                    let func_ref = module.declare_func_in_func(target_func_id, builder.func);

                    // Call the function
                    let call_inst = builder.ins().call(func_ref, &arg_vals);

                    // Get the result
                    let results = builder.inst_results(call_inst);
                    if !results.is_empty() {
                        value_map.insert(*dst, results[0]);
                        // Track actual return type from the LIR function signature
                        let ret_type = function_return_types
                            .get(func_name)
                            .cloned()
                            .unwrap_or(crate::backends::common::lir::LirType::I64);
                        let value_type = match ret_type {
                            crate::backends::common::lir::LirType::F32 => AotValueType::F32,
                            crate::backends::common::lir::LirType::F64 => AotValueType::F64,
                            crate::backends::common::lir::LirType::Bool => AotValueType::Bool,
                            crate::backends::common::lir::LirType::Ptr => AotValueType::String,
                            crate::backends::common::lir::LirType::Void => AotValueType::Int,
                            _ => AotValueType::Int,
                        };
                        value_types.insert(*dst, value_type);
                    } else {
                        // Function returned void, store 0
                        let zero = builder.ins().iconst(types::I64, 0);
                        value_map.insert(*dst, zero);
                    }
                } else {
                    // Unknown function - return 0
                    let zero = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, zero);
                }
                Ok(())
            }
            LirInst::CallBuiltin(dst, builtin_name, args) => {
                // Handle different builtins
                match builtin_name.as_str() {
                    "make_object" => {
                        // Create object using runtime bridge
                        let mut handle =
                            Self::call_runtime_fn_0(builder, module, "jit_make_object")?;

                        // Track properties for compile-time print option detection
                        let mut new_props = HashMap::new();

                        // Process key-value pairs from arguments
                        for chunk in args.chunks(2) {
                            if chunk.len() == 2 {
                                let key_id = chunk[0];
                                let val_id = chunk[1];

                                // Track for compile-time optimization
                                if let Some(field_name) = const_strings.get(&key_id).cloned() {
                                    let val_type = value_types
                                        .get(&val_id)
                                        .cloned()
                                        .unwrap_or(AotValueType::Unknown);
                                    new_props
                                        .insert(field_name.clone(), (val_id, val_type.clone()));

                                    // Call runtime to set the field
                                    if let (Some(&field_ptr), Some(&val)) =
                                        (value_map.get(&key_id), value_map.get(&val_id))
                                    {
                                        let const_bool = const_bools.get(&val_id).copied();
                                        let is_const_string = const_strings.contains_key(&val_id);
                                        handle = match val_type {
                                            _ if is_const_string => Self::call_runtime_fn_3(
                                                builder,
                                                module,
                                                "jit_object_set_str",
                                                handle,
                                                field_ptr,
                                                val,
                                            )?,
                                            _ if const_bool.is_some() => {
                                                let bool_as_i64 = builder.ins().iconst(
                                                    types::I64,
                                                    if const_bool.unwrap_or(false) { 1 } else { 0 },
                                                );
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_bool",
                                                    handle,
                                                    field_ptr,
                                                    bool_as_i64,
                                                )?
                                            }
                                            AotValueType::String => Self::call_runtime_fn_3(
                                                builder,
                                                module,
                                                "jit_object_set_str",
                                                handle,
                                                field_ptr,
                                                val,
                                            )?,
                                            AotValueType::F64
                                            | AotValueType::F32
                                            | AotValueType::Float => Self::call_runtime_fn_3_f64(
                                                builder,
                                                module,
                                                "jit_object_set_float",
                                                handle,
                                                field_ptr,
                                                val,
                                            )?,
                                            AotValueType::Bool => {
                                                let bool_as_i64 =
                                                    Self::to_i64_unsigned(builder, val);
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_bool",
                                                    handle,
                                                    field_ptr,
                                                    bool_as_i64,
                                                )?
                                            }
                                            AotValueType::Handle => Self::call_runtime_fn_3(
                                                builder,
                                                module,
                                                "jit_object_set_handle",
                                                handle,
                                                field_ptr,
                                                val,
                                            )?,
                                            _ => {
                                                // Default: treat as integer
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_int",
                                                    handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            }
                                        };
                                    }
                                }
                            }
                        }

                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                        object_properties.insert(*dst, new_props);
                    }
                    "set_field" => {
                        // set_field(obj, field_name, value) -> new object
                        if args.len() >= 3 {
                            let obj_id = args[0];
                            let field_id = args[1];
                            let val_id = args[2];

                            // Track properties for compile-time print options
                            let mut new_props =
                                object_properties.get(&obj_id).cloned().unwrap_or_default();

                            if let Some(field_name) = const_strings.get(&field_id).cloned() {
                                let val_type = value_types
                                    .get(&val_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Unknown);
                                new_props.insert(field_name.clone(), (val_id, val_type.clone()));

                                // Call runtime to actually set the field
                                if let (Some(&obj_handle), Some(&field_ptr)) =
                                    (value_map.get(&obj_id), value_map.get(&field_id))
                                {
                                    let const_bool = const_bools.get(&val_id).copied();
                                    let is_const_string = const_strings.contains_key(&val_id);
                                    let new_handle = match val_type {
                                        _ if is_const_string => {
                                            if let Some(&val) = value_map.get(&val_id) {
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_str",
                                                    obj_handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                        _ if const_bool.is_some() => {
                                            let bool_as_i64 = builder.ins().iconst(
                                                types::I64,
                                                if const_bool.unwrap_or(false) { 1 } else { 0 },
                                            );
                                            Self::call_runtime_fn_3(
                                                builder,
                                                module,
                                                "jit_object_set_bool",
                                                obj_handle,
                                                field_ptr,
                                                bool_as_i64,
                                            )?
                                        }
                                        AotValueType::String => {
                                            if let Some(&val) = value_map.get(&val_id) {
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_str",
                                                    obj_handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                        AotValueType::F64
                                        | AotValueType::F32
                                        | AotValueType::Float => {
                                            if let Some(&val) = value_map.get(&val_id) {
                                                Self::call_runtime_fn_3_f64(
                                                    builder,
                                                    module,
                                                    "jit_object_set_float",
                                                    obj_handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                        AotValueType::Bool => {
                                            if let Some(&val) = value_map.get(&val_id) {
                                                let bool_as_i64 =
                                                    Self::to_i64_unsigned(builder, val);
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_bool",
                                                    obj_handle,
                                                    field_ptr,
                                                    bool_as_i64,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                        AotValueType::Handle => {
                                            if let Some(&val) = value_map.get(&val_id) {
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_handle",
                                                    obj_handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                        _ => {
                                            // Default: treat as integer
                                            if let Some(&val) = value_map.get(&val_id) {
                                                Self::call_runtime_fn_3(
                                                    builder,
                                                    module,
                                                    "jit_object_set_int",
                                                    obj_handle,
                                                    field_ptr,
                                                    val,
                                                )?
                                            } else {
                                                obj_handle
                                            }
                                        }
                                    };
                                    value_map.insert(*dst, new_handle);
                                    value_types.insert(*dst, AotValueType::Handle);
                                } else {
                                    let v = builder.ins().iconst(types::I64, 0);
                                    value_map.insert(*dst, v);
                                    value_types.insert(*dst, AotValueType::Handle);
                                }
                            } else {
                                let v = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, v);
                                value_types.insert(*dst, AotValueType::Handle);
                            }

                            object_properties.insert(*dst, new_props);
                        } else {
                            let v = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, v);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "get_field" => {
                        // get_field(obj, field_name) -> value handle
                        if args.len() >= 2 {
                            let obj_id = args[0];
                            let field_id = args[1];

                            if let Some(field_name) = const_strings.get(&field_id).cloned() {
                                if let Some(obj_props) = object_properties.get(&obj_id) {
                                    if let Some((stored_val_id, stored_ty)) =
                                        obj_props.get(&field_name)
                                    {
                                        if let Some(stored_val) =
                                            value_map.get(stored_val_id).copied()
                                        {
                                            value_map.insert(*dst, stored_val);
                                            value_types.insert(*dst, stored_ty.clone());
                                            return Ok(());
                                        }
                                    }
                                }
                            }

                            if let (Some(&obj_handle), Some(&field_ptr)) =
                                (value_map.get(&obj_id), value_map.get(&field_id))
                            {
                                // In-register direct memory load optimization for non-null pointer handles
                                let value_handle = Self::call_runtime_fn_2(
                                    builder,
                                    module,
                                    "jit_get_field",
                                    obj_handle,
                                    field_ptr,
                                )?;
                                value_map.insert(*dst, value_handle);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                                value_types.insert(*dst, AotValueType::Handle);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "hasKey" => {
                        // hasKey(obj, key) -> bool
                        if args.len() >= 2 {
                            if let (Some(&obj_handle), Some(&field_handle)) =
                                (value_map.get(&args[0]), value_map.get(&args[1]))
                            {
                                let result = Self::call_runtime_fn_2(
                                    builder,
                                    module,
                                    "jit_object_has_key",
                                    obj_handle,
                                    field_handle,
                                )?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Bool);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                                value_types.insert(*dst, AotValueType::Bool);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Bool);
                        }
                    }
                    "make_array" | "make_array_spread" => {
                        // Create array using runtime bridge
                        let handle = Self::call_runtime_fn_0(builder, module, "jit_make_array")?;

                        // Push each element
                        let mut current_handle = handle;
                        for arg_id in args.iter() {
                            let val_type = value_types
                                .get(arg_id)
                                .cloned()
                                .unwrap_or(AotValueType::Int);
                            let val = if let Some(s) = const_strings.get(arg_id) {
                                let ptr = Self::get_or_create_string_ptr(
                                    module,
                                    format_strings,
                                    builder,
                                    s,
                                )?;
                                Some(ptr)
                            } else {
                                value_map.get(arg_id).copied()
                            };
                            if let Some(val) = val {
                                current_handle = match val_type {
                                    AotValueType::String => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_str",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::F64 | AotValueType::Float => {
                                        Self::call_runtime_fn_2_f64(
                                            builder,
                                            module,
                                            "jit_array_push_float",
                                            current_handle,
                                            val,
                                        )?
                                    }
                                    AotValueType::F32 => {
                                        let promoted = builder.ins().fpromote(types::F64, val);
                                        Self::call_runtime_fn_2_f64(
                                            builder,
                                            module,
                                            "jit_array_push_float",
                                            current_handle,
                                            promoted,
                                        )?
                                    }
                                    AotValueType::Bool => {
                                        let bool_as_i64 = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_bool",
                                            current_handle,
                                            bool_as_i64,
                                        )?
                                    }
                                    AotValueType::Char => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_char",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U8 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_u8",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U16 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_u16",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U32 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_u32",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U64 => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_u64",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::I8 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_i8",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I16 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_i16",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I32 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_i32",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I64 => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_i64",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::Handle => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_handle",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::Ptr => {
                                        // Null pointer
                                        Self::call_runtime_fn_1(
                                            builder,
                                            module,
                                            "jit_array_push_null",
                                            current_handle,
                                        )?
                                    }
                                    _ => {
                                        // Default: treat as integer
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_int",
                                            current_handle,
                                            val,
                                        )?
                                    }
                                };
                            }
                        }

                        value_map.insert(*dst, current_handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "make_tuple" => {
                        // Create tuple using runtime bridge
                        let handle = Self::call_runtime_fn_0(builder, module, "jit_make_tuple")?;

                        // Push each element
                        let mut current_handle = handle;
                        for arg_id in args.iter() {
                            if let Some(&val) = value_map.get(arg_id) {
                                let val_type = value_types
                                    .get(arg_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Int);
                                current_handle = match val_type {
                                    AotValueType::String => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_tuple_push_str",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::U8 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_u8",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U16 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_u16",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U32 => {
                                        let extended = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_u32",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::U64 => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_tuple_push_u64",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::I8 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_i8",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I16 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_i16",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I32 => {
                                        let extended = Self::to_i64_signed(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_i32",
                                            current_handle,
                                            extended,
                                        )?
                                    }
                                    AotValueType::I64 => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_tuple_push_i64",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::F32 => {
                                        let f32_val =
                                            if builder.func.dfg.value_type(val) == types::F64 {
                                                builder.ins().fdemote(types::F32, val)
                                            } else {
                                                val
                                            };
                                        Self::call_runtime_fn_2_f32(
                                            builder,
                                            module,
                                            "jit_tuple_push_f32",
                                            current_handle,
                                            f32_val,
                                        )?
                                    }
                                    AotValueType::F64 | AotValueType::Float => {
                                        let f64_val = Self::to_f64(builder, val);
                                        Self::call_runtime_fn_2_f64(
                                            builder,
                                            module,
                                            "jit_tuple_push_float",
                                            current_handle,
                                            f64_val,
                                        )?
                                    }
                                    AotValueType::Bool => {
                                        let bool_as_i64 = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_bool",
                                            current_handle,
                                            bool_as_i64,
                                        )?
                                    }
                                    AotValueType::Handle => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_tuple_push_handle",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::Ptr => {
                                        // Null pointer
                                        Self::call_runtime_fn_1(
                                            builder,
                                            module,
                                            "jit_tuple_push_null",
                                            current_handle,
                                        )?
                                    }
                                    _ => {
                                        // Default: treat as integer
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_tuple_push_int",
                                            current_handle,
                                            val,
                                        )?
                                    }
                                };
                            }
                        }

                        value_map.insert(*dst, current_handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "make_set" | "makeSet" => {
                        // Sets are represented as arrays in the runtime (no native Set in RuntimeValue)
                        // Create array using runtime bridge
                        let handle = Self::call_runtime_fn_0(builder, module, "jit_make_array")?;

                        // Push each element
                        let mut current_handle = handle;
                        for arg_id in args.iter() {
                            if let Some(&val) = value_map.get(arg_id) {
                                let val_type = value_types
                                    .get(arg_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Int);
                                current_handle = match val_type {
                                    AotValueType::String => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_str",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::F64 | AotValueType::F32 | AotValueType::Float => {
                                        Self::call_runtime_fn_2_f64(
                                            builder,
                                            module,
                                            "jit_array_push_float",
                                            current_handle,
                                            val,
                                        )?
                                    }
                                    AotValueType::Bool => {
                                        let bool_as_i64 = Self::to_i64_unsigned(builder, val);
                                        Self::call_runtime_fn_2(
                                            builder,
                                            module,
                                            "jit_array_push_bool",
                                            current_handle,
                                            bool_as_i64,
                                        )?
                                    }
                                    AotValueType::Handle => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_handle",
                                        current_handle,
                                        val,
                                    )?,
                                    AotValueType::Ptr => Self::call_runtime_fn_1(
                                        builder,
                                        module,
                                        "jit_array_push_null",
                                        current_handle,
                                    )?,
                                    _ => Self::call_runtime_fn_2(
                                        builder,
                                        module,
                                        "jit_array_push_int",
                                        current_handle,
                                        val,
                                    )?,
                                };
                            }
                        }

                        current_handle = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "jit_make_set_from_array",
                            current_handle,
                        )?;

                        value_map.insert(*dst, current_handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "__call_method" => {
                        // args[0] = obj, args[1] = method_name, args[2..] = method args
                        if args.len() < 2 {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Handle);
                        } else {
                            let obj = value_map
                                .get(&args[0])
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let method_name = value_map
                                .get(&args[1])
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

                            let a0 = args
                                .get(2)
                                .and_then(|id| value_map.get(id))
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let a1 = args
                                .get(3)
                                .and_then(|id| value_map.get(id))
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let a2 = args
                                .get(4)
                                .and_then(|id| value_map.get(id))
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let a3 = args
                                .get(5)
                                .and_then(|id| value_map.get(id))
                                .copied()
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let argc = builder
                                .ins()
                                .iconst(types::I64, (args.len().saturating_sub(2)) as i64);

                            let res = Self::call_runtime_fn_7(
                                builder,
                                module,
                                "jit_call_method",
                                obj,
                                method_name,
                                a0,
                                a1,
                                a2,
                                a3,
                                argc,
                            )?;
                            value_map.insert(*dst, res);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "__make_class_object" => {
                        let result =
                            Self::call_runtime_fn_0(builder, module, "jit_make_class_object")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "sizeof" => {
                        // sizeof(value) -> size in bytes
                        if !args.is_empty() {
                            let arg_id = args[0];
                            if let Some(&AotValueType::Handle) = value_types.get(&arg_id) {
                                if let Some(&handle_val) = value_map.get(&arg_id) {
                                    let res = Self::call_runtime_fn_1(
                                        builder,
                                        module,
                                        "jit_sizeof_handle",
                                        handle_val,
                                    )?;
                                    value_map.insert(*dst, res);
                                    value_types.insert(*dst, AotValueType::Int);
                                } else {
                                    let v = builder.ins().iconst(types::I64, 8);
                                    value_map.insert(*dst, v);
                                    value_types.insert(*dst, AotValueType::Int);
                                }
                            } else {
                                let size = match value_types.get(&arg_id) {
                                    Some(&AotValueType::U8)
                                    | Some(&AotValueType::I8)
                                    | Some(&AotValueType::Bool) => 1,
                                    Some(&AotValueType::U16) | Some(&AotValueType::I16) => 2,
                                    Some(&AotValueType::U32)
                                    | Some(&AotValueType::I32)
                                    | Some(&AotValueType::Char)
                                    | Some(&AotValueType::F32) => 4,
                                    Some(&AotValueType::U64)
                                    | Some(&AotValueType::I64)
                                    | Some(&AotValueType::F64)
                                    | Some(&AotValueType::Int)
                                    | Some(&AotValueType::Float)
                                    | Some(&AotValueType::Ptr)
                                    | Some(&AotValueType::String) => 8,
                                    Some(&AotValueType::Handle) => 8,
                                    Some(&AotValueType::U128) | Some(&AotValueType::I128) => 16,
                                    Some(AotValueType::Array(elem_type, len)) => {
                                        let elem_size = match **elem_type {
                                            AotValueType::U8
                                            | AotValueType::I8
                                            | AotValueType::Bool => 1,
                                            AotValueType::U16 | AotValueType::I16 => 2,
                                            AotValueType::U32
                                            | AotValueType::I32
                                            | AotValueType::F32 => 4,
                                            AotValueType::U64
                                            | AotValueType::I64
                                            | AotValueType::F64
                                            | AotValueType::Int
                                            | AotValueType::Float
                                            | AotValueType::Ptr
                                            | AotValueType::String
                                            | AotValueType::Handle => 8,
                                            AotValueType::U128 | AotValueType::I128 => 16,
                                            _ => 8,
                                        };
                                        (elem_size * (*len as i64)) + elem_type.metadata_size()
                                    }
                                    Some(AotValueType::RawArray(elem_type, len)) => {
                                        let elem_size = match **elem_type {
                                            AotValueType::U8
                                            | AotValueType::I8
                                            | AotValueType::Bool => 1,
                                            AotValueType::U16 | AotValueType::I16 => 2,
                                            AotValueType::U32
                                            | AotValueType::I32
                                            | AotValueType::F32 => 4,
                                            AotValueType::U64
                                            | AotValueType::I64
                                            | AotValueType::F64
                                            | AotValueType::Int
                                            | AotValueType::Float
                                            | AotValueType::Ptr
                                            | AotValueType::String
                                            | AotValueType::Handle => 8,
                                            AotValueType::U128 | AotValueType::I128 => 16,
                                            _ => 8,
                                        };
                                        elem_size * (*len as i64)
                                    }
                                    Some(AotValueType::Set(elem_type, len)) => {
                                        let elem_size = match **elem_type {
                                            AotValueType::U8
                                            | AotValueType::I8
                                            | AotValueType::Bool => 1,
                                            AotValueType::U16 | AotValueType::I16 => 2,
                                            AotValueType::U32
                                            | AotValueType::I32
                                            | AotValueType::F32 => 4,
                                            AotValueType::U64
                                            | AotValueType::I64
                                            | AotValueType::F64
                                            | AotValueType::Int
                                            | AotValueType::Float
                                            | AotValueType::Ptr
                                            | AotValueType::String
                                            | AotValueType::Handle => 8,
                                            AotValueType::U128 | AotValueType::I128 => 16,
                                            _ => 8,
                                        };
                                        (elem_size * (*len as i64)) + elem_type.metadata_size()
                                    }
                                    Some(AotValueType::Tuple(elem_types)) => {
                                        8 * (elem_types.len() as i64)
                                    }
                                    Some(&AotValueType::Unknown) | None => 0,
                                };
                                let v = builder.ins().iconst(types::I64, size);
                                value_map.insert(*dst, v);
                                value_types.insert(*dst, AotValueType::Int);
                            }
                        } else {
                            let v = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, v);
                            value_types.insert(*dst, AotValueType::Int);
                        }
                    }
                    "__set_class_name" => {
                        if args.len() >= 2 {
                            if let (Some(&cls), Some(&name)) =
                                (value_map.get(&args[0]), value_map.get(&args[1]))
                            {
                                let result = Self::call_runtime_fn_2(
                                    builder,
                                    module,
                                    "jit_set_class_name",
                                    cls,
                                    name,
                                )?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                    }

                    "__new_class" => {
                        if args.len() >= 1 {
                            if let Some(&cls) = value_map.get(&args[0]) {
                                let result =
                                    Self::call_runtime_fn_1(builder, module, "jit_new_class", cls)?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                    }
                    "type" | "typeof" => {
                        // Return a type name string pointer similar to interpreter/AOT behavior.
                        if !args.is_empty() {
                            let arg_id = args[0];

                            if let Some(AotValueType::Handle) = value_types.get(&arg_id) {
                                if let Some(&h) = value_map.get(&arg_id) {
                                    let ptr =
                                        Self::call_runtime_fn_1(builder, module, "jit_typeof", h)?;
                                    value_map.insert(*dst, ptr);
                                    value_types.insert(*dst, AotValueType::String);
                                    return Ok(());
                                }
                            }

                            let type_name = if const_nulls.contains(&arg_id) {
                                "null".to_string()
                            } else if object_properties.contains_key(&arg_id) {
                                "object".to_string()
                            } else {
                                match value_types.get(&arg_id) {
                                    Some(AotValueType::Array(elem_type, _)) => {
                                        format!("[{}]", elem_type.element_type_name())
                                    }
                                    Some(AotValueType::RawArray(elem_type, _)) => {
                                        format!("[{};raw]", elem_type.element_type_name())
                                    }
                                    Some(AotValueType::Set(elem_type, _)) => {
                                        format!("{{{}}}", elem_type.element_type_name())
                                    }
                                    Some(AotValueType::Tuple(_)) => "tuple".to_string(),
                                    Some(AotValueType::Int) | Some(AotValueType::Float) => {
                                        "number".to_string()
                                    }
                                    Some(AotValueType::Bool) => "boolean".to_string(),
                                    Some(AotValueType::Char) => "char".to_string(),
                                    Some(AotValueType::String) => "string".to_string(),
                                    Some(AotValueType::Ptr) => "pointer".to_string(),
                                    Some(AotValueType::Handle) => "object".to_string(),
                                    Some(AotValueType::U8) => "u8".to_string(),
                                    Some(AotValueType::U16) => "u16".to_string(),
                                    Some(AotValueType::U32) => "u32".to_string(),
                                    Some(AotValueType::U64) => "u64".to_string(),
                                    Some(AotValueType::U128) => "u128".to_string(),
                                    Some(AotValueType::I8) => "i8".to_string(),
                                    Some(AotValueType::I16) => "i16".to_string(),
                                    Some(AotValueType::I32) => "i32".to_string(),
                                    Some(AotValueType::I64) => "i64".to_string(),
                                    Some(AotValueType::I128) => "i128".to_string(),
                                    Some(AotValueType::F32) => "f32".to_string(),
                                    Some(AotValueType::F64) => "f64".to_string(),
                                    Some(AotValueType::Unknown) | None => "unknown".to_string(),
                                }
                            };

                            let ptr = Self::get_or_create_string_ptr(
                                module,
                                format_strings,
                                builder,
                                &type_name,
                            )?;
                            value_map.insert(*dst, ptr);
                            value_types.insert(*dst, AotValueType::String);
                        } else {
                            let ptr = Self::get_or_create_string_ptr(
                                module,
                                format_strings,
                                builder,
                                "unknown",
                            )?;
                            value_map.insert(*dst, ptr);
                            value_types.insert(*dst, AotValueType::String);
                        }
                    }
                    "print" | "println" => {
                        // Delegate all print behavior to the unified compiler path to keep
                        // option parsing and formatting consistent with interpreter semantics.
                        Self::compile_print_builtin(
                            builder,
                            module,
                            format_strings,
                            args,
                            value_map,
                            value_types,
                            const_strings,
                            const_bools,
                            const_nulls,
                            object_properties,
                            builtin_name == "println",
                        )?;

                        let zero = builder.ins().iconst(types::I64, 0);
                        value_map.insert(*dst, zero);
                        value_types.insert(*dst, AotValueType::Int);
                    }
                    "__has_exception" => {
                        let result = Self::call_runtime_fn_0(builder, module, "jit_has_exception")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "__get_exception" => {
                        let result = Self::call_runtime_fn_0(builder, module, "jit_get_exception")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "__clear_exception" => {
                        let result =
                            Self::call_runtime_fn_0(builder, module, "jit_clear_exception")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "__throw" => {
                        let val = args
                            .first()
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let result = Self::call_runtime_fn_1(builder, module, "jit_throw", val)?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "clock" => {
                        let mut sig = module.make_signature();
                        sig.returns.push(AbiParam::new(types::F64));
                        let func_id = module
                            .declare_function("jit_clock", Linkage::Import, &sig)
                            .map_err(|e| format!("Failed to declare jit_clock: {}", e))?;
                        let local_func = module.declare_func_in_func(func_id, &mut builder.func);
                        let call = builder.ins().call(local_func, &[]);
                        let result = builder.inst_results(call)[0];
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::F64);
                    }
                    "char" => {
                        // char(x): produce a runtime Char handle.
                        let handle = if let Some(&arg_id) = args.first() {
                            if let Some(s) = const_strings.get(&arg_id) {
                                let cp = s.chars().next().unwrap_or('\0') as i64;
                                let cp_val = builder.ins().iconst(types::I64, cp);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_char", cp_val)?
                            } else if let Some(&arg_val) = value_map.get(&arg_id) {
                                let arg_ty = value_types
                                    .get(&arg_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Unknown);
                                match arg_ty {
                                    AotValueType::String | AotValueType::Ptr => {
                                        Self::call_runtime_fn_1(
                                            builder,
                                            module,
                                            "jit_char_from_str",
                                            arg_val,
                                        )?
                                    }
                                    _ => Self::call_runtime_fn_1(
                                        builder,
                                        module,
                                        "jit_wrap_char",
                                        arg_val,
                                    )?,
                                }
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_char", zero)?
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_char", zero)?
                        };

                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    // Type conversion builtins - just pass through the value
                    "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64"
                    | "u128" | "f32" | "f64" | "bool" => {
                        // Type conversion/cast: just use the input value
                        if !args.is_empty() {
                            if let Some(&arg_val) = value_map.get(&args[0]) {
                                value_map.insert(*dst, arg_val);
                                // Update type based on conversion
                                let new_type = match builtin_name.as_str() {
                                    "i64" | "i32" | "i16" | "i8" => AotValueType::I64,
                                    "u64" | "u32" | "u16" | "u8" => AotValueType::U64,
                                    "f64" => AotValueType::F64,
                                    "f32" => AotValueType::F32,
                                    "bool" => AotValueType::I64, // bool as i64
                                    _ => AotValueType::Int,
                                };
                                value_types.insert(*dst, new_type);
                            } else {
                                // Argument not found, use 0
                                let result = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, result);
                            }
                        } else {
                            // No arguments, use 0
                            let result = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, result);
                        }
                    }
                    // Command-line argument builtins
                    "argc" => {
                        // Call jit_argc() which returns i64
                        let result = Self::call_runtime_fn_0(builder, module, "jit_argc")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "argv" => {
                        // Call jit_argv() which returns a handle to an array
                        let result = Self::call_runtime_fn_0(builder, module, "jit_argv")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "arg" => {
                        // Call jit_arg(index) - takes one i64 argument, returns handle
                        if !args.is_empty() {
                            if let Some(&arg_val) = value_map.get(&args[0]) {
                                let result =
                                    Self::call_runtime_fn_1(builder, module, "jit_arg", arg_val)?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                let result =
                                    Self::call_runtime_fn_1(builder, module, "jit_arg", zero)?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Handle);
                            }
                        } else {
                            // No arguments, get first arg (index 0)
                            let zero = builder.ins().iconst(types::I64, 0);
                            let result = Self::call_runtime_fn_1(builder, module, "jit_arg", zero)?;
                            value_map.insert(*dst, result);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "execName" => {
                        // Call jit_exec_name() which returns a handle to a string
                        let result = Self::call_runtime_fn_0(builder, module, "jit_exec_name")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "argsCount" => {
                        // Call jit_args_count() which returns i64
                        let result = Self::call_runtime_fn_0(builder, module, "jit_args_count")?;
                        value_map.insert(*dst, result);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    // Environment variable builtins
                    "env" => {
                        if !args.is_empty() {
                            if let Some(&arg_val) = value_map.get(&args[0]) {
                                let result =
                                    Self::call_runtime_fn_1(builder, module, "jit_env", arg_val)?;
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                                value_types.insert(*dst, AotValueType::Handle);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "envGet" => {
                        let name_val = args.first().and_then(|id| value_map.get(id)).copied();
                        let default_val = args.get(1).and_then(|id| value_map.get(id)).copied();

                        if let Some(name) = name_val {
                            let res = Self::call_runtime_fn_1(builder, module, "jit_env", name)?;

                            if let Some(def) = default_val {
                                let is_zero = builder.ins().icmp_imm(IntCC::Equal, res, 0);
                                let final_res = builder.ins().select(is_zero, def, res);
                                value_map.insert(*dst, final_res);
                            } else {
                                value_map.insert(*dst, res);
                            }
                            value_types.insert(*dst, AotValueType::Handle);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "envHas" => {
                        if let Some(&name_val) = args.first().and_then(|id| value_map.get(id)) {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_env_has", name_val)?;
                            value_map.insert(*dst, res);
                            value_types.insert(*dst, AotValueType::Bool);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Bool);
                        }
                    }
                    "envAll" => {
                        let res = Self::call_runtime_fn_0(builder, module, "jit_env_all")?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "envFromFile" => {
                        let path_val = args.first().and_then(|id| value_map.get(id)).copied();
                        let res = if let Some(path) = path_val {
                            Self::call_runtime_fn_1(builder, module, "jit_env_from_file", path)?
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            Self::call_runtime_fn_1(builder, module, "jit_env_from_file", zero)?
                        };
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "envRuntimeGet" => {
                        if let Some(&name_val) = args.first().and_then(|id| value_map.get(id)) {
                            let res = Self::call_runtime_fn_1(
                                builder,
                                module,
                                "jit_env_runtime_get",
                                name_val,
                            )?;
                            value_map.insert(*dst, res);
                            value_types.insert(*dst, AotValueType::Handle);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Handle);
                        }
                    }
                    "envRuntimeHas" => {
                        if let Some(&name_val) = args.first().and_then(|id| value_map.get(id)) {
                            let res = Self::call_runtime_fn_1(
                                builder,
                                module,
                                "jit_env_runtime_has",
                                name_val,
                            )?;
                            value_map.insert(*dst, res);
                            value_types.insert(*dst, AotValueType::Bool);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Bool);
                        }
                    }
                    "envRuntimeAll" => {
                        let res = Self::call_runtime_fn_0(builder, module, "jit_env_runtime_all")?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "envRuntimeLoad" => {
                        let obj_val = args.first().and_then(|id| value_map.get(id)).copied();
                        let overwrite_val = args.get(1).and_then(|id| value_map.get(id)).copied();

                        if let Some(obj) = obj_val {
                            let overwrite = overwrite_val
                                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                            let res = Self::call_runtime_fn_2(
                                builder,
                                module,
                                "jit_env_runtime_load",
                                obj,
                                overwrite,
                            )?;
                            value_map.insert(*dst, res);
                            value_types.insert(*dst, AotValueType::Int);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Int);
                        }
                    }
                    "parseArgs" => {
                        let res = Self::call_runtime_fn_0(builder, module, "jit_parse_args")?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "argGet" => {
                        let name_val = args.first().and_then(|id| value_map.get(id)).copied();
                        let default_val = args.get(1).and_then(|id| value_map.get(id)).copied();

                        if let Some(name) = name_val {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_arg_get", name)?;
                            if let Some(def) = default_val {
                                let is_zero = builder.ins().icmp_imm(IntCC::Equal, res, 0);
                                let final_res = builder.ins().select(is_zero, def, res);
                                value_map.insert(*dst, final_res);
                            } else {
                                value_map.insert(*dst, res);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "argHas" => {
                        let name_val = args.first().and_then(|id| value_map.get(id)).copied();
                        if let Some(name) = name_val {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_arg_has", name)?;
                            value_map.insert(*dst, res);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                        value_types.insert(*dst, AotValueType::Bool);
                    }
                    "argsSlice" => {
                        let start_val = args.first().and_then(|id| value_map.get(id)).copied();
                        if let Some(start) = start_val {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_args_slice", start)?;
                            value_map.insert(*dst, res);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_args_slice", zero)?;
                            value_map.insert(*dst, res);
                        }
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "argsJoin" => {
                        let sep_val = args.first().and_then(|id| value_map.get(id)).copied();
                        if let Some(sep) = sep_val {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_args_join", sep)?;
                            value_map.insert(*dst, res);
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_args_join", zero)?;
                            value_map.insert(*dst, res);
                        }
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "strong_count" => {
                        let handle_val = args
                            .first()
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let sc = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "adesh_rt_arc_strong_count",
                            handle_val,
                        )?;
                        value_map.insert(*dst, sc);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "weak_count" => {
                        let handle_val = args
                            .first()
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let wc = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "adesh_rt_arc_weak_count",
                            handle_val,
                        )?;
                        value_map.insert(*dst, wc);
                        value_types.insert(*dst, AotValueType::I64);
                    }
                    "is_alive" => {
                        let handle_val = args
                            .first()
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let sc = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "adesh_rt_arc_strong_count",
                            handle_val,
                        )?;
                        let is_gt_zero = builder.ins().icmp_imm(IntCC::SignedGreaterThan, sc, 0);
                        let res = builder.ins().uextend(types::I64, is_gt_zero);
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Bool);
                    }
                    "upgrade" => {
                        let handle_val = args
                            .first()
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let sc = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "adesh_rt_arc_strong_count",
                            handle_val,
                        )?;
                        let is_gt_zero = builder.ins().icmp_imm(IntCC::SignedGreaterThan, sc, 0);

                        // If strong count > 0, clone the arc and return the handle, else return 0 (null)
                        let cloned_handle = Self::call_runtime_fn_1(
                            builder,
                            module,
                            "adesh_rt_arc_clone",
                            handle_val,
                        )?;
                        let zero = builder.ins().iconst(types::I64, 0);
                        let final_res = builder.ins().select(is_gt_zero, cloned_handle, zero);
                        value_map.insert(*dst, final_res);
                        value_types.insert(*dst, AotValueType::Handle);
                    }

                    "argsIndexOf" => {
                        let val = args.first().and_then(|id| value_map.get(id)).copied();
                        if let Some(v) = val {
                            let res =
                                Self::call_runtime_fn_1(builder, module, "jit_args_index_of", v)?;
                            value_map.insert(*dst, res);
                        } else {
                            let res = builder.ins().iconst(types::I64, -1);
                            value_map.insert(*dst, res);
                        }
                        value_types.insert(*dst, AotValueType::I64);
                    }

                    "str_concat" | "concat" => {
                        // String concatenation for template literals
                        // Pass type tags so the runtime knows how to interpret each value
                        // Type tags: 0=int, 1=float, 2=string/ptr, 3=bool, 4=handle
                        if args.len() >= 2 {
                            let left_id = args[0];
                            let right_id = args[1];
                            let left_val = value_map.get(&left_id).copied();
                            let right_val = value_map.get(&right_id).copied();

                            if let (Some(left), Some(right)) = (left_val, right_val) {
                                // Determine type tags
                                let left_type = value_types
                                    .get(&left_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Int);
                                let right_type = value_types
                                    .get(&right_id)
                                    .cloned()
                                    .unwrap_or(AotValueType::Int);

                                let type_tag = |t: &AotValueType| -> i64 {
                                    match t {
                                        AotValueType::F64
                                        | AotValueType::F32
                                        | AotValueType::Float => 1,
                                        AotValueType::String | AotValueType::Ptr => 2,
                                        AotValueType::Bool => 3,
                                        AotValueType::Handle => 4,
                                        _ => 0, // int/unknown
                                    }
                                };

                                let left_tag =
                                    builder.ins().iconst(types::I64, type_tag(&left_type));
                                let right_tag =
                                    builder.ins().iconst(types::I64, type_tag(&right_type));

                                // Call jit_str_concat_typed(left, left_tag, right, right_tag) -> handle
                                let mut sig = module.make_signature();
                                sig.params.push(AbiParam::new(types::I64)); // left value
                                sig.params.push(AbiParam::new(types::I64)); // left type tag
                                sig.params.push(AbiParam::new(types::I64)); // right value
                                sig.params.push(AbiParam::new(types::I64)); // right type tag
                                sig.returns.push(AbiParam::new(types::I64)); // result handle

                                let func_id = module
                                    .declare_function(
                                        "jit_str_concat_typed",
                                        cranelift_module::Linkage::Import,
                                        &sig,
                                    )
                                    .map_err(|e| {
                                        format!("Failed to declare jit_str_concat_typed: {}", e)
                                    })?;
                                let func_ref = module.declare_func_in_func(func_id, builder.func);
                                let call = builder
                                    .ins()
                                    .call(func_ref, &[left, left_tag, right, right_tag]);
                                let res = builder.inst_results(call)[0];

                                value_map.insert(*dst, res);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                    }
                    "format" => {
                        // Format value with format spec (for template literals like ${val:spec})
                        if args.len() >= 2 {
                            let value_val = args.get(0).and_then(|id| value_map.get(id)).copied();
                            let spec_val = args.get(1).and_then(|id| value_map.get(id)).copied();

                            if let (Some(val), Some(spec)) = (value_val, spec_val) {
                                let res = Self::call_runtime_fn_2(
                                    builder,
                                    module,
                                    "jit_format_value",
                                    val,
                                    spec,
                                )?;
                                value_map.insert(*dst, res);
                                value_types.insert(*dst, AotValueType::Handle);
                            } else {
                                let zero = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, zero);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                        }
                    }
                    "div" => {
                        // Polymorphic division - returns float
                        if args.len() >= 2 {
                            if let (Some(&left), Some(&right)) =
                                (value_map.get(&args[0]), value_map.get(&args[1]))
                            {
                                // Convert both operands to f64
                                let left_f64 = Self::to_f64(builder, left);
                                let right_f64 = Self::to_f64(builder, right);
                                // Perform float division
                                let result = builder.ins().fdiv(left_f64, right_f64);
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::F64);
                            } else {
                                let result = builder.ins().f64const(0.0);
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::F64);
                            }
                        } else {
                            let result = builder.ins().f64const(0.0);
                            value_map.insert(*dst, result);
                            value_types.insert(*dst, AotValueType::F64);
                        }
                    }
                    "int_div" => {
                        // Integer division - returns int (floor of division)
                        if args.len() >= 2 {
                            if let (Some(&left), Some(&right)) =
                                (value_map.get(&args[0]), value_map.get(&args[1]))
                            {
                                let left_i64 = Self::to_i64_signed(builder, left);
                                let right_i64 = Self::to_i64_signed(builder, right);
                                // Perform signed integer division
                                let result = builder.ins().sdiv(left_i64, right_i64);
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::I64);
                            } else {
                                let result = builder.ins().iconst(types::I64, 0);
                                value_map.insert(*dst, result);
                                value_types.insert(*dst, AotValueType::I64);
                            }
                        } else {
                            let result = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, result);
                            value_types.insert(*dst, AotValueType::I64);
                        }
                    }
                    name if name.starts_with("input.") || name == "input" => {
                        let name_c_str = format!("{}\0", name);
                        let name_sym = format!("__jit_bname_{}", name.replace('.', "_"));
                        let name_data_id = module
                            .declare_data(&name_sym, cranelift_module::Linkage::Local, true, false)
                            .map_err(|e| format!("Failed to declare builtin name data: {}", e))?;
                        let mut data_desc = DataDescription::new();
                        data_desc.define(name_c_str.into_bytes().into_boxed_slice());
                        let _ = module.define_data(name_data_id, &data_desc);
                        let name_global = module.declare_data_in_func(name_data_id, builder.func);
                        let name_ptr = builder.ins().global_value(types::I64, name_global);

                        let a0 = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a1 = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a2 = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a3 = args
                            .get(3)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a4 = args
                            .get(4)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let argc = builder.ins().iconst(types::I64, args.len() as i64);

                        let handle = Self::call_runtime_fn_7(
                            builder,
                            module,
                            "jit_runtime_call_input_builtin",
                            name_ptr,
                            a0,
                            a1,
                            a2,
                            a3,
                            a4,
                            argc,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "array_to_dynamic" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let ty = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let cap = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_3(
                            builder,
                            module,
                            "jit_array_to_dynamic",
                            arr,
                            ty,
                            cap,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "array_to_fixed" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let ty = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let cap = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_3(
                            builder,
                            module,
                            "jit_array_to_fixed",
                            arr,
                            ty,
                            cap,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "array_to_fixed_raw" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let ty = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let cap = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_3(
                            builder,
                            module,
                            "jit_array_to_fixed_raw",
                            arr,
                            ty,
                            cap,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "array_to_raw" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let ty = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle =
                            Self::call_runtime_fn_2(builder, module, "jit_array_to_raw", arr, ty)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "set_index" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let idx = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let val = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_3(
                            builder,
                            module,
                            "jit_set_index",
                            arr,
                            idx,
                            val,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "get_index" | "array_get" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let idx = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle =
                            Self::call_runtime_fn_2(builder, module, "jit_get_index", arr, idx)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "first" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_1(builder, module, "jit_first", arr)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "last" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_1(builder, module, "jit_last", arr)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "len" | "length" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let res = Self::call_runtime_fn_1(builder, module, "jit_len", arr)?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Int);
                    }
                    "capacity" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let res = Self::call_runtime_fn_1(builder, module, "jit_capacity", arr)?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Int);
                    }
                    "metadata_size" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let res =
                            Self::call_runtime_fn_1(builder, module, "jit_metadata_size", arr)?;
                        value_map.insert(*dst, res);
                        value_types.insert(*dst, AotValueType::Int);
                    }
                    "push" | "append" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let val = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle =
                            Self::call_runtime_fn_2(builder, module, "jit_push", arr, val)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    "pop" => {
                        let arr = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let handle = Self::call_runtime_fn_1(builder, module, "jit_pop", arr)?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                    _ => {
                        // Fallback to jit_call_builtin
                        let name_c_str = format!("{}\0", builtin_name);
                        let name_sym = format!(
                            "__jit_bname_{}",
                            builtin_name.replace('.', "_").replace('-', "_")
                        );
                        let name_data_id = module
                            .declare_data(&name_sym, cranelift_module::Linkage::Local, true, false)
                            .map_err(|e| format!("Failed to declare builtin name data: {}", e))?;
                        let mut data_desc = DataDescription::new();
                        data_desc.define(name_c_str.into_bytes().into_boxed_slice());
                        let _ = module.define_data(name_data_id, &data_desc);
                        let name_global = module.declare_data_in_func(name_data_id, builder.func);
                        let name_ptr = builder.ins().global_value(types::I64, name_global);

                        let a0 = args
                            .get(0)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a1 = args
                            .get(1)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a2 = args
                            .get(2)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a3 = args
                            .get(3)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let a4 = args
                            .get(4)
                            .and_then(|id| value_map.get(id))
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                        let argc = builder.ins().iconst(types::I64, args.len() as i64);

                        let handle = Self::call_runtime_fn_7(
                            builder,
                            module,
                            "jit_call_builtin",
                            name_ptr,
                            a0,
                            a1,
                            a2,
                            a3,
                            a4,
                            argc,
                        )?;
                        value_map.insert(*dst, handle);
                        value_types.insert(*dst, AotValueType::Handle);
                    }
                }
                Ok(())
            }
            LirInst::CallBuiltinGeneric(dst, builtin_name, args, _generic_type) => {
                if builtin_name == "input" {
                    let prompt_val = if let Some(arg0) = args.first() {
                        value_map
                            .get(arg0)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
                    } else {
                        builder.ins().iconst(types::I64, 0)
                    };
                    let opts_val = if let Some(arg1) = args.get(1) {
                        value_map
                            .get(arg1)
                            .copied()
                            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
                    } else {
                        builder.ins().iconst(types::I64, 0)
                    };

                    let handle = Self::call_runtime_fn_2(
                        builder,
                        module,
                        "jit_runtime_input",
                        prompt_val,
                        opts_val,
                    )?;
                    value_map.insert(*dst, handle);
                    value_types.insert(*dst, AotValueType::Handle);
                } else {
                    let result = match builtin_name.as_str() {
                        "len" => builder.ins().iconst(types::I64, 0),
                        _ => builder.ins().iconst(types::I64, 0),
                    };
                    value_map.insert(*dst, result);
                }
                Ok(())
            }
            LirInst::ArcNew(dst, value) => {
                let val_input = if let Some(&val) = value_map.get(value) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let handle =
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_arc_new", val_input)?;
                value_map.insert(*dst, handle);
                value_types.insert(*dst, AotValueType::Handle);
                Ok(())
            }
            LirInst::ArcClone(dst, arc_id) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let handle =
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_arc_clone", handle_input)?;
                value_map.insert(*dst, handle);
                value_types.insert(*dst, AotValueType::Handle);
                Ok(())
            }
            LirInst::ArcDrop(arc_id) => {
                if let Some(&handle_input) = value_map.get(arc_id) {
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_arc_drop", handle_input)?;
                }
                Ok(())
            }
            LirInst::WeakNew(dst, arc_id) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let weak_handle =
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_weak_new", handle_input)?;
                value_map.insert(*dst, weak_handle);
                value_types.insert(*dst, AotValueType::Handle);
                Ok(())
            }
            LirInst::WeakDrop(weak_id) => {
                if let Some(&handle_input) = value_map.get(weak_id) {
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_weak_drop", handle_input)?;
                }
                Ok(())
            }
            LirInst::ArcGet(dst, arc_id) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let val =
                    Self::call_runtime_fn_1(builder, module, "adesh_rt_arc_get", handle_input)?;
                value_map.insert(*dst, val);
                value_types.insert(*dst, AotValueType::I64);
                Ok(())
            }
            LirInst::ArcSet(arc_id, value) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let val_input = if let Some(&val) = value_map.get(value) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                Self::call_runtime_fn_2(
                    builder,
                    module,
                    "adesh_rt_arc_set",
                    handle_input,
                    val_input,
                )?;
                Ok(())
            }
            LirInst::LoadVar(dst, var_name) => {
                let infer_type = |var_name: &str| -> AotValueType {
                    if let Some(vt) = var_types.get(var_name) {
                        return vt.clone();
                    }
                    if let Some(&lir_type) = global_types.get(var_name) {
                        use crate::backends::common::lir::LirType;
                        match lir_type {
                            LirType::I64 | LirType::I32 | LirType::I16 | LirType::I8 => {
                                AotValueType::I64
                            }
                            LirType::U64 | LirType::U32 | LirType::U16 | LirType::U8 => {
                                AotValueType::I64
                            }
                            LirType::F64 => AotValueType::F64,
                            LirType::F32 => AotValueType::F32,
                            LirType::Bool => AotValueType::I64,
                            LirType::Ptr => AotValueType::Ptr,
                            _ => AotValueType::Int,
                        }
                    } else {
                        AotValueType::Int
                    }
                };

                let aot_type = infer_type(var_name);
                value_types.insert(*dst, aot_type.clone());

                if var_nulls.contains(var_name) {
                    const_nulls.insert(*dst);
                } else {
                    const_nulls.remove(dst);
                }

                if let Some(props) = var_object_properties.get(var_name).cloned() {
                    object_properties.insert(*dst, props);
                }

                if let Some(data_id) = global_vars.get(var_name) {
                    let gv = module.declare_data_in_func(*data_id, builder.func);
                    let ptr = builder.ins().global_value(types::I64, gv);
                    let ty = match aot_type {
                        AotValueType::F64 | AotValueType::Float => types::F64,
                        AotValueType::F32 => types::F32,
                        AotValueType::I8 | AotValueType::U8 | AotValueType::Bool => types::I8,
                        AotValueType::I16 | AotValueType::U16 => types::I16,
                        AotValueType::I32 | AotValueType::U32 => types::I32,
                        AotValueType::Handle
                        | AotValueType::String
                        | AotValueType::Ptr
                        | AotValueType::Array(_, _)
                        | AotValueType::RawArray(_, _)
                        | AotValueType::Set(_, _)
                        | AotValueType::Tuple(_)
                        | AotValueType::I64
                        | AotValueType::U64
                        | AotValueType::Int => types::I64,
                        _ => global_types
                            .get(var_name)
                            .copied()
                            .map(Self::global_clif_type)
                            .unwrap_or(types::I64),
                    };
                    let loaded = builder.ins().load(ty, MemFlags::new(), ptr, 0);
                    value_map.insert(*dst, loaded);
                } else if let Some(&(v_i64, v_f64)) = cranelift_vars.get(var_name) {
                    let is_float = matches!(aot_type, AotValueType::F64 | AotValueType::F32);
                    let var = if is_float { v_f64 } else { v_i64 };
                    let val = builder.use_var(var);
                    value_map.insert(*dst, val);
                } else {
                    // Fallback to default zero if not found
                    let val = if matches!(aot_type, AotValueType::F64) {
                        builder.ins().f64const(0.0)
                    } else if matches!(aot_type, AotValueType::F32) {
                        builder.ins().f32const(0.0)
                    } else {
                        builder.ins().iconst(types::I64, 0)
                    };
                    value_map.insert(*dst, val);
                }
                Ok(())
            }
            LirInst::StoreVar(var_name, src) => {
                if let Some(&src_val) = value_map.get(src) {
                    var_namespace.insert(var_name.clone(), src_val);
                    if let Some(src_type) = value_types.get(src).cloned() {
                        var_types.insert(var_name.clone(), src_type);
                    }
                    if let Some(props) = object_properties.get(src).cloned() {
                        var_object_properties.insert(var_name.clone(), props);
                    } else {
                        var_object_properties.remove(var_name);
                    }
                    if const_nulls.contains(src) {
                        var_nulls.insert(var_name.clone());
                    } else {
                        var_nulls.remove(var_name);
                    }
                    if let Some(data_id) = global_vars.get(var_name) {
                        let gv = module.declare_data_in_func(*data_id, builder.func);
                        let ptr = builder.ins().global_value(types::I64, gv);
                        builder.ins().store(MemFlags::new(), src_val, ptr, 0);
                    } else if let Some(&(v_i64, v_f64)) = cranelift_vars.get(var_name) {
                        // Store into Cranelift Variable
                        let ty = builder.func.dfg.value_type(src_val);
                        if ty == types::F64 || ty == types::F32 {
                            let src_val_converted = Self::to_f64(builder, src_val);
                            builder.def_var(v_f64, src_val_converted);
                        } else {
                            let src_val_converted = Self::to_i64_unsigned(builder, src_val);
                            builder.def_var(v_i64, src_val_converted);
                        }
                    }
                } else {
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "Warning: StoreVar '{}' source value {} not found in value_map",
                            var_name, src
                        );
                    }
                }
                Ok(())
            }
            LirInst::Copy(dst, src) => {
                // Copy instruction - just copy the Cranelift value
                if let Some(&src_val) = value_map.get(src) {
                    value_map.insert(*dst, src_val);
                    if let Some(ty) = value_types.get(src).cloned() {
                        value_types.insert(*dst, ty);
                    }
                    if let Some(props) = object_properties.get(src).cloned() {
                        object_properties.insert(*dst, props);
                    } else {
                        object_properties.remove(dst);
                    }
                    if const_nulls.contains(src) {
                        const_nulls.insert(*dst);
                    } else {
                        const_nulls.remove(dst);
                    }
                }
                Ok(())
            }
            LirInst::Phi(dst, sources) => {
                if let Some(&(v_i64, v_f64)) = phi_vars.get(dst) {
                    let is_float = sources.iter().any(|(_, src)| {
                        if let Some(ty) = value_types.get(src) {
                            matches!(ty, AotValueType::F64 | AotValueType::F32)
                        } else {
                            false
                        }
                    });
                    let var = if is_float { v_f64 } else { v_i64 };
                    let val = builder.use_var(var);
                    value_map.insert(*dst, val);

                    // Propagate type, const nulls, etc.
                    let mut inferred_type = AotValueType::Int;
                    let mut is_null = false;
                    for (_, src) in sources {
                        if let Some(ty) = value_types.get(src).cloned() {
                            inferred_type = ty;
                        }
                        if const_nulls.contains(src) {
                            is_null = true;
                        }
                    }
                    value_types.insert(*dst, inferred_type.clone());
                    if is_null {
                        const_nulls.insert(*dst);
                        if matches!(inferred_type, AotValueType::Int | AotValueType::Unknown) {
                            value_types.insert(*dst, AotValueType::Ptr);
                        }
                    } else {
                        const_nulls.remove(dst);
                    }
                } else {
                    // Fallback stub
                    if let Some((_, val)) = sources.first() {
                        if let Some(&v) = value_map.get(val) {
                            value_map.insert(*dst, v);
                            if let Some(ty) = value_types.get(val).cloned() {
                                value_types.insert(*dst, ty);
                            }
                            if const_nulls.contains(val) {
                                const_nulls.insert(*dst);
                                value_types.insert(*dst, AotValueType::Ptr);
                            } else {
                                const_nulls.remove(dst);
                            }
                        } else {
                            let zero = builder.ins().iconst(types::I64, 0);
                            value_map.insert(*dst, zero);
                            value_types.insert(*dst, AotValueType::Int);
                            const_nulls.remove(dst);
                        }
                    } else {
                        let zero = builder.ins().iconst(types::I64, 0);
                        value_map.insert(*dst, zero);
                        value_types.insert(*dst, AotValueType::Int);
                        const_nulls.remove(dst);
                    }
                }
                Ok(())
            }
            LirInst::ConstString(dst, string_value) => {
                const_strings.insert(*dst, string_value.clone());
                let string_ptr =
                    Self::get_or_create_string_ptr(module, format_strings, builder, string_value)?;
                value_map.insert(*dst, string_ptr);
                value_types.insert(*dst, AotValueType::String);
                const_nulls.remove(dst);
                Ok(())
            }
            // Memory operations
            LirInst::Alloc(dst, size_id) => {
                // Allocate memory using malloc
                if let Some(&size_val) = value_map.get(size_id) {
                    // Get malloc function reference
                    // let malloc_sig = module.make_signature();
                    let mut malloc_sig_builder = module.make_signature();
                    malloc_sig_builder.params.push(AbiParam::new(types::I64));
                    malloc_sig_builder.returns.push(AbiParam::new(types::I64));
                    let malloc_func = module
                        .declare_function("malloc", Linkage::Import, &malloc_sig_builder)
                        .map_err(|e| format!("Failed to declare malloc: {}", e))?;
                    let malloc_ref = module.declare_func_in_func(malloc_func, builder.func);

                    // Call malloc
                    let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                    let results = builder.inst_results(call_inst);
                    if !results.is_empty() {
                        value_map.insert(*dst, results[0]);
                        value_types.insert(*dst, AotValueType::Ptr);
                    }
                } else {
                    // Size not found, return null
                    let zero = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, zero);
                    value_types.insert(*dst, AotValueType::Ptr);
                }
                Ok(())
            }
            LirInst::AllocTyped(dst, size_id, _elem_size) => {
                // AllocTyped - similar to Alloc for now
                if let Some(&size_val) = value_map.get(size_id) {
                    // let malloc_sig_builder = module.make_signature();
                    let mut malloc_sig = module.make_signature();
                    malloc_sig.params.push(AbiParam::new(types::I64));
                    malloc_sig.returns.push(AbiParam::new(types::I64));
                    let malloc_func = module
                        .declare_function("malloc", Linkage::Import, &malloc_sig)
                        .map_err(|e| format!("Failed to declare malloc: {}", e))?;
                    let malloc_ref = module.declare_func_in_func(malloc_func, builder.func);

                    let call_inst = builder.ins().call(malloc_ref, &[size_val]);
                    let results = builder.inst_results(call_inst);
                    if !results.is_empty() {
                        value_map.insert(*dst, results[0]);
                        value_types.insert(*dst, AotValueType::Ptr);
                    }
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, zero);
                    value_types.insert(*dst, AotValueType::Ptr);
                }
                Ok(())
            }
            LirInst::Free(ptr_id) => {
                // Free memory using free
                if let Some(&ptr_val) = value_map.get(ptr_id) {
                    let mut free_sig = module.make_signature();
                    free_sig.params.push(AbiParam::new(types::I64));
                    let free_func = module
                        .declare_function("free", Linkage::Import, &free_sig)
                        .map_err(|e| format!("Failed to declare free: {}", e))?;
                    let free_ref = module.declare_func_in_func(free_func, builder.func);

                    builder.ins().call(free_ref, &[ptr_val]);
                }
                Ok(())
            }
            LirInst::PtrLoad(dst, ptr_id, index_id) => {
                // Load from pointer at index
                if let (Some(&ptr_val), Some(&index_val)) =
                    (value_map.get(ptr_id), value_map.get(index_id))
                {
                    // Calculate offset: ptr + index * 8 (assuming 64-bit values)
                    let eight = builder.ins().iconst(types::I64, 8);
                    let offset = builder.ins().imul(index_val, eight);
                    let addr = builder.ins().iadd(ptr_val, offset);

                    // Load i64 from memory
                    let loaded = builder.ins().load(types::I64, MemFlags::new(), addr, 0);
                    value_map.insert(*dst, loaded);
                    value_types.insert(*dst, AotValueType::I64);
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    value_map.insert(*dst, zero);
                }
                Ok(())
            }
            LirInst::PtrStore(ptr_id, value_id, index_id) => {
                // Store to pointer at index
                if let (Some(&ptr_val), Some(&value_val), Some(&index_val)) = (
                    value_map.get(ptr_id),
                    value_map.get(value_id),
                    value_map.get(index_id),
                ) {
                    // Calculate offset: ptr + index * 8
                    let eight = builder.ins().iconst(types::I64, 8);
                    let offset = builder.ins().imul(index_val, eight);
                    let addr = builder.ins().iadd(ptr_val, offset);

                    // Store i64 to memory
                    builder.ins().store(MemFlags::new(), value_val, addr, 0);
                }
                Ok(())
            }
            // Additional constant types
            LirInst::ConstBigInt(dst, bigint) => {
                use num_traits::ToPrimitive;
                let v = if let Some(u) = bigint.to_u64() {
                    let c = builder.ins().iconst(types::I64, u as i64);
                    value_types.insert(*dst, AotValueType::U64);
                    c
                } else if let Some(i) = bigint.to_i64() {
                    let c = builder.ins().iconst(types::I64, i);
                    value_types.insert(*dst, AotValueType::I64);
                    c
                } else {
                    let c = builder.ins().iconst(types::I64, 0);
                    value_types.insert(*dst, AotValueType::I64);
                    c
                };
                value_map.insert(*dst, v);
                Ok(())
            }
            LirInst::ConstU128(dst, val) => {
                // U128 truncated to i64
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::U128);
                Ok(())
            }
            LirInst::ConstI128(dst, val) => {
                // I128 truncated to i64
                let v = builder.ins().iconst(types::I64, *val as i64);
                value_map.insert(*dst, v);
                value_types.insert(*dst, AotValueType::I128);
                Ok(())
            }
            LirInst::ConstFunc(dst, _func_name, _captured, _is_async) => {
                // Function constants not fully supported - return function ID as integer
                let zero = builder.ins().iconst(types::I64, 0);
                value_map.insert(*dst, zero);
                value_types.insert(*dst, AotValueType::Ptr);
                Ok(())
            }
            LirInst::ArcStrongCount(dst, arc_id) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let count = Self::call_runtime_fn_1(
                    builder,
                    module,
                    "adesh_rt_arc_strong_count",
                    handle_input,
                )?;
                value_map.insert(*dst, count);
                value_types.insert(*dst, AotValueType::I64);
                Ok(())
            }
            LirInst::ArcWeakCount(dst, arc_id) => {
                let handle_input = if let Some(&val) = value_map.get(arc_id) {
                    val
                } else {
                    builder.ins().iconst(types::I64, 0)
                };
                let count = Self::call_runtime_fn_1(
                    builder,
                    module,
                    "adesh_rt_arc_weak_count",
                    handle_input,
                )?;
                value_map.insert(*dst, count);
                value_types.insert(*dst, AotValueType::I64);
                Ok(())
            }
            LirInst::LoadModule(dst, _module_name) => {
                // Module loading not supported in JIT - return null
                let zero = builder.ins().iconst(types::I64, 0);
                value_map.insert(*dst, zero);
                value_types.insert(*dst, AotValueType::Ptr);
                Ok(())
            }
            LirInst::TailCall(func_name, args) => {
                // Tail call optimization: compile as call + return.
                // Cranelift may further optimize this into a true tail call at the IR level.
                if let Some(&target_func_id) = functions.get(func_name) {
                    let arg_vals: Vec<Value> = args
                        .iter()
                        .filter_map(|a| {
                            value_map.get(a).copied().map(|v| {
                                match value_types.get(a).cloned().unwrap_or(AotValueType::Unknown) {
                                    AotValueType::Bool
                                    | AotValueType::U8
                                    | AotValueType::U16
                                    | AotValueType::U32 => Self::to_i64_unsigned(builder, v),
                                    AotValueType::I8 | AotValueType::I16 | AotValueType::I32 => {
                                        Self::to_i64_signed(builder, v)
                                    }
                                    _ => {
                                        let ty = builder.func.dfg.value_type(v);
                                        if ty.is_int() && ty != types::I64 {
                                            Self::to_i64_unsigned(builder, v)
                                        } else {
                                            v
                                        }
                                    }
                                }
                            })
                        })
                        .collect();

                    let func_ref = module.declare_func_in_func(target_func_id, builder.func);
                    let call_inst = builder.ins().call(func_ref, &arg_vals);

                    // Copy result before returning to avoid borrow conflict
                    let ret_results: Vec<Value> = builder.inst_results(call_inst).to_vec();
                    if !ret_results.is_empty() {
                        builder.ins().return_(&[ret_results[0]]);
                    } else {
                        builder.ins().return_(&[]);
                    }
                } else {
                    // Unknown function - return 0
                    if let Some(ret_param) = builder.func.signature.returns.get(0) {
                        let ty = ret_param.value_type;
                        let zero = if ty == types::F64 {
                            builder.ins().f64const(0.0)
                        } else if ty == types::F32 {
                            builder.ins().f32const(0.0)
                        } else {
                            builder.ins().iconst(ty, 0)
                        };
                        builder.ins().return_(&[zero]);
                    } else {
                        builder.ins().return_(&[]);
                    }
                }
                Ok(())
            }
            _ => {
                // For now, skip unsupported instructions
                Ok(())
            }
        }
    }

    fn to_i64_unsigned(builder: &mut FunctionBuilder, value: Value) -> Value {
        let ty = builder.func.dfg.value_type(value);
        if ty == types::I64 {
            value
        } else if ty.is_int() {
            builder.ins().uextend(types::I64, value)
        } else if ty == types::F64 || ty == types::F32 {
            let f64_val = if ty == types::F32 {
                builder.ins().fpromote(types::F64, value)
            } else {
                value
            };
            builder.ins().fcvt_to_sint(types::I64, f64_val)
        } else {
            value
        }
    }

    fn to_i64_signed(builder: &mut FunctionBuilder, value: Value) -> Value {
        let ty = builder.func.dfg.value_type(value);
        if ty == types::I64 {
            value
        } else if ty.is_int() {
            builder.ins().sextend(types::I64, value)
        } else if ty == types::F64 || ty == types::F32 {
            let f64_val = if ty == types::F32 {
                builder.ins().fpromote(types::F64, value)
            } else {
                value
            };
            builder.ins().fcvt_to_sint(types::I64, f64_val)
        } else {
            value
        }
    }

    fn to_f64(builder: &mut FunctionBuilder, value: Value) -> Value {
        let ty = builder.func.dfg.value_type(value);
        if ty == types::F64 {
            value
        } else if ty == types::F32 {
            builder.ins().fpromote(types::F64, value)
        } else if ty.is_int() {
            let i64_val = Self::to_i64_signed(builder, value);
            builder.ins().fcvt_from_sint(types::F64, i64_val)
        } else {
            value
        }
    }

    /// Compile print builtin by calling printf
    #[allow(unused)]
    fn compile_print_builtin(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        format_strings: &mut HashMap<String, cranelift_module::DataId>,
        args: &[ValueId],
        value_map: &HashMap<ValueId, Value>,
        value_types: &HashMap<ValueId, crate::backends::aot::AotValueType>,
        const_strings: &HashMap<ValueId, String>,
        const_bools: &HashMap<ValueId, bool>,
        const_nulls: &HashSet<ValueId>,
        object_properties: &HashMap<ValueId, HashMap<String, (ValueId, AotValueType)>>,
        add_newline: bool,
    ) -> Result<(), String> {
        use crate::backends::aot::AotValueType;
        let mut print_args = args;
        let mut opts = JitPrintOptions::default();
        let mut options_handle: Option<Value> = None;

        if let Some(&last_arg) = args.last() {
            if let Some(props) = object_properties.get(&last_arg) {
                let is_options = props.keys().any(|k| {
                    matches!(
                        k.as_str(),
                        "sep"
                            | "end"
                            | "file"
                            | "color"
                            | "background"
                            | "bold"
                            | "italic"
                            | "underline"
                            | "strikethrough"
                            | "flush"
                            | "pretty"
                    )
                });

                if is_options {
                    options_handle = value_map.get(&last_arg).copied();
                    for (key, (val_id, _val_type)) in props {
                        match key.as_str() {
                            "sep" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.sep = Some(s.clone());
                                }
                            }
                            "end" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.end = Some(s.clone());
                                }
                            }
                            "file" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.file = Some(s.clone());
                                }
                            }
                            "color" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.color = Some(s.clone());
                                }
                            }
                            "background" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.background = Some(s.clone());
                                }
                            }
                            "bold" => {
                                if let Some(&b) = const_bools.get(val_id) {
                                    opts.bold = b;
                                }
                            }
                            "italic" => {
                                if let Some(&b) = const_bools.get(val_id) {
                                    opts.italic = b;
                                }
                            }
                            "underline" => {
                                if let Some(&b) = const_bools.get(val_id) {
                                    opts.underline = b;
                                }
                            }
                            "strikethrough" => {
                                if let Some(&b) = const_bools.get(val_id) {
                                    opts.strikethrough = b;
                                }
                            }
                            "flush" => {
                                if let Some(&b) = const_bools.get(val_id) {
                                    opts.flush = b;
                                }
                            }
                            "pretty" => {
                                if let Some(s) = const_strings.get(val_id) {
                                    opts.pretty = Some(s.clone());
                                } else if let Some(&b) = const_bools.get(val_id) {
                                    opts.pretty = Some(if b {
                                        "full".to_string()
                                    } else {
                                        "none".to_string()
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    print_args = &args[..args.len() - 1];
                }
            }
        }

        let null_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, "null")?;

        if let Some(opts_handle) = options_handle {
            let mut wrapped_handles: Vec<Value> = Vec::new();
            for arg_id in print_args.iter() {
                let val = if let Some(s) = const_strings.get(arg_id) {
                    Self::get_or_create_string_ptr(module, format_strings, builder, s)?
                } else {
                    value_map
                        .get(arg_id)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
                };
                let val_type = value_types
                    .get(arg_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                let is_known_null = const_nulls.contains(arg_id);

                let handle = if is_known_null {
                    Self::call_wrap_null(builder, module)?
                } else {
                    match val_type {
                        AotValueType::Handle => val,
                        AotValueType::Bool => Self::call_wrap_bool(builder, module, val)?,
                        AotValueType::Char => {
                            let extended = Self::to_i64_unsigned(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_char", extended)?
                        }
                        AotValueType::F64 | AotValueType::Float => {
                            Self::call_wrap_f64(builder, module, val)?
                        }
                        AotValueType::F32 => Self::call_wrap_f32(builder, module, val)?,
                        AotValueType::String => Self::call_wrap_str(builder, module, val)?,
                        AotValueType::Ptr => {
                            let is_null = builder.ins().icmp_imm(IntCC::Equal, val, 0);
                            let string_ptr = builder.ins().select(is_null, null_ptr, val);
                            Self::call_wrap_str(builder, module, string_ptr)?
                        }
                        AotValueType::U8 => {
                            let extended = Self::to_i64_unsigned(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_u8", extended)?
                        }
                        AotValueType::U16 => {
                            let extended = Self::to_i64_unsigned(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_u16", extended)?
                        }
                        AotValueType::U32 => {
                            let extended = Self::to_i64_unsigned(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_u32", extended)?
                        }
                        AotValueType::U64 => {
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_u64", val)?
                        }
                        AotValueType::I8 => {
                            let extended = Self::to_i64_signed(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_i8", extended)?
                        }
                        AotValueType::I16 => {
                            let extended = Self::to_i64_signed(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_i16", extended)?
                        }
                        AotValueType::I32 => {
                            let extended = Self::to_i64_signed(builder, val);
                            Self::call_runtime_fn_1(builder, module, "jit_wrap_i32", extended)?
                        }
                        _ => Self::call_wrap_i64(builder, module, val)?,
                    }
                };

                wrapped_handles.push(handle);
            }

            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                ((wrapped_handles.len().max(1)) * 8) as u32,
                8,
            ));
            let ptr = builder.ins().stack_addr(types::I64, slot, 0);

            for (i, handle) in wrapped_handles.iter().enumerate() {
                let off = (i * 8) as i32;
                builder.ins().store(MemFlags::new(), *handle, ptr, off);
            }

            let count = builder
                .ins()
                .iconst(types::I64, wrapped_handles.len() as i64);
            Self::call_runtime_fn_3(
                builder,
                module,
                "jit_print_values_with_options",
                ptr,
                count,
                opts_handle,
            )?;
            return Ok(());
        }

        let sep = opts.sep.as_deref().unwrap_or(" ");
        let end = opts
            .end
            .as_deref()
            .unwrap_or(if add_newline { "\n" } else { "\n" });
        let pretty_enabled = match opts.pretty.as_deref() {
            Some("none") => false,
            Some(_) => true,
            None => false,
        };

        let mut sig_i64 = module.make_signature();
        sig_i64.call_conv = CallConv::triple_default(module.isa().triple());
        sig_i64.params.push(AbiParam::new(types::I64));

        let mut sig_f64 = module.make_signature();
        sig_f64.call_conv = CallConv::triple_default(module.isa().triple());
        sig_f64.params.push(AbiParam::new(types::F64));

        let mut sig_str = module.make_signature();
        sig_str.call_conv = CallConv::triple_default(module.isa().triple());
        sig_str.params.push(AbiParam::new(types::I64));

        let mut sig_flush = module.make_signature();
        sig_flush.call_conv = CallConv::triple_default(module.isa().triple());

        let mut sig_file_str = module.make_signature();
        sig_file_str.call_conv = CallConv::triple_default(module.isa().triple());
        sig_file_str.params.push(AbiParam::new(types::I64));
        sig_file_str.params.push(AbiParam::new(types::I64));

        let mut sig_file_i64 = module.make_signature();
        sig_file_i64.call_conv = CallConv::triple_default(module.isa().triple());
        sig_file_i64.params.push(AbiParam::new(types::I64));
        sig_file_i64.params.push(AbiParam::new(types::I64));

        let mut sig_file_f64 = module.make_signature();
        sig_file_f64.call_conv = CallConv::triple_default(module.isa().triple());
        sig_file_f64.params.push(AbiParam::new(types::I64));
        sig_file_f64.params.push(AbiParam::new(types::F64));

        let print_i64_id = module
            .declare_function("print_i64", Linkage::Import, &sig_i64)
            .map_err(|e| format!("Failed to declare print_i64: {}", e))?;
        let print_i64_ref = module.declare_func_in_func(print_i64_id, builder.func);

        let mut sig_f32 = module.make_signature();
        sig_f32.call_conv = CallConv::triple_default(module.isa().triple());
        sig_f32.params.push(AbiParam::new(types::F32));

        let print_f64_id = module
            .declare_function("print_f64", Linkage::Import, &sig_f64)
            .map_err(|e| format!("Failed to declare print_f64: {}", e))?;
        let print_f64_ref = module.declare_func_in_func(print_f64_id, builder.func);

        let print_f32_id = module
            .declare_function("print_f32", Linkage::Import, &sig_f32)
            .map_err(|e| format!("Failed to declare print_f32: {}", e))?;
        let print_f32_ref = module.declare_func_in_func(print_f32_id, builder.func);

        let print_str_id = module
            .declare_function("print_str", Linkage::Import, &sig_str)
            .map_err(|e| format!("Failed to declare print_str: {}", e))?;
        let print_str_ref = module.declare_func_in_func(print_str_id, builder.func);

        let print_flush_id = module
            .declare_function("print_flush", Linkage::Import, &sig_flush)
            .map_err(|e| format!("Failed to declare print_flush: {}", e))?;
        let print_flush_ref = module.declare_func_in_func(print_flush_id, builder.func);

        let file_write_str_id = module
            .declare_function("file_write_str", Linkage::Import, &sig_file_str)
            .map_err(|e| format!("Failed to declare file_write_str: {}", e))?;
        let file_write_str_ref = module.declare_func_in_func(file_write_str_id, builder.func);

        let file_write_i64_id = module
            .declare_function("file_write_i64", Linkage::Import, &sig_file_i64)
            .map_err(|e| format!("Failed to declare file_write_i64: {}", e))?;
        let file_write_i64_ref = module.declare_func_in_func(file_write_i64_id, builder.func);

        let file_write_f64_id = module
            .declare_function("file_write_f64", Linkage::Import, &sig_file_f64)
            .map_err(|e| format!("Failed to declare file_write_f64: {}", e))?;
        let file_write_f64_ref = module.declare_func_in_func(file_write_f64_id, builder.func);

        let true_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, "true")?;
        let false_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, "false")?;

        if let Some(file_path) = opts.file.as_deref() {
            let file_path_ptr =
                Self::get_or_create_string_ptr(module, format_strings, builder, file_path)?;

            if print_args.is_empty() {
                let end_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, end)?;
                builder
                    .ins()
                    .call(file_write_str_ref, &[file_path_ptr, end_ptr]);
                return Ok(());
            }

            for (i, arg_id) in print_args.iter().enumerate() {
                if i > 0 {
                    let sep_ptr =
                        Self::get_or_create_string_ptr(module, format_strings, builder, sep)?;
                    builder
                        .ins()
                        .call(file_write_str_ref, &[file_path_ptr, sep_ptr]);
                }

                let val = value_map
                    .get(arg_id)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                let val_type = value_types
                    .get(arg_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);

                let hint = if pretty_enabled {
                    match val_type {
                        AotValueType::Bool => Some(" <bool>"),
                        AotValueType::String => Some(" <string>"),
                        AotValueType::F64 | AotValueType::F32 | AotValueType::Float => {
                            Some(" <number>")
                        }
                        _ => Some(" <int>"),
                    }
                } else {
                    None
                };

                match val_type {
                    AotValueType::Bool => {
                        let cond = builder.ins().icmp_imm(IntCC::NotEqual, val, 0);
                        let ptr = builder.ins().select(cond, true_ptr, false_ptr);
                        builder
                            .ins()
                            .call(file_write_str_ref, &[file_path_ptr, ptr]);
                    }
                    AotValueType::F64 | AotValueType::Float => {
                        builder
                            .ins()
                            .call(file_write_f64_ref, &[file_path_ptr, val]);
                    }
                    AotValueType::F32 => {
                        let promoted = builder.ins().fpromote(types::F64, val);
                        builder
                            .ins()
                            .call(file_write_f64_ref, &[file_path_ptr, promoted]);
                    }
                    AotValueType::String => {
                        builder
                            .ins()
                            .call(file_write_str_ref, &[file_path_ptr, val]);
                    }
                    AotValueType::Ptr => {
                        builder
                            .ins()
                            .call(file_write_str_ref, &[file_path_ptr, null_ptr]);
                    }
                    _ => {
                        builder
                            .ins()
                            .call(file_write_i64_ref, &[file_path_ptr, val]);
                    }
                }

                if let Some(h) = hint {
                    let hint_ptr =
                        Self::get_or_create_string_ptr(module, format_strings, builder, h)?;
                    builder
                        .ins()
                        .call(file_write_str_ref, &[file_path_ptr, hint_ptr]);
                }
            }

            let end_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, end)?;
            builder
                .ins()
                .call(file_write_str_ref, &[file_path_ptr, end_ptr]);
            return Ok(());
        }

        let has_styling = opts.bold
            || opts.italic
            || opts.underline
            || opts.strikethrough
            || opts.color.is_some()
            || opts.background.is_some();

        if has_styling {
            let mut codes: Vec<String> = Vec::with_capacity(6);
            if opts.bold {
                codes.push("1".to_string());
            }
            if opts.italic {
                codes.push("3".to_string());
            }
            if opts.underline {
                codes.push("4".to_string());
            }
            if opts.strikethrough {
                codes.push("9".to_string());
            }
            if let Some(hex) = opts.color.as_deref() {
                if let Some((r, g, b)) = parse_hex_color(hex) {
                    codes.push(
                        make_fg_color_escape(r, g, b)
                            .trim_start_matches("\x1b[")
                            .trim_end_matches('m')
                            .to_string(),
                    );
                }
            }
            if let Some(hex) = opts.background.as_deref() {
                if let Some((r, g, b)) = parse_hex_color(hex) {
                    codes.push(
                        make_bg_color_escape(r, g, b)
                            .trim_start_matches("\x1b[")
                            .trim_end_matches('m')
                            .to_string(),
                    );
                }
            }
            if !codes.is_empty() {
                let style = format!("\x1b[{}m", codes.join(";"));
                let style_ptr =
                    Self::get_or_create_string_ptr(module, format_strings, builder, &style)?;
                builder.ins().call(print_str_ref, &[style_ptr]);
            }
        }

        if print_args.is_empty() {
            let end_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, end)?;
            builder.ins().call(print_str_ref, &[end_ptr]);
        } else {
            // Determine pretty print mode for handles
            let pretty_mode: i64 = match opts.pretty.as_deref() {
                Some("compact") => 2,
                Some("simple") => 3,
                Some("full") | Some("true") => 1,
                Some(_) if pretty_enabled => 1,
                _ => 0,
            };

            for (i, arg_id) in print_args.iter().enumerate() {
                if i > 0 {
                    let sep_ptr =
                        Self::get_or_create_string_ptr(module, format_strings, builder, sep)?;
                    builder.ins().call(print_str_ref, &[sep_ptr]);
                }

                let val = if let Some(s) = const_strings.get(arg_id) {
                    Self::get_or_create_string_ptr(module, format_strings, builder, s)?
                } else {
                    value_map
                        .get(arg_id)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0))
                };
                let val_type = value_types
                    .get(arg_id)
                    .cloned()
                    .unwrap_or(AotValueType::Int);
                let is_known_null = const_nulls.contains(arg_id);

                // If pretty printing is enabled, wrap all values and use unified print
                if pretty_enabled {
                    let wrapped_handle = if is_known_null {
                        Self::call_wrap_null(builder, module)?
                    } else {
                        match val_type {
                            AotValueType::Handle => val, // Already a handle
                            AotValueType::Bool => Self::call_wrap_bool(builder, module, val)?,
                            AotValueType::Char => {
                                let extended = Self::to_i64_unsigned(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_char", extended)?
                            }
                            AotValueType::F64 | AotValueType::Float => {
                                Self::call_wrap_f64(builder, module, val)?
                            }
                            AotValueType::F32 => Self::call_wrap_f32(builder, module, val)?,
                            AotValueType::String => Self::call_wrap_str(builder, module, val)?,
                            AotValueType::Ptr => Self::call_wrap_null(builder, module)?,
                            AotValueType::U8 => {
                                let extended = Self::to_i64_unsigned(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_u8", extended)?
                            }
                            AotValueType::U16 => {
                                let extended = Self::to_i64_unsigned(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_u16", extended)?
                            }
                            AotValueType::U32 => {
                                let extended = Self::to_i64_unsigned(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_u32", extended)?
                            }
                            AotValueType::U64 => {
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_u64", val)?
                            }
                            AotValueType::I8 => {
                                let extended = Self::to_i64_signed(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_i8", extended)?
                            }
                            AotValueType::I16 => {
                                let extended = Self::to_i64_signed(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_i16", extended)?
                            }
                            AotValueType::I32 => {
                                let extended = Self::to_i64_signed(builder, val);
                                Self::call_runtime_fn_1(builder, module, "jit_wrap_i32", extended)?
                            }
                            _ => Self::call_wrap_i64(builder, module, val)?,
                        }
                    };
                    Self::call_print_value(builder, module, wrapped_handle, pretty_mode)?;
                    continue;
                }

                // Non-pretty path: print values directly without wrapping
                match val_type {
                    AotValueType::Handle => {
                        Self::call_print_value(builder, module, val, 0)?;
                    }
                    AotValueType::Bool => {
                        let cond = builder.ins().icmp_imm(IntCC::NotEqual, val, 0);
                        let ptr = builder.ins().select(cond, true_ptr, false_ptr);
                        builder.ins().call(print_str_ref, &[ptr]);
                    }
                    AotValueType::F64 | AotValueType::Float => {
                        builder.ins().call(print_f64_ref, &[val]);
                    }
                    AotValueType::F32 => {
                        builder.ins().call(print_f32_ref, &[val]);
                    }
                    AotValueType::String => {
                        builder.ins().call(print_str_ref, &[val]);
                    }
                    AotValueType::Ptr => {
                        let is_null = builder.ins().icmp_imm(IntCC::Equal, val, 0);
                        let string_ptr = builder.ins().select(is_null, null_ptr, val);
                        builder.ins().call(print_str_ref, &[string_ptr]);
                    }
                    // Smaller unsigned integer types - zero extend to I64
                    AotValueType::U8 => {
                        let extended = Self::to_i64_unsigned(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    AotValueType::U16 => {
                        let extended = Self::to_i64_unsigned(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    AotValueType::U16 => {
                        let extended = Self::to_i64_unsigned(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    AotValueType::U32 => {
                        let extended = Self::to_i64_unsigned(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    // Smaller signed integer types - sign extend to I64
                    AotValueType::I8 => {
                        let extended = Self::to_i64_signed(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    AotValueType::I16 => {
                        let extended = Self::to_i64_signed(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    AotValueType::I32 => {
                        let extended = Self::to_i64_signed(builder, val);
                        builder.ins().call(print_i64_ref, &[extended]);
                    }
                    // I64, U64, Int, and Unknown - pass directly (already 64-bit)
                    _ => {
                        builder.ins().call(print_i64_ref, &[val]);
                    }
                }
            }

            let end_ptr = Self::get_or_create_string_ptr(module, format_strings, builder, end)?;
            builder.ins().call(print_str_ref, &[end_ptr]);
        }

        if has_styling {
            let reset_ptr =
                Self::get_or_create_string_ptr(module, format_strings, builder, "\x1b[0m")?;
            builder.ins().call(print_str_ref, &[reset_ptr]);
        }

        if opts.flush {
            builder.ins().call(print_flush_ref, &[]);
        }

        Ok(())
    }

    fn global_type_size(ty: crate::backends::common::lir::LirType) -> usize {
        use crate::backends::common::lir::LirType;
        match ty {
            LirType::I8 | LirType::U8 | LirType::Bool => 1,
            LirType::I16 | LirType::U16 => 2,
            LirType::I32 | LirType::U32 | LirType::F32 => 4,
            LirType::I64 | LirType::U64 | LirType::F64 | LirType::Ptr => 8,
            LirType::I128 | LirType::U128 => 16,
            LirType::Void => 0,
        }
    }

    fn global_clif_type(ty: crate::backends::common::lir::LirType) -> types::Type {
        use crate::backends::common::lir::LirType;
        match ty {
            LirType::I8 | LirType::U8 | LirType::Bool => types::I8,
            LirType::I16 | LirType::U16 => types::I16,
            LirType::I32 | LirType::U32 => types::I32,
            LirType::I64 | LirType::U64 | LirType::Ptr => types::I64,
            LirType::I128 | LirType::U128 => types::I128,
            LirType::F32 => types::F32,
            LirType::F64 => types::F64,
            LirType::Void => types::I64,
        }
    }

    // ========================================================================
    // Runtime bridge call helpers
    // ========================================================================

    /// Call a runtime function with no arguments, returns u64
    fn call_runtime_fn_0(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let call_inst = builder.ins().call(func_ref, &[]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with one u64 argument, returns u64
    fn call_runtime_fn_1(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let call_inst = builder.ins().call(func_ref, &[arg0]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with two u64 arguments, returns u64
    fn call_runtime_fn_2(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let arg1 = Self::to_i64_unsigned(builder, arg1);
        let call_inst = builder.ins().call(func_ref, &[arg0, arg1]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with u64, f64 arguments, returns u64
    fn call_runtime_fn_2_f64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let call_inst = builder.ins().call(func_ref, &[arg0, arg1]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with u64, f32 arguments, returns u64
    fn call_runtime_fn_2_f32(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let call_inst = builder.ins().call(func_ref, &[arg0, arg1]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with f64 value and i64 newline flag
    #[allow(dead_code)]
    fn call_print_f64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::F64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        // Use a unique name to avoid conflicts
        let fn_name = "jit_print_f64";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let newline = Self::to_i64_unsigned(builder, newline);
        let call_inst = builder.ins().call(func_ref, &[value, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with i64 value and i64 newline flag
    #[allow(dead_code)]
    fn call_print_i64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        // Use a unique name to avoid conflicts
        let fn_name = "jit_print_i64";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let newline = Self::to_i64_unsigned(builder, newline);
        let call_inst = builder.ins().call(func_ref, &[value, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with u64 value bits and i64 newline flag
    #[allow(dead_code)]
    fn call_print_u64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let fn_name = "jit_print_u64";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let newline = Self::to_i64_unsigned(builder, newline);
        let call_inst = builder.ins().call(func_ref, &[value, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with bool value and i64 newline flag
    #[allow(dead_code)]
    fn call_print_bool(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let fn_name = "jit_print_bool";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let call_inst = builder.ins().call(func_ref, &[value, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with string pointer and i64 newline flag
    #[allow(dead_code)]
    fn call_print_str(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        str_ptr: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        // Use a unique name to avoid conflicts
        let fn_name = "jit_print_str_raw";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let str_ptr = Self::to_i64_unsigned(builder, str_ptr);
        let newline = Self::to_i64_unsigned(builder, newline);
        let call_inst = builder.ins().call(func_ref, &[str_ptr, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call print function with object handle and i64 newline flag
    #[allow(dead_code)]
    fn call_print_object(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        handle: Value,
        newline: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        // Use a unique name to avoid conflicts
        let fn_name = "jit_print_object";
        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let handle = Self::to_i64_unsigned(builder, handle);
        let newline = Self::to_i64_unsigned(builder, newline);
        let call_inst = builder.ins().call(func_ref, &[handle, newline]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with three u64 arguments, returns u64
    fn call_runtime_fn_3(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
        arg2: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let arg1 = Self::to_i64_unsigned(builder, arg1);
        let arg2 = Self::to_i64_unsigned(builder, arg2);
        let call_inst = builder.ins().call(func_ref, &[arg0, arg1, arg2]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with u64, u64, f64 arguments, returns u64
    fn call_runtime_fn_3_f64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
        arg2: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let arg0 = Self::to_i64_unsigned(builder, arg0);
        let arg1 = Self::to_i64_unsigned(builder, arg1);
        let call_inst = builder.ins().call(func_ref, &[arg0, arg1, arg2]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call a runtime function with 7 u64 arguments, returns u64
    fn call_runtime_fn_7(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        fn_name: &str,
        arg0: Value,
        arg1: Value,
        arg2: Value,
        arg3: Value,
        arg4: Value,
        arg5: Value,
        arg6: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        for _ in 0..7 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function(fn_name, Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare {}: {}", fn_name, e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let a0 = Self::to_i64_unsigned(builder, arg0);
        let a1 = Self::to_i64_unsigned(builder, arg1);
        let a2 = Self::to_i64_unsigned(builder, arg2);
        let a3 = Self::to_i64_unsigned(builder, arg3);
        let a4 = Self::to_i64_unsigned(builder, arg4);
        let a5 = Self::to_i64_unsigned(builder, arg5);
        let a6 = Self::to_i64_unsigned(builder, arg6);
        let call_inst = builder.ins().call(func_ref, &[a0, a1, a2, a3, a4, a5, a6]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_print_value to print a handle with pretty mode
    fn call_print_value(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        handle: Value,
        mode: i64,
    ) -> Result<(), String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_print_value", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_print_value: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let mode_val = builder.ins().iconst(types::I64, mode);
        builder.ins().call(func_ref, &[handle, mode_val]);
        Ok(())
    }

    /// Call jit_print_with_options to print a handle using runtime options object
    #[allow(dead_code)]
    fn call_print_with_options(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        handle: Value,
        options_handle: Value,
    ) -> Result<(), String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_print_with_options", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_print_with_options: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let handle = Self::to_i64_unsigned(builder, handle);
        let options_handle = Self::to_i64_unsigned(builder, options_handle);
        builder.ins().call(func_ref, &[handle, options_handle]);
        Ok(())
    }

    /// Call jit_wrap_i64 to wrap an i64 value in a RuntimeValue handle
    fn call_wrap_i64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_i64", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_i64: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let call_inst = builder.ins().call(func_ref, &[value]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_wrap_f64 to wrap an f64 value in a RuntimeValue handle
    fn call_wrap_f64(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_f64", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_f64: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let val_f64 = if builder.func.dfg.value_type(value) == types::F64 {
            value
        } else if builder.func.dfg.value_type(value) == types::F32 {
            builder.ins().fpromote(types::F64, value)
        } else if builder.func.dfg.value_type(value).is_int() {
            builder.ins().bitcast(types::F64, MemFlags::new(), value)
        } else {
            builder.ins().f64const(0.0)
        };

        let call_inst = builder.ins().call(func_ref, &[val_f64]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_wrap_f32 to wrap an f32 value in a RuntimeValue handle
    fn call_wrap_f32(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_f32", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_f32: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let val_f64 = if builder.func.dfg.value_type(value) == types::F64 {
            value
        } else if builder.func.dfg.value_type(value) == types::F32 {
            builder.ins().fpromote(types::F64, value)
        } else if builder.func.dfg.value_type(value).is_int() {
            builder.ins().bitcast(types::F64, MemFlags::new(), value)
        } else {
            builder.ins().f64const(0.0)
        };

        let call_inst = builder.ins().call(func_ref, &[val_f64]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_wrap_bool to wrap a bool value in a RuntimeValue handle
    fn call_wrap_bool(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_bool", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_bool: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let call_inst = builder.ins().call(func_ref, &[value]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_wrap_null to create a RuntimeValue::Null handle
    fn call_wrap_null(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_null", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_null: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let call_inst = builder.ins().call(func_ref, &[]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Call jit_wrap_str to wrap a string pointer in a RuntimeValue handle
    fn call_wrap_str(
        builder: &mut FunctionBuilder,
        module: &mut JITModule,
        value: Value,
    ) -> Result<Value, String> {
        let mut sig = module.make_signature();
        sig.call_conv = CallConv::triple_default(module.isa().triple());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("jit_wrap_str", Linkage::Import, &sig)
            .map_err(|e| format!("Failed to declare jit_wrap_str: {}", e))?;
        let func_ref = module.declare_func_in_func(func_id, builder.func);

        let value = Self::to_i64_unsigned(builder, value);
        let call_inst = builder.ins().call(func_ref, &[value]);
        let results = builder.inst_results(call_inst);
        Ok(results[0])
    }

    /// Define Phi incoming variables for block control transitions
    fn define_phi_incoming_vars(
        builder: &mut FunctionBuilder,
        value_map: &HashMap<ValueId, Value>,
        phi_vars: &HashMap<ValueId, (Variable, Variable)>,
        func: &LirFunction,
        current_block_id: BlockId,
        target_block_id: BlockId,
    ) {
        if let Some(target_lir_block) = func.blocks.iter().find(|b| b.id == target_block_id) {
            for inst in &target_lir_block.instructions {
                if let LirInst::Phi(dst, sources) = inst {
                    if let Some(&(_, src_val_id)) = sources
                        .iter()
                        .find(|(pred_id, _)| *pred_id == current_block_id)
                    {
                        if let Some(&src_val) = value_map.get(&src_val_id) {
                            if let Some(&(v_i64, v_f64)) = phi_vars.get(dst) {
                                let ty = builder.func.dfg.value_type(src_val);
                                if ty == types::F64 || ty == types::F32 {
                                    let src_val_converted = if ty == types::F32 {
                                        builder.ins().fpromote(types::F64, src_val)
                                    } else {
                                        src_val
                                    };
                                    builder.def_var(v_f64, src_val_converted);
                                } else {
                                    let src_val_converted = Self::to_i64_unsigned(builder, src_val);
                                    builder.def_var(v_i64, src_val_converted);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
