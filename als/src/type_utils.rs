//! Shared semantic, type-resolution, and catalog helpers for ALS.
//!
//! Centralizes:
//! - Idiomatic `Ty` formatting and base/inner user-type extraction
//! - Chained receiver expression type resolution (`user.address.city`, `get_user().name`, `arr[0]`, `this`)
//! - Enclosing scope / type / function lookup at a source line
//! - Doc-comment extraction from preceding `///` or `//` comments
//! - Levenshtein similarity for "Did you mean ...?" diagnostics & quick fixes
//! - Shared builtin & stdlib module catalog (used by hover, completion, signature help, diagnostics)

use crate::document::Document;
use crate::workspace::WorkspaceIndex;
use adeshlang::parsing::ast::{Expr, ExprKind, Function, Stmt, StmtKind};
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, SymbolEntry, TypeMember};
use adeshlang::typesystem::checker::Ty;

/// Builtin function metadata shared across completion, hover, signature help, and diagnostics.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinFnSpec {
    pub name: &'static str,
    pub signature: &'static str,
    pub params: &'static [(&'static str, &'static str)],
    pub return_type: &'static str,
    pub min_args: usize,
    pub max_args: Option<usize>,
    pub doc: &'static str,
}

/// Builtin namespace member metadata.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceMemberSpec {
    pub name: &'static str,
    pub signature: &'static str,
    pub is_const: bool,
    pub params: &'static [(&'static str, &'static str)],
    pub return_type: &'static str,
    pub doc: &'static str,
}

/// Stdlib module metadata for import completions and auto-imports.
#[derive(Debug, Clone, Copy)]
pub struct StdlibModuleSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub exports: &'static [NamespaceMemberSpec],
}

pub const BUILTIN_FUNCTIONS: &[BuiltinFnSpec] = &[
    BuiltinFnSpec {
        name: "print",
        signature: "fn print(...args, options?)",
        params: &[
            ("...args", "Values to print"),
            (
                "options?",
                "Formatting options object (sep, end, color, bold, pretty, file, flush)",
            ),
        ],
        return_type: "void",
        min_args: 0,
        max_args: None,
        doc: "Print any number of values to stdout separated by spaces.",
    },
    BuiltinFnSpec {
        name: "println",
        signature: "fn println(...args)",
        params: &[("...args", "Values to print with trailing newline")],
        return_type: "void",
        min_args: 0,
        max_args: None,
        doc: "Print values to stdout with a trailing newline.",
    },
    BuiltinFnSpec {
        name: "eprint",
        signature: "fn eprint(...args)",
        params: &[("...args", "Values to print to stderr")],
        return_type: "void",
        min_args: 0,
        max_args: None,
        doc: "Print values to standard error (stderr).",
    },
    BuiltinFnSpec {
        name: "assert",
        signature: "fn assert(condition: bool, message?: string)",
        params: &[
            ("condition: bool", "Boolean condition expected to be true"),
            ("message?: string", "Optional failure message"),
        ],
        return_type: "void",
        min_args: 1,
        max_args: Some(2),
        doc: "Fail the test or program if `condition` is false.",
    },
    BuiltinFnSpec {
        name: "assert_eq",
        signature: "fn assert_eq(left: any, right: any, message?: string)",
        params: &[
            ("left: any", "Actual value"),
            ("right: any", "Expected value"),
            ("message?: string", "Optional failure message"),
        ],
        return_type: "void",
        min_args: 2,
        max_args: Some(3),
        doc: "Fail the test if `left` and `right` are not equal.",
    },
    BuiltinFnSpec {
        name: "assert_ne",
        signature: "fn assert_ne(left: any, right: any, message?: string)",
        params: &[
            ("left: any", "First value"),
            ("right: any", "Value that must differ"),
            ("message?: string", "Optional failure message"),
        ],
        return_type: "void",
        min_args: 2,
        max_args: Some(3),
        doc: "Fail the test if `left` and `right` are equal.",
    },
    BuiltinFnSpec {
        name: "input",
        signature: "fn input(prompt?: string): string",
        params: &[(
            "prompt?: string",
            "Optional prompt displayed before reading stdin",
        )],
        return_type: "string",
        min_args: 0,
        max_args: Some(1),
        doc: "Read a line from stdin.",
    },
    BuiltinFnSpec {
        name: "len",
        signature: "fn len(collection: any): int",
        params: &[("collection: any", "Array, string, map, set, or tuple")],
        return_type: "int",
        min_args: 1,
        max_args: Some(1),
        doc: "Get the length of an array, string, map, or collection.",
    },
    BuiltinFnSpec {
        name: "type",
        signature: "fn type(value: any): string",
        params: &[("value: any", "Value to inspect")],
        return_type: "string",
        min_args: 1,
        max_args: Some(1),
        doc: "Return the runtime type name of `value`.",
    },
    BuiltinFnSpec {
        name: "range",
        signature: "fn range(start: int, end: int, step?: int): Array<int>",
        params: &[
            ("start: int", "Start value (inclusive)"),
            ("end: int", "End value (exclusive)"),
            ("step?: int", "Step increment (default 1)"),
        ],
        return_type: "Array<int>",
        min_args: 1,
        max_args: Some(3),
        doc: "Create a numeric range array.",
    },
    BuiltinFnSpec {
        name: "map",
        signature: "fn map(arr: Array<T>, transform: fn(T) -> U): Array<U>",
        params: &[
            ("arr: Array<T>", "Input array"),
            ("transform: fn(T) -> U", "Transformation callback"),
        ],
        return_type: "Array<any>",
        min_args: 2,
        max_args: Some(2),
        doc: "Transform each element of an array with `transform`.",
    },
    BuiltinFnSpec {
        name: "filter",
        signature: "fn filter(arr: Array<T>, predicate: fn(T) -> bool): Array<T>",
        params: &[
            ("arr: Array<T>", "Input array"),
            (
                "predicate: fn(T) -> bool",
                "Predicate returning true to keep element",
            ),
        ],
        return_type: "Array<any>",
        min_args: 2,
        max_args: Some(2),
        doc: "Filter array elements matching `predicate`.",
    },
    BuiltinFnSpec {
        name: "reduce",
        signature: "fn reduce(arr: Array<T>, reducer: fn(Acc, T) -> Acc, initial: Acc): Acc",
        params: &[
            ("arr: Array<T>", "Input array"),
            (
                "reducer: fn(Acc, T) -> Acc",
                "Accumulator callback `(acc, item) -> acc`",
            ),
            ("initial: Acc", "Initial accumulator value"),
        ],
        return_type: "any",
        min_args: 3,
        max_args: Some(3),
        doc: "Reduce an array into a single accumulated value.",
    },
    BuiltinFnSpec {
        name: "clock",
        signature: "fn clock(): i64",
        params: &[],
        return_type: "i64",
        min_args: 0,
        max_args: Some(0),
        doc: "Return a monotonic high-resolution timestamp in nanoseconds.",
    },
    BuiltinFnSpec {
        name: "sleep",
        signature: "fn sleep(ms: number)",
        params: &[("ms: number", "Duration in milliseconds")],
        return_type: "void",
        min_args: 1,
        max_args: Some(1),
        doc: "Pause execution for `ms` milliseconds.",
    },
    BuiltinFnSpec {
        name: "str",
        signature: "fn str(value: any): string",
        params: &[("value: any", "Value to stringify")],
        return_type: "string",
        min_args: 1,
        max_args: Some(1),
        doc: "Convert any value to its string representation.",
    },
    BuiltinFnSpec {
        name: "int",
        signature: "fn int(value: any): int",
        params: &[("value: any", "Value to convert to integer")],
        return_type: "int",
        min_args: 1,
        max_args: Some(1),
        doc: "Convert a value to a 32-bit signed integer.",
    },
    BuiltinFnSpec {
        name: "float",
        signature: "fn float(value: any): float",
        params: &[("value: any", "Value to convert to float")],
        return_type: "float",
        min_args: 1,
        max_args: Some(1),
        doc: "Convert a value to a 64-bit floating-point number.",
    },
    BuiltinFnSpec {
        name: "min",
        signature: "fn min(...args: number): number",
        params: &[("...args: number", "Numbers to compare")],
        return_type: "number",
        min_args: 1,
        max_args: None,
        doc: "Return the minimum value.",
    },
    BuiltinFnSpec {
        name: "max",
        signature: "fn max(...args: number): number",
        params: &[("...args: number", "Numbers to compare")],
        return_type: "number",
        min_args: 1,
        max_args: None,
        doc: "Return the maximum value.",
    },
    BuiltinFnSpec {
        name: "sizeof",
        signature: "fn sizeof<T>(): int",
        params: &[],
        return_type: "int",
        min_args: 0,
        max_args: Some(1),
        doc: "Return the size of a type in bytes.",
    },
    BuiltinFnSpec {
        name: "alignof",
        signature: "fn alignof<T>(): int",
        params: &[],
        return_type: "int",
        min_args: 0,
        max_args: Some(1),
        doc: "Return the alignment of a type in bytes.",
    },
    BuiltinFnSpec {
        name: "panic",
        signature: "fn panic(message?: string): never",
        params: &[("message?: string", "Panic message")],
        return_type: "never",
        min_args: 0,
        max_args: Some(1),
        doc: "Abort execution immediately with an error message.",
    },
];

