//! Hover Information Provider
//!
//! Provides type information, ownership status, and documentation on hover.
//! Uses the semantic engine to resolve types from the symbol table and
//! type inference, rather than text matching.

use crate::document::Document;
use crate::workspace::WorkspaceIndex;
use adeshlang::semantics::{
    SemanticIndex, SemanticSymbolKind, SymbolEntry, TypeDefinition, VisibilityKind,
};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

/// Get hover information using the semantic engine.
pub fn get_hover_semantic(
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    _doc: &Document,
    line: u32,
    col: u32,
) -> Option<Hover> {
    // Convert LSP position (0-based) to internal position (1-based)
    let internal_line = (line + 1) as usize;
    let internal_col = (col + 1) as usize;

    // Keywords are tokenized as their own token kinds, so accept them here too.
    let lexeme = index.identifier_or_keyword_at(internal_line, internal_col)?;

    // Check built-in keywords, types, functions, and modules first
    if let Some(hover) = get_builtin_hover(lexeme) {
        return Some(hover);
    }

    // Find the symbol declaration in the index
    if let Some(sym) = index.find_declaration(lexeme) {
        // Type declarations get a richer type-definition hover with
        // fields, methods, and variants.
        if sym.kind.is_type() {
            if let Some(td) = index.get_type(lexeme) {
                return Some(type_definition_hover(td));
            }
            if let Some(td) = workspace.find_type_definition(lexeme) {
                return Some(type_definition_hover(td));
            }
        }
        return Some(symbol_to_hover(sym, index, workspace));
    }

    // Search workspace for cross-file definitions
    if let Some((_uri, sym)) = workspace.find_declaration_workspace(lexeme) {
        return Some(symbol_to_hover(sym, index, workspace));
    }

    // Try to find the symbol at the exact position (could be a usage, not declaration)
    if let Some(sym) = index.find_symbol_at(internal_line, internal_col) {
        return Some(symbol_to_hover(sym, index, workspace));
    }

    // Fallback: check if it's a type name
    if let Some(td) = index.get_type(lexeme) {
        return Some(type_definition_hover(td));
    }
    if let Some(td) = workspace.find_type_definition(lexeme) {
        return Some(type_definition_hover(td));
    }

    None
}

/// Convert a SymbolEntry to a Hover display.
fn symbol_to_hover(sym: &SymbolEntry, index: &SemanticIndex, workspace: &WorkspaceIndex) -> Hover {
    let mut content = String::new();

    // Type annotation or signature
    if let Some(sig) = &sym.signature {
        content.push_str(&format!("```adesh\n{}\n```", sig));
    } else if let Some(type_ann) = &sym.type_annotation {
        content.push_str(&format!("```adesh\n{}: {}\n```", sym.name, type_ann));
    } else {
        content.push_str(&format!("```adesh\n{}\n```", sym.name));
    }

    // Kind label
    content.push_str(&format!("\n\n**{}**", format_symbol_kind(sym.kind)));

    // Parameters for functions/methods
    if !sym.params.is_empty() {
        content.push_str("\n\n**Parameters:**\n");
        for (pname, ptype) in &sym.params {
            if let Some(t) = ptype {
                content.push_str(&format!("- `{}: {}`\n", pname, t));
            } else {
                content.push_str(&format!("- `{}`\n", pname));
            }
        }
    }

    // Return type
    if let Some(ret) = &sym.return_type {
        content.push_str(&format!("\n**Returns:** `{}`\n", ret));
    }

    // Visibility
    match sym.visibility {
        VisibilityKind::Public => content.push_str("\n`public`"),
        VisibilityKind::Private => content.push_str("\n`private`"),
        VisibilityKind::Protected => content.push_str("\n`protected`"),
        VisibilityKind::Default => {}
    }

    // Mutability
    if sym.kind == SemanticSymbolKind::Variable || sym.kind == SemanticSymbolKind::Field {
        if sym.mutable {
            content.push_str("\n`mut`");
        } else {
            content.push_str("\n`immutable`");
        }
    }

    // Async
    if sym.is_async {
        content.push_str("\n`async`");
    }

    // Documentation
    if let Some(doc) = &sym.documentation {
        content.push_str(&format!("\n\n{}", doc));
    }

    // Members of the symbol's type. For a typed variable/parameter this shows
    // every field/method of its type; for a type symbol it shows its own
    // members (works even when the declaration lives in another file).
    if let Some(type_name) = symbol_member_type_name(sym) {
        append_type_members(&mut content, index, workspace, &type_name);
    }

    create_markdown_hover(&content)
}

