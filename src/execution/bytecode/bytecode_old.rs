//! Bytecode Compiler
//!
//! Implements two minimal compilers from language AST to a custom bytecode:
//! - v1: stack-based format (`MAGIC`), simple opcodes like `LOAD_CONST`, `PRINT`, `ADD`
//! - v2: register-based format (`MAGIC_V2`), with `ROp` instructions and basic control flow
//!
//! The formats are designed for educational clarity and ease of disassembly.
//! Both compilers share constant pools and global name tables. The v2 encoder
//! supports small functions with direct calls and returns.
#![allow(dead_code, unused_variables, unused_assignments)]
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::parsing::ast::*;
use crate::parsing::error::{ErrorKind, LangError};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;

#[derive(Clone, Debug)]
pub enum Constant {
    Number(f64),
    Str(String),
    Null,
}

/// Simple bytecode format:
/// [MAGIC="Adesh-BC\n"][u8 version]
/// [u32 const_count]
/// for each const: [u32 len][bytes]
/// [u32 code_len][bytes]
/// Instructions: opcode (u8) followed by optional operands (u32 LE)

const MAGIC: &[u8] = b"Adesh-BC\n";
const VERSION: u8 = 1;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    LoadConst = 1,
    Print = 2,
    Add = 3,
    LoadGlobal = 4,
    StoreGlobal = 5,
    Call = 6,
    Return = 7,
    /// Call a foreign function by symbol name. Encoding (stack VM v1):
    /// u8 opcode, u32 const_index (string symbol), u32 arg_count. Caller pushes args.
    CallFfi = 8,
    /// Allocate memory: ALLOC, pops size from stack, pushes pointer
    Alloc = 9,
    /// Free memory: FREE, pops pointer from stack
    Free = 10,
    /// Load from pointer: PTR_LOAD, pops offset and pointer, pushes value
    PtrLoad = 11,
    /// Store to pointer: PTR_STORE, pops value, offset, and pointer
    PtrStore = 12,
    /// Push a defer block onto the defer stack. Encoding: u8 opcode, u32 block_offset, u32 block_len
    DeferPush = 13,
    /// Execute all defers in LIFO order for the current scope
    DeferRun = 14,
    /// Call a decorated function with pipeline execution
    /// Encoding: u8 opcode, u32 fn_index, u32 pipeline_index
    CallDecorated = 15,
    // Arithmetic operations
    Sub = 16,
    Mul = 17,
    Div = 18,
    Mod = 19,
    // Comparison operations
    CmpLT = 20,
    CmpLE = 21,
    CmpGT = 22,
    CmpGE = 23,
    CmpEQ = 24,
    CmpNE = 25,
    Halt = 255,
}

impl OpCode {
    pub fn from_u8(v: u8) -> Option<OpCode> {
        match v {
            1 => Some(OpCode::LoadConst),
            2 => Some(OpCode::Print),
            3 => Some(OpCode::Add),
            4 => Some(OpCode::LoadGlobal),
            5 => Some(OpCode::StoreGlobal),
            6 => Some(OpCode::Call),
            7 => Some(OpCode::Return),
            8 => Some(OpCode::CallFfi),
            9 => Some(OpCode::Alloc),
            10 => Some(OpCode::Free),
            11 => Some(OpCode::PtrLoad),
            12 => Some(OpCode::PtrStore),
            13 => Some(OpCode::DeferPush),
            14 => Some(OpCode::DeferRun),
            15 => Some(OpCode::CallDecorated),
            16 => Some(OpCode::Sub),
            17 => Some(OpCode::Mul),
            18 => Some(OpCode::Div),
            19 => Some(OpCode::Mod),
            20 => Some(OpCode::CmpLT),
            21 => Some(OpCode::CmpLE),
            22 => Some(OpCode::CmpGT),
            23 => Some(OpCode::CmpGE),
            24 => Some(OpCode::CmpEQ),
            25 => Some(OpCode::CmpNE),
            255 => Some(OpCode::Halt),
            _ => None,
        }
    }
}

pub struct BytecodeModule {
    pub consts: Vec<Constant>,
    pub globals: Vec<String>,
    pub code: Vec<u8>,
}

pub fn compile_to_file(src: &str, out: &Path) -> Result<(), LangError> {
    let mut lx = Lexer::new(src);
    let toks = lx.tokenize()?;
    let mut p = Parser::new(toks, None);
    let prog = p.parse_program()?;
    let prog = crate::parsing::ast_optimizer::optimize_program(&prog);

    let mut ctx = EmitCtx::new();
    // collect top-level globals (let bindings)
    for s in &prog {
        if let StmtKind::Let(name, _init, _type_ann, _export, _is_const, _is_readonly) = &s.kind {
            ctx.add_global(name.clone());
        } else if let StmtKind::ShareDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        } else if let StmtKind::StrongDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        } else if let StmtKind::WeakDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        }
    }

    for s in &prog {
        emit_stmt(s, &mut ctx).map_err(|m| LangError::new(ErrorKind::Type, m, 0, 0, "".into()))?;
    }

    // Emit all registered defer blocks in LIFO order at program end (compile-time inlining)
    while let Some(defer_block) = ctx.defer_blocks.pop() {
        emit_stmt(&defer_block, &mut ctx)
            .map_err(|m| LangError::new(ErrorKind::Type, m, 0, 0, "".into()))?;
    }

    let module = BytecodeModule {
        consts: ctx.consts,
        globals: ctx.globals,
        code: ctx.code,
    };
    write_module(&module, out)
}

