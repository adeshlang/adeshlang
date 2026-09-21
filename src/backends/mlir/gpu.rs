//! GPU Support for MLIR Backend
//!
//! Utilities for GPU kernel generation and execution.

use crate::backends::common::BackendResult;
use crate::ir::vir::{VirBlock, VirFunction, VirInstruction, VirTerminator, VirType};
use crate::toolchain::config::GpuTarget;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// GPU Memory Spaces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySpace {
    /// Global memory (heap)
    Global = 0,
    /// Unified / Managed memory (zero-copy shared CPU-GPU buffer)
    Unified = 1,
    /// Local/Shared memory (stack)
    Local = 3,
    /// Private memory (registers)
    Private = 5,
}

impl MemorySpace {
    /// Convert to MLIR memory space attribute
    pub fn to_mlir_attr(&self) -> u32 {
        *self as u32
    }

    /// Get MLIR type with memory space annotation
    pub fn annotate_ptr_type(&self, _base_ty: &str) -> String {
        match self {
            MemorySpace::Global => format!("!llvm.ptr<{}>", self.to_mlir_attr()),
            MemorySpace::Unified => format!("!llvm.ptr<{}>", self.to_mlir_attr()),
            MemorySpace::Local => format!("!llvm.ptr<{}>", self.to_mlir_attr()),
            MemorySpace::Private => format!("!llvm.ptr<{}>", self.to_mlir_attr()),
        }
    }
}

/// GPU memory access pattern
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryAccessPattern {
    /// Coalesced access (optimal)
    Coalesced,
    /// Strided access (suboptimal)
    Strided { stride: usize },
    /// Random access (poor)
    Random,
    /// Unknown pattern
    Unknown,
}

/// GPU kernel optimization context (Phase 5)
#[derive(Debug, Clone)]
pub struct GpuOptimizationContext {
    /// Memory space tracking: ValueId → MemorySpace
    pub memory_spaces: HashMap<u32, MemorySpace>,
    /// Shared memory usage tracking
    pub shared_memory_used: bool,
    /// Barrier insertion points (BlockId)
    pub barrier_points: Vec<u32>,
    /// Divergent branches (BlockId)
    pub divergent_branches: Vec<u32>,
    /// Memory access patterns
    pub memory_patterns: HashMap<u32, MemoryAccessPattern>,
}

impl GpuOptimizationContext {
    pub fn new() -> Self {
        Self {
            memory_spaces: HashMap::new(),
            shared_memory_used: false,
            barrier_points: Vec::new(),
            divergent_branches: Vec::new(),
            memory_patterns: HashMap::new(),
        }
    }

    /// Mark a value as residing in a specific memory space
    pub fn set_memory_space(&mut self, value_id: u32, space: MemorySpace) {
        self.memory_spaces.insert(value_id, space);
        if space == MemorySpace::Local {
            self.shared_memory_used = true;
        }
    }

    /// Get memory space for a value (default: Global)
    pub fn get_memory_space(&self, value_id: u32) -> MemorySpace {
        self.memory_spaces
            .get(&value_id)
            .copied()
            .unwrap_or(MemorySpace::Global)
    }

    /// Add a barrier synchronization point
    pub fn add_barrier(&mut self, block_id: u32) {
        if !self.barrier_points.contains(&block_id) {
            self.barrier_points.push(block_id);
        }
    }

    /// Mark a branch as divergent
    pub fn mark_divergent(&mut self, block_id: u32) {
        if !self.divergent_branches.contains(&block_id) {
            self.divergent_branches.push(block_id);
        }
    }
}

/// GPU toolchain discovery
#[derive(Debug, Clone)]
pub struct GpuToolchain {
    pub mlir_opt: Option<PathBuf>,
    pub mlir_translate: Option<PathBuf>,
    pub llc: Option<PathBuf>,
    pub clang: Option<PathBuf>,
}

impl GpuToolchain {
    pub fn is_available(&self) -> bool {
        self.mlir_opt.is_some()
            && self.mlir_translate.is_some()
            && self.llc.is_some()
            && self.clang.is_some()
    }
}

/// GPU Kernel Configuration
#[derive(Debug, Clone)]
pub struct GpuKernelConfig {
    /// Grid dimensions (blocks)
    pub grid_dims: (u32, u32, u32),
    /// Block dimensions (threads)
    pub block_dims: (u32, u32, u32),
    /// Shared memory size
    pub shared_memory_size: usize,
}

impl Default for GpuKernelConfig {
    fn default() -> Self {
        Self {
            grid_dims: (1, 1, 1),
            block_dims: (256, 1, 1),
            shared_memory_size: 0,
        }
    }
}

/// Check if VIR instruction is GPU-safe
pub fn is_gpu_safe_instruction(inst: &VirInstruction) -> bool {
    use VirInstruction::*;

    match inst {
        // Safe: Constants
        ConstInt { .. } | ConstFloat { .. } | ConstBool { .. } | ConstNull { .. } => true,
        ConstString { .. } => true, // String addresses are safe GPU targets internally

        // Safe: Arithmetic
        IntBinOp { .. } | FloatBinOp { .. } | IntUnOp { .. } | FloatUnOp { .. } => true,
        IntCmp { .. } | FloatCmp { .. } => true,

        // Safe: Memory operations (but with caveats for shared memory)
        Load { .. } | Store { .. } | Alloc { .. } => true,
        LoadLocal { .. } | StoreLocal { .. } => true,

        // Safe: Type conversions
        Cast { .. } | Bitcast { .. } => true,

        // Safe: Move/Copy operations
        Copy { .. } | Move { .. } => true,

        // Safe: Array operations
        BuildArray { .. } | ArrayIndex { .. } => true,

        // Safe: Tuple operations
        BuildTuple { .. } | ExtractTuple { .. } => true,

        // Safe: Object operations
        BuildObject { .. } => true,

        // Safe: Struct operations
        BuildStruct { .. } | ExtractField { .. } | InsertField { .. } => true,

        // Safe: Enum operations
        BuildEnum { .. } | GetDiscriminant { .. } | ExtractPayload { .. } => true,

        // Conditionally safe: Intrinsics
        Intrinsic { intrinsic, .. } => is_intrinsic_gpu_safe(intrinsic),

        // UNSAFE: I/O operations (print, file I/O are host-only)
        // These would be in intrinsics if they existed

        // UNSAFE: Dynamic memory allocation/free on host heap
        Free { .. } => false, // GPU kernels don't manage host heap

        // UNSAFE: Function calls (unless inlined or GPU-compatible)
        Call { .. } => false, // Indirect calls not supported on GPU

        // Safe: ARC operations (though typically not used in GPU kernels)
        ArcIncrement { .. } | ArcDecrement { .. } | ArcClone { .. } | ArcDrop { .. } => true,

        // Safe: Drop operations
        Drop { .. } => true,

        // No-op is always safe
        Nop => true,
    }
}

/// Check if intrinsic is GPU-safe
fn is_intrinsic_gpu_safe(intrinsic: &crate::ir::vir::Intrinsic) -> bool {
    use crate::ir::vir::Intrinsic::*;

    match intrinsic {
        // Safe: Math operations
        Sin | Cos | Tan | Log | Exp | Pow => true,

        // Safe: Memory operations (within GPU memory)
        MemCopy | MemMove | MemSet => true,

        // Safe: Atomic operations (GPU supports these)
        AtomicLoad | AtomicStore | AtomicCAS | AtomicAdd => true,

        // Safe: Bit operations
        CountOnes | CountZeros | LeadingZeros | TrailingZeros => true,

        // Safe: Size/alignment (compile-time constants)
        SizeOf | AlignOf => true,
    }
}

/// Check if operation is GPU-safe (legacy string-based API)
pub fn is_gpu_safe(_operation: &str) -> bool {
    // Legacy API for backward compatibility
    // Most basic operations are safe
    true
}

// ═══ SSA Helpers for GPU Kernel Lowering ═══

/// Generate SSA reference name for a value ID
fn ssa_ref_gpu(value_id: u32, name_map: &HashMap<u32, String>) -> String {
    name_map
        .get(&value_id)
        .cloned()
        .unwrap_or_else(|| format!("_{}", value_id))
}

/// Generate SSA definition name for a value ID
fn ssa_def_gpu(value_id: u32, name_map: &mut HashMap<u32, String>, fresh: &mut u32) -> String {
    let name = format!("_u{}", *fresh);
    *fresh += 1;
    name_map.insert(value_id, name.clone());
    name
}