/// The name of the type whose members should be listed for this symbol, if any.
fn symbol_member_type_name(sym: &SymbolEntry) -> Option<String> {
    match sym.kind {
        SemanticSymbolKind::Class
        | SemanticSymbolKind::Struct
        | SemanticSymbolKind::Enum
        | SemanticSymbolKind::Interface
        | SemanticSymbolKind::TypeAlias => Some(sym.name.clone()),
        SemanticSymbolKind::Variable
        | SemanticSymbolKind::Constant
        | SemanticSymbolKind::Parameter => {
            sym.type_annotation.as_ref().map(|t| base_type_name(t).to_string())
        }
        _ => None,
    }
}

/// Strip generics/nullability/pointer syntax from an annotation:
/// `"User<int>"` → `"User"`, `"User?"` → `"User"`.
fn base_type_name(type_str: &str) -> &str {
    let s = type_str.trim().trim_start_matches('*').trim_end_matches('?').trim();
    match s.find('<') {
        Some(idx) => s[..idx].trim(),
        None => s,
    }
}

/// Append a markdown "Members of X" section listing the type's fields, methods,
/// static members and variants. The type can be defined in this file or any
/// other file in the workspace.
fn append_type_members(
    content: &mut String,
    index: &SemanticIndex,
    workspace: &WorkspaceIndex,
    type_name: &str,
) {
    if type_name.is_empty() {
        return;
    }
    let members = if index.get_type(type_name).is_some() {
        index.get_type_members(type_name)
    } else {
        workspace.get_type_members(type_name).unwrap_or_default()
    };
    if members.is_empty() {
        return;
    }

    content.push_str(&format!("\n\n**Members of `{}`:**\n", type_name));
    for m in members {
        let head = match m.kind {
            SemanticSymbolKind::Method => "`fn`",
            SemanticSymbolKind::StaticMethod => "`static`",
            SemanticSymbolKind::EnumVariant => "`variant`",
            SemanticSymbolKind::Field => "`field`",
            SemanticSymbolKind::Property => "`property`",
            _ => continue,
        };
        let sig = m.signature.clone().unwrap_or_else(|| {
            m.type_annotation
                .as_ref()
                .map(|t| format!("{}: {}", m.name, t))
                .unwrap_or_else(|| m.name.clone())
        });
        content.push_str(&format!("- {} `{}`\n", head, sig));
    }
}

/// Hover for a type definition: kind, generics, parent/interfaces, fields,
/// methods, static members, and variants.
fn type_definition_hover(td: &TypeDefinition) -> Hover {
    let mut content = format!("**{}** ({})\n", td.name, format_symbol_kind(td.kind));

    if !td.type_params.is_empty() {
        content.push_str(&format!("`<{}>`\n", td.type_params.join(", ")));
    }
    if let Some(parent) = &td.extends {
        content.push_str(&format!("\n`extends {}`", parent));
    }
    if !td.implements.is_empty() {
        content.push_str(&format!("\n`implements {}`", td.implements.join(", ")));
    }

    if !td.fields.is_empty() {
        content.push_str("\n\n**Fields:**\n");
        for (fname, ftype, fvis) in &td.fields {
            content.push_str(&format!("- `{}: {}`{}\n", fname, ftype, vis_suffix(*fvis)));
        }
    }
    if !td.methods.is_empty() {
        content.push_str("\n**Methods:**\n");
        for m in &td.methods {
            content.push_str(&format!("- `{}`\n", m.signature.as_deref().unwrap_or(&m.name)));
        }
    }
    if !td.static_methods.is_empty() {
        content.push_str("\n**Static methods:**\n");
        for m in &td.static_methods {
            content.push_str(&format!("- `{}`\n", m.signature.as_deref().unwrap_or(&m.name)));
        }
    }
    if !td.static_properties.is_empty() {
        content.push_str("\n**Static properties:**\n");
        for (name, ty) in &td.static_properties {
            content.push_str(&format!("- `{}: {}`\n", name, ty));
        }
    }
    if !td.variants.is_empty() {
        content.push_str("\n**Variants:**\n");
        for (vname, vtype) in &td.variants {
            match vtype {
                Some(t) => content.push_str(&format!("- `{}({})`\n", vname, t)),
                None => content.push_str(&format!("- `{}`\n", vname)),
            }
        }
    }

    create_markdown_hover(&content)
}

fn vis_suffix(v: VisibilityKind) -> &'static str {
    match v {
        VisibilityKind::Public => " `public`",
        VisibilityKind::Private => " `private`",
        VisibilityKind::Protected => " `protected`",
        VisibilityKind::Default => "",
    }
}

fn format_symbol_kind(kind: SemanticSymbolKind) -> &'static str {
    match kind {
        SemanticSymbolKind::Variable => "variable",
        SemanticSymbolKind::Constant => "constant",
        SemanticSymbolKind::Parameter => "parameter",
        SemanticSymbolKind::Function => "function",
        SemanticSymbolKind::Method => "method",
        SemanticSymbolKind::StaticMethod => "static method",
        SemanticSymbolKind::Field => "field",
        SemanticSymbolKind::Property => "property",
        SemanticSymbolKind::Class => "class",
        SemanticSymbolKind::Struct => "struct",
        SemanticSymbolKind::Enum => "enum",
        SemanticSymbolKind::EnumVariant => "enum variant",
        SemanticSymbolKind::Interface => "interface",
        SemanticSymbolKind::TypeAlias => "type alias",
        SemanticSymbolKind::Module => "module",
        SemanticSymbolKind::Import => "import",
    }
}

