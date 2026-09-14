//! Type-Aware Auto-Completion Provider
//!
//! Provides intelligent code completion based on the semantic engine.
//! Completion is context-sensitive and type-aware:
//! - `user.` → resolves `user`'s type and shows its members
//! - `Math.` → shows Math namespace members
//! - After `let x: ` → shows type completions
//! - In expression position → shows visible symbols + builtins

use crate::analysis::{SymbolInfo, SymbolKind};
use adeshlang::semantics::{SemanticIndex, SemanticSymbolKind, CompletionCandidate};
use adeshlang::typesystem::checker::Ty;
use lsp_types::{
    CompletionItem, CompletionItemKind, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind,
};

/// Built-in keywords in Adesh (CFG v2.2)
const KEYWORDS: &[&str] = &[
    "let", "const", "readonly", "test", "fn", "class", "extend", "raw", "decorator",
    "if", "else", "elif", "while", "do", "for", "in", "return", "break", "continue",
    "try", "catch", "throw", "import", "export", "from", "as", "new", "this", "super",
    "true", "false", "null", "async", "await", "enum", "interface", "extends", "implements",
    "static", "abstract", "type", "match", "struct", "pub", "mut", "yield", "defer",
    "where", "is", "typeof", "sizeof", "alignof", "get", "set", "sealed", "on", "region", "unsafe",
];

/// Built-in functions with signatures
const BUILTINS: &[(&str, &str, &str)] = &[
    ("assert", "fn assert(condition: bool, message?: string)", "Fail the test if the condition is false"),
    ("assert_eq", "fn assert_eq(left, right)", "Fail the test if values are not equal"),
    ("assert_ne", "fn assert_ne(left, right)", "Fail the test if values are equal"),
    ("print", "fn print(...args, options?)", "Print values to stdout.\n\nPrints any number of values, converted to strings and separated by a space by default.\n\n**Options** (trailing object literal):\n- `sep: string` — separator between values (default `\" \"`)\n- `end: string` — string appended at the end (default `\"\\n\"`)\n- `file: string` — write to a file instead of stdout\n- `flush: bool` — force-flush the output buffer\n- `color: string` — text color, hex `#RRGGBB` or `#RGB`\n- `background: string` — background color, hex\n- `bold/italic/underline/strikethrough: bool` — text styles\n- `pretty: bool|string` — pretty-print: `true`/`\"full\"`, `\"compact\"`, `\"simple\"`, `\"none\"`\n\n**Examples:**\n```adesh\nprint(\"Hello\", \"World\");                  // Hello World\nprint(\"apple\", \"banana\", { sep: \", \" }); // apple, banana\nprint(\"Error:\", x, { color: \"#FF0000\", bold: true });\nprint(data, { pretty: true });\nprint(\"log\", { file: \"app.log\" });\n```"),
    ("println", "fn println(...args)", "Print values with newline. Equivalent to `print(..., { end: \"\\n\" })`"),
    ("input", "fn input(prompt?: string): string", "Read line from stdin"),
    ("len", "fn len(collection): number", "Get length of array/string"),
    ("type", "fn type(value): string", "Get type name of value"),
    ("range", "fn range(start, end, step?): array", "Create numeric range"),
    ("map", "fn map(arr, fn): array", "Map function over array"),
    ("filter", "fn filter(arr, fn): array", "Filter array by predicate"),
    ("reduce", "fn reduce(arr, fn, init): any", "Reduce array to single value"),
    ("clock", "fn clock(): number", "Get high-resolution timestamp (ns)"),
    ("sleep", "fn sleep(ms: number)", "Sleep for milliseconds"),
    ("sizeof", "fn sizeof<T>(): number", "Get size of type in bytes"),
    ("alignof", "fn alignof<T>(): number", "Get alignment of type in bytes"),
    ("str", "fn str(value): string", "Convert value to string"),
    ("max", "fn max(...args): number", "Return maximum value"),
    ("min", "fn min(...args): number", "Return minimum value"),
];

