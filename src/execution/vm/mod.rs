//! Bytecode Virtual Machines
//!
//! Interprets the custom bytecode formats produced by `execution::bytecode`:
//! - v1 (stack machine): simple push/pop semantics with a small opcode set
//! - v2 (register machine): fixed register file with basic arithmetic, jumps, and calls
//!
//! The VM exposes `run_file` and `run_file_with_writer` for testing and tooling.
//! Values supported: numbers, strings, null, and `BigInt` in v2.

mod api;
mod v1_stack;
mod v2_register;
mod values;

// Re-export public API
pub use api::{run_file, run_file_with_args, run_file_with_writer, run_file_with_writer_and_args};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::bytecode::write_module;
    use crate::execution::bytecode::{Constant, OpCode};

    #[test]
    fn vm_runs_placeholder() {
        let tmp = std::env::temp_dir().join("india-vm-test.bin");
        let consts = vec![Constant::Str("hello from bc".to_string())];
        // code: LOAD_CONST 0, PRINT, HALT
        let mut code: Vec<u8> = Vec::new();
        code.push(OpCode::LoadConst as u8);
        code.extend(&0u32.to_le_bytes());
        code.push(OpCode::Print as u8);
        code.push(OpCode::Halt as u8);
        let module = crate::execution::bytecode::BytecodeModule {
            consts,
            globals: Vec::new(),
            code,
        };
        write_module(&module, &tmp).unwrap();
        assert!(run_file(&tmp).is_ok());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn vm_store_and_load_global_print() {
        let tmp = std::env::temp_dir().join("india-vm-store-global.bin");
        // single constant string
        let consts = vec![Constant::Str("greeting".to_string())];
        // globals: one name
        let globals = vec!["g".to_string()];
        // code: LOAD_CONST 0, STORE_GLOBAL 0, LOAD_GLOBAL 0, PRINT, HALT
        let mut code: Vec<u8> = Vec::new();
        code.push(OpCode::LoadConst as u8);
        code.extend(&0u32.to_le_bytes());
        code.push(OpCode::StoreGlobal as u8);
        code.extend(&0u32.to_le_bytes());
        code.push(OpCode::LoadGlobal as u8);
        code.extend(&0u32.to_le_bytes());
        code.push(OpCode::Print as u8);
        code.push(OpCode::Halt as u8);

        let module = crate::execution::bytecode::BytecodeModule {
            consts,
            globals,
            code,
        };
        write_module(&module, &tmp).unwrap();
        // capture output
        let mut out_buf: Vec<u8> = Vec::new();
        run_file_with_writer(&tmp, &mut out_buf).expect("run failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        let _ = std::fs::remove_file(&tmp);
        assert!(output.contains("greeting"));
    }

    #[test]
    fn compile_and_run_print_expr() {
        use crate::execution::bytecode::compile_to_file;
        // small source: print(1 + 2);
        let src = "print(1 + 2);";
        let tmp = std::env::temp_dir().join("india-vm-compile-test.bin");
        // compile
        compile_to_file(src, &tmp).expect("compile failed");
        // capture VM output into a buffer
        let mut out_buf: Vec<u8> = Vec::new();
        run_file_with_writer(&tmp, &mut out_buf).expect("run failed");
        let output = String::from_utf8_lossy(&out_buf).to_string();
        // cleanup
        let _ = std::fs::remove_file(&tmp);
        // the VM prints the numeric result; accept either '3' or '3.0' in output
        assert!(output.contains('3'));
    }
}