fn create_markdown_hover(content: &str) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content.to_string(),
        }),
        range: None,
    }
}

/// Get hover for built-in keywords, functions, modules, and types.
fn get_builtin_hover(word: &str) -> Option<Hover> {
    for (w, title, doc) in KEYWORDS {
        if *w == word {
            return Some(create_hover(title, doc));
        }
    }
    for (w, title, doc) in BUILTIN_FUNCTIONS {
        if *w == word {
            return Some(create_hover(title, doc));
        }
    }
    for (w, title, doc) in BUILTIN_MODULES {
        if *w == word {
            return Some(create_hover(title, doc));
        }
    }
    for (w, title, doc) in BUILTIN_TYPES {
        if *w == word {
            return Some(create_hover(title, doc));
        }
    }
    None
}

fn create_hover(title: &str, doc: &str) -> Hover {
    create_markdown_hover(&format!("**{}**\n\n{}", title, doc))
}

/// Language keywords with usage docs and examples.
const KEYWORDS: &[(&str, &str, &str)] = &[
    ("let", "keyword let", "Declares a mutable variable.\n\n```adesh\nlet x = 42;\nlet name: string = \"Ajay\";\n```\n\nUse `const` for immutable bindings."),
    ("const", "keyword const", "Declares an immutable constant.\n\n```adesh\nconst PI = 3.14159;\nconst NAME: string = \"Adesh\";\n```"),
    ("readonly", "keyword readonly", "Declares a variable whose binding cannot be reassigned (read-only).\n\n```adesh\nreadonly config = load_config();\n```"),
    ("fn", "keyword fn", "Declares a function.\n\n```adesh\nfn add(a: i32, b: i32): i32 {\n    return a + b;\n}\n```\n\nFunctions are first-class values:\n```adesh\nlet op = fn(x: i32, y: i32) -> i32 { x + y };\n```"),
    ("function", "keyword function", "Alias of `fn` used to declare functions."),
    ("class", "keyword class", "Declares a class with fields, methods, and inheritance.\n\n```adesh\nclass User extends Base implements Serializable {\n    id: i32;\n    name: string;\n\n    fn save(): bool {\n        return true;\n    }\n}\n```"),
    ("struct", "keyword struct", "Declares a value-type struct with named fields.\n\n```adesh\nstruct Point {\n    x: f64,\n    y: f64,\n}\n\nlet p = Point(1.0, 2.0);\n```"),
    ("enum", "keyword enum", "Declares an enumeration. Variants can optionally carry data.\n\n```adesh\nenum Color {\n    Red,\n    Green,\n    Blue,\n}\n\nenum Shape {\n    Circle(f64),\n    Rect(f64, f64),\n}\n```"),
    ("interface", "keyword interface", "Declares an interface that classes can implement. Interfaces list method signatures; implementing classes must provide them.\n\n```adesh\ninterface Drawable {\n    fn draw();\n    fn resize(w: f64, h: f64);\n}\n\nclass Canvas implements Drawable {\n    fn draw() { ... }\n    fn resize(w: f64, h: f64) { ... }\n}\n```"),
    ("type", "keyword type", "Declares a type alias or record type.\n\n```adesh\ntype ID = i64;\ntype Point2D = { x: f64, y: f64 };\n```"),
    ("match", "keyword match", "Pattern matching expression with exhaustive arms.\n\n```adesh\nmatch color {\n    Red => \"red\",\n    Green => \"green\",\n    _ => \"unknown\",\n}\n\nmatch status {\n    200 | 201 => \"ok\",\n    400..499 => \"client error\",\n    _ => \"other\",\n}\n```"),
    ("if", "keyword if", "Conditional statement/expression.\n\n```adesh\nif (x > 0) {\n    print(\"positive\");\n} else if (x < 0) {\n    print(\"negative\");\n} else {\n    print(\"zero\");\n}\n```"),
    ("else", "keyword else", "The `else` branch of an `if`; combines with `elif` for chains."),
    ("elif", "keyword elif", "Else-if chaining.\n\n```adesh\nif (a) { ... } elif (b) { ... } else { ... }\n```"),
    ("while", "keyword while", "While loop.\n\n```adesh\nwhile (i < 10) {\n    i = i + 1;\n}\n```"),
    ("do", "keyword do", "Do-while loop: the body always executes at least once.\n\n```adesh\ndo {\n    work();\n} while (condition);\n```"),
    ("for", "keyword for", "For-in loop over collections/ranges.\n\n```adesh\nfor (item in array) {\n    print(item);\n}\n\nfor i in 0..10 {\n    print(i);\n}\n\nfor (key, value) in map {\n    print(key, value);\n}\n```"),
    ("in", "keyword in", "Used in `for ... in ...` loops and `... in collection` membership checks."),
    ("of", "keyword of", "Iterator-style `for ... of ...` loop over iterables."),
    ("return", "keyword return", "Returns a value from a function. Bare `return;` exits an void function.\n\n```adesh\nfn half(x: f64): f64 {\n    return x / 2.0;\n}\n```"),
    ("break", "keyword break", "Exits the innermost loop early.\n\n```adesh\nfor i in 0..100 {\n    if i > 50 { break; }\n}\n```"),
    ("continue", "keyword continue", "Skips to the next iteration of the innermost loop.\n\n```adesh\nfor i in 0..100 {\n    if i % 2 == 0 { continue; }\n    print(i);\n}\n```"),
    ("jump", "keyword jump", "Jumps to a labeled statement (low-level control flow)."),
    ("import", "keyword import", "Imports a module.\n\n```adesh\nimport \"math\" as math;\nimport { add, sub } from \"math\";\nimport \"utils\";\n```"),
    ("export", "keyword export", "Marks a declaration as part of the module's public API.\n\n```adesh\nexport fn public_fn() { ... }\nexport class PublicClass { ... }\nexport const VERSION = \"1.0\";\n```"),
    ("from", "keyword from", "Used in named imports: `import { a, b } from \"module\";`."),
    ("as", "keyword as", "Alias in imports (`import \"m\" as alias`), casts, and rename in for/of destructuring."),
    ("new", "keyword new", "Constructs an instance of a class/type.\n\n```adesh\nlet user = new User(1, \"Ajay\");\nlet user2 = User(1, \"Ajay\"); // constructor call syntax\n```"),
    ("this", "keyword this", "Reference to the current class instance inside methods.\n\n```adesh\nfn get_name(): string {\n    return this.name;\n}\n```"),
    ("self", "keyword self", "Alias of `this` — the current instance inside methods."),
    ("super", "keyword super", "Reference to the parent class inside a subclass.\n\n```adesh\nfn init(x: i32) {\n    super.init(x);\n}\n```"),
    ("true", "keyword true", "Boolean literal `true`."),
    ("false", "keyword false", "Boolean literal `false`."),
    ("null", "keyword null", "The null value. Combine with nullable types (`string?`) for null safety."),
    ("async", "keyword async", "Declares an async function that returns a promise. Use `await` to wait for promises.\n\n```adesh\nasync fn fetch_data(): Promise {\n    let data = await http_get(url);\n    return data;\n}\n```"),
    ("await", "keyword await", "Waits for a promise to resolve. Only valid inside async functions.\n\n```adesh\nlet result = await fetch_data();\n```"),
    ("spawn", "keyword spawn", "Starts a concurrent task without blocking the caller.\n\n```adesh\nspawn { heavy_work(); }\n```"),
    ("try", "keyword try", "Try-catch block for error handling.\n\n```adesh\ntry {\n    risky_operation();\n} catch (e) {\n    print(\"Error: \" + e);\n}\n```"),
    ("catch", "keyword catch", "Catches an error thrown in a `try` block. The error variable is in scope in the block."),
    ("throw", "keyword throw", "Raises an error value that can be caught by `catch`.\n\n```adesh\nthrow \"invalid input\";\n```"),
    ("static", "keyword static", "Class-level member shared by all instances; call it on the type, not an instance.\n\n```adesh\nclass Math {\n    static fn square(x: f64): f64 { return x * x; }\n}\n\nlet s = Math.square(4);\n```"),
    ("get", "keyword get", "Getter accessor on a class property.\n\n```adesh\nclass Circle {\n    radius: f64;\n    get area(): f64 { return 3.14159 * radius * radius; }\n}\n```"),
    ("set", "keyword set", "Setter accessor on a class property.\n\n```adesh\nclass User {\n    set name(v: string) { this._name = v; }\n}\n```"),
    ("operator", "keyword operator", "Defines operator overloads for a type.\n\n```adesh\noperator +(other: Vec2): Vec2 { ... }\n```"),
    ("constructor", "keyword constructor", "Declares the instance constructor of a class.\n\n```adesh\nclass User {\n    constructor(id: i32, name: string) { ... }\n}\n```"),
    ("extends", "keyword extends", "Inherits from a parent class.\n\n```adesh\nclass Dog extends Animal { ... }\n```"),
    ("implements", "keyword implements", "Declares that a class satisfies an interface.\n\n```adesh\nclass Circle implements Shape { ... }\n```"),
    ("abstract", "keyword abstract", "Abstract class/method: cannot be instantiated/requires overriding.\n\n```adesh\nabstract class Animal {\n    abstract fn speak(): string;\n}\n```"),
    ("sealed", "keyword sealed", "Sealed class modifier: prevents the class from being extended outside its own module.\n\n```adesh\nsealed class FinalConfig {\n    // ...\n}\n```"),
    ("extend", "keyword extend", "Opens an existing type to add methods at runtime.\n\n```adesh\nextend on Person {\n    fn greet() { print(\"Hi\"); }\n}\n```"),
    ("on", "keyword on", "Used with `extend` to target the type being extended: `extend on Type { ... }`."),
    ("region", "keyword region", "Region-based memory allocation block. All allocations are freed at once when the region exits.\n\n```adesh\nregion buf {\n    let data = allocate_in(buf, size);\n} // freed here\n```"),
    ("defer", "keyword defer", "Defers execution until scope exit (LIFO order). Guaranteed to run on scope exit.\n\n```adesh\ndefer file.close();\ndefer {\n    cleanup();\n}\n```"),
    ("unsafe", "keyword unsafe", "Unsafe block: disables borrow/ownership checking. Use for FFI and manual memory management.\n\n```adesh\nunsafe {\n    let ptr = alloc<u8>(100);\n    free(ptr);\n}\n```"),
    ("raw", "keyword raw", "Raw array type without metadata overhead (`raw u8[16]`)."),
    ("test", "keyword test", "Declares a test function executed by the test runner.\n\n```adesh\n@test fn math_works() {\n    assert(2 + 2 == 4);\n}\n```"),
    ("decorator", "keyword decorator", "Declares a decorator with compile/runtime/typecheck phases.\n\n```adesh\ndecorator log(name) {\n    runtime { ... }\n}\n```"),
    ("typeof", "keyword typeof", "Type query operator: returns the type of an expression.\n\n```adesh\nlet t = typeof 42;        // \"int\"\nlet kind = typeof value;  // runtime type name\n```"),
    ("instanceof", "keyword instanceof", "Checks if a value is an instance of a class/type.\n\n```adesh\nif (obj instanceof User) { ... }\n```"),
    ("and", "keyword and", "Logical AND (equivalent to `&&`)."),
    ("or", "keyword or", "Logical OR (equivalent to `||`)."),
    ("not", "keyword not", "Logical NOT (equivalent to `!`)."),
    ("vec", "keyword vec", "SIMD vector type keyword: `vec4<f32>` style SIMD vectors."),
    ("alloc", "keyword alloc", "Allocates raw memory (usually inside `unsafe`): `alloc<u8>(size)`."),
    ("free", "keyword free", "Frees memory previously allocated with `alloc`."),
    ("extern", "keyword extern", "Declares a foreign function for FFI.\n\n```adesh\nextern \"C\" fn puts(s: string) -> i32;\n```"),
    ("private", "keyword private", "Visibility modifier: visible only inside the declaring type/module."),
    ("protected", "keyword protected", "Visibility modifier: visible inside the declaring type and subclasses."),
    ("public", "keyword public", "Visibility modifier: visible everywhere."),
    ("default", "keyword default", "Used in `export default`, `match` default arms, and parameter defaults."),
    ("share", "keyword share", "Shared (read-only) reference type. Copyable; read-only access.\n\n```adesh\nshare data = config;\n```"),
    ("strong", "keyword strong", "Strong owning reference (ARC): keeps the value alive.\n\n```adesh\nstrong ref = server_config;\n```"),
    ("weak", "keyword weak", "Weak non-owning reference: does not keep the value alive, prevents cycles.\n\n```adesh\nweak observer = server_config;\n```"),
    ("compile", "keyword compile", "Decorator compile phase: runs at compile time to transform the AST."),
    ("runtime", "keyword runtime", "Decorator runtime phase: wraps behavior at runtime."),
    ("typecheck", "keyword typecheck", "Decorator typecheck phase: enforces compile-time type constraints."),
    ("emit", "keyword emit", "Decorator emit phase: emits code or metadata."),
    ("require", "keyword require", "Contract/require statement: precondition that must hold.\n\n```adesh\nrequire x > 0, \"x must be positive\";\n```"),
    ("proceed", "keyword proceed", "Inside a decorator runtime phase, calls the wrapped original implementation."),
    ("ignore", "keyword ignore", "Test attribute: skip a test (`@ignore`)."),
    ("expect_fail", "keyword expect_fail", "Test attribute: the test is expected to fail (`@expect_fail`)."),
    ("uint", "keyword uint", "Unsigned integer type. Prefer fixed-width forms like `u32`/`u64`."),
    ("int", "keyword int", "Default integer type (alias of `i32`)."),
    ("Int", "keyword int", "Default integer type (alias of `i32`)."),
    // Contextual words used by the language (lexed as identifiers).
    ("pub", "keyword pub", "Public visibility modifier (short form of `public`)."),
    ("mut", "keyword mut", "Mutable modifier for variables and references."),
    ("where", "keyword where", "Generic constraint clause: `fn f<T>(x: T) where T: Trait { ... }`."),
    ("is", "keyword is", "Type-test/case keyword used in pattern matching: `if (x is User) { ... }`."),
    ("yield", "keyword yield", "Pauses a generator/coroutine and produces a value."),
];