pub const MATH_MEMBERS: &[NamespaceMemberSpec] = &[
    NamespaceMemberSpec {
        name: "PI",
        signature: "const PI: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "π ≈ 3.141592653589793",
    },
    NamespaceMemberSpec {
        name: "E",
        signature: "const E: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "Euler's number ≈ 2.718281828459045",
    },
    NamespaceMemberSpec {
        name: "TAU",
        signature: "const TAU: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "τ = 2π ≈ 6.283185307179586",
    },
    NamespaceMemberSpec {
        name: "SQRT2",
        signature: "const SQRT2: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "√2 ≈ 1.4142135623730951",
    },
    NamespaceMemberSpec {
        name: "LN2",
        signature: "const LN2: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "ln(2) ≈ 0.6931471805599453",
    },
    NamespaceMemberSpec {
        name: "LN10",
        signature: "const LN10: f64",
        is_const: true,
        params: &[],
        return_type: "f64",
        doc: "ln(10) ≈ 2.302585092994046",
    },
    NamespaceMemberSpec {
        name: "abs",
        signature: "fn abs(x: number): number",
        is_const: false,
        params: &[("x: number", "Input number")],
        return_type: "number",
        doc: "Absolute value of `x`.",
    },
    NamespaceMemberSpec {
        name: "floor",
        signature: "fn floor(x: number): number",
        is_const: false,
        params: &[("x: number", "Input number")],
        return_type: "number",
        doc: "Largest integer less than or equal to `x`.",
    },
    NamespaceMemberSpec {
        name: "ceil",
        signature: "fn ceil(x: number): number",
        is_const: false,
        params: &[("x: number", "Input number")],
        return_type: "number",
        doc: "Smallest integer greater than or equal to `x`.",
    },
    NamespaceMemberSpec {
        name: "round",
        signature: "fn round(x: number): number",
        is_const: false,
        params: &[("x: number", "Input number")],
        return_type: "number",
        doc: "Round to nearest integer.",
    },
    NamespaceMemberSpec {
        name: "sqrt",
        signature: "fn sqrt(x: number): f64",
        is_const: false,
        params: &[("x: number", "Non-negative number")],
        return_type: "f64",
        doc: "Square root of `x`.",
    },
    NamespaceMemberSpec {
        name: "pow",
        signature: "fn pow(base: number, exp: number): f64",
        is_const: false,
        params: &[("base: number", "Base"), ("exp: number", "Exponent")],
        return_type: "f64",
        doc: "Raise `base` to the power `exp`.",
    },
    NamespaceMemberSpec {
        name: "sin",
        signature: "fn sin(x: number): f64",
        is_const: false,
        params: &[("x: number", "Angle in radians")],
        return_type: "f64",
        doc: "Sine of `x` (radians).",
    },
    NamespaceMemberSpec {
        name: "cos",
        signature: "fn cos(x: number): f64",
        is_const: false,
        params: &[("x: number", "Angle in radians")],
        return_type: "f64",
        doc: "Cosine of `x` (radians).",
    },
    NamespaceMemberSpec {
        name: "tan",
        signature: "fn tan(x: number): f64",
        is_const: false,
        params: &[("x: number", "Angle in radians")],
        return_type: "f64",
        doc: "Tangent of `x` (radians).",
    },
    NamespaceMemberSpec {
        name: "exp",
        signature: "fn exp(x: number): f64",
        is_const: false,
        params: &[("x: number", "Exponent")],
        return_type: "f64",
        doc: "Compute e^x.",
    },
    NamespaceMemberSpec {
        name: "log",
        signature: "fn log(x: number): f64",
        is_const: false,
        params: &[("x: number", "Positive number")],
        return_type: "f64",
        doc: "Natural logarithm of `x`.",
    },
    NamespaceMemberSpec {
        name: "log10",
        signature: "fn log10(x: number): f64",
        is_const: false,
        params: &[("x: number", "Positive number")],
        return_type: "f64",
        doc: "Base-10 logarithm of `x`.",
    },
    NamespaceMemberSpec {
        name: "random",
        signature: "fn random(): f64",
        is_const: false,
        params: &[],
        return_type: "f64",
        doc: "Random floating-point number in `[0, 1)`.",
    },
    NamespaceMemberSpec {
        name: "randomInt",
        signature: "fn randomInt(min: int, max: int): int",
        is_const: false,
        params: &[
            ("min: int", "Minimum (inclusive)"),
            ("max: int", "Maximum (inclusive)"),
        ],
        return_type: "int",
        doc: "Random integer in `[min, max]`.",
    },
    NamespaceMemberSpec {
        name: "randomRange",
        signature: "fn randomRange(min: f64, max: f64): f64",
        is_const: false,
        params: &[("min: f64", "Lower bound"), ("max: f64", "Upper bound")],
        return_type: "f64",
        doc: "Random float in `[min, max)`.",
    },
    NamespaceMemberSpec {
        name: "seed",
        signature: "fn seed(n: int)",
        is_const: false,
        params: &[("n: int", "PRNG seed")],
        return_type: "void",
        doc: "Seed the pseudo-random number generator.",
    },
    NamespaceMemberSpec {
        name: "min",
        signature: "fn min(...args: number): number",
        is_const: false,
        params: &[("...args: number", "Numbers to compare")],
        return_type: "number",
        doc: "Return the smallest argument.",
    },
    NamespaceMemberSpec {
        name: "max",
        signature: "fn max(...args: number): number",
        is_const: false,
        params: &[("...args: number", "Numbers to compare")],
        return_type: "number",
        doc: "Return the largest argument.",
    },
    NamespaceMemberSpec {
        name: "clamp",
        signature: "fn clamp(x: number, low: number, high: number): number",
        is_const: false,
        params: &[
            ("x: number", "Value"),
            ("low: number", "Lower bound"),
            ("high: number", "Upper bound"),
        ],
        return_type: "number",
        doc: "Clamp `x` to `[low, high]`.",
    },
];

pub const TIME_MEMBERS: &[NamespaceMemberSpec] = &[
    NamespaceMemberSpec {
        name: "now",
        signature: "fn now(): i64",
        is_const: false,
        params: &[],
        return_type: "i64",
        doc: "Current monotonic time in nanoseconds.",
    },
    NamespaceMemberSpec {
        name: "nowMs",
        signature: "fn nowMs(): i64",
        is_const: false,
        params: &[],
        return_type: "i64",
        doc: "Current time in milliseconds.",
    },
    NamespaceMemberSpec {
        name: "nowUs",
        signature: "fn nowUs(): i64",
        is_const: false,
        params: &[],
        return_type: "i64",
        doc: "Current time in microseconds.",
    },
    NamespaceMemberSpec {
        name: "nowSecs",
        signature: "fn nowSecs(): f64",
        is_const: false,
        params: &[],
        return_type: "f64",
        doc: "Current time in seconds.",
    },
    NamespaceMemberSpec {
        name: "epoch",
        signature: "fn epoch(): i64",
        is_const: false,
        params: &[],
        return_type: "i64",
        doc: "Unix timestamp in seconds.",
    },
    NamespaceMemberSpec {
        name: "epochNanos",
        signature: "fn epochNanos(): i64",
        is_const: false,
        params: &[],
        return_type: "i64",
        doc: "Unix timestamp in nanoseconds.",
    },
];

pub const JSON_MEMBERS: &[NamespaceMemberSpec] = &[
    NamespaceMemberSpec {
        name: "parse",
        signature: "fn parse(text: string): any",
        is_const: false,
        params: &[("text: string", "JSON string to parse")],
        return_type: "any",
        doc: "Parse a JSON string into an Adesh value.",
    },
    NamespaceMemberSpec {
        name: "stringify",
        signature: "fn stringify(value: any, indent?: int): string",
        is_const: false,
        params: &[
            ("value: any", "Value to serialize"),
            ("indent?: int", "Optional pretty-print indentation"),
        ],
        return_type: "string",
        doc: "Serialize an Adesh value to JSON.",
    },
];

