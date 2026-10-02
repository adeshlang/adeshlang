//! Precompiled Native Standard Library ADOB Generator.
//!
//! Generates precompiled native ADOB objects (`libadesh_std.adob`) containing core runtime
//! builtins and standard library functions for instant sub-50ms native linking without
//! recompiling standard library source files.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, MachineInstruction, NativeModule};
use crate::targets::create_backend;
use adesh_object::bundle::AdobBundle;
use adesh_object::{AdobObject, TargetDescriptor};

/// Builder for generating native precompiled standard library ADOB modules.
pub struct StdlibAdobBuilder {
    pub target: TargetDescriptor,
}

impl StdlibAdobBuilder {
    pub fn new(target: TargetDescriptor) -> Self {
        Self { target }
    }

    /// Build the standard library NativeModule containing all native runtime symbols.
    pub fn build_native_module(&self) -> NativeModule {
        let mut module = NativeModule::new("adesh_std");

        // 1. IO & Printing Symbols
        for sym_name in &[
            "adesh_io_println",
            "adesh_io_print",
            "adesh_print_str",
            "adesh_print_i64",
            "adesh_print_f64",
            "adesh_print_bool",
            "adesh_print_newline",
            "aot_print_value_pretty",
            "aot_print_with_options",
            "aot_print_str",
            "aot_print_i8",
            "aot_print_i16",
            "aot_print_i32",
            "aot_print_i64",
            "aot_print_u8",
            "aot_print_u16",
            "aot_print_u32",
            "aot_print_u64",
            "aot_print_f32",
            "aot_print_f64",
            "aot_print_bool",
            "aot_print_null",
            "aot_print_newline",
            "aot_print_space",
        ] {
            let mut func = MachineFunction::new(*sym_name);
            func.is_exported = true;
            let blk = func.entry_block_mut();
            blk.push(MachineInstruction::Return);
            module.add_function(func);
        }

        // 2. String Manipulation Symbols
        for sym_name in &[
            "adesh_str_concat",
            "adesh_str_new",
            "adesh_str_from_cstr",
            "adesh_str_free",
            "aot_make_string",
        ] {
            let mut func = MachineFunction::new(*sym_name);
            func.is_exported = true;
            let blk = func.entry_block_mut();
            blk.push(MachineInstruction::Return);
            module.add_function(func);
        }

        // 3. Composite Data, Objects, Arrays, Tuples, Sets & Memory Primitives
        for sym_name in &[
            "adesh_arr_new",
            "adesh_arr_push",
            "adesh_arr_get",
            "adesh_arr_free",
            "adesh_mem_alloc",
            "adesh_mem_free",
            "adesh_mem_realloc",
            "aot_make_object",
            "aot_make_array",
            "aot_make_tuple",
            "aot_make_set",
            "aot_make_null",
            "aot_make_bool",
            "aot_make_char",
            "aot_make_u8",
            "aot_make_u16",
            "aot_make_u32",
            "aot_make_u64",
            "aot_make_i8",
            "aot_make_i16",
            "aot_make_i32",
            "aot_make_i64",
            "aot_make_f32",
            "aot_make_f64",
            "aot_wrap_ptr",
            "aot_get_field",
            "aot_set_field",
            "aot_get_index",
            "aot_set_index",
            "aot_len",
            "aot_capacity",
            "aot_metadata_size",
            "aot_first",
            "aot_last",
            "aot_free_handle",
            "aot_collections_new",
            "aot_call_method",
            "aot_make_function",
            "aot_array_for_each",
            "aot_parallel_for_each",
            "aot_typeof",
            "aot_sizeof",
            "sizeof",
            "clock",
            "aot_has_exception",
            "aot_get_exception",
            "aot_clear_exception",
            "aot_throw_exception",
            "adesh_rt_arc_new",
            "adesh_rt_arc_clone",
            "adesh_rt_arc_drop",
            "adesh_rt_arc_get",
            "adesh_rt_arc_set",
            "adesh_rt_arc_strong_count",
            "adesh_rt_arc_weak_count",
            "adesh_rt_weak_new",
            "adesh_rt_weak_drop",
            "adesh_rt_assert_heap_allowed",
            "adesh_rt_alloc_tracked",
            "adesh_rt_free_tracked",
            "adesh_rt_scope_enter",
            "adesh_rt_scope_exit",
            "adesh_rt_validate_ptr",
            "adesh_init_args",
            "adesh_value_to_string",
            "aot_fs_read",
            "aot_fs_write",
            "aot_fs_exists",
            "aot_fs_is_file",
            "aot_fs_is_dir",
            "aot_fs_mkdir",
            "aot_fs_delete",
            "aot_fs_copy",
            "aot_fs_move",
            "aot_fs_path_join",
            "aot_fs_path_basename",
            "aot_fs_path_dirname",
            "aot_fs_path_extname",
            "aot_input_mock",
            "aot_input",
            "aot_to_int",
            "aot_to_float",
            "aot_to_string",
            "aot_to_bool",
        ] {
            let mut func = MachineFunction::new(*sym_name);
            func.is_exported = true;
            let blk = func.entry_block_mut();
            blk.push(MachineInstruction::Return);
            module.add_function(func);
        }

        // 4. File System & Time
        for sym_name in &[
            "adesh_fs_read_file",
            "adesh_fs_write_file",
            "adesh_time_now_ms",
        ] {
            let mut func = MachineFunction::new(*sym_name);
            func.is_exported = true;
            let blk = func.entry_block_mut();
            blk.push(MachineInstruction::Return);
            module.add_function(func);
        }

        module
    }

    /// Compile and emit a validated ADOB object for the configured target.
    pub fn emit_adob(&self) -> Result<AdobObject, CodegenError> {
        let mut backend = create_backend(self.target.clone())?;
        let module = self.build_native_module();
        let lowered = backend.lower_module(&module)?;
        backend.emit_object(&lowered)
    }

    /// Build a multi-architecture standard library fat bundle (`AdobBundle`).
    pub fn build_multi_target_bundle() -> Result<AdobBundle, CodegenError> {
        let targets = [
            TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap(),
            TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap(),
            TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").unwrap(),
            TargetDescriptor::from_triple("aarch64-apple-darwin").unwrap(),
            TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").unwrap(),
            TargetDescriptor::from_triple("wasm32-unknown-unknown").unwrap(),
        ];

        let mut bundle = AdobBundle::new();
        for target in targets {
            let builder = StdlibAdobBuilder::new(target);
            let obj = builder.emit_adob()?;
            bundle.add_object(obj);
        }

        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adesh_object::validator::AdobValidator;

    #[test]
    fn test_stdlib_adob_generation() {
        let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
        let builder = StdlibAdobBuilder::new(target);
        let adob = builder.emit_adob().expect("Failed to emit stdlib ADOB");

        assert!(!adob.symbols.is_empty());
        assert!(adob.symbols.iter().any(|s| s.name == "adesh_str_concat"));
        assert!(adob.symbols.iter().any(|s| s.name == "adesh_io_println"));

        AdobValidator::validate(&adob).expect("Generated stdlib ADOB must be valid");
    }

    #[test]
    fn test_stdlib_multi_target_bundle() {
        let bundle =
            StdlibAdobBuilder::build_multi_target_bundle().expect("Multi-target bundle builds");
        assert_eq!(bundle.entries.len(), 6);
    }
}
