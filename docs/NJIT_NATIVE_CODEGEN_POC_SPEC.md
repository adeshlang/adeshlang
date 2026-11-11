# AdeshLang NJIT Native Codegen: End-to-End Proof-of-Concept (POC) Specification

**Status:** Ready for Implementation  
**Target:** Minimal-Risk Vertical Slice Prototype  
**Test File:** `tests/njit_native_codegen_poc_test.rs`

---

## 1. Goal & Scope

The goal of this proof-of-concept is to prove that AdeshLang's **Native Codegen (`adesh-codegen`)** and an in-memory page loader can compile and execute native machine code in-process without any reliance on Cranelift.

The POC will validate:
1. Translating an Adesh AST/HIR function into `Machine IR`.
2. Emitting machine instructions and `AdobRelocation` records using `X86_64Encoder`.
3. Allocating memory, writing code, applying relocations in place, and enforcing $W \oplus X$ (Read-Execute).
4. Calling the compiled function pointer and asserting the return value.

---

## 2. Test Cases & Progression

### Test 1: Immediate Value Return (Base Case)
```adesh
fn main() -> int {
    return 42;
}
```
- **Validation:**
  - Machine IR has one block emitting:
    - Win64/SysV: `mov eax, 42` (or `mov rax, 42`)
    - `ret`
  - Zero relocations needed.
  - Returns `42`.

### Test 2: Internal Function Call (Inter-Function Relocation)
```adesh
fn helper(x: int, y: int) -> int {
    return x + y;
}

fn main() -> int {
    return helper(20, 22);
}
```
- **Validation:**
  - `helper` and `main` compiled into a single contiguous code buffer.
  - Call from `main` to `helper` produces an `AdobRelocation` of kind `RelocationKind::PcRelative32` with addend `-4`.
  - In-memory relocation calculation:
    $$\text{disp32} = (\text{helper\_offset} - (\text{call\_site\_offset} + 4)) \text{ as i32}$$
  - Displacement written to memory.
  - Invoking `main` returns `42`.

### Test 3: External Runtime Call (Host Symbol Resolution)
```adesh
fn main() -> int {
    print_i64(1337);
    return 0;
}
```
- **Validation:**
  - Call to external C symbol `print_i64`.
  - Symbol resolved via host symbol table (`print_i64 as *const u8`).
  - Relocation patched (direct relative displacement if within 2GB, or via a 64-bit jump trampoline if $>2\text{GB}$).
  - Invoking `main` prints `1337` to stdout and returns `0`.

---

## 3. Concrete Implementation Structure for the POC

```rust
// tests/njit_native_codegen_poc_test.rs

use adesh_codegen::backend::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister,
};
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::{Architecture, OperatingSystem, RelocationKind, TargetDescriptor};

#[test]
fn test_poc_native_codegen_jit_execution() {
    // 1. Build a target descriptor for host
    #[cfg(target_os = "windows")]
    let os = OperatingSystem::Windows;
    #[cfg(target_os = "linux")]
    let os = OperatingSystem::Linux;
    #[cfg(target_os = "macos")]
    let os = OperatingSystem::MacOS;

    let target = TargetDescriptor {
        architecture: Architecture::X86_64,
        operating_system: os,
        environment: None,
    };

    // 2. Build a Machine IR function: main() -> returns 42
    let mut module = NativeModule::new("poc_module");
    let mut func = MachineFunction::new("main");
    func.is_exported = true;

    let entry = func.entry_block_mut();
    // mov rax, 42
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    // ret
    entry.push(MachineInstruction::Return);
    module.add_function(func);

    // 3. Compile to machine code and relocations
    let mut backend = X86_64Backend::new(target);
    let (code, relocs) = backend
        .generate_function_with_relocations(&module.functions[0])
        .expect("codegen failed");

    assert!(!code.is_empty(), "Code buffer must not be empty");

    // 4. Allocate Executable Memory (W^X Lifecycle)
    unsafe {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Memory::*;
            let page_size = 4096;
            let mem = VirtualAlloc(
                std::ptr::null_mut(),
                page_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            ) as *mut u8;
            assert!(!mem.is_null());

            // Copy code into allocated memory
            std::ptr::copy_nonoverlapping(code.as_ptr(), mem, code.len());

            // (Apply any relocations here)

            // Seal to RX
            let mut old_protect = 0;
            let res = VirtualProtect(mem as _, page_size, PAGE_EXECUTE_READ, &mut old_protect);
            assert_ne!(res, 0);

            // Transmute and invoke
            let func: extern "C" fn() -> i64 = std::mem::transmute(mem);
            let result = func();
            assert_eq!(result, 42);

            // Clean up
            VirtualFree(mem as _, 0, MEM_RELEASE);
        }
    }
}
```

---

## 4. Acceptance Criteria

1. The test passes on Windows and Linux x86-64 without compiler warnings or sanitizer errors.
2. No `PAGE_EXECUTE_READWRITE` (RWX) permissions are used; memory transitions strictly from RW $\to$ RX.
3. Execution does not crash, leak memory, or corrupt the calling thread's register state.
4. Return values match expected outputs across integer, boolean, and inter-function calls.
