//! Promise and microtask processing for the tiered JIT.
//!
//! Handles async execution, promise settlement, and microtask queue processing.

use crate::backends::builtins::{Microtask, PROMISE_RUNTIME, PromiseState, RuntimeValue};
use crate::backends::jit::tiered::TieredJitContext;

impl TieredJitContext {
    /// Process pending microtasks from the promise runtime
    pub(in crate::backends::jit::tiered) fn process_microtasks(&mut self) -> Result<(), String> {
        let max_iterations = 1000;
        let mut iterations = 0;

        while let Some(task) = PROMISE_RUNTIME.pop_microtask() {
            iterations += 1;
            if iterations > max_iterations {
                break;
            }

            match task {
                Microtask::SettleFulfill(id, value) => {
                    PROMISE_RUNTIME.fulfill(id, value);
                }
                Microtask::SettleReject(id, reason) => {
                    PROMISE_RUNTIME.reject(id, reason);
                }
                Microtask::CallHandler {
                    handler,
                    arg,
                    downstream_id,
                    is_rejection: _,
                } => {
                    if let RuntimeValue::Function(func) = handler.as_ref() {
                        if let Some(target_func) = self.functions.get(&func.name).cloned() {
                            match super::execution::execute_function_with_captures(
                                self,
                                &target_func,
                                vec![(*arg).clone()],
                                &func.captures,
                            ) {
                                Ok(result) => {
                                    PROMISE_RUNTIME.fulfill_value(downstream_id, result);
                                }
                                Err(e) => {
                                    PROMISE_RUNTIME
                                        .reject_value(downstream_id, RuntimeValue::String(e));
                                }
                            }
                        } else {
                            PROMISE_RUNTIME.fulfill_value(downstream_id, (*arg).clone());
                        }
                    } else {
                        PROMISE_RUNTIME.fulfill_value(downstream_id, (*arg).clone());
                    }
                }
                Microtask::Timer {
                    callback,
                    promise_id,
                } => {
                    if let Some(target_func) = self.functions.get(&callback.name).cloned() {
                        match super::execution::execute_function_with_captures(
                            self,
                            &target_func,
                            vec![],
                            &callback.captures,
                        ) {
                            Ok(result) => {
                                PROMISE_RUNTIME.fulfill_value(promise_id, result);
                            }
                            Err(e) => {
                                PROMISE_RUNTIME.reject_value(promise_id, RuntimeValue::String(e));
                            }
                        }
                    } else {
                        PROMISE_RUNTIME.fulfill_value(promise_id, RuntimeValue::Null);
                    }
                }
                Microtask::PromiseAllCheck { result_id } => {
                    if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                        let mut results: Vec<RuntimeValue> = Vec::with_capacity(pids.len());
                        let mut all_fulfilled = true;
                        let mut any_rejected = false;
                        let mut reject_reason = RuntimeValue::Null;

                        for pid in pids.iter() {
                            if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                                match state {
                                    PromiseState::Fulfilled(value) => {
                                        results.push((*value).clone());
                                    }
                                    PromiseState::Rejected(reason) => {
                                        any_rejected = true;
                                        reject_reason = (*reason).clone();
                                        break;
                                    }
                                    PromiseState::Pending => {
                                        all_fulfilled = false;
                                    }
                                }
                            } else {
                                all_fulfilled = false;
                            }
                        }

                        if any_rejected {
                            PROMISE_RUNTIME.reject_value(result_id, reject_reason);
                        } else if all_fulfilled {
                            PROMISE_RUNTIME.fulfill_value(result_id, RuntimeValue::Array(results));
                        }
                    }
                }
                Microtask::PromiseRaceCheck { result_id } => {
                    if PROMISE_RUNTIME.is_settled(result_id) {
                        continue;
                    }
                    if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                        for pid in pids.iter() {
                            if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                                match state {
                                    PromiseState::Fulfilled(value) => {
                                        PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                                        break;
                                    }
                                    PromiseState::Rejected(reason) => {
                                        PROMISE_RUNTIME.reject_value(result_id, (*reason).clone());
                                        break;
                                    }
                                    PromiseState::Pending => {}
                                }
                            }
                        }
                    }
                }
                Microtask::PromiseAnyCheck { result_id } => {
                    if PROMISE_RUNTIME.is_settled(result_id) {
                        continue;
                    }
                    if let Some(pids) = PROMISE_RUNTIME.get_aggregate_list(result_id) {
                        let mut all_rejected = true;
                        let mut reject_reasons = vec![];

                        for pid in pids.iter() {
                            if let Some(state) = PROMISE_RUNTIME.get_state(*pid) {
                                match state {
                                    PromiseState::Fulfilled(value) => {
                                        PROMISE_RUNTIME.fulfill_value(result_id, (*value).clone());
                                        all_rejected = false;
                                        break;
                                    }
                                    PromiseState::Rejected(reason) => {
                                        reject_reasons.push((*reason).clone());
                                    }
                                    PromiseState::Pending => {
                                        all_rejected = false;
                                    }
                                }
                            } else {
                                all_rejected = false;
                            }
                        }

                        if all_rejected && !reject_reasons.is_empty() {
                            PROMISE_RUNTIME
                                .reject_value(result_id, RuntimeValue::Array(reject_reasons));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