/// Math namespace members
const MATH_MEMBERS: &[(&str, &str, &str)] = &[
    ("PI", "const PI: number", "π ≈ 3.14159265358979"),
    ("E", "const E: number", "Euler's number ≈ 2.71828"),
    ("TAU", "const TAU: number", "τ = 2π ≈ 6.28318"),
    ("SQRT2", "const SQRT2: number", "√2 ≈ 1.41421"),
    ("LN2", "const LN2: number", "ln(2) ≈ 0.69314"),
    ("LN10", "const LN10: number", "ln(10) ≈ 2.30258"),
    ("abs", "fn abs(x: number): number", "Absolute value"),
    ("floor", "fn floor(x: number): number", "Floor value"),
    ("ceil", "fn ceil(x: number): number", "Ceiling value"),
    ("round", "fn round(x: number): number", "Round to nearest integer"),
    ("sqrt", "fn sqrt(x: number): number", "Square root"),
    ("pow", "fn pow(base, exp): number", "Power function"),
    ("sin", "fn sin(x: number): number", "Sine (radians)"),
    ("cos", "fn cos(x: number): number", "Cosine (radians)"),
    ("tan", "fn tan(x: number): number", "Tangent (radians)"),
    ("exp", "fn exp(x: number): number", "e^x"),
    ("log", "fn log(x: number): number", "Natural logarithm"),
    ("log10", "fn log10(x: number): number", "Base-10 logarithm"),
    ("random", "fn random(): number", "Random number [0, 1)"),
    ("randomInt", "fn randomInt(min, max): number", "Random integer [min, max]"),
    ("randomRange", "fn randomRange(min, max): number", "Random float [min, max)"),
    ("seed", "fn seed(n: number)", "Seed the RNG"),
    ("min", "fn min(...args): number", "Minimum value"),
    ("max", "fn max(...args): number", "Maximum value"),
];

/// Time namespace members
const TIME_MEMBERS: &[(&str, &str, &str)] = &[
    ("now", "fn now(): number", "Current time in nanoseconds"),
    ("nowMs", "fn nowMs(): number", "Current time in milliseconds"),
    ("nowUs", "fn nowUs(): number", "Current time in microseconds"),
    ("nowSecs", "fn nowSecs(): number", "Current time in seconds"),
    ("epoch", "fn epoch(): number", "Unix timestamp in seconds"),
    ("epochNanos", "fn epochNanos(): number", "Unix timestamp in nanoseconds"),
];

/// Generate type-aware, context-sensitive completions using the semantic engine.
pub fn get_completions_semantic(
    index: &SemanticIndex,
    trigger: Option<&str>,
    word: Option<&str>,
    line_text: &str,
    line: u32,
    col: u32,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    match trigger {
        Some(".") => {
            // Member access completion — resolve the type of the object before the dot
            items.extend(get_member_completions(index, word, line_text, line, col));
        }
        Some(":") => {
            // Type annotation position (e.g., `let x: int`, `fn foo(n: i32): string`)
            // AdeshLang uses `:` for type annotations, not `::` for namespaces
            items.extend(get_type_completions_semantic(index));
        }
        Some("(") => {
            // Function call — could trigger signature help, but also provide completions
            // for partial identifier before '('
            if let Some(w) = word {
                items.extend(get_filtered_symbol_completions(index, w));
            }
        }
        Some("@") => {
            // Decorator position — show available decorators
            items.extend(get_decorator_completions());
        }
        _ => {
            // General expression context — triggered on every keystroke by the client
            items.extend(get_keyword_completions());
            items.extend(get_builtin_completions());
            items.extend(get_symbol_completions_semantic(index));

            // Add namespace suggestions
            items.push(namespace_completion("Math", "Mathematical functions and constants"));
            items.push(namespace_completion("time", "High-resolution time functions"));

            // Add type names for constructor context
            items.extend(get_type_name_completions(index));

            // Filter by word prefix if provided
            if let Some(w) = word {
                if !w.is_empty() {
                    items.retain(|item| item.label.to_lowercase().starts_with(&w.to_lowercase()));
                }
            }
        }
    }

    // Sort items by label for consistent ordering
    items.sort_by(|a, b| a.label.cmp(&b.label));

    // Deduplicate by label (keep first occurrence which has more detail)
    items.dedup_by(|a, b| a.label == b.label);

    items
}

