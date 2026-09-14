//! Call Stack Management
//!
//! This module provides call stack tracking for debugging, error reporting,
//! and stack overflow protection. It maintains a record of active function
//! calls with depth limits.
//!
//! ## Key Features
//!
//! - **Function Call Tracking**: Records function names and call sequence
//! - **Stack Overflow Protection**: Enforces maximum recursion depth
//! - **Backtrace Generation**: Provides stack traces for errors
//! - **Performance Monitoring**: Tracks call depth for profiling
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::execution::runtime_core::interpreter::state::CallStack;
//!
//! let mut call_stack = CallStack::new(1000); // max 1000 calls deep
//!
//! // Push function call
//! call_stack.push("main".to_string())?;
//! call_stack.push("foo".to_string())?;
//!
//! // Check depth
//! assert_eq!(call_stack.depth(), 2);
//!
//! // Get backtrace
//! let trace = call_stack.backtrace(10);
//! assert_eq!(trace, vec!["foo", "main"]);
//!
//! // Pop function call
//! call_stack.pop();
//! assert_eq!(call_stack.depth(), 1);
//! ```

/// A single frame in the call stack.
#[derive(Clone, Debug)]
pub struct CallFrame {
    /// Function or method name
    pub function_name: String,
    /// Optional additional context (e.g., class name for methods)
    pub context: Option<String>,
}

impl CallFrame {
    /// Create a new call frame with just a function name.
    #[inline]
    pub fn new(function_name: String) -> Self {
        Self {
            function_name,
            context: None,
        }
    }

    /// Create a new call frame with function name and context.
    #[inline]
    pub fn with_context(function_name: String, context: String) -> Self {
        Self {
            function_name,
            context: Some(context),
        }
    }

    /// Get a formatted string for this frame.
    ///
    /// Formats as "function" or "context.function" if context is present.
    pub fn to_string(&self) -> String {
        if let Some(ref ctx) = self.context {
            format!("{}.{}", ctx, self.function_name)
        } else {
            self.function_name.clone()
        }
    }
}

/// Manages the function call stack with depth limits.
///
/// Provides stack overflow protection by enforcing a maximum depth limit.
/// Useful for debugging and error reporting with stack traces.
pub struct CallStack {
    frames: Vec<CallFrame>,
    max_depth: usize,
}

impl CallStack {
    /// Create a new call stack with the given maximum depth.
    ///
    /// # Arguments
    ///
    /// * `max_depth` - Maximum number of nested function calls allowed
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let call_stack = CallStack::new(1000); // Allow up to 1000 nested calls
    /// ```
    #[inline]
    pub fn new(max_depth: usize) -> Self {
        Self {
            frames: Vec::new(),
            max_depth,
        }
    }

    /// Create a call stack with pre-allocated capacity.
    ///
    /// This can improve performance by avoiding repeated allocations.
    ///
    /// # Arguments
    ///
    /// * `max_depth` - Maximum number of nested function calls allowed
    /// * `capacity` - Initial capacity for the frame vector
    #[inline]
    pub fn with_capacity(max_depth: usize, capacity: usize) -> Self {
        Self {
            frames: Vec::with_capacity(capacity),
            max_depth,
        }
    }

    /// Push a new function call onto the stack.
    ///
    /// # Arguments
    ///
    /// * `function_name` - Name of the function being called
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if max depth exceeded
    ///
    /// # Errors
    ///
    /// Returns an error if pushing would exceed the maximum depth limit.
    pub fn push(&mut self, function_name: String) -> Result<(), String> {
        if self.frames.len() >= self.max_depth {
            return Err(format!(
                "Stack overflow: maximum recursion depth {} exceeded",
                self.max_depth
            ));
        }

        self.frames.push(CallFrame::new(function_name));
        Ok(())
    }

    /// Push a new function call with context onto the stack.
    ///
    /// # Arguments
    ///
    /// * `function_name` - Name of the function being called
    /// * `context` - Additional context (e.g., class name for methods)
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err if max depth exceeded
    pub fn push_with_context(
        &mut self,
        function_name: String,
        context: String,
    ) -> Result<(), String> {
        if self.frames.len() >= self.max_depth {
            return Err(format!(
                "Stack overflow: maximum recursion depth {} exceeded",
                self.max_depth
            ));
        }

        self.frames
            .push(CallFrame::with_context(function_name, context));
        Ok(())
    }

    /// Pop the top function call from the stack.
    ///
    /// # Note
    ///
    /// This is a no-op if the stack is already empty.
    #[inline]
    pub fn pop(&mut self) {
        self.frames.pop();
    }

    /// Get the current call depth.
    ///
    /// # Returns
    ///
    /// Number of active function calls on the stack
    #[inline]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Check if the call stack is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Get the maximum allowed depth.
    #[inline]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Get a reference to the current (top) call frame.
    ///
    /// # Returns
    ///
    /// Optional reference to the top frame (None if stack is empty)
    #[inline]
    pub fn current_frame(&self) -> Option<&CallFrame> {
        self.frames.last()
    }

