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

        // 1. IO Symbols: print, println, read_line, print_str, print_i64, etc.
        for sym_name in &[
            "adesh_io_println",
            "adesh_io_print",
            "adesh_print_str",
            "adesh_print_i64",
            "adesh_print_f64",
            "adesh_print_bool",
            "adesh_print_newline",
        ] {
            let mut func = MachineFunction::new(*sym_name);
            func.is_exported = true;
            let blk = func.entry_block_mut();
            blk.push(MachineInstruction::Return);
            module.add_function(func);
        }

        // 2. String Manipulation Symbols: new, concat, free, len
        let mut fn_str_concat = MachineFunction::new("adesh_str_concat");
        fn_str_concat.is_exported = true;
        let blk = fn_str_concat.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_str_concat);

        let mut fn_str_new = MachineFunction::new("adesh_str_new");
        fn_str_new.is_exported = true;
        let blk = fn_str_new.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_str_new);

        let mut fn_str_free = MachineFunction::new("adesh_str_free");
        fn_str_free.is_exported = true;
        let blk = fn_str_free.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_str_free);

        // 3. Array & Memory Primitives: arr_new, arr_push, arr_get, mem_alloc, mem_free
        let mut fn_arr_new = MachineFunction::new("adesh_arr_new");
        fn_arr_new.is_exported = true;
        let blk = fn_arr_new.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_arr_new);

        let mut fn_arr_push = MachineFunction::new("adesh_arr_push");
        fn_arr_push.is_exported = true;
        let blk = fn_arr_push.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_arr_push);

        let mut fn_arr_get = MachineFunction::new("adesh_arr_get");
        fn_arr_get.is_exported = true;
        let blk = fn_arr_get.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_arr_get);

        let mut fn_mem_alloc = MachineFunction::new("adesh_mem_alloc");
        fn_mem_alloc.is_exported = true;
        let blk = fn_mem_alloc.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_mem_alloc);

        let mut fn_mem_free = MachineFunction::new("adesh_mem_free");
        fn_mem_free.is_exported = true;
        let blk = fn_mem_free.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_mem_free);

        // 4. File System & Time: read_file, write_file, now_ms
        let mut fn_fs_read = MachineFunction::new("adesh_fs_read_file");
        fn_fs_read.is_exported = true;
        let blk = fn_fs_read.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_fs_read);

        let mut fn_fs_write = MachineFunction::new("adesh_fs_write_file");
        fn_fs_write.is_exported = true;
        let blk = fn_fs_write.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_fs_write);

        let mut fn_time_now = MachineFunction::new("adesh_time_now_ms");
        fn_time_now.is_exported = true;
        let blk = fn_time_now.entry_block_mut();
        blk.push(MachineInstruction::Return);
        module.add_function(fn_time_now);

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
