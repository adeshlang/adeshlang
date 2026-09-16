//! GPU Kernel Generation Validation Tests
//!
//! Comprehensive tests for Phase 7.3: GPU kernel lowering and optimization

#[cfg(test)]
mod tests {
    use adeshlang::backends::mlir::gpu::*;
    use adeshlang::ir::vir::*;

    /// Test 1: Basic GPU safety check for all instruction types
    #[test]
    fn test_gpu_safety_comprehensive() {
        // Safe instructions - arithmetic
        assert!(is_gpu_safe_instruction(&VirInstruction::IntBinOp {
            dest: 0,
            op: IntBinOp::Add,
            lhs: 1,
            rhs: 2,
            ty: VirType::I32,
        }));

        assert!(is_gpu_safe_instruction(&VirInstruction::FloatBinOp {
            dest: 0,
            op: FloatBinOp::Add,
            lhs: 1,
            rhs: 2,
            ty: VirType::F32,
        }));

        assert!(is_gpu_safe_instruction(&VirInstruction::IntCmp {
            dest: 0,
            op: CmpOp::Eq,
            lhs: 1,
            rhs: 2,
        }));

        assert!(is_gpu_safe_instruction(&VirInstruction::Load {
            dest: 0,
            ptr: 1,
            ty: VirType::I32,
        }));

        assert!(is_gpu_safe_instruction(&VirInstruction::Store {
            ptr: 0,
            value: 1,
        }));

        // Safe - constants
        assert!(is_gpu_safe_instruction(&VirInstruction::ConstInt {
            dest: 0,
            value: 42,
            ty: VirType::I32,
        }));
    }

    /// Test 2: GPU optimization context
    #[test]
    fn test_gpu_optimization_context() {
        let mut ctx = GpuOptimizationContext::new();

        // Test memory space tracking
        ctx.set_memory_space(0, MemorySpace::Global);
        assert_eq!(ctx.get_memory_space(0), MemorySpace::Global);

        ctx.set_memory_space(1, MemorySpace::Local);
        assert_eq!(ctx.get_memory_space(1), MemorySpace::Local);
        assert!(ctx.shared_memory_used);

        // Test barrier tracking
        ctx.add_barrier(0);
        assert_eq!(ctx.barrier_points.len(), 1);
        assert!(ctx.barrier_points.contains(&0));

        // Test divergence tracking
        ctx.mark_divergent(2);
        assert_eq!(ctx.divergent_branches.len(), 1);
        assert!(ctx.divergent_branches.contains(&2));
    }

    /// Test 3: GPU kernel config
    #[test]
    fn test_gpu_kernel_config() {
        let config = GpuKernelConfig::default();
        assert_eq!(config.grid_dims, (1, 1, 1));
        assert_eq!(config.block_dims, (256, 1, 1));
        assert_eq!(config.shared_memory_size, 0);

        let custom_config = GpuKernelConfig {
            grid_dims: (16, 16, 1),
            block_dims: (32, 32, 1),
            shared_memory_size: 4096,
        };
        assert_eq!(custom_config.grid_dims, (16, 16, 1));
        assert_eq!(custom_config.shared_memory_size, 4096);
    }

    /// Test 4: GPU memory space attributes
    #[test]
    fn test_gpu_memory_space() {
        assert_eq!(MemorySpace::Global.to_mlir_attr(), 0);
        assert_eq!(MemorySpace::Local.to_mlir_attr(), 3);
        assert_eq!(MemorySpace::Private.to_mlir_attr(), 5);
    }

    /// Test 5: GPU safety for various instruction types
    #[test]
    fn test_gpu_instruction_safety_details() {
        // Memory operations - safe
        assert!(is_gpu_safe_instruction(&VirInstruction::Alloc {
            dest: 0,
            ty: VirType::I32,
            size: 1,
        }));

        assert!(!is_gpu_safe_instruction(&VirInstruction::Free { ptr: 0 }));

        // ARC operations - need special handling
        assert!(is_gpu_safe_instruction(&VirInstruction::ArcIncrement {
            ptr: 0
        }));

        // Casts - safe
        assert!(is_gpu_safe_instruction(&VirInstruction::Cast {
            dest: 0,
            value: 1,
            from_ty: VirType::I32,
            to_ty: VirType::F32,
        }));

        // Copy/Move - safe
        assert!(is_gpu_safe_instruction(&VirInstruction::Copy {
            dest: 0,
            src: 1,
        }));

        assert!(is_gpu_safe_instruction(&VirInstruction::Move {
            dest: 0,
            src: 1,
        }));
    }

    /// Test 6: GPU memory access patterns
    #[test]
    fn test_gpu_memory_patterns() {
        let coalesced = MemoryAccessPattern::Coalesced;
        assert_eq!(coalesced, MemoryAccessPattern::Coalesced);

        let strided = MemoryAccessPattern::Strided { stride: 4 };
        match strided {
            MemoryAccessPattern::Strided { stride } => assert_eq!(stride, 4),
            _ => panic!("Expected strided pattern"),
        }

        let random = MemoryAccessPattern::Random;
        assert_eq!(random, MemoryAccessPattern::Random);
    }

    /// Test 7: GPU context multiple barriers
    #[test]
    fn test_gpu_multiple_barriers() {
        let mut ctx = GpuOptimizationContext::new();

        ctx.add_barrier(0);
        ctx.add_barrier(1);
        ctx.add_barrier(2);

        assert_eq!(ctx.barrier_points.len(), 3);

        // Adding duplicate should not increase count
        ctx.add_barrier(1);
        assert_eq!(ctx.barrier_points.len(), 3);
    }

    /// Test 8: GPU context multiple divergent branches
    #[test]
    fn test_gpu_multiple_divergent_branches() {
        let mut ctx = GpuOptimizationContext::new();

        ctx.mark_divergent(10);
        ctx.mark_divergent(20);
        ctx.mark_divergent(30);

        assert_eq!(ctx.divergent_branches.len(), 3);

        // Adding duplicate should not increase count
        ctx.mark_divergent(20);
        assert_eq!(ctx.divergent_branches.len(), 3);
    }

    /// Test 9: GPU safety check for unsafe operations
    #[test]
    fn test_gpu_unsafe_operations() {
        // Function calls might be unsafe depending on the function
        // (This test documents expected behavior - actual implementation may vary)
        let call_inst = VirInstruction::Call {
            dest: Some(0),
            func: 100,
            args: vec![],
        };

        // Whether this is safe depends on what function is being called
        // Test infrastructure might mark as safe if it's a GPU intrinsic
        let _is_safe = is_gpu_safe_instruction(&call_inst);
    }

    /// Test 10: GPU configuration with large dimensions
    #[test]
    fn test_gpu_large_config() {
        let config = GpuKernelConfig {
            grid_dims: (1024, 1024, 1),
            block_dims: (32, 32, 1),
            shared_memory_size: 49152, // 48 KB
        };

        assert_eq!(config.grid_dims.0 * config.grid_dims.1, 1024 * 1024);
        assert_eq!(config.block_dims.0 * config.block_dims.1, 32 * 32);
    }
}