    /// Get a backtrace of function calls.
    ///
    /// Returns a vector of function names in reverse order (most recent first).
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of frames to include
    ///
    /// # Returns
    ///
    /// Vector of formatted function names (most recent first)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let trace = call_stack.backtrace(10);
    /// // trace might be: ["inner_fn", "middle_fn", "outer_fn", "main"]
    /// ```
    pub fn backtrace(&self, limit: usize) -> Vec<String> {
        self.frames
            .iter()
            .rev()
            .take(limit)
            .map(|frame| frame.to_string())
            .collect()
    }

    /// Get a full backtrace of all function calls.
    ///
    /// Returns all frames in reverse order (most recent first).
    ///
    /// # Returns
    ///
    /// Vector of formatted function names
    pub fn full_backtrace(&self) -> Vec<String> {
        self.frames
            .iter()
            .rev()
            .map(|frame| frame.to_string())
            .collect()
    }

    /// Get a formatted backtrace string for error messages.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of frames to include
    ///
    /// # Returns
    ///
    /// Formatted string with each frame on a new line
    pub fn format_backtrace(&self, limit: usize) -> String {
        let frames = self.backtrace(limit);
        if frames.is_empty() {
            return String::from("  <no stack trace available>");
        }

        frames
            .iter()
            .enumerate()
            .map(|(i, frame)| format!("  {} at {}", i, frame))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear the entire call stack.
    ///
    /// This is useful for resetting state in REPL or test scenarios.
    #[inline]
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// Get direct access to the frames for advanced use cases.
    ///
    /// # Safety
    ///
    /// Direct access bypasses call stack invariants. Use with caution.
    #[inline]
    pub fn frames(&self) -> &[CallFrame] {
        &self.frames
    }

    /// Get mutable access to the frames for advanced use cases.
    ///
    /// # Safety
    ///
    /// Direct access bypasses call stack invariants. Use with caution.
    #[inline]
    pub fn frames_mut(&mut self) -> &mut Vec<CallFrame> {
        &mut self.frames
    }
}

impl Default for CallStack {
    fn default() -> Self {
        // Default to 1000 max depth (reasonable for most programs)
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_stack_creation() {
        let stack = CallStack::new(100);
        assert_eq!(stack.depth(), 0);
        assert_eq!(stack.max_depth(), 100);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_push_pop() {
        let mut stack = CallStack::new(100);

        stack.push("main".to_string()).unwrap();
        assert_eq!(stack.depth(), 1);

        stack.push("foo".to_string()).unwrap();
        assert_eq!(stack.depth(), 2);

        stack.pop();
        assert_eq!(stack.depth(), 1);

        stack.pop();
        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_max_depth_enforcement() {
        let mut stack = CallStack::new(3);

        assert!(stack.push("f1".to_string()).is_ok());
        assert!(stack.push("f2".to_string()).is_ok());
        assert!(stack.push("f3".to_string()).is_ok());

        // Fourth push should fail
        let result = stack.push("f4".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stack overflow"));
    }

    #[test]
    fn test_backtrace() {
        let mut stack = CallStack::new(100);

        stack.push("main".to_string()).unwrap();
        stack.push("foo".to_string()).unwrap();
        stack.push("bar".to_string()).unwrap();

        let trace = stack.backtrace(10);
        assert_eq!(trace, vec!["bar", "foo", "main"]);
    }

    #[test]
    fn test_backtrace_limit() {
        let mut stack = CallStack::new(100);

        stack.push("f1".to_string()).unwrap();
        stack.push("f2".to_string()).unwrap();
        stack.push("f3".to_string()).unwrap();
        stack.push("f4".to_string()).unwrap();

        let trace = stack.backtrace(2);
        assert_eq!(trace, vec!["f4", "f3"]);
    }

    #[test]
    fn test_current_frame() {
        let mut stack = CallStack::new(100);

        assert!(stack.current_frame().is_none());

        stack.push("main".to_string()).unwrap();
        let frame = stack.current_frame().unwrap();
        assert_eq!(frame.function_name, "main");

        stack.push("foo".to_string()).unwrap();
        let frame = stack.current_frame().unwrap();
        assert_eq!(frame.function_name, "foo");
    }

    #[test]
    fn test_call_frame_with_context() {
        let mut stack = CallStack::new(100);

        stack
            .push_with_context("method".to_string(), "MyClass".to_string())
            .unwrap();

        let frame = stack.current_frame().unwrap();
        assert_eq!(frame.to_string(), "MyClass.method");
    }

    #[test]
    fn test_clear() {
        let mut stack = CallStack::new(100);

        stack.push("f1".to_string()).unwrap();
        stack.push("f2".to_string()).unwrap();
        assert_eq!(stack.depth(), 2);

        stack.clear();
        assert_eq!(stack.depth(), 0);
        assert!(stack.is_empty());
    }
}
