//! Opt-in, typed Machine IR to relocatable WebAssembly objects.
//!
//! This is deliberately separate from the historical `WasmBackend`. No native
//! register allocator, host ABI, third-party emitter, or unsafe code is used.
//! Every call has an explicit signature and operand list. Virtual registers are
//! function-local WASM locals, and general CFGs use a bounded `br_table`
//! dispatcher rather than assuming that Machine IR blocks are structured.
#![forbid(unsafe_code)]

use crate::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction as Inst, MachineOperand as Operand,
    MachineRegister, VirtualRegister,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueType {
    I32,
    I64,
    F32,
    F64,
}

impl ValueType {
    fn byte(self) -> u8 {
        match self {
            Self::I32 => 0x7f,
            Self::I64 => 0x7e,
            Self::F32 => 0x7d,
            Self::F64 => 0x7c,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Signature {
    pub params: Vec<ValueType>,
    pub result: Option<ValueType>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub signature: Signature,
}

#[derive(Debug, Clone)]
pub struct CallSite {
    pub arguments: Vec<Operand>,
    pub result: Option<VirtualRegister>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub body: MachineFunction,
    pub parameters: Vec<VirtualRegister>,
    pub locals: BTreeMap<VirtualRegister, ValueType>,
    pub result: Option<VirtualRegister>,
    /// `(block id, instruction index)` -> arguments and result.
    pub calls: BTreeMap<(u32, usize), CallSite>,
}

impl Function {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            body: MachineFunction::new(name),
            parameters: Vec::new(),
            locals: BTreeMap::new(),
            result: None,
            calls: BTreeMap::new(),
        }
    }

    pub fn local(&mut self, ty: ValueType) -> VirtualRegister {
        let reg = self.body.alloc_vreg();
        self.locals.insert(reg, ty);
        reg
    }

    pub fn signature(&self) -> Result<Signature, String> {
        let ty = |reg: &VirtualRegister| {
            self.locals
                .get(reg)
                .copied()
                .ok_or_else(|| format!("undeclared register {reg:?} in `{}`", self.body.name))
        };
        Ok(Signature {
            params: self.parameters.iter().map(ty).collect::<Result<_, _>>()?,
            result: self.result.as_ref().map(ty).transpose()?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub functions: Vec<Function>,
    /// Symbol -> host import, or an import another input object defines.
    pub imports: BTreeMap<String, Import>,
    pub data: Vec<(String, Vec<u8>)>,
    pub undefined_data: BTreeSet<String>,
}

/// Explicit compiler resource limits, not runtime memory-safety guarantees.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub functions: usize,
    pub locals: usize,
    pub blocks: usize,
    pub object_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            functions: 65_536,
            locals: 65_536,
            blocks: 4096,
            object_bytes: 64 * 1024 * 1024,
        }
    }
}

pub fn uleb(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut b = (value & 127) as u8;
        value >>= 7;
        if value != 0 {
            b |= 128;
        }
        out.push(b);
        if value == 0 {
            break;
        }
    }
}

pub fn sleb(mut value: i64, out: &mut Vec<u8>) {
    loop {
        let b = (value & 127) as u8;
        value >>= 7;
        let done = (value == 0 && b & 64 == 0) || (value == -1 && b & 64 != 0);
        out.push(if done { b } else { b | 128 });
        if done {
            break;
        }
    }
}

fn count(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "WASM object exceeds a 32-bit index or section size".into())
}

