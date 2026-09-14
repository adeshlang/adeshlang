//! Function expression evaluation (Fn keyword)

use crate::parsing::ast::{Expr, Stmt, UserFn, Value};
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

use super::super::core::Exec;

impl Exec {
    pub(super) fn eval_fn(
        &mut self,
        params: &[(String, Option<Expr>, Option<String>)],
        body: &Arc<Vec<Stmt>>,
        is_async: bool,
    ) -> Result<Value, String> {
        // Eager capture: spawned threads run via a fresh Exec, so a closure
        // index into this Exec's env table is not valid on the worker.
        let mut captured: HashMap<String, Value> = HashMap::default();
        let mut chain: Vec<usize> = Vec::new();
        let mut c = Some(self.current);
        while let Some(id) = c {
            chain.push(id);
            c = self.envs[id].enclosing;
        }
        for id in chain.iter().rev() {
            for (k, v) in &self.envs[*id].values {
                captured.insert(k.clone(), v.clone());
            }
        }
        Ok(Value::UserFunction(UserFn {
            name: "<anon>".into(),
            type_params: Vec::new(),
            params: params.to_vec(),
            body: body.clone(),
            closure: 0,
            visibility: None,
            ret_type: None,
            is_async,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_getter: false,
            is_setter: false,
            is_operator: false,
            operator_symbol: None,
            captured: Some(captured),
            defining_class: None,
            is_unsafe: false,
        }))
    }
}
