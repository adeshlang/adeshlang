use crate::parsing::ast::Value;
use std::fmt::Write;

/// ANSI color codes for pretty printing
#[derive(Clone, Copy)]
pub struct ColorScheme {
    pub key: &'static str,
    pub string: &'static str,
    pub number: &'static str,
    pub boolean: &'static str,
    pub null: &'static str,
    pub bracket: &'static str,
    pub colon: &'static str,
    pub comma: &'static str,
    pub reset: &'static str,
    pub type_hint: &'static str,
    pub function: &'static str,
    pub class: &'static str,
    pub instance: &'static str,
}

impl ColorScheme {
    /// Default color scheme inspired by JavaScript console and JSON formatters
    pub const fn default() -> Self {
        ColorScheme {
            key: "\x1b[38;2;156;220;254m",      // Light blue for keys
            string: "\x1b[38;2;206;145;120m",   // Peach/orange for strings
            number: "\x1b[38;2;181;206;168m",   // Light green for numbers
            boolean: "\x1b[38;2;86;156;214m",   // Blue for booleans
            null: "\x1b[38;2;128;128;128m",     // Gray for null
            bracket: "\x1b[38;2;212;212;212m",  // Light gray for brackets
            colon: "\x1b[38;2;212;212;212m",    // Light gray for colons
            comma: "\x1b[38;2;212;212;212m",    // Light gray for commas
            type_hint: "\x1b[38;2;78;201;176m", // Teal for type hints
            function: "\x1b[38;2;220;220;170m", // Yellow for functions
            class: "\x1b[38;2;78;201;176m",     // Teal for classes
            instance: "\x1b[38;2;156;220;254m", // Light blue for instances
            reset: "\x1b[0m",
        }
    }

    /// Simple color scheme for environments that support limited colors
    pub const fn simple() -> Self {
        ColorScheme {
            key: "\x1b[36m",       // Cyan
            string: "\x1b[33m",    // Yellow
            number: "\x1b[32m",    // Green
            boolean: "\x1b[35m",   // Magenta
            null: "\x1b[90m",      // Bright black (gray)
            bracket: "\x1b[37m",   // White
            colon: "\x1b[37m",     // White
            comma: "\x1b[37m",     // White
            type_hint: "\x1b[96m", // Bright cyan
            function: "\x1b[93m",  // Bright yellow
            class: "\x1b[96m",     // Bright cyan
            instance: "\x1b[94m",  // Bright blue
            reset: "\x1b[0m",
        }
    }

    /// No colors - plain text
    pub const fn none() -> Self {
        ColorScheme {
            key: "",
            string: "",
            number: "",
            boolean: "",
            null: "",
            bracket: "",
            colon: "",
            comma: "",
            type_hint: "",
            function: "",
            class: "",
            instance: "",
            reset: "",
        }
    }
}

#[derive(Clone)]
pub struct PrettyPrintOptions {
    /// Indentation string (e.g., "  " for 2 spaces, "\t" for tab)
    pub indent_str: String,
    /// Maximum depth to print (prevents infinite recursion)
    pub max_depth: usize,
    /// Color scheme to use
    pub colors: ColorScheme,
    /// Whether to show type hints
    pub show_types: bool,
    /// Whether to show expanded object fields
    pub expand_objects: bool,
    /// Whether to show array indices
    pub show_indices: bool,
    /// Whether to align values in arrays/objects
    pub align_values: bool,
    /// Maximum line width before wrapping
    pub max_line_width: usize,
    /// Whether to show memory addresses for references
    pub show_addresses: bool,
}

impl Default for PrettyPrintOptions {
    fn default() -> Self {
        PrettyPrintOptions {
            indent_str: "  ".to_string(),
            max_depth: 10,
            colors: ColorScheme::default(),
            show_types: true,
            expand_objects: true,
            show_indices: false,
            align_values: true,
            max_line_width: 80,
            show_addresses: false,
        }
    }
}

impl PrettyPrintOptions {
    /// Create options with no colors
    pub fn no_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::none(),
            ..Default::default()
        }
    }

    /// Create options with simple colors
    pub fn simple_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::simple(),
            ..Default::default()
        }
    }

    /// Create compact options (no types, no expansion)
    pub fn compact() -> Self {
        PrettyPrintOptions {
            show_types: false,
            // Keep object expansion in compact mode to match interpreter
            // output shape (keys on separate lines) while omitting type hints.
            expand_objects: true,
            show_indices: false,
            align_values: false,
            ..Default::default()
        }
    }
}

/// Pretty print a value with colors and indentation
pub fn pretty_print(value: &Value, options: &PrettyPrintOptions) -> String {
    let mut output = String::new();
    pretty_print_inner(value, options, &mut output, 0, 0);
    output
}