pub const STRING_METHODS: &[NamespaceMemberSpec] = &[
    NamespaceMemberSpec {
        name: "length",
        signature: "length: int",
        is_const: true,
        params: &[],
        return_type: "int",
        doc: "Length of the string.",
    },
    NamespaceMemberSpec {
        name: "len",
        signature: "fn len(): int",
        is_const: false,
        params: &[],
        return_type: "int",
        doc: "Return the length of the string.",
    },
    NamespaceMemberSpec {
        name: "toUpperCase",
        signature: "fn toUpperCase(): string",
        is_const: false,
        params: &[],
        return_type: "string",
        doc: "Convert string to uppercase.",
    },
    NamespaceMemberSpec {
        name: "toLowerCase",
        signature: "fn toLowerCase(): string",
        is_const: false,
        params: &[],
        return_type: "string",
        doc: "Convert string to lowercase.",
    },
    NamespaceMemberSpec {
        name: "trim",
        signature: "fn trim(): string",
        is_const: false,
        params: &[],
        return_type: "string",
        doc: "Remove leading and trailing whitespace.",
    },
    NamespaceMemberSpec {
        name: "trimStart",
        signature: "fn trimStart(): string",
        is_const: false,
        params: &[],
        return_type: "string",
        doc: "Remove leading whitespace.",
    },
    NamespaceMemberSpec {
        name: "trimEnd",
        signature: "fn trimEnd(): string",
        is_const: false,
        params: &[],
        return_type: "string",
        doc: "Remove trailing whitespace.",
    },
    NamespaceMemberSpec {
        name: "split",
        signature: "fn split(sep: string): Array<string>",
        is_const: false,
        params: &[("sep: string", "Delimiter string")],
        return_type: "Array<string>",
        doc: "Split the string by `sep`.",
    },
    NamespaceMemberSpec {
        name: "replace",
        signature: "fn replace(old: string, replacement: string): string",
        is_const: false,
        params: &[
            ("old: string", "Substring to replace"),
            ("replacement: string", "Replacement text"),
        ],
        return_type: "string",
        doc: "Replace occurrences of `old` with `replacement`.",
    },
    NamespaceMemberSpec {
        name: "contains",
        signature: "fn contains(substr: string): bool",
        is_const: false,
        params: &[("substr: string", "Substring to search for")],
        return_type: "bool",
        doc: "Return true if string contains `substr`.",
    },
    NamespaceMemberSpec {
        name: "startsWith",
        signature: "fn startsWith(prefix: string): bool",
        is_const: false,
        params: &[("prefix: string", "Prefix to check")],
        return_type: "bool",
        doc: "Return true if string starts with `prefix`.",
    },
    NamespaceMemberSpec {
        name: "endsWith",
        signature: "fn endsWith(suffix: string): bool",
        is_const: false,
        params: &[("suffix: string", "Suffix to check")],
        return_type: "bool",
        doc: "Return true if string ends with `suffix`.",
    },
    NamespaceMemberSpec {
        name: "slice",
        signature: "fn slice(start: int, end?: int): string",
        is_const: false,
        params: &[
            ("start: int", "Start index"),
            ("end?: int", "End index (exclusive)"),
        ],
        return_type: "string",
        doc: "Extract a substring.",
    },
    NamespaceMemberSpec {
        name: "indexOf",
        signature: "fn indexOf(substr: string): int",
        is_const: false,
        params: &[("substr: string", "Substring to locate")],
        return_type: "int",
        doc: "Return index of first match, or -1.",
    },
    NamespaceMemberSpec {
        name: "repeat",
        signature: "fn repeat(count: int): string",
        is_const: false,
        params: &[("count: int", "Repetition count")],
        return_type: "string",
        doc: "Repeat the string `count` times.",
    },
    NamespaceMemberSpec {
        name: "charAt",
        signature: "fn charAt(index: int): char",
        is_const: false,
        params: &[("index: int", "Character index")],
        return_type: "char",
        doc: "Return character at `index`.",
    },
    NamespaceMemberSpec {
        name: "concat",
        signature: "fn concat(other: string): string",
        is_const: false,
        params: &[("other: string", "String to append")],
        return_type: "string",
        doc: "Concatenate two strings.",
    },
    NamespaceMemberSpec {
        name: "isEmpty",
        signature: "fn isEmpty(): bool",
        is_const: false,
        params: &[],
        return_type: "bool",
        doc: "Return true if string is empty.",
    },
];

pub const ARRAY_METHODS: &[NamespaceMemberSpec] = &[
    NamespaceMemberSpec {
        name: "length",
        signature: "length: int",
        is_const: true,
        params: &[],
        return_type: "int",
        doc: "Number of elements in the array.",
    },
    NamespaceMemberSpec {
        name: "len",
        signature: "fn len(): int",
        is_const: false,
        params: &[],
        return_type: "int",
        doc: "Return number of elements.",
    },
    NamespaceMemberSpec {
        name: "push",
        signature: "fn push(item: T)",
        is_const: false,
        params: &[("item: T", "Element to append")],
        return_type: "void",
        doc: "Append `item` to the end of the array.",
    },
    NamespaceMemberSpec {
        name: "pop",
        signature: "fn pop(): T?",
        is_const: false,
        params: &[],
        return_type: "any",
        doc: "Remove and return the last element.",
    },
    NamespaceMemberSpec {
        name: "map",
        signature: "fn map(transform: fn(T) -> U): Array<U>",
        is_const: false,
        params: &[("transform: fn(T) -> U", "Element mapper")],
        return_type: "Array<any>",
        doc: "Create a new array by transforming each element.",
    },
    NamespaceMemberSpec {
        name: "filter",
        signature: "fn filter(predicate: fn(T) -> bool): Array<T>",
        is_const: false,
        params: &[("predicate: fn(T) -> bool", "Predicate")],
        return_type: "Array<any>",
        doc: "Create a new array containing matching elements.",
    },
    NamespaceMemberSpec {
        name: "reduce",
        signature: "fn reduce(reducer: fn(Acc, T) -> Acc, init: Acc): Acc",
        is_const: false,
        params: &[
            ("reducer: fn(Acc, T) -> Acc", "Reducer function"),
            ("init: Acc", "Initial accumulator"),
        ],
        return_type: "any",
        doc: "Fold the array into a single value.",
    },
    NamespaceMemberSpec {
        name: "forEach",
        signature: "fn forEach(callback: fn(T))",
        is_const: false,
        params: &[("callback: fn(T)", "Callback called per element")],
        return_type: "void",
        doc: "Execute `callback` for each element.",
    },
    NamespaceMemberSpec {
        name: "find",
        signature: "fn find(predicate: fn(T) -> bool): T?",
        is_const: false,
        params: &[("predicate: fn(T) -> bool", "Predicate")],
        return_type: "any",
        doc: "Return the first element matching `predicate`.",
    },
    NamespaceMemberSpec {
        name: "contains",
        signature: "fn contains(item: T): bool",
        is_const: false,
        params: &[("item: T", "Item to search for")],
        return_type: "bool",
        doc: "Return true if `item` is present.",
    },
    NamespaceMemberSpec {
        name: "includes",
        signature: "fn includes(item: T): bool",
        is_const: false,
        params: &[("item: T", "Item to search for")],
        return_type: "bool",
        doc: "Return true if `item` is present.",
    },
    NamespaceMemberSpec {
        name: "indexOf",
        signature: "fn indexOf(item: T): int",
        is_const: false,
        params: &[("item: T", "Item to locate")],
        return_type: "int",
        doc: "Return index of `item`, or -1.",
    },
    NamespaceMemberSpec {
        name: "join",
        signature: "fn join(sep?: string): string",
        is_const: false,
        params: &[("sep?: string", "Separator string")],
        return_type: "string",
        doc: "Join elements into a single string.",
    },
    NamespaceMemberSpec {
        name: "slice",
        signature: "fn slice(start: int, end?: int): Array<T>",
        is_const: false,
        params: &[("start: int", "Start index"), ("end?: int", "End index")],
        return_type: "Array<any>",
        doc: "Return a sub-slice of the array.",
    },
    NamespaceMemberSpec {
        name: "reverse",
        signature: "fn reverse(): Array<T>",
        is_const: false,
        params: &[],
        return_type: "Array<any>",
        doc: "Reverse the array.",
    },
    NamespaceMemberSpec {
        name: "sort",
        signature: "fn sort(comparator?: fn(T, T) -> int): Array<T>",
        is_const: false,
        params: &[(
            "comparator?: fn(T, T) -> int",
            "Optional comparison function",
        )],
        return_type: "Array<any>",
        doc: "Sort the array elements.",
    },
    NamespaceMemberSpec {
        name: "isEmpty",
        signature: "fn isEmpty(): bool",
        is_const: false,
        params: &[],
        return_type: "bool",
        doc: "Return true if the array has no elements.",
    },
    NamespaceMemberSpec {
        name: "first",
        signature: "fn first(): T?",
        is_const: false,
        params: &[],
        return_type: "any",
        doc: "Return the first element or null.",
    },
    NamespaceMemberSpec {
        name: "last",
        signature: "fn last(): T?",
        is_const: false,
        params: &[],
        return_type: "any",
        doc: "Return the last element or null.",
    },
];

