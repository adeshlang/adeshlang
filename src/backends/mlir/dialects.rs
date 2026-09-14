//! MLIR Dialect Abstractions
//!
//! Wrapper types for MLIR dialects used in lowering.

/// Arith dialect operations
pub mod arith {
    /// Integer addition
    pub fn addi(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.addi {}, {} : {}", lhs, rhs, ty)
    }

    /// Integer subtraction
    pub fn subi(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.subi {}, {} : {}", lhs, rhs, ty)
    }

    /// Integer multiplication
    pub fn muli(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.muli {}, {} : {}", lhs, rhs, ty)
    }

    /// Signed integer division
    pub fn divsi(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.divsi {}, {} : {}", lhs, rhs, ty)
    }

    /// Float addition
    pub fn addf(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.addf {}, {} : {}", lhs, rhs, ty)
    }

    /// Float subtraction
    pub fn subf(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.subf {}, {} : {}", lhs, rhs, ty)
    }

    /// Float multiplication
    pub fn mulf(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.mulf {}, {} : {}", lhs, rhs, ty)
    }

    /// Float division
    pub fn divf(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("arith.divf {}, {} : {}", lhs, rhs, ty)
    }

    /// Constant
    pub fn constant(value: &str, ty: &str) -> String {
        format!("arith.constant {} : {}", value, ty)
    }
}

/// Math dialect operations
pub mod math {
    /// Square root
    pub fn sqrt(operand: &str, ty: &str) -> String {
        format!("math.sqrt {} : {}", operand, ty)
    }

    /// Absolute value float
    pub fn absf(operand: &str, ty: &str) -> String {
        format!("math.absf {} : {}", operand, ty)
    }

    /// Power float
    pub fn powf(lhs: &str, rhs: &str, ty: &str) -> String {
        format!("math.powf {}, {} : {}", lhs, rhs, ty)
    }

    /// Exponential
    pub fn exp(operand: &str, ty: &str) -> String {
        format!("math.exp {} : {}", operand, ty)
    }

    /// Natural logarithm
    pub fn log(operand: &str, ty: &str) -> String {
        format!("math.log {} : {}", operand, ty)
    }

    /// Sine
    pub fn sin(operand: &str, ty: &str) -> String {
        format!("math.sin {} : {}", operand, ty)
    }

    /// Cosine
    pub fn cos(operand: &str, ty: &str) -> String {
        format!("math.cos {} : {}", operand, ty)
    }
}

/// Affine dialect operations for high-performance numerical loops
pub mod affine {
    /// Affine for loop
    pub fn for_op(lower: &str, upper: &str, step: &str) -> String {
        format!("affine.for %i = {} to {} step {}", lower, upper, step)
    }

    /// Affine load from memref
    pub fn load(ptr: &str, indices: &[&str], ty: &str) -> String {
        let idx_str = indices.join(", ");
        format!("affine.load {}[{}] : {}", ptr, idx_str, ty)
    }

    /// Affine store to memref
    pub fn store(value: &str, ptr: &str, indices: &[&str], ty: &str) -> String {
        let idx_str = indices.join(", ");
        format!("affine.store {}, {}[{}] : {}", value, ptr, idx_str, ty)
    }
}

/// Memref dialect operations
pub mod memref {
    /// Allocate memory
    pub fn alloc(size: Option<&str>, ty: &str) -> String {
        if let Some(s) = size {
            format!("memref.alloc({}) : {}", s, ty)
        } else {
            format!("memref.alloc() : {}", ty)
        }
    }

    /// Deallocate memory
    pub fn dealloc(ptr: &str) -> String {
        format!("memref.dealloc {}", ptr)
    }

    /// Load from memory
    pub fn load(ptr: &str, indices: &[&str], ty: &str) -> String {
        let idx_str = indices.join(", ");
        format!("memref.load {}[{}] : {}", ptr, idx_str, ty)
    }

    /// Store to memory
    pub fn store(value: &str, ptr: &str, indices: &[&str], ty: &str) -> String {
        let idx_str = indices.join(", ");
        format!("memref.store {}, {}[{}] : {}", value, ptr, idx_str, ty)
    }
}

/// SCF (Structured Control Flow) dialect operations
pub mod scf {
    /// If statement
    pub fn if_op(condition: &str) -> String {
        format!("scf.if {}", condition)
    }

    /// For loop
    pub fn for_op(lower: &str, upper: &str, step: &str) -> String {
        format!("scf.for %i = {} to {} step {}", lower, upper, step)
    }

    /// While loop
    pub fn while_op() -> String {
        "scf.while".to_string()
    }
}

/// CF (Control Flow) dialect operations
pub mod cf {
    /// Unconditional branch
    pub fn br(target: &str) -> String {
        format!("cf.br {}", target)
    }

    /// Conditional branch
    pub fn cond_br(condition: &str, true_target: &str, false_target: &str) -> String {
        format!(
            "cf.cond_br {}, {}, {}",
            condition, true_target, false_target
        )
    }
}

/// GPU dialect operations
pub mod gpu {
    /// Launch GPU kernel
    pub fn launch_func(kernel: &str, grid: &str, block: &str) -> String {
        format!(
            "gpu.launch_func @{} blocks in {} threads in {}",
            kernel, grid, block
        )
    }

    /// GPU memory allocation
    pub fn alloc(size: &str, ty: &str) -> String {
        format!("gpu.alloc({}) : {}", size, ty)
    }

    /// GPU memory deallocation
    pub fn dealloc(ptr: &str) -> String {
        format!("gpu.dealloc {}", ptr)
    }
}

/// LLVM dialect operations
pub mod llvm {
    /// LLVM pointer type
    pub fn ptr_type() -> String {
        "!llvm.ptr".to_string()
    }

    /// LLVM atomic RMW (read-modify-write)
    pub fn atomic_rmw_add(ptr: &str, value: &str, ordering: &str) -> String {
        format!("llvm.atomicrmw add {} , {} {}", ptr, value, ordering)
    }

    /// LLVM atomic RMW sub
    pub fn atomic_rmw_sub(ptr: &str, value: &str, ordering: &str) -> String {
        format!("llvm.atomicrmw sub {} , {} {}", ptr, value, ordering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arith_addi() {
        let op = arith::addi("%0", "%1", "i64");
        assert_eq!(op, "arith.addi %0, %1 : i64");
    }

    #[test]
    fn test_memref_load() {
        let op = memref::load("%ptr", &[], "memref<i64>");
        assert_eq!(op, "memref.load %ptr[] : memref<i64>");
    }

    #[test]
    fn test_cf_br() {
        let op = cf::br("^bb1");
        assert_eq!(op, "cf.br ^bb1");
    }
}
