//! VIR to MLIR Lowering
//!
//! Converts VIR (Value IR) to MLIR representation.

use super::MlirConfig;
use super::arc_runtime;
use super::gpu;
use crate::backends::common::BackendResult;
use crate::ir::vir::{VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator, VirType};
use std::collections::HashMap;

/// Lower VIR module to MLIR
pub fn lower_module(module: &VirModule, config: &MlirConfig) -> BackendResult<String> {
    let mut mlir_output = String::new();

    // MLIR module header with gpu.container_module attribute if GPU is enabled
    if config.enable_gpu {
        mlir_output.push_str("module attributes {gpu.container_module} {\n");
    } else {
        mlir_output.push_str("module {\n");
    }

    // Emit string constant globals
    for (string_id, string_val) in &module.strings {
        // Escape string for MLIR (handle backslashes, quotes, newlines)
        let escaped = string_val
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        let len = string_val.len(); // Length without null terminator
        mlir_output.push_str(&format!(
            "  llvm.mlir.global internal constant @str_{}(\"{}\") : !llvm.array<{} x i8>\n",
            string_id, escaped, len
        ));
    }
    if !module.strings.is_empty() {
        mlir_output.push('\n'); // Blank line after string constants
    }

    // Emit runtime function declarations (ARC + libc)
    mlir_output.push_str(&arc_runtime::generate_runtime_decls());

    // Lower each function
    for function in &module.functions {
        let func_mlir = lower_function(function, config)?;
        mlir_output.push_str(&func_mlir);
        mlir_output.push('\n');
    }

    if config.enable_gpu {
        for function in &module.functions {
            let kernel_mlir = gpu::generate_gpu_kernel(function, &config.gpu_kernel_config)?;
            mlir_output.push_str(&kernel_mlir);
            mlir_output.push('\n');

            let launch_mlir = gpu::generate_gpu_launch_wrapper(
                function,
                &config.gpu_kernel_config,
                config.gpu_target,
            )?;
            mlir_output.push_str(&launch_mlir);
            mlir_output.push('\n');
        }
    }

    mlir_output.push_str("}\n");

    Ok(mlir_output)
}

/// Lower VIR function to MLIR
pub fn lower_function(function: &VirFunction, config: &MlirConfig) -> BackendResult<String> {
    let mut mlir_output = String::new();

    // Function signature
    mlir_output.push_str(&format!("  func.func @{}(", function.name));

    // Parameters
    for (i, param) in function.params.iter().enumerate() {
        if i > 0 {
            mlir_output.push_str(", ");
        }
        mlir_output.push_str(&format!("%arg{}: {}", i, lower_type(&param.ty)));
    }

    mlir_output.push_str(") -> ");

    // Return type
    mlir_output.push_str(&lower_type(&function.return_type));

    mlir_output.push_str(" {\n");

    // Emit local allocations at function entry (stack-allocated locals)
    for (local_id, local) in function.locals.iter().enumerate() {
        // Skip void-typed locals (they don't need storage)
        if matches!(local.ty, VirType::Void) {
            continue;
        }
        let ty = lower_type(&local.ty);
        mlir_output.push_str(&format!(
            "    %local_{} = memref.alloca() : memref<1x{}>\n",
            local_id, ty
        ));
    }
    if !function.locals.is_empty() {
        mlir_output.push('\n'); // Blank line after locals
    }

    // Lower blocks, passing return type for terminator validation
    for block in &function.blocks {
        let block_mlir = lower_block(block, config, &function.return_type, &function.blocks)?;
        mlir_output.push_str(&block_mlir);
    }

    mlir_output.push_str("  }\n");

    Ok(mlir_output)
}

/// Lower VIR block to MLIR
///
/// Implements SSA value uniquification: VIR may re-use the same `dest` ID for
/// different instructions (e.g., constant value 2 collides with computed value ID 2).
/// We track VIR dest → latest MLIR SSA name and generate fresh names on collision.
fn lower_block(
    block: &VirBlock,
    _config: &MlirConfig,
    return_type: &VirType,
    all_blocks: &[VirBlock],
) -> BackendResult<String> {
    let mut mlir_output = String::new();

    // SSA name map: VIR value ID → current MLIR SSA name (without `%` prefix)
    let mut name_map: HashMap<u32, String> = HashMap::new();
    // Fresh name counter for collision resolution
    let mut fresh: u32 = 0;

    // Only emit block label for non-entry blocks (bb0 is implicit in MLIR)
    if block.id != 0 {
        // Emit block with parameters for PHI nodes
        if !block.phis.is_empty() {
            let params: Vec<String> = block
                .phis
                .iter()
                .map(|phi| {
                    let param_name = phi.dest.to_string();
                    name_map.insert(phi.dest, param_name.clone());
                    format!("%{}: {}", param_name, lower_type(&phi.ty))
                })
                .collect();
            mlir_output.push_str(&format!("  ^bb{}({}):\n", block.id, params.join(", ")));
        } else {
            mlir_output.push_str(&format!("  ^bb{}:\n", block.id));
        }
    } else if !block.phis.is_empty() {
        // Entry block with PHI nodes (rare, but handle it)
        let params: Vec<String> = block
            .phis
            .iter()
            .map(|phi| {
                let param_name = phi.dest.to_string();
                name_map.insert(phi.dest, param_name.clone());
                format!("%{}: {}", param_name, lower_type(&phi.ty))
            })
            .collect();
        mlir_output.push_str(&format!(
            "  // Entry block PHI params: {}\n",
            params.join(", ")
        ));
    }

    // Lower instructions with SSA uniquification
    for inst in &block.instructions {
        match lower_instruction_ssa(inst, &mut name_map, &mut fresh)? {
            Some(mlir) => {
                for line in mlir.lines() {
                    mlir_output.push_str(&format!("    {}\n", line));
                }
            }
            None => {} // Copy / no-op instructions produce no MLIR
        }
    }

    // Lower terminator using the resolved name map
    let term_mlir = lower_terminator_ssa(
        &block.terminator,
        return_type,
        &name_map,
        block.id,
        all_blocks,
    )?;
    mlir_output.push_str(&format!("    {}\n", term_mlir));

    Ok(mlir_output)
}