/// Lower VIR type to MLIR type string (GPU dialect compatible)
fn lower_type_gpu(ty: &VirType) -> String {
    use VirType::*;
    match ty {
        Void => "i64".to_string(), // Treat void as i64 for GPU
        I8 => "i8".to_string(),
        I16 => "i16".to_string(),
        I32 => "i32".to_string(),
        I64 => "i64".to_string(),
        I128 => "i128".to_string(),
        U8 => "i8".to_string(),
        U16 => "i16".to_string(),
        U32 => "i32".to_string(),
        U64 => "i64".to_string(),
        U128 => "i128".to_string(),
        F32 => "f32".to_string(),
        F64 => "f64".to_string(),
        Bool => "i1".to_string(),
        Ptr | TypedPtr(_) => "!llvm.ptr".to_string(),
        Struct(_) => "!llvm.ptr".to_string(),
        Enum(_) => "!llvm.ptr".to_string(),
        Array { .. } => "!llvm.ptr".to_string(),
        Tuple(_) => "!llvm.ptr".to_string(),
        FuncPtr { .. } => "!llvm.ptr".to_string(),
    }
}

/// Lower a single VIR instruction to GPU MLIR
fn lower_instruction_gpu(
    inst: &VirInstruction,
    name_map: &mut HashMap<u32, String>,
    fresh: &mut u32,
    opt_ctx: &mut GpuOptimizationContext,
) -> BackendResult<Option<String>> {
    use VirInstruction::*;

    // Validate GPU safety (skip for performance, but log unsafe operations)
    if !is_gpu_safe_instruction(inst) {
        // For now, emit a warning comment but allow the operation
        // In production, this might be a hard error
        return Ok(Some(format!(
            "// WARNING: Potentially unsafe GPU operation: {:?}",
            inst
        )));
    }

    match inst {
        // ── Copy: Handle value aliases ──
        Copy { dest, src } => {
            let src_name = ssa_ref_gpu(*src, name_map);
            name_map.insert(*dest, src_name);
            Ok(None)
        }

        // ── Constants ──
        ConstInt { dest, value, ty } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : {}",
                d,
                value,
                lower_type_gpu(ty)
            )))
        }
        ConstFloat { dest, value, ty } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : {}",
                d,
                value,
                lower_type_gpu(ty)
            )))
        }
        ConstBool { dest, value } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : i1",
                d,
                if *value { "true" } else { "false" }
            )))
        }
        ConstString { dest, string_id } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.mlir.addressof @str_{} : !llvm.ptr",
                d, string_id
            )))
        }
        ConstNull { dest } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!("%{} = llvm.mlir.zero : !llvm.ptr", d)))
        }

        // ── Integer arithmetic ──
        IntBinOp {
            dest,
            op,
            lhs,
            rhs,
            ty,
        } => {
            use crate::ir::vir::IntBinOp as IntOp;
            let mlir_op = match op {
                IntOp::Add => "arith.addi",
                IntOp::Sub => "arith.subi",
                IntOp::Mul => "arith.muli",
                IntOp::Div => "arith.divsi",
                IntOp::Rem => "arith.remsi",
                IntOp::And => "arith.andi",
                IntOp::Or => "arith.ori",
                IntOp::Xor => "arith.xori",
                IntOp::Shl => "arith.shli",
                IntOp::Shr => "arith.shrsi",
            };
            let l = ssa_ref_gpu(*lhs, name_map);
            let r = ssa_ref_gpu(*rhs, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = {} %{}, %{} : {}",
                d,
                mlir_op,
                l,
                r,
                lower_type_gpu(ty)
            )))
        }

        // ── Float arithmetic ──
        FloatBinOp {
            dest,
            op,
            lhs,
            rhs,
            ty,
        } => {
            use crate::ir::vir::FloatBinOp as FloatOp;
            let mlir_op = match op {
                FloatOp::Add => "arith.addf",
                FloatOp::Sub => "arith.subf",
                FloatOp::Mul => "arith.mulf",
                FloatOp::Div => "arith.divf",
            };
            let l = ssa_ref_gpu(*lhs, name_map);
            let r = ssa_ref_gpu(*rhs, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = {} %{}, %{} : {}",
                d,
                mlir_op,
                l,
                r,
                lower_type_gpu(ty)
            )))
        }

        // ── Unary operations ──
        IntUnOp {
            dest,
            op,
            operand,
            ty,
        } => {
            use crate::ir::vir::IntUnOp;
            let operand_name = ssa_ref_gpu(*operand, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mlir_ty = lower_type_gpu(ty);
            match op {
                IntUnOp::Neg => {
                    // -x = 0 - x
                    let zero = ssa_def_gpu(u32::MAX, name_map, fresh);
                    Ok(Some(format!(
                        "%{} = arith.constant 0 : {}\n      %{} = arith.subi %{}, %{} : {}",
                        zero, mlir_ty, d, zero, operand_name, mlir_ty
                    )))
                }
                IntUnOp::Not => {
                    // ~x (bitwise not)
                    Ok(Some(format!(
                        "%{} = arith.xori %{}, -1 : {}",
                        d, operand_name, mlir_ty
                    )))
                }
            }
        }
        FloatUnOp {
            dest,
            op,
            operand,
            ty,
        } => {
            use crate::ir::vir::FloatUnOp;
            let operand_name = ssa_ref_gpu(*operand, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mlir_ty = lower_type_gpu(ty);
            match op {
                FloatUnOp::Neg => Ok(Some(format!(
                    "%{} = arith.negf %{} : {}",
                    d, operand_name, mlir_ty
                ))),
                FloatUnOp::Abs => Ok(Some(format!(
                    "%{} = math.absf %{} : {}",
                    d, operand_name, mlir_ty
                ))),
                FloatUnOp::Sqrt => Ok(Some(format!(
                    "%{} = math.sqrt %{} : {}",
                    d, operand_name, mlir_ty
                ))),
            }
        }

        // ── Comparisons ──
        IntCmp { dest, op, lhs, rhs } => {
            use crate::ir::vir::CmpOp;
            let pred = match op {
                CmpOp::Eq => "eq",
                CmpOp::Ne => "ne",
                CmpOp::Lt => "slt",
                CmpOp::Le => "sle",
                CmpOp::Gt => "sgt",
                CmpOp::Ge => "sge",
            };
            let l = ssa_ref_gpu(*lhs, name_map);
            let r = ssa_ref_gpu(*rhs, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.cmpi {}, %{}, %{} : i64",
                d, pred, l, r
            )))
        }
        FloatCmp { dest, op, lhs, rhs } => {
            use crate::ir::vir::CmpOp;
            let pred = match op {
                CmpOp::Eq => "oeq",
                CmpOp::Ne => "one",
                CmpOp::Lt => "olt",
                CmpOp::Le => "ole",
                CmpOp::Gt => "ogt",
                CmpOp::Ge => "oge",
            };
            let l = ssa_ref_gpu(*lhs, name_map);
            let r = ssa_ref_gpu(*rhs, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.cmpf {}, %{}, %{} : f64",
                d, pred, l, r
            )))
        }

        // ── Memory operations (Phase 5.1: Memory space attribution) ──
        Load { dest, ptr, ty } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mem_space = opt_ctx.get_memory_space(*ptr);
            let ty_str = lower_type_gpu(ty);

            // Add memory space annotation for GPU optimization
            let load_str = match mem_space {
                MemorySpace::Global => {
                    format!(
                        "%{} = llvm.load %{} {{alignment = 4 : i64}} : !llvm.ptr<0> -> {}",
                        d, ptr_name, ty_str
                    )
                }
                MemorySpace::Unified => {
                    format!(
                        "%{} = llvm.load %{} {{alignment = 4 : i64}} : !llvm.ptr<1> -> {}",
                        d, ptr_name, ty_str
                    )
                }
                MemorySpace::Local => {
                    // Shared memory access - may need barrier
                    format!(
                        "%{} = llvm.load %{} {{alignment = 4 : i64}} : !llvm.ptr<3> -> {}",
                        d, ptr_name, ty_str
                    )
                }
                MemorySpace::Private => {
                    format!(
                        "%{} = llvm.load %{} {{alignment = 4 : i64}} : !llvm.ptr<5> -> {}",
                        d, ptr_name, ty_str
                    )
                }
            };
            Ok(Some(load_str))
        }
        Store { ptr, value } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            let val_name = ssa_ref_gpu(*value, name_map);
            let mem_space = opt_ctx.get_memory_space(*ptr);

            // Add memory space annotation for GPU optimization
            let store_str = match mem_space {
                MemorySpace::Global => {
                    format!(
                        "llvm.store %{}, %{} {{alignment = 4 : i64}} : i64, !llvm.ptr<0>",
                        val_name, ptr_name
                    )
                }
                MemorySpace::Unified => {
                    format!(
                        "llvm.store %{}, %{} {{alignment = 4 : i64}} : i64, !llvm.ptr<1>",
                        val_name, ptr_name
                    )
                }
                MemorySpace::Local => {
                    // Shared memory access - may need barrier
                    format!(
                        "llvm.store %{}, %{} {{alignment = 4 : i64}} : i64, !llvm.ptr<3>",
                        val_name, ptr_name
                    )
                }
                MemorySpace::Private => {
                    format!(
                        "llvm.store %{}, %{} {{alignment = 4 : i64}} : i64, !llvm.ptr<5>",
                        val_name, ptr_name
                    )
                }
            };
            Ok(Some(store_str))
        }

        // ── Allocation (Phase 5.1: Memory space attribution) ──
        Alloc { dest, ty, size } => {
            let size_name = ssa_ref_gpu(*size, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let ty_str = lower_type_gpu(ty);

            // Allocate in global memory by default
            // For shared memory, would need explicit annotation
            opt_ctx.set_memory_space(*dest, MemorySpace::Global);

            Ok(Some(format!(
                "%{} = llvm.alloca %{} x {} {{alignment = 16 : i64}} : (i64) -> !llvm.ptr<0>",
                d, size_name, ty_str
            )))
        }

        // ── Free: Explicit deallocation ──
        Free { ptr } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            // GPU typically doesn't need explicit free in kernels
            // Memory is managed by the host
            Ok(Some(format!("// Free: %{} (managed by host)", ptr_name)))
        }

        LoadLocal { dest, local } => {
            // For GPU, treat locals as registers/SSA values
            // This is a simplified model - real implementation might use shared memory
            let d = ssa_def_gpu(*dest, name_map, fresh);
            opt_ctx.set_memory_space(*dest, MemorySpace::Private);
            Ok(Some(format!("// LoadLocal: %{} = local_{}", d, local)))
        }
        StoreLocal { local, value } => {
            let val_name = ssa_ref_gpu(*value, name_map);
            Ok(Some(format!(
                "// StoreLocal: local_{} = %{}",
                local, val_name
            )))
        }

        // ── Type operations ──
        Cast {
            dest,
            value,
            from_ty,
            to_ty,
        } => {
            let val_name = ssa_ref_gpu(*value, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let from_mlir = lower_type_gpu(from_ty);
            let to_mlir = lower_type_gpu(to_ty);

            // Determine cast operation based on types
            use VirType::*;
            let cast_op = match (from_ty, to_ty) {
                // Integer to integer casts
                (
                    I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64,
                    I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64,
                ) => {
                    "arith.extsi" // Sign extend
                }
                // Float to float casts
                (F32, F64) => "arith.extf",
                (F64, F32) => "arith.truncf",
                // Integer to float
                (I8 | I16 | I32 | I64, F32 | F64) => "arith.sitofp",
                (U8 | U16 | U32 | U64, F32 | F64) => "arith.uitofp",
                // Float to integer
                (F32 | F64, I8 | I16 | I32 | I64) => "arith.fptosi",
                (F32 | F64, U8 | U16 | U32 | U64) => "arith.fptoui",
                // Fallback to bitcast for complex types
                _ => "llvm.bitcast",
            };
            Ok(Some(format!(
                "%{} = {} %{} : {} to {}",
                d, cast_op, val_name, from_mlir, to_mlir
            )))
        }
        Bitcast { dest, value, to_ty } => {
            let val_name = ssa_ref_gpu(*value, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let to_mlir = lower_type_gpu(to_ty);
            Ok(Some(format!(
                "%{} = llvm.bitcast %{} : !llvm.ptr to {}",
                d, val_name, to_mlir
            )))
        }

        // ── Aggregate operations ──
        BuildArray {
            dest,
            elem_ty,
            elements,
        } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let ty_str = lower_type_gpu(elem_ty);
            let mut code = format!(
                "%{}_arr = llvm.mlir.undef : !llvm.array<{} x {}>\n",
                d,
                elements.len(),
                ty_str
            );
            for (i, elem) in elements.iter().enumerate() {
                let elem_name = ssa_ref_gpu(*elem, name_map);
                code.push_str(&format!(
                    "      %{}_{} = llvm.insertvalue %{}, %{}_arr[{}] : !llvm.array<{} x {}>\n",
                    d,
                    i,
                    elem_name,
                    d,
                    i,
                    elements.len(),
                    ty_str
                ));
            }
            if !elements.is_empty() {
                code.push_str(&format!("      %{} = %{}_{}\n", d, d, elements.len() - 1));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ArrayIndex { dest, array, index } => {
            let arr_name = ssa_ref_gpu(*array, name_map);
            let idx_name = ssa_ref_gpu(*index, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.getelementptr %{}[%{}] : (!llvm.ptr, i64) -> !llvm.ptr",
                d, arr_name, idx_name
            )))
        }
        BuildStruct {
            dest,
            ty: _,
            fields,
            ..
        } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mut code = format!(
                "%{}_s = llvm.mlir.undef : !llvm.struct<({})>\n",
                d,
                fields.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
            );
            for (i, field) in fields.iter().enumerate() {
                let field_name = ssa_ref_gpu(*field, name_map);
                code.push_str(&format!(
                    "      %{}_{} = llvm.insertvalue %{}, %{}_s[{}] : !llvm.struct<({})>\n",
                    d,
                    i,
                    field_name,
                    d,
                    i,
                    fields.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
                ));
            }
            if !fields.is_empty() {
                code.push_str(&format!("      %{} = %{}_{}\n", d, d, fields.len() - 1));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ExtractField {
            dest,
            struct_val,
            field,
        } => {
            let struct_name = ssa_ref_gpu(*struct_val, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[{}] : !llvm.struct<()>",
                d, struct_name, field
            )))
        }
        InsertField {
            dest,
            struct_val,
            field,
            value,
        } => {
            let struct_name = ssa_ref_gpu(*struct_val, name_map);
            let val_name = ssa_ref_gpu(*value, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.insertvalue %{}, %{}[{}] : !llvm.struct<()>",
                d, val_name, struct_name, field
            )))
        }
        BuildTuple { dest, elements } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mut code = format!(
                "%{}_t = llvm.mlir.undef : !llvm.struct<({})>\n",
                d,
                elements
                    .iter()
                    .map(|_| "i64")
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            for (i, elem) in elements.iter().enumerate() {
                let elem_name = ssa_ref_gpu(*elem, name_map);
                code.push_str(&format!(
                    "      %{}_{} = llvm.insertvalue %{}, %{}_t[{}] : !llvm.struct<({})>\n",
                    d,
                    i,
                    elem_name,
                    d,
                    i,
                    elements
                        .iter()
                        .map(|_| "i64")
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !elements.is_empty() {
                code.push_str(&format!("      %{} = %{}_{}\n", d, d, elements.len() - 1));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ExtractTuple { dest, tuple, index } => {
            let tuple_name = ssa_ref_gpu(*tuple, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[{}] : !llvm.struct<()>",
                d, tuple_name, index
            )))
        }

        BuildObject {
            dest,
            keys: _,
            values: _,
        } => {
            // Objects are runtime values in GPU - use placeholder pointer
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!("%{} = llvm.mlir.undef : !llvm.ptr", d)))
        }

        // ── Enum operations ──
        BuildEnum {
            dest,
            ty: _,
            variant,
            payload,
        } => {
            let d = ssa_def_gpu(*dest, name_map, fresh);
            let mut code = format!("%{}_disc = arith.constant {} : i32\n", d, variant);
            code.push_str(&format!(
                "      %{}_enum = llvm.mlir.undef : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n",
                d
            ));
            code.push_str(&format!("      %{}_0 = llvm.insertvalue %{}_disc, %{}_enum[0] : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n", d, d, d));
            if !payload.is_empty() {
                let p = ssa_ref_gpu(payload[0], name_map);
                code.push_str(&format!("      %{} = llvm.insertvalue %{}, %{}_0[1] : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n", d, p, d));
            } else {
                code.push_str(&format!("      %{} = %{}_0\n", d, d));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        GetDiscriminant { dest, enum_val } => {
            let enum_name = ssa_ref_gpu(*enum_val, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[0] : !llvm.struct<(i32, !llvm.array<16 x i8>)>",
                d, enum_name
            )))
        }
        ExtractPayload {
            dest,
            enum_val,
            variant: _,
        } => {
            let enum_name = ssa_ref_gpu(*enum_val, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[1] : !llvm.struct<(i32, !llvm.array<16 x i8>)>",
                d, enum_name
            )))
        }

        // ── Intrinsics ──
        Intrinsic {
            dest,
            intrinsic,
            args,
        } => {
            use crate::ir::vir::Intrinsic as Intr;
            let arg_refs: Vec<String> = args.iter().map(|a| ssa_ref_gpu(*a, name_map)).collect();

            let code = match intrinsic {
                // Math operations
                Intr::Sin => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = math.sin %{} : f64", dn, arg_refs[0])
                        } else {
                            "// ERROR: Sin requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: Sin requires dest".to_string()
                    }
                }
                Intr::Cos => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = math.cos %{} : f64", dn, arg_refs[0])
                        } else {
                            "// ERROR: Cos requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: Cos requires dest".to_string()
                    }
                }
                Intr::Tan => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = math.tan %{} : f64", dn, arg_refs[0])
                        } else {
                            "// ERROR: Tan requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: Tan requires dest".to_string()
                    }
                }
                Intr::Log => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = math.log %{} : f64", dn, arg_refs[0])
                        } else {
                            "// ERROR: Log requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: Log requires dest".to_string()
                    }
                }
                Intr::Exp => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = math.exp %{} : f64", dn, arg_refs[0])
                        } else {
                            "// ERROR: Exp requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: Exp requires dest".to_string()
                    }
                }
                Intr::Pow => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if arg_refs.len() >= 2 {
                            format!(
                                "%{} = math.pow %{}, %{} : f64",
                                dn, arg_refs[0], arg_refs[1]
                            )
                        } else {
                            "// ERROR: Pow requires 2 args".to_string()
                        }
                    } else {
                        "// ERROR: Pow requires dest".to_string()
                    }
                }
                // Memory operations
                Intr::MemCopy => {
                    if arg_refs.len() >= 3 {
                        format!(
                            "llvm.call @memcpy(%{}, %{}, %{}) : (!llvm.ptr, !llvm.ptr, i64) -> ()",
                            arg_refs[0], arg_refs[1], arg_refs[2]
                        )
                    } else {
                        "// ERROR: MemCopy requires 3 args".to_string()
                    }
                }
                Intr::MemMove => {
                    if arg_refs.len() >= 3 {
                        format!(
                            "llvm.call @memmove(%{}, %{}, %{}) : (!llvm.ptr, !llvm.ptr, i64) -> ()",
                            arg_refs[0], arg_refs[1], arg_refs[2]
                        )
                    } else {
                        "// ERROR: MemMove requires 3 args".to_string()
                    }
                }
                Intr::MemSet => {
                    if arg_refs.len() >= 3 {
                        format!(
                            "llvm.call @memset(%{}, %{}, %{}) : (!llvm.ptr, i32, i64) -> ()",
                            arg_refs[0], arg_refs[1], arg_refs[2]
                        )
                    } else {
                        "// ERROR: MemSet requires 3 args".to_string()
                    }
                }
                // Atomic operations
                Intr::AtomicLoad => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!(
                                "%{} = llvm.load atomic %{} seq_cst : !llvm.ptr -> i64",
                                dn, arg_refs[0]
                            )
                        } else {
                            "// ERROR: AtomicLoad requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: AtomicLoad requires dest".to_string()
                    }
                }
                Intr::AtomicStore => {
                    if arg_refs.len() >= 2 {
                        format!(
                            "llvm.store atomic %{}, %{} seq_cst : i64, !llvm.ptr",
                            arg_refs[1], arg_refs[0]
                        )
                    } else {
                        "// ERROR: AtomicStore requires 2 args".to_string()
                    }
                }
                Intr::AtomicCAS => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if arg_refs.len() >= 3 {
                            format!(
                                "%{} = llvm.cmpxchg %{}, %{}, %{} seq_cst seq_cst : !llvm.ptr, i64",
                                dn, arg_refs[0], arg_refs[1], arg_refs[2]
                            )
                        } else {
                            "// ERROR: AtomicCAS requires 3 args".to_string()
                        }
                    } else {
                        "// ERROR: AtomicCAS requires dest".to_string()
                    }
                }
                Intr::AtomicAdd => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if arg_refs.len() >= 2 {
                            format!(
                                "%{} = llvm.atomicrmw add %{}, %{} seq_cst : !llvm.ptr, i64",
                                dn, arg_refs[0], arg_refs[1]
                            )
                        } else {
                            "// ERROR: AtomicAdd requires 2 args".to_string()
                        }
                    } else {
                        "// ERROR: AtomicAdd requires dest".to_string()
                    }
                }
                // Bit operations
                Intr::CountOnes => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = llvm.intr.ctpop(%{}) : (i64) -> i64", dn, arg_refs[0])
                        } else {
                            "// ERROR: CountOnes requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: CountOnes requires dest".to_string()
                    }
                }
                Intr::LeadingZeros => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!(
                                "%{} = llvm.intr.ctlz(%{}, 0) : (i64, i1) -> i64",
                                dn, arg_refs[0]
                            )
                        } else {
                            "// ERROR: LeadingZeros requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: LeadingZeros requires dest".to_string()
                    }
                }
                Intr::TrailingZeros => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!(
                                "%{} = llvm.intr.cttz(%{}, 0) : (i64, i1) -> i64",
                                dn, arg_refs[0]
                            )
                        } else {
                            "// ERROR: TrailingZeros requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: TrailingZeros requires dest".to_string()
                    }
                }
                Intr::CountZeros => {
                    // CountZeros = 64 - CountOnes (for 64-bit integers)
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            let tmp = ssa_def_gpu(u32::MAX - 1, name_map, fresh);
                            format!(
                                "%{} = llvm.intr.ctpop(%{}) : (i64) -> i64\n      %{}_c = arith.constant 64 : i64\n      %{} = arith.subi %{}_c, %{} : i64",
                                tmp, arg_refs[0], dn, dn, dn, tmp
                            )
                        } else {
                            "// ERROR: CountZeros requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: CountZeros requires dest".to_string()
                    }
                }
                // Size/alignment (compile-time constants)
                Intr::SizeOf | Intr::AlignOf => {
                    if let Some(d) = dest {
                        let dn = ssa_def_gpu(*d, name_map, fresh);
                        // SizeOf/AlignOf should be resolved at VIR lowering time to constants
                        // For now, emit a constant 8 (typical pointer/i64 size)
                        format!("%{} = arith.constant 8 : i64", dn)
                    } else {
                        "// ERROR: SizeOf/AlignOf requires dest".to_string()
                    }
                }
            };
            Ok(Some(code))
        }

        // ── ARC operations ──
        ArcIncrement { ptr } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            // ARC operations are typically not used in GPU kernels
            Ok(Some(format!(
                "// ARC increment: %{} (skipped on GPU)",
                ptr_name
            )))
        }
        ArcDecrement { ptr } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            Ok(Some(format!(
                "// ARC decrement: %{} (skipped on GPU)",
                ptr_name
            )))
        }
        ArcClone { dest, src } => {
            let src_name = ssa_ref_gpu(*src, name_map);
            let d = ssa_def_gpu(*dest, name_map, fresh);
            Ok(Some(format!(
                "// ARC clone: %{} = %{} (skipped on GPU)",
                d, src_name
            )))
        }
        ArcDrop { ptr } => {
            let ptr_name = ssa_ref_gpu(*ptr, name_map);
            Ok(Some(format!("// ARC drop: %{} (skipped on GPU)", ptr_name)))
        }

        // ── Drop ──
        Drop { value } => {
            let val_name = ssa_ref_gpu(*value, name_map);
            Ok(Some(format!("// Drop: %{} (skipped on GPU)", val_name)))
        }

        // ── Function calls ──
        Call { dest, func, args } => {
            // Function calls are generally not supported on GPU
            // unless they're inlined or GPU-compatible device functions
            let func_name = ssa_ref_gpu(*func, name_map);
            let arg_refs: Vec<String> = args.iter().map(|a| ssa_ref_gpu(*a, name_map)).collect();
            if let Some(d) = dest {
                let dn = ssa_def_gpu(*d, name_map, fresh);
                Ok(Some(format!(
                    "// WARNING: Indirect call on GPU: %{} = call %{}({})",
                    dn,
                    func_name,
                    arg_refs.join(", ")
                )))
            } else {
                Ok(Some(format!(
                    "// WARNING: Indirect call on GPU: call %{}({})",
                    func_name,
                    arg_refs.join(", ")
                )))
            }
        }

        // ── Move (alias) ──
        Move { dest, src } => {
            let src_name = ssa_ref_gpu(*src, name_map);
            name_map.insert(*dest, src_name);
            Ok(None)
        }

        // ── No-op ──
        Nop => Ok(None),
    }
}