pub const STDLIB_MODULES: &[StdlibModuleSpec] = &[
    StdlibModuleSpec {
        name: "IO",
        description: "File system, streams, and standard I/O operations.",
        exports: &[
            NamespaceMemberSpec {
                name: "readFile",
                signature: "fn readFile(path: string): string",
                is_const: false,
                params: &[("path: string", "File path")],
                return_type: "string",
                doc: "Read entire file contents as UTF-8 string.",
            },
            NamespaceMemberSpec {
                name: "writeFile",
                signature: "fn writeFile(path: string, content: string): bool",
                is_const: false,
                params: &[
                    ("path: string", "File path"),
                    ("content: string", "Content to write"),
                ],
                return_type: "bool",
                doc: "Write UTF-8 content to a file.",
            },
            NamespaceMemberSpec {
                name: "exists",
                signature: "fn exists(path: string): bool",
                is_const: false,
                params: &[("path: string", "File or directory path")],
                return_type: "bool",
                doc: "Check whether path exists.",
            },
            NamespaceMemberSpec {
                name: "readDir",
                signature: "fn readDir(path: string): Array<string>",
                is_const: false,
                params: &[("path: string", "Directory path")],
                return_type: "Array<string>",
                doc: "List entries in a directory.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "HTTP",
        description: "HTTP/1.1, HTTP/2, WebSocket client and server APIs.",
        exports: &[
            NamespaceMemberSpec {
                name: "get",
                signature: "async fn get(url: string, options?: object): Response",
                is_const: false,
                params: &[
                    ("url: string", "Target URL"),
                    ("options?: object", "Headers/timeout options"),
                ],
                return_type: "Response",
                doc: "Send an HTTP GET request.",
            },
            NamespaceMemberSpec {
                name: "post",
                signature: "async fn post(url: string, body: any, options?: object): Response",
                is_const: false,
                params: &[
                    ("url: string", "Target URL"),
                    ("body: any", "Request body"),
                    ("options?: object", "Headers/timeout options"),
                ],
                return_type: "Response",
                doc: "Send an HTTP POST request.",
            },
            NamespaceMemberSpec {
                name: "serve",
                signature: "fn serve(addr: string, handler: fn(Request) -> Response)",
                is_const: false,
                params: &[
                    ("addr: string", "Bind address"),
                    ("handler: fn(Request) -> Response", "Request handler"),
                ],
                return_type: "void",
                doc: "Start an HTTP server.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Env",
        description: "Environment variables, CLI arguments, and OS details.",
        exports: &[
            NamespaceMemberSpec {
                name: "get",
                signature: "fn get(key: string): string?",
                is_const: false,
                params: &[("key: string", "Environment variable name")],
                return_type: "string?",
                doc: "Read an environment variable.",
            },
            NamespaceMemberSpec {
                name: "set",
                signature: "fn set(key: string, value: string)",
                is_const: false,
                params: &[
                    ("key: string", "Variable name"),
                    ("value: string", "Variable value"),
                ],
                return_type: "void",
                doc: "Set an environment variable.",
            },
            NamespaceMemberSpec {
                name: "args",
                signature: "fn args(): Array<string>",
                is_const: false,
                params: &[],
                return_type: "Array<string>",
                doc: "Return command-line arguments.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Path",
        description: "Cross-platform filesystem path manipulation.",
        exports: &[
            NamespaceMemberSpec {
                name: "join",
                signature: "fn join(...parts: string): string",
                is_const: false,
                params: &[("...parts: string", "Path segments")],
                return_type: "string",
                doc: "Join path segments.",
            },
            NamespaceMemberSpec {
                name: "dirname",
                signature: "fn dirname(path: string): string",
                is_const: false,
                params: &[("path: string", "Path")],
                return_type: "string",
                doc: "Return parent directory.",
            },
            NamespaceMemberSpec {
                name: "basename",
                signature: "fn basename(path: string): string",
                is_const: false,
                params: &[("path: string", "Path")],
                return_type: "string",
                doc: "Return file name component.",
            },
            NamespaceMemberSpec {
                name: "extname",
                signature: "fn extname(path: string): string",
                is_const: false,
                params: &[("path: string", "Path")],
                return_type: "string",
                doc: "Return file extension.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Crypto",
        description: "Hashing (SHA-256/BLAKE3), AEAD, HMAC, Ed25519, JWT, Argon2.",
        exports: &[
            NamespaceMemberSpec {
                name: "sha256",
                signature: "fn sha256(data: string): string",
                is_const: false,
                params: &[("data: string", "Input data")],
                return_type: "string",
                doc: "Compute SHA-256 hex digest.",
            },
            NamespaceMemberSpec {
                name: "blake3",
                signature: "fn blake3(data: string): string",
                is_const: false,
                params: &[("data: string", "Input data")],
                return_type: "string",
                doc: "Compute BLAKE3 hex digest.",
            },
            NamespaceMemberSpec {
                name: "randomBytes",
                signature: "fn randomBytes(len: int): Array<u8>",
                is_const: false,
                params: &[("len: int", "Number of bytes")],
                return_type: "Array<u8>",
                doc: "Generate cryptographically secure random bytes.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Encoding",
        description: "Base64, hex, UTF-8, percent-encoding, and binary codecs.",
        exports: &[
            NamespaceMemberSpec {
                name: "base64Encode",
                signature: "fn base64Encode(data: string): string",
                is_const: false,
                params: &[("data: string", "Input")],
                return_type: "string",
                doc: "Encode data as Base64.",
            },
            NamespaceMemberSpec {
                name: "base64Decode",
                signature: "fn base64Decode(encoded: string): string",
                is_const: false,
                params: &[("encoded: string", "Base64 input")],
                return_type: "string",
                doc: "Decode Base64 text.",
            },
            NamespaceMemberSpec {
                name: "hexEncode",
                signature: "fn hexEncode(data: string): string",
                is_const: false,
                params: &[("data: string", "Input")],
                return_type: "string",
                doc: "Encode data as lowercase hex.",
            },
            NamespaceMemberSpec {
                name: "hexDecode",
                signature: "fn hexDecode(hex: string): string",
                is_const: false,
                params: &[("hex: string", "Hex input")],
                return_type: "string",
                doc: "Decode hexadecimal text.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Compression",
        description: "Gzip, Zstd, Brotli, LZ4, and archive compression.",
        exports: &[
            NamespaceMemberSpec {
                name: "gzip",
                signature: "fn gzip(data: string): Array<u8>",
                is_const: false,
                params: &[("data: string", "Input data")],
                return_type: "Array<u8>",
                doc: "Compress with gzip.",
            },
            NamespaceMemberSpec {
                name: "gunzip",
                signature: "fn gunzip(bytes: Array<u8>): string",
                is_const: false,
                params: &[("bytes: Array<u8>", "Gzip payload")],
                return_type: "string",
                doc: "Decompress gzip data.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Random",
        description: "Seedable PRNGs, distributions, shuffling, and random strings.",
        exports: &[
            NamespaceMemberSpec {
                name: "int",
                signature: "fn int(min: int, max: int): int",
                is_const: false,
                params: &[("min: int", "Min"), ("max: int", "Max")],
                return_type: "int",
                doc: "Random integer in `[min, max]`.",
            },
            NamespaceMemberSpec {
                name: "float",
                signature: "fn float(): f64",
                is_const: false,
                params: &[],
                return_type: "f64",
                doc: "Random float in `[0.0, 1.0)`.",
            },
            NamespaceMemberSpec {
                name: "choice",
                signature: "fn choice(arr: Array<T>): T",
                is_const: false,
                params: &[("arr: Array<T>", "Non-empty array")],
                return_type: "any",
                doc: "Pick a random element from `arr`.",
            },
        ],
    },
    StdlibModuleSpec {
        name: "Process",
        description: "Subprocess spawning, exit codes, and process metadata.",
        exports: &[
            NamespaceMemberSpec {
                name: "exec",
                signature: "fn exec(cmd: string, args?: Array<string>): object",
                is_const: false,
                params: &[
                    ("cmd: string", "Executable"),
                    ("args?: Array<string>", "Arguments"),
                ],
                return_type: "object",
                doc: "Run a child process and capture output.",
            },
            NamespaceMemberSpec {
                name: "exit",
                signature: "fn exit(code?: int): never",
                is_const: false,
                params: &[("code?: int", "Exit status code")],
                return_type: "never",
                doc: "Terminate the current process.",
            },
            NamespaceMemberSpec {
                name: "pid",
                signature: "fn pid(): int",
                is_const: false,
                params: &[],
                return_type: "int",
                doc: "Return the current process ID.",
            },
        ],
    },
];

pub fn find_builtin_fn(name: &str) -> Option<&'static BuiltinFnSpec> {
    BUILTIN_FUNCTIONS.iter().find(|f| f.name == name)
}

pub fn find_namespace_members(ns: &str) -> Option<&'static [NamespaceMemberSpec]> {
    match ns {
        "Math" => Some(MATH_MEMBERS),
        "time" => Some(TIME_MEMBERS),
        "JSON" => Some(JSON_MEMBERS),
        _ => STDLIB_MODULES
            .iter()
            .find(|m| m.name == ns)
            .map(|m| m.exports),
    }
}

pub fn is_reserved_keyword(word: &str) -> bool {
    matches!(
        word,
        "let"
            | "const"
            | "readonly"
            | "test"
            | "fn"
            | "function"
            | "class"
            | "extend"
            | "raw"
            | "decorator"
            | "if"
            | "else"
            | "elif"
            | "while"
            | "do"
            | "for"
            | "in"
            | "of"
            | "return"
            | "break"
            | "continue"
            | "jump"
            | "try"
            | "catch"
            | "throw"
            | "import"
            | "export"
            | "from"
            | "as"
            | "new"
            | "this"
            | "self"
            | "super"
            | "true"
            | "false"
            | "null"
            | "async"
            | "await"
            | "spawn"
            | "enum"
            | "interface"
            | "extends"
            | "implements"
            | "static"
            | "abstract"
            | "sealed"
            | "type"
            | "match"
            | "struct"
            | "pub"
            | "public"
            | "private"
            | "protected"
            | "mut"
            | "yield"
            | "defer"
            | "where"
            | "is"
            | "typeof"
            | "instanceof"
            | "sizeof"
            | "alignof"
            | "get"
            | "set"
            | "on"
            | "region"
            | "unsafe"
            | "share"
            | "strong"
            | "weak"
            | "extern"
            | "default"
            | "and"
            | "or"
            | "not"
            | "operator"
            | "constructor"
            | "require"
            | "proceed"
            | "alloc"
            | "free"
    )
}

pub fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() || is_reserved_keyword(name) {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Format a `Ty` into an idiomatic Adesh type string (for hover, inlay hints, code actions).
pub fn format_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".to_string(),
        Ty::Float => "f64".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Char => "char".to_string(),
        Ty::Str => "string".to_string(),
        Ty::Null => "null".to_string(),
        Ty::Void => "void".to_string(),
        Ty::Never => "never".to_string(),
        Ty::Any => "any".to_string(),
        Ty::Unknown => "unknown".to_string(),
        Ty::U8 => "u8".to_string(),
        Ty::U16 => "u16".to_string(),
        Ty::U32 => "u32".to_string(),
        Ty::U64 => "u64".to_string(),
        Ty::U128 => "u128".to_string(),
        Ty::I8 => "i8".to_string(),
        Ty::I16 => "i16".to_string(),
        Ty::I32 => "i32".to_string(),
        Ty::I64 => "i64".to_string(),
        Ty::I128 => "i128".to_string(),
        Ty::F32 => "f32".to_string(),
        Ty::F64Ty => "f64".to_string(),
        Ty::Ptr(inner) => format!("*{}", format_ty(inner)),
        Ty::PtrOwning(inner) => format!("own *{}", format_ty(inner)),
        Ty::PtrShared(inner) => format!("&{}", format_ty(inner)),
        Ty::PtrMut(inner) => format!("&mut {}", format_ty(inner)),
        Ty::Array(elem) => format!("Array<{}>", format_ty(elem)),
        Ty::Map(k, v) => format!("Map<{}, {}>", format_ty(k), format_ty(v)),
        Ty::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(format_ty).collect();
            format!("({})", parts.join(", "))
        }
        Ty::Nullable(inner) => format!("{}?", format_ty(inner)),
        Ty::Union(items) => {
            let parts: Vec<String> = items.iter().map(format_ty).collect();
            parts.join(" | ")
        }
        Ty::Record { required, optional } => {
            let mut parts: Vec<String> = required
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_ty(v)))
                .collect();
            parts.extend(
                optional
                    .iter()
                    .map(|(k, v)| format!("{}?: {}", k, format_ty(v))),
            );
            format!("{{ {} }}", parts.join(", "))
        }
        Ty::Class(n) | Ty::Struct(n) | Ty::Interface(n) | Ty::Enum(n) | Ty::GenericParam(n) => {
            n.clone()
        }
        Ty::OptionTy(inner) => format!("Option<{}>", format_ty(inner)),
        Ty::ResultTy(ok, err) => format!("Result<{}, {}>", format_ty(ok), format_ty(err)),
        Ty::GenericInstance { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                let parts: Vec<String> = args.iter().map(format_ty).collect();
                format!("{}<{}>", name, parts.join(", "))
            }
        }
        Ty::Func { params, ret } => {
            let parts: Vec<String> = params.iter().map(format_ty).collect();
            format!("fn({}) -> {}", parts.join(", "), format_ty(ret))
        }
    }
}

/// Returns true if a `Ty` is informative enough to display in inlay hints / hover.
pub fn is_concrete_ty(ty: &Ty) -> bool {
    !matches!(ty, Ty::Any | Ty::Unknown)
}

/// Strip generics, nullability, array brackets, pointers, and reference prefixes from a type annotation.
pub fn base_type_name(type_str: &str) -> &str {
    let mut s = type_str.trim();
    loop {
        let before = s;
        s = s
            .trim_start_matches("&mut ")
            .trim_start_matches('&')
            .trim_start_matches('*')
            .trim_end_matches('?')
            .trim();
        if s.starts_with('[') && s.ends_with(']') && s.len() >= 2 {
            s = s[1..s.len() - 1].trim();
        }
        if s == before {
            break;
        }
    }
    // Unwrap container types like Array<User>, Option<User>, Result<User, Err>, Promise<User>
    if let Some(lt) = s.find('<') {
        let outer = s[..lt].trim();
        if matches!(outer, "Array" | "Option" | "Promise" | "Result" | "Set") {
            if let Some(gt) = s.rfind('>') {
                let inner = &s[lt + 1..gt];
                let first_arg = inner.split(',').next().unwrap_or("").trim();
                if !first_arg.is_empty() {
                    return base_type_name(first_arg);
                }
            }
        }
        return outer;
    }
    s
}

/// Extract the primary nominal/user-defined type name from a `Ty` (unwrapping containers).
pub fn extract_nominal_type_name(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Class(n) | Ty::Struct(n) | Ty::Interface(n) | Ty::Enum(n) => Some(n.clone()),
        Ty::GenericInstance { name, args } => {
            if matches!(
                name.as_str(),
                "Array" | "Option" | "Promise" | "Result" | "Set"
            ) {
                args.first().and_then(extract_nominal_type_name)
            } else {
                Some(name.clone())
            }
        }
        Ty::Array(inner)
        | Ty::Nullable(inner)
        | Ty::OptionTy(inner)
        | Ty::Ptr(inner)
        | Ty::PtrOwning(inner)
        | Ty::PtrShared(inner)
        | Ty::PtrMut(inner) => extract_nominal_type_name(inner),
        Ty::ResultTy(ok, _) => extract_nominal_type_name(ok),
        Ty::Map(_, v) => extract_nominal_type_name(v),
        Ty::Union(items) => items.iter().find_map(extract_nominal_type_name),
        Ty::Func { ret, .. } => extract_nominal_type_name(ret),
        _ => None,
    }
}

/// Resolve a type string to a `Ty`, checking both the current file and the workspace.
pub fn resolve_type_str(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    type_str: &str,
) -> Option<Ty> {
    let trimmed = type_str.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(ty) = index.resolve_type(trimmed) {
        if !matches!(ty, Ty::Any | Ty::Unknown) {
            return Some(ty);
        }
    }
    // Check workspace types directly
    let base = base_type_name(trimmed);
    if let Some(td) = index
        .get_type(base)
        .or_else(|| workspace.find_type_definition(base))
    {
        let base_ty = match td.kind {
            SemanticSymbolKind::Class => Ty::Class(td.name.clone()),
            SemanticSymbolKind::Struct => Ty::Struct(td.name.clone()),
            SemanticSymbolKind::Enum => Ty::Enum(td.name.clone()),
            SemanticSymbolKind::Interface => Ty::Interface(td.name.clone()),
            _ => Ty::GenericInstance {
                name: td.name.clone(),
                args: vec![],
            },
        };
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return Some(Ty::Array(Box::new(base_ty)));
        }
        if trimmed.ends_with('?') {
            return Some(Ty::Nullable(Box::new(base_ty)));
        }
        return Some(base_ty);
    }
    match trimmed {
        "int" | "i32" => Some(Ty::I32),
        "i8" => Some(Ty::I8),
        "i16" => Some(Ty::I16),
        "i64" => Some(Ty::I64),
        "i128" => Some(Ty::I128),
        "u8" => Some(Ty::U8),
        "u16" => Some(Ty::U16),
        "u32" => Some(Ty::U32),
        "u64" => Some(Ty::U64),
        "u128" => Some(Ty::U128),
        "f32" => Some(Ty::F32),
        "f64" | "float" | "number" => Some(Ty::Float),
        "bool" => Some(Ty::Bool),
        "string" | "str" => Some(Ty::Str),
        "char" => Some(Ty::Char),
        "void" => Some(Ty::Void),
        "null" => Some(Ty::Null),
        "never" => Some(Ty::Never),
        _ => index.resolve_type(trimmed),
    }
}

/// Find the enclosing class/struct/extend type name for a 1-based line number.
pub fn enclosing_type_at_line(index: &SemanticIndex, line_1based: usize) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for stmt in &index.statements {
        match &stmt.kind {
            StmtKind::Class(c, _) => {
                if stmt.span.line <= line_1based {
                    best = Some((stmt.span.line, c.name.clone()));
                }
            }
            StmtKind::Extend(_, target, _, _, _) => {
                if stmt.span.line <= line_1based {
                    best = Some((stmt.span.line, target.clone()));
                }
            }
            StmtKind::ExportDefaultClass(c) => {
                if stmt.span.line <= line_1based {
                    best = Some((stmt.span.line, c.name.clone()));
                }
            }
            _ => {}
        }
    }
    // Verify the line actually falls inside the brace block of that declaration if possible
    if let Some((start_line, name)) = best {
        if let Some(end_line) = find_block_end_line(&index.source, start_line) {
            if line_1based <= end_line {
                return Some(name);
            }
        } else {
            return Some(name);
        }
    }
    None
}

/// Find the 1-based closing brace line for a block starting on or after `start_line_1based`.
pub fn find_block_end_line(source: &str, start_line_1based: usize) -> Option<usize> {
    let lines: Vec<&str> = source.lines().collect();
    if start_line_1based == 0 || start_line_1based > lines.len() {
        return None;
    }
    let mut depth = 0i32;
    let mut saw_open = false;
    let mut in_block_comment = false;

    for (idx, line) in lines.iter().enumerate().skip(start_line_1based - 1) {
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut in_string: Option<u8> = None;
        while i < bytes.len() {
            let b = bytes[i];
            if in_block_comment {
                if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if let Some(q) = in_string {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    in_string = None;
                }
                i += 1;
                continue;
            }
            if b == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    break;
                }
                if bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
            }
            if b == b'"' || b == b'\'' || b == b'`' {
                in_string = Some(b);
                i += 1;
                continue;
            }
            if b == b'{' {
                depth += 1;
                saw_open = true;
            } else if b == b'}' && saw_open {
                depth -= 1;
                if depth == 0 {
                    return Some(idx + 1);
                }
            }
            i += 1;
        }
    }
    None
}

/// Extract a doc comment block (`/// ...`, `/** ... */`, or contiguous `// ...`)
/// immediately above a 1-based line number.
pub fn extract_doc_comment(source: &str, decl_line_1based: usize) -> Option<String> {
    if decl_line_1based <= 1 {
        return None;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut idx = decl_line_1based.saturating_sub(2);
    if idx >= lines.len() {
        return None;
    }

    // Skip decorators (`@foo`) right above the declaration
    while idx > 0 && lines[idx].trim().starts_with('@') {
        idx -= 1;
    }
    if lines[idx].trim().starts_with('@') {
        return None;
    }

    let trimmed = lines[idx].trim();
    if trimmed.ends_with("*/") {
        // Block comment /** ... */ or /* ... */
        let mut block = Vec::new();
        let mut cur = idx;
        loop {
            let l = lines[cur].trim();
            block.push(l);
            if l.starts_with("/*") {
                break;
            }
            if cur == 0 {
                return None;
            }
            cur -= 1;
        }
        block.reverse();
        let cleaned: Vec<String> = block
            .into_iter()
            .map(|l| {
                l.trim_start_matches("/**")
                    .trim_start_matches("/*")
                    .trim_end_matches("*/")
                    .trim_start_matches('*')
                    .trim()
                    .to_string()
            })
            .filter(|l| !l.is_empty())
            .collect();
        return if cleaned.is_empty() {
            None
        } else {
            Some(cleaned.join("\n"))
        };
    }

    if trimmed.starts_with("//") {
        let mut comment_lines = Vec::new();
        let mut cur = idx;
        loop {
            let l = lines[cur].trim();
            if let Some(rest) = l.strip_prefix("///") {
                comment_lines.push(rest.trim().to_string());
            } else if let Some(rest) = l.strip_prefix("//") {
                comment_lines.push(rest.trim().to_string());
            } else {
                break;
            }
            if cur == 0 {
                break;
            }
            cur -= 1;
        }
        comment_lines.reverse();
        return if comment_lines.is_empty() {
            None
        } else {
            Some(comment_lines.join("\n"))
        };
    }

    None
}

/// Resolve the type of a chained receiver expression like:
/// `user`, `this`, `super`, `user.address`, `get_user().name`, `users[0]`, `User(1, "A")`.
pub fn resolve_expression_chain_type(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    expr_str: &str,
    line_1based: usize,
) -> Option<Ty> {
    let segments = split_member_chain(expr_str.trim());
    if segments.is_empty() {
        return None;
    }

    let mut current_ty = resolve_root_segment(index, workspace, &segments[0], line_1based)?;

    for seg in &segments[1..] {
        current_ty = resolve_member_step(index, workspace, &current_ty, seg)?;
    }

    Some(current_ty)
}

/// Split a member access expression on top-level `.` / `?.` (respecting `()`, `[]`, strings).
pub fn split_member_chain(expr: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut in_str: Option<char> = None;
    let mut prev_escape = false;

    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            current.push(c);
            if prev_escape {
                prev_escape = false;
            } else if c == '\\' {
                prev_escape = true;
            } else if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                in_str = Some(c);
                current.push(c);
            }
            '(' => {
                paren += 1;
                current.push(c);
            }
            ')' => {
                paren = (paren - 1).max(0);
                current.push(c);
            }
            '[' => {
                bracket += 1;
                current.push(c);
            }
            ']' => {
                bracket = (bracket - 1).max(0);
                current.push(c);
            }
            '{' => {
                brace += 1;
                current.push(c);
            }
            '}' => {
                brace = (brace - 1).max(0);
                current.push(c);
            }
            '?' if paren == 0
                && bracket == 0
                && brace == 0
                && i + 1 < chars.len()
                && chars[i + 1] == '.' =>
            {
                let t = current.trim().to_string();
                if !t.is_empty() {
                    parts.push(t);
                }
                current.clear();
                i += 2;
                continue;
            }
            '.' if paren == 0 && bracket == 0 && brace == 0 => {
                let t = current.trim().to_string();
                if !t.is_empty() {
                    parts.push(t);
                }
                current.clear();
            }
            _ => current.push(c),
        }
        i += 1;
    }

    let t = current.trim().to_string();
    if !t.is_empty() {
        parts.push(t);
    }
    parts
}

