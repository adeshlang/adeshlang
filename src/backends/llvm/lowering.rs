//! LLVM Backend - Direct VIR to LLVM IR lowering
//!
//! This module implements Phase 7.3: a direct VIR to LLVM IR lowering path
//! that leverages LLVM for high-performance JIT and AOT compilation.

use crate::ir::vir::{VirFunction, VirType};
use std::collections::HashMap;

/// LLVM type representation
#[derive(Debug, Clone, PartialEq)]
pub enum LLVMType {
    Void,
    Int(usize), // i8, i16, i32, i64, etc
    Float,      // f64
    Double,     // f128
    Pointer(Box<LLVMType>),
    Array(Box<LLVMType>, usize),
    Struct(Vec<LLVMType>, String),          // fields and struct name
    Function(Vec<LLVMType>, Box<LLVMType>), // args and return type
}

impl LLVMType {
    /// Get LLVM type string representation
    pub fn to_string(&self) -> String {
        match self {
            LLVMType::Void => "void".to_string(),
            LLVMType::Int(bits) => format!("i{}", bits),
            LLVMType::Float => "f64".to_string(),
            LLVMType::Double => "f128".to_string(),
            LLVMType::Pointer(ty) => format!("{}*", ty.to_string()),
            LLVMType::Array(ty, size) => format!("[{} x {}]", size, ty.to_string()),
            LLVMType::Struct(_fields, name) => {
                format!("{}{{field_types}}", name) // Simplified
            }
            LLVMType::Function(args, ret) => {
                let arg_str = args
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", ret.to_string(), arg_str)
            }
        }
    }
}

/// LLVM value references
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LLVMValue {
    Const(String),
    Local(String),
    Global(String),
    Temp(String),
}

impl LLVMValue {
    pub fn to_string(&self) -> String {
        match self {
            LLVMValue::Const(s) => s.clone(),
            LLVMValue::Local(s) => format!("%{}", s),
            LLVMValue::Global(s) => format!("@{}", s),
            LLVMValue::Temp(s) => format!("%{}", s),
        }
    }
}

/// LLVM instruction
#[derive(Debug, Clone)]
pub enum LLVMInstr {
    // Arithmetic
    Add(String, LLVMType, LLVMValue, LLVMValue), // %result = add type %a, %b
    Sub(String, LLVMType, LLVMValue, LLVMValue),
    Mul(String, LLVMType, LLVMValue, LLVMValue),
    Div(String, LLVMType, LLVMValue, LLVMValue),
    Rem(String, LLVMType, LLVMValue, LLVMValue),

    // Bitwise
    And(String, LLVMType, LLVMValue, LLVMValue),
    Or(String, LLVMType, LLVMValue, LLVMValue),
    Xor(String, LLVMType, LLVMValue, LLVMValue),
    Shl(String, LLVMType, LLVMValue, LLVMValue),
    Shr(String, LLVMType, LLVMValue, LLVMValue),

    // Comparison
    ICmp(String, String, LLVMType, LLVMValue, LLVMValue), // %result = icmp cond type %a, %b
    FCmp(String, String, LLVMType, LLVMValue, LLVMValue),

    // Memory
    Alloca(String, LLVMType),                        // %ptr = alloca type
    Load(String, LLVMType, LLVMValue),               // %value = load type, type* %ptr
    Store(LLVMType, LLVMValue, LLVMType, LLVMValue), // store type %value, type* %ptr
    GetElementPtr(String, LLVMType, LLVMValue, Vec<LLVMValue>),

    // Type conversion
    Trunc(String, LLVMType, LLVMValue, LLVMType),
    ZExt(String, LLVMType, LLVMValue, LLVMType),
    SExt(String, LLVMType, LLVMValue, LLVMType),
    FPTrunc(String, LLVMType, LLVMValue, LLVMType),
    FPExt(String, LLVMType, LLVMValue, LLVMType),
    FPToUI(String, LLVMType, LLVMValue, LLVMType),
    FPToSI(String, LLVMType, LLVMValue, LLVMType),
    UIToFP(String, LLVMType, LLVMValue, LLVMType),
    SIToFP(String, LLVMType, LLVMValue, LLVMType),

    // Control flow
    Br(String),                        // br label %target
    CondBr(LLVMValue, String, String), // br i1 %cond, label %then, label %else

    // Function
    Call(String, LLVMType, String, Vec<(LLVMType, LLVMValue)>),
    Ret(Option<(LLVMType, LLVMValue)>),

    // Atomic operations (for ARC)
    AtomicRMW(String, String, LLVMType, LLVMValue, LLVMValue, String),
    AtomicCmpXchg(String, LLVMType, LLVMValue, LLVMValue, LLVMValue, String),

