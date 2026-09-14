//! Array method handlers for JIT execution

use crate::backends::jit::builtins::{CallableFunction, RuntimeValue};
use crate::backends::jit::cranelift::JitContext;

impl JitContext {
    /// Handle array.map(callback) - transforms each element
    pub(in crate::backends::jit::cranelift) fn handle_array_map(
        &mut self,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        if args.len() < 2 {
            return Ok(RuntimeValue::Null);
        }

        let array = args[0].clone();
        let callback = match &args[1] {
            RuntimeValue::Function(f) => f.clone(),
            RuntimeValue::Null => return Ok(array), // No callback, return original
            _ => return Ok(RuntimeValue::Null),
        };

        // Get the array elements
        let elements = match &array {
            RuntimeValue::Array(arr) => arr.clone(),
            RuntimeValue::DynArray {
                data,
                element_type,
                concrete_type,
                tracked_capacity,
            } => {
                // Return DynArray with mapped elements
                let mapped = self.map_elements(data.clone(), &callback)?;
                return Ok(RuntimeValue::DynArray {
                    data: mapped,
                    element_type: element_type.clone(),
                    concrete_type: concrete_type.clone(),
                    tracked_capacity: *tracked_capacity,
                });
            }
            _ => return Ok(RuntimeValue::Null),
        };

        let mapped = self.map_elements(elements, &callback)?;
        Ok(RuntimeValue::Array(mapped))
    }

    /// Handle array.filter(callback) - keeps elements where callback returns true
    pub(in crate::backends::jit::cranelift) fn handle_array_filter(
        &mut self,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        if args.len() < 2 {
            return Ok(RuntimeValue::Null);
        }

        let array = args[0].clone();
        let callback = match &args[1] {
            RuntimeValue::Function(f) => f.clone(),
            RuntimeValue::Null => return Ok(array), // No callback, return original
            _ => return Ok(RuntimeValue::Null),
        };

        let elements = match &array {
            RuntimeValue::Array(arr) => arr.clone(),
            RuntimeValue::DynArray {
                data,
                element_type,
                concrete_type,
                tracked_capacity,
            } => {
                let filtered = self.filter_elements(data.clone(), &callback)?;
                return Ok(RuntimeValue::DynArray {
                    data: filtered,
                    element_type: element_type.clone(),
                    concrete_type: concrete_type.clone(),
                    tracked_capacity: *tracked_capacity,
                });
            }
            _ => return Ok(RuntimeValue::Null),
        };

        let filtered = self.filter_elements(elements, &callback)?;
        Ok(RuntimeValue::Array(filtered))
    }

    /// Handle array.reduce(callback, initial) - accumulates a single value
    pub(in crate::backends::jit::cranelift) fn handle_array_reduce(
        &mut self,
        args: &[RuntimeValue],
    ) -> Result<RuntimeValue, String> {
        if args.is_empty() {
            return Ok(RuntimeValue::Null);
        }

        let array = args[0].clone();
        let callback = match args.get(1) {
            Some(RuntimeValue::Function(f)) => f.clone(),
            Some(RuntimeValue::Null) => return Ok(RuntimeValue::Null),
            None => return Ok(RuntimeValue::Null),
            Some(_) => return Ok(RuntimeValue::Null),
        };

        let initial = args.get(2).cloned().unwrap_or(RuntimeValue::Null);

        let elements = match &array {
            RuntimeValue::Array(arr) => arr.clone(),
            RuntimeValue::DynArray { data, .. } => data.clone(),
            _ => return Ok(RuntimeValue::Null),
        };

        self.reduce_elements(elements, &callback, initial)
    }

    /// Helper: apply map callback to all elements
    fn map_elements(
        &mut self,
        elements: Vec<RuntimeValue>,
        callback: &CallableFunction,
    ) -> Result<Vec<RuntimeValue>, String> {
        let mut result = Vec::with_capacity(elements.len());

        for (index, element) in elements.iter().enumerate() {
            // Check if function is in compiled functions
            if let Some(func) = self.functions.get(&callback.name).cloned() {
                // Execute callback with [element, index, array]
                let call_result = self.execute_function_with_captures(
                    &func,
                    vec![element.clone(), RuntimeValue::Int(index as i64)],
                    &callback.captures,
                )?;
                result.push(call_result);
            } else {
                // Function not compiled - return null
                result.push(RuntimeValue::Null);
            }
        }

        Ok(result)
    }

    /// Helper: filter elements using callback
    fn filter_elements(
        &mut self,
        elements: Vec<RuntimeValue>,
        callback: &CallableFunction,
    ) -> Result<Vec<RuntimeValue>, String> {
        let mut result = Vec::new();

        for (index, element) in elements.iter().enumerate() {
            if let Some(func) = self.functions.get(&callback.name).cloned() {
                let call_result = self.execute_function_with_captures(
                    &func,
                    vec![element.clone(), RuntimeValue::Int(index as i64)],
                    &callback.captures,
                )?;

                // Include element if callback returns truthy value
                if call_result.as_bool().unwrap_or(false) {
                    result.push(element.clone());
                }
            }
        }

        Ok(result)
    }

    /// Helper: reduce elements to single value
    fn reduce_elements(
        &mut self,
        elements: Vec<RuntimeValue>,
        callback: &CallableFunction,
        initial: RuntimeValue,
    ) -> Result<RuntimeValue, String> {
        let mut accumulator = initial;

        for (index, element) in elements.iter().enumerate() {
            if let Some(func) = self.functions.get(&callback.name).cloned() {
                accumulator = self.execute_function_with_captures(
                    &func,
                    vec![
                        accumulator,
                        element.clone(),
                        RuntimeValue::Int(index as i64),
                    ],
                    &callback.captures,
                )?;
            }
        }

        Ok(accumulator)
    }
}
