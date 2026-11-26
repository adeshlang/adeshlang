//! Target-aware integer and operand legalization passes.
//!
//! Lowers wide integer operations (such as 32-bit `i32`/`u32` on 16-bit/8-bit
//! targets whose native register unit is 16-bit register pairs) into sequences
//! of word-sized instructions and carry-chained operations (`AddWithCarry`,
//! `SubWithBorrow`, `CompareWithBorrow`), or target runtime helper calls for
//! non-trivial multi-word operations (`mul`, `div`, `mod`, variable shifts).

#![forbid(unsafe_code)]

use crate::calling_convention::{MoveLocation, MoveOperation};
use crate::error::CodegenError;
use crate::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, PhysicalRegister,
    RegisterClass, VirtualRegister,
};
use std::collections::HashMap;

/// Split every 32-bit (`width == 4`) virtual register and 4-byte operation in
/// `func` into pairs of 16-bit (`width == 2`) register-pair operations.
///
/// Used by 8-bit/16-bit backends such as ATmega328P AVR (whose allocatable unit
/// is a 16-bit even register pair `r(p+1):r(p)`).
pub fn legalize_32bit_to_16bit_pairs(
    func: &mut MachineFunction,
    arch: &str,
) -> Result<(), CodegenError> {
    let wide_vregs: Vec<VirtualRegister> = func
        .vreg_widths
        .iter()
        .filter_map(|(&v, &w)| if w == 4 { Some(v) } else { None })
        .collect();

    if wide_vregs.is_empty()
        && !func.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(
                    i,
                    MachineInstruction::Load { size: 4, .. }
                        | MachineInstruction::Store { size: 4, .. }
                )
            })
        })
    {
        return Ok(());
    }

    let mut pairs: HashMap<VirtualRegister, (VirtualRegister, VirtualRegister)> = HashMap::new();
    for v in wide_vregs {
        let lo = func.alloc_vreg_with_class(RegisterClass::Gpr);
        let hi = func.alloc_vreg_with_class(RegisterClass::Gpr);
        func.set_vreg_width(lo, 2);
        func.set_vreg_width(hi, 2);
        pairs.insert(v, (lo, hi));
    }

    let is_wide_op = |op: &MachineOperand| -> bool {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => pairs.contains_key(v),
            _ => false,
        }
    };

    let split_op = |op: &MachineOperand,
                    pairs: &HashMap<VirtualRegister, (VirtualRegister, VirtualRegister)>|
     -> (MachineOperand, MachineOperand) {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                if let Some(&(lo, hi)) = pairs.get(v) {
                    (
                        MachineOperand::Register(MachineRegister::Virtual(lo)),
                        MachineOperand::Register(MachineRegister::Virtual(hi)),
                    )
                } else {
                    (op.clone(), MachineOperand::Immediate(0))
                }
            }
            MachineOperand::Register(MachineRegister::Physical(p)) => {
                // On AVR, 32-bit physical return/arg base is `r_lo` (e.g. r22), hi is `r_lo + 2` (r24).
                // If `p.0 == 24` was used as generic return reg for a 32-bit value, return pair is (22, 24).
                if p.0 == 24 {
                    (
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(22))),
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(24))),
                    )
                } else {
                    (
                        MachineOperand::Register(MachineRegister::Physical(*p)),
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            p.0.saturating_add(2),
                        ))),
                    )
                }
            }
            MachineOperand::Immediate(val) => {
                let lo = (*val as i16) as i64;
                let hi = ((*val >> 16) as i16) as i64;
                (MachineOperand::Immediate(lo), MachineOperand::Immediate(hi))
            }
            MachineOperand::StackSlot(off) => (
                MachineOperand::StackSlot(*off),
                MachineOperand::StackSlot(*off + 2),
            ),
            MachineOperand::Memory { base, offset, index } => (
                MachineOperand::Memory {
                    base: *base,
                    offset: *offset,
                    index: *index,
                },
                MachineOperand::Memory {
                    base: *base,
                    offset: *offset + 2,
                    index: *index,
                },
            ),
            _ => (op.clone(), MachineOperand::Immediate(0)),
        }
    };

    for block in &mut func.blocks {
        let orig = std::mem::take(&mut block.instructions);
        let mut out = Vec::with_capacity(orig.len() * 2);

        for inst in orig {
            match inst {
                MachineInstruction::Move { dst, src }
                    if is_wide_op(&dst) || is_wide_op(&src) =>
                {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    if matches!(d_hi, MachineOperand::Immediate(_)) {
                        return Err(CodegenError::new(
                            arch,
                            format!(
                                "32-bit Move in `{}` has non-wide dst {dst:?} for wide src {src:?}",
                                func.name
                            ),
                        ));
                    }
                    out.push(MachineInstruction::Move { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::Move { dst: d_hi, src: s_hi });
                }
                MachineInstruction::Load { dst, src, size: 4 } => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Load {
                        dst: d_lo,
                        src: s_lo,
                        size: 2,
                    });
                    out.push(MachineInstruction::Load {
                        dst: d_hi,
                        src: s_hi,
                        size: 2,
                    });
                }
                MachineInstruction::Store { dst, src, size: 4 } => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Store {
                        dst: d_lo,
                        src: s_lo,
                        size: 2,
                    });
                    out.push(MachineInstruction::Store {
                        dst: d_hi,
                        src: s_hi,
                        size: 2,
                    });
                }
                MachineInstruction::Add { dst, src } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Add { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::AddWithCarry { dst: d_hi, src: s_hi });
                }
                MachineInstruction::Sub { dst, src } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Sub { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::SubWithBorrow { dst: d_hi, src: s_hi });
                }
                MachineInstruction::And { dst, src } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::And { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::And { dst: d_hi, src: s_hi });
                }
                MachineInstruction::Or { dst, src } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Or { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::Or { dst: d_hi, src: s_hi });
                }
                MachineInstruction::Xor { dst, src } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Xor { dst: d_lo, src: s_lo });
                    out.push(MachineInstruction::Xor { dst: d_hi, src: s_hi });
                }
                MachineInstruction::Not { dst } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    out.push(MachineInstruction::Not { dst: d_lo });
                    out.push(MachineInstruction::Not { dst: d_hi });
                }
                MachineInstruction::Neg { dst } if is_wide_op(&dst) => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    out.push(MachineInstruction::Not { dst: d_lo.clone() });
                    out.push(MachineInstruction::Not { dst: d_hi.clone() });
                    out.push(MachineInstruction::Add {
                        dst: d_lo,
                        src: MachineOperand::Immediate(1),
                    });
                    out.push(MachineInstruction::AddWithCarry {
                        dst: d_hi,
                        src: MachineOperand::Immediate(0),
                    });
                }
                MachineInstruction::Compare { lhs, rhs }
                    if is_wide_op(&lhs) || is_wide_op(&rhs) =>
                {
                    let (l_lo, l_hi) = split_op(&lhs, &pairs);
                    let (r_lo, r_hi) = split_op(&rhs, &pairs);
                    out.push(MachineInstruction::Compare {
                        lhs: l_lo,
                        rhs: r_lo,
                    });
                    out.push(MachineInstruction::CompareWithBorrow {
                        lhs: l_hi,
                        rhs: r_hi,
                    });
                }
                MachineInstruction::Mul { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_mul32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Div { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_div32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Mod { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_mod32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Shl { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_shl32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Shr { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_shr32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Sar { dst, src } if is_wide_op(&dst) => {
                    emit_avr_32bit_helper_call("__avr_sar32", &dst, &src, &pairs, &mut out);
                }
                MachineInstruction::Custom { name, operands }
                    if name == "avr_pack_i32" && operands.len() == 3 =>
                {
                    let (w_lo, w_hi) = split_op(&operands[0], &pairs);
                    out.push(MachineInstruction::Move {
                        dst: w_lo,
                        src: operands[1].clone(),
                    });
                    out.push(MachineInstruction::Move {
                        dst: w_hi,
                        src: operands[2].clone(),
                    });
                }
                MachineInstruction::ParallelMove { moves } => {
                    let mut expanded_moves = Vec::with_capacity(moves.len());
                    for m in moves {
                        let wide_src = matches!(
                            m.src,
                            MoveLocation::VirtualRegister(v) if pairs.contains_key(&v)
                        );
                        if wide_src {
                            let MoveLocation::VirtualRegister(v) = m.src else {
                                unreachable!()
                            };
                            let (v_lo, v_hi) = pairs[&v];
                            match m.dst {
                                MoveLocation::PhysicalRegister(p) => {
                                    // `p` is the low register of an adjacent pair (p, p+2).
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::PhysicalRegister(p),
                                        MoveLocation::VirtualRegister(v_lo),
                                        2,
                                    ));
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::PhysicalRegister(PhysicalRegister(
                                            p.0.saturating_add(2),
                                        )),
                                        MoveLocation::VirtualRegister(v_hi),
                                        2,
                                    ));
                                }
                                MoveLocation::StackSlot { base, offset } => {
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::StackSlot { base, offset },
                                        MoveLocation::VirtualRegister(v_lo),
                                        2,
                                    ));
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::StackSlot {
                                            base,
                                            offset: offset + 2,
                                        },
                                        MoveLocation::VirtualRegister(v_hi),
                                        2,
                                    ));
                                }
                                _ => {
                                    return Err(CodegenError::new(
                                        arch,
                                        format!(
                                            "cannot legalize 32-bit parallel move destination {:?} in `{}`",
                                            m.dst, func.name
                                        ),
                                    ));
                                }
                            }
                        } else {
                            expanded_moves.push(m);
                        }
                    }
                    out.push(MachineInstruction::ParallelMove {
                        moves: expanded_moves,
                    });
                }
                other => out.push(other),
            }
        }
        block.instructions = out;
    }

    Ok(())
}