/// Built-in functions with full signatures, parameters, and examples.
const BUILTIN_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("print", "fn print(...args, options?)", "Print any number of values to stdout. Values are converted to strings and separated by a space by default.\n\n**Parameters**\n- `...args` — values to print (strings, numbers, booleans, arrays, objects, ...).\n- `options?` — optional trailing object literal with formatting options (below).\n\n**Options**\n| Option | Type | Default | Description |\n| :--- | :--- | :--- | :--- |\n| `sep` | `string` | `\" \"` | Separator between values. |\n| `end` | `string` | `\"\\n\"` | String appended after the last value. |\n| `file` | `string` | - | Write output to this file instead of stdout (creates or appends). |\n| `flush` | `bool` | `false` | Force-flush the output buffer immediately. |\n| `color` | `string` | - | Text color, hex `#RRGGBB` or `#RGB` (ANSI). |\n| `background` | `string` | - | Background color, hex format. |\n| `bold` | `bool` | `false` | Render text bold. |\n| `italic` | `bool` | `false` | Render text italic. |\n| `underline` | `bool` | `false` | Underline text. |\n| `strikethrough` | `bool` | `false` | Strikethrough text. |\n| `pretty` | `bool \\| string` | `false` | Pretty-print complex values: `true`/`\"full\"`, `\"compact\"`, `\"simple\"`, `\"none\"`/`false`. |\n\n**Examples**\n```adesh\nprint(\"Hello\", \"World\");                  // Hello World\nprint(\"apple\", \"banana\", { sep: \", \" }); // apple, banana\nprint(\"Loading\", { end: \"...\" });\nprint(\"Error:\", \"disk full\", { color: \"#FF0000\", bold: true });\nprint(user, { pretty: true });             // pretty-print object\nprint(\"log entry\", { file: \"app.log\" });  // append to file\n```\n\n**Related**\n- `println(...)` — same as `print(..., { end: \"\\n\" })`.\n- `eprint(...)` — prints to stderr (no styling or pretty options)."),
    ("println", "fn println(...args)", "Print values to stdout with a trailing newline. Equivalent to `print(..., { end: \"\\n\" })`.\n\n```adesh\nprintln(\"Line 1\");\nprintln(\"Value:\", 42, \"Status:\", \"OK\");\n```"),
    ("eprint", "fn eprint(...args)", "Print values to standard error (stderr). Useful for diagnostics and error messages. Does **not** support styling or pretty-print options.\n\n```adesh\neprint(\"Error: configuration file missing\");\n```"),
    ("assert", "fn assert(condition: bool, message?: string)", "Fail the test/program if `condition` is false. Optionally include a failure message.\n\n```adesh\nassert(2 + 2 == 4);\nassert(user.id > 0, \"invalid user id\");\n```"),
    ("assert_eq", "fn assert_eq(left, right)", "Fail the test if `left` and `right` are not equal.\n\n```adesh\nassert_eq(add(2, 3), 5);\n```"),
    ("assert_ne", "fn assert_ne(left, right)", "Fail the test if `left` and `right` are equal.\n\n```adesh\nassert_ne(result, expected);\n```"),
    ("input", "fn input(prompt?: string): string", "Read a line from stdin. Prints the optional `prompt` first.\n\n```adesh\nlet name = input(\"What is your name? \");\n```"),
    ("len", "fn len(collection): number", "Get the number of elements/characters in an array, string, map, or collection.\n\n```adesh\nlet n = len([1, 2, 3]);      // 3\nlet s = len(\"hello\");       // 5\n```"),
    ("range", "fn range(start, end, step?): array", "Create an array of numbers from `start` (inclusive) to `end` (exclusive), optionally stepping by `step`.\n\n```adesh\nlet xs = range(0, 5);      // [0, 1, 2, 3, 4]\nlet ys = range(0, 10, 2);  // [0, 2, 4, 6, 8]\n```"),
    ("map", "fn map(arr, fn): array", "Transform each element of an array with a function, returning a new array.\n\n```adesh\nlet squares = map([1, 2, 3], fn(x) { return x * x; });\n// or: [1, 2, 3].map(fn(x) { return x * x; })\n```"),
    ("filter", "fn filter(arr, fn): array", "Return a new array with only the elements for which the predicate returns true.\n\n```adesh\nlet evens = filter([1, 2, 3, 4], fn(x) { return x % 2 == 0; });\n```"),
    ("reduce", "fn reduce(arr, fn, init): any", "Reduce an array to a single value. The function receives `(acc, item)` and returns the new accumulator; `init` is the starting value.\n\n```adesh\nlet sum = reduce([1, 2, 3, 4], fn(acc, x) { return acc + x; }, 0); // 10\n```"),
    ("clock", "fn clock(): number", "High-resolution monotonic timestamp in nanoseconds. Use for timing/benchmarks.\n\n```adesh\nlet start = clock();\nwork();\nprint(\"elapsed ns:\", clock() - start);\n```"),
    ("sleep", "fn sleep(ms: number)", "Pause execution for `ms` milliseconds.\n\n```adesh\nsleep(500);\n```"),
    ("str", "fn str(value): string", "Convert a value to its string representation.\n\n```adesh\nlet s = str(42);        // \"42\"\nlet t = str(true);      // \"true\"\n```"),
    ("min", "fn min(...args): number", "Return the minimum of the given numbers.\n\n```adesh\nlet m = min(3, 1, 2);   // 1\n```"),
    ("max", "fn max(...args): number", "Return the maximum of the given numbers.\n\n```adesh\nlet m = max(3, 1, 2);   // 3\n```"),
    ("sizeof", "fn sizeof<T>(): number", "Return the size of type `T` in bytes.\n\n```adesh\nlet sz = sizeof<i32>();  // 4\n```"),
    ("alignof", "fn alignof<T>(): number", "Return the alignment of type `T` in bytes.\n\n```adesh\nlet a = alignof<i64>();  // 8\n```"),
];

