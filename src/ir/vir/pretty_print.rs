//! VIR Pretty Printer
//!
//! Human-readable representation of VIR for debugging.

use super::{VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator};
use std::fmt;

impl fmt::Display for VirModule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "module {} {{", self.name)?;

        for func in &self.functions {
            write!(f, "{}", func)?;
        }

        writeln!(f, "}}")
    }
}

impl fmt::Display for VirFunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "  fn {}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", param.name, param.ty)?;
        }
        writeln!(f, ") -> {} {{", self.return_type)?;

        for block in &self.blocks {
            write!(f, "{}", block)?;
        }

        writeln!(f, "  }}")
    }
}

impl fmt::Display for VirBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(label) = &self.label {
            writeln!(f, "  {}:", label)?;
        } else {
            writeln!(f, "  bb{}:", self.id)?;
        }

        for phi in &self.phis {
            writeln!(
                f,
                "    %{} = phi {} [{}]",
                phi.dest,
                phi.ty,
                phi.incoming
                    .iter()
                    .map(|(b, v)| format!("bb{}: %{}", b, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }

        for inst in &self.instructions {
            writeln!(f, "    {}", inst)?;
        }

        writeln!(f, "    {}", self.terminator)
    }
}

impl fmt::Display for VirInstruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VirInstruction::ConstInt { dest, value, ty } => {
                write!(f, "%{} = const.{} {}", dest, ty, value)
            }
            VirInstruction::ConstBool { dest, value } => {
                write!(f, "%{} = const.bool {}", dest, value)
            }
            VirInstruction::Load { dest, ptr, ty } => {
                write!(f, "%{} = load {} %{}", dest, ty, ptr)
            }
            VirInstruction::Store { ptr, value } => {
                write!(f, "store %{}, %{}", ptr, value)
            }
            VirInstruction::ArcClone { dest, src } => {
                write!(f, "%{} = arc.clone %{}", dest, src)
            }
            VirInstruction::ArcDrop { ptr } => {
                write!(f, "arc.drop %{}", ptr)
            }
            VirInstruction::Drop { value } => {
                write!(f, "drop %{}", value)
            }
            VirInstruction::IntBinOp {
                dest,
                op,
                lhs,
                rhs,
                ty,
            } => {
                write!(f, "%{} = {:?}.{} %{}, %{}", dest, op, ty, lhs, rhs)
            }
            VirInstruction::Call { dest, func, args } => {
                if let Some(dest) = dest {
                    write!(f, "%{} = ", dest)?;
                }
                write!(f, "call %{}(", func)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "%{}", arg)?;
                }
                write!(f, ")")
            }
            VirInstruction::Nop => write!(f, "nop"),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl fmt::Display for VirTerminator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VirTerminator::Return { value } => {
                if let Some(v) = value {
                    write!(f, "ret %{}", v)
                } else {
                    write!(f, "ret")
                }
            }
            VirTerminator::Jump { target } => {
                write!(f, "jump bb{}", target)
            }
            VirTerminator::Branch {
                cond,
                true_target,
                false_target,
            } => {
                write!(f, "br %{}, bb{}, bb{}", cond, true_target, false_target)
            }
            VirTerminator::Switch {
                value,
                cases,
                default,
            } => {
                write!(f, "switch %{} [", value)?;
                for (val, target) in cases {
                    write!(f, "{}: bb{}, ", val, target)?;
                }
                write!(f, "default: bb{}]", default)
            }
            VirTerminator::Unreachable => write!(f, "unreachable"),
        }
    }
}