/// Get member completions for a type (the core of type-aware completion).
fn get_member_completions(
    index: &SemanticIndex,
    receiver_name: Option<&str>,
    line_text: &str,
    _line: u32,
    _col: u32,
) -> Vec<CompletionItem> {
    // Determine the receiver expression before the dot
    let receiver = receiver_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Extract the expression before the last dot on the line
            let before_dot = line_text.rfind('.').map(|i| &line_text[..i]).unwrap_or("");
            before_dot.trim().to_string()
        });

    if receiver.is_empty() {
        return vec![];
    }

    // Check for built-in namespaces first
    if receiver == "Math" {
        return get_math_completions();
    }
    if receiver == "time" {
        return get_time_completions();
    }

    // Resolve the receiver's type using the semantic engine
    let receiver_type = resolve_receiver_type(index, &receiver);

    match receiver_type {
        Some(ty) => {
            // Get completions for the resolved type
            let candidates = index.completions_for_type(&ty);
            candidates.into_iter().map(candidate_to_completion_item).collect()
        }
        None => {
            // Fallback: if we can't resolve the type, try to find it as a known type name
            // (e.g., for static access like `Color.Red`)
            if let Some(_td) = index.get_type(&receiver) {
                let candidates = index.completions_for_type(&Ty::GenericInstance {
                    name: receiver.clone(),
                    args: vec![],
                });
                return candidates.into_iter().map(candidate_to_completion_item).collect();
            }
            vec![]
        }
    }
}

/// Resolve the type of a receiver expression (e.g., "user", "get_user()", "users[0]")
fn resolve_receiver_type(index: &SemanticIndex, receiver: &str) -> Option<Ty> {
    // Simple variable reference. The semantic engine resolves the declared type
    // annotation (`let user: User`) and, when there is none, infers the type of
    // the initializer (`let user = User(1, "Ajay")`, `let p = Point(1, 2)`).
    if let Some(ty) = index.resolve_symbol_type(receiver) {
        return Some(ty);
    }

    // Function call: "func_name()" — resolve return type
    if receiver.ends_with("()") {
        let func_name = &receiver[..receiver.len() - 2];
        if let Some(sym) = index.find_declaration(func_name) {
            if let Some(ret) = &sym.return_type {
                return index.resolve_type(ret);
            }
            if let Some(ty) = &sym.ty {
                return Some(ty.clone());
            }
        }
    }

    // Indexed access: "arr[0]" — resolve element type
    if receiver.ends_with(']') {
        if let Some(bracket_start) = receiver.rfind('[') {
            let base = &receiver[..bracket_start];
            if let Some(sym) = index.find_declaration(base) {
                if let Some(ty) = &sym.ty {
                    if let Ty::Array(elem) = ty {
                        return Some((**elem).clone());
                    }
                    if let Ty::GenericInstance { name, args } = ty {
                        if name == "Array" && !args.is_empty() {
                            return Some(args[0].clone());
                        }
                    }
                }
                // `let arr: [T] = ...` — the annotation resolves to Ty::Array.
                if let Some(ty) = index.resolve_symbol_type(base) {
                    if let Ty::Array(elem) = ty {
                        return Some((*elem).clone());
                    }
                }
            }
        }
    }

    None
}

/// Convert a semantic CompletionCandidate to an LSP CompletionItem.
fn candidate_to_completion_item(candidate: CompletionCandidate) -> CompletionItem {
    let kind = match candidate.kind {
        SemanticSymbolKind::Variable => CompletionItemKind::VARIABLE,
        SemanticSymbolKind::Constant => CompletionItemKind::CONSTANT,
        SemanticSymbolKind::Parameter => CompletionItemKind::VARIABLE,
        SemanticSymbolKind::Function => CompletionItemKind::FUNCTION,
        SemanticSymbolKind::Method => CompletionItemKind::METHOD,
        SemanticSymbolKind::StaticMethod => CompletionItemKind::METHOD,
        SemanticSymbolKind::Field => CompletionItemKind::FIELD,
        SemanticSymbolKind::Property => CompletionItemKind::PROPERTY,
        SemanticSymbolKind::Class => CompletionItemKind::CLASS,
        SemanticSymbolKind::Struct => CompletionItemKind::STRUCT,
        SemanticSymbolKind::Enum => CompletionItemKind::ENUM,
        SemanticSymbolKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
        SemanticSymbolKind::Interface => CompletionItemKind::INTERFACE,
        SemanticSymbolKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
        SemanticSymbolKind::Module => CompletionItemKind::MODULE,
        SemanticSymbolKind::Import => CompletionItemKind::MODULE,
    };

    let insert_text_format = if candidate.insert_text.as_ref().map_or(false, |t| t.contains("$0")) {
        Some(InsertTextFormat::SNIPPET)
    } else {
        Some(InsertTextFormat::PLAIN_TEXT)
    };

    CompletionItem {
        label: candidate.label,
        kind: Some(kind),
        detail: candidate.detail,
        documentation: candidate.documentation.map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: d,
            })
        }),
        insert_text: candidate.insert_text,
        insert_text_format,
        ..Default::default()
    }
}