    // Labels
    Label(String),

    // Inline comment
    Comment(String),
}

/// LLVM basic block
#[derive(Debug, Clone)]
pub struct LLVMBasicBlock {
    pub label: String,
    pub instructions: Vec<LLVMInstr>,
}

/// LLVM function
#[derive(Debug, Clone)]
pub struct LLVMFunctionDef {
    pub name: String,
    pub return_type: LLVMType,
    pub args: Vec<(String, LLVMType)>,
    pub blocks: Vec<LLVMBasicBlock>,
    pub linkage: String, // "internal", "external", "weak", etc
}

impl LLVMFunctionDef {
    /// Convert to LLVM IR string
    pub fn to_ir_string(&self) -> String {
        let args_str = self
            .args
            .iter()
            .map(|(name, ty)| format!("{} %{}", ty.to_string(), name))
            .collect::<Vec<_>>()
            .join(", ");

        let mut ir = format!(
            "define {} {} @{}({}) {{\n",
            self.linkage,
            self.return_type.to_string(),
            self.name,
            args_str
        );

        for block in &self.blocks {
            ir.push_str(&format!("{}:\n", block.label));
            for instr in &block.instructions {
                ir.push_str(&format!("  {}\n", instr.to_ir_string()));
            }
        }

        ir.push_str("}\n");
        ir
    }
}