pub fn write_module(m: &BytecodeModule, out: &Path) -> Result<(), LangError> {
    let mut f = File::create(out)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(MAGIC)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(&[VERSION])
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    // write typed constants
    let cc = m.consts.len() as u32;
    f.write_all(&cc.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    for c in &m.consts {
        match c {
            // representation: tag 0 = Number, 1 = String, 2 = Null
            Constant::Number(n) => {
                f.write_all(&[0u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                f.write_all(&n.to_le_bytes())
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
            Constant::Str(s) => {
                f.write_all(&[1u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                let bytes = s.as_bytes();
                let l = bytes.len() as u32;
                f.write_all(&l.to_le_bytes())
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                f.write_all(bytes)
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
            Constant::Null => {
                f.write_all(&[2u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
        }
    }
    // write globals
    let gc = m.globals.len() as u32;
    f.write_all(&gc.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    for g in &m.globals {
        let bytes = g.as_bytes();
        let l = bytes.len() as u32;
        f.write_all(&l.to_le_bytes())
            .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
        f.write_all(bytes)
            .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    }
    let cl = m.code.len() as u32;
    f.write_all(&cl.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(&m.code)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    Ok(())
}
pub fn disassemble_file(path: &Path) -> Result<(), LangError> {
    disassemble_file_internal(path, None)
}

pub fn disassemble_file_to_file(path: &Path, out_path: Option<&Path>) -> Result<(), LangError> {
    disassemble_file_internal(path, out_path)
}

fn disassemble_file_internal(path: &Path, out_path: Option<&Path>) -> Result<(), LangError> {
    let mut data = Vec::new();
    let mut f = File::open(path)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.read_to_end(&mut data)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;

    let mut output = String::new();

    if data.starts_with(MAGIC) {
        let mut idx = MAGIC.len();
        let version = data[idx];
        idx += 1;
        output.push_str(&format!("Language bytecode v{} - disassembly\n", version));
        let const_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        output.push_str(&format!("Constants ({}):\n", const_count));
        let mut consts: Vec<Constant> = Vec::new();
        for _ in 0..const_count {
            let tag = data[idx];
            idx += 1;
            match tag {
                0 => {
                    let n = f64::from_le_bytes(data[idx..idx + 8].try_into().unwrap());
                    idx += 8;
                    consts.push(Constant::Number(n));
                }
                1 => {
                    let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                    idx += 4;
                    let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
                    idx += l;
                    consts.push(Constant::Str(s));
                }
                2 => {
                    consts.push(Constant::Null);
                }
                _ => {
                    return Err(LangError::new(
                        ErrorKind::Io,
                        format!("unknown const tag {}", tag),
                        0,
                        0,
                        "".into(),
                    ));
                }
            }
        }
        for (i, c) in consts.iter().enumerate() {
            match c {
                Constant::Number(n) => output.push_str(&format!("  [{}] Number({})\n", i, n)),
                Constant::Str(s) => output.push_str(&format!(
                    "  [{}] Str({})\n",
                    i,
                    s.lines().next().unwrap_or("")
                )),
                Constant::Null => output.push_str(&format!("  [{}] Null\n", i)),
            }
        }
        let glob_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        output.push_str(&format!("Globals ({}):\n", glob_count));
        let mut globals = Vec::new();
        for _ in 0..glob_count {
            let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
            idx += 4;
            let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
            idx += l;
            globals.push(s);
        }
        for (i, g) in globals.iter().enumerate() {
            output.push_str(&format!("  [{}] {}\n", i, g));
        }
        let code_len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        let code_end = idx + code_len;
        output.push_str("Instructions:\n");
        let mut pc = 0usize;
        while idx < code_end {
            let op = data[idx];
            idx += 1;
            pc += 1;
            if let Some(opc) = OpCode::from_u8(op) {
                match opc {
                    OpCode::LoadConst => {
                        let operand =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        output.push_str(&format!("  {:04} LOAD_CONST {}\n", pc - 5, operand));
                    }
                    OpCode::Print => output.push_str(&format!("  {:04} PRINT\n", pc - 1)),
                    OpCode::Add => output.push_str(&format!("  {:04} ADD\n", pc - 1)),
                    OpCode::LoadGlobal => {
                        let operand =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        output.push_str(&format!("  {:04} LOAD_GLOBAL {}\n", pc - 5, operand));
                    }
                    OpCode::StoreGlobal => {
                        let operand =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        output.push_str(&format!("  {:04} STORE_GLOBAL {}\n", pc - 5, operand));
                    }
                    OpCode::Call => {
                        let operand =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        output.push_str(&format!("  {:04} CALL {}\n", pc - 5, operand));
                    }
                    OpCode::Return => output.push_str(&format!("  {:04} RETURN\n", pc - 1)),
                    OpCode::CallFfi => {
                        let sym_idx =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        let argc =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 4;
                        output.push_str(&format!(
                            "  {:04} CALL_FFI k{} argc={}\n",
                            pc - 9,
                            sym_idx,
                            argc
                        ));
                    }
                    OpCode::Alloc => output.push_str(&format!("  {:04} ALLOC\n", pc - 1)),
                    OpCode::Free => output.push_str(&format!("  {:04} FREE\n", pc - 1)),
                    OpCode::PtrLoad => output.push_str(&format!("  {:04} PTR_LOAD\n", pc - 1)),
                    OpCode::PtrStore => output.push_str(&format!("  {:04} PTR_STORE\n", pc - 1)),
                    OpCode::DeferPush => {
                        let offset = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                        idx += 4;
                        let len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                        idx += 4;
                        pc += 8;
                        output.push_str(&format!(
                            "  {:04} DEFER_PUSH offset={} len={}\n",
                            pc - 9,
                            offset,
                            len
                        ));
                    }
                    OpCode::DeferRun => output.push_str(&format!("  {:04} DEFER_RUN\n", pc - 1)),
                    OpCode::CallDecorated => {
                        let fn_idx =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        let pipeline_idx =
                            u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                        idx += 4;
                        pc += 8;
                        output.push_str(&format!(
                            "  {:04} CALL_DECORATED fn={} pipeline={}\n",
                            pc - 9,
                            fn_idx,
                            pipeline_idx
                        ));
                    }
                    OpCode::Halt => output.push_str(&format!("  {:04} HALT\n", pc - 1)),
                    OpCode::Sub => output.push_str(&format!("  {:04} SUB\n", pc - 1)),
                    OpCode::Mul => output.push_str(&format!("  {:04} MUL\n", pc - 1)),
                    OpCode::Div => output.push_str(&format!("  {:04} DIV\n", pc - 1)),
                    OpCode::Mod => output.push_str(&format!("  {:04} MOD\n", pc - 1)),
                    OpCode::CmpLT => output.push_str(&format!("  {:04} CMP_LT\n", pc - 1)),
                    OpCode::CmpLE => output.push_str(&format!("  {:04} CMP_LE\n", pc - 1)),
                    OpCode::CmpGT => output.push_str(&format!("  {:04} CMP_GT\n", pc - 1)),
                    OpCode::CmpGE => output.push_str(&format!("  {:04} CMP_GE\n", pc - 1)),
                    OpCode::CmpEQ => output.push_str(&format!("  {:04} CMP_EQ\n", pc - 1)),
                    OpCode::CmpNE => output.push_str(&format!("  {:04} CMP_NE\n", pc - 1)),
                }
            } else {
                output.push_str(&format!("  {:04} UNKNOWN_OPCODE {}\n", pc - 1, op));
            }
        }
    } else if data.starts_with(MAGIC_V2) {
        let mut idx = MAGIC_V2.len();
        let version = data[idx];
        idx += 1;
        output.push_str(&format!(
            "Language bytecode v{} (register) - disassembly\n",
            version
        ));
        let const_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        output.push_str(&format!("Constants ({}):\n", const_count));
        let mut consts: Vec<Constant> = Vec::new();
        for _ in 0..const_count {
            let tag = data[idx];
            idx += 1;
            match tag {
                0 => {
                    let n = f64::from_le_bytes(data[idx..idx + 8].try_into().unwrap());
                    idx += 8;
                    consts.push(Constant::Number(n));
                }
                1 => {
                    let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
                    idx += 4;
                    let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
                    idx += l;
                    consts.push(Constant::Str(s));
                }
                2 => {
                    consts.push(Constant::Null);
                }
                _ => {
                    return Err(LangError::new(
                        ErrorKind::Io,
                        format!("unknown const tag {}", tag),
                        0,
                        0,
                        "".into(),
                    ));
                }
            }
        }
        for (i, c) in consts.iter().enumerate() {
            match c {
                Constant::Number(n) => output.push_str(&format!("  [{}] Number({})\n", i, n)),
                Constant::Str(s) => output.push_str(&format!(
                    "  [{}] Str({})\n",
                    i,
                    s.lines().next().unwrap_or("")
                )),
                Constant::Null => output.push_str(&format!("  [{}] Null\n", i)),
            }
        }
        let glob_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        output.push_str(&format!("Globals ({}):\n", glob_count));
        let mut globals = Vec::new();
        for _ in 0..glob_count {
            let l = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
            idx += 4;
            let s = String::from_utf8_lossy(&data[idx..idx + l]).to_string();
            idx += l;
            globals.push(s);
        }
        for (i, g) in globals.iter().enumerate() {
            output.push_str(&format!("  [{}] {}\n", i, g));
        }
        let reg_count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        output.push_str(&format!("Registers: {}\n", reg_count));
        let code_len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        let code_end = idx + code_len;
        output.push_str("Instructions:\n");
        let mut pc = idx;
        while pc < code_end {
            let op = data[pc];
            pc += 1;
            match ROp::from_u8(op) {
                Some(ROp::Move) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  MOVE r{} <- r{}\n", dst, src));
                }
                Some(ROp::LoadConst) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  LOAD_CONST r{} <- k{}\n", dst, k));
                }
                Some(ROp::LoadGlobal) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  LOAD_GLOBAL r{} <- g{}\n", dst, g));
                }
                Some(ROp::StoreGlobal) => {
                    let g = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  STORE_GLOBAL g{} <- r{}\n", g, src));
                }
                Some(ROp::Add) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  ADD r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Sub) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  SUB r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Mul) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  MUL r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Div) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  DIV r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Mod) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  MOD r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Print) => {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  PRINT r{}\n", r));
                }
                Some(ROp::PrintNewline) => {
                    output.push_str("  PRINT_NEWLINE\n");
                }
                Some(ROp::PrintSpace) => {
                    output.push_str("  PRINT_SPACE\n");
                }
                Some(ROp::Jump) => {
                    let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  JUMP +{}\n", rel));
                }
                Some(ROp::JumpIfTrue) => {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  JUMP_IF_TRUE r{}, +{}\n", r, rel));
                }
                Some(ROp::JumpIfFalse) => {
                    let r = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let rel = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  JUMP_IF_FALSE r{}, +{}\n", r, rel));
                }
                Some(ROp::JumpAbs) => {
                    let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  JUMP_ABS {}\n", abs));
                }
                Some(ROp::LoadConstBigInt) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let k = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  LOAD_CONST_BIGINT r{} <- k{}\n", dst, k));
                }
                Some(ROp::Clock) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CLOCK r{}\n", dst));
                }
                Some(ROp::CmpLT) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpLT r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::CmpLE) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpLE r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::CmpGT) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpGT r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::CmpGE) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpGE r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::CmpEQ) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpEQ r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::CmpNE) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let a = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let b = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  CmpNE r{} <- r{}, r{}\n", dst, a, b));
                }
                Some(ROp::Call) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let abs = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    output.push_str(&format!("  CALL r{} <- @{}, argc {}\n", dst, abs, argc));
                    for _ in 0..argc {
                        let ar = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                        pc += 4;
                        output.push_str(&format!("        arg r{}\n", ar));
                    }
                }
                Some(ROp::Return) => {
                    let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  RETURN r{}\n", src));
                }
                Some(ROp::CallBuiltin) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let name_idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    output.push_str(&format!(
                        "  CALL_BUILTIN r{} <- k{}, argc {}\n",
                        dst, name_idx, argc
                    ));
                    for _ in 0..argc {
                        let ar = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                        pc += 4;
                        output.push_str(&format!("        arg r{}\n", ar));
                    }
                }
                Some(ROp::EnvRuntimeLoadFile) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let src = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  ENV_RUNTIME_LOAD_FILE r{} <- r{}\n", dst, src));
                }
                Some(ROp::NewObject) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    pc += count * 8;
                    output.push_str(&format!("  NEW_OBJECT r{} ({} fields)\n", dst, count));
                }
                Some(ROp::NewArray) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    pc += count * 4;
                    output.push_str(&format!("  NEW_ARRAY r{} ({} items)\n", dst, count));
                }
                Some(ROp::NewTuple) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    pc += count * 4;
                    output.push_str(&format!("  NEW_TUPLE r{} ({} items)\n", dst, count));
                }
                Some(ROp::GetField) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let obj = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let field = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  GET_FIELD r{} <- r{}.k{}\n", dst, obj, field));
                }
                Some(ROp::SetField) => {
                    let obj = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let field = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let val = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  SET_FIELD r{}.k{} <- r{}\n", obj, field, val));
                }
                Some(ROp::MakeClosure) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let off = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let count = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    pc += count * 4;
                    output.push_str(&format!(
                        "  MAKE_CLOSURE r{} <- @{} (captures={})\n",
                        dst, off, count
                    ));
                }
                Some(ROp::CallMethod) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let obj = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let m = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let argc = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap()) as usize;
                    pc += 4;
                    pc += argc * 4;
                    output.push_str(&format!(
                        "  CALL_METHOD r{} <- r{}.k{} (argc={})\n",
                        dst, obj, m, argc
                    ));
                }
                Some(ROp::GetIndex) => {
                    let dst = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let coll = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  GET_INDEX r{} <- r{}[r{}]\n", dst, coll, idx));
                }
                Some(ROp::SetIndex) => {
                    let coll = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let idx = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let val = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!("  SET_INDEX r{}[r{}] <- r{}\n", coll, idx, val));
                }
                Some(ROp::DeferPush) => {
                    let block_offset = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    let block_len = u32::from_le_bytes(data[pc..pc + 4].try_into().unwrap());
                    pc += 4;
                    output.push_str(&format!(
                        "  DEFER_PUSH offset {}, len {}\n",
                        block_offset, block_len
                    ));
                }
                Some(ROp::DeferRun) => {
                    output.push_str("  DEFER_RUN\n");
                }
                Some(ROp::Halt) => {
                    output.push_str("  HALT\n");
                }
                None => {
                    output.push_str(&format!("  UNKNOWN_OPCODE {}\n", op));
                }
            }
        }
    } else {
        return Err(LangError::new(
            ErrorKind::Io,
            "unknown bytecode format".into(),
            0,
            0,
            "".into(),
        ));
    }

    // Write output
    if let Some(out_file) = out_path {
        std::fs::write(out_file, &output)
            .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    } else {
        print!("{}", output);
    }

    Ok(())
}