/// Lower a VIR block to GPU MLIR
fn lower_block_gpu(
    block: &VirBlock,
    name_map: &mut HashMap<u32, String>,
    fresh: &mut u32,
    opt_ctx: &mut GpuOptimizationContext,
    indent: &str,
) -> BackendResult<String> {
    let mut mlir = String::new();

    // Phase 5.2: Insert barrier synchronization if needed
    if opt_ctx.barrier_points.contains(&block.id) {
        mlir.push_str(&format!("{}gpu.barrier\n", indent));
    }

    // Lower PHI nodes
    for phi in &block.phis {
        let d = ssa_def_gpu(phi.dest, name_map, fresh);
        let ty = lower_type_gpu(&phi.ty);
        let mut incoming_str = String::new();
        for (i, (pred_id, value_id)) in phi.incoming.iter().enumerate() {
            if i > 0 {
                incoming_str.push_str(", ");
            }
            let v = ssa_ref_gpu(*value_id, name_map);
            incoming_str.push_str(&format!("bb{}: %{}", pred_id, v));
        }
        mlir.push_str(&format!(
            "{}%{} = phi {} [{}]\n",
            indent, d, ty, incoming_str
        ));
    }

    // Lower instructions
    for inst in &block.instructions {
        if let Some(inst_mlir) = lower_instruction_gpu(inst, name_map, fresh, opt_ctx)? {
            mlir.push_str(&format!("{}{}\n", indent, inst_mlir));
        }
    }

    // Lower terminator
    mlir.push_str(&lower_terminator_gpu(&block.terminator, name_map, indent)?);

    Ok(mlir)
}

