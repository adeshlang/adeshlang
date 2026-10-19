//! AdeshLang Authoritative Diagnostic Registry
//!
//! Canonical, machine-readable registry of diagnostic error codes used by AdeshLang.
//! Maps verified Rust-equivalent errors to official Rust E#### numbers and allocates
//! the distinct `AD####` namespace for Adesh-specific compiler/runtime/linker diagnostics.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticPhase {
    Lexer,
    Parser,
    Semantic,
    TypeCheck,
    Ownership,
    Lowering,
    Codegen,
    Linker,
    Runtime,
    Cli,
}

impl DiagnosticPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticPhase::Lexer => "lexer",
            DiagnosticPhase::Parser => "parser",
            DiagnosticPhase::Semantic => "semantic",
            DiagnosticPhase::TypeCheck => "typecheck",
            DiagnosticPhase::Ownership => "ownership",
            DiagnosticPhase::Lowering => "lowering",
            DiagnosticPhase::Codegen => "codegen",
            DiagnosticPhase::Linker => "linker",
            DiagnosticPhase::Runtime => "runtime",
            DiagnosticPhase::Cli => "cli",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RustCompatibility {
    /// Semantically identical to the Rust compiler error code with the same number.
    Equivalent,
    /// Adesh-specific diagnostic with no direct Rust equivalent.
    AdeshSpecific,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticInfo {
    pub code: &'static str,
    pub title: &'static str,
    pub severity: DiagnosticSeverity,
    pub phase: DiagnosticPhase,
    pub compatibility: RustCompatibility,
    pub short_description: &'static str,
    pub detailed_explanation: &'static str,
    pub common_causes: &'static [&'static str],
    pub suggested_fixes: &'static [&'static str],
    pub bad_example: Option<&'static str>,
    pub good_example: Option<&'static str>,
    pub semantic_justification: Option<&'static str>,
}

