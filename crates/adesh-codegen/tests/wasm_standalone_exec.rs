//! Execute Adesh-generated relocatable objects after standalone linking.
use adesh_codegen::machine_ir::{
    ConditionCode, MachineInstruction as I, MachineOperand as O, MachineRegister as R,
};
use adesh_codegen::targets::wasm::standalone::{
    CallSite, Function, Import, Limits, Module, Signature, ValueType as T,
};
use adesh_linker::{LinkConfig, Linker, Target};
use std::process::Command;

fn reg(r: adesh_codegen::machine_ir::VirtualRegister) -> O {
    O::Register(R::Virtual(r))
}

fn link(modules: &[Module], script: &str) {
    let dir = tempfile::tempdir().unwrap();
    let objects: Vec<_> = modules
        .iter()
        .enumerate()
        .map(|(i, module)| {
            let path = dir.path().join(format!("{i}.o"));
            std::fs::write(&path, module.emit_object(Limits::default()).unwrap()).unwrap();
            path
        })
        .collect();
    let output = dir.path().join("program.wasm");
    let config = LinkConfig::new(
        output.clone(),
        Target::from_triple("wasm32-unknown-unknown").unwrap(),
    );
    Linker::link(&objects, config).unwrap();
    let js = format!(
        "const fs=require('fs'),assert=require('assert'); const instance=new WebAssembly.Instance(new WebAssembly.Module(fs.readFileSync(process.argv[1])),{{}}); const e=instance.exports; {script}"
    );
    let result = Command::new("node")
        .arg("-e")
        .arg(js)
        .arg(output)
        .output()
        .expect("Node is required");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn typed_parameters_returns_cross_object_calls_and_recursion_execute() {
    let mut twice = Function::new("twice");
    twice.body.is_exported = true;
    let x = twice.local(T::I64);
    twice.parameters.push(x);
    twice.result = Some(x);
    twice.body.blocks[0].push(I::Add {
        dst: reg(x),
        src: reg(x),
    });
    twice.body.blocks[0].push(I::Return);
    let mut caller = Function::new("calculate");
    caller.body.is_exported = true;
    let x = caller.local(T::I64);
    caller.parameters.push(x);
    let answer = caller.local(T::I64);
    caller.result = Some(answer);
    caller.calls.insert(
        (0, 0),
        CallSite {
            arguments: vec![reg(x)],
            result: Some(answer),
        },
    );
    caller.body.blocks[0].push(I::Call {
        target: O::Symbol("twice".into()),
        num_args: 1,
    });
    caller.body.blocks[0].push(I::Add {
        dst: reg(answer),
        src: O::Immediate(1),
    });
    caller.body.blocks[0].push(I::Return);
    let mut m = Module::default();
    m.functions.push(caller);
    m.imports.insert(
        "twice".into(),
        Import {
            module: "env".into(),
            name: "twice".into(),
            signature: Signature {
                params: vec![T::I64],
                result: Some(T::I64),
            },
        },
    );
    let mut other = Module::default();
    other.functions.push(twice);
    link(
        &[m, other],
        "assert.equal(e.calculate(20n),41n); assert.equal(e.calculate(-5n),-9n);",
    );
}

#[test]
fn backward_branches_and_runtime_parameters_execute() {
    let mut f = Function::new("sum");
    f.body.is_exported = true;
    let n = f.local(T::I64);
    f.parameters.push(n);
    let total = f.local(T::I64);
    f.result = Some(total);
    f.body.create_block("test");
    f.body.create_block("body");
    f.body.create_block("done");
    f.body.blocks[0].push(I::Branch {
        target: "test".into(),
    });
    f.body.blocks[1].push(I::Compare {
        lhs: reg(n),
        rhs: O::Immediate(0),
    });
    f.body.blocks[1].push(I::BranchCc {
        cc: ConditionCode::LessOrEqual,
        target: "done".into(),
    });
    f.body.blocks[1].push(I::Branch {
        target: "body".into(),
    });
    f.body.blocks[2].push(I::Add {
        dst: reg(total),
        src: reg(n),
    });
    f.body.blocks[2].push(I::Sub {
        dst: reg(n),
        src: O::Immediate(1),
    });
    f.body.blocks[2].push(I::Branch {
        target: "test".into(),
    });
    f.body.blocks[3].push(I::Return);
    let m = Module {
        functions: vec![f],
        ..Module::default()
    };
    link(
        &[m],
        "assert.equal(e.sum(100n),5050n); assert.equal(e.sum(0n),0n); assert.equal(e.sum(-3n),0n);",
    );
}

#[test]
fn float_widths_memory_relocations_and_engine_bounds_checks_execute() {
    let mut f = Function::new("multiply");
    f.body.is_exported = true;
    let x = f.local(T::F32);
    let y = f.local(T::F32);
    f.parameters.extend([x, y]);
    f.result = Some(x);
    f.body.blocks[0].push(I::FMul {
        dst: reg(x),
        src: reg(y),
        size: 4,
    });
    f.body.blocks[0].push(I::Return);
    let mut read = Function::new("read");
    read.body.is_exported = true;
    let ptr = read.local(T::I32);
    let value = read.local(T::I64);
    read.result = Some(value);
    read.body.blocks[0].push(I::Move {
        dst: reg(ptr),
        src: O::Symbol("constant".into()),
    });
    read.body.blocks[0].push(I::Load {
        dst: reg(value),
        src: reg(ptr),
        size: 8,
    });
    read.body.blocks[0].push(I::Return);
    let mut load = Function::new("load");
    load.body.is_exported = true;
    let ptr = load.local(T::I32);
    load.parameters.push(ptr);
    let value = load.local(T::I64);
    load.result = Some(value);
    load.body.blocks[0].push(I::Load {
        dst: reg(value),
        src: reg(ptr),
        size: 8,
    });
    load.body.blocks[0].push(I::Return);
    let m = Module {
        functions: vec![f, read, load],
        data: vec![("constant".into(), 123456789i64.to_le_bytes().to_vec())],
        ..Module::default()
    };
    link(
        &[m],
        "assert.equal(e.multiply(1.25,2.5),3.125); assert.equal(e.read(),123456789n); assert.throws(()=>e.load(-4),WebAssembly.RuntimeError);",
    );
}

#[test]
fn malformed_ir_and_resource_limits_are_errors_not_panics() {
    let mut f = Function::new("bad");
    f.body.blocks[0].push(I::Branch {
        target: "missing".into(),
    });
    let mut m = Module {
        functions: vec![f],
        ..Module::default()
    };
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("unknown block")
    );
    m.functions[0].body.blocks[0].instructions.clear();
    m.functions[0].body.blocks[0].push(I::Call {
        target: O::Symbol("missing".into()),
        num_args: 0,
    });
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("explicit arguments")
    );
    assert!(
        m.emit_object(Limits {
            functions: 0,
            ..Limits::default()
        })
        .is_err()
    );
    assert!(
        m.emit_object(Limits {
            blocks: 0,
            ..Limits::default()
        })
        .is_err()
    );
    assert!(
        m.emit_object(Limits {
            object_bytes: 16,
            ..Limits::default()
        })
        .is_err()
    );
}

