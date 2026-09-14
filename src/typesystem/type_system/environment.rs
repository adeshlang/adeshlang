//! Type environment management for variable lookups and updates.

use crate::typesystem::checker::Ty;
use crate::utils::collections::FastMap;

pub(crate) fn lookup_var(env: &Vec<FastMap<String, Ty>>, name: &str) -> Option<Ty> {
    for scope in env.iter().rev() {
        if let Some(t) = scope.get(name) {
            return Some(t.clone());
        }
    }
    None
}

pub(crate) fn set_var(env: &mut Vec<FastMap<String, Ty>>, name: &str, t: Ty) {
    for scope in env.iter_mut().rev() {
        if scope.contains_key(name) {
            scope.insert(name.to_string(), t);
            return;
        }
    }
    // if not found, set in current scope
    if let Some(scope) = env.last_mut() {
        scope.insert(name.to_string(), t);
    }
}