pub static DIAGNOSTIC_REGISTRY: &[DiagnosticInfo] = &[
    // -------------------------------------------------------------
    // Rust-Compatible Codes (E####)
    // -------------------------------------------------------------
    DiagnosticInfo {
        code: "E0106",
        title: "Missing or invalid lifetime / reference outlives owner",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A reference or borrowed value may outlive the owner that created it.",
        detailed_explanation: "This error occurs when attempting to return or store a reference to a value whose scope or lifetime terminates before the reference is finished being used.",
        common_causes: &[
            "Returning a local stack variable by reference.",
            "Storing a short-lived borrow in an outer or longer-lived structure.",
        ],
        suggested_fixes: &[
            "Return an owned value instead of a borrowed reference.",
            "Use `share` / `strong` for shared ARC ownership instead of a short-lived borrow.",
            "Extend the owner's lifetime to outlive the reference.",
        ],
        bad_example: Some(
            "fn bad() -> ptr<int> {\n    let x: int = 42;\n    return &x; // error[E0106]\n}",
        ),
        good_example: Some("fn good() -> int {\n    let x: int = 42;\n    return x;\n}"),
        semantic_justification: Some(
            "Equivalent to Rust E0106: reference escapes owner scope / missing lifetime specifier.",
        ),
    },
    DiagnosticInfo {
        code: "E0277",
        title: "Trait bound not satisfied / concurrency constraint violated",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::TypeCheck,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A type does not implement the required trait or capability (such as Send or Sync).",
        detailed_explanation: "In AdeshLang, passing values across thread boundaries or executing parallel operations requires the types to implement concurrency safety traits like Send or Sync.",
        common_causes: &[
            "Passing an un-synchronized mutable reference across a spawned thread boundary.",
            "Attempting to share non-thread-safe state across concurrent tasks.",
        ],
        suggested_fixes: &[
            "Ensure exclusive ownership or synchronize access with mutex or channel.",
            "Use ARC (`share` / `strong`) designed for concurrent thread sharing.",
        ],
        bad_example: Some(
            "let raw_ptr: *u8 = alloc(10);\nspawn {\n    *raw_ptr = 1; // error[E0277]\n};",
        ),
        good_example: Some(
            "share s = alloc(10);\nstrong t = s;\nspawn {\n    // access via thread-safe shared handle\n};",
        ),
        semantic_justification: Some(
            "Equivalent to Rust E0277: a required trait bound (e.g. Send/Sync) is not satisfied for the given type.",
        ),
    },
    DiagnosticInfo {
        code: "E0308",
        title: "Mismatched types",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::TypeCheck,
        compatibility: RustCompatibility::Equivalent,
        short_description: "An expression's type does not match the expected type in its context.",
        detailed_explanation: "AdeshLang is statically typed. An assignment, function argument, or return statement must provide a value whose type conforms to the declared or inferred target type.",
        common_causes: &[
            "Assigning a string or float value to an integer variable.",
            "Returning a value of type A when the function signature specifies type B.",
            "Passing an incompatible argument type to a function call.",
        ],
        suggested_fixes: &[
            "Change the variable or parameter type annotation to match the value.",
            "Convert or cast the expression to the expected type.",
            "Provide a value of the expected type.",
        ],
        bad_example: Some("let count: int = \"forty-two\"; // error[E0308]"),
        good_example: Some("let count: int = 42;"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0308: mismatched expected and actual types.",
        ),
    },
    DiagnosticInfo {
        code: "E0381",
        title: "Use of possibly uninitialized variable",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A variable is read or used before it is guaranteed to have been assigned a value.",
        detailed_explanation: "AdeshLang requires all variables to be definitely initialized before they can be read. Reading uninitialized memory is unsafe.",
        common_causes: &[
            "Declaring a variable with `let x: int;` and using it without assignment.",
            "Initializing a variable only inside one branch of an `if` statement.",
        ],
        suggested_fixes: &[
            "Initialize the variable when declared: `let x = 0;`.",
            "Ensure all conditional branches assign a value before the variable is read.",
        ],
        bad_example: Some("let x: int;\nif condition {\n    x = 10;\n}\nprint(x); // error[E0381]"),
        good_example: Some("let x: int = 0;\nif condition {\n    x = 10;\n}\nprint(x);"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0381: variable used before definite assignment/initialization.",
        ),
    },
    DiagnosticInfo {
        code: "E0382",
        title: "Use of moved value",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A variable was accessed after its ownership was transferred (moved).",
        detailed_explanation: "In AdeshLang, ownership is exclusive by default. When an owning handle is assigned to another variable or passed by value, ownership moves and the original identifier cannot be used again.",
        common_causes: &[
            "Using a variable after assigning it to another variable: `let y = x; print(x);`.",
            "Passing a variable by value to a function that takes ownership, then using it again.",
            "Moving a value inside a loop without re-initializing it on subsequent iterations.",
        ],
        suggested_fixes: &[
            "Perform all operations on the value before moving it.",
            "Share ownership using ARC: `share x = val;` then `strong alias = x;`.",
            "Use a `weak` reference for non-owning inspection.",
        ],
        bad_example: Some(
            "let a = alloc(64);\nlet b = a; // ownership moves to b\nlet c = a; // error[E0382]: use of moved value `a`",
        ),
        good_example: Some(
            "share a = alloc(64);\nstrong b = a;\nstrong c = a;\n// Both b and c share ownership safely via ARC",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0382: access to value after ownership transfer (move).",
        ),
    },
    DiagnosticInfo {
        code: "E0384",
        title: "Cannot assign twice to immutable variable",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::TypeCheck,
        compatibility: RustCompatibility::Equivalent,
        short_description: "An immutable variable or constant was assigned to more than once.",
        detailed_explanation: "Variables declared without explicit mutability are immutable once initialized. Reassignment is prohibited.",
        common_causes: &[
            "Assigning a new value to an immutable `let` or `const` binding.",
            "Re-freeing an already freed resource or double-free.",
        ],
        suggested_fixes: &[
            "Declare the variable as mutable if modification is intended.",
            "Define a new variable with a distinct name rather than mutating an immutable binding.",
        ],
        bad_example: Some("let x = 10;\nx = 20; // error[E0384]"),
        good_example: Some("let mut x = 10;\nx = 20;"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0384: re-assignment to immutable variable.",
        ),
    },
    DiagnosticInfo {
        code: "E0416",
        title: "Use after free / duplicate binding in scope",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "An operation attempted to access memory or a binding after it was freed or invalidated.",
        detailed_explanation: "Explicit `free(x)` invalidates the memory backing `x`. Any subsequent access constitutes undefined behavior and is rejected at compile time.",
        common_causes: &[
            "Calling `free(ptr)` and subsequently reading or writing `*ptr`.",
            "Duplicate identifiers in the same pattern or binding list.",
        ],
        suggested_fixes: &[
            "Keep the resource alive until after its last use before freeing.",
            "Rely on AdeshLang RAII scopes (`region` or automatic block drops) instead of manual free.",
        ],
        bad_example: Some("let ptr = alloc(32);\nfree(ptr);\nlet val = *ptr; // error[E0416]"),
        good_example: Some("let ptr = alloc(32);\nlet val = *ptr;\nfree(ptr);"),
        semantic_justification: Some(
            "Semantic equivalent to Rust E0416: identifier invalidated or bound multiple times in the same pattern/scope.",
        ),
    },
    DiagnosticInfo {
        code: "E0425",
        title: "Cannot find value, function, or module in this scope",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Semantic,
        compatibility: RustCompatibility::Equivalent,
        short_description: "An identifier was used that has not been declared or imported in scope.",
        detailed_explanation: "The compiler encountered a variable, function, or module name that cannot be resolved in any enclosing lexical scope or imported namespace.",
        common_causes: &[
            "Misspelled variable or function name.",
            "Missing `let` or `fn` declaration.",
            "Using a variable outside of the block or function where it was declared.",
            "Missing `import` statement.",
        ],
        suggested_fixes: &[
            "Check the spelling of the identifier.",
            "Declare the name before using it: `let x = ...`.",
            "Import the module containing the desired symbol.",
        ],
        bad_example: Some("fn main() {\n    print(undeclared_value); // error[E0425]\n}"),
        good_example: Some(
            "fn main() {\n    let undeclared_value = 100;\n    print(undeclared_value);\n}",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0425: unresolved identifier in current scope.",
        ),
    },
    DiagnosticInfo {
        code: "E0428",
        title: "Duplicate symbol declaration",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Semantic,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A name is already defined in the current scope.",
        detailed_explanation: "AdeshLang does not allow declaring two functions, types, or variables with the identical name in the same lexical scope.",
        common_causes: &[
            "Declaring two functions with the same name in the same file.",
            "Declaring two classes or structs with the same name.",
            "Copy-pasting declarations without renaming them.",
        ],
        suggested_fixes: &[
            "Rename one of the declarations to be unique.",
            "Remove or merge redundant definitions.",
        ],
        bad_example: Some("fn run() {}\nfn run() {} // error[E0428]"),
        good_example: Some("fn run_first() {}\nfn run_second() {}"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0428: a value or type with this name is already defined in this module.",
        ),
    },
    DiagnosticInfo {
        code: "E0499",
        title: "Cannot borrow as mutable more than once at a time",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "Multiple exclusive (mutable) borrows of the same variable overlap.",
        detailed_explanation: "To eliminate data races and iterator invalidation, AdeshLang allows only one active exclusive borrow (`&mut`) of a given value at any single point in time.",
        common_causes: &[
            "Taking two mutable references to the same object simultaneously.",
            "Calling a method requiring mutable access while holding a mutable borrow.",
        ],
        suggested_fixes: &[
            "Narrow the scope of the first mutable borrow so it finishes before the second begins.",
            "Restructure data access to operate sequentially.",
        ],
        bad_example: Some("let mut x = 10;\nlet r1 = &mut x;\nlet r2 = &mut x; // error[E0499]"),
        good_example: Some(
            "let mut x = 10;\n{\n    let r1 = &mut x;\n    *r1 += 1;\n}\nlet r2 = &mut x;\n*r2 += 2;",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0499: multiple mutable borrows of the same place concurrently.",
        ),
    },
    DiagnosticInfo {
        code: "E0502",
        title: "Cannot borrow as mutable because it is also borrowed as immutable",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "Cannot take an exclusive borrow of a variable while shared borrows remain active.",
        detailed_explanation: "AdeshLang enforces the aliasing XOR mutability invariant: you can have either any number of immutable borrows (`&T`) or exactly one mutable borrow (`&mut T`), but never both at once.",
        common_causes: &[
            "Mutating an object or taking `&mut` while an immutable reference is still being read.",
            "Passing an object by mutable reference to a function while reading from it in another argument.",
        ],
        suggested_fixes: &[
            "Ensure the immutable borrow scope finishes before taking the mutable borrow.",
            "Clone or copy data if read access must continue alongside mutation.",
        ],
        bad_example: Some(
            "let mut x = [1, 2, 3];\nlet item = &x[0];\nx.push(4); // error[E0502]: cannot borrow `x` as mutable\nprint(*item);",
        ),
        good_example: Some(
            "let mut x = [1, 2, 3];\nlet val = x[0]; // copy/read value\nx.push(4);\nprint(val);",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0502: conflicting shared and mutable borrows active concurrently.",
        ),
    },
    DiagnosticInfo {
        code: "E0505",
        title: "Cannot move or free value while borrowed",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A value cannot be moved or freed while active references point to it.",
        detailed_explanation: "Moving or freeing a value invalidates its existing memory location. If references to that location are still active, they would become dangling pointers.",
        common_causes: &[
            "Calling `free(x)` while a borrow of `x` is still active.",
            "Passing `x` by value (moving it) while holding a reference to `x`.",
        ],
        suggested_fixes: &[
            "Complete all uses of borrowed references before moving or freeing the owner.",
            "Place the move after the borrow scope has exited.",
        ],
        bad_example: Some(
            "let x = alloc(16);\nlet r = &x;\nfree(x); // error[E0505]: cannot free `x` because it is borrowed\nprint(*r);",
        ),
        good_example: Some(
            "let x = alloc(16);\n{\n    let r = &x;\n    print(*r);\n}\nfree(x); // safe: borrow has ended",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0505: cannot move/free out of place because it is borrowed.",
        ),
    },
    DiagnosticInfo {
        code: "E0506",
        title: "Cannot assign to variable while borrowed",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "Cannot reassign or mutate a variable while active references exist to it.",
        detailed_explanation: "Direct assignment to a variable whose contents or address is currently borrowed violates reader stability and may invalidate active references.",
        common_causes: &[
            "Assigning a new value to `x` while an active reference `&x` is in scope.",
        ],
        suggested_fixes: &["Finish using the reference before reassigning the variable."],
        bad_example: Some("let mut x = 1;\nlet r = &x;\nx = 2; // error[E0506]\nprint(*r);"),
        good_example: Some("let mut x = 1;\nlet r = &x;\nprint(*r);\nx = 2;"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0506: cannot assign to borrowed value.",
        ),
    },
    DiagnosticInfo {
        code: "E0515",
        title: "Cannot return reference to local variable",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A function returns a reference to local data created inside the function.",
        detailed_explanation: "Local variables allocated on the stack are dropped when the function returns. Returning a pointer or reference to them would produce a dangling reference.",
        common_causes: &[
            "Returning `&local_var` from a function.",
            "Returning a slice or view into a temporary variable.",
        ],
        suggested_fixes: &[
            "Return the value itself by value: `return local_var;`.",
            "Allocate the value with `alloc` or ARC (`share`) and return the owning pointer or handle.",
        ],
        bad_example: Some(
            "fn get_ref() -> &int {\n    let val = 123;\n    return &val; // error[E0515]\n}",
        ),
        good_example: Some("fn get_val() -> int {\n    let val = 123;\n    return val;\n}"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0515: cannot return value referencing local data.",
        ),
    },
    DiagnosticInfo {
        code: "E0597",
        title: "Value does not live long enough",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A borrowed value is dropped while its borrow is still in use.",
        detailed_explanation: "This error occurs when the lifetime of a referenced value ends before the variable holding the borrow is finished being used.",
        common_causes: &[
            "Assigning a reference to a variable in an outer scope from a temporary in an inner block.",
        ],
        suggested_fixes: &[
            "Declare the referenced value in the same or an outer scope.",
            "Shorten the lifetime of the borrow to stay within the inner block.",
        ],
        bad_example: Some(
            "let r;\n{\n    let x = 10;\n    r = &x; // error[E0597]: `x` does not live long enough\n}\nprint(*r);",
        ),
        good_example: Some("let x = 10;\nlet r = &x;\nprint(*r);"),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0597: borrowed value does not live long enough.",
        ),
    },
    DiagnosticInfo {
        code: "E0631",
        title: "Expected type or signature mismatch in callable / missing cleanup",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::TypeCheck,
        compatibility: RustCompatibility::Equivalent,
        short_description: "Callable parameter types, arity, or cleanup interface do not match expected signature.",
        detailed_explanation: "Passing a function or closure with arguments that differ from the parameter list expected by a higher-order function, or missing a mandatory lifecycle interface.",
        common_causes: &[
            "Passing a closure taking 2 arguments where 1 argument was expected.",
            "Argument types in callback do not match expected signature.",
        ],
        suggested_fixes: &[
            "Adjust the callback parameters and types to match the expected signature.",
        ],
        bad_example: Some(
            "fn apply(f: fn(int) -> int) -> int { return f(5); }\napply(|a: int, b: int| => a + b); // error[E0631]",
        ),
        good_example: Some(
            "fn apply(f: fn(int) -> int) -> int { return f(5); }\napply(|a: int| => a * 2);",
        ),
        semantic_justification: Some(
            "Exact semantic equivalent to Rust E0631: type mismatch in function arguments.",
        ),
    },
    DiagnosticInfo {
        code: "E0733",
        title: "Potential memory leak / recursive async or reference cycle",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::Equivalent,
        short_description: "A reference cycle or unconstrained recursion was detected that may leak memory or stack.",
        detailed_explanation: "Strong reference cycles created with ARC (`strong` / `share`) cannot be automatically freed without weak references, leading to permanent memory leaks.",
        common_causes: &[
            "Two objects holding `strong` ARC references to each other in a closed loop.",
            "Self-referential object holding an owning ARC to itself.",
        ],
        suggested_fixes: &[
            "Break the cycle by converting one side of the reference to `weak`.",
            "Explicitly clear parent or child references before dropping the root.",
        ],
        bad_example: Some(
            "class Node { strong next: Node; }\n// a.next = b; b.next = a; // error[E0733]",
        ),
        good_example: Some(
            "class Node { strong next: Node; weak parent: Node; }\n// safe bidirectional link using weak back-reference",
        ),
        semantic_justification: Some(
            "Semantic equivalent to Rust E0733: recursive cycle requiring indirection or weak references to break cycle.",
        ),
    },
    // -------------------------------------------------------------
    // Adesh-Specific Codes (AD####)
    // -------------------------------------------------------------
    DiagnosticInfo {
        code: "AD0001",
        title: "Unrecognized character or invalid escape sequence",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Lexer,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "The lexer encountered an unexpected character or invalid string escape sequence.",
        detailed_explanation: "AdeshLang source files must contain valid UTF-8 characters and supported operator tokens. String escape sequences must follow Adesh syntax (e.g. \\n, \\t, \\xHH, \\u{HHHH}).",
        common_causes: &[
            "Typing non-ASCII symbols or non-operator symbols outside strings.",
            "Invalid hex or unicode escape sequence inside a string literal.",
            "Unterminated string literal.",
        ],
        suggested_fixes: &[
            "Remove invalid characters or place them inside quotes.",
            "Ensure string literals are closed with a matching quote.",
            "Verify escape sequences use correct hex digits: `\\x41` or `\\u{1F600}`.",
        ],
        bad_example: Some("let s = \"\\xZZ\"; // error[AD0001]: invalid hex escape"),
        good_example: Some("let s = \"\\x41\"; // 'A'"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0002",
        title: "Syntax error or unexpected token in parser",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Parser,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "The parser encountered a token that violates the Adesh grammar.",
        detailed_explanation: "AdeshLang statements and expressions must adhere to the formal language grammar. This error indicates unexpected, missing, or malformed tokens.",
        common_causes: &[
            "Missing closing parenthesis `)`, bracket `]`, or brace `}`.",
            "Missing expression after an assignment `=` operator.",
            "Missing semicolon `;` at the end of a statement.",
        ],
        suggested_fixes: &[
            "Check for balanced delimiters `()`, `[]`, `{}`.",
            "Provide the missing expression or operand.",
            "Ensure statement ends with a semicolon `;`.",
        ],
        bad_example: Some("let x: int = ; // error[AD0002]: expected expression"),
        good_example: Some("let x: int = 10;"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0003",
        title: "Invalid async context or await expression",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::TypeCheck,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "An `await` expression was used outside an async function or on a non-awaitable type.",
        detailed_explanation: "`await` is only valid inside functions declared with `async fn` and can only be applied to asynchronous promises, futures, or async task handles.",
        common_causes: &[
            "Using `await` in a synchronous top-level block or non-async function.",
            "Calling `await` on a plain integer or string instead of a promise.",
        ],
        suggested_fixes: &[
            "Mark the enclosing function as `async`: `async fn worker() { ... }`.",
            "Ensure the awaited expression produces a Promise or Future.",
        ],
        bad_example: Some(
            "fn run() {\n    let res = await fetch_data(); // error[AD0003]: await outside async\n}",
        ),
        good_example: Some("async fn run() {\n    let res = await fetch_data();\n}"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0004",
        title: "Heap allocation forbidden in embedded mode",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Ownership,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "Dynamic heap allocation (`alloc`) is prohibited when embedded mode is enabled.",
        detailed_explanation: "When running or compiling with `--embedded`, AdeshLang disables dynamic heap allocations and ARC to ensure deterministic zero-allocation guarantees for bare-metal and microcontroller targets.",
        common_causes: &[
            "Calling `alloc(bytes)` in an embedded configuration.",
            "Using heap-allocated collections like dynamic strings or resizing arrays in embedded mode.",
        ],
        suggested_fixes: &[
            "Use stack arrays, static buffers, or fixed-capacity arena memory instead of `alloc`.",
            "Disable embedded mode if heap allocations are required.",
        ],
        bad_example: Some(
            "let ptr = alloc(128); // error[AD0004]: heap allocation forbidden in embedded mode",
        ),
        good_example: Some("let buf: [u8; 128] = [0; 128]; // stack allocated"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0005",
        title: "Malformed or corrupted ADOB object file",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Linker,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "The Adesh binary object file (ADOB) has an invalid header, section layout, or magic bytes.",
        detailed_explanation: "The Adesh linker reads and produces ADOB files. If an input file is corrupted, truncated, or generated with an incompatible toolchain version, it is rejected.",
        common_causes: &[
            "Attempting to link an empty or truncated `.adob` file.",
            "Incompatible ADOB format version between compiler and linker.",
        ],
        suggested_fixes: &[
            "Recompile the source file to regenerate the object file.",
            "Verify file integrity and disk storage.",
        ],
        bad_example: Some("adesh build --adob corrupted.adob // error[AD0005]"),
        good_example: Some("adesh build --adob valid.adesh"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0006",
        title: "Unsupported architecture or relocation type in Adesh Linker",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Linker,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "The linker encountered an unsupported relocation type or target architecture.",
        detailed_explanation: "The native Adesh linker natively supports ELF, PE, Mach-O across x86_64, aarch64, and riscv64. Relocation calculation for unsupported targets or relocations triggers this diagnostic.",
        common_causes: &[
            "Specifying an unsupported cross-compilation target triple.",
            "Object file contains vendor-specific relocation kinds not recognized by the linker.",
        ],
        suggested_fixes: &[
            "Verify supported target triples using `adesh target list`.",
            "Use `--external-linker` if special platform relocations are required.",
        ],
        bad_example: Some("adesh build --target=mips-unknown-linux // error[AD0006]"),
        good_example: Some("adesh build --target=x86_64-unknown-linux-gnu"),
        semantic_justification: None,
    },
    DiagnosticInfo {
        code: "AD0007",
        title: "Runtime exception or trap",
        severity: DiagnosticSeverity::Error,
        phase: DiagnosticPhase::Runtime,
        compatibility: RustCompatibility::AdeshSpecific,
        short_description: "A runtime error occurred during program execution (e.g. division by zero, out of bounds).",
        detailed_explanation: "During interpreter or native JIT/AOT execution, an unhandled exception, assertion failure, or arithmetic panic occurred.",
        common_causes: &[
            "Division by integer zero: `10 / 0`.",
            "Array index out of bounds.",
            "Explicit panic or abort instruction.",
        ],
        suggested_fixes: &[
            "Check preconditions before dividing: `if divisor != 0 { ... }`.",
            "Verify array bounds using `.len()` before indexing.",
            "Handle errors using `Result` or `try/catch`.",
        ],
        bad_example: Some("let x = 10 / 0; // error[AD0007]: division by zero"),
        good_example: Some("let denom = 0;\nlet x = if denom != 0 { 10 / denom } else { 0 };"),
        semantic_justification: None,
    },
];

/// Look up a diagnostic code in the registry.
pub fn lookup_diagnostic(code: &str) -> Option<&'static DiagnosticInfo> {
    let normalized = code.trim().to_uppercase();
    DIAGNOSTIC_REGISTRY.iter().find(|d| d.code == normalized)
}

/// Print formatted explanation for a diagnostic code. Returns Ok(()) on success,
/// or Err with an unknown code message if not found.
pub fn explain_code(code: &str) -> Result<String, String> {
    let normalized = code.trim().to_uppercase();
    let diag = lookup_diagnostic(&normalized).ok_or_else(|| {
        format!(
            "error: unknown diagnostic code `{}`\nhelp: run `adesh --explain list` to see all registered diagnostic codes",
            code
        )
    })?;

    let mut out = String::new();
    out.push_str(&format!("{} — {}\n", diag.code, diag.title));
    out.push_str(&format!("Severity:      {}\n", diag.severity.as_str()));
    out.push_str(&format!("Phase:         {}\n", diag.phase.as_str()));
    out.push_str(&format!(
        "Compatibility: {}\n\n",
        match diag.compatibility {
            RustCompatibility::Equivalent => "Rust-equivalent (verified semantic mapping)",
            RustCompatibility::AdeshSpecific => "AdeshLang-specific (distinct namespace)",
        }
    ));

    out.push_str("Meaning:\n");
    out.push_str(&format!("  {}\n\n", diag.short_description));

    out.push_str("Detailed Explanation:\n");
    out.push_str(&format!("  {}\n\n", diag.detailed_explanation));

    if !diag.common_causes.is_empty() {
        out.push_str("Common Causes:\n");
        for cause in diag.common_causes {
            out.push_str(&format!("  - {}\n", cause));
        }
        out.push('\n');
    }

    if !diag.suggested_fixes.is_empty() {
        out.push_str("Suggested Fixes:\n");
        for fix in diag.suggested_fixes {
            out.push_str(&format!("  - {}\n", fix));
        }
        out.push('\n');
    }

    if let Some(bad) = diag.bad_example {
        out.push_str("Example of Erroneous Code:\n");
        for line in bad.lines() {
            out.push_str(&format!("  {}\n", line));
        }
        out.push('\n');
    }

    if let Some(good) = diag.good_example {
        out.push_str("Corrected Example:\n");
        for line in good.lines() {
            out.push_str(&format!("  {}\n", line));
        }
        out.push('\n');
    }

    if let Some(justification) = diag.semantic_justification {
        out.push_str("Rust Mapping Justification:\n");
        out.push_str(&format!("  {}\n\n", justification));
    }

    out.push_str(&format!(
        "For additional information, visit: https://adeshlang.org/docs/compiler/error-codes#{}\n",
        diag.code.to_lowercase()
    ));

    Ok(out)
}

/// List all registered diagnostic codes in tabular format.
pub fn list_diagnostics() -> String {
    let mut out = String::new();
    out.push_str("AdeshLang Registered Diagnostic Codes:\n");
    out.push_str(&format!(
        "{:<8} | {:<9} | {:<10} | {:<16} | {}\n",
        "Code", "Severity", "Phase", "Compatibility", "Title"
    ));
    out.push_str(&format!(
        "{:-<8}-|-{:-<9}-|-{:-<10}-|-{:-<16}-|-{:-<35}\n",
        "", "", "", "", ""
    ));

    for diag in DIAGNOSTIC_REGISTRY {
        let compat_str = match diag.compatibility {
            RustCompatibility::Equivalent => "Rust-compatible",
            RustCompatibility::AdeshSpecific => "Adesh-specific",
        };
        out.push_str(&format!(
            "{:<8} | {:<9} | {:<10} | {:<16} | {}\n",
            diag.code,
            diag.severity.as_str(),
            diag.phase.as_str(),
            compat_str,
            diag.title
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_codes_unique() {
        let mut seen = std::collections::HashSet::new();
        for diag in DIAGNOSTIC_REGISTRY {
            assert!(
                seen.insert(diag.code),
                "Duplicate diagnostic code found in registry: {}",
                diag.code
            );
        }
    }

    #[test]
    fn test_rust_codes_format() {
        for diag in DIAGNOSTIC_REGISTRY {
            if diag.compatibility == RustCompatibility::Equivalent {
                assert!(
                    diag.code.starts_with('E') && diag.code.len() == 5,
                    "Rust equivalent code must be formatted as E####: {}",
                    diag.code
                );
                assert!(
                    diag.semantic_justification.is_some(),
                    "Rust equivalent code must have semantic justification: {}",
                    diag.code
                );
            } else {
                assert!(
                    diag.code.starts_with("AD") && diag.code.len() == 6,
                    "Adesh-specific code must be formatted as AD####: {}",
                    diag.code
                );
            }
        }
    }

    #[test]
    fn test_explain_valid_code() {
        let res = explain_code("E0308").unwrap();
        assert!(res.contains("E0308 — Mismatched types"));
        assert!(res.contains("Common Causes:"));
        assert!(res.contains("Suggested Fixes:"));
    }

    #[test]
    fn test_explain_adesh_code() {
        let res = explain_code("AD0001").unwrap();
        assert!(res.contains("AD0001 — Unrecognized character"));
    }

    #[test]
    fn test_explain_unknown_code() {
        let res = explain_code("E9999");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown diagnostic code"));
    }
}