/// Lower a VIR terminator to GPU MLIR
fn lower_terminator_gpu(
    term: &VirTerminator,
    name_map: &HashMap<u32, String>,
    indent: &str,
) -> BackendResult<String> {
    use VirTerminator::*;

    Ok(match term {
        Return { value } => {
            if let Some(v) = value {
                let val_name = ssa_ref_gpu(*v, name_map);
                format!("{}gpu.return %{} : i64\n", indent, val_name)
            } else {
                format!("{}gpu.return\n", indent)
            }
        }
        Jump { target } => {
            format!("{}cf.br ^bb{}\n", indent, target)
        }
        Branch {
            cond,
            true_target,
            false_target,
        } => {
            let cond_name = ssa_ref_gpu(*cond, name_map);
            format!(
                "{}cf.cond_br %{}, ^bb{}, ^bb{}\n",
                indent, cond_name, true_target, false_target
            )
        }
        Switch {
            value,
            cases,
            default,
        } => {
            let val_name = ssa_ref_gpu(*value, name_map);
            let mut case_list = String::new();
            let mut dest_list = String::new();
            for (i, (case_val, target)) in cases.iter().enumerate() {
                if i > 0 {
                    case_list.push_str(", ");
                    dest_list.push_str(", ");
                }
                case_list.push_str(&case_val.to_string());
                dest_list.push_str(&format!("^bb{}", target));
            }
            if !cases.is_empty() {
                format!(
                    "{}cf.switch %{} : i64, [\n{}  default: ^bb{},\n{}  {}: {}\n{}]\n",
                    indent, val_name, indent, default, indent, case_list, dest_list, indent
                )
            } else {
                format!("{}cf.br ^bb{}\n", indent, default)
            }
        }
        Unreachable => {
            format!("{}llvm.unreachable\n", indent)
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// Phase 5: GPU Optimization Analysis
// ═══════════════════════════════════════════════════════════════════

/// Analyze GPU kernel for divergence (Phase 5.4)
fn analyze_divergence(function: &VirFunction, opt_ctx: &mut GpuOptimizationContext) {
    for block in &function.blocks {
        use VirTerminator::*;
        match &block.terminator {
            Branch { cond: _, .. } => {
                // Conditional branch can cause divergence
                // Mark this as potentially divergent
                opt_ctx.mark_divergent(block.id);
            }
            Switch { .. } => {
                // Switch statement highly likely to cause divergence
                opt_ctx.mark_divergent(block.id);
            }
            _ => {}
        }
    }
}

/// Analyze memory access patterns (Phase 5.3)
fn analyze_memory_patterns(function: &VirFunction, opt_ctx: &mut GpuOptimizationContext) {
    for block in &function.blocks {
        for inst in &block.instructions {
            use VirInstruction::*;
            match inst {
                Load { dest, ptr: _, .. } => {
                    // Analyze access pattern based on pointer computation
                    // For now, assume unknown pattern
                    opt_ctx
                        .memory_patterns
                        .insert(*dest, MemoryAccessPattern::Unknown);

                    // Check if this is an array access with thread ID
                    // (would indicate coalesced access)
                    // This is a simplified heuristic
                }
                ArrayIndex {
                    dest,
                    array: _,
                    index: _,
                } => {
                    // Array indexing - check if index is thread-dependent
                    // Linear thread index → likely coalesced
                    // This is a heuristic - real analysis would be more sophisticated
                    opt_ctx
                        .memory_patterns
                        .insert(*dest, MemoryAccessPattern::Unknown);
                }
                _ => {}
            }
        }
    }
}

/// Insert barrier synchronization points (Phase 5.2)
fn insert_barriers(function: &VirFunction, opt_ctx: &mut GpuOptimizationContext) {
    // Scan for shared memory usage patterns
    for (block_idx, block) in function.blocks.iter().enumerate() {
        let mut has_shared_write = false;
        let mut _has_shared_read = false;

        for inst in &block.instructions {
            use VirInstruction::*;
            match inst {
                Store { ptr, .. } => {
                    if opt_ctx.get_memory_space(*ptr) == MemorySpace::Local {
                        has_shared_write = true;
                    }
                }
                Load { ptr, .. } => {
                    if opt_ctx.get_memory_space(*ptr) == MemorySpace::Local {
                        _has_shared_read = true;
                    }
                }
                _ => {}
            }
        }

        // If this block writes to shared memory followed by reads,
        // insert a barrier after writes
        if has_shared_write {
            // Check if next block reads from shared memory
            if block_idx + 1 < function.blocks.len() {
                let next_block_id = function.blocks[block_idx + 1].id;
                opt_ctx.add_barrier(next_block_id);
            }
        }
    }
}

/// Generate optimization warnings and hints
fn generate_optimization_report(opt_ctx: &GpuOptimizationContext) -> String {
    let mut report = String::new();

    report.push_str("// ═══ GPU Optimization Report (Phase 5) ═══\n");

    // Phase 5.1: Memory space usage
    let global_count = opt_ctx
        .memory_spaces
        .values()
        .filter(|&&s| s == MemorySpace::Global)
        .count();
    let local_count = opt_ctx
        .memory_spaces
        .values()
        .filter(|&&s| s == MemorySpace::Local)
        .count();
    let private_count = opt_ctx
        .memory_spaces
        .values()
        .filter(|&&s| s == MemorySpace::Private)
        .count();

    report.push_str(&format!(
        "// Memory Spaces: Global={}, Shared/Local={}, Private={}\n",
        global_count, local_count, private_count
    ));

    // Phase 5.2: Barrier synchronization
    if !opt_ctx.barrier_points.is_empty() {
        report.push_str(&format!(
            "// Barriers inserted: {} points\n",
            opt_ctx.barrier_points.len()
        ));
    }

    // Phase 5.3: Memory access patterns
    let _coalesced = opt_ctx
        .memory_patterns
        .values()
        .filter(|p| **p == MemoryAccessPattern::Coalesced)
        .count();
    if !opt_ctx.memory_patterns.is_empty() {
        report.push_str(&format!(
            "// Memory patterns analyzed: {} operations\n",
            opt_ctx.memory_patterns.len()
        ));
    }

    // Phase 5.4: Divergence warnings
    if !opt_ctx.divergent_branches.is_empty() {
        report.push_str(&format!(
            "// WARNING: {} potentially divergent branches detected\n",
            opt_ctx.divergent_branches.len()
        ));
        report.push_str("// Tip: Minimize branching for better GPU performance\n");
    }

    if opt_ctx.shared_memory_used {
        report.push_str("// INFO: Shared memory in use - barriers inserted for correctness\n");
    }

    report.push_str("// ═══════════════════════════════════════════\n");

    report
}

/// Generate GPU kernel from VIR function
pub fn generate_gpu_kernel(
    function: &VirFunction,
    _config: &GpuKernelConfig,
) -> BackendResult<String> {
    let mut mlir = String::new();

    // ══════════════════════════════════════════════════════════════
    // Phase 5: GPU Optimization Context
    // ══════════════════════════════════════════════════════════════
    let mut opt_ctx = GpuOptimizationContext::new();

    // Phase 5.4: Analyze divergence
    analyze_divergence(function, &mut opt_ctx);

    // Phase 5.3: Analyze memory access patterns
    analyze_memory_patterns(function, &mut opt_ctx);

    // Phase 5.2: Insert barrier synchronization
    insert_barriers(function, &mut opt_ctx);

    // Generate optimization report
    let opt_report = generate_optimization_report(&opt_ctx);
    mlir.push_str(&opt_report);
    mlir.push_str("\n");

    mlir.push_str(&format!("  gpu.module @{}_gpu {{\n", function.name));
    mlir.push_str(&format!("    gpu.func @{}_kernel(", function.name));

    // Emit parameters
    for (i, param) in function.params.iter().enumerate() {
        if i > 0 {
            mlir.push_str(", ");
        }
        mlir.push_str(&format!("%arg{}: {}", i, lower_vir_type(&param.ty)));
    }

    mlir.push_str(") kernel {\n");

    // TODO Phase 4.1: Emit actual kernel body from VIR blocks
    // For now, emit basic structure with thread ID computation

    // Get thread indices (useful for parallel computation)
    mlir.push_str("      // Thread IDs (for future parallel work distribution)\n");
    mlir.push_str("      %thread_id_x = gpu.thread_id x\n");
    mlir.push_str("      %thread_id_y = gpu.thread_id y\n");
    mlir.push_str("      %thread_id_z = gpu.thread_id z\n");
    mlir.push_str("      %block_id_x = gpu.block_id x\n");
    mlir.push_str("      %block_id_y = gpu.block_id y\n");
    mlir.push_str("      %block_id_z = gpu.block_id z\n");

    // ══════════════════════════════════════════════════════════════
    // Phase 4.1: Emit actual kernel body from VIR blocks
    // ══════════════════════════════════════════════════════════════

    // Create SSA name mapping context
    let mut name_map: HashMap<u32, String> = HashMap::new();
    let mut fresh = 0u32;

    // Note: Function parameters are already bound as %arg0, %arg1, etc. in the function signature
    // and will be referenced by those names in the generated MLIR.

    // Lower all VIR blocks
    if !function.blocks.is_empty() {
        mlir.push_str("\n");
        for (block_idx, block) in function.blocks.iter().enumerate() {
            // Emit block label (skip for first block as it's the entry)
            if block_idx > 0 {
                mlir.push_str(&format!("    ^bb{}:\n", block.id));
            }

            // Lower the block (Phase 5: with optimization context)
            let block_mlir =
                lower_block_gpu(block, &mut name_map, &mut fresh, &mut opt_ctx, "      ")?;
            mlir.push_str(&block_mlir);
        }
    } else {
        // Empty function - just return
        mlir.push_str("      gpu.return\n");
    }
    mlir.push_str("    }\n");
    mlir.push_str("  }\n");

    Ok(mlir)
}

/// Generate a host-side wrapper that launches the GPU kernel
pub fn generate_gpu_launch_wrapper(
    function: &VirFunction,
    config: &GpuKernelConfig,
    target: GpuTarget,
) -> BackendResult<String> {
    let mut mlir = String::new();

    mlir.push_str(&format!("  func.func @{}_gpu_launch(", function.name));
    for (i, param) in function.params.iter().enumerate() {
        if i > 0 {
            mlir.push_str(", ");
        }
        mlir.push_str(&format!("%arg{}: {}", i, lower_vir_type(&param.ty)));
    }
    mlir.push_str(") -> () {\n");

    mlir.push_str(&format!(
        "    %gx = arith.constant {} : index\n",
        config.grid_dims.0
    ));
    mlir.push_str(&format!(
        "    %gy = arith.constant {} : index\n",
        config.grid_dims.1
    ));
    mlir.push_str(&format!(
        "    %gz = arith.constant {} : index\n",
        config.grid_dims.2
    ));
    mlir.push_str(&format!(
        "    %bx = arith.constant {} : index\n",
        config.block_dims.0
    ));
    mlir.push_str(&format!(
        "    %by = arith.constant {} : index\n",
        config.block_dims.1
    ));
    mlir.push_str(&format!(
        "    %bz = arith.constant {} : index\n",
        config.block_dims.2
    ));

    let mut args = String::new();
    for (i, param) in function.params.iter().enumerate() {
        if i > 0 {
            args.push_str(", ");
        }
        args.push_str(&format!("%arg{} : {}", i, lower_vir_type(&param.ty)));
    }

    mlir.push_str(&format!(
        "    gpu.launch_func @{}_gpu::@{}_kernel blocks in (%gx, %gy, %gz) threads in (%bx, %by, %bz) args({})\n",
        function.name,
        function.name,
        args
    ));
    if target == GpuTarget::Cuda || target == GpuTarget::Metal {
        mlir.push_str("    // Zero-copy unified memory buffer enabled (CUDA Managed Memory / Metal Unified Memory)\n");
    }
    mlir.push_str(&format!("    // target: {}\n", target.as_str()));
    mlir.push_str("    return\n");
    mlir.push_str("  }\n");

    Ok(mlir)
}

/// Map memory space for GPU
pub fn map_memory_space(is_heap: bool) -> MemorySpace {
    if is_heap {
        MemorySpace::Global
    } else {
        MemorySpace::Local
    }
}

/// Check GPU availability
pub fn is_gpu_available() -> bool {
    let env_hits = env::var("CUDA_VISIBLE_DEVICES").is_ok()
        || env::var("ROCR_VISIBLE_DEVICES").is_ok()
        || env::var("HIP_VISIBLE_DEVICES").is_ok()
        || env::var("VK_ICD_FILENAMES").is_ok()
        || env::var("METAL_DEVICE_WRAPPER_TYPE").is_ok();

    env_hits || detect_toolchain(GpuTarget::Auto).is_available()
}

// ─────────────────────────────────────────────────────────────────────────────
// Device compatibility checking
// ─────────────────────────────────────────────────────────────────────────────

/// Overall status for a single compatibility check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    /// Feature confirmed present / working
    Ok,
    /// Feature not found but not required
    Missing,
    /// Feature is a hard requirement that is absent
    Error,
    /// Unable to determine (tool not available for probing)
    Unknown,
}

impl CheckStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "✓",
            CheckStatus::Missing => "–",
            CheckStatus::Error => "✗",
            CheckStatus::Unknown => "?",
        }
    }
}

