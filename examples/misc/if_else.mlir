module attributes {gpu.container_module} {
  llvm.mlir.global internal constant @str_0("__top_level_wrapper") : !llvm.array<19 x i8>

  // ARC Runtime Functions
  func.func private @arc_retain(!llvm.ptr)
  func.func private @arc_release(!llvm.ptr)
  func.func private @arc_clone(!llvm.ptr) -> !llvm.ptr

  // libc Memory Functions
  func.func private @free(!llvm.ptr)
  func.func private @malloc(i64) -> !llvm.ptr
  func.func private @memcpy(!llvm.ptr, !llvm.ptr, i64)
  func.func private @memmove(!llvm.ptr, !llvm.ptr, i64)
  func.func private @memset(!llvm.ptr, i32, i64)

  func.func @__top_level_wrapper() -> () {
    %local_0 = memref.alloca() : memref<1xi64>

    %0 = arith.constant 12 : i64
    return
  }

  func.func @main() -> () {

    %1 = llvm.mlir.addressof @str_0 : !llvm.ptr
    %0 = llvm.call %1() : !llvm.ptr, () -> i64
    return
  }

  gpu.module @__top_level_wrapper_gpu {
    gpu.func @__top_level_wrapper_kernel() kernel {
      gpu.return
    }
  }

  func.func @__top_level_wrapper_gpu_launch() -> () {
    %gx = arith.constant 1 : index
    %gy = arith.constant 1 : index
    %gz = arith.constant 1 : index
    %bx = arith.constant 256 : index
    %by = arith.constant 1 : index
    %bz = arith.constant 1 : index
    gpu.launch_func @__top_level_wrapper_gpu::@__top_level_wrapper_kernel blocks in (%gx, %gy, %gz) threads in (%bx, %by, %bz) args()
    // target: auto
    return
  }

  gpu.module @main_gpu {
    gpu.func @main_kernel() kernel {
      gpu.return
    }
  }

  func.func @main_gpu_launch() -> () {
    %gx = arith.constant 1 : index
    %gy = arith.constant 1 : index
    %gz = arith.constant 1 : index
    %bx = arith.constant 256 : index
    %by = arith.constant 1 : index
    %bz = arith.constant 1 : index
    gpu.launch_func @main_gpu::@main_kernel blocks in (%gx, %gy, %gz) threads in (%bx, %by, %bz) args()
    // target: auto
    return
  }

}
