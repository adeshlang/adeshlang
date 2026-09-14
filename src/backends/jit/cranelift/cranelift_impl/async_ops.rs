//! Async and Promise handling for JIT execution

use crate::backends::jit::builtins::{Microtask, PROMISE_RUNTIME, PromiseState, RuntimeValue};
use crate::backends::jit::cranelift::JitContext;
use crate::backends::jit::lir::LirFunction;
use crate::utils::collections::FastMap;
use std::sync::Arc;

impl JitContext {
    /// Execute an async function and return a Promise
    pub(in crate::backends::jit::cranelift) fn call_async_function(
        &mut self,
        func: &LirFunction,
        args: Vec<RuntimeValue>,
        captures: &FastMap<String, RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        // Create a new Promise for this async function
        let promise_id = PROMISE_RUNTIME.create_promise();

        // Execute the function body and capture its result
        // Process microtasks periodically during execution to allow inner awaits to work
        match self.execute_function_with_captures(func, args, captures) {
            Ok(result) => {
                // Check if an exception was thrown inside the async function
                // (when throw is used without try/catch)
                if let Some(exc) = self.check_and_clear_exception() {
                    // Reject the promise with the exception
                    PROMISE_RUNTIME.reject_value(promise_id, exc);
                } else {
                    // Function completed successfully
                    // Process any remaining microtasks before fulfilling
                    let _ = self.process_microtasks();
                    // Fulfill the promise with the result
                    PROMISE_RUNTIME.fulfill_value(promise_id, result);
                }
            }
            Err(err) => {
                // Function threw an error - reject the promise
                PROMISE_RUNTIME.reject_value(promise_id, RuntimeValue::String(err));
            }
        }

        // Process microtasks one more time to settle the async function's promise
        let _ = self.process_microtasks();

        Ok(RuntimeValue::Promise(promise_id))
    }

    /// Check if there's a pending exception and clear it
    pub(in crate::backends::jit::cranelift) fn check_and_clear_exception(
        &self,
    ) -> Option<RuntimeValue> {
        use crate::backends::builtins::CURRENT_EXCEPTION;
        CURRENT_EXCEPTION.with(|exc| exc.borrow_mut().take())
    }

    /// Handle Promise.then() and .catch() with callback execution
    pub(in crate::backends::jit::cranelift) fn handle_promise_then_catch(
        &mut self,
        method: &str,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        if args.is_empty() {
            return Ok(RuntimeValue::Null);
        }

        let promise_id = match &args[0] {
            RuntimeValue::Promise(id) => *id,
            _ => return Ok(RuntimeValue::Null),
        };

        // Get the callback function
        let callback = args.get(1).cloned();

        // Use Promise runtime's attach_then for proper microtask scheduling
        // This queues microtasks for both already-settled and pending promises
        let callback_arc = callback.map(|c| Arc::new(c));
        let downstream_id = if method == "__method_then" {
            PROMISE_RUNTIME.attach_then(promise_id, callback_arc, None)
        } else {
            PROMISE_RUNTIME.attach_then(promise_id, None, callback_arc)
        };

        // Process any immediately queued microtasks (for already-settled promises)
        let _ = self.process_microtasks();

        Ok(RuntimeValue::Promise(downstream_id))
    }

    /// Handle indirect function calls - calls functions from variables/fields
    pub(in crate::backends::jit::cranelift) fn handle_call_indirect(
        &mut self,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        if args.is_empty() {
            return Ok(RuntimeValue::Null);
        }

        let func = match &args[0] {
            RuntimeValue::Function(f) => f.clone(),
            _ => return Ok(RuntimeValue::Null),
        };

        let call_args: Vec<RuntimeValue> = args[1..].to_vec();

        // Handle special promise resolve/reject callbacks
        match func.name.as_str() {
            "__resolve" => {
                if let Some(RuntimeValue::Int(id)) = func.captures.get("__promise_id") {
                    let value = call_args.first().cloned().unwrap_or(RuntimeValue::Null);
                    PROMISE_RUNTIME.fulfill_value(*id as u64, value);
                }
                Ok(RuntimeValue::Null)
            }
            "__reject" => {
                if let Some(RuntimeValue::Int(id)) = func.captures.get("__promise_id") {
                    let reason = call_args.first().cloned().unwrap_or(RuntimeValue::Null);
                    PROMISE_RUNTIME.reject_value(*id as u64, reason);
                }
                Ok(RuntimeValue::Null)
            }
            _ => {
                // Regular function - execute it with the JIT context
                if let Some(target_func) = self.functions.get(&func.name).cloned() {
                    if func.is_async {
                        self.call_async_function(&target_func, call_args, &func.captures)
                    } else {
                        self.execute_function_with_captures(&target_func, call_args, &func.captures)
                    }
                } else {
                    Ok(RuntimeValue::Null)
                }
            }
        }
    }

    /// Handle await expression with proper microtask processing
    pub(in crate::backends::jit::cranelift) fn handle_await(
        &mut self,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        use crate::backends::builtins::CURRENT_EXCEPTION;

        if args.is_empty() {
            return Ok(RuntimeValue::Null);
        }

        let promise_id = match &args[0] {
            RuntimeValue::Promise(id) => *id,
            // If not a promise, just return the value
            other => return Ok(other.clone()),
        };

        // Wait for the promise to settle, processing microtasks
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 1_000_000;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Err("Await timeout: promise never settled".to_string());
            }

            // Process pending microtasks
            self.process_microtasks()?;

            // Check if promise is settled
            if let Some(state) = PROMISE_RUNTIME.get_state(promise_id) {
                match state {
                    PromiseState::Fulfilled(value) => {
                        return Ok((*value).clone());
                    }
                    PromiseState::Rejected(reason) => {
                        // Set the exception so try/catch can handle it
                        let reason_val = (*reason).clone();
                        CURRENT_EXCEPTION.with(|exc| {
                            *exc.borrow_mut() = Some(reason_val);
                        });
                        // Return null - the exception check after await will jump to catch block
                        return Ok(RuntimeValue::Null);
                    }
                    PromiseState::Pending => {
                        std::thread::sleep(std::time::Duration::from_micros(10));
                    }
                }
            } else {
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Process all pending microtasks
    pub(in crate::backends::jit::cranelift) fn process_microtasks(&mut self) -> Result<(), String> {
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
                    // Execute the callback function
                    if let RuntimeValue::Function(func) = handler.as_ref() {
                        if let Some(target_func) = self.functions.get(&func.name).cloned() {
                            match self.execute_function_with_captures(
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
                            // Function not found, pass through
                            PROMISE_RUNTIME.fulfill_value(downstream_id, (*arg).clone());
                        }
                    } else {
                        // Not a function, pass through
                        PROMISE_RUNTIME.fulfill_value(downstream_id, (*arg).clone());
                    }
                }
                Microtask::Timer {
                    callback,
                    promise_id,
                } => {
                    // Execute the timer callback
                    if let Some(target_func) = self.functions.get(&callback.name).cloned() {
                        match self.execute_function_with_captures(
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
                        // Callback function not found, just fulfill with null
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
                    // Check if result is already settled
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
                    // Check if result is already settled
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
                                        // First fulfilled wins
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

                        // If all promises rejected (and we didn't fulfill above), reject with aggregate
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