/// Built-in modules with their exported members.
const BUILTIN_MODULES: &[(&str, &str, &str)] = &[
    ("Math", "module Math", "Mathematical functions and constants. Global (no import needed).\n\n**Constants:** `PI`, `E`, `TAU`, `SQRT2`, `LN2`, `LN10`\n\n**Functions:**\n- `abs(x)` — absolute value\n- `floor(x)`, `ceil(x)`, `round(x)` — rounding\n- `sqrt(x)`, `pow(base, exp)` — powers/roots\n- `sin(x)`, `cos(x)`, `tan(x)` — trigonometry (radians)\n- `exp(x)`, `log(x)`, `log10(x)` — exponentials/logarithms\n- `random()` — random float in `[0, 1)`\n- `randomInt(min, max)` — random integer in `[min, max]`\n- `randomRange(min, max)` — random float in `[min, max)`\n- `seed(n)` — seed the RNG\n- `min(...args)`, `max(...args)` — extrema\n\n```adesh\nlet r = Math.sqrt(144);        // 12\nlet c = Math.pow(2, 10);       // 1024\nprint(Math.PI);\n```"),
    ("time", "module time", "High-resolution time functions. Global (no import needed).\n\n**Functions:**\n- `now()` — current time in nanoseconds\n- `nowMs()` — current time in milliseconds\n- `nowUs()` — current time in microseconds\n- `nowSecs()` — current time in seconds\n- `epoch()` — Unix timestamp (seconds)\n- `epochNanos()` — Unix timestamp (nanoseconds)\n\n```adesh\nlet start = time.nowMs();\nwork();\nprint(\"elapsed ms:\", time.nowMs() - start);\n```"),
];