#[test]
fn more_than_127_locals_use_multibyte_indices_and_output_is_deterministic() {
    let mut f = Function::new("many");
    f.body.is_exported = true;
    let mut last = None;
    for i in 0..300 {
        let r = f.local(T::I64);
        f.body.blocks[0].push(I::Move {
            dst: reg(r),
            src: O::Immediate(i),
        });
        last = Some(r);
    }
    f.result = last;
    f.body.blocks[0].push(I::Return);
    let m = Module {
        functions: vec![f],
        ..Module::default()
    };
    assert_eq!(
        m.emit_object(Limits::default()).unwrap(),
        m.emit_object(Limits::default()).unwrap()
    );
    link(&[m], "assert.equal(e.many(),299n);");
}

#[test]
fn rejects_bad_registers_widths_calls_and_bulk_data_before_emission() {
    let mut f = Function::new("invalid");
    let a = f.local(T::F32);
    let b = f.local(T::I32);
    f.body.blocks[0].push(I::FCmp {
        lhs: reg(a),
        rhs: reg(a),
        size: 8,
    });
    f.body.blocks[0].push(I::SetCc {
        dst: reg(b),
        cc: ConditionCode::Equal,
    });
    let mut m = Module {
        functions: vec![f],
        ..Module::default()
    };
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("width mismatch")
    );
    m.functions[0].body.blocks[0].instructions.clear();
    m.functions[0].result = Some(adesh_codegen::machine_ir::VirtualRegister(999));
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("undeclared register")
    );
    m.functions[0].result = None;
    m.functions[0].parameters = vec![a, a];
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("duplicate parameter")
    );
    m.functions[0].parameters.clear();
    m.functions[0].body.blocks[0].push(I::Return);
    m.functions[0].calls.insert(
        (0, 10),
        CallSite {
            arguments: vec![],
            result: None,
        },
    );
    assert!(
        m.emit_object(Limits::default())
            .unwrap_err()
            .contains("orphan")
    );
    m.functions[0].calls.clear();
    m.data.push(("large".into(), vec![0; 4096]));
    assert!(
        m.emit_object(Limits {
            object_bytes: 4096,
            ..Limits::default()
        })
        .is_err()
    );
    assert!(
        m.emit_object(Limits {
            locals: 1,
            ..Limits::default()
        })
        .is_err()
    );
}