fn strip_call_and_index(seg: &str) -> (&str, bool, usize) {
    let mut s = seg.trim();
    let mut index_depth = 0usize;
    while s.ends_with(']') {
        if let Some(open) = find_matching_open(s, '[', ']') {
            s = s[..open].trim();
            index_depth += 1;
        } else {
            break;
        }
    }
    let mut is_call = false;
    if s.ends_with(')') {
        if let Some(open) = find_matching_open(s, '(', ')') {
            s = s[..open].trim();
            is_call = true;
        }
    }
    // Strip trailing ! or ? (non-null / try)
    s = s.trim_end_matches('!').trim_end_matches('?').trim();
    // Strip leading `new ` or `await `
    s = s
        .strip_prefix("await ")
        .unwrap_or(s)
        .strip_prefix("new ")
        .unwrap_or(s)
        .trim();
    (s, is_call, index_depth)
}

fn find_matching_open(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, c) in s.char_indices().rev() {
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn unwrap_index(mut ty: Ty, index_depth: usize) -> Option<Ty> {
    for _ in 0..index_depth {
        ty = match ty {
            Ty::Array(elem) => *elem,
            Ty::Map(_, val) => *val,
            Ty::Str => Ty::Char,
            Ty::GenericInstance { name, args } if name == "Array" && !args.is_empty() => {
                args[0].clone()
            }
            Ty::GenericInstance { name, args } if name == "Map" && args.len() >= 2 => {
                args[1].clone()
            }
            Ty::Tuple(elems) if !elems.is_empty() => elems[0].clone(),
            _ => return Some(Ty::Any),
        };
    }
    Some(ty)
}

fn resolve_root_segment(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    seg: &str,
    line_1based: usize,
) -> Option<Ty> {
    let trimmed = seg.trim();

    // String literal receiver: `"hello".`
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('`') && trimmed.ends_with('`'))
    {
        return Some(Ty::Str);
    }
    // Array literal receiver: `[1, 2, 3].`
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(Ty::Array(Box::new(Ty::Any)));
    }

    let (ident, is_call, index_depth) = strip_call_and_index(trimmed);
    if ident.is_empty() {
        return None;
    }

    // `this` / `self` / `super` inside a class or extend block
    if ident == "this" || ident == "self" {
        if let Some(cls) = enclosing_type_at_line(index, line_1based) {
            return unwrap_index(Ty::Class(cls), index_depth);
        }
    }
    if ident == "super" {
        if let Some(cls) = enclosing_type_at_line(index, line_1based) {
            if let Some(td) = index
                .get_type(&cls)
                .or_else(|| workspace.find_type_definition(&cls))
            {
                if let Some(parent) = &td.extends {
                    return unwrap_index(Ty::Class(parent.clone()), index_depth);
                }
            }
        }
    }

    // Direct call to a constructor or function
    if is_call {
        // Constructor call: `User(...)`
        if let Some(td) = index
            .get_type(ident)
            .or_else(|| workspace.find_type_definition(ident))
        {
            let ty = match td.kind {
                SemanticSymbolKind::Class => Ty::Class(td.name.clone()),
                SemanticSymbolKind::Struct => Ty::Struct(td.name.clone()),
                SemanticSymbolKind::Enum => Ty::Enum(td.name.clone()),
                SemanticSymbolKind::Interface => Ty::Interface(td.name.clone()),
                _ => Ty::Class(td.name.clone()),
            };
            return unwrap_index(ty, index_depth);
        }
        // User function in current file or workspace
        if let Some(sym) = index
            .find_declaration(ident)
            .or_else(|| workspace.find_declaration_workspace(ident).map(|(_, s)| s))
        {
            if let Some(ret) = &sym.return_type {
                if let Some(ty) = resolve_type_str(index, workspace, ret) {
                    return unwrap_index(ty, index_depth);
                }
            }
            if let Some(ty) = &sym.ty {
                if let Ty::Func { ret, .. } = ty {
                    return unwrap_index((**ret).clone(), index_depth);
                }
                return unwrap_index(ty.clone(), index_depth);
            }
        }
        // Builtin function return type
        if let Some(b) = find_builtin_fn(ident) {
            if let Some(ty) = resolve_type_str(index, workspace, b.return_type) {
                return unwrap_index(ty, index_depth);
            }
        }
    }

    // Scope-aware variable/parameter lookup in current file
    if let Some(sym) = find_best_declaration_for_line(index, ident, line_1based) {
        if let Some(ty) = symbol_resolved_type(index, workspace, sym) {
            return unwrap_index(ty, index_depth);
        }
    }

    // Fallback to semantic engine's resolve_symbol_type
    if let Some(ty) = index.resolve_symbol_type(ident) {
        return unwrap_index(ty, index_depth);
    }

    // Cross-file variable/symbol
    if let Some((_, sym)) = workspace.find_declaration_workspace(ident) {
        if let Some(ty) = symbol_resolved_type(index, workspace, sym) {
            return unwrap_index(ty, index_depth);
        }
    }

    // Recover constructor-inferred bindings while the user is still typing arguments.
    // The recovering parser can omit `let u = User(1, );` entirely.
    for tokens in index.tokens.windows(5).rev() {
        if tokens[0].line > line_1based {
            continue;
        }
        if matches!(tokens[0].lexeme.as_str(), "let" | "const" | "readonly")
            && tokens[1].lexeme == ident
            && tokens[2].lexeme == "="
            && tokens[4].lexeme == "("
        {
            let name = &tokens[3].lexeme;
            if let Some(td) = index
                .get_type(name)
                .or_else(|| workspace.find_type_definition(name))
            {
                let ty = match td.kind {
                    SemanticSymbolKind::Class => Ty::Class(name.clone()),
                    SemanticSymbolKind::Struct => Ty::Struct(name.clone()),
                    _ => continue,
                };
                return unwrap_index(ty, index_depth);
            }
        }
    }

    // Static access on a type name (`User.`, `Color.`)
    if let Some(td) = index
        .get_type(ident)
        .or_else(|| workspace.find_type_definition(ident))
    {
        let ty = match td.kind {
            SemanticSymbolKind::Class => Ty::Class(td.name.clone()),
            SemanticSymbolKind::Struct => Ty::Struct(td.name.clone()),
            SemanticSymbolKind::Enum => Ty::Enum(td.name.clone()),
            SemanticSymbolKind::Interface => Ty::Interface(td.name.clone()),
            _ => Ty::GenericInstance {
                name: td.name.clone(),
                args: vec![],
            },
        };
        return unwrap_index(ty, index_depth);
    }

    None
}