/// Primitive and built-in types.
const BUILTIN_TYPES: &[(&str, &str, &str)] = &[
    ("int", "type int", "Default signed integer type (alias of `i32`). Range: -2,147,483,648 to 2,147,483,647."),
    ("Int", "type int", "Default signed integer type (alias of `i32`). Range: -2,147,483,648 to 2,147,483,647."),
    ("i32", "type i32", "32-bit signed integer. Range: -2,147,483,648 to 2,147,483,647."),
    ("i64", "type i64", "64-bit signed integer. Range: -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807."),
    ("i8", "type i8", "8-bit signed integer. Range: -128 to 127."),
    ("i16", "type i16", "16-bit signed integer. Range: -32,768 to 32,767."),
    ("i128", "type i128", "128-bit signed integer."),
    ("uint", "type uint", "Unsigned integer type. Prefer fixed-width forms like `u32`/`u64` for exact ranges."),
    ("u8", "type u8", "8-bit unsigned integer. Range: 0 to 255."),
    ("u16", "type u16", "16-bit unsigned integer. Range: 0 to 65,535."),
    ("u32", "type u32", "32-bit unsigned integer. Range: 0 to 4,294,967,295."),
    ("u64", "type u64", "64-bit unsigned integer. Range: 0 to 18,446,744,073,709,551,615."),
    ("u128", "type u128", "128-bit unsigned integer."),
    ("f32", "type f32", "32-bit floating point (IEEE 754)."),
    ("f64", "type f64", "64-bit floating point (IEEE 754). Default floating point type."),
    ("float", "type float", "Floating point type (alias of `f64`)."),
    ("number", "type number", "Numeric type: integer or floating point (`int`/`float`)."),
    ("bigint", "type bigint", "Arbitrary-precision integer type."),
    ("bool", "type bool", "Boolean type. Values: `true`, `false`."),
    ("string", "type string", "UTF-8 string type.\n\n**Methods:** `length`, `toUpperCase()`, `toLowerCase()`, `trim()`, `split(sep)`, `replace(old, new)`, `contains(s)`, `startsWith(s)`, `endsWith(s)`, `slice(start, end?)`, `indexOf(s)`, `repeat(n)`, `charAt(i)`, `concat(s)`"),
    ("str", "type string", "UTF-8 string type (alias of `string`)."),
    ("char", "type char", "Unicode character type."),
    ("void", "type void", "Unit/void type. Represents the absence of a value."),
    ("any", "type any", "Any type (dynamic). Disables static type checking for this value."),
    ("never", "type never", "Never type. Represents a value that never returns (e.g. infinite loop, panic)."),
    ("null", "type null", "Null type. Represents the absence of a value. Use with nullable types (`string?`)."),
];