struct EmitCtx {
    pub consts: Vec<Constant>,
    idx_map_str: std::collections::HashMap<String, u32>,
    idx_map_num: std::collections::HashMap<u64, u32>,
    pub code: Vec<u8>,
    pub globals: Vec<String>,
    gidx_map: std::collections::HashMap<String, u32>,
    /// Defer blocks registered for compile-time inlining (LIFO at program end).
    defer_blocks: Vec<Stmt>,
}

impl EmitCtx {
    fn new() -> Self {
        Self {
            consts: Vec::new(),
            idx_map_str: std::collections::HashMap::new(),
            idx_map_num: std::collections::HashMap::new(),
            code: Vec::new(),
            globals: Vec::new(),
            gidx_map: std::collections::HashMap::new(),
            defer_blocks: Vec::new(),
        }
    }

    fn add_const_str(&mut self, s: String) -> u32 {
        if let Some(&i) = self.idx_map_str.get(&s) {
            return i;
        }
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Str(s.clone()));
        self.idx_map_str.insert(s, i);
        i
    }

    fn add_const_number(&mut self, n: f64) -> u32 {
        let bits = n.to_bits();
        if let Some(&i) = self.idx_map_num.get(&bits) {
            return i;
        }
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Number(n));
        self.idx_map_num.insert(bits, i);
        i
    }

    fn add_const_null(&mut self) -> u32 {
        // reuse existing null if present
        // search map for tag; easiest: linear scan (nulls are rare)
        for (i, c) in self.consts.iter().enumerate() {
            if matches!(c, Constant::Null) {
                return i as u32;
            }
        }
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Null);
        i
    }

    fn add_global(&mut self, name: String) -> u32 {
        if let Some(&i) = self.gidx_map.get(&name) {
            return i;
        }
        let i = self.globals.len() as u32;
        self.globals.push(name.clone());
        self.gidx_map.insert(name, i);
        i
    }

    fn get_global_idx(&self, name: &str) -> Option<u32> {
        self.gidx_map.get(name).cloned()
    }

    fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }
    fn emit_u32(&mut self, v: u32) {
        self.code.extend(&v.to_le_bytes());
    }
    #[allow(unused)]
    fn patch_u32_at(&mut self, idx: usize, v: u32) {
        let b = v.to_le_bytes();
        if idx + 3 < self.code.len() {
            self.code[idx] = b[0];
            self.code[idx + 1] = b[1];
            self.code[idx + 2] = b[2];
            self.code[idx + 3] = b[3];
        }
    }
}

fn emit_stmt(s: &Stmt, ctx: &mut EmitCtx) -> Result<(), String> {
    match &s.kind {
        StmtKind::ExprStmt(e) => {
            emit_expr(e, ctx)?;
            Ok(())
        }
        StmtKind::Let(name, init, _type_ann, _export, _is_const, _is_readonly) => {
            // initializer then store to global
            let gidx = ctx.add_global(name.clone());
            if let Some(e) = init {
                emit_expr(e, ctx)?;
            } else {
                let null_idx = ctx.add_const_null();
                ctx.emit_u8(OpCode::LoadConst as u8);
                ctx.emit_u32(null_idx);
            }
            ctx.emit_u8(OpCode::StoreGlobal as u8);
            ctx.emit_u32(gidx);
            Ok(())
        }
        StmtKind::ShareDeclaration(decl, _) => {
            let gidx = ctx.add_global(decl.name.clone());
            emit_expr(&decl.expr, ctx)?;
            ctx.emit_u8(OpCode::StoreGlobal as u8);
            ctx.emit_u32(gidx);
            Ok(())
        }
        StmtKind::StrongDeclaration(decl, _) => {
            let gidx = ctx.add_global(decl.name.clone());
            emit_expr(&decl.expr, ctx)?;
            ctx.emit_u8(OpCode::StoreGlobal as u8);
            ctx.emit_u32(gidx);
            Ok(())
        }
        StmtKind::WeakDeclaration(decl, _) => {
            let gidx = ctx.add_global(decl.name.clone());
            emit_expr(&decl.expr, ctx)?;
            ctx.emit_u8(OpCode::StoreGlobal as u8);
            ctx.emit_u32(gidx);
            Ok(())
        }
        StmtKind::ExternFunction(_) | StmtKind::ExternBlock { .. } => {
            // Extern declarations are handled at runtime; no bytecode emission needed
            Ok(())
        }
        StmtKind::Defer(block) => {
            // Compile-time defer inlining: register the block for emission at program end.
            // Zero runtime overhead — no DeferPush/DeferRun opcodes needed.
            ctx.defer_blocks.push((**block).clone());
            Ok(())
        }
        StmtKind::Import { path, alias } => Err(format!(
            "imports are not supported by the emitter/bytecode compiler: import {} as {}. Try running the program (use 'run') or compile a single-file program without imports.",
            path, alias
        )),
        other => Err(format!("unsupported stmt in emitter: {:?}", other)),
    }
}

fn emit_expr(e: &Expr, ctx: &mut EmitCtx) -> Result<(), String> {
    match &e.kind {
        ExprKind::Literal(v) => {
            use Value;
            match v {
                Value::Number(n) => {
                    let idx = ctx.add_const_number(*n);
                    ctx.emit_u8(OpCode::LoadConst as u8);
                    ctx.emit_u32(idx);
                    Ok(())
                }
                Value::Str(st) => {
                    let idx = ctx.add_const_str(st.clone());
                    ctx.emit_u8(OpCode::LoadConst as u8);
                    ctx.emit_u32(idx);
                    Ok(())
                }
                Value::Bool(b) => {
                    // represent booleans as strings "true"/"false" for now
                    let s = if *b { "true".into() } else { "false".into() };
                    let idx = ctx.add_const_str(s);
                    ctx.emit_u8(OpCode::LoadConst as u8);
                    ctx.emit_u32(idx);
                    Ok(())
                }
                Value::Null => {
                    let idx = ctx.add_const_null();
                    ctx.emit_u8(OpCode::LoadConst as u8);
                    ctx.emit_u32(idx);
                    Ok(())
                }
                _ => Err(format!("unsupported literal in emitter: {:?}", v)),
            }
        }
        ExprKind::Variable(name) => {
            if let Some(idx) = ctx.get_global_idx(name) {
                ctx.emit_u8(OpCode::LoadGlobal as u8);
                ctx.emit_u32(idx);
                return Ok(());
            }
            Err(format!("unknown variable in emitter: {}", name))
        }
        ExprKind::Binary(l, op, r) => {
            // Only support '+' -> Add
            emit_expr(l, ctx)?;
            emit_expr(r, ctx)?;
            match op {
                TokenKind::Plus => {
                    ctx.emit_u8(OpCode::Add as u8);
                    Ok(())
                }
                _ => Err(format!("unsupported binary operator in emitter: {:?}", op)),
            }
        }
        ExprKind::Call(callee, args, _type_args) => {
            // only support print(arg...)
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "print" {
                    for a in args {
                        emit_expr(a, ctx)?; // pushes value
                        ctx.emit_u8(OpCode::Print as u8);
                    }
                    return Ok(());
                }
            }
            Err(format!("unsupported call in emitter: {:?}", callee))
        }
        other => Err(format!("unsupported expr in emitter: {:?}", other)),
    }
}

// ===== Register-based bytecode (v2) =====

const MAGIC_V2: &[u8] = b"Adesh-BC2\n";
const VERSION_V2: u8 = 2;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ROp {
    // data movement
    Move = 1,        // dst, src
    LoadConst = 2,   // dst, kidx
    LoadGlobal = 3,  // dst, gidx
    StoreGlobal = 4, // gidx, src
    // arithmetic
    Add = 10, // dst, a, b
    Sub = 11,
    Mul = 12,
    Div = 13,
    Mod = 14,
    // control/print
    Print = 20,           // r
    Jump = 21,            // rel
    JumpIfTrue = 22,      // r, rel
    JumpIfFalse = 23,     // r, rel
    JumpAbs = 24,         // abs
    LoadConstBigInt = 25, // dst, kidx (digits string)
    Clock = 26,           // dst
    DeferPush = 15,       // block_offset, block_len
    DeferRun = 16,        // (no args) - execute all defers in LIFO order
    PrintNewline = 27,    // (no args)
    PrintSpace = 28,      // (no args)
    CmpLT = 40,           // dst, a, b
    CmpLE = 41,
    CmpGT = 42,
    CmpGE = 43,
    CmpEQ = 44,
    CmpNE = 45,
    Call = 60,               // dst, abs, argc, [args]
    Return = 61,             // src
    CallBuiltin = 62,        // dst, name_idx, argc, [args]
    EnvRuntimeLoadFile = 70, // dst, path_reg -> load .env file at runtime and update __env:* globals, return count
    NewObject = 80,          // dst, count, [k_idx, v_reg] * count
    NewArray = 81,           // dst, count, [elem_reg] * count
    NewTuple = 82,           // dst, count, [elem_reg] * count
    GetField = 83,           // dst, obj_reg, field_kidx
    SetField = 84,           // obj_reg, field_kidx, val_reg
    MakeClosure = 85,        // dst, fn_offset, count, [capture_reg] * count
    CallMethod = 86,         // dst, obj_reg, method_kidx, argc, [arg_reg] * argc
    GetIndex = 87,           // dst, coll_reg, idx_reg
    SetIndex = 88,           // coll_reg, idx_reg, val_reg
    Halt = 255,
}