fn resolve_member_step(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    receiver_ty: &Ty,
    seg: &str,
) -> Option<Ty> {
    let (member_name, _is_call, index_depth) = strip_call_and_index(seg);
    if member_name.is_empty() {
        return None;
    }

    // Unwrap nullable/pointer/option for member access (`user?.name`)
    let unwrapped = match receiver_ty {
        Ty::Nullable(inner)
        | Ty::OptionTy(inner)
        | Ty::Ptr(inner)
        | Ty::PtrOwning(inner)
        | Ty::PtrShared(inner)
        | Ty::PtrMut(inner) => inner.as_ref(),
        other => other,
    };

    match unwrapped {
        Ty::Str => {
            if let Some(spec) = STRING_METHODS.iter().find(|m| m.name == member_name) {
                if let Some(ty) = resolve_type_str(index, workspace, spec.return_type) {
                    return unwrap_index(ty, index_depth);
                }
            }
            None
        }
        Ty::Array(elem) => {
            if matches!(member_name, "pop" | "first" | "last" | "find") {
                return unwrap_index((**elem).clone(), index_depth);
            }
            if matches!(member_name, "filter" | "slice" | "reverse" | "sort") {
                return unwrap_index(Ty::Array(elem.clone()), index_depth);
            }
            if let Some(spec) = ARRAY_METHODS.iter().find(|m| m.name == member_name) {
                if let Some(ty) = resolve_type_str(index, workspace, spec.return_type) {
                    return unwrap_index(ty, index_depth);
                }
            }
            None
        }
        Ty::Record { required, optional } => {
            for (k, v) in required.iter().chain(optional.iter()) {
                if k == member_name {
                    return unwrap_index(v.clone(), index_depth);
                }
            }
            None
        }
        _ => {
            let type_name = extract_nominal_type_name(unwrapped)?;
            let members = get_type_members_workspace(index, workspace, &type_name);
            for m in members {
                if m.name == member_name {
                    let resolved = m
                        .return_type
                        .as_deref()
                        .or(m.type_annotation.as_deref())
                        .and_then(|s| resolve_type_str(index, workspace, s))
                        .or(m.ty.clone());
                    if let Some(ty) = resolved {
                        return unwrap_index(ty, index_depth);
                    }
                    // Enum variant belongs to the enum type
                    if m.kind == SemanticSymbolKind::EnumVariant {
                        return unwrap_index(Ty::Enum(type_name), index_depth);
                    }
                }
            }
            None
        }
    }
}

