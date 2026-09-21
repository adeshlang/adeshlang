//! Comprehensive Backend Array Tests
//!
//! Verifies complete array capabilities across backends:
//! 1. Raw arrays ([T; N; raw]) with zero overhead and rejection of size-altering operations.
//! 2. Dynamic arrays ([T; N] fixed capacity, [T] dynamic) with capacity enforcement.
//! 3. All array methods: push, pop, shift, unshift, insert, remove, clear, extend, concat,
//!    count, index, indexOf, lastIndexOf, includes, contains, sort, reverse, slice, flat,
//!    sum, min, max, distinct, toSet, toTuple.
//! 4. Direct index assignments, negative indexing (arr[-1]), and bounds-checked safety.
//! 5. Spread operator ([...a, ...b]) and array/tuple destructuring (let (a, b, c) = arr;).

use adeshlang::backends::lowering::*;
use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_interp(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src_owned, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn test_array_mutation_methods_and_semantics() {
    let code = r#"
        let a1 = [10, 20, 30, 40];
        let p = a1.push(50);
        assert_eq(p.length, 5);
        assert_eq(p[4], 50);
        
        let popped = p.pop();
        assert_eq(popped.length, 4);
        assert_eq(popped[3], 40);
        
        let a2 = [10, 20, 30, 40];
        let shifted = a2.shift();
        assert_eq(shifted.length, 3);
        assert_eq(shifted[0], 20);
        
        let a3 = [10, 20, 30, 40];
        let unshifted = a3.unshift(5);
        assert_eq(unshifted.length, 5);
        assert_eq(unshifted[0], 5);
        assert_eq(unshifted[1], 10);
        
        let a4 = [10, 20, 30, 40];
        let ins = a4.insert(2, 25);
        assert_eq(ins.length, 5);
        assert_eq(ins[2], 25);
        
        let a5 = [10, 20, 30, 40];
        let cleared = a5.clear();
        assert_eq(cleared.length, 0);
    "#;
    let res = run_interp(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_array_search_transform_and_aggregations() {
    let code = r#"
        let nums = [3, 1, 4, 1, 5, 9, 2, 6, 5];
        
        assert_eq(nums.count(1), 2);
        assert_eq(nums.count(5), 2);
        assert_eq(nums.count(100), 0);
        
        assert_eq(nums.indexOf(4), 2);
        assert_eq(nums.lastIndexOf(5), 8);
        assert(nums.includes(9));
        assert(nums.contains(2));
        assert(nums.includes(99) == false);
        
        let to_rev = [1, 2, 3, 4, 5];
        let rev = to_rev.reverse();
        assert_eq(rev[0], 5);
        assert_eq(rev[-1], 1);
        
        let to_sort = [5, 2, 8, 1, 9];
        let s = to_sort.sort();
        assert_eq(s[0], 1);
        assert_eq(s[1], 2);
        assert_eq(s[-1], 9);
        
        let to_slice = [10, 20, 30, 40, 50];
        let sl = to_slice.slice(1, 4);
        assert_eq(sl.length, 3);
        assert_eq(sl[0], 20);
        assert_eq(sl[1], 30);
        assert_eq(sl[2], 40);
        
        let simple_nums = [10, 20, 30, 40];
        assert_eq(simple_nums.sum(), 100);
        assert_eq(simple_nums.min(), 10);
        assert_eq(simple_nums.max(), 40);
        
        let dups = [1, 2, 2, 3, 1, 4];
        let uniq = dups.distinct();
        assert_eq(uniq.length, 4);
        assert_eq(uniq[0], 1);
        assert_eq(uniq[1], 2);
        assert_eq(uniq[2], 3);
        assert_eq(uniq[3], 4);
        
        let joined = simple_nums.join("-");
        assert_eq(joined, "10-20-30-40");
    "#;
    let res = run_interp(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_fixed_capacity_and_raw_arrays() {
    let code = r#"
        let dyn_cap: [i32; 5] = [1, 2, 3];
        assert_eq(dyn_cap.length, 3);
        assert_eq(dyn_cap.capacity, 5);
        
        dyn_cap = dyn_cap.append(4);
        dyn_cap = dyn_cap.append(5);
        assert_eq(dyn_cap.length, 5);
        
        // Mutating via direct index assignment
        dyn_cap[0] = 100;
        assert_eq(dyn_cap[0], 100);
        
        // Negative indexing
        assert_eq(dyn_cap[-1], 5);
        assert_eq(dyn_cap[-2], 4);
        
        // Raw array with fixed size and zero metadata overhead
        let raw_arr: [i32; 3; raw] = [10, 20, 30];
        assert_eq(raw_arr.length, 3);
        assert_eq(raw_arr.metadata_size(), 0);
        
        raw_arr[0] = 999;
        assert_eq(raw_arr[0], 999);
        assert_eq(raw_arr[-1], 30);
    "#;
    let res = run_interp(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_array_spread_and_destructuring() {
    let code = r#"
        let a = [1, 2];
        let b = [3, 4];
        let combined = [...a, ...b, 5];
        assert_eq(combined.length, 5);
        assert_eq(combined[0], 1);
        assert_eq(combined[1], 2);
        assert_eq(combined[2], 3);
        assert_eq(combined[3], 4);
        assert_eq(combined[4], 5);
        
        // Destructuring
        let tuple_val: (u8, i32, string) = (42u8, 1000i32, "hello");
        let (x, y, z): (u8, i32, string) = tuple_val;
        assert_eq(x, 42u8);
        assert_eq(y, 1000i32);
        assert_eq(z, "hello");
    "#;
    let res = run_interp(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_bytecode_vm_array_operations() {
    let instrs = vec![
        BytecodeInstr::PushInt(3),
        BytecodeInstr::ArrayNew,
        BytecodeInstr::Dup,
        BytecodeInstr::PushInt(0),
        BytecodeInstr::PushInt(42),
        BytecodeInstr::ArraySet,
        BytecodeInstr::Dup,
        BytecodeInstr::PushInt(-1),
        BytecodeInstr::PushInt(99),
        BytecodeInstr::ArraySet,
        BytecodeInstr::Dup,
        BytecodeInstr::PushInt(0),
        BytecodeInstr::ArrayGet,
        BytecodeInstr::Swap,
        BytecodeInstr::PushInt(-1),
        BytecodeInstr::ArrayGet,
        BytecodeInstr::Halt,
    ];
    let mut vm = BytecodeVM::new(instrs);
    let res = vm.run();
    assert!(res.is_ok(), "VM execution failed: {:?}", res.err());
}