/// One line in the compatibility report
#[derive(Debug, Clone)]
pub struct CheckEntry {
    pub category: &'static str,
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
}

/// Full compatibility report
#[derive(Debug, Clone)]
pub struct CompatibilityReport {
    pub entries: Vec<CheckEntry>,
}

impl CompatibilityReport {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn push(
        &mut self,
        category: &'static str,
        label: impl AsRef<str>,
        status: CheckStatus,
        detail: impl AsRef<str>,
    ) {
        self.entries.push(CheckEntry {
            category,
            label: label.as_ref().to_string(),
            status,
            detail: detail.as_ref().to_string(),
        });
    }

    /// Count Ok entries
    pub fn ok_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == CheckStatus::Ok)
            .count()
    }

    /// Count Error entries
    pub fn error_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == CheckStatus::Error)
            .count()
    }

    /// True if no hard errors
    pub fn compatible(&self) -> bool {
        self.error_count() == 0
    }
}

/// Run all device compatibility checks and return a report.
pub fn check_device_compatibility() -> CompatibilityReport {
    let mut report = CompatibilityReport::new();

    check_env_vars(&mut report);
    check_toolchain_tools(&mut report);
    check_nvidia(&mut report);
    check_rocm(&mut report);
    check_vulkan(&mut report);
    check_opencl(&mut report);
    check_metal(&mut report);
    check_driver_permissions(&mut report);

    report
}

