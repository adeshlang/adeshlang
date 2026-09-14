use crate::parsing::ast::Value;

#[derive(Clone)]
pub enum Flow {
    Next,
    Return(Option<Value>),
    Break,
    Continue,
    Jump(f64),
    /// Tail call optimization: instead of recursing, restart with new args.
    /// Contains (function_name, new_args) for the trampoline to pick up.
    TailCall(String, Vec<Value>),
}

#[derive(Clone)]
pub enum ExecFlow {
    Next,
    Return(Option<Value>),
    Break,
    Continue,
    Jump(f64),
}