fn pretty_print_inner(
    value: &Value,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    if depth >= options.max_depth {
        let _ = write!(
            output,
            "{}<max depth>{}",
            options.colors.null, options.colors.reset
        );
        return;
    }

    match value {
        // Primitives
        Value::Null => {
            let _ = write!(
                output,
                "{}null{}",
                options.colors.null, options.colors.reset
            );
        }
        Value::Bool(b) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.boolean, b, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨bool⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        Value::Number(n) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, n, options.colors.reset
            );
            if options.show_types {
                let hint = infer_integer_hint_from_number(*n).unwrap_or("number");
                let _ = write!(
                    output,
                    " {}⟨{}⟩{}",
                    options.colors.type_hint, hint, options.colors.reset
                );
            }
        }
        Value::BigInt(bi) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, bi, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨bigint⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        Value::Char(c) => {
            let _ = write!(
                output,
                "{}'{}'{}",
                options.colors.string, c, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨char⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        Value::Str(s) => {
            let _ = write!(
                output,
                "{}\"{}\"{}",
                options.colors.string,
                escape_string(s),
                options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨string⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }

        // Fixed-width integers
        Value::U8(n) => print_fixed_integer(output, n, "u8", options),
        Value::U16(n) => print_fixed_integer(output, n, "u16", options),
        Value::U32(n) => print_fixed_integer(output, n, "u32", options),
        Value::U64(n) => print_fixed_integer(output, n, "u64", options),
        Value::U128(n) => print_fixed_integer(output, n, "u128", options),
        Value::I8(n) => print_fixed_integer(output, n, "i8", options),
        Value::I16(n) => print_fixed_integer(output, n, "i16", options),
        Value::I32(n) => print_fixed_integer(output, n, "i32", options),
        Value::I64(n) => print_fixed_integer(output, n, "i64", options),
        Value::I128(n) => print_fixed_integer(output, n, "i128", options),
        Value::F32(n) => print_fixed_integer(output, n, "f32", options),
        Value::F64(n) => print_fixed_integer(output, n, "f64", options),

        // Complex numbers
        Value::Complex(r, i) => {
            let _ = write!(
                output,
                "{}{}+{}j{}",
                options.colors.number, r, i, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨complex⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }

        // Arrays
        Value::Array(arr) | Value::RawArray(_, arr) => {
            if arr.is_empty() {
                let _ = write!(
                    output,
                    "{}[]{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let type_name = if matches!(value, Value::RawArray(..)) {
                        "raw array"
                    } else {
                        "array"
                    };
                    let _ = write!(
                        output,
                        " {}⟨{}⟩{}",
                        options.colors.type_hint, type_name, options.colors.reset
                    );
                }
            } else {
                print_array(arr, options, output, depth, current_indent);
            }
        }
        Value::DynArray(da) => {
            if da.is_empty() {
                let _ = write!(
                    output,
                    "{}[]{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨dyn array⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_array(&da.data, options, output, depth, current_indent);
            }
        }

        // Tuples
        Value::Tuple(t) => {
            if t.is_empty() {
                let _ = write!(
                    output,
                    "{}(){}",
                    options.colors.bracket, options.colors.reset
                );
            } else {
                print_tuple(t, options, output, depth, current_indent);
            }
        }

        // Sets
        Value::Set(s) => {
            if s.is_empty() {
                let _ = write!(
                    output,
                    "{}{{}}{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨set⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_set(s, options, output, depth, current_indent);
            }
        }

        // Objects
        Value::Object(obj) => {
            if obj.is_empty() {
                let _ = write!(
                    output,
                    "{}{{}}{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨object⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_object(obj, options, output, depth, current_indent);
            }
        }

        // Functions and Classes
        Value::Function(_) => {
            let _ = write!(
                output,
                "{}⟨native function⟩{}",
                options.colors.function, options.colors.reset
            );
        }
        Value::UserFunction(uf) => {
            let _ = write!(
                output,
                "{}⟨function {}⟩{}",
                options.colors.function, uf.name, options.colors.reset
            );
        }
        Value::BoundNative(name, _) => {
            let _ = write!(
                output,
                "{}⟨bound native {}⟩{}",
                options.colors.function, name, options.colors.reset
            );
        }
        Value::BoundMethod(uf, inst) => {
            let _ = write!(
                output,
                "{}⟨bound {}.{}⟩{}",
                options.colors.function, inst.class_name, uf.name, options.colors.reset
            );
        }
        Value::Class(c) => {
            let _ = write!(
                output,
                "{}⟨class {}⟩{}",
                options.colors.class, c.name, options.colors.reset
            );
        }
        Value::Instance(inst) => {
            if options.expand_objects {
                print_instance(inst, options, output, depth, current_indent);
            } else {
                let _ = write!(
                    output,
                    "{}⟨{} instance⟩{}",
                    options.colors.instance, inst.class_name, options.colors.reset
                );
            }
        }
        Value::Struct(s) => {
            let _ = write!(
                output,
                "{}⟨struct {}⟩{}",
                options.colors.class, s.name, options.colors.reset
            );
        }
        Value::Enum(e) => {
            let _ = write!(
                output,
                "{}⟨enum {}⟩{}",
                options.colors.class, e.name, options.colors.reset
            );
        }
        Value::EnumCtor(e, variant) => {
            let _ = write!(
                output,
                "{}⟨{}::{}⟩{}",
                options.colors.class, e.name, variant, options.colors.reset
            );
        }
        Value::Interface(i) => {
            let _ = write!(
                output,
                "{}⟨interface {}⟩{}",
                options.colors.class, i.name, options.colors.reset
            );
        }
        Value::Super(_, _) => {
            let _ = write!(
                output,
                "{}⟨super⟩{}",
                options.colors.class, options.colors.reset
            );
        }

        // Special types
        Value::Promise(id) => {
            let _ = write!(
                output,
                "{}⟨promise #{}⟩{}",
                options.colors.type_hint, id, options.colors.reset
            );
        }
        Value::Error(err) => {
            let _ = write!(
                output,
                "{}⟨error: {}⟩{}",
                options.colors.null, err.message, options.colors.reset
            );
        }
        Value::Ref(inner, _handle) => {
            let _ = write!(output, "{}⟨ref ", options.colors.type_hint);
            if options.show_addresses {
                let _ = write!(output, "0x{:x} ", inner.as_ref() as *const _ as usize);
            }
            pretty_print_inner(inner, options, output, depth, current_indent);
        }
        Value::Share(strong_ref) => unsafe {
            let obj = &*strong_ref.ptr;
            let _ = write!(
                output,
                "{}⟨share (strong: {}, weak: {})⟩{}",
                options.colors.type_hint,
                obj.strong_count.load(std::sync::atomic::Ordering::SeqCst),
                obj.weak_count.load(std::sync::atomic::Ordering::SeqCst),
                options.colors.reset
            );
            pretty_print_inner(&obj.value, options, output, depth + 1, current_indent);
        },
        Value::Weak(weak_ref) => unsafe {
            let obj = &*weak_ref.ptr;
            let is_alive = obj.strong_count.load(std::sync::atomic::Ordering::SeqCst) > 0;
            let _ = write!(
                output,
                "{}⟨weak (alive: {}, strong: {}, weak: {})⟩{}",
                options.colors.type_hint,
                is_alive,
                obj.strong_count.load(std::sync::atomic::Ordering::SeqCst),
                obj.weak_count.load(std::sync::atomic::Ordering::SeqCst),
                options.colors.reset
            );
        },
        Value::LazyRange(s, e, step) => {
            let _ = write!(output, "range({}, {}, {})", s, e, step);
        }
    }
}

fn infer_integer_hint_from_number(n: f64) -> Option<&'static str> {
    if !n.is_finite() {
        return None;
    }

    // If the number has a fractional part, it's a float
    if n.fract() != 0.0 {
        // Determine if it's f32 or f64 based on precision required
        // f32 can precisely represent values up to ±2^24 (16,777,216)
        // For smaller values with 7 decimal digits of precision, f32 is sufficient
        let f32_val = n as f32 as f64;
        if (f32_val - n).abs() < 1e-6 && n.abs() <= 1e7 {
            Some("f32")
        } else {
            Some("f64")
        }
    } else {
        // Whole number - infer integer type based on range
        if n >= 0.0 {
            if n <= u8::MAX as f64 {
                Some("u8")
            } else if n <= u16::MAX as f64 {
                Some("u16")
            } else if n <= u32::MAX as f64 {
                Some("u32")
            } else {
                Some("u64")
            }
        } else if n >= i8::MIN as f64 {
            Some("i8")
        } else if n >= i16::MIN as f64 {
            Some("i16")
        } else if n >= i32::MIN as f64 {
            Some("i32")
        } else {
            Some("i64")
        }
    }
}

fn print_fixed_integer<T: std::fmt::Display>(
    output: &mut String,
    value: &T,
    type_name: &str,
    options: &PrettyPrintOptions,
) {
    let _ = write!(
        output,
        "{}{}{}",
        options.colors.number, value, options.colors.reset
    );
    if options.show_types {
        let _ = write!(
            output,
            " {}⟨{}⟩{}",
            options.colors.type_hint, type_name, options.colors.reset
        );
    }
}

fn print_array(
    arr: &[Value],
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}[{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    for (i, item) in arr.iter().enumerate() {
        // Indent
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        // Optional index
        if options.show_indices {
            let _ = write!(
                output,
                "{}{}:{} ",
                options.colors.type_hint, i, options.colors.reset
            );
        }

        pretty_print_inner(item, options, output, depth + 1, new_indent);

        if i < arr.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    // Closing bracket
    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}]{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨array[{}]⟩{}",
            options.colors.type_hint,
            arr.len(),
            options.colors.reset
        );
    }
}

fn print_tuple(
    tuple: &[Value],
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}({}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    for (i, item) in tuple.iter().enumerate() {
        // Indent
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        pretty_print_inner(item, options, output, depth + 1, new_indent);

        if i < tuple.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    // Closing bracket
    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}){}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨tuple[{}]⟩{}",
            options.colors.type_hint,
            tuple.len(),
            options.colors.reset
        );
    }
}