// ── Environment variables ────────────────────────────────────────────────────

fn check_env_vars(r: &mut CompatibilityReport) {
    let vars = [
        ("CUDA_VISIBLE_DEVICES", "CUDA device visibility"),
        ("ROCR_VISIBLE_DEVICES", "ROCm device visibility (ROCR)"),
        ("HIP_VISIBLE_DEVICES", "HIP device visibility"),
        ("VK_ICD_FILENAMES", "Vulkan ICD configuration"),
        ("METAL_DEVICE_WRAPPER_TYPE", "Metal device override (macOS)"),
        ("ADESH_MLIR_OPT", "MLIR optimizer override"),
        ("ADESH_MLIR_TRANSLATE", "MLIR translator override"),
        ("ADESH_LLC", "LLVM LLC override"),
        ("ADESH_CLANG", "Clang override"),
        ("GPU_DEVICE_ORDINAL", "Generic GPU ordinal"),
        ("CUDA_HOME", "CUDA installation root"),
        ("ROCM_HOME", "ROCm installation root"),
        ("HIP_PATH", "HIP installation path"),
    ];

    for (var, label) in &vars {
        match env::var(var) {
            Ok(val) => {
                let display = if val.len() > 60 {
                    format!("{}…", &val[..60])
                } else {
                    val.clone()
                };
                r.push(
                    "Env",
                    format!("{}", label),
                    CheckStatus::Ok,
                    format!("{}={}", var, display),
                );
            }
            Err(_) => {
                r.push(
                    "Env",
                    format!("{}", label),
                    CheckStatus::Missing,
                    format!("{} not set", var),
                );
            }
        }
    }
}

// ── Toolchain tools ──────────────────────────────────────────────────────────

fn check_toolchain_tools(r: &mut CompatibilityReport) {
    let tools: &[(&str, &str, &str, bool)] = &[
        ("ADESH_MLIR_OPT", "mlir-opt", "MLIR optimizer", true),
        (
            "ADESH_MLIR_TRANSLATE",
            "mlir-translate",
            "MLIR→LLVM translator",
            true,
        ),
        ("ADESH_LLC", "llc", "LLVM code generator", true),
        ("ADESH_CLANG", "clang", "C/LLVM compiler driver", true),
        ("", "nvidia-smi", "NVIDIA SMI utility", false),
        ("", "rocm-smi", "ROCm SMI utility", false),
        ("", "vulkaninfo", "Vulkan info tool", false),
        ("", "clinfo", "OpenCL info tool", false),
        ("", "metal", "Metal compiler (macOS)", false),
    ];

    for (env_var, cmd, label, required) in tools {
        let found = if !env_var.is_empty() {
            env_override_or_path(env_var, cmd)
        } else {
            find_in_path(cmd)
        };

        match found {
            Some(p) => {
                let version = probe_version(p.to_str().unwrap_or(cmd));
                r.push(
                    "Toolchain",
                    format!("{}", label),
                    CheckStatus::Ok,
                    format!("{} [{}]", p.display(), version),
                );
            }
            None => {
                let status = if *required {
                    CheckStatus::Error
                } else {
                    CheckStatus::Missing
                };
                r.push(
                    "Toolchain",
                    format!("{}", label),
                    status,
                    format!("{} not found in PATH", cmd),
                );
            }
        }
    }
}