fn get_keyword_completions() -> Vec<CompletionItem> {
    KEYWORDS.iter().map(|kw| CompletionItem {
        label: (*kw).to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("keyword".to_string()),
        ..Default::default()
    }).collect()
}

fn get_builtin_completions() -> Vec<CompletionItem> {
    BUILTINS.iter().map(|(name, sig, doc)| CompletionItem {
        label: (*name).to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some((*sig).to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```adesh\n{}\n```\n\n{}", sig, doc),
        })),
        insert_text: Some(format!("{}($0)", name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }).collect()
}

fn get_math_completions() -> Vec<CompletionItem> {
    MATH_MEMBERS.iter().map(|(name, sig, doc)| {
        let is_const = sig.starts_with("const");
        CompletionItem {
            label: (*name).to_string(),
            kind: Some(if is_const { CompletionItemKind::CONSTANT } else { CompletionItemKind::FUNCTION }),
            detail: Some((*sig).to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```adesh\n{}\n```\n\n{}", sig, doc),
            })),
            insert_text: if is_const { Some((*name).to_string()) } else { Some(format!("{}($0)", name)) },
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        }
    }).collect()
}

fn get_time_completions() -> Vec<CompletionItem> {
    TIME_MEMBERS.iter().map(|(name, sig, doc)| CompletionItem {
        label: (*name).to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some((*sig).to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```adesh\n{}\n```\n\n{}", sig, doc),
        })),
        insert_text: Some(format!("{}()", name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }).collect()
}

fn get_type_completions_semantic(index: &SemanticIndex) -> Vec<CompletionItem> {
    let candidates = index.type_completions();
    candidates.into_iter().map(candidate_to_completion_item).collect()
}

fn get_symbol_completions_semantic(index: &SemanticIndex) -> Vec<CompletionItem> {
    let candidates = index.visible_symbols_at(usize::MAX, 0);
    candidates.into_iter().map(candidate_to_completion_item).collect()
}

fn get_filtered_symbol_completions(index: &SemanticIndex, prefix: &str) -> Vec<CompletionItem> {
    let mut candidates = index.visible_symbols_at(usize::MAX, 0);
    candidates.retain(|c| c.label.to_lowercase().starts_with(&prefix.to_lowercase()));
    candidates.into_iter().map(candidate_to_completion_item).collect()
}

fn namespace_completion(name: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::MODULE),
        detail: Some(doc.to_string()),
        documentation: Some(Documentation::String(doc.to_string())),
        ..Default::default()
    }
}

/// Get decorator completions (after `@`).
fn get_decorator_completions() -> Vec<CompletionItem> {
    const DECORATORS: &[(&str, &str)] = &[
        ("inline", "Inline the function at call sites"),
        ("memoize", "Cache function results by arguments"),
        ("deprecated", "Mark as deprecated"),
        ("test", "Mark function as a test case"),
        ("bench", "Mark function as a benchmark"),
        ("readonly", "Make fields read-only"),
        ("unsafe", "Mark function as unsafe"),
        ("pure", "Mark function as pure (no side effects)"),
        ("tailcall", "Enable tail-call optimization"),
        ("noalias", "Mark pointer as noalias"),
        ("derive", "Auto-derive trait implementations"),
    ];

    DECORATORS.iter().map(|(name, doc)| CompletionItem {
        label: (*name).to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some("decorator".to_string()),
        documentation: Some(Documentation::String((*doc).to_string())),
        insert_text: Some(format!("{}($0)", name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }).collect()
}

/// Get type name completions for constructor context (e.g., `new User`).
fn get_type_name_completions(index: &SemanticIndex) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for (name, td) in &index.types {
        let detail = match td.kind {
            SemanticSymbolKind::Class => format!("class {}", name),
            SemanticSymbolKind::Struct => format!("struct {}", name),
            SemanticSymbolKind::Enum => format!("enum {}", name),
            SemanticSymbolKind::Interface => format!("interface {}", name),
            _ => continue,
        };

        let kind = match td.kind {
            SemanticSymbolKind::Class => CompletionItemKind::CLASS,
            SemanticSymbolKind::Struct => CompletionItemKind::STRUCT,
            SemanticSymbolKind::Enum => CompletionItemKind::ENUM,
            SemanticSymbolKind::Interface => CompletionItemKind::INTERFACE,
            _ => CompletionItemKind::CLASS,
        };

        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(kind),
            detail: Some(detail),
            ..Default::default()
        });
    }

    items
}