fn emit_avr_32bit_helper_call(
    helper: &str,
    dst: &MachineOperand,
    src: &MachineOperand,
    pairs: &HashMap<VirtualRegister, (VirtualRegister, VirtualRegister)>,
    out: &mut Vec<MachineInstruction>,
) {
    let (d_lo, d_hi) = match dst {
        MachineOperand::Register(MachineRegister::Virtual(v)) => {
            if let Some(&(lo, hi)) = pairs.get(v) {
                (
                    MachineOperand::Register(MachineRegister::Virtual(lo)),
                    MachineOperand::Register(MachineRegister::Virtual(hi)),
                )
            } else {
                (dst.clone(), MachineOperand::Immediate(0))
            }
        }
        _ => (dst.clone(), MachineOperand::Immediate(0)),
    };
    let (s_lo, s_hi) = match src {
        MachineOperand::Register(MachineRegister::Virtual(v)) => {
            if let Some(&(lo, hi)) = pairs.get(v) {
                (
                    MachineOperand::Register(MachineRegister::Virtual(lo)),
                    MachineOperand::Register(MachineRegister::Virtual(hi)),
                )
            } else {
                (
                    MachineOperand::Register(MachineRegister::Virtual(*v)),
                    MachineOperand::Immediate(0),
                )
            }
        }
        MachineOperand::Immediate(val) => (
            MachineOperand::Immediate((*val as i16) as i64),
            MachineOperand::Immediate(((*val >> 16) as i16) as i64),
        ),
        _ => (src.clone(), MachineOperand::Immediate(0)),
    };

    // avr-gcc 32-bit binary op ABI: arg0 in r25:r24:r23:r22, arg1 in r21:r20:r19:r18, ret in r25:r24:r23:r22.
    let to_move_loc = |op: &MachineOperand| -> Option<MoveLocation> {
        match op {
            MachineOperand::Register(MachineRegister::Virtual(v)) => {
                Some(MoveLocation::VirtualRegister(*v))
            }
            MachineOperand::Register(MachineRegister::Physical(p)) => {
                Some(MoveLocation::PhysicalRegister(*p))
            }
            MachineOperand::Immediate(imm) => Some(MoveLocation::Immediate(*imm)),
            _ => None,
        }
    };
    if let (Some(dl), Some(dh), Some(sl), Some(sh)) = (
        to_move_loc(&d_lo),
        to_move_loc(&d_hi),
        to_move_loc(&s_lo),
        to_move_loc(&s_hi),
    ) {
        out.push(MachineInstruction::ParallelMove {
            moves: vec![
                MoveOperation::new(MoveLocation::PhysicalRegister(PhysicalRegister(22)), dl, 2),
                MoveOperation::new(MoveLocation::PhysicalRegister(PhysicalRegister(24)), dh, 2),
                MoveOperation::new(MoveLocation::PhysicalRegister(PhysicalRegister(18)), sl, 2),
                MoveOperation::new(MoveLocation::PhysicalRegister(PhysicalRegister(20)), sh, 2),
            ],
        });
    } else {
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(22),
            src: d_lo.clone(),
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(24),
            src: d_hi.clone(),
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(18),
            src: s_lo,
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(20),
            src: s_hi,
        });
    }
    out.push(MachineInstruction::Call {
        target: MachineOperand::Symbol(helper.to_string()),
        num_args: 2,
    });
    out.push(MachineInstruction::Move {
        dst: d_lo,
        src: MachineOperand::phys(22),
    });
    out.push(MachineInstruction::Move {
        dst: d_hi,
        src: MachineOperand::phys(24),
    });
}