fn name(value: &str, out: &mut Vec<u8>) -> Result<(), String> {
    uleb(count(value.len())?, out);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn section(id: u8, data: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<(), String> {
    if out
        .len()
        .checked_add(data.len())
        .and_then(|n| n.checked_add(6))
        .is_none_or(|n| n > limit)
    {
        return Err("WASM object exceeds its configured byte limit".into());
    }
    out.push(id);
    uleb(count(data.len())?, out);
    out.extend_from_slice(data);
    Ok(())
}

#[derive(Debug)]
struct Fixup {
    kind: u8,
    offset: u32,
    symbol: u32,
}

struct Encoder<'a> {
    function: &'a Function,
    locals: BTreeMap<VirtualRegister, u32>,
    signatures: &'a BTreeMap<String, Signature>,
    symbols: &'a BTreeMap<String, u32>,
    blocks: BTreeMap<String, u32>,
    bytes: Vec<u8>,
    fixups: Vec<Fixup>,
    limit: usize,
}

impl Encoder<'_> {
    fn local_index(&self, reg: VirtualRegister) -> Result<u32, String> {
        self.locals
            .get(&reg)
            .copied()
            .ok_or_else(|| format!("undeclared local {reg:?}"))
    }

    fn reg(op: &Operand) -> Result<VirtualRegister, String> {
        match op {
            Operand::Register(MachineRegister::Virtual(reg)) => Ok(*reg),
            _ => Err(format!("expected a virtual register, got {op:?}")),
        }
    }

    fn ty(&self, op: &Operand) -> Result<ValueType, String> {
        let reg = Self::reg(op)?;
        self.function
            .locals
            .get(&reg)
            .copied()
            .ok_or_else(|| format!("untyped local {reg:?}"))
    }

    fn set(&mut self, reg: VirtualRegister) -> Result<(), String> {
        self.bytes.push(0x21);
        uleb(self.local_index(reg)?, &mut self.bytes);
        Ok(())
    }

    fn fixup(&mut self, kind: u8, symbol: &str) -> Result<(), String> {
        let index = self
            .symbols
            .get(symbol)
            .copied()
            .ok_or_else(|| format!("undeclared symbol `{symbol}`"))?;
        self.fixups.push(Fixup {
            kind,
            offset: count(self.bytes.len())?,
            symbol: index,
        });
        // Fixed-width relocatable LEB, as required by the WASM object ABI.
        self.bytes
            .extend_from_slice(&[0x80, 0x80, 0x80, 0x80, 0x00]);
        Ok(())
    }

    fn value(&mut self, op: &Operand, expected: ValueType) -> Result<(), String> {
        match op {
            Operand::Register(_) => {
                if self.ty(op)? != expected {
                    return Err(format!(
                        "type mismatch: expected {expected:?}, got {:?}",
                        self.ty(op)?
                    ));
                }
                self.bytes.push(0x20);
                uleb(self.local_index(Self::reg(op)?)?, &mut self.bytes);
            }
            Operand::Immediate(n) => {
                self.bytes.push(match expected {
                    ValueType::I32 => {
                        i32::try_from(*n).map_err(|_| "i32 constant out of range")?;
                        0x41
                    }
                    ValueType::I64 => 0x42,
                    _ => {
                        return Err(
                            "integer constant used as a floating value without conversion".into(),
                        );
                    }
                });
                sleb(*n, &mut self.bytes);
            }
            Operand::FloatImmediate(n) => match expected {
                ValueType::F32 => {
                    self.bytes.push(0x43);
                    self.bytes.extend_from_slice(&(*n as f32).to_le_bytes());
                }
                ValueType::F64 => {
                    self.bytes.push(0x44);
                    self.bytes.extend_from_slice(&n.to_le_bytes());
                }
                _ => return Err("floating constant used as an integer without conversion".into()),
            },
            Operand::Symbol(symbol) if expected == ValueType::I32 => {
                if self.signatures.contains_key(symbol) {
                    return Err("function addresses require a typed indirect-call/table ABI".into());
                }
                self.bytes.push(0x41);
                self.fixup(4, symbol)?;
            }
            _ => return Err(format!("unsupported WASM operand {op:?}")),
        }
        Ok(())
    }

    fn binary(&mut self, dst: &Operand, src: &Operand, opcode: u8) -> Result<(), String> {
        let ty = self.ty(dst)?;
        self.value(dst, ty)?;
        self.value(src, ty)?;
        self.bytes.push(opcode);
        self.set(Self::reg(dst)?)
    }

    fn comparison(
        &mut self,
        lhs: &Operand,
        rhs: &Operand,
        cc: ConditionCode,
    ) -> Result<(), String> {
        use ConditionCode::*;
        let ty = self.ty(lhs)?;
        self.value(lhs, ty)?;
        self.value(rhs, ty)?;
        let offset = match cc {
            Equal | Zero => 0,
            NotEqual | NotZero => 1,
            LessThan => 2,
            Below => 3,
            GreaterThan => 4,
            Above => 5,
            LessOrEqual => 6,
            BelowOrEqual => 7,
            GreaterOrEqual => 8,
            AboveOrEqual => 9,
            _ => return Err(format!("condition {cc:?} has no scalar WASM comparison")),
        };
        let opcode = match ty {
            ValueType::I32 => 0x46 + offset,
            ValueType::I64 => 0x51 + offset,
            ValueType::F32 | ValueType::F64 => {
                let offset = match cc {
                    Equal => 0,
                    NotEqual => 1,
                    LessThan => 2,
                    GreaterThan => 3,
                    LessOrEqual => 4,
                    GreaterOrEqual => 5,
                    _ => return Err("unsigned/flag comparisons do not apply to floats".into()),
                };
                if ty == ValueType::F32 {
                    0x5b + offset
                } else {
                    0x61 + offset
                }
            }
        };
        self.bytes.push(opcode);
        Ok(())
    }

    fn jump(&mut self, target: &str, pc: u32, depth: u32) -> Result<(), String> {
        let block = self
            .blocks
            .get(target)
            .copied()
            .ok_or_else(|| format!("unknown block `{target}`"))?;
        self.bytes.push(0x41);
        sleb(i64::from(block), &mut self.bytes);
        self.bytes.push(0x21);
        uleb(pc, &mut self.bytes);
        self.bytes.push(0x0c);
        uleb(depth, &mut self.bytes);
        Ok(())
    }

    fn address(&mut self, op: &Operand) -> Result<(), String> {
        match op {
            Operand::Memory {
                base,
                offset,
                index,
            } => {
                self.value(&Operand::Register(*base), ValueType::I32)?;
                if let Some((reg, scale)) = index {
                    if !matches!(*scale, 1 | 2 | 4 | 8) {
                        return Err("invalid memory index scale".into());
                    }
                    self.value(&Operand::Register(*reg), ValueType::I32)?;
                    self.bytes.push(0x41);
                    sleb(i64::from(*scale), &mut self.bytes);
                    self.bytes.extend_from_slice(&[0x6c, 0x6a]);
                }
                if *offset != 0 {
                    self.bytes.push(0x41);
                    sleb(i64::from(*offset), &mut self.bytes);
                    self.bytes.push(0x6a);
                }
                Ok(())
            }
            _ => self.value(op, ValueType::I32),
        }
    }

    fn encode(mut self) -> Result<(Vec<u8>, Vec<Fixup>), String> {
        let f = self.function;
        let mut seen = BTreeSet::new();
        for reg in &f.parameters {
            if !seen.insert(*reg) {
                return Err("duplicate parameter register".into());
            }
            f.locals.get(reg).ok_or("untyped parameter")?;
            self.locals.insert(*reg, count(self.locals.len())?);
        }
        let non_params: Vec<_> = f.locals.iter().filter(|(r, _)| !seen.contains(r)).collect();
        let mut groups: Vec<(u32, ValueType)> = Vec::new();
        for (&reg, &ty) in non_params {
            self.locals.insert(reg, count(self.locals.len())?);
            if let Some((n, last)) = groups.last_mut().filter(|(_, last)| *last == ty) {
                *n += 1;
                let _ = last;
            } else {
                groups.push((1, ty));
            }
        }
        let pc = count(self.locals.len())?;
        if let Some((n, _)) = groups.last_mut().filter(|(_, ty)| *ty == ValueType::I32) {
            *n += 1;
        } else {
            groups.push((1, ValueType::I32));
        }
        uleb(count(groups.len())?, &mut self.bytes);
        for (n, ty) in groups {
            uleb(n, &mut self.bytes);
            self.bytes.push(ty.byte());
        }
        // loop dispatch; block invalid; nested case blocks; br_table
        self.bytes.extend_from_slice(&[0x03, 0x40, 0x02, 0x40]);
        let nblocks = count(f.body.blocks.len())?;
        for _ in 0..nblocks {
            self.bytes.extend_from_slice(&[0x02, 0x40]);
        }
        self.bytes.push(0x20);
        uleb(pc, &mut self.bytes);
        self.bytes.push(0x0e);
        uleb(nblocks, &mut self.bytes);
        for i in 0..nblocks {
            uleb(i, &mut self.bytes);
        }
        uleb(nblocks, &mut self.bytes); // invalid -> unreachable, never silently loop
        let mut used_calls = BTreeSet::new();
        for (block_index, block) in f.body.blocks.iter().enumerate() {
            self.bytes.push(0x0b);
            let depth = nblocks - count(block_index)?;
            let mut pending_compare: Option<(&Operand, &Operand)> = None;
            let mut terminated = false;
            for (index, inst) in block.instructions.iter().enumerate() {
                if terminated {
                    return Err(format!("instruction after terminator in `{}`", block.label));
                }
                if pending_compare.is_some()
                    && !matches!(inst, Inst::SetCc { .. } | Inst::BranchCc { .. })
                {
                    return Err("Compare must be immediately consumed by SetCc or BranchCc".into());
                }
                match inst {
                    Inst::Nop => {}
                    Inst::Move { dst, src } => {
                        self.value(src, self.ty(dst)?)?;
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::Add { dst, src }
                    | Inst::Sub { dst, src }
                    | Inst::Mul { dst, src }
                    | Inst::Div { dst, src }
                    | Inst::Mod { dst, src }
                    | Inst::And { dst, src }
                    | Inst::Or { dst, src }
                    | Inst::Xor { dst, src }
                    | Inst::Shl { dst, src }
                    | Inst::Shr { dst, src }
                    | Inst::Sar { dst, src } => {
                        let base = match self.ty(dst)? {
                            ValueType::I32 => 0x6a,
                            ValueType::I64 => 0x7c,
                            _ => return Err("integer instruction has floating operands".into()),
                        };
                        let off = match inst {
                            Inst::Add { .. } => 0,
                            Inst::Sub { .. } => 1,
                            Inst::Mul { .. } => 2,
                            Inst::Div { .. } => 3,
                            Inst::Mod { .. } => 5,
                            Inst::And { .. } => 7,
                            Inst::Or { .. } => 8,
                            Inst::Xor { .. } => 9,
                            Inst::Shl { .. } => 10,
                            Inst::Sar { .. } => 11,
                            _ => 12,
                        };
                        self.binary(dst, src, base + off)?;
                    }
                    Inst::FAdd { dst, src, size }
                    | Inst::FSub { dst, src, size }
                    | Inst::FMul { dst, src, size }
                    | Inst::FDiv { dst, src, size } => {
                        let base = match (self.ty(dst)?, size) {
                            (ValueType::F32, 4) => 0x92,
                            (ValueType::F64, 8) => 0xa0,
                            _ => return Err("floating width does not match its typed local".into()),
                        };
                        let off = match inst {
                            Inst::FAdd { .. } => 0,
                            Inst::FSub { .. } => 1,
                            Inst::FMul { .. } => 2,
                            _ => 3,
                        };
                        self.binary(dst, src, base + off)?;
                    }
                    Inst::Neg { dst } => {
                        let ty = self.ty(dst)?;
                        self.value(&Operand::Immediate(0), ty)?;
                        self.value(dst, ty)?;
                        self.bytes
                            .push(if ty == ValueType::I32 { 0x6b } else { 0x7d });
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::Not { dst } => {
                        let ty = self.ty(dst)?;
                        self.value(dst, ty)?;
                        self.value(&Operand::Immediate(-1), ty)?;
                        self.bytes
                            .push(if ty == ValueType::I32 { 0x73 } else { 0x85 });
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::FNeg { dst, size } => {
                        let opcode = match (self.ty(dst)?, size) {
                            (ValueType::F32, 4) => 0x8c,
                            (ValueType::F64, 8) => 0x9a,
                            _ => return Err("FNeg width mismatch".into()),
                        };
                        self.value(dst, self.ty(dst)?)?;
                        self.bytes.push(opcode);
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::Compare { lhs, rhs } => {
                        pending_compare = Some((lhs, rhs));
                    }
                    Inst::FCmp { lhs, rhs, size } => {
                        if !matches!(
                            (self.ty(lhs)?, size),
                            (ValueType::F32, 4) | (ValueType::F64, 8)
                        ) {
                            return Err("FCmp width mismatch".into());
                        }
                        pending_compare = Some((lhs, rhs));
                    }
                    Inst::SetCc { dst, cc } => {
                        if self.ty(dst)? != ValueType::I32 {
                            return Err("comparison result must be i32".into());
                        }
                        let (lhs, rhs) = pending_compare
                            .take()
                            .ok_or("SetCc without adjacent Compare")?;
                        self.comparison(lhs, rhs, *cc)?;
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::BranchCc { cc, target } => {
                        let (lhs, rhs) = pending_compare
                            .take()
                            .ok_or("BranchCc without adjacent Compare")?;
                        self.comparison(lhs, rhs, *cc)?;
                        self.bytes.extend_from_slice(&[0x04, 0x40]);
                        self.jump(target, pc, depth + 1)?;
                        self.bytes.push(0x0b);
                    }
                    Inst::Branch { target } => {
                        self.jump(target, pc, depth)?;
                        terminated = true;
                    }
                    Inst::Call {
                        target: Operand::Symbol(symbol),
                        num_args,
                    } => {
                        let key = (block.id, index);
                        let call = f
                            .calls
                            .get(&key)
                            .ok_or("call lacks explicit arguments/results")?;
                        used_calls.insert(key);
                        let sig = self
                            .signatures
                            .get(symbol)
                            .ok_or_else(|| format!("unknown function `{symbol}`"))?;
                        if call.arguments.len() != sig.params.len() || *num_args != sig.params.len()
                        {
                            return Err(format!("argument-count mismatch calling `{symbol}`"));
                        }
                        for (arg, ty) in call.arguments.iter().zip(&sig.params) {
                            self.value(arg, *ty)?;
                        }
                        self.bytes.push(0x10);
                        self.fixup(0, symbol)?;
                        match (sig.result, call.result) {
                            (Some(ty), Some(reg)) => {
                                if f.locals.get(&reg) != Some(&ty) {
                                    return Err("call result type mismatch".into());
                                }
                                self.set(reg)?;
                            }
                            (Some(_), None) => self.bytes.push(0x1a),
                            (None, Some(_)) => return Err("void call assigned to a result".into()),
                            (None, None) => {}
                        }
                    }
                    Inst::Load { dst, src, size } => {
                        self.address(src)?;
                        let opcode = match (self.ty(dst)?, size) {
                            (ValueType::I32, 1) => 0x2c,
                            (ValueType::I32, 2) => 0x2e,
                            (ValueType::I32, 4) => 0x28,
                            (ValueType::I64, 1) => 0x30,
                            (ValueType::I64, 2) => 0x32,
                            (ValueType::I64, 4) => 0x34,
                            (ValueType::I64, 8) => 0x29,
                            (ValueType::F32, 4) => 0x2a,
                            (ValueType::F64, 8) => 0x2b,
                            _ => return Err("invalid load width/type".into()),
                        };
                        self.bytes.extend_from_slice(&[opcode, 0, 0]);
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::Store { dst, src, size } => {
                        self.address(dst)?;
                        let ty = self.ty(src)?;
                        self.value(src, ty)?;
                        let opcode = match (ty, size) {
                            (ValueType::I32, 1) => 0x3a,
                            (ValueType::I32, 2) => 0x3b,
                            (ValueType::I32, 4) => 0x36,
                            (ValueType::I64, 1) => 0x3c,
                            (ValueType::I64, 2) => 0x3d,
                            (ValueType::I64, 4) => 0x3e,
                            (ValueType::I64, 8) => 0x37,
                            (ValueType::F32, 4) => 0x38,
                            (ValueType::F64, 8) => 0x39,
                            _ => return Err("invalid store width/type".into()),
                        };
                        self.bytes.extend_from_slice(&[opcode, 0, 0]);
                    }
                    Inst::FCvtIntToFloat {
                        dst,
                        src,
                        is_f64,
                        is_signed,
                    } => {
                        let dt = self.ty(dst)?;
                        if dt
                            != if *is_f64 {
                                ValueType::F64
                            } else {
                                ValueType::F32
                            }
                        {
                            return Err("conversion destination mismatch".into());
                        }
                        let offset = match self.ty(src)? {
                            ValueType::I32 => 0,
                            ValueType::I64 => 2,
                            _ => return Err("expected integer conversion source".into()),
                        };
                        self.value(src, self.ty(src)?)?;
                        self.bytes.push(
                            if *is_f64 { 0xb7 } else { 0xb2 } + offset + u8::from(!is_signed),
                        );
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::FCvtFloatToFloat { dst, src, to_f64 } => {
                        let (st, dt, opcode) = if *to_f64 {
                            (ValueType::F32, ValueType::F64, 0xbb)
                        } else {
                            (ValueType::F64, ValueType::F32, 0xb6)
                        };
                        if self.ty(dst)? != dt {
                            return Err("float conversion destination mismatch".into());
                        }
                        self.value(src, st)?;
                        self.bytes.push(opcode);
                        self.set(Self::reg(dst)?)?;
                    }
                    Inst::Custom { name, operands }
                        if name == "wasm.i32.wrap_i64" && operands.len() == 2 =>
                    {
                        if self.ty(&operands[0])? != ValueType::I32 {
                            return Err("wrap destination must be i32".into());
                        }
                        self.value(&operands[1], ValueType::I64)?;
                        self.bytes.push(0xa7);
                        self.set(Self::reg(&operands[0])?)?;
                    }
                    Inst::Return => {
                        if let Some(reg) = f.result {
                            self.value(
                                &Operand::Register(MachineRegister::Virtual(reg)),
                                f.locals[&reg],
                            )?;
                        }
                        self.bytes.push(0x0f);
                        terminated = true;
                    }
                    _ => return Err(format!("unsupported Machine IR instruction: {inst:?}")),
                }
                if self.bytes.len() > self.limit {
                    return Err("function exceeds object byte limit".into());
                }
            }
            if pending_compare.is_some() {
                return Err("unconsumed Compare at block end".into());
            }
            if !terminated {
                if block_index + 1 < f.body.blocks.len() {
                    self.jump(&f.body.blocks[block_index + 1].label, pc, depth)?;
                } else {
                    // Never synthesize a zero result for a missing return.
                    self.bytes.push(0x00);
                }
            }
        }
        if used_calls.len() != f.calls.len() {
            return Err("orphan call-site metadata".into());
        }
        self.bytes
            .extend_from_slice(&[0x0b, 0x00, 0x0b, 0x00, 0x0b]);
        Ok((self.bytes, self.fixups))
    }
}

impl Module {
    pub fn emit_object(&self, limits: Limits) -> Result<Vec<u8>, String> {
        if self
            .functions
            .len()
            .checked_add(self.imports.len())
            .is_none_or(|n| n > limits.functions)
        {
            return Err("function count exceeds configured WASM limit".into());
        }
        // Bound bulk data/name copies before allocating section payloads.
        let mut input_bytes = 8usize;
        let mut budget = |n: usize| -> Result<(), String> {
            input_bytes = input_bytes
                .checked_add(n)
                .ok_or("WASM input size overflow")?;
            if input_bytes > limits.object_bytes {
                return Err("WASM input exceeds configured object byte limit".into());
            }
            Ok(())
        };
        for f in &self.functions {
            budget(f.body.name.len())?;
        }
        for (symbol, import) in &self.imports {
            budget(symbol.len())?;
            budget(import.module.len())?;
            budget(import.name.len())?;
            if import.signature.params.len() > limits.locals {
                return Err("import parameter count exceeds local limit".into());
            }
        }
        for (symbol, bytes) in &self.data {
            budget(symbol.len())?;
            budget(bytes.len())?;
        }
        for symbol in &self.undefined_data {
            budget(symbol.len())?;
        }
        let mut signatures = BTreeMap::new();
        for (symbol, import) in &self.imports {
            signatures.insert(symbol.clone(), import.signature.clone());
        }
        for f in &self.functions {
            if signatures
                .insert(f.body.name.clone(), f.signature()?)
                .is_some()
            {
                return Err(format!("duplicate function symbol `{}`", f.body.name));
            }
            if f.locals.len() > limits.locals
                || f.body.blocks.len() > limits.blocks
                || f.body.blocks.is_empty()
            {
                return Err(format!(
                    "function `{}` exceeds local/block limits or has no entry block",
                    f.body.name
                ));
            }
        }
        let mut types = Vec::new();
        let mut type_indices = BTreeMap::new();
        for sig in signatures.values() {
            if !type_indices.contains_key(sig) {
                type_indices.insert(sig, count(types.len())?);
                types.push(sig.clone());
            }
        }
        let type_index = |sig: &Signature| -> Result<u32, String> {
            type_indices
                .get(sig)
                .copied()
                .ok_or_else(|| "missing signature".into())
        };
        let mut symbols = BTreeMap::new();
        let mut symtab = Vec::new();
        let nsymbols =
            self.functions.len() + self.imports.len() + self.data.len() + self.undefined_data.len();
        uleb(count(nsymbols)?, &mut symtab);
        for (index, (symbol, _)) in self.imports.iter().enumerate() {
            symbols.insert(symbol.clone(), count(symbols.len())?);
            symtab.push(0);
            uleb(0x50, &mut symtab);
            uleb(count(index)?, &mut symtab);
            name(symbol, &mut symtab)?;
        }
        for (index, f) in self.functions.iter().enumerate() {
            symbols.insert(f.body.name.clone(), count(symbols.len())?);
            symtab.push(0);
            uleb(if f.body.is_exported { 0x20 } else { 0 }, &mut symtab);
            uleb(count(self.imports.len() + index)?, &mut symtab);
            name(&f.body.name, &mut symtab)?;
        }
        for (index, (symbol, bytes)) in self.data.iter().enumerate() {
            if symbols
                .insert(symbol.clone(), count(symbols.len())?)
                .is_some()
            {
                return Err(format!("duplicate symbol `{symbol}`"));
            }
            symtab.push(1);
            uleb(0, &mut symtab);
            name(symbol, &mut symtab)?;
            uleb(count(index)?, &mut symtab);
            uleb(0, &mut symtab);
            uleb(count(bytes.len())?, &mut symtab);
        }
        for symbol in &self.undefined_data {
            if symbols
                .insert(symbol.clone(), count(symbols.len())?)
                .is_some()
            {
                return Err(format!("duplicate symbol `{symbol}`"));
            }
            symtab.push(1);
            uleb(0x10, &mut symtab);
            name(symbol, &mut symtab)?;
        }
        let mut out = b"\0asm\x01\0\0\0".to_vec();
        let mut payload = Vec::new();
        uleb(count(types.len())?, &mut payload);
        for sig in &types {
            payload.push(0x60);
            uleb(count(sig.params.len())?, &mut payload);
            payload.extend(sig.params.iter().map(|t| t.byte()));
            uleb(u32::from(sig.result.is_some()), &mut payload);
            if let Some(ty) = sig.result {
                payload.push(ty.byte());
            }
        }
        section(1, &payload, &mut out, limits.object_bytes)?;
        payload.clear();
        uleb(count(self.imports.len() + 1)?, &mut payload);
        for import in self.imports.values() {
            name(&import.module, &mut payload)?;
            name(&import.name, &mut payload)?;
            payload.push(0);
            uleb(type_index(&import.signature)?, &mut payload);
        }
        name("env", &mut payload)?;
        name("__linear_memory", &mut payload)?;
        payload.extend_from_slice(&[2, 0, 1]);
        section(2, &payload, &mut out, limits.object_bytes)?;
        payload.clear();
        uleb(count(self.functions.len())?, &mut payload);
        for f in &self.functions {
            uleb(type_index(&f.signature()?)?, &mut payload);
        }
        section(3, &payload, &mut out, limits.object_bytes)?;
        let mut code = Vec::new();
        let mut fixups = Vec::new();
        uleb(count(self.functions.len())?, &mut code);
        for f in &self.functions {
            let mut blocks = BTreeMap::new();
            let mut ids = BTreeSet::new();
            for (index, block) in f.body.blocks.iter().enumerate() {
                if blocks.insert(block.label.clone(), count(index)?).is_some()
                    || !ids.insert(block.id)
                {
                    return Err(format!("duplicate block label/id in `{}`", f.body.name));
                }
            }
            let encoder = Encoder {
                function: f,
                locals: BTreeMap::new(),
                signatures: &signatures,
                symbols: &symbols,
                blocks,
                bytes: Vec::new(),
                fixups: Vec::new(),
                limit: limits.object_bytes,
            };
            let (body, mut relocs) = encoder
                .encode()
                .map_err(|e| format!("WASM function `{}`: {e}", f.body.name))?;
            uleb(count(body.len())?, &mut code);
            let base = count(code.len())?;
            for reloc in &mut relocs {
                reloc.offset = reloc
                    .offset
                    .checked_add(base)
                    .ok_or("relocation offset overflow")?;
            }
            fixups.extend(relocs);
            code.extend(body);
            if code.len() > limits.object_bytes {
                return Err("code exceeds configured object limit".into());
            }
        }
        // CODE is section index 3, including the type/import/function sections.
        section(10, &code, &mut out, limits.object_bytes)?;
        payload.clear();
        uleb(count(self.data.len())?, &mut payload);
        for (_, bytes) in &self.data {
            payload.extend_from_slice(&[0, 0x41, 0, 0x0b]);
            uleb(count(bytes.len())?, &mut payload);
            payload.extend_from_slice(bytes);
        }
        section(11, &payload, &mut out, limits.object_bytes)?;
        payload.clear();
        name("linking", &mut payload)?;
        uleb(2, &mut payload);
        payload.push(8);
        uleb(count(symtab.len())?, &mut payload);
        payload.extend(symtab);
        section(0, &payload, &mut out, limits.object_bytes)?;
        payload.clear();
        name("reloc.CODE", &mut payload)?;
        uleb(3, &mut payload);
        uleb(count(fixups.len())?, &mut payload);
        for fixup in fixups {
            payload.push(fixup.kind);
            uleb(fixup.offset, &mut payload);
            uleb(fixup.symbol, &mut payload);
            if fixup.kind == 4 {
                sleb(0, &mut payload);
            }
        }
        section(0, &payload, &mut out, limits.object_bytes)?;
        Ok(out)
    }
}