/// Run `<tool> --version` (or `-v`) and return the first line, or "unknown".
fn probe_version(tool: &str) -> String {
    // Try --version first
    if let Ok(out) = Command::new(tool).arg("--version").output() {
        if let Ok(text) = std::str::from_utf8(&out.stdout) {
            if let Some(line) = text.lines().next() {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
        // Some tools print version to stderr
        if let Ok(text) = std::str::from_utf8(&out.stderr) {
            if let Some(line) = text.lines().next() {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
    }
    "version unknown".to_string()
}

// ── NVIDIA / CUDA ────────────────────────────────────────────────────────────

fn check_nvidia(r: &mut CompatibilityReport) {
    // Try nvidia-smi
    match Command::new("nvidia-smi")
        .arg("--query-gpu=name,driver_version,memory.total,compute_cap")
        .arg("--format=csv,noheader")
        .output()
    {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            for (i, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                r.push(
                    "CUDA/NVIDIA",
                    format!("GPU #{}", i),
                    CheckStatus::Ok,
                    line.to_string(),
                );
            }
            if text.trim().is_empty() {
                r.push(
                    "CUDA/NVIDIA",
                    "nvidia-smi (no devices)",
                    CheckStatus::Missing,
                    "nvidia-smi present but reported no GPUs".to_string(),
                );
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            r.push(
                "CUDA/NVIDIA",
                "nvidia-smi",
                CheckStatus::Missing,
                format!("nvidia-smi failed: {}", err.trim()),
            );
        }
        Err(_) => {
            r.push(
                "CUDA/NVIDIA",
                "nvidia-smi",
                CheckStatus::Missing,
                "nvidia-smi not found".to_string(),
            );
        }
    }

    // Check CUDA libraries
    let cuda_lib = if cfg!(windows) {
        "nvcuda.dll"
    } else if cfg!(target_os = "macos") {
        "libcuda.dylib"
    } else {
        "libcuda.so.1"
    };
    let cuda_paths = cuda_lib_search_paths();
    let found_cuda_lib = cuda_paths.iter().any(|p| p.join(cuda_lib).exists());
    if found_cuda_lib {
        r.push(
            "CUDA/NVIDIA",
            "CUDA runtime library",
            CheckStatus::Ok,
            format!("{} found", cuda_lib),
        );
    } else {
        r.push(
            "CUDA/NVIDIA",
            "CUDA runtime library",
            CheckStatus::Missing,
            format!("{} not found", cuda_lib),
        );
    }
}

fn cuda_lib_search_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(cuda_home) = env::var("CUDA_HOME") {
        paths.push(PathBuf::from(&cuda_home).join("lib64"));
        paths.push(PathBuf::from(&cuda_home).join("lib"));
    }
    if cfg!(windows) {
        if let Ok(sys32) = env::var("SystemRoot") {
            paths.push(PathBuf::from(sys32).join("System32"));
        }
    } else {
        paths.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        paths.push(PathBuf::from("/usr/local/cuda/lib64"));
        paths.push(PathBuf::from("/usr/lib64"));
    }
    paths
}

// ── AMD / ROCm ───────────────────────────────────────────────────────────────

fn check_rocm(r: &mut CompatibilityReport) {
    match Command::new("rocm-smi").arg("--showproductname").output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let devices: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            if devices.is_empty() {
                r.push(
                    "ROCm/HIP",
                    "rocm-smi (no devices)",
                    CheckStatus::Missing,
                    "ROCm SMI present but no devices".to_string(),
                );
            } else {
                for (i, line) in devices.iter().enumerate() {
                    r.push(
                        "ROCm/HIP",
                        format!("GPU #{}", i),
                        CheckStatus::Ok,
                        line.trim().to_string(),
                    );
                }
            }
        }
        Ok(_) => {
            r.push(
                "ROCm/HIP",
                "rocm-smi",
                CheckStatus::Missing,
                "rocm-smi present but no ROCm devices detected".to_string(),
            );
        }
        Err(_) => {
            r.push(
                "ROCm/HIP",
                "rocm-smi",
                CheckStatus::Missing,
                "rocm-smi not found".to_string(),
            );
        }
    }

    // HIP runtime check
    let hip_lib = if cfg!(windows) {
        "amdhip64.dll"
    } else if cfg!(target_os = "macos") {
        "libhip.dylib"
    } else {
        "libhip_hcc.so"
    };
    let hip_home = env::var("HIP_PATH")
        .or_else(|_| env::var("ROCM_HOME"))
        .unwrap_or_default();
    let hip_found = if !hip_home.is_empty() {
        PathBuf::from(&hip_home).join("lib").join(hip_lib).exists()
    } else {
        PathBuf::from("/opt/rocm/lib").join(hip_lib).exists()
    };
    if hip_found {
        r.push(
            "ROCm/HIP",
            "HIP runtime library",
            CheckStatus::Ok,
            format!("{} found", hip_lib),
        );
    } else {
        r.push(
            "ROCm/HIP",
            "HIP runtime library",
            CheckStatus::Missing,
            format!("{} not found", hip_lib),
        );
    }
}

// ── Vulkan ───────────────────────────────────────────────────────────────────

fn check_vulkan(r: &mut CompatibilityReport) {
    match Command::new("vulkaninfo").arg("--summary").output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            // Extract GPU names from vulkaninfo --summary output
            let mut device_count = 0;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("GPU") || trimmed.contains("deviceName") {
                    r.push(
                        "Vulkan",
                        format!("Device #{}", device_count),
                        CheckStatus::Ok,
                        trimmed.to_string(),
                    );
                    device_count += 1;
                }
            }
            if device_count == 0 {
                // Full output first few lines as fallback
                let summary: String = text.lines().take(6).collect::<Vec<_>>().join("; ");
                r.push("Vulkan", "Vulkan device", CheckStatus::Ok, summary);
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).into_owned();
            r.push(
                "Vulkan",
                "vulkaninfo",
                CheckStatus::Missing,
                format!("vulkaninfo failed: {}", err.trim()),
            );
        }
        Err(_) => {
            // Check for ICD config file as fallback
            if env::var("VK_ICD_FILENAMES").is_ok() {
                r.push(
                    "Vulkan",
                    "Vulkan ICD",
                    CheckStatus::Ok,
                    "VK_ICD_FILENAMES set (vulkaninfo not installed)".to_string(),
                );
            } else {
                r.push(
                    "Vulkan",
                    "vulkaninfo",
                    CheckStatus::Missing,
                    "vulkaninfo not found and VK_ICD_FILENAMES not set".to_string(),
                );
            }
        }
    }

    // Vulkan loader library
    let vk_lib = if cfg!(windows) {
        "vulkan-1.dll"
    } else if cfg!(target_os = "macos") {
        "libvulkan.1.dylib"
    } else {
        "libvulkan.so.1"
    };
    let vk_loader_paths: &[&str] = if cfg!(windows) {
        &[r"C:\Windows\System32"]
    } else if cfg!(target_os = "macos") {
        &["/usr/local/lib", "/opt/homebrew/lib"]
    } else {
        &["/usr/lib/x86_64-linux-gnu", "/usr/lib64", "/usr/lib"]
    };
    let vk_found = vk_loader_paths
        .iter()
        .any(|p| PathBuf::from(p).join(vk_lib).exists());
    if vk_found {
        r.push(
            "Vulkan",
            "Vulkan loader",
            CheckStatus::Ok,
            format!("{} found", vk_lib),
        );
    } else {
        r.push(
            "Vulkan",
            "Vulkan loader",
            CheckStatus::Missing,
            format!("{} not found", vk_lib),
        );
    }
}

// ── OpenCL ───────────────────────────────────────────────────────────────────

fn check_opencl(r: &mut CompatibilityReport) {
    match Command::new("clinfo").arg("--list").output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let platforms: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            if platforms.is_empty() {
                r.push(
                    "OpenCL",
                    "clinfo (no platforms)",
                    CheckStatus::Missing,
                    "clinfo found but no OpenCL platforms".to_string(),
                );
            } else {
                r.push(
                    "OpenCL",
                    "OpenCL platforms",
                    CheckStatus::Ok,
                    platforms.join("; "),
                );
            }
        }
        Ok(_) => {
            r.push(
                "OpenCL",
                "clinfo",
                CheckStatus::Missing,
                "clinfo present but returned no platforms".to_string(),
            );
        }
        Err(_) => {
            r.push(
                "OpenCL",
                "clinfo",
                CheckStatus::Missing,
                "clinfo not found".to_string(),
            );
        }
    }
}

// ── Metal (macOS) ────────────────────────────────────────────────────────────