fn print_set(
    set: &[Value],
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}{{{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    for (i, item) in set.iter().enumerate() {
        // Indent
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        pretty_print_inner(item, options, output, depth + 1, new_indent);

        if i < set.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    // Closing bracket
    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}}}{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨set[{}]⟩{}",
            options.colors.type_hint,
            set.len(),
            options.colors.reset
        );
    }
}

fn print_object(
    obj: &std::sync::Arc<rustc_hash::FxHashMap<String, Value>>,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}{{{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    let mut keys: Vec<_> = obj.keys().collect();
    keys.sort(); // Sort keys for consistent output

    let max_key_len = if options.align_values {
        keys.iter().map(|k| k.len()).max().unwrap_or(0)
    } else {
        0
    };

    for (i, key) in keys.iter().enumerate() {
        // Indent
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        // Key
        let _ = write!(output, "{}", options.colors.key);
        if options.align_values {
            let _ = write!(output, "{:<width$}", key, width = max_key_len);
        } else {
            let _ = write!(output, "{}", key);
        }
        let _ = write!(output, "{}", options.colors.reset);

        // Colon
        let _ = write!(output, "{}: {}", options.colors.colon, options.colors.reset);

        // Value
        if let Some(value) = obj.get(*key) {
            pretty_print_inner(value, options, output, depth + 1, new_indent);
        }

        if i < keys.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    // Closing bracket
    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}}}{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨object⟩{}",
            options.colors.type_hint, options.colors.reset
        );
    }
}