// ===== Legacy API (for backward compatibility) =====

use crate::analysis::{analyze, extract_symbols_with_positions};

/// Get hover information for a position in a document (legacy API)
#[allow(dead_code)]
pub fn get_hover(doc: &Document, line: u32, col: u32) -> Option<Hover> {
    let word = doc.get_word_at(line, col)?;

    // Check built-ins first
    if let Some(hover) = get_builtin_hover(&word) {
        return Some(hover);
    }

    // Check user-defined symbols
    let result = analyze(&doc.content);
    let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

    for sym in &symbols {
        if sym.name == word {
            let mut content = String::new();
            if let Some(sig) = &sym.signature {
                content.push_str(&format!("```adesh\n{}\n```", sig));
            } else if let Some(t) = &sym.type_name {
                content.push_str(&format!("```adesh\n{}: {}\n```", sym.name, t));
            } else {
                content.push_str(&format!("```adesh\n{}\n```", sym.name));
            }
            return Some(create_markdown_hover(&content));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_value(source: &str, line: u32, col: u32) -> Option<String> {
        let index = adeshlang::semantics::index_source(source);
        let workspace = WorkspaceIndex::new();
        let uri = lsp_types::Url::parse("file:///test.adesh").unwrap();
        let doc = Document::new(uri, source.to_string(), 1);
        let hover = get_hover_semantic(&index, &workspace, &doc, line, col)?;
        match hover.contents {
            HoverContents::Markup(m) => Some(m.value),
            _ => None,
        }
    }

    #[test]
    fn test_print_hover_is_detailed() {
        let source = "let x = 1;\nprint(x);\n";
        let h = hover_value(source, 1, 1).expect("hover for print");
        // All print options are documented.
        for opt in ["sep", "end", "file", "flush", "color", "background",
                    "bold", "italic", "underline", "strikethrough", "pretty"] {
            assert!(h.contains(opt), "print hover missing option '{}': {}", opt, h);
        }
        assert!(h.contains("Examples"));
        assert!(h.contains("...args"));
        assert!(h.contains("Hello"));
    }

    #[test]
    fn test_keyword_hover() {
        // `for` is a keyword token, not an identifier.
        let source = "for i in items {\n    print(i);\n}\n";
        let h = hover_value(source, 0, 1).expect("hover for for");
        assert!(h.contains("For-in loop"), "hover: {}", h);
        assert!(h.contains("```adesh"));
    }

    #[test]
    fn test_type_annotated_variable_hover_shows_fields() {
        let source = "class User {\n    id: i32;\n    name: string;\n}\n\nlet user: User = User(1, \"Ajay\");\n";
        let h = hover_value(source, 5, 4).expect("hover for user");
        assert!(h.contains("Members of `User`"), "hover: {}", h);
        assert!(h.contains("id"), "hover: {}", h);
        assert!(h.contains("name"), "hover: {}", h);
    }

    #[test]
    fn test_interface_variable_hover_shows_methods() {
        // Interface method declarations use `fn name(...);` (no return type syntax).
        let source = "interface Drawable {\n    fn draw();\n    fn resize(w: f64, h: f64);\n}\n\nlet d: Drawable = create_drawable();\n";
        let h = hover_value(source, 5, 4).expect("hover for d");
        assert!(h.contains("Members of `Drawable`"), "hover: {}", h);
        assert!(h.contains("draw"), "hover: {}", h);
        assert!(h.contains("resize"), "hover: {}", h);
    }

    #[test]
    fn test_type_name_hover_shows_definition() {
        let source = "class User {\n    id: i32;\n    name: string;\n    fn save(): bool { return true; }\n}\n\nlet user: User = User(1, \"Ajay\");\n";
        // Hover the type name in the annotation.
        let h = hover_value(source, 6, 10).expect("hover for User type");
        assert!(h.contains("**User** (class)"), "hover: {}", h);
        assert!(h.contains("Fields"), "hover: {}", h);
        assert!(h.contains("save"), "hover: {}", h);
    }
}
