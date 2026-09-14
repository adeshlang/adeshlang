//! Instruction dispatcher
//!
//! This module provides the main instruction dispatch logic that routes
//! LIR instructions to their appropriate handlers.

use cranelift::prelude::*;
use cranelift_module::Module;

use crate::backends::aot::cranelift::FunctionCompileContext;
use crate::backends::aot::cranelift_impl::instructions;
use crate::backends::common::lir::LirInst;

/// Dispatch a LIR instruction to its appropriate handler
///
/// This function handles the remaining instruction types that aren't
/// handled by the specialized lower_*_instruction functions.
pub(crate) fn dispatch_instruction(
    ctx: &mut FunctionCompileContext,
    builder: &mut FunctionBuilder,
    module: &mut dyn Module,
    inst: &LirInst,
) -> Result<(), String> {
    match inst {
        // Function calls
        LirInst::Call(dst, func_name, args) => {
            instructions::handle_call(ctx, builder, module, dst, func_name, args)
        }

        LirInst::CallBuiltin(dst, builtin_name, args) => {
            instructions::handle_call_builtin(ctx, builder, module, dst, builtin_name, args)
        }

        LirInst::CallBuiltinGeneric(dst, builtin_name, args, generic_type) => {
            instructions::handle_call_builtin_generic(
                ctx,
                builder,
                module,
                dst,
                builtin_name,
                args,
                generic_type,
            )
        }

        LirInst::TailCall(func_name, args) => {
            instructions::handle_tail_call(ctx, builder, module, func_name, args)
        }

        // All other instructions are handled by specialized lower_*_instruction functions
        // These patterns exist only to satisfy exhaustiveness checking
        LirInst::Jump(_)
        | LirInst::JumpIf(_, _, _)
        | LirInst::Return(_)
        | LirInst::Phi(_, _)
        | LirInst::CmpLtI64(_, _, _)
        | LirInst::CmpLeI64(_, _, _)
        | LirInst::CmpGtI64(_, _, _)
        | LirInst::CmpGeI64(_, _, _)
        | LirInst::CmpEqI64(_, _, _)
        | LirInst::CmpNeI64(_, _, _)
        | LirInst::CmpLtF64(_, _, _)
        | LirInst::CmpLeF64(_, _, _)
        | LirInst::CmpGtF64(_, _, _)
        | LirInst::CmpGeF64(_, _, _)
        | LirInst::CmpEqF64(_, _, _)
        | LirInst::CmpNeF64(_, _, _)
        | LirInst::I64ToF64(_, _)
        | LirInst::F64ToI64(_, _)
        | LirInst::ConstI64(_, _)
        | LirInst::ConstF64(_, _)
        | LirInst::ConstBool(_, _)
        | LirInst::ConstString(_, _)
        | LirInst::ConstBigInt(_, _)
        | LirInst::ConstNull(_)
        | LirInst::ConstFunc(_, _, _, _)
        | LirInst::ConstU8(_, _)
        | LirInst::ConstU16(_, _)
        | LirInst::ConstU32(_, _)
        | LirInst::ConstU64(_, _)
        | LirInst::ConstU128(_, _)
        | LirInst::ConstI8(_, _)
        | LirInst::ConstI16(_, _)
        | LirInst::ConstI32(_, _)
        | LirInst::ConstI128(_, _)
        | LirInst::ConstF32(_, _)
        | LirInst::LoadVar(_, _)
        | LirInst::StoreVar(_, _)
        | LirInst::LoadModule(_, _)
        | LirInst::Copy(_, _)
        | LirInst::AddI64(_, _, _)
        | LirInst::SubI64(_, _, _)
        | LirInst::MulI64(_, _, _)
        | LirInst::DivI64(_, _, _)
        | LirInst::ModI64(_, _, _)
        | LirInst::NegI64(_, _)
        | LirInst::AddF64(_, _, _)
        | LirInst::SubF64(_, _, _)
        | LirInst::MulF64(_, _, _)
        | LirInst::DivF64(_, _, _)
        | LirInst::NegF64(_, _)
        | LirInst::And(_, _, _)
        | LirInst::Or(_, _, _)
        | LirInst::Not(_, _)
        | LirInst::BitAnd(_, _, _)
        | LirInst::BitOr(_, _, _)
        | LirInst::BitXor(_, _, _)
        | LirInst::Shl(_, _, _)
        | LirInst::Shr(_, _, _)
        | LirInst::Alloc(_, _)
        | LirInst::AllocTyped(_, _, _)
        | LirInst::Free(_)
        | LirInst::PtrLoad(_, _, _)
        | LirInst::PtrStore(_, _, _)
        | LirInst::ArcNew(_, _)
        | LirInst::ArcClone(_, _)
        | LirInst::ArcDrop(_)
        | LirInst::ArcGet(_, _)
        | LirInst::ArcSet(_, _)
        | LirInst::ArcStrongCount(_, _)
        | LirInst::ArcWeakCount(_, _)
        | LirInst::WeakNew(_, _)
        | LirInst::WeakDrop(_) => {
            // Already handled by lower_*_instruction functions above
            Ok(())
        }
    }
}