impl LLVMInstr {
    /// Convert instruction to LLVM IR string
    pub fn to_ir_string(&self) -> String {
        match self {
            LLVMInstr::Add(result, ty, a, b) => {
                format!(
                    "{} = add {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Sub(result, ty, a, b) => {
                format!(
                    "{} = sub {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Mul(result, ty, a, b) => {
                format!(
                    "{} = mul {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Div(result, ty, a, b) => {
                format!(
                    "{} = sdiv {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Rem(result, ty, a, b) => {
                format!(
                    "{} = srem {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }

            LLVMInstr::And(result, ty, a, b) => {
                format!(
                    "{} = and {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Or(result, ty, a, b) => {
                format!(
                    "{} = or {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Xor(result, ty, a, b) => {
                format!(
                    "{} = xor {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Shl(result, ty, a, b) => {
                format!(
                    "{} = shl {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::Shr(result, ty, a, b) => {
                format!(
                    "{} = ashr {} {}, {}",
                    result,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }

            LLVMInstr::ICmp(result, pred, ty, a, b) => {
                format!(
                    "{} = icmp {} {} {}, {}",
                    result,
                    pred,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }
            LLVMInstr::FCmp(result, pred, ty, a, b) => {
                format!(
                    "{} = fcmp {} {} {}, {}",
                    result,
                    pred,
                    ty.to_string(),
                    a.to_string(),
                    b.to_string()
                )
            }

            LLVMInstr::Alloca(result, ty) => {
                format!("{} = alloca {}", result, ty.to_string())
            }
            LLVMInstr::Load(result, ty, ptr) => {
                format!(
                    "{} = load {}, {}* {}",
                    result,
                    ty.to_string(),
                    ty.to_string(),
                    ptr.to_string()
                )
            }
            LLVMInstr::Store(val_ty, val, ptr_ty, ptr) => {
                format!(
                    "store {} {}, {}* {}",
                    val_ty.to_string(),
                    val.to_string(),
                    ptr_ty.to_string(),
                    ptr.to_string()
                )
            }
            LLVMInstr::GetElementPtr(result, ty, ptr, indices) => {
                let indices_str = indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{} = getelementptr {}, {}* {}, {}",
                    result,
                    ty.to_string(),
                    ty.to_string(),
                    ptr.to_string(),
                    indices_str
                )
            }

            LLVMInstr::Trunc(result, from_ty, val, to_ty) => {
                format!(
                    "{} = trunc {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::ZExt(result, from_ty, val, to_ty) => {
                format!(
                    "{} = zext {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::SExt(result, from_ty, val, to_ty) => {
                format!(
                    "{} = sext {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::FPTrunc(result, from_ty, val, to_ty) => {
                format!(
                    "{} = fptrunc {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::FPExt(result, from_ty, val, to_ty) => {
                format!(
                    "{} = fpext {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::FPToUI(result, from_ty, val, to_ty) => {
                format!(
                    "{} = fptoui {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::FPToSI(result, from_ty, val, to_ty) => {
                format!(
                    "{} = fptosi {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::UIToFP(result, from_ty, val, to_ty) => {
                format!(
                    "{} = uitofp {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }
            LLVMInstr::SIToFP(result, from_ty, val, to_ty) => {
                format!(
                    "{} = sitofp {} {} to {}",
                    result,
                    from_ty.to_string(),
                    val.to_string(),
                    to_ty.to_string()
                )
            }

            LLVMInstr::Br(target) => {
                format!("br label %{}", target)
            }
            LLVMInstr::CondBr(cond, then_bb, else_bb) => {
                format!(
                    "br i1 {}, label %{}, label %{}",
                    cond.to_string(),
                    then_bb,
                    else_bb
                )
            }

            LLVMInstr::Call(result, ret_ty, func_name, args) => {
                let args_str = args
                    .iter()
                    .map(|(ty, val)| format!("{} {}", ty.to_string(), val.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{} = call {} @{}({})",
                    result,
                    ret_ty.to_string(),
                    func_name,
                    args_str
                )
            }
            LLVMInstr::Ret(None) => "ret void".to_string(),
            LLVMInstr::Ret(Some((ty, val))) => {
                format!("ret {} {}", ty.to_string(), val.to_string())
            }

            LLVMInstr::AtomicRMW(result, op, ty, ptr, val, ordering) => {
                format!(
                    "{} = atomicrmw {} {} {}* {}, {} {}",
                    result,
                    op,
                    ty.to_string(),
                    ty.to_string(),
                    ptr.to_string(),
                    val.to_string(),
                    ordering
                )
            }
            LLVMInstr::AtomicCmpXchg(result, ty, ptr, cmp, new, ordering) => {
                format!(
                    "{} = cmpxchg {} {}* {}, {} {} {}",
                    result,
                    ty.to_string(),
                    ty.to_string(),
                    ptr.to_string(),
                    cmp.to_string(),
                    new.to_string(),
                    ordering
                )
            }

            LLVMInstr::Label(name) => {
                format!("{}:", name)
            }
            LLVMInstr::Comment(text) => {
                format!("; {}", text)
            }
        }
    }
}

/// VIR to LLVM IR lowerer
pub struct VirToLLVMLowerer {
    /// Module name
    module_name: String,

    /// Functions being generated
    functions: Vec<LLVMFunctionDef>,

    /// Global variables
    #[allow(dead_code)]
    globals: Vec<(String, LLVMType)>,

    /// Temporary counter
    #[allow(dead_code)]
    temp_counter: usize,

    /// Label counter
    #[allow(dead_code)]
    label_counter: usize,

    /// Variable to type mapping for current function
    var_types: HashMap<String, LLVMType>,
}

impl VirToLLVMLowerer {
    /// Create a new lowerer
    pub fn new(module_name: String) -> Self {
        Self {
            module_name,
            functions: Vec::new(),
            globals: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            var_types: HashMap::new(),
        }
    }

    /// Generate a temporary variable
    #[allow(dead_code)]
    fn gensym_temp(&mut self) -> String {
        let name = format!("t{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    /// Generate a label
    #[allow(dead_code)]
    fn gensym_label(&mut self) -> String {
        let name = format!("L{}", self.label_counter);
        self.label_counter += 1;
        name
    }

    /// Convert VIR type to LLVM type
    pub fn ir_type(&self, vir_type: &VirType) -> LLVMType {
        match vir_type {
            VirType::Void => LLVMType::Void,
            VirType::Bool => LLVMType::Int(1),
            VirType::I8 | VirType::U8 => LLVMType::Int(8),
            VirType::I16 | VirType::U16 => LLVMType::Int(16),
            VirType::I32 | VirType::U32 => LLVMType::Int(32),
            VirType::F32 => LLVMType::Float,
            VirType::I64 | VirType::U64 => LLVMType::Int(64),
            VirType::F64 => LLVMType::Float,
            VirType::I128 | VirType::U128 => LLVMType::Int(128),
            VirType::Ptr => LLVMType::Pointer(Box::new(LLVMType::Int(8))),
            VirType::TypedPtr(inner) => LLVMType::Pointer(Box::new(self.ir_type(inner))),
            VirType::Array { elem, size } => LLVMType::Array(Box::new(self.ir_type(elem)), *size),
            VirType::Struct(name) => LLVMType::Struct(vec![], name.clone()),
            VirType::Enum(name) => {
                // Enums are represented as i64 + tag
                LLVMType::Struct(vec![LLVMType::Int(8), LLVMType::Int(56)], name.clone())
            }
            VirType::Tuple(types) => {
                let fields = types.iter().map(|t| self.ir_type(t)).collect();
                LLVMType::Struct(fields, "__tuple".to_string())
            }
            VirType::FuncPtr { params, ret } => {
                let arg_types = params.iter().map(|t| self.ir_type(t)).collect();
                LLVMType::Function(arg_types, Box::new(self.ir_type(ret)))
            }
        }
    }

    /// Lower a VIR function to LLVM
    pub fn lower_function(&mut self, func: &VirFunction) -> Result<(), String> {
        let return_type = self.ir_type(&func.return_type);

        let args: Vec<(String, LLVMType)> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, param)| (format!("arg{}", i), self.ir_type(&param.ty)))
            .collect();

        let mut entry_block = LLVMBasicBlock {
            label: "entry".to_string(),
            instructions: vec![],
        };

        // Add comment
        entry_block
            .instructions
            .push(LLVMInstr::Comment(format!("VIR function: {}", func.name)));

        // Allocate space for locals
        for (i, local) in func.locals.iter().enumerate() {
            let llvm_ty = self.ir_type(&local.ty);
            let var_name = format!("local.{}", i);
            self.var_types.insert(var_name.clone(), llvm_ty.clone());

            entry_block
                .instructions
                .push(LLVMInstr::Alloca(format!("%{}", var_name), llvm_ty));
        }

        // Lower VIR instructions (simplified - just add return for now)
        entry_block.instructions.push(LLVMInstr::Ret(None));

        let llvm_func = LLVMFunctionDef {
            name: func.name.clone(),
            return_type,
            args,
            blocks: vec![entry_block],
            linkage: "external".to_string(),
        };

        self.functions.push(llvm_func);
        Ok(())
    }

    /// Get generated LLVM IR as string
    pub fn to_ir_string(&self) -> String {
        let mut ir = format!("; Module: {}\n", self.module_name);
        ir.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");

        // Emit function definitions
        for func in &self.functions {
            ir.push_str(&func.to_ir_string());
            ir.push('\n');
        }

        ir
    }

    /// Get vector of functions
    pub fn functions(&self) -> &[LLVMFunctionDef] {
        &self.functions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llvm_type_to_string() {
        assert_eq!(LLVMType::Void.to_string(), "void");
        assert_eq!(LLVMType::Int(64).to_string(), "i64");
        assert_eq!(LLVMType::Float.to_string(), "f64");
    }

    #[test]
    fn test_llvm_value_to_string() {
        assert_eq!(LLVMValue::Local("x".to_string()).to_string(), "%x");
        assert_eq!(LLVMValue::Global("main".to_string()).to_string(), "@main");
        assert_eq!(LLVMValue::Temp("t0".to_string()).to_string(), "%t0");
    }

    #[test]
    fn test_llvm_instr_basic() {
        let instr = LLVMInstr::Add(
            "%result".to_string(),
            LLVMType::Int(64),
            LLVMValue::Local("a".to_string()),
            LLVMValue::Local("b".to_string()),
        );
        let ir_str = instr.to_ir_string();
        assert!(ir_str.contains("add"));
        assert!(ir_str.contains("i64"));
    }

    #[test]
    fn test_llvm_function_ir() {
        let func = LLVMFunctionDef {
            name: "test".to_string(),
            return_type: LLVMType::Int(64),
            args: vec![
                ("a".to_string(), LLVMType::Int(64)),
                ("b".to_string(), LLVMType::Int(64)),
            ],
            blocks: vec![LLVMBasicBlock {
                label: "entry".to_string(),
                instructions: vec![LLVMInstr::Ret(Some((
                    LLVMType::Int(64),
                    LLVMValue::Local("a".to_string()),
                )))],
            }],
            linkage: "external".to_string(),
        };

        let ir_str = func.to_ir_string();
        assert!(ir_str.contains("define"));
        assert!(ir_str.contains("@test"));
        assert!(ir_str.contains("i64"));
    }

    #[test]
    fn test_lowerer_new() {
        let lowerer = VirToLLVMLowerer::new("test_module".to_string());
        assert_eq!(lowerer.module_name, "test_module");
    }

    #[test]
    fn test_ir_type_conversion() {
        let lowerer = VirToLLVMLowerer::new("test".to_string());
        assert_eq!(lowerer.ir_type(&VirType::Void), LLVMType::Void);
        assert_eq!(lowerer.ir_type(&VirType::Bool), LLVMType::Int(1));
        assert_eq!(lowerer.ir_type(&VirType::I64), LLVMType::Int(64));
        assert_eq!(lowerer.ir_type(&VirType::F64), LLVMType::Float);
    }

    #[test]
    fn test_temp_generation() {
        let mut lowerer = VirToLLVMLowerer::new("test".to_string());
        let t1 = lowerer.gensym_temp();
        let t2 = lowerer.gensym_temp();
        assert_ne!(t1, t2);
        assert!(t1.starts_with('t'));
    }
}