fn check_metal(r: &mut CompatibilityReport) {
    if !cfg!(target_os = "macos") {
        r.push(
            "Metal",
            "Metal API",
            CheckStatus::Missing,
            "Metal is macOS-only".to_string(),
        );
        return;
    }
    // On macOS, Metal is always available if the SDK is installed
    if PathBuf::from("/System/Library/Frameworks/Metal.framework").exists() {
        r.push(
            "Metal",
            "Metal.framework",
            CheckStatus::Ok,
            "Metal framework present".to_string(),
        );
    } else {
        r.push(
            "Metal",
            "Metal.framework",
            CheckStatus::Missing,
            "Metal.framework not found".to_string(),
        );
    }
    if env::var("METAL_DEVICE_WRAPPER_TYPE").is_ok() {
        r.push(
            "Metal",
            "Metal device override",
            CheckStatus::Ok,
            "METAL_DEVICE_WRAPPER_TYPE set".to_string(),
        );
    }
}

// ── Driver / permission checks ───────────────────────────────────────────────

#[allow(unused_variables)]
fn check_driver_permissions(r: &mut CompatibilityReport) {
    // Linux: /dev/nvidiaX devices
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let nvidia_ctl = PathBuf::from("/dev/nvidiactl");
        if nvidia_ctl.exists() {
            match fs::metadata(&nvidia_ctl) {
                Ok(meta) => {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = meta.permissions().mode();
                    let world_read = (mode & 0o004) != 0;
                    if world_read {
                        r.push(
                            "Permissions",
                            "/dev/nvidiactl",
                            CheckStatus::Ok,
                            format!("mode {:o}", mode),
                        );
                    } else {
                        r.push(
                            "Permissions",
                            "/dev/nvidiactl",
                            CheckStatus::Error,
                            format!("mode {:o} — may need 'sudo usermod -aG video $USER'", mode),
                        );
                    }
                }
                Err(e) => {
                    r.push(
                        "Permissions",
                        "/dev/nvidiactl",
                        CheckStatus::Unknown,
                        e.to_string(),
                    );
                }
            }
        } else {
            r.push(
                "Permissions",
                "/dev/nvidiactl",
                CheckStatus::Missing,
                "not present (no NVIDIA driver)".to_string(),
            );
        }

        let kfd = PathBuf::from("/dev/kfd");
        if kfd.exists() {
            r.push(
                "Permissions",
                "/dev/kfd (ROCm)",
                CheckStatus::Ok,
                "ROCm KFD device present".to_string(),
            );
        } else {
            r.push(
                "Permissions",
                "/dev/kfd (ROCm)",
                CheckStatus::Missing,
                "not present (no ROCm driver)".to_string(),
            );
        }
    }

    #[cfg(windows)]
    {
        // On Windows, look for GPU-related registry keys
        r.push(
            "Permissions",
            "Windows GPU access",
            CheckStatus::Ok,
            "DirectX/WDDM handles GPU access (no special permissions needed)".to_string(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        r.push(
            "Permissions",
            "macOS GPU access",
            CheckStatus::Ok,
            "Metal API grants GPU access via entitlements".to_string(),
        );
    }
}

/// Get GPU device info
pub fn get_gpu_info() -> Option<GpuInfo> {
    // Try nvidia-smi for NVIDIA GPUs
    if let Ok(out) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,memory.total,compute_cap",
            "--format=csv,noheader",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            if let Some(line) = text.lines().next() {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 4 {
                    let cap_parts: Vec<u32> =
                        parts[3].split('.').filter_map(|s| s.parse().ok()).collect();
                    return Some(GpuInfo {
                        name: parts[0].to_string(),
                        driver_version: parts[1].to_string(),
                        total_memory_mb: parts[2].replace("MiB", "").trim().parse().unwrap_or(0),
                        compute_capability: (
                            cap_parts.get(0).copied().unwrap_or(0),
                            cap_parts.get(1).copied().unwrap_or(0),
                        ),
                        backend: GpuTarget::Cuda,
                        multiprocessors: 0,
                    });
                }
            }
        }
    }
    None
}

/// Detect GPU toolchain for a target (best-effort)
pub fn detect_toolchain(target: GpuTarget) -> GpuToolchain {
    let mlir_opt = env_override_or_path("ADESH_MLIR_OPT", "mlir-opt");
    let mlir_translate = env_override_or_path("ADESH_MLIR_TRANSLATE", "mlir-translate");
    let llc = env_override_or_path("ADESH_LLC", "llc");
    let clang = env_override_or_path("ADESH_CLANG", "clang");

    let _ = target; // Reserved for target-specific toolchain checks

    GpuToolchain {
        mlir_opt,
        mlir_translate,
        llc,
        clang,
    }
}

/// Resolve an automatic GPU target based on environment hints
pub fn resolve_target(requested: GpuTarget) -> GpuTarget {
    if requested != GpuTarget::Auto {
        return requested;
    }

    if env::var("CUDA_VISIBLE_DEVICES").is_ok() {
        return GpuTarget::Cuda;
    }
    if env::var("ROCR_VISIBLE_DEVICES").is_ok() || env::var("HIP_VISIBLE_DEVICES").is_ok() {
        return GpuTarget::Rocm;
    }
    if env::var("VK_ICD_FILENAMES").is_ok() {
        return GpuTarget::Vulkan;
    }
    if env::var("METAL_DEVICE_WRAPPER_TYPE").is_ok() {
        return GpuTarget::Metal;
    }

    GpuTarget::Auto
}

fn env_override_or_path(env_var: &str, default_cmd: &str) -> Option<PathBuf> {
    if let Ok(path) = env::var(env_var) {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let full_name = format!("{}{}", default_cmd, exe_suffix);

    // 1. Check bundled toolchain root from resolver
    if let Some(bundled) = crate::toolchain::resolver::bundled_root() {
        let p = bundled.join("bin").join(&full_name);
        if p.is_file() {
            return Some(p);
        }
        let p_root = bundled.join(&full_name);
        if p_root.is_file() {
            return Some(p_root);
        }
    }

    // 2. Check explicit ADESH_TOOLCHAIN or ADESH_HOME
    for var in ["ADESH_TOOLCHAIN", "ADESHLANG_TOOLCHAIN"] {
        if let Ok(tc) = env::var(var) {
            let p1 = PathBuf::from(&tc).join("bin").join(&full_name);
            if p1.is_file() {
                return Some(p1);
            }
            let p2 = PathBuf::from(&tc).join(&full_name);
            if p2.is_file() {
                return Some(p2);
            }
        }
    }
    for var in ["ADESH_HOME", "ADESHLANG_HOME"] {
        if let Ok(home) = env::var(var) {
            let p = PathBuf::from(home)
                .join("toolchain")
                .join("llvm")
                .join("bin")
                .join(&full_name);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 3. Check executable-relative toolchain
    if let Ok(exe) = env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let p1 = bin_dir
                .join("toolchain")
                .join("llvm")
                .join("bin")
                .join(&full_name);
            if p1.is_file() {
                return Some(p1);
            }
            if let Some(parent) = bin_dir.parent() {
                let p2 = parent
                    .join("toolchain")
                    .join("llvm")
                    .join("bin")
                    .join(&full_name);
                if p2.is_file() {
                    return Some(p2);
                }
            }
        }
    }

    // 4. Check well-known system LLVM roots
    #[cfg(windows)]
    {
        for root in [
            "C:\\Program Files\\AdeshLang\\toolchain\\llvm\\bin",
            "C:\\Program Files\\LLVM\\bin",
            "C:\\LLVM\\bin",
            "C:\\Program Files (x86)\\LLVM\\bin",
        ] {
            let p = Path::new(root).join(&full_name);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    find_in_path(default_cmd)
}

fn find_in_path(cmd: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(cmd);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate_exe = dir.join(format!("{}.exe", cmd));
            if is_executable(&candidate_exe) {
                return Some(candidate_exe);
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn lower_vir_type(ty: &VirType) -> &'static str {
    match ty {
        VirType::I8 => "i8",
        VirType::I16 => "i16",
        VirType::I32 => "i32",
        VirType::I64 => "i64",
        VirType::U8 => "i8",
        VirType::U16 => "i16",
        VirType::U32 => "i32",
        VirType::U64 => "i64",
        VirType::F32 => "f32",
        VirType::F64 => "f64",
        VirType::Bool => "i1",
        VirType::Void => "()",
        _ => "!llvm.ptr",
    }
}

/// GPU Device Information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Device name
    pub name: String,
    /// Driver version
    pub driver_version: String,
    /// Total memory in MiB
    pub total_memory_mb: u32,
    /// Compute capability (major, minor) — CUDA-specific, (0,0) for other backends
    pub compute_capability: (u32, u32),
    /// Detected GPU backend
    pub backend: GpuTarget,
    /// Number of multiprocessors / compute units
    pub multiprocessors: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_space_mapping() {
        assert_eq!(map_memory_space(true), MemorySpace::Global);
        assert_eq!(map_memory_space(false), MemorySpace::Local);
    }

    #[test]
    fn test_gpu_kernel_config_default() {
        let config = GpuKernelConfig::default();
        assert_eq!(config.grid_dims, (1, 1, 1));
        assert_eq!(config.block_dims, (256, 1, 1));
    }

    #[test]
    fn test_is_gpu_safe() {
        assert!(is_gpu_safe("add"));
        assert!(is_gpu_safe("mul"));
    }
}