// ─────────────────────────────────────────────────────────────────────────────
// SSA helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve a VIR value ID to its current MLIR SSA name.
/// Falls back to the VIR ID as a string if not yet defined.
fn ssa_ref(id: u32, name_map: &HashMap<u32, String>) -> String {
    name_map.get(&id).cloned().unwrap_or_else(|| id.to_string())
}

/// Define a new MLIR SSA name for a VIR dest ID.
/// If the ID was already defined (VIR SSA violation), a fresh unique name is generated
/// and the map is updated to point to the new name.
fn ssa_def(dest: u32, name_map: &mut HashMap<u32, String>, fresh: &mut u32) -> String {
    if name_map.contains_key(&dest) {
        // Collision: generate a fresh unique name so MLIR remains valid
        let n = format!("_u{}", *fresh);
        *fresh += 1;
        name_map.insert(dest, n.clone());
        n
    } else {
        let n = dest.to_string();
        name_map.insert(dest, n.clone());
        n
    }
}

/// Lower a single VIR instruction using the live SSA name map.
/// Returns `None` for no-op instructions (e.g. Copy, ARC ops that are not yet
/// translated to real MLIR operations).
fn lower_instruction_ssa(
    inst: &VirInstruction,
    name_map: &mut HashMap<u32, String>,
    fresh: &mut u32,
) -> BackendResult<Option<String>> {
    use VirInstruction::*;

    match inst {
        // ── Copy: VIR's non-SSA assignment. Model as an alias in the name map.
        // This is how VIR handles `let local = some_value; use local`.
        Copy { dest, src } => {
            let src_name = ssa_ref(*src, name_map);
            name_map.insert(*dest, src_name);
            Ok(None)
        }

        // ── Constants ──────────────────────────────────────────────────────────
        ConstInt { dest, value, ty } => {
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : {}",
                d,
                value,
                lower_type(ty)
            )))
        }
        ConstFloat { dest, value, ty } => {
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : {}",
                d,
                value,
                lower_type(ty)
            )))
        }
        ConstBool { dest, value } => {
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.constant {} : i1",
                d,
                if *value { "true" } else { "false" }
            )))
        }
        ConstString { dest, string_id } => {
            let d = ssa_def(*dest, name_map, fresh);
            // Reference to module-level string global (emitted in lower_module)
            Ok(Some(format!(
                "%{} = llvm.mlir.addressof @str_{} : !llvm.ptr",
                d, string_id
            )))
        }
        ConstNull { dest } => {
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!("%{} = llvm.mlir.null : !llvm.ptr", d)))
        }

        // ── Integer arithmetic ─────────────────────────────────────────────────
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
            let l = ssa_ref(*lhs, name_map);
            let r = ssa_ref(*rhs, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = {} %{}, %{} : {}",
                d,
                mlir_op,
                l,
                r,
                lower_type(ty)
            )))
        }

        // ── Float arithmetic ──────────────────────────────────────────────────
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
            let l = ssa_ref(*lhs, name_map);
            let r = ssa_ref(*rhs, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = {} %{}, %{} : {}",
                d,
                mlir_op,
                l,
                r,
                lower_type(ty)
            )))
        }

        IntUnOp {
            dest,
            op,
            operand,
            ty,
        } => {
            use crate::ir::vir::IntUnOp as IntOp;
            let ty_str = lower_type(ty);
            let opr = ssa_ref(*operand, name_map);
            match op {
                IntOp::Neg => {
                    let c0 = format!("_u{}", *fresh);
                    *fresh += 1;
                    let d = ssa_def(*dest, name_map, fresh);
                    Ok(Some(format!(
                        "%{} = arith.constant 0 : {}\n%{} = arith.subi %{}, %{} : {}",
                        c0, ty_str, d, c0, opr, ty_str
                    )))
                }
                IntOp::Not => {
                    let cm = format!("_u{}", *fresh);
                    *fresh += 1;
                    let d = ssa_def(*dest, name_map, fresh);
                    Ok(Some(format!(
                        "%{} = arith.constant -1 : {}\n%{} = arith.xori %{}, %{} : {}",
                        cm, ty_str, d, opr, cm, ty_str
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
            use crate::ir::vir::FloatUnOp as FloatOp;
            let ty_str = lower_type(ty);
            let mlir_op = match op {
                FloatOp::Neg => "arith.negf",
                FloatOp::Abs => "arith.absf",
                FloatOp::Sqrt => "math.sqrt",
            };
            let opr = ssa_ref(*operand, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!("%{} = {} %{} : {}", d, mlir_op, opr, ty_str)))
        }

        IntCmp { dest, op, lhs, rhs } => {
            let predicate = match op {
                crate::ir::vir::CmpOp::Eq => "eq",
                crate::ir::vir::CmpOp::Ne => "ne",
                crate::ir::vir::CmpOp::Lt => "slt",
                crate::ir::vir::CmpOp::Le => "sle",
                crate::ir::vir::CmpOp::Gt => "sgt",
                crate::ir::vir::CmpOp::Ge => "sge",
            };
            let l = ssa_ref(*lhs, name_map);
            let r = ssa_ref(*rhs, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.cmpi {}, %{}, %{} : i64",
                d, predicate, l, r
            )))
        }

        FloatCmp { dest, op, lhs, rhs } => {
            let predicate = match op {
                crate::ir::vir::CmpOp::Eq => "oeq",
                crate::ir::vir::CmpOp::Ne => "one",
                crate::ir::vir::CmpOp::Lt => "olt",
                crate::ir::vir::CmpOp::Le => "ole",
                crate::ir::vir::CmpOp::Gt => "ogt",
                crate::ir::vir::CmpOp::Ge => "oge",
            };
            let l = ssa_ref(*lhs, name_map);
            let r = ssa_ref(*rhs, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.cmpf {}, %{}, %{} : f64",
                d, predicate, l, r
            )))
        }

        // ── Memory ────────────────────────────────────────────────────────────
        Alloc { dest, size, ty } => {
            let elem_ty = lower_type(ty);
            let sz = ssa_ref(*size, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = memref.alloc(%{}) : memref<?x{}>",
                d, sz, elem_ty
            )))
        }
        Load { dest, ptr, ty } => {
            let elem_ty = lower_type(ty);
            let p = ssa_ref(*ptr, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = memref.load %{}[] : memref<?x{}>",
                d, p, elem_ty
            )))
        }
        Store { ptr, value } => {
            let p = ssa_ref(*ptr, name_map);
            let v = ssa_ref(*value, name_map);
            Ok(Some(format!(
                "memref.store %{}, %{}[] : memref<?xi64>",
                v, p
            )))
        }
        LoadLocal { dest, local } => {
            let d = ssa_def(*dest, name_map, fresh);
            // Local allocations are %local_N (emitted in lower_function)
            Ok(Some(format!(
                "%{} = memref.load %local_{}[] : memref<1xi64>",
                d, local
            )))
        }
        StoreLocal { local, value } => {
            let v = ssa_ref(*value, name_map);
            Ok(Some(format!(
                "memref.store %{}, %local_{}[] : memref<1xi64>",
                v, local
            )))
        }
        Free { ptr } => {
            let p = ssa_ref(*ptr, name_map);
            Ok(Some(format!("llvm.call @free(%{}) : (!llvm.ptr) -> ()", p)))
        }

        Cast {
            dest,
            value,
            from_ty,
            to_ty,
        } => {
            let v = ssa_ref(*value, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(lower_cast_named(&d, &v, from_ty, to_ty)))
        }
        Bitcast { dest, value, to_ty } => {
            let v = ssa_ref(*value, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = arith.bitcast %{} : !llvm.ptr to {}",
                d,
                v,
                lower_type(to_ty)
            )))
        }

        // ── ARC ops ───────────────────────────────────────────────────────────
        ArcIncrement { ptr } => {
            let p = ssa_ref(*ptr, name_map);
            Ok(Some(format!(
                "llvm.call @arc_retain(%{}) : (!llvm.ptr) -> ()",
                p
            )))
        }
        ArcDecrement { ptr } => {
            let p = ssa_ref(*ptr, name_map);
            Ok(Some(format!(
                "llvm.call @arc_release(%{}) : (!llvm.ptr) -> ()",
                p
            )))
        }
        ArcClone { dest, src } => {
            let s = ssa_ref(*src, name_map);
            let d = ssa_def(*dest, name_map, fresh);
            Ok(Some(format!(
                "%{} = llvm.call @arc_clone(%{}) : (!llvm.ptr) -> !llvm.ptr",
                d, s
            )))
        }
        ArcDrop { ptr } => {
            let p = ssa_ref(*ptr, name_map);
            // ArcDrop calls arc_release (same as ArcDecrement)
            Ok(Some(format!(
                "llvm.call @arc_release(%{}) : (!llvm.ptr) -> ()",
                p
            )))
        }
        Drop { value } => {
            let v = ssa_ref(*value, name_map);
            // Drop may need type-specific logic; for now, call arc_release for pointer types
            Ok(Some(format!(
                "llvm.call @arc_release(%{}) : (!llvm.ptr) -> ()",
                v
            )))
        }

        // ── Misc (no-ops / aliases) ───────────────────────────────────────────
        Move { dest, src } => {
            // Move in SSA form is just an alias (same as Copy)
            let src_name = ssa_ref(*src, name_map);
            name_map.insert(*dest, src_name);
            Ok(None)
        }
        Nop => Ok(None),

        // ── Aggregate operations ──────────────────────────────────────────────
        BuildStruct {
            dest,
            ty: _,
            fields,
            ..
        } => {
            let d = ssa_def(*dest, name_map, fresh);
            // Build struct using llvm.mlir.undef + llvm.insertvalue sequence
            let mut code = format!("%{}_init = llvm.mlir.undef : !llvm.struct<()>\n", d);
            let mut current = format!("{}_init", d);
            for (idx, field_val) in fields.iter().enumerate() {
                let fv = ssa_ref(*field_val, name_map);
                let next = if idx == fields.len() - 1 {
                    d.clone()
                } else {
                    format!("{}_{}", d, idx)
                };
                code.push_str(&format!(
                    "    %{} = llvm.insertvalue %{}, %{}_init[{}] : !llvm.struct<()>\n",
                    next, fv, current, idx
                ));
                current = next;
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ExtractField {
            dest,
            struct_val,
            field,
        } => {
            let d = ssa_def(*dest, name_map, fresh);
            let sv = ssa_ref(*struct_val, name_map);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[{}] : !llvm.struct<()>",
                d, sv, field
            )))
        }
        InsertField {
            dest,
            struct_val,
            field,
            value,
        } => {
            let d = ssa_def(*dest, name_map, fresh);
            let sv = ssa_ref(*struct_val, name_map);
            let v = ssa_ref(*value, name_map);
            Ok(Some(format!(
                "%{} = llvm.insertvalue %{}, %{}[{}] : !llvm.struct<()>",
                d, v, sv, field
            )))
        }
        BuildArray {
            dest,
            elem_ty,
            elements,
        } => {
            let d = ssa_def(*dest, name_map, fresh);
            let ty = lower_type(elem_ty);
            let len = elements.len();
            // Allocate array on stack, then store each element
            let mut code = format!("%{} = memref.alloca() : memref<{}x{}>\n", d, len, ty);
            for (idx, elem) in elements.iter().enumerate() {
                let e = ssa_ref(*elem, name_map);
                let idx_val = format!("{}_idx_{}", d, idx);
                code.push_str(&format!(
                    "    %{} = arith.constant {} : index\n",
                    idx_val, idx
                ));
                code.push_str(&format!(
                    "    memref.store %{}, %{}[%{}] : memref<{}x{}>\n",
                    e, d, idx_val, len, ty
                ));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ArrayIndex { dest, array, index } => {
            let d = ssa_def(*dest, name_map, fresh);
            let arr = ssa_ref(*array, name_map);
            let idx = ssa_ref(*index, name_map);
            // Cast index to MLIR index type, then load
            let idx_casted = format!("{}_idx", d);
            Ok(Some(format!(
                "%{} = arith.index_cast %{} : i64 to index\n    %{} = memref.load %{}[%{}] : memref<?xi64>",
                idx_casted, idx, d, arr, idx_casted
            )))
        }
        BuildTuple { dest, elements } => {
            // Tuples are like structs with positional fields
            let d = ssa_def(*dest, name_map, fresh);
            let mut code = format!("%{}_init = llvm.mlir.undef : !llvm.struct<()>\n", d);
            let mut current = format!("{}_init", d);
            for (idx, elem) in elements.iter().enumerate() {
                let e = ssa_ref(*elem, name_map);
                let next = if idx == elements.len() - 1 {
                    d.clone()
                } else {
                    format!("{}_{}", d, idx)
                };
                code.push_str(&format!(
                    "    %{} = llvm.insertvalue %{}, %{}_init[{}] : !llvm.struct<()>\n",
                    next, e, current, idx
                ));
                current = next;
            }
            Ok(Some(code.trim_end().to_string()))
        }
        ExtractTuple { dest, tuple, index } => {
            let d = ssa_def(*dest, name_map, fresh);
            let t = ssa_ref(*tuple, name_map);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[{}] : !llvm.struct<()>",
                d, t, index
            )))
        }
        BuildObject {
            dest,
            keys: _,
            values: _,
        } => {
            // Objects are implemented as runtime values for now
            // In a more sophisticated implementation, they could use LLVM struct types
            let d = ssa_def(*dest, name_map, fresh);
            // For now, just create a placeholder pointer (like an empty struct)
            // The actual object operations are handled at runtime
            Ok(Some(format!("%{} = llvm.mlir.undef : !llvm.ptr", d)))
        }
        BuildEnum {
            dest,
            ty: _,
            variant,
            payload,
        } => {
            // Enum: struct of (i32 discriminant, payload union)
            let d = ssa_def(*dest, name_map, fresh);
            let mut code = format!("%{}_disc = arith.constant {} : i32\n", d, variant);
            code.push_str(&format!(
                "    %{}_enum = llvm.mlir.undef : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n",
                d
            ));
            code.push_str(&format!("    %{}_0 = llvm.insertvalue %{}_disc, %{}_enum[0] : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n",
                d, d, d));
            // For payload, bitcast first element to the payload area (simplified)
            if !payload.is_empty() {
                let p = ssa_ref(payload[0], name_map);
                code.push_str(&format!("    %{} = llvm.insertvalue %{}, %{}_0[1] : !llvm.struct<(i32, !llvm.array<16 x i8>)>\n",
                    d, p, d));
            } else {
                code.push_str(&format!("    %{} = %{}_0\n", d, d));
            }
            Ok(Some(code.trim_end().to_string()))
        }
        GetDiscriminant { dest, enum_val } => {
            let d = ssa_def(*dest, name_map, fresh);
            let ev = ssa_ref(*enum_val, name_map);
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[0] : !llvm.struct<(i32, !llvm.array<16 x i8>)>",
                d, ev
            )))
        }
        ExtractPayload {
            dest,
            enum_val,
            variant: _,
        } => {
            let d = ssa_def(*dest, name_map, fresh);
            let ev = ssa_ref(*enum_val, name_map);
            // Extract payload (simplified - just get the array)
            Ok(Some(format!(
                "%{} = llvm.extractvalue %{}[1] : !llvm.struct<(i32, !llvm.array<16 x i8>)>",
                d, ev
            )))
        }

        // ── Function calls ────────────────────────────────────────────────────
        Call { dest, func, args } => {
            let arg_refs: Vec<String> = args.iter().map(|a| ssa_ref(*a, name_map)).collect();
            let func_ref = ssa_ref(*func, name_map);

            // Detect direct calls: if func is a constant reference to a known function symbol,
            // emit func.call @symbol instead of indirect call
            // For now, we check if the func name looks like a direct symbol (starts with @ or is a known name)
            // In a full implementation, this would check the VIR metadata for function symbols

            let is_direct_call = false; // TODO: Implement symbol table lookup from VIR metadata

            let code = if is_direct_call {
                // Direct call to known function symbol
                if let Some(d) = dest {
                    let dn = ssa_def(*d, name_map, fresh);
                    let args_str = arg_refs
                        .iter()
                        .map(|a| format!("%{}", a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if args_str.is_empty() {
                        format!("%{} = func.call @{}() : () -> i64", dn, func_ref)
                    } else {
                        format!(
                            "%{} = func.call @{}({}) : ({}) -> i64",
                            dn,
                            func_ref,
                            args_str,
                            args.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
                        )
                    }
                } else {
                    let args_str = arg_refs
                        .iter()
                        .map(|a| format!("%{}", a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if args_str.is_empty() {
                        format!("func.call @{}() : () -> ()", func_ref)
                    } else {
                        format!(
                            "func.call @{}({}) : ({}) -> ()",
                            func_ref,
                            args_str,
                            args.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
                        )
                    }
                }
            } else {
                // Indirect call through function pointer
                if let Some(d) = dest {
                    let dn = ssa_def(*d, name_map, fresh);
                    let args_str = arg_refs
                        .iter()
                        .map(|a| format!("%{}", a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if args_str.is_empty() {
                        format!("%{} = llvm.call %{}() : !llvm.ptr, () -> i64", dn, func_ref)
                    } else {
                        format!(
                            "%{} = llvm.call %{}({}) : !llvm.ptr, ({}) -> i64",
                            dn,
                            func_ref,
                            args_str,
                            args.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
                        )
                    }
                } else {
                    let args_str = arg_refs
                        .iter()
                        .map(|a| format!("%{}", a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if args_str.is_empty() {
                        format!("llvm.call %{}() : !llvm.ptr, () -> ()", func_ref)
                    } else {
                        format!(
                            "llvm.call %{}({}) : !llvm.ptr, ({}) -> ()",
                            func_ref,
                            args_str,
                            args.iter().map(|_| "i64").collect::<Vec<_>>().join(", ")
                        )
                    }
                }
            };
            Ok(Some(code))
        }

        // ── Intrinsics ────────────────────────────────────────────────────────
        Intrinsic {
            dest,
            intrinsic,
            args,
        } => {
            use crate::ir::vir::Intrinsic as Intr;
            let arg_refs: Vec<String> = args.iter().map(|a| ssa_ref(*a, name_map)).collect();

            let code = match intrinsic {
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
                Intr::Sin => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                Intr::SizeOf | Intr::AlignOf => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
                        // SizeOf/AlignOf should be resolved at VIR lowering time to constants
                        // For now, emit a constant 8 (typical pointer/i64 size)
                        format!("%{} = arith.constant 8 : i64", dn)
                    } else {
                        "// ERROR: SizeOf/AlignOf requires dest".to_string()
                    }
                }
                Intr::AtomicCAS => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
                        if arg_refs.len() >= 3 {
                            format!(
                                "%{} = llvm.cmpxchg %{}, %{}, %{} : !llvm.ptr",
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
                        let dn = ssa_def(*d, name_map, fresh);
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
                Intr::AtomicLoad => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
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
                Intr::CountOnes => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            format!("%{} = llvm.intr.ctpop(%{}) : (i64) -> i64", dn, arg_refs[0])
                        } else {
                            "// ERROR: CountOnes requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: CountOnes requires dest".to_string()
                    }
                }
                Intr::CountZeros => {
                    // CountZeros = 64 - CountOnes (for 64-bit integers)
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            let tmp = format!("_ctpop_{}", *fresh);
                            *fresh += 1;
                            let c64 = format!("_c64_{}", *fresh);
                            *fresh += 1;
                            format!(
                                "%{} = llvm.intr.ctpop(%{}) : (i64) -> i64\n    %{} = arith.constant 64 : i64\n    %{} = arith.subi %{}, %{} : i64",
                                tmp, arg_refs[0], c64, dn, c64, tmp
                            )
                        } else {
                            "// ERROR: CountZeros requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: CountZeros requires dest".to_string()
                    }
                }
                Intr::LeadingZeros => {
                    if let Some(d) = dest {
                        let dn = ssa_def(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            // ctlz takes two arguments: value and is_zero_undef (false = 0)
                            let zero_flag = format!("_zero_{}", *fresh);
                            *fresh += 1;
                            format!(
                                "%{} = arith.constant 0 : i1\n    %{} = llvm.intr.ctlz(%{}, %{}) : (i64, i1) -> i64",
                                zero_flag, dn, arg_refs[0], zero_flag
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
                        let dn = ssa_def(*d, name_map, fresh);
                        if !arg_refs.is_empty() {
                            // cttz takes two arguments: value and is_zero_undef (false = 0)
                            let zero_flag = format!("_zero_{}", *fresh);
                            *fresh += 1;
                            format!(
                                "%{} = arith.constant 0 : i1\n    %{} = llvm.intr.cttz(%{}, %{}) : (i64, i1) -> i64",
                                zero_flag, dn, arg_refs[0], zero_flag
                            )
                        } else {
                            "// ERROR: TrailingZeros requires 1 arg".to_string()
                        }
                    } else {
                        "// ERROR: TrailingZeros requires dest".to_string()
                    }
                }
            };
            Ok(Some(code))
        }
    }
}

/// Lower VIR terminator to MLIR using the live SSA name map.
/// Get PHI arguments for a branch to a target block
/// Returns a string like "(%val1, %val2)" or "" if no PHI nodes
fn get_phi_args(
    target_block_id: u32,
    source_block_id: u32,
    all_blocks: &[VirBlock],
    name_map: &HashMap<u32, String>,
) -> String {
    // Find the target block
    let target_block = all_blocks.iter().find(|b| b.id == target_block_id);
    if target_block.is_none() || target_block.unwrap().phis.is_empty() {
        return String::new();
    }

    let target_block = target_block.unwrap();
    let mut args = Vec::new();

    // For each PHI node in target block, find the incoming value from source block
    for phi in &target_block.phis {
        let incoming_value = phi
            .incoming
            .iter()
            .find(|(block_id, _)| *block_id == source_block_id)
            .map(|(_, value)| *value);

        if let Some(value) = incoming_value {
            let val_name = ssa_ref(value, name_map);
            args.push(format!("%{}", val_name));
        } else {
            // No incoming value from this source (shouldn't happen in well-formed VIR)
            // Use a dummy value to prevent MLIR errors
            args.push("%0".to_string());
        }
    }

    if args.is_empty() {
        String::new()
    } else {
        format!("({})", args.join(", "))
    }
}

fn lower_terminator_ssa(
    term: &VirTerminator,
    return_type: &VirType,
    name_map: &HashMap<u32, String>,
    current_block_id: u32,
    all_blocks: &[VirBlock],
) -> BackendResult<String> {
    use VirTerminator::*;

    match term {
        Return { value: Some(val) } => {
            if matches!(return_type, VirType::Void) {
                Ok("return".to_string())
            } else {
                let v = ssa_ref(*val, name_map);
                Ok(format!("return %{} : {}", v, lower_type(return_type)))
            }
        }
        Return { value: None } => Ok("return".to_string()),
        Jump { target } => {
            let args = get_phi_args(*target, current_block_id, all_blocks, name_map);
            Ok(format!("cf.br ^bb{}{}", target, args))
        }
        Branch {
            cond,
            true_target,
            false_target,
        } => {
            let c = ssa_ref(*cond, name_map);
            let true_args = get_phi_args(*true_target, current_block_id, all_blocks, name_map);
            let false_args = get_phi_args(*false_target, current_block_id, all_blocks, name_map);
            Ok(format!(
                "cf.cond_br %{}, ^bb{}{}, ^bb{}{}",
                c, true_target, true_args, false_target, false_args
            ))
        }
        Switch {
            value,
            cases,
            default,
        } => {
            let v = ssa_ref(*value, name_map);
            let mut case_str = String::new();
            for (case_val, target) in cases {
                if !case_str.is_empty() {
                    case_str.push_str(", ");
                }
                let args = get_phi_args(*target, current_block_id, all_blocks, name_map);
                case_str.push_str(&format!("{}: ^bb{}{}", case_val, target, args));
            }
            let default_args = get_phi_args(*default, current_block_id, all_blocks, name_map);
            Ok(format!(
                "cf.switch %{} : i64, [{}], default: ^bb{}{}",
                v, case_str, default, default_args
            ))
        }
        Unreachable => Ok("llvm.unreachable".to_string()),
    }
}

/// Lower VIR instruction to MLIR (legacy, used by unit tests).
/// For main lowering pipeline use `lower_instruction_ssa`.
#[allow(dead_code)]
fn lower_instruction(inst: &VirInstruction) -> BackendResult<String> {
    let mut map = HashMap::new();
    let mut fresh = 0u32;
    match lower_instruction_ssa(inst, &mut map, &mut fresh)? {
        Some(s) => Ok(s),
        None => Ok(String::new()),
    }
}

/// Lower VIR type to MLIR type
fn lower_type(ty: &VirType) -> String {
    match ty {
        VirType::I8 => "i8".to_string(),
        VirType::I16 => "i16".to_string(),
        VirType::I32 => "i32".to_string(),
        VirType::I64 => "i64".to_string(),
        VirType::I128 => "i128".to_string(),
        VirType::U8 => "i8".to_string(),
        VirType::U16 => "i16".to_string(),
        VirType::U32 => "i32".to_string(),
        VirType::U64 => "i64".to_string(),
        VirType::U128 => "i128".to_string(),
        VirType::F32 => "f32".to_string(),
        VirType::F64 => "f64".to_string(),
        VirType::Bool => "i1".to_string(),
        VirType::Ptr => "!llvm.ptr".to_string(),
        VirType::TypedPtr(_) => "!llvm.ptr".to_string(),
        VirType::Void => "()".to_string(),
        _ => "!llvm.ptr".to_string(), // Default to pointer for complex types
    }
}

fn lower_cast_named(dest: &str, src: &str, from_ty: &VirType, to_ty: &VirType) -> String {
    let from = lower_type(from_ty);
    let to = lower_type(to_ty);

    // Integer to integer casts
    if from_ty.is_integer() && to_ty.is_integer() {
        let from_bits = from_ty.size_bytes().unwrap_or(8) * 8;
        let to_bits = to_ty.size_bytes().unwrap_or(8) * 8;
        let is_unsigned = matches!(
            from_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );
        if to_bits > from_bits {
            let op = if is_unsigned {
                "arith.extui"
            } else {
                "arith.extsi"
            };
            return format!("%{} = {} %{} : {} to {}", dest, op, src, from, to);
        }
        if to_bits < from_bits {
            return format!("%{} = arith.trunci %{} : {} to {}", dest, src, from, to);
        }
        // Same size: just use the value as-is (reinterpret)
        return format!("%{} = arith.bitcast %{} : {} to {}", dest, src, from, to);
    }

    // Float to float casts
    if from_ty.is_float() && to_ty.is_float() {
        let from_bits = from_ty.size_bytes().unwrap_or(8) * 8;
        let to_bits = to_ty.size_bytes().unwrap_or(8) * 8;
        if to_bits > from_bits {
            return format!("%{} = arith.extf %{} : {} to {}", dest, src, from, to);
        }
        if to_bits < from_bits {
            return format!("%{} = arith.truncf %{} : {} to {}", dest, src, from, to);
        }
        // Same size: just use the value as-is
        return format!("%{} = arith.bitcast %{} : {} to {}", dest, src, from, to);
    }

    // Integer to float
    if from_ty.is_integer() && to_ty.is_float() {
        let is_unsigned = matches!(
            from_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );
        let op = if is_unsigned {
            "arith.uitofp"
        } else {
            "arith.sitofp"
        };
        return format!("%{} = {} %{} : {} to {}", dest, op, src, from, to);
    }

    // Float to integer
    if from_ty.is_float() && to_ty.is_integer() {
        let is_unsigned = matches!(
            to_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );
        let op = if is_unsigned {
            "arith.fptoui"
        } else {
            "arith.fptosi"
        };
        return format!("%{} = {} %{} : {} to {}", dest, op, src, from, to);
    }

    // Integer to pointer
    if from_ty.is_integer() && matches!(to_ty, VirType::Ptr | VirType::TypedPtr(_)) {
        return format!("%{} = llvm.inttoptr %{} : {} to !llvm.ptr", dest, src, from);
    }

    // Pointer to integer
    if matches!(from_ty, VirType::Ptr | VirType::TypedPtr(_)) && to_ty.is_integer() {
        return format!("%{} = llvm.ptrtoint %{} : !llvm.ptr to {}", dest, src, to);
    }

    // Pointer to pointer (just bitcast)
    if matches!(from_ty, VirType::Ptr | VirType::TypedPtr(_))
        && matches!(to_ty, VirType::Ptr | VirType::TypedPtr(_))
    {
        return format!("%{} = llvm.bitcast %{} : !llvm.ptr to !llvm.ptr", dest, src);
    }

    // Bool to integer
    if matches!(from_ty, VirType::Bool) && to_ty.is_integer() {
        return format!("%{} = arith.extui %{} : i1 to {}", dest, src, to);
    }

    // Integer to bool
    if from_ty.is_integer() && matches!(to_ty, VirType::Bool) {
        let zero = format!("{}_zero", dest);
        return format!(
            "%{} = arith.constant 0 : {}\n    %{} = arith.cmpi ne, %{}, %{} : {}",
            zero, from, dest, src, zero, from
        );
    }

    // Complex types (structs, arrays, enums) - use llvm.bitcast
    if matches!(
        from_ty,
        VirType::Struct(_)
            | VirType::Array { .. }
            | VirType::Enum(_)
            | VirType::Tuple(_)
            | VirType::FuncPtr { .. }
    ) || matches!(
        to_ty,
        VirType::Struct(_)
            | VirType::Array { .. }
            | VirType::Enum(_)
            | VirType::Tuple(_)
            | VirType::FuncPtr { .. }
    ) {
        // For complex types, both are likely pointers
        return format!("%{} = llvm.bitcast %{} : !llvm.ptr to !llvm.ptr", dest, src);
    }

    // Fallback: bitcast (may not be semantically correct, but prevents errors)
    format!("%{} = llvm.bitcast %{} : {} to {}", dest, src, from, to)
}

#[allow(dead_code)]
fn lower_cast(dest: u32, value: u32, from_ty: &VirType, to_ty: &VirType) -> String {
    let from = lower_type(from_ty);
    let to = lower_type(to_ty);

    // Integer to integer casts
    if from_ty.is_integer() && to_ty.is_integer() {
        let from_bits = from_ty.size_bytes().unwrap_or(8) * 8;
        let to_bits = to_ty.size_bytes().unwrap_or(8) * 8;
        let is_unsigned = matches!(
            from_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );

        if to_bits > from_bits {
            let op = if is_unsigned {
                "arith.extui"
            } else {
                "arith.extsi"
            };
            return format!("%{} = {} %{} : {} to {}", dest, op, value, from, to);
        }
        if to_bits < from_bits {
            return format!("%{} = arith.trunci %{} : {} to {}", dest, value, from, to);
        }
        return format!("%{} = arith.bitcast %{} : {} to {}", dest, value, from, to);
    }

    // Float to float casts
    if from_ty.is_float() && to_ty.is_float() {
        let from_bits = from_ty.size_bytes().unwrap_or(8) * 8;
        let to_bits = to_ty.size_bytes().unwrap_or(8) * 8;
        if to_bits > from_bits {
            return format!("%{} = arith.extf %{} : {} to {}", dest, value, from, to);
        }
        if to_bits < from_bits {
            return format!("%{} = arith.truncf %{} : {} to {}", dest, value, from, to);
        }
        return format!("%{} = arith.bitcast %{} : {} to {}", dest, value, from, to);
    }

    // Integer to float
    if from_ty.is_integer() && to_ty.is_float() {
        let is_unsigned = matches!(
            from_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );
        let op = if is_unsigned {
            "arith.uitofp"
        } else {
            "arith.sitofp"
        };
        return format!("%{} = {} %{} : {} to {}", dest, op, value, from, to);
    }

    // Float to integer
    if from_ty.is_float() && to_ty.is_integer() {
        let is_unsigned = matches!(
            to_ty,
            VirType::U8 | VirType::U16 | VirType::U32 | VirType::U64 | VirType::U128
        );
        let op = if is_unsigned {
            "arith.fptoui"
        } else {
            "arith.fptosi"
        };
        return format!("%{} = {} %{} : {} to {}", dest, op, value, from, to);
    }

    // Integer to pointer
    if from_ty.is_integer() && matches!(to_ty, VirType::Ptr | VirType::TypedPtr(_)) {
        return format!(
            "%{} = llvm.inttoptr %{} : {} to !llvm.ptr",
            dest, value, from
        );
    }

    // Pointer to integer
    if matches!(from_ty, VirType::Ptr | VirType::TypedPtr(_)) && to_ty.is_integer() {
        return format!("%{} = llvm.ptrtoint %{} : !llvm.ptr to {}", dest, value, to);
    }

    // Pointer to pointer or complex types - use bitcast
    format!("%{} = llvm.bitcast %{} : {} to {}", dest, value, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::vir::*;

    #[test]
    fn test_lower_type() {
        assert_eq!(lower_type(&VirType::I64), "i64");
        assert_eq!(lower_type(&VirType::F64), "f64");
        assert_eq!(lower_type(&VirType::Bool), "i1");
    }

    #[test]
    fn test_lower_simple_function() {
        let config = MlirConfig::default();
        let func = VirFunction {
            name: "test".to_string(),
            params: vec![VirParam {
                name: "x".to_string(),
                ty: VirType::I64,
            }],
            return_type: VirType::I64,
            blocks: vec![],
            locals: vec![],
            is_async: false,
        };

        let result = lower_function(&func, &config);
        assert!(result.is_ok());
        let mlir = result.unwrap();
        assert!(mlir.contains("func.func @test"));
        assert!(mlir.contains("i64"));
    }

    #[test]
    fn test_lower_cast_int_widen() {
        let mlir = lower_cast(1, 2, &VirType::I32, &VirType::I64);
        assert!(mlir.contains("arith.extsi"));
    }

    #[test]
    fn test_lower_cast_uint_widen() {
        let mlir = lower_cast(1, 2, &VirType::U32, &VirType::U64);
        assert!(mlir.contains("arith.extui"));
    }

    #[test]
    fn test_lower_cast_float_to_int() {
        let mlir = lower_cast(1, 2, &VirType::F64, &VirType::I32);
        assert!(mlir.contains("arith.fptosi"));
    }
}