// ===== Legacy API (for backward compatibility) =====

/// Generate completions for a given context (legacy API, used by server.rs)
#[allow(dead_code)]
pub fn get_completions(
    trigger: Option<&str>,
    word: Option<&str>,
    symbols: &[SymbolInfo],
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    match trigger {
        Some(".") => {
            if let Some(w) = word {
                if w == "Math" {
                    items.extend(get_math_completions());
                } else if w == "time" {
                    items.extend(get_time_completions());
                }
            }
        }
        _ => {
            items.extend(get_keyword_completions());
            items.extend(get_builtin_completions());
            items.extend(get_symbol_completions(symbols));
        }
    }

    items
}

fn get_symbol_completions(symbols: &[SymbolInfo]) -> Vec<CompletionItem> {
    symbols.iter().map(|sym| {
        let kind = match sym.kind {
            SymbolKind::Function => CompletionItemKind::FUNCTION,
            SymbolKind::Class => CompletionItemKind::CLASS,
            SymbolKind::Variable => CompletionItemKind::VARIABLE,
            SymbolKind::Constant => CompletionItemKind::CONSTANT,
            SymbolKind::Parameter => CompletionItemKind::VARIABLE,
            SymbolKind::Method => CompletionItemKind::METHOD,
            SymbolKind::Property => CompletionItemKind::PROPERTY,
            SymbolKind::Enum => CompletionItemKind::ENUM,
            SymbolKind::Interface => CompletionItemKind::INTERFACE,
            SymbolKind::Module => CompletionItemKind::MODULE,
        };

        CompletionItem {
            label: sym.name.clone(),
            kind: Some(kind),
            detail: sym.signature.clone(),
            documentation: sym.documentation.clone().map(Documentation::String),
            ..Default::default()
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run member completions for `receiver.` in a source file.
    fn member_completions(source: &str, receiver: &str, line: u32, col: u32) -> Vec<String> {
        let index = adeshlang::semantics::index_source(source);
        let line_text = format!("{}.", receiver);
        get_completions_semantic(&index, Some("."), Some(receiver), &line_text, line, col)
            .into_iter()
            .map(|i| i.label.clone())
            .collect()
    }

    /// Assert a list of labels contains a name.
    fn has(labels: &[String], name: &str) -> bool {
        labels.iter().any(|l| l == name)
    }

    #[test]
    fn test_member_completion_for_annotated_variable() {
        let source = "class User {\n    id: i32;\n    name: string;\n    fn save(): bool { return true; }\n}\n\nlet user: User = User(1, \"Ajay\");\nuser.\n";
        let labels = member_completions(source, "user", 7, 5);
        assert!(has(&labels, "id"), "Labels: {:?}", labels);
        assert!(has(&labels, "name"), "Labels: {:?}", labels);
        assert!(has(&labels, "save"), "Labels: {:?}", labels);
    }

    #[test]
    fn test_member_completion_for_constructor_inferred() {
        // No type annotation — the type is inferred from `User(1, "Ajay")`.
        let source = "class User {\n    id: i32;\n    name: string;\n}\n\nlet user = User(1, \"Ajay\");\nuser.\n";
        let labels = member_completions(source, "user", 6, 5);
        assert!(has(&labels, "id"), "Labels: {:?}", labels);
        assert!(has(&labels, "name"), "Labels: {:?}", labels);
    }

    #[test]
    fn test_member_completion_for_struct() {
        let source = "struct Point {\n    x: f64,\n    y: f64,\n}\n\nlet p: Point = Point(1.0, 2.0);\np.\n";
        let labels = member_completions(source, "p", 6, 2);
        assert!(has(&labels, "x"), "Labels: {:?}", labels);
        assert!(has(&labels, "y"), "Labels: {:?}", labels);
    }

    #[test]
    fn test_print_builtin_completion_documentation() {
        let index = adeshlang::semantics::index_source("print(x);\n");
        let items = get_completions_semantic(&index, None, None, "", 0, 6);
        let print_item = items.into_iter().find(|i| i.label == "print").expect("print completion");
        let doc = match print_item.documentation {
            Some(Documentation::MarkupContent(m)) => m.value,
            _ => String::new(),
        };
        assert!(doc.contains("sep"), "doc: {}", doc);
        assert!(doc.contains("pretty"), "doc: {}", doc);
        assert!(doc.contains("color"), "doc: {}", doc);
    }
}