impl ROp {
    pub fn from_u8(v: u8) -> Option<ROp> {
        match v {
            1 => Some(ROp::Move),
            2 => Some(ROp::LoadConst),
            3 => Some(ROp::LoadGlobal),
            4 => Some(ROp::StoreGlobal),
            10 => Some(ROp::Add),
            11 => Some(ROp::Sub),
            12 => Some(ROp::Mul),
            13 => Some(ROp::Div),
            14 => Some(ROp::Mod),
            20 => Some(ROp::Print),
            21 => Some(ROp::Jump),
            22 => Some(ROp::JumpIfTrue),
            23 => Some(ROp::JumpIfFalse),
            24 => Some(ROp::JumpAbs),
            25 => Some(ROp::LoadConstBigInt),
            26 => Some(ROp::Clock),
            15 => Some(ROp::DeferPush),
            16 => Some(ROp::DeferRun),
            40 => Some(ROp::CmpLT),
            41 => Some(ROp::CmpLE),
            42 => Some(ROp::CmpGT),
            43 => Some(ROp::CmpGE),
            44 => Some(ROp::CmpEQ),
            45 => Some(ROp::CmpNE),
            60 => Some(ROp::Call),
            61 => Some(ROp::Return),
            62 => Some(ROp::CallBuiltin),
            70 => Some(ROp::EnvRuntimeLoadFile),
            80 => Some(ROp::NewObject),
            81 => Some(ROp::NewArray),
            82 => Some(ROp::NewTuple),
            83 => Some(ROp::GetField),
            84 => Some(ROp::SetField),
            85 => Some(ROp::MakeClosure),
            86 => Some(ROp::CallMethod),
            87 => Some(ROp::GetIndex),
            88 => Some(ROp::SetIndex),
            255 => Some(ROp::Halt),
            27 => Some(ROp::PrintNewline),
            28 => Some(ROp::PrintSpace),
            _ => None,
        }
    }
}

pub struct RModule {
    pub consts: Vec<Constant>,
    pub globals: Vec<String>,
    pub reg_count: u32,
    pub code: Vec<u8>,
}