fn print_instance(
    inst: &crate::parsing::ast::UserInstance,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}{} instance{} {{\n",
        options.colors.instance, inst.class_name, options.colors.reset
    );

    let new_indent = current_indent + 1;

    // Get fields from the instance
    if let Ok(fields) = inst.fields.read() {
        let mut keys: Vec<_> = fields.keys().collect();
        keys.sort();

        let max_key_len = if options.align_values {
            keys.iter().map(|k| k.len()).max().unwrap_or(0)
        } else {
            0
        };

        for (i, key) in keys.iter().enumerate() {
            // Indent
            for _ in 0..new_indent {
                output.push_str(&options.indent_str);
            }

            // Key
            let _ = write!(output, "{}", options.colors.key);
            if options.align_values {
                let _ = write!(output, "{:<width$}", key, width = max_key_len);
            } else {
                let _ = write!(output, "{}", key);
            }
            let _ = write!(output, "{}", options.colors.reset);

            // Colon
            let _ = write!(output, "{}: {}", options.colors.colon, options.colors.reset);

            // Value
            if let Some(value) = fields.get(*key) {
                pretty_print_inner(value, options, output, depth + 1, new_indent);
            }

            if i < keys.len() - 1 {
                let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
            }
            output.push('\n');
        }
    }

    // Closing bracket
    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}}}{}",
        options.colors.bracket, options.colors.reset
    );
}

/// Escape special characters in strings for display
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            c if c.is_control() => {
                result.push_str(&format!("\\u{{{:04x}}}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pretty_print_primitives() {
        let options = PrettyPrintOptions::no_color();

        assert_eq!(pretty_print(&Value::Null, &options), "null");
        assert_eq!(pretty_print(&Value::Bool(true), &options), "true ⟨bool⟩");
        assert_eq!(pretty_print(&Value::Number(42.0), &options), "42 ⟨u8⟩");
    }

    #[test]
    fn test_pretty_print_array() {
        let options = PrettyPrintOptions::no_color();
        let arr = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);

        let result = pretty_print(&arr, &options);
        assert!(result.contains("[\n"));
        assert!(result.contains("  1 ⟨u8⟩"));
        assert!(result.contains("] ⟨array[3]⟩"));
    }
}
