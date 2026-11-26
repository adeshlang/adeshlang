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

/// Target-specific ABI/storage parameters for splitting 32-bit values into two 16-bit halves.
#[derive(Debug, Clone, Copy)]
pub struct PairLegalizeProfile {
    pub arch: &'static str,
    /// Offset between the low and high half in stack slots / memory (`2` bytes on AVR, `1` word on DSP56000).
    pub slot_step: i32,
    /// Stride between low and high half in adjacent physical register IDs (`2` on AVR, `1` on DSP56000).
    pub phys_step: u8,
    /// Primary physical return register used when generic lowering targets `return_reg` -> maps to `(ret_lo, ret_hi)`.
    pub ret_alias: u8,
    pub ret_lo: u8,
    pub ret_hi: u8,
    /// Helper ABI argument pairs `(arg0_lo, arg0_hi)` and `(arg1_lo, arg1_hi)` for 32-bit `mul/div/mod/shl/shr/sar`.
    pub helper_arg0_lo: u8,
    pub helper_arg0_hi: u8,
    pub helper_arg1_lo: u8,
    pub helper_arg1_hi: u8,
    pub helper_prefix: &'static str,
    pub half_width_bytes: u8,
}

impl PairLegalizeProfile {
    pub const fn avr() -> Self {
        Self {
            arch: "avr",
            slot_step: 2,
            phys_step: 2,
            ret_alias: 24,
            ret_lo: 22,
            ret_hi: 24,
            helper_arg0_lo: 22,
            helper_arg0_hi: 24,
            helper_arg1_lo: 18,
            helper_arg1_hi: 20,
            helper_prefix: "__avr_",
            half_width_bytes: 2,
        }
    }

    pub const fn dsp56000() -> Self {
        Self {
            arch: "dsp56000",
            slot_step: 1,
            phys_step: 1,
            ret_alias: 0,
            ret_lo: 0,
            ret_hi: 1,
            helper_arg0_lo: 0,
            helper_arg0_hi: 1,
            helper_arg1_lo: 2,
            helper_arg1_hi: 3,
            helper_prefix: "__dsp56k_",
            half_width_bytes: 1,
        }
    }
}

/// Split every 32-bit (`width == 4`) virtual register and 4-byte operation in
/// `func` into pairs of 16-bit (`width == 2`) register-pair operations.
pub fn legalize_32bit_to_16bit_pairs(
    func: &mut MachineFunction,
    arch: &str,
) -> Result<(), CodegenError> {
    let profile = if arch == "dsp56000" {
        PairLegalizeProfile::dsp56000()
    } else {
        PairLegalizeProfile {
            arch: "avr",
            ..PairLegalizeProfile::avr()
        }
    };
    legalize_32bit_with_profile(func, &profile, arch)
}