fn write_module_v2(m: &RModule, out: &Path) -> Result<(), LangError> {
    let mut f = File::create(out)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(MAGIC_V2)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(&[VERSION_V2])
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    // consts
    let cc = m.consts.len() as u32;
    f.write_all(&cc.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    for c in &m.consts {
        match c {
            Constant::Number(n) => {
                f.write_all(&[0u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                f.write_all(&n.to_le_bytes())
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
            Constant::Str(s) => {
                f.write_all(&[1u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                let bytes = s.as_bytes();
                let l = bytes.len() as u32;
                f.write_all(&l.to_le_bytes())
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
                f.write_all(bytes)
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
            Constant::Null => {
                f.write_all(&[2u8])
                    .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
            }
        }
    }
    // globals
    let gc = m.globals.len() as u32;
    f.write_all(&gc.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    for g in &m.globals {
        let bytes = g.as_bytes();
        let l = bytes.len() as u32;
        f.write_all(&l.to_le_bytes())
            .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
        f.write_all(bytes)
            .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    }
    // reg count
    f.write_all(&m.reg_count.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    // code
    let cl = m.code.len() as u32;
    f.write_all(&cl.to_le_bytes())
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    f.write_all(&m.code)
        .map_err(|e| LangError::new(ErrorKind::Io, e.to_string(), 0, 0, "".into()))?;
    Ok(())
}

struct REmit {
    consts: Vec<Constant>,
    cidx_str: std::collections::HashMap<String, u32>,
    cidx_num: std::collections::HashMap<u64, u32>,
    code: Vec<u8>,
    globals: Vec<String>,
    gidx: std::collections::HashMap<String, u32>,
    next_reg: u32,
    locals: Option<std::collections::HashMap<String, u32>>,
    fun_offsets: std::collections::HashMap<String, u32>,
    current_fun: Option<(String, u32, usize)>,
    // Map variable name -> env file path for lets initialized via envFromFile(path)
    env_file_vars: std::collections::HashMap<String, String>,
    /// Stack of defer scopes for compile-time inlining (zero runtime overhead).
    /// Each scope is a list of defer block AST nodes. At scope exit, defers are
    /// emitted inline in LIFO order — no runtime defer stack, no heap allocation.
    defer_scopes: Vec<Vec<Stmt>>,
    /// Records `defer_scopes.len()` at each loop body entry for break/continue unwinding.
    loop_defer_depths: Vec<usize>,
}

impl REmit {
    fn new() -> Self {
        Self {
            consts: Vec::new(),
            cidx_str: std::collections::HashMap::new(),
            cidx_num: std::collections::HashMap::new(),
            code: Vec::new(),
            globals: Vec::new(),
            gidx: std::collections::HashMap::new(),
            next_reg: 0,
            locals: None,
            fun_offsets: std::collections::HashMap::new(),
            current_fun: None,
            env_file_vars: std::collections::HashMap::new(),
            defer_scopes: Vec::new(),
            loop_defer_depths: Vec::new(),
        }
    }
    fn add_const_str(&mut self, s: String) -> u32 {
        if let Some(i) = self.cidx_str.get(&s) {
            return *i;
        }
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Str(s.clone()));
        self.cidx_str.insert(s, i);
        i
    }
    fn add_const_num(&mut self, n: f64) -> u32 {
        let k = n.to_bits();
        if let Some(i) = self.cidx_num.get(&k) {
            return *i;
        }
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Number(n));
        self.cidx_num.insert(k, i);
        i
    }
    fn add_const_null(&mut self) -> u32 {
        let i = self.consts.len() as u32;
        self.consts.push(Constant::Null);
        i
    }
    fn add_global(&mut self, name: String) -> u32 {
        if let Some(i) = self.gidx.get(&name) {
            return *i;
        }
        let i = self.globals.len() as u32;
        self.globals.push(name.clone());
        self.gidx.insert(name, i);
        i
    }
    fn reg(&mut self) -> u32 {
        let r = self.next_reg;
        self.next_reg += 1;
        r
    }
    fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }
    fn emit_u32(&mut self, v: u32) {
        self.code.extend(&v.to_le_bytes());
    }
    fn patch_u32_at(&mut self, idx: usize, v: u32) {
        let b = v.to_le_bytes();
        if idx + 3 < self.code.len() {
            self.code[idx] = b[0];
            self.code[idx + 1] = b[1];
            self.code[idx + 2] = b[2];
            self.code[idx + 3] = b[3];
        }
    }
    fn set_locals(&mut self, m: std::collections::HashMap<String, u32>) {
        self.locals = Some(m);
    }
    fn clear_locals(&mut self) {
        self.locals = None;
    }
    fn set_current_fun(&mut self, name: String, off: u32, arity: usize) {
        self.current_fun = Some((name, off, arity));
    }
    fn clear_current_fun(&mut self) {
        self.current_fun = None;
    }

    // --- Defer scope management (compile-time inlining, zero runtime overhead) ---

    /// Push a new defer scope (entering a block/function body).
    #[inline]
    fn push_defer_scope(&mut self) {
        self.defer_scopes.push(Vec::new());
    }

    /// Pop the current defer scope, returning its defers for inlining.
    #[inline]
    fn pop_defer_scope(&mut self) -> Vec<Stmt> {
        self.defer_scopes.pop().unwrap_or_default()
    }

    /// Register a defer block in the current scope.
    #[inline]
    fn register_defer(&mut self, block: Stmt) {
        if let Some(scope) = self.defer_scopes.last_mut() {
            scope.push(block);
        }
    }

    /// Emit the current scope's defers in LIFO order (compile-time inlining).
    fn emit_scope_defers(&mut self) -> Result<(), LangError> {
        let defers = self.pop_defer_scope();
        for defer_block in defers.iter().rev() {
            emit_stmt_v2(defer_block, self)?;
        }
        Ok(())
    }

    /// Emit ALL defers from ALL active scopes in LIFO order (for return).
    fn emit_all_defers(&mut self) -> Result<(), LangError> {
        for scope_idx in (0..self.defer_scopes.len()).rev() {
            let defers = std::mem::take(&mut self.defer_scopes[scope_idx]);
            for defer_block in defers.iter().rev() {
                emit_stmt_v2(defer_block, self)?;
            }
        }
        self.defer_scopes.clear();
        Ok(())
    }

    /// Emit defers from all scopes down to `target_depth` (for break/continue).
    fn emit_defers_until_depth(&mut self, target_depth: usize) -> Result<(), LangError> {
        while self.defer_scopes.len() > target_depth {
            let defers = self.defer_scopes.pop().unwrap_or_default();
            for defer_block in defers.iter().rev() {
                emit_stmt_v2(defer_block, self)?;
            }
        }
        Ok(())
    }
}

/// Minimal register lowering for: let, literals, a+b, print
pub fn compile_to_file_v2(src: &str, out: &Path) -> Result<(), LangError> {
    let mut lx = Lexer::new(src);
    let toks = lx.tokenize()?;
    let mut p = Parser::new(toks, None);
    let prog = crate::parsing::ast_optimizer::optimize_program(&p.parse_program()?);
    let mut ctx = REmit::new();

    // First pass: collect globals
    for s in &prog {
        if let StmtKind::Let(name, _init, _ann, _exp, _cst, _readonly) = &s.kind {
            ctx.add_global(name.clone());
        } else if let StmtKind::ShareDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        } else if let StmtKind::StrongDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        } else if let StmtKind::WeakDeclaration(decl, _) = &s.kind {
            ctx.add_global(decl.name.clone());
        }
    }

    // Second pass: emit all statements
    for s in &prog {
        emit_stmt_v2(s, &mut ctx)?;
    }

    // Call user-defined main() after executing all top-level statements
    // We can look up the function offset directly since it has been emitted
    if let Some(&main_offset) = ctx.fun_offsets.get("main") {
        let ret_reg = ctx.reg();
        ctx.emit_u8(ROp::Call as u8);
        ctx.emit_u32(ret_reg);
        ctx.emit_u32(main_offset);
        ctx.emit_u32(0); // argc
    }

    ctx.emit_u8(ROp::Halt as u8);

    let m = RModule {
        consts: ctx.consts,
        globals: ctx.globals,
        reg_count: ctx.next_reg.max(1),
        code: ctx.code,
    };
    write_module_v2(&m, out)
}

fn emit_stmt_v2(s: &Stmt, ctx: &mut REmit) -> Result<(), LangError> {
    match &s.kind {
        StmtKind::Let(name, init, _ann, _exp, _cst, _readonly) => {
            if ctx.locals.is_some() {
                let rslot = ctx.reg();
                let mut lm = ctx.locals.take().unwrap();
                lm.insert(name.clone(), rslot);
                ctx.locals = Some(lm);
                let r = if let Some(e) = init {
                    emit_expr_v2(e, ctx)?
                } else {
                    let k = ctx.add_const_null();
                    let rr = ctx.reg();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rr);
                    ctx.emit_u32(k);
                    rr
                };
                ctx.emit_u8(ROp::Move as u8);
                ctx.emit_u32(rslot);
                ctx.emit_u32(r);
            } else {
                let g = ctx.add_global(name.clone());
                // If init is envFromFile("path"), record mapping so property GETs can be compiled to precomputed globals
                if let Some(e_inner) = init {
                    if let ExprKind::Call(callee, args, _) = &e_inner.kind {
                        if let ExprKind::Variable(cn) = &callee.kind {
                            if cn == "envFromFile" && args.len() == 1 {
                                if let ExprKind::Literal(Value::Str(p)) = &args[0].kind {
                                    ctx.env_file_vars.insert(name.clone(), p.clone());
                                }
                            }
                        }
                    }
                }
                let r = if let Some(e) = init {
                    emit_expr_v2(e, ctx)?
                } else {
                    let k = ctx.add_const_null();
                    let rr = ctx.reg();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rr);
                    ctx.emit_u32(k);
                    rr
                };
                ctx.emit_u8(ROp::StoreGlobal as u8);
                ctx.emit_u32(g);
                ctx.emit_u32(r);
            }
            Ok(())
        }
        StmtKind::ShareDeclaration(decl, _) => {
            if ctx.locals.is_some() {
                let rslot = ctx.reg();
                let mut lm = ctx.locals.take().unwrap();
                lm.insert(decl.name.clone(), rslot);
                ctx.locals = Some(lm);
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::Move as u8);
                ctx.emit_u32(rslot);
                ctx.emit_u32(r);
            } else {
                let g = ctx.add_global(decl.name.clone());
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::StoreGlobal as u8);
                ctx.emit_u32(g);
                ctx.emit_u32(r);
            }
            Ok(())
        }
        StmtKind::StrongDeclaration(decl, _) => {
            if ctx.locals.is_some() {
                let rslot = ctx.reg();
                let mut lm = ctx.locals.take().unwrap();
                lm.insert(decl.name.clone(), rslot);
                ctx.locals = Some(lm);
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::Move as u8);
                ctx.emit_u32(rslot);
                ctx.emit_u32(r);
            } else {
                let g = ctx.add_global(decl.name.clone());
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::StoreGlobal as u8);
                ctx.emit_u32(g);
                ctx.emit_u32(r);
            }
            Ok(())
        }
        StmtKind::WeakDeclaration(decl, _) => {
            if ctx.locals.is_some() {
                let rslot = ctx.reg();
                let mut lm = ctx.locals.take().unwrap();
                lm.insert(decl.name.clone(), rslot);
                ctx.locals = Some(lm);
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::Move as u8);
                ctx.emit_u32(rslot);
                ctx.emit_u32(r);
            } else {
                let g = ctx.add_global(decl.name.clone());
                let r = emit_expr_v2(&decl.expr, ctx)?;
                ctx.emit_u8(ROp::StoreGlobal as u8);
                ctx.emit_u32(g);
                ctx.emit_u32(r);
            }
            Ok(())
        }
        StmtKind::ExprStmt(e) => {
            let _ = emit_expr_v2(e, ctx)?;
            Ok(())
        }
        StmtKind::Block(bs) => {
            // Push a new defer scope for this block
            ctx.push_defer_scope();
            for b in bs {
                emit_stmt_v2(b, ctx)?;
            }
            // Emit this scope's defers in LIFO order (compile-time inlining)
            ctx.emit_scope_defers()?;
            Ok(())
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let rc = emit_expr_v2(cond, ctx)?;
            ctx.emit_u8(ROp::JumpIfFalse as u8);
            ctx.emit_u32(rc);
            let jfalse_rel_pos = ctx.code.len();
            ctx.emit_u32(0);
            let jfalse_after_pos = ctx.code.len();
            // Then branch — own defer scope
            ctx.push_defer_scope();
            emit_stmt_v2(then_branch, ctx)?;
            ctx.emit_scope_defers()?;
            if let Some(el) = else_branch {
                ctx.emit_u8(ROp::Jump as u8);
                let jend_rel_pos = ctx.code.len();
                ctx.emit_u32(0);
                let jend_after_pos = ctx.code.len();
                let else_start = ctx.code.len();
                let rel_then_skip = (else_start - jfalse_after_pos) as u32;
                ctx.patch_u32_at(jfalse_rel_pos, rel_then_skip);
                // Else branch — own defer scope
                ctx.push_defer_scope();
                emit_stmt_v2(el, ctx)?;
                ctx.emit_scope_defers()?;
                let end_pos = ctx.code.len();
                let rel_end_skip = (end_pos - jend_after_pos) as u32;
                ctx.patch_u32_at(jend_rel_pos, rel_end_skip);
            } else {
                let end_pos = ctx.code.len();
                let rel_then_skip = (end_pos - jfalse_after_pos) as u32;
                ctx.patch_u32_at(jfalse_rel_pos, rel_then_skip);
            }
            Ok(())
        }
        StmtKind::While { cond, body } => {
            let cond_pos = ctx.code.len();
            let rc = emit_expr_v2(cond, ctx)?;
            ctx.emit_u8(ROp::JumpIfFalse as u8);
            ctx.emit_u32(rc);
            let jfalse_rel_pos = ctx.code.len();
            ctx.emit_u32(0);
            let jfalse_after_pos = ctx.code.len();
            // Loop body — own defer scope; record depth for break/continue
            ctx.push_defer_scope();
            let saved_depth = ctx.defer_scopes.len() - 1;
            ctx.loop_defer_depths.push(saved_depth);
            emit_stmt_v2(body, ctx)?;
            // Emit this scope's defers at end of loop body (compile-time inlining)
            ctx.emit_scope_defers()?;
            ctx.loop_defer_depths.pop();
            ctx.emit_u8(ROp::JumpAbs as u8);
            ctx.emit_u32(cond_pos as u32);
            let end_pos = ctx.code.len();
            let rel_then_skip = (end_pos - jfalse_after_pos) as u32;
            ctx.patch_u32_at(jfalse_rel_pos, rel_then_skip);
            Ok(())
        }
        StmtKind::ForIn { name, iter, body } => {
            if let ExprKind::Range(start, end, inclusive) = &iter.kind {
                // Compile start/end expressions
                let r_start = emit_expr_v2(start, ctx)?;
                let r_end = emit_expr_v2(end, ctx)?;

                // Alloc loop var register
                let r_curr = ctx.reg();
                ctx.emit_u8(ROp::Move as u8);
                ctx.emit_u32(r_curr);
                ctx.emit_u32(r_start);

                // Scope handling: save old locals, insert name -> r_curr
                let old_locals = ctx.locals.clone();
                if ctx.locals.is_none() {
                    ctx.locals = Some(std::collections::HashMap::new());
                }
                if let Some(lm) = &mut ctx.locals {
                    lm.insert(name.clone(), r_curr);
                }

                // ULTRA FAST PATH: Detect accumulator patterns (e.g. count = count + 1)
                if let StmtKind::Block(stmts) = &body.kind {
                    if stmts.len() == 1 {
                        if let StmtKind::ExprStmt(loop_expr) = &stmts[0].kind {
                            if let ExprKind::AssignOp(lhs_expr, assign_op, rhs_expr) =
                                &loop_expr.kind
                            {
                                if let ExprKind::Variable(target_name) = &lhs_expr.kind {
                                    if *assign_op == TokenKind::Equal {
                                        if let ExprKind::Binary(bin_lhs, op, bin_rhs) =
                                            &rhs_expr.kind
                                        {
                                            if let ExprKind::Variable(lhs_name) = &bin_lhs.kind {
                                                if lhs_name == target_name && *op == TokenKind::Plus
                                                {
                                                    if let ExprKind::Literal(lit) = &bin_rhs.kind {
                                                        let lit_num = crate::execution::runtime_core::ops::num(lit.clone()).unwrap_or(1.0);
                                                        let r_target_opt =
                                                            ctx.locals.as_ref().and_then(|lm| {
                                                                lm.get(target_name).copied()
                                                            });
                                                        if let Some(r_target) = r_target_opt {
                                                            let r_diff = ctx.reg();
                                                            ctx.emit_u8(ROp::Sub as u8);
                                                            ctx.emit_u32(r_diff);
                                                            ctx.emit_u32(r_end);
                                                            ctx.emit_u32(r_start);

                                                            if *inclusive {
                                                                let k_one = ctx.add_const_num(1.0);
                                                                let r_one = ctx.reg();
                                                                ctx.emit_u8(ROp::LoadConst as u8);
                                                                ctx.emit_u32(r_one);
                                                                ctx.emit_u32(k_one);
                                                                ctx.emit_u8(ROp::Add as u8);
                                                                ctx.emit_u32(r_diff);
                                                                ctx.emit_u32(r_diff);
                                                                ctx.emit_u32(r_one);
                                                            }

                                                            let r_scale = if lit_num == 1.0 {
                                                                r_diff
                                                            } else {
                                                                let k_lit =
                                                                    ctx.add_const_num(lit_num);
                                                                let r_lit = ctx.reg();
                                                                ctx.emit_u8(ROp::LoadConst as u8);
                                                                ctx.emit_u32(r_lit);
                                                                ctx.emit_u32(k_lit);
                                                                let r_scaled = ctx.reg();
                                                                ctx.emit_u8(ROp::Mul as u8);
                                                                ctx.emit_u32(r_scaled);
                                                                ctx.emit_u32(r_diff);
                                                                ctx.emit_u32(r_lit);
                                                                r_scaled
                                                            };

                                                            ctx.emit_u8(ROp::Add as u8);
                                                            ctx.emit_u32(r_target);
                                                            ctx.emit_u32(r_target);
                                                            ctx.emit_u32(r_scale);

                                                            ctx.locals = old_locals;
                                                            return Ok(());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Hoist increment 1.0 outside the loop
                let k_one = ctx.add_const_num(1.0);
                let r_one = ctx.reg();
                ctx.emit_u8(ROp::LoadConst as u8);
                ctx.emit_u32(r_one);
                ctx.emit_u32(k_one);

                let loop_start_pos = ctx.code.len();

                // Condition check: r_curr < r_end (or <=)
                let r_cond = ctx.reg();
                let op_cmp = if *inclusive { ROp::CmpLE } else { ROp::CmpLT };
                ctx.emit_u8(op_cmp as u8);
                ctx.emit_u32(r_cond);
                ctx.emit_u32(r_curr);
                ctx.emit_u32(r_end);

                // JumpIfFalse to END
                ctx.emit_u8(ROp::JumpIfFalse as u8);
                ctx.emit_u32(r_cond);
                let jfalse_rel_pos = ctx.code.len();
                ctx.emit_u32(0);
                let jfalse_after_pos = ctx.code.len();

                // Body — push defer scope for loop body
                ctx.push_defer_scope();
                let saved_depth = ctx.defer_scopes.len() - 1;
                ctx.loop_defer_depths.push(saved_depth);
                emit_stmt_v2(body, ctx)?;
                // Emit this scope's defers at end of loop body (compile-time inlining)
                ctx.emit_scope_defers()?;
                ctx.loop_defer_depths.pop();

                // Increment: r_curr += 1
                ctx.emit_u8(ROp::Add as u8);
                ctx.emit_u32(r_curr);
                ctx.emit_u32(r_curr);
                ctx.emit_u32(r_one);

                // Loop back
                ctx.emit_u8(ROp::JumpAbs as u8);
                ctx.emit_u32(loop_start_pos as u32);

                // Patch exit
                let end_pos = ctx.code.len();
                let rel_skip = (end_pos - jfalse_after_pos) as u32;
                ctx.patch_u32_at(jfalse_rel_pos, rel_skip);

                // Restore locals
                ctx.locals = old_locals;

                Ok(())
            } else {
                Err(LangError::new(
                    ErrorKind::Type,
                    "VM v2 only supports range for-loops".into(),
                    0,
                    0,
                    "".into(),
                ))
            }
        }
        StmtKind::Return(opt) => {
            if let Some((fname, foff, farity)) = ctx.current_fun.clone() {
                if let Some(e) = opt {
                    if let ExprKind::Call(callee, args, _type_args) = &e.kind {
                        if let ExprKind::Variable(nm) = &callee.kind {
                            if *nm == fname && args.len() == farity {
                                let mut arg_regs: Vec<u32> = Vec::new();
                                for a in args {
                                    arg_regs.push(emit_expr_v2(a, ctx)?);
                                }
                                for (i, ar) in arg_regs.iter().enumerate() {
                                    ctx.emit_u8(ROp::Move as u8);
                                    ctx.emit_u32(i as u32);
                                    ctx.emit_u32(*ar);
                                }
                                // Emit all defers before tail call (compile-time inlining)
                                ctx.emit_all_defers()?;
                                ctx.emit_u8(ROp::JumpAbs as u8);
                                ctx.emit_u32(foff);
                                return Ok(());
                            }
                        }
                    }
                }
            }
            let r = if let Some(e) = opt {
                emit_expr_v2(e, ctx)?
            } else {
                let k = ctx.add_const_null();
                let rr = ctx.reg();
                ctx.emit_u8(ROp::LoadConst as u8);
                ctx.emit_u32(rr);
                ctx.emit_u32(k);
                rr
            };
            // Emit all defers in LIFO order before returning (compile-time inlining)
            ctx.emit_all_defers()?;
            ctx.emit_u8(ROp::Return as u8);
            ctx.emit_u32(r);
            Ok(())
        }
        StmtKind::ExternFunction(_) | StmtKind::ExternBlock { .. } => {
            // Extern declarations handled at runtime; no bytecode emission needed
            Ok(())
        }
        StmtKind::Function(f, _exp) => {
            // Emit jump instruction to skip this function during initial execution
            ctx.emit_u8(ROp::Jump as u8);
            let skip_pos = ctx.code.len();
            ctx.emit_u32(0); // Placeholder - will patch this

            // Now record the function offset (where CALL will jump to)
            let off = ctx.code.len() as u32;
            ctx.fun_offsets.insert(f.name.clone(), off);

            let mut lm: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
            for (i, p) in f.params.iter().enumerate() {
                lm.insert(p.0.clone(), i as u32);
            }
            let prev_next_reg = ctx.next_reg;
            ctx.next_reg = f.params.len() as u32;
            ctx.set_locals(lm);
            ctx.set_current_fun(f.name.clone(), off, f.params.len());
            // Push a defer scope for the function body
            ctx.push_defer_scope();
            for st in f.body.iter() {
                emit_stmt_v2(st, ctx)?;
            }
            let fn_max_reg = ctx.next_reg;
            ctx.clear_current_fun();
            ctx.clear_locals();
            ctx.next_reg = prev_next_reg.max(fn_max_reg);

            // Emit all remaining defers before implicit return (compile-time inlining)
            ctx.emit_all_defers()?;
            // Add implicit return if not already present
            let null_const = ctx.add_const_null();
            let ret_reg = ctx.reg();
            ctx.emit_u8(ROp::LoadConst as u8);
            ctx.emit_u32(ret_reg);
            ctx.emit_u32(null_const);
            ctx.emit_u8(ROp::Return as u8);
            ctx.emit_u32(ret_reg);

            // Patch the skip jump with the relative offset to after the function
            let after_fn = ctx.code.len();
            let rel_offset = (after_fn - skip_pos - 4) as u32;
            ctx.patch_u32_at(skip_pos, rel_offset);

            Ok(())
        }
        StmtKind::Defer(block) => {
            // Compile-time defer inlining: register the block in the current scope.
            // At scope exit (return, break, continue, end of block/function),
            // the defer blocks are emitted inline in LIFO order.
            // Zero runtime overhead — no DeferPush/DeferRun opcodes, no runtime stack.
            ctx.register_defer((**block).clone());
            Ok(())
        }
        _ => Ok(()),
    }
}

fn emit_expr_v2(e: &Expr, ctx: &mut REmit) -> Result<u32, LangError> {
    match &e.kind {
        ExprKind::Literal(v) => {
            use Value::*;
            let r = ctx.reg();
            match v {
                Number(n) => {
                    let k = ctx.add_const_num(*n);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                F64(n) => {
                    let k = ctx.add_const_num(*n);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                F32(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                I64(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                I32(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                I16(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                I8(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                U64(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                U32(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                U16(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                U8(n) => {
                    let k = ctx.add_const_num(*n as f64);
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                Str(s) => {
                    let k = ctx.add_const_str(s.clone());
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                Bool(b) => {
                    let k = ctx.add_const_num(if *b { 1.0 } else { 0.0 });
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                Null => {
                    let k = ctx.add_const_null();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                BigInt(bi) => {
                    let digits = bi.to_string();
                    let k = ctx.add_const_str(digits);
                    ctx.emit_u8(ROp::LoadConstBigInt as u8);
                    ctx.emit_u32(r);
                    ctx.emit_u32(k);
                }
                _ => {
                    return Err(LangError::new(
                        ErrorKind::Type,
                        format!("unsupported literal in v2: {:?}", v),
                        0,
                        0,
                        "".into(),
                    ));
                }
            }
            Ok(r)
        }
        ExprKind::Variable(name) => {
            if let Some(lm) = &ctx.locals {
                if let Some(&lr) = lm.get(name) {
                    return Ok(lr);
                }
            }
            if let Some(gidx) = ctx.gidx.get(name).copied() {
                let r = ctx.reg();
                ctx.emit_u8(ROp::LoadGlobal as u8);
                ctx.emit_u32(r);
                ctx.emit_u32(gidx);
                return Ok(r);
            }
            Err(LangError::new(
                ErrorKind::Type,
                format!("unknown variable {}", name),
                0,
                0,
                "".into(),
            ))
        }
        ExprKind::Binary(l, op, r) => {
            let ra = emit_expr_v2(l, ctx)?;
            let rb = emit_expr_v2(r, ctx)?;
            let rd = ctx.reg();
            match op {
                TokenKind::Plus => {
                    ctx.emit_u8(ROp::Add as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Minus => {
                    ctx.emit_u8(ROp::Sub as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Star => {
                    ctx.emit_u8(ROp::Mul as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Slash => {
                    ctx.emit_u8(ROp::Div as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Percent => {
                    ctx.emit_u8(ROp::Mod as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Less => {
                    ctx.emit_u8(ROp::CmpLT as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::LessEqual => {
                    ctx.emit_u8(ROp::CmpLE as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::Greater => {
                    ctx.emit_u8(ROp::CmpGT as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::GreaterEqual => {
                    ctx.emit_u8(ROp::CmpGE as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::EqualEqual => {
                    ctx.emit_u8(ROp::CmpEQ as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                TokenKind::BangEqual => {
                    ctx.emit_u8(ROp::CmpNE as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(ra);
                    ctx.emit_u32(rb);
                    Ok(rd)
                }
                _ => Err(LangError::new(
                    ErrorKind::Type,
                    format!("unsupported binop {:?}", op),
                    0,
                    0,
                    "".into(),
                )),
            }
        }
        ExprKind::Unary(op, r) => {
            let rr = emit_expr_v2(r, ctx)?;
            match op {
                TokenKind::Minus => {
                    let zero = ctx.add_const_num(0.0);
                    let rz = ctx.reg();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rz);
                    ctx.emit_u32(zero);
                    let rd = ctx.reg();
                    ctx.emit_u8(ROp::Sub as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(rz);
                    ctx.emit_u32(rr);
                    Ok(rd)
                }
                _ => Err(LangError::new(
                    ErrorKind::Type,
                    "unsupported unary op".into(),
                    0,
                    0,
                    "".into(),
                )),
            }
        }
        ExprKind::Assign(name, rhs) => {
            let rr = emit_expr_v2(rhs, ctx)?;
            if let Some(lm) = &ctx.locals {
                if let Some(&lr) = lm.get(name) {
                    ctx.emit_u8(ROp::Move as u8);
                    ctx.emit_u32(lr);
                    ctx.emit_u32(rr);
                    return Ok(lr);
                }
            }
            if let Some(gidx) = ctx.gidx.get(name).copied() {
                ctx.emit_u8(ROp::StoreGlobal as u8);
                ctx.emit_u32(gidx);
                ctx.emit_u32(rr);
                Ok(rr)
            } else {
                Err(LangError::new(
                    ErrorKind::Type,
                    format!("unknown variable {}", name),
                    0,
                    0,
                    "".into(),
                ))
            }
        }
        ExprKind::AssignOp(target, _op, rhs) => {
            // AssignOp represents compound assignments like i -= 1
            // The parser already expands this to the full expression (i = i - 1)
            // so we just treat it like a regular assignment
            let rr = emit_expr_v2(rhs, ctx)?;
            if let ExprKind::Variable(name) = &target.kind {
                if let Some(lm) = &ctx.locals {
                    if let Some(&lr) = lm.get(name) {
                        ctx.emit_u8(ROp::Move as u8);
                        ctx.emit_u32(lr);
                        ctx.emit_u32(rr);
                        return Ok(lr);
                    }
                }
                if let Some(gidx) = ctx.gidx.get(name).copied() {
                    ctx.emit_u8(ROp::StoreGlobal as u8);
                    ctx.emit_u32(gidx);
                    ctx.emit_u32(rr);
                    Ok(rr)
                } else {
                    Err(LangError::new(
                        ErrorKind::Type,
                        format!("unknown variable {}", name),
                        0,
                        0,
                        "".into(),
                    ))
                }
            } else {
                Err(LangError::new(
                    ErrorKind::Type,
                    format!("unsupported assignment target in v2: {:?}", target),
                    0,
                    0,
                    "".into(),
                ))
            }
        }
        ExprKind::Call(callee, args, _type_args) => {
            if let ExprKind::Variable(name) = &callee.kind {
                if name == "print" || name == "println" {
                    let mut arg_regs: Vec<u32> = Vec::new();
                    for a in args {
                        arg_regs.push(emit_expr_v2(a, ctx)?);
                    }
                    let rd = ctx.reg();
                    let name_idx = ctx.add_const_str(name.clone());
                    ctx.emit_u8(ROp::CallBuiltin as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(name_idx);
                    ctx.emit_u32(arg_regs.len() as u32);
                    for ar in arg_regs {
                        ctx.emit_u32(ar);
                    }
                    return Ok(rd);
                }
                if name == "clock" && args.is_empty() {
                    let rd = ctx.reg();
                    ctx.emit_u8(ROp::Clock as u8);
                    ctx.emit_u32(rd);
                    return Ok(rd);
                }
                // Handle argument functions
                if name == "argc" && args.is_empty() {
                    let rd = ctx.reg();
                    let argc_global = ctx.add_global("__argc".to_string());
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(argc_global);
                    return Ok(rd);
                }
                if name == "argv" && args.is_empty() {
                    let rd = ctx.reg();
                    let argv_global = ctx.add_global("__argv".to_string());
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(argv_global);
                    return Ok(rd);
                }
                if name == "execName" && args.is_empty() {
                    let rd = ctx.reg();
                    let exec_global = ctx.add_global("__execName".to_string());
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(exec_global);
                    return Ok(rd);
                }
                if name == "argsCount" && args.is_empty() {
                    let rd = ctx.reg();
                    let argc_global = ctx.add_global("__argc".to_string());
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(argc_global);
                    return Ok(rd);
                }
                if name == "arg" && args.len() == 1 {
                    // For now, arg(0) returns execName
                    if let ExprKind::Literal(Value::Number(n)) = &args[0].kind {
                        if *n == 0.0 {
                            let rd = ctx.reg();
                            let exec_global = ctx.add_global("__execName".to_string());
                            ctx.emit_u8(ROp::LoadGlobal as u8);
                            ctx.emit_u32(rd);
                            ctx.emit_u32(exec_global);
                            return Ok(rd);
                        }
                    }
                    // For other indices, return null for now
                    let rd = ctx.reg();
                    let null_const = ctx.add_const_null();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(null_const);
                    return Ok(rd);
                }
                // Handle environment functions
                if name == "env" && args.len() == 1 {
                    // If called as env("KEY") with a string literal, compile to LoadGlobal(__env:<KEY>)
                    if let ExprKind::Literal(Value::Str(key)) = &args[0].kind {
                        let g = ctx.add_global(format!("__env:{}", key));
                        let rd = ctx.reg();
                        ctx.emit_u8(ROp::LoadGlobal as u8);
                        ctx.emit_u32(rd);
                        ctx.emit_u32(g);
                        return Ok(rd);
                    }
                    // Otherwise return null (unsupported dynamic lookup)
                    let rd = ctx.reg();
                    let null_const = ctx.add_const_null();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(null_const);
                    return Ok(rd);
                }
                // Check if it's a builtin function (parseArgs, argGet, argHas, argsIndexOf, argsJoin, etc.)
                let builtin_names = [
                    "parseArgs",
                    "argGet",
                    "argHas",
                    "argsIndexOf",
                    "argsJoin",
                    "type",
                    "typeof",
                    "sizeof",
                    "hasKey",
                    "len",
                ];
                if builtin_names.contains(&name.as_str()) {
                    // Emit CallBuiltin instruction
                    let mut arg_regs: Vec<u32> = Vec::new();
                    for a in args {
                        arg_regs.push(emit_expr_v2(a, ctx)?);
                    }
                    let rd = ctx.reg();
                    let name_idx = ctx.add_const_str(name.clone());
                    ctx.emit_u8(ROp::CallBuiltin as u8);
                    ctx.emit_u32(rd); // destination register
                    ctx.emit_u32(name_idx); // builtin name (const index)
                    ctx.emit_u32(arg_regs.len() as u32); // arg count
                    for ar in arg_regs {
                        ctx.emit_u32(ar);
                    } // arg registers
                    return Ok(rd);
                }
                // Handle argsSlice function
                if name == "argsSlice" && args.len() == 1 {
                    // For now, only support argsSlice(1) with pre-computed value
                    if let ExprKind::Literal(Value::Number(n)) = &args[0].kind {
                        if *n == 1.0 {
                            let rd = ctx.reg();
                            let slice_global = ctx.add_global("__argsSlice1".to_string());
                            ctx.emit_u8(ROp::LoadGlobal as u8);
                            ctx.emit_u32(rd);
                            ctx.emit_u32(slice_global);
                            return Ok(rd);
                        }
                    }
                    // For other indices, return empty array
                    let rd = ctx.reg();
                    let empty_array_const = ctx.add_const_str("[]".to_string());
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(empty_array_const);
                    return Ok(rd);
                }
                // Handle argsJoin function
                if name == "argsJoin" && args.len() <= 1 {
                    // Return pre-computed joined string
                    let rd = ctx.reg();
                    let joined_global = ctx.add_global("__argsJoined".to_string());
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(joined_global);
                    return Ok(rd);
                }
                // Handle envFromFile function
                if name == "envFromFile" && args.len() <= 1 {
                    if args.len() == 1 {
                        if let ExprKind::Literal(Value::Str(path)) = &args[0].kind {
                            let g = ctx.add_global(format!("__envFromFile:{}", path));
                            let rd = ctx.reg();
                            ctx.emit_u8(ROp::LoadGlobal as u8);
                            ctx.emit_u32(rd);
                            ctx.emit_u32(g);
                            return Ok(rd);
                        }
                    }
                    // For other cases, return null
                    let rd = ctx.reg();
                    let null_const = ctx.add_const_null();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(null_const);
                    return Ok(rd);
                }
                // Handle envFileGet function
                if name == "envFileGet" && args.len() >= 1 && args.len() <= 3 {
                    // If both key and path are string literals, compile to LoadGlobal(__envFile:<path>:<key>)
                    if args.len() >= 1 {
                        if let ExprKind::Literal(Value::Str(key)) = &args[0].kind {
                            let default = if args.len() >= 2 {
                                Some(&args[1])
                            } else {
                                None
                            };
                            let path = if args.len() >= 3 {
                                if let ExprKind::Literal(Value::Str(p)) = &args[2].kind {
                                    Some(p.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if let Some(p) = path {
                                let g = ctx.add_global(format!("__envFile:{}:{}", p, key));
                                let rd = ctx.reg();
                                ctx.emit_u8(ROp::LoadGlobal as u8);
                                ctx.emit_u32(rd);
                                ctx.emit_u32(g);
                                return Ok(rd);
                            }
                            // if no path or not literal, fall back to default if provided
                            if let Some(d) = default {
                                if let ExprKind::Literal(Value::Str(_)) = &d.kind { /* handled above */
                                }
                            }
                        }
                    }
                    let rd = ctx.reg();
                    let null_const = ctx.add_const_null();
                    ctx.emit_u8(ROp::LoadConst as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(null_const);
                    return Ok(rd);
                }
                // Handle envRuntimeLoad function - emit runtime load instruction
                if name == "envRuntimeLoad" && args.len() >= 1 {
                    // Evaluate the argument to get either a path string or an object loaded from envFromFile
                    let arg_reg = emit_expr_v2(&args[0], ctx)?;
                    let rd = ctx.reg();
                    // Emit EnvRuntimeLoadFile opcode: takes src register (path string or object), returns count
                    ctx.emit_u8(ROp::EnvRuntimeLoadFile as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(arg_reg);
                    return Ok(rd);
                }
                if let Some(&off) = ctx.fun_offsets.get(name) {
                    let mut arg_regs: Vec<u32> = Vec::new();
                    for a in args {
                        arg_regs.push(emit_expr_v2(a, ctx)?);
                    }
                    let rd = ctx.reg();
                    ctx.emit_u8(ROp::Call as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(off);
                    ctx.emit_u32(arg_regs.len() as u32);
                    for ar in arg_regs {
                        ctx.emit_u32(ar);
                    }
                    return Ok(rd);
                }
            } else if let ExprKind::Get(obj, method_name) = &callee.kind {
                // Method call: obj.method(args...)
                let obj_reg = emit_expr_v2(obj, ctx)?;
                let mut arg_regs: Vec<u32> = Vec::new();
                for a in args {
                    arg_regs.push(emit_expr_v2(a, ctx)?);
                }
                let method_idx = ctx.add_const_str(method_name.clone());
                let rd = ctx.reg();
                ctx.emit_u8(ROp::CallMethod as u8);
                ctx.emit_u32(rd);
                ctx.emit_u32(obj_reg);
                ctx.emit_u32(method_idx);
                ctx.emit_u32(arg_regs.len() as u32);
                for ar in arg_regs {
                    ctx.emit_u32(ar);
                }
                return Ok(rd);
            }
            Err(LangError::new(
                ErrorKind::Type,
                format!("unsupported call in v2: {:?}", callee),
                0,
                0,
                "".into(),
            ))
        }
        ExprKind::Grouping(g) => emit_expr_v2(g, ctx),
        ExprKind::Get(obj, prop) => {
            // If object is a variable assigned via envFromFile(path), compile to LoadGlobal(__envFile:<path>:<prop>)
            if let ExprKind::Variable(name) = &obj.kind {
                if let Some(p) = ctx.env_file_vars.get(name) {
                    let g = ctx.add_global(format!("__envFile:{}:{}", p, prop));
                    let rd = ctx.reg();
                    ctx.emit_u8(ROp::LoadGlobal as u8);
                    ctx.emit_u32(rd);
                    ctx.emit_u32(g);
                    return Ok(rd);
                }
            }
            let obj_reg = emit_expr_v2(obj, ctx)?;
            let field_idx = ctx.add_const_str(prop.clone());
            let rd = ctx.reg();
            ctx.emit_u8(ROp::GetField as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(obj_reg);
            ctx.emit_u32(field_idx);
            Ok(rd)
        }
        ExprKind::Set(obj, prop, val) => {
            let obj_reg = emit_expr_v2(obj, ctx)?;
            let val_reg = emit_expr_v2(val, ctx)?;
            let field_idx = ctx.add_const_str(prop.clone());
            ctx.emit_u8(ROp::SetField as u8);
            ctx.emit_u32(obj_reg);
            ctx.emit_u32(field_idx);
            ctx.emit_u32(val_reg);
            Ok(val_reg)
        }
        ExprKind::Index(coll, idx) => {
            let coll_reg = emit_expr_v2(coll, ctx)?;
            let idx_reg = emit_expr_v2(idx, ctx)?;
            let rd = ctx.reg();
            ctx.emit_u8(ROp::GetIndex as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(coll_reg);
            ctx.emit_u32(idx_reg);
            Ok(rd)
        }
        ExprKind::Object(entries) => {
            let mut kv_regs = Vec::with_capacity(entries.len() * 2);
            for (k, v) in entries {
                let kidx = ctx.add_const_str(k.clone());
                let vreg = emit_expr_v2(v, ctx)?;
                kv_regs.push((kidx, vreg));
            }
            let rd = ctx.reg();
            ctx.emit_u8(ROp::NewObject as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(kv_regs.len() as u32);
            for (kidx, vreg) in kv_regs {
                ctx.emit_u32(kidx);
                ctx.emit_u32(vreg);
            }
            Ok(rd)
        }
        ExprKind::Array(elems) => {
            let mut elem_regs = Vec::with_capacity(elems.len());
            for elem in elems {
                elem_regs.push(emit_expr_v2(elem, ctx)?);
            }
            let rd = ctx.reg();
            ctx.emit_u8(ROp::NewArray as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(elem_regs.len() as u32);
            for reg in elem_regs {
                ctx.emit_u32(reg);
            }
            Ok(rd)
        }
        ExprKind::Tuple(elems) => {
            let mut elem_regs = Vec::with_capacity(elems.len());
            for elem in elems {
                elem_regs.push(emit_expr_v2(elem, ctx)?);
            }
            let rd = ctx.reg();
            ctx.emit_u8(ROp::NewTuple as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(elem_regs.len() as u32);
            for reg in elem_regs {
                ctx.emit_u32(reg);
            }
            Ok(rd)
        }
        ExprKind::StructLiteral(_name, kv) => {
            let mut kv_regs = Vec::with_capacity(kv.len() * 2);
            for (k, v) in kv {
                let kidx = ctx.add_const_str(k.clone());
                let vreg = emit_expr_v2(v, ctx)?;
                kv_regs.push((kidx, vreg));
            }
            let rd = ctx.reg();
            ctx.emit_u8(ROp::NewObject as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(kv_regs.len() as u32);
            for (kidx, vreg) in kv_regs {
                ctx.emit_u32(kidx);
                ctx.emit_u32(vreg);
            }
            Ok(rd)
        }
        ExprKind::Fn(params, body, _is_async) => {
            // Emit jump over anonymous closure function body
            ctx.emit_u8(ROp::Jump as u8);
            let skip_pos = ctx.code.len();
            ctx.emit_u32(0);
            let fn_start_pos = ctx.code.len();

            let saved_locals = ctx.locals.clone();
            let saved_fun = ctx.current_fun.clone();

            let mut fn_locals = std::collections::HashMap::new();
            for (i, (pname, _pinit, _pty)) in params.iter().enumerate() {
                fn_locals.insert(pname.clone(), i as u32);
            }
            ctx.locals = Some(fn_locals);
            ctx.current_fun = None;

            for stmt in body.iter() {
                emit_stmt_v2(stmt, ctx)?;
            }

            // Emit implicit return null
            let null_k = ctx.add_const_null();
            let ret_reg = ctx.reg();
            ctx.emit_u8(ROp::LoadConst as u8);
            ctx.emit_u32(ret_reg);
            ctx.emit_u32(null_k);
            ctx.emit_u8(ROp::Return as u8);
            ctx.emit_u32(ret_reg);

            let fn_end_pos = ctx.code.len();
            let skip_rel = (fn_end_pos - (skip_pos + 4)) as u32;
            ctx.patch_u32_at(skip_pos, skip_rel);

            ctx.locals = saved_locals;
            ctx.current_fun = saved_fun;

            let rd = ctx.reg();
            ctx.emit_u8(ROp::MakeClosure as u8);
            ctx.emit_u32(rd);
            ctx.emit_u32(fn_start_pos as u32);
            ctx.emit_u32(0); // 0 captures for now
            Ok(rd)
        }
        _ => Err(LangError::new(
            ErrorKind::Type,
            format!("unsupported expr in v2: {:?}", e),
            0,
            0,
            "".into(),
        )),
    }
}