/// Get all members of a type from either the current file or anywhere in the workspace,
/// including inherited members via `extends`.
pub fn get_type_members_workspace(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    type_name: &str,
) -> Vec<TypeMember> {
    let mut members = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut current = Some(type_name.to_string());

    while let Some(name) = current {
        if !visited.insert(name.clone()) {
            break;
        }
        if index.get_type(&name).is_some() {
            for m in index.get_type_members(&name) {
                if !members
                    .iter()
                    .any(|existing: &TypeMember| existing.name == m.name)
                {
                    members.push(m);
                }
            }
            current = index.get_type(&name).and_then(|td| td.extends.clone());
        } else if let Some(ws_members) = workspace.get_type_members(&name) {
            for m in ws_members {
                if !members
                    .iter()
                    .any(|existing: &TypeMember| existing.name == m.name)
                {
                    members.push(m);
                }
            }
            current = workspace
                .find_type_definition(&name)
                .and_then(|td| td.extends.clone());
        } else {
            break;
        }
    }

    members
}

/// Resolve a symbol's effective `Ty` from its annotation, cached `ty`, or `resolve_symbol_type`.
pub fn symbol_resolved_type(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    sym: &SymbolEntry,
) -> Option<Ty> {
    if let Some(ann) = &sym.type_annotation {
        if let Some(ty) = resolve_type_str(index, workspace, ann) {
            if is_concrete_ty(&ty) {
                return Some(ty);
            }
        }
    }
    if let Some(ty) = &sym.ty {
        if is_concrete_ty(ty) {
            return Some(ty.clone());
        }
    }
    if let Some(ret) = &sym.return_type {
        if matches!(
            sym.kind,
            SemanticSymbolKind::Function
                | SemanticSymbolKind::Method
                | SemanticSymbolKind::StaticMethod
        ) {
            if let Some(ty) = resolve_type_str(index, workspace, ret) {
                return Some(ty);
            }
        }
    }
    index.resolve_symbol_type(&sym.name).filter(is_concrete_ty)
}

/// Find the most relevant declaration of `name` visible at `line_1based` in the current file.
/// Prefers the nearest preceding local declaration in the same enclosing function over a
/// top-level declaration or one in another function.
pub fn find_best_declaration_for_line<'a>(
    index: &'a SemanticIndex,
    name: &str,
    line_1based: usize,
) -> Option<&'a SymbolEntry> {
    let matches: Vec<&SymbolEntry> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.parent_type.is_none())
        .collect();

    if matches.is_empty() {
        return index.find_declaration(name);
    }
    if matches.len() == 1 {
        return Some(matches[0]);
    }

    // Find enclosing function span, if any
    let enclosing_fn = enclosing_function_span(index, line_1based);

    if let Some((fn_start, fn_end)) = enclosing_fn {
        // Prefer declaration inside the same function on or before `line_1based`
        if let Some(local) = matches
            .iter()
            .filter(|s| s.line >= fn_start && s.line <= fn_end && s.line <= line_1based)
            .max_by_key(|s| s.line)
        {
            return Some(local);
        }
    }

    // Otherwise prefer top-level declaration (not inside another function)
    let fn_spans = all_function_spans(index);
    if let Some(top) = matches
        .iter()
        .find(|s| !fn_spans.iter().any(|&(a, b)| s.line > a && s.line <= b))
    {
        return Some(top);
    }

    // Fallback: closest preceding match
    matches
        .iter()
        .filter(|s| s.line <= line_1based)
        .max_by_key(|s| s.line)
        .copied()
        .or_else(|| matches.first().copied())
}

/// Return `(start_line_1based, end_line_1based)` of the enclosing function/method at `line_1based`.
pub fn enclosing_function_span(
    index: &SemanticIndex,
    line_1based: usize,
) -> Option<(usize, usize)> {
    let spans = all_function_spans(index);
    spans
        .into_iter()
        .filter(|&(start, end)| line_1based >= start && line_1based <= end)
        .min_by_key(|&(start, end)| end.saturating_sub(start))
}

/// Collect `(start_line_1based, end_line_1based)` for every function and method in the file.
pub fn all_function_spans(index: &SemanticIndex) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    collect_fn_spans(&index.statements, &index.source, &mut spans);
    spans
}

fn collect_fn_spans(stmts: &[Stmt], source: &str, out: &mut Vec<(usize, usize)>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f, _) | StmtKind::ExportDefaultFunction(f) => {
                let start = stmt.span.line;
                let end = find_block_end_line(source, start)
                    .or_else(|| f.body.last().map(|s| s.span.line + 1))
                    .unwrap_or(start);
                out.push((start, end));
                collect_fn_spans(&f.body, source, out);
            }
            StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
                for m in c.methods.iter().chain(c.static_methods.iter()) {
                    push_method_span(m, source, out);
                }
            }
            StmtKind::Extend(_, _, _, methods, _) => {
                for m in methods {
                    push_method_span(m, source, out);
                }
            }
            StmtKind::Block(inner) => collect_fn_spans(inner, source, out),
            _ => {}
        }
    }
}

fn push_method_span(m: &Function, source: &str, out: &mut Vec<(usize, usize)>) {
    if let Some(first) = m.body.first() {
        let start = first.span.line.saturating_sub(1).max(1);
        let end = find_block_end_line(source, start)
            .or_else(|| m.body.last().map(|s| s.span.line + 1))
            .unwrap_or(start);
        out.push((start, end));
        collect_fn_spans(&m.body, source, out);
    }
}