pub fn legalize_32bit_with_profile(
    func: &mut MachineFunction,
    profile: &PairLegalizeProfile,
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
        func.set_vreg_width(lo, profile.half_width_bytes);
        func.set_vreg_width(hi, profile.half_width_bytes);
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
                if p.0 == profile.ret_alias {
                    (
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            profile.ret_lo,
                        ))),
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            profile.ret_hi,
                        ))),
                    )
                } else {
                    (
                        MachineOperand::Register(MachineRegister::Physical(*p)),
                        MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(
                            p.0.saturating_add(profile.phys_step),
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
                MachineOperand::StackSlot(*off + profile.slot_step),
            ),
            MachineOperand::Memory { base, offset, index } => (
                MachineOperand::Memory {
                    base: *base,
                    offset: *offset,
                    index: *index,
                },
                MachineOperand::Memory {
                    base: *base,
                    offset: *offset + profile.slot_step,
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
                        size: profile.half_width_bytes,
                    });
                    out.push(MachineInstruction::Load {
                        dst: d_hi,
                        src: s_hi,
                        size: profile.half_width_bytes,
                    });
                }
                MachineInstruction::Store { dst, src, size: 4 } => {
                    let (d_lo, d_hi) = split_op(&dst, &pairs);
                    let (s_lo, s_hi) = split_op(&src, &pairs);
                    out.push(MachineInstruction::Store {
                        dst: d_lo,
                        src: s_lo,
                        size: profile.half_width_bytes,
                    });
                    out.push(MachineInstruction::Store {
                        dst: d_hi,
                        src: s_hi,
                        size: profile.half_width_bytes,
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
                    emit_32bit_helper_call(
                        &format!("{}mul32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Div { dst, src } if is_wide_op(&dst) => {
                    emit_32bit_helper_call(
                        &format!("{}div32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Mod { dst, src } if is_wide_op(&dst) => {
                    emit_32bit_helper_call(
                        &format!("{}mod32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Shl { dst, src } if is_wide_op(&dst) => {
                    emit_32bit_helper_call(
                        &format!("{}shl32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Shr { dst, src } if is_wide_op(&dst) => {
                    emit_32bit_helper_call(
                        &format!("{}shr32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Sar { dst, src } if is_wide_op(&dst) => {
                    emit_32bit_helper_call(
                        &format!("{}sar32", profile.helper_prefix),
                        profile,
                        &dst,
                        &src,
                        &pairs,
                        &mut out,
                    );
                }
                MachineInstruction::Custom { name, operands }
                    if (name == "avr_pack_i32" || name == "pack_i32") && operands.len() == 3 =>
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
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::PhysicalRegister(p),
                                        MoveLocation::VirtualRegister(v_lo),
                                        profile.half_width_bytes,
                                    ));
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::PhysicalRegister(PhysicalRegister(
                                            p.0.saturating_add(profile.phys_step),
                                        )),
                                        MoveLocation::VirtualRegister(v_hi),
                                        profile.half_width_bytes,
                                    ));
                                }
                                MoveLocation::StackSlot { base, offset } => {
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::StackSlot { base, offset },
                                        MoveLocation::VirtualRegister(v_lo),
                                        profile.half_width_bytes,
                                    ));
                                    expanded_moves.push(MoveOperation::new(
                                        MoveLocation::StackSlot {
                                            base,
                                            offset: offset + profile.slot_step,
                                        },
                                        MoveLocation::VirtualRegister(v_hi),
                                        profile.half_width_bytes,
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

fn emit_32bit_helper_call(
    helper: &str,
    profile: &PairLegalizeProfile,
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
    let hw = profile.half_width_bytes;
    if let (Some(dl), Some(dh), Some(sl), Some(sh)) = (
        to_move_loc(&d_lo),
        to_move_loc(&d_hi),
        to_move_loc(&s_lo),
        to_move_loc(&s_hi),
    ) {
        out.push(MachineInstruction::ParallelMove {
            moves: vec![
                MoveOperation::new(
                    MoveLocation::PhysicalRegister(PhysicalRegister(profile.helper_arg0_lo)),
                    dl,
                    hw,
                ),
                MoveOperation::new(
                    MoveLocation::PhysicalRegister(PhysicalRegister(profile.helper_arg0_hi)),
                    dh,
                    hw,
                ),
                MoveOperation::new(
                    MoveLocation::PhysicalRegister(PhysicalRegister(profile.helper_arg1_lo)),
                    sl,
                    hw,
                ),
                MoveOperation::new(
                    MoveLocation::PhysicalRegister(PhysicalRegister(profile.helper_arg1_hi)),
                    sh,
                    hw,
                ),
            ],
        });
    } else {
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(profile.helper_arg0_lo),
            src: d_lo.clone(),
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(profile.helper_arg0_hi),
            src: d_hi.clone(),
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(profile.helper_arg1_lo),
            src: s_lo,
        });
        out.push(MachineInstruction::Move {
            dst: MachineOperand::phys(profile.helper_arg1_hi),
            src: s_hi,
        });
    }
    out.push(MachineInstruction::Call {
        target: MachineOperand::Symbol(helper.to_string()),
        num_args: 2,
    });
    out.push(MachineInstruction::Move {
        dst: d_lo,
        src: MachineOperand::phys(profile.helper_arg0_lo),
    });
    out.push(MachineInstruction::Move {
        dst: d_hi,
        src: MachineOperand::phys(profile.helper_arg0_hi),
    });
}