/// Extract the expression immediately before the `.` that precedes a member access at `(line, col)` (0-based).
/// Returns `Some((receiver_expr, member_name))` when cursor is on `member` in `receiver.member`.
pub fn extract_member_access_at(doc: &Document, line: u32, col: u32) -> Option<(String, String)> {
    let line_text = doc.get_line(line as usize)?;
    let byte_col = Document::utf16_col_to_byte_offset(line_text, col);

    // Find boundaries of the identifier at cursor
    let start = line_text[..byte_col]
        .char_indices()
        .rev()
        .find(|&(_, c)| !c.is_alphanumeric() && c != '_')
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let end = line_text[byte_col..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| byte_col + i)
        .unwrap_or(line_text.len());

    if start >= end {
        return None;
    }
    let member = line_text[start..end].to_string();

    // Check if preceded by `.` or `?.`
    let before = line_text[..start].trim_end();
    let before_dot = if let Some(s) = before.strip_suffix("?.") {
        s
    } else if let Some(s) = before.strip_suffix('.') {
        s
    } else {
        return None;
    };

    let receiver = extract_trailing_receiver(before_dot);
    if receiver.is_empty() {
        return None;
    }
    Some((receiver, member))
}

/// Extract the trailing expression immediately before a `.` (e.g., `foo(bar).baz[0]`).
pub fn extract_trailing_receiver(before_dot: &str) -> String {
    let trimmed = before_dot.trim_end();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut i = chars.len();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut in_str: Option<char> = None;

    while i > 0 {
        i -= 1;
        let c = chars[i];
        if let Some(q) = in_str {
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => in_str = Some(c),
            ')' => paren += 1,
            '(' => {
                if paren > 0 {
                    paren -= 1;
                } else {
                    return chars[i + 1..].iter().collect::<String>().trim().to_string();
                }
            }
            ']' => bracket += 1,
            '[' => {
                if bracket > 0 {
                    bracket -= 1;
                } else {
                    return chars[i + 1..].iter().collect::<String>().trim().to_string();
                }
            }
            _ if paren == 0 && bracket == 0 => {
                let allowed = c.is_alphanumeric() || c == '_' || c == '.' || c == '?' || c == '!';
                if !allowed {
                    return chars[i + 1..].iter().collect::<String>().trim().to_string();
                }
            }
            _ => {}
        }
    }

    trimmed.to_string()
}

/// Compute Levenshtein edit distance between two strings (case-insensitive option).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca.eq_ignore_ascii_case(&cb) { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Find the closest candidate name to `needle` within a reasonable edit distance threshold.
pub fn find_closest_match<'a>(
    needle: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<String> {
    if needle.len() < 2 {
        return None;
    }
    let max_dist = match needle.len() {
        2..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    let mut best: Option<(usize, String)> = None;
    for cand in candidates {
        if cand == needle || cand.is_empty() {
            continue;
        }
        let d = levenshtein(needle, cand);
        if d <= max_dist {
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, cand.to_string()));
            }
        }
    }
    best.map(|(_, s)| s)
}

/// Infer the return type of a function body when no explicit return type annotation is present.
pub fn infer_function_return_type(index: &SemanticIndex, func: &Function) -> Option<Ty> {
    let mut ret_types = Vec::new();
    collect_return_expr_types(index, &func.body, &mut ret_types);
    if ret_types.is_empty() {
        return None;
    }
    let first = ret_types[0].clone();
    if ret_types.iter().all(|t| t == &first) && is_concrete_ty(&first) {
        Some(first)
    } else {
        ret_types.into_iter().find(is_concrete_ty)
    }
}

fn collect_return_expr_types(index: &SemanticIndex, stmts: &[Stmt], out: &mut Vec<Ty>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                let ty = index.infer_type(expr);
                if is_concrete_ty(&ty) {
                    out.push(ty);
                }
            }
            StmtKind::Block(inner) => collect_return_expr_types(index, inner, out),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_return_expr_types(index, std::slice::from_ref(then_branch.as_ref()), out);
                if let Some(e) = else_branch {
                    collect_return_expr_types(index, std::slice::from_ref(e.as_ref()), out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::ForIn { body, .. } => {
                collect_return_expr_types(index, std::slice::from_ref(body.as_ref()), out);
            }
            StmtKind::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                collect_return_expr_types(index, std::slice::from_ref(try_block.as_ref()), out);
                collect_return_expr_types(index, std::slice::from_ref(catch_block.as_ref()), out);
            }
            _ => {}
        }
    }
}

/// Walk every expression inside a slice of statements with a callback.
pub fn visit_exprs_in_stmts<F: FnMut(&Expr)>(stmts: &[Stmt], f: &mut F) {
    for stmt in stmts {
        visit_exprs_in_stmt(stmt, f);
    }
}

pub fn visit_exprs_in_stmt<F: FnMut(&Expr)>(stmt: &Stmt, f: &mut F) {
    match &stmt.kind {
        StmtKind::Let(_, init, _, _, _, _)
        | StmtKind::LetTuple(_, _, init, _, _, _)
        | StmtKind::LetObject(_, init, _, _, _) => {
            if let Some(e) = init {
                visit_expr(e, f);
            }
        }
        StmtKind::ShareDeclaration(d, _) => visit_expr(&d.expr, f),
        StmtKind::StrongDeclaration(d, _) => visit_expr(&d.expr, f),
        StmtKind::WeakDeclaration(d, _) => visit_expr(&d.expr, f),
        StmtKind::ExprStmt(e) | StmtKind::Jump(e) => visit_expr(e, f),
        StmtKind::Return(Some(e)) => visit_expr(e, f),
        StmtKind::Block(inner) => visit_exprs_in_stmts(inner, f),
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            visit_expr(cond, f);
            visit_exprs_in_stmt(then_branch, f);
            if let Some(e) = else_branch {
                visit_exprs_in_stmt(e, f);
            }
        }
        StmtKind::While { cond, body } => {
            visit_expr(cond, f);
            visit_exprs_in_stmt(body, f);
        }
        StmtKind::ForIn { iter, body, .. } => {
            visit_expr(iter, f);
            visit_exprs_in_stmt(body, f);
        }
        StmtKind::Function(func, _) | StmtKind::ExportDefaultFunction(func) => {
            visit_exprs_in_stmts(&func.body, f);
        }
        StmtKind::Class(c, _) | StmtKind::ExportDefaultClass(c) => {
            for m in c.methods.iter().chain(c.static_methods.iter()) {
                visit_exprs_in_stmts(&m.body, f);
            }
        }
        StmtKind::Extend(_, _, _, methods, _) => {
            for m in methods {
                visit_exprs_in_stmts(&m.body, f);
            }
        }
        StmtKind::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            visit_exprs_in_stmt(try_block, f);
            visit_exprs_in_stmt(catch_block, f);
        }
        StmtKind::Region { body, .. } => visit_exprs_in_stmt(body, f),
        StmtKind::UnsafeBlock(body) | StmtKind::Defer(body) => visit_exprs_in_stmt(body, f),
        _ => {}
    }
}

pub fn visit_expr<F: FnMut(&Expr)>(expr: &Expr, f: &mut F) {
    f(expr);
    match &expr.kind {
        ExprKind::Assign(_, rhs) => visit_expr(rhs, f),
        ExprKind::AssignOp(lhs, _, rhs)
        | ExprKind::Binary(lhs, _, rhs)
        | ExprKind::Logical(lhs, _, rhs)
        | ExprKind::Index(lhs, rhs)
        | ExprKind::Range(lhs, rhs, _) => {
            visit_expr(lhs, f);
            visit_expr(rhs, f);
        }
        ExprKind::AssignTuple(_, rhs)
        | ExprKind::AssignObject(_, rhs)
        | ExprKind::Unary(_, rhs)
        | ExprKind::Grouping(rhs)
        | ExprKind::Await(rhs)
        | ExprKind::Spawn(rhs)
        | ExprKind::Throw(rhs)
        | ExprKind::Spread(rhs)
        | ExprKind::NonNull(rhs)
        | ExprKind::Try(rhs)
        | ExprKind::Update(_, _, rhs)
        | ExprKind::Format(rhs, _)
        | ExprKind::Cast(rhs, _)
        | ExprKind::Get(rhs, _)
        | ExprKind::OptGet(rhs, _) => visit_expr(rhs, f),
        ExprKind::Set(obj, _, val) => {
            visit_expr(obj, f);
            visit_expr(val, f);
        }
        ExprKind::Call(callee, args, _) | ExprKind::OptCall(callee, args, _) => {
            visit_expr(callee, f);
            for a in args {
                visit_expr(a, f);
            }
        }
        ExprKind::New(callee, args) => {
            visit_expr(callee, f);
            for a in args {
                visit_expr(a, f);
            }
        }
        ExprKind::Array(elems) | ExprKind::Tuple(elems) | ExprKind::SetLiteral(elems) => {
            for e in elems {
                visit_expr(e, f);
            }
        }
        ExprKind::Object(fields) | ExprKind::StructLiteral(_, fields) => {
            for (_, v) in fields {
                visit_expr(v, f);
            }
        }
        ExprKind::Fn(_, body, _) => visit_exprs_in_stmts(body, f),
        ExprKind::Conditional(c, t, e) => {
            visit_expr(c, f);
            visit_expr(t, f);
            visit_expr(e, f);
        }
        ExprKind::Match(scrutinee, arms) => {
            visit_expr(scrutinee, f);
            for (_, arm_expr) in arms {
                visit_expr(arm_expr, f);
            }
        }
        ExprKind::Literal(_) | ExprKind::Variable(_) => {}
    }
}
