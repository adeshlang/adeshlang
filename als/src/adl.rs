//! ADL (Adesh Package Manifest) support — production-grade
//!
//! Provides schema-driven auto-completion, hover hints, validation diagnostics,
//! code actions, document formatting, semantic tokens, document links, and
//! selection ranges for `.adl` files (`adesh.adl` project manifests and
//! `adesh.lock.adl` lockfiles).
//!
//! This is the ADL equivalent of the rich `package.json` / `package-lock.json`
//! editing experience in the JavaScript ecosystem.

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand,
    CompletionItem, CompletionItemKind,
    Diagnostic, DiagnosticSeverity,
    DocumentLink, DocumentSymbol, Documentation,
    FoldingRange, FoldingRangeKind,
    Hover, HoverContents, InsertTextFormat,
    MarkupContent, MarkupKind, Position, Range,
    SelectionRange, SemanticTokenModifier, SemanticTokenType,
    SemanticToken, SemanticTokens, SymbolKind,
    TextEdit, Url, WorkspaceEdit,
};

// ============================================================================
// URI DETECTION
// ============================================================================

/// Check whether a URI refers to an ADL file.
pub fn is_adl_uri(uri: &Url) -> bool {
    let path = uri.path();
    path.ends_with(".adl")
}

/// Check whether a URI refers to a lockfile (`adesh.lock.adl`).
pub fn is_lockfile_uri(uri: &Url) -> bool {
    let path = uri.path();
    path.ends_with("adesh.lock.adl")
}

// ============================================================================
// SCHEMA DEFINITIONS
// ============================================================================

/// Metadata for a single ADL key (used for completion, hover, and validation).
struct AdlField {
    key: &'static str,
    label: &'static str,
    detail: &'static str,
    doc: &'static str,
    snippet: Option<&'static str>,
    /// Whether this field is required in its section.
    required: bool,
    /// Allowed enum values (if any). When present, the value must be one of these.
    enum_values: &'static [(&'static str, &'static str)],
    /// Whether the value is a version requirement string (e.g. `^1.0.0`).
    is_version: bool,
    /// Whether the value is a semantic version string (e.g. `0.1.0`).
    is_semver: bool,
}

impl AdlField {
    /// Create a simple string field.
    const fn new(key: &'static str, detail: &'static str, doc: &'static str) -> Self {
        Self {
            key,
            label: key,
            detail,
            doc,
            snippet: None,
            required: false,
            enum_values: &[],
            is_version: false,
            is_semver: false,
        }
    }

    /// Mark the field as required.
    const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set the snippet.
    const fn snippet(mut self, s: &'static str) -> Self {
        self.snippet = Some(s);
        self
    }

    /// Set allowed enum values with descriptions.
    const fn enums(mut self, vals: &'static [(&'static str, &'static str)]) -> Self {
        self.enum_values = vals;
        self
    }

    /// Mark as a version requirement field.
    const fn version(mut self) -> Self {
        self.is_version = true;
        self
    }

    /// Mark as a semver field.
    const fn semver(mut self) -> Self {
        self.is_semver = true;
        self
    }
}

/// Metadata for an ADL section.
struct AdlSection {
    name: &'static str,
    doc: &'static str,
    fields: &'static [AdlField],
    /// Whether this section is required in a manifest.
    required: bool,
}

// ---- [project] section ----
static PROJECT_FIELDS: &[AdlField] = &[
    AdlField::new(
        "name",
        "string",
        "The name of the project. Must be a valid identifier (letters, digits, hyphens, underscores).",
    )
    .required()
    .snippet("name = \"${1:project_name}\""),
    AdlField::new(
        "version",
        "semver",
        "The semantic version of the project (e.g. `0.1.0`). Follows semver: `MAJOR.MINOR.PATCH`.",
    )
    .required()
    .semver()
    .snippet("version = \"${1:0.1.0}\""),
    AdlField::new(
        "template",
        "\"app\" | \"lib\" | \"workspace\" | \"plugin\" | \"package\"",
        "The project template type.\n\n- `app` — executable application\n- `lib` — reusable library\n- `workspace` — multi-package workspace\n- `plugin` — compiler plugin\n- `package` — distributable package",
    )
    .required()
    .snippet("template = \"${1:app}\"")
    .enums(&[
        ("app", "Executable application"),
        ("lib", "Reusable library"),
        ("workspace", "Multi-package workspace"),
        ("plugin", "Compiler plugin"),
        ("package", "Distributable package"),
    ]),
    AdlField::new(
        "libs",
        "array<string>",
        "List of library names to link against (for native backends).",
    )
    .snippet("libs = [${1:\"libname\"}]"),
];

// ---- [compiler] section ----
static COMPILER_FIELDS: &[AdlField] = &[
    AdlField::new(
        "backend",
        "\"interpreter\" | \"llvm\" | \"native\"",
        "The compilation backend to use.\n\n- `interpreter` — tree-walking interpreter (default)\n- `llvm` — LLVM-based native compilation\n- `native` — direct native code generation",
    )
    .snippet("backend = \"${1:interpreter}\"")
    .enums(&[
        ("interpreter", "Tree-walking interpreter (default)"),
        ("llvm", "LLVM-based native compilation"),
        ("native", "Direct native code generation"),
    ]),
    AdlField::new(
        "opt-level",
        "\"debug\" | \"release\" | \"fast\"",
        "Optimization level.\n\n- `debug` — no optimizations, full debug info\n- `release` — full optimizations, no debug info\n- `fast` — fast compilation with basic optimizations",
    )
    .snippet("opt-level = \"${1:debug}\"")
    .enums(&[
        ("debug", "No optimizations, full debug info"),
        ("release", "Full optimizations, no debug info"),
        ("fast", "Fast compilation with basic optimizations"),
    ]),
];

// ---- [dependencies] section ----
static DEPENDENCY_FIELDS: &[AdlField] = &[
    AdlField::new(
        "<package-name>",
        "version-requirement",
        "Add a dependency by name with a version requirement.\n\nExamples:\n```adl\nCrypto = \"^1.0.0\"\nHTTP = \">=2.0.0\"\n```",
    )
    .version()
    .snippet("${1:PackageName} = \"${2:^1.0.0}\""),
    AdlField::new(
        "<package-name> (path)",
        "object",
        "Path-based dependency. Resolves from a local directory.\n\n```adl\nMyLib = { source = \"path\", location = \"../my-lib\" }\n```",
    )
    .snippet("${1:PackageName} = { source = \"path\", location = \"${2:../path/to/lib}\" }"),
    AdlField::new(
        "<package-name> (git)",
        "object",
        "Git-based dependency. Clones from a git repository.\n\n```adl\nMyLib = { source = \"git\", location = \"https://github.com/user/repo\" }\n```",
    )
    .snippet("${1:PackageName} = { source = \"git\", location = \"${2:https://github.com/user/repo}\" }"),
    AdlField::new(
        "<package-name> (registry)",
        "object",
        "Registry-based dependency. Downloads from a named registry.\n\n```adl\nMyLib = { source = \"registry\", location = \"official\" }\n```",
    )
    .snippet("${1:PackageName} = { source = \"registry\", location = \"${2:official}\" }"),
    AdlField::new(
        "<package-name> (workspace)",
        "object",
        "Workspace dependency. Resolves from a workspace member.\n\n```adl\nMyLib = { source = \"workspace\", location = \"packages/my-lib\" }\n```",
    )
    .snippet("${1:PackageName} = { source = \"workspace\", location = \"${2:packages/my-lib}\" }"),
];

// ---- [scripts] section ----
static SCRIPTS_FIELDS: &[AdlField] = &[
    AdlField::new(
        "const <name>",
        "binding",
        "Define a named script binding. Use `const` for fixed values or `let` for parameterized scripts.\n\n```adl\nconst build = \"default\"\nconst test = \"default\"\n```",
    )
    .snippet("const ${1:name} = \"${2:default}\""),
    AdlField::new(
        "let <name>",
        "binding",
        "Define a mutable script binding.\n\n```adl\nlet deploy = \"default\"\n```",
    )
    .snippet("let ${1:name} = \"${2:default}\""),
];

// ---- [workspace] section ----
static WORKSPACE_FIELDS: &[AdlField] = &[
    AdlField::new(
        "members",
        "array<string>",
        "List of workspace member paths. Supports glob patterns with `/*`.\n\n```adl\nmembers = [\"packages/*\", \"tools/cli\"]\n```",
    )
    .snippet("members = [${1:\"packages/*\"}]"),
];

// ---- Lockfile top-level keys (inside `lock { }`) ----
static LOCKFILE_FIELDS: &[AdlField] = &[
    AdlField::new(
        "version",
        "string",
        "Lockfile format version (currently `\"1\"`).",
    )
    .required()
    .snippet("version = \"${1:1}\""),
    AdlField::new(
        "generated-at",
        "ISO-8601 timestamp",
        "Timestamp when the lockfile was generated (RFC 3339 / ISO 8601 format).",
    )
    .snippet("generated-at = \"${1:2026-01-01T00:00:00Z}\""),
    AdlField::new(
        "compiler-version",
        "semver string",
        "Version of the Adesh compiler that generated this lockfile.",
    )
    .semver()
    .snippet("compiler-version = \"${1:0.3.0}\""),
    AdlField::new(
        "adl-version",
        "semver string",
        "Version of the ADL package manager that generated this lockfile.",
    )
    .semver()
    .snippet("adl-version = \"${1:0.3.0}\""),
    AdlField::new(
        "package-id",
        "string",
        "Unique identifier for the package this lockfile belongs to.",
    )
    .snippet("package-id = \"${1:my_project}\""),
    AdlField::new(
        "dependencies",
        "array<object>",
        "Locked dependency entries. Each entry records the exact version, checksum, and source for a dependency.",
    )
    .snippet("dependencies = [\n  {\n    package-id = \"${1:Name@0.0.0}\"\n    name = \"${2:Name}\"\n    version = \"${3:0.0.0}\"\n    requirement = \"${4:^1.0.0}\"\n    checksum = \"${5:0000000000000000}\"\n    sha256 = \"${5:0000000000000000}\"\n    source = { kind = \"${6:registry}\", location = \"${7:official}\" }\n  }\n]"),
    AdlField::new(
        "confidential",
        "object",
        "Sensitive key-value pairs (tokens, secrets) that are stripped from the published manifest.",
    )
    .snippet("confidential = {\n  ${1:key} = \"${2:value}\"\n}"),
    AdlField::new(
        "signature",
        "hex string",
        "Integrity signature (FNV-1a hash) computed over all lockfile fields. Used to detect tampering.",
    )
    .snippet("signature = \"${1:0000000000000000}\""),
];

// ---- Lockfile dependency entry keys ----
static LOCK_DEP_FIELDS: &[AdlField] = &[
    AdlField::new(
        "package-id",
        "string",
        "Unique identifier for the dependency, typically `Name@version`.",
    )
    .required()
    .snippet("package-id = \"${1:Name@0.0.0}\""),
    AdlField::new(
        "name",
        "string",
        "The package name.",
    )
    .required()
    .snippet("name = \"${1:Name}\""),
    AdlField::new(
        "version",
        "semver",
        "The exact resolved version of the dependency.",
    )
    .required()
    .semver()
    .snippet("version = \"${1:0.0.0}\""),
    AdlField::new(
        "requirement",
        "version-requirement",
        "The version requirement from the manifest that was resolved to this version (e.g. `^1.0.0`).",
    )
    .version()
    .snippet("requirement = \"${1:^1.0.0}\""),
    AdlField::new(
        "checksum",
        "hex string",
        "Short checksum (16 hex chars) for integrity verification.",
    )
    .snippet("checksum = \"${1:0000000000000000}\""),
    AdlField::new(
        "sha256",
        "hex string",
        "SHA-256 checksum for content verification.",
    )
    .snippet("sha256 = \"${1:0000000000000000}\""),
    AdlField::new(
        "source",
        "object",
        "Source descriptor for the dependency.\n\n```adl\nsource = { kind = \"registry\", location = \"official\" }\n```\n\n- `kind`: `\"registry\"`, `\"git\"`, `\"path\"`, `\"local\"`, or `\"workspace\"`\n- `location`: registry name, git URL, or filesystem path",
    )
    .snippet("source = { kind = \"${1:registry}\", location = \"${2:official}\" }"),
    AdlField::new(
        "features",
        "array<string>",
        "Feature flags enabled for this dependency.",
    )
    .snippet("features = [${1:\"feature1\"}]"),
    AdlField::new(
        "registries",
        "array<string>",
        "Registries consulted when resolving this dependency.",
    )
    .snippet("registries = [${1:\"official\"}]"),
    AdlField::new(
        "transitive",
        "array<string>",
        "Transitive dependency IDs pulled in by this dependency.",
    )
    .snippet("transitive = [${1:\"Other@0.0.0\"}]"),
    AdlField::new(
        "target",
        "string",
        "Target platform triple (e.g. `x86_64-pc-windows-msvc`). If absent, the dependency applies to all targets.",
    )
    .snippet("target = \"${1:x86_64-pc-windows-msvc}\""),
    AdlField::new(
        "profile",
        "string",
        "Build profile for this dependency (e.g. `debug`, `release`).",
    )
    .snippet("profile = \"${1:debug}\"")
    .enums(&[
        ("debug", "Debug profile — no optimizations"),
        ("release", "Release profile — full optimizations"),
        ("fast", "Fast profile — basic optimizations"),
    ]),
];

// ---- Source object keys ----
static SOURCE_FIELDS: &[AdlField] = &[
    AdlField::new(
        "kind",
        "\"registry\" | \"git\" | \"path\" | \"local\" | \"workspace\"",
        "The source kind for a dependency.\n\n- `registry` — download from a named registry\n- `git` — clone from a git URL\n- `path` — resolve from a local filesystem path\n- `local` — same as path but not published\n- `workspace` — resolve from a workspace member",
    )
    .required()
    .snippet("kind = \"${1:registry}\"")
    .enums(&[
        ("registry", "Download from a named registry"),
        ("git", "Clone from a git URL"),
        ("path", "Resolve from a local filesystem path"),
        ("local", "Same as path but not published"),
        ("workspace", "Resolve from a workspace member"),
    ]),
    AdlField::new(
        "location",
        "string",
        "The source location. For `registry`, the registry name (e.g. `\"official\"`). For `git`, the repository URL. For `path`/`local`/`workspace`, the filesystem path.",
    )
    .required()
    .snippet("location = \"${1:official}\""),
];

// ---- Section definitions ----
static SECTIONS: &[AdlSection] = &[
    AdlSection {
        name: "project",
        doc: "Project metadata section. Defines the project name, version, and template type.",
        fields: PROJECT_FIELDS,
        required: true,
    },
    AdlSection {
        name: "compiler",
        doc: "Compiler configuration section. Controls the backend and optimization level.",
        fields: COMPILER_FIELDS,
        required: false,
    },
    AdlSection {
        name: "dependencies",
        doc: "Dependencies section. Lists all external packages the project depends on, each with a version requirement.",
        fields: DEPENDENCY_FIELDS,
        required: false,
    },
    AdlSection {
        name: "scripts",
        doc: "Scripts section. Defines named build/test/deploy tasks using `const` or `let` bindings.",
        fields: SCRIPTS_FIELDS,
        required: false,
    },
    AdlSection {
        name: "workspace",
        doc: "Workspace section. Defines workspace members for multi-package projects.",
        fields: WORKSPACE_FIELDS,
        required: false,
    },
];

/// Known section names.
const KNOWN_SECTIONS: &[&str] = &["project", "compiler", "dependencies", "scripts", "workspace"];

/// Known lockfile top-level keys.
const KNOWN_LOCK_KEYS: &[&str] = &[
    "version", "generated-at", "compiler-version", "adl-version",
    "package-id", "dependencies", "confidential", "signature",
];

/// Known lockfile dependency entry keys.
const KNOWN_LOCK_DEP_KEYS: &[&str] = &[
    "package-id", "name", "version", "requirement", "checksum",
    "sha256", "source", "features", "registries", "transitive",
    "target", "profile",
];

/// Known source object keys.
#[allow(dead_code)]
const KNOWN_SOURCE_KEYS: &[&str] = &["kind", "location"];

// ============================================================================
// CONTEXT DETECTION
// ============================================================================

/// The parsing context within an ADL file, determined by cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdlContext {
    /// Top-level (outside any section or lock block)
    TopLevel,
    /// Inside a `[section]` block
    Section(&'static str),
    /// Inside the `lock { }` block (top-level keys)
    LockFile,
    /// Inside a dependency entry object in the lockfile
    LockDependency,
    /// Inside a `source = { }` object
    SourceObject,
    /// Inside a `confidential = { }` object
    ConfidentialObject,
    /// Unknown / unparseable context
    Unknown,
}

/// More granular position within a key-value line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValuePosition {
    /// On the key (before `=`)
    Key,
    /// On the value (after `=`)
    Value,
    /// Not on a key-value line
    Other,
}

/// Detect the ADL context at a given position in the document.
pub fn detect_context(content: &str, line: u32, _character: u32) -> AdlContext {
    let lines: Vec<&str> = content.lines().collect();
    let current_line = line as usize;

    let mut current_section: Option<&'static str> = None;
    let mut in_lock = false;
    let mut in_dep_array = false;
    let mut in_dep_object = false;
    let mut in_source_object = false;
    let mut in_confidential = false;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for (i, line_text) in lines.iter().enumerate() {
        let trimmed = line_text.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            if i == current_line {
                return current_context(current_section, in_lock, in_dep_object, in_source_object, in_confidential);
            }
            continue;
        }

        // Track lock block
        if trimmed.starts_with("lock") && trimmed.contains('{') {
            in_lock = true;
            brace_depth = 1;
            current_section = None;
            if i == current_line {
                return AdlContext::LockFile;
            }
            continue;
        }

        if in_lock {
            // Count braces to detect end of lock block
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            if brace_depth <= 0 {
                in_lock = false;
                in_dep_array = false;
                in_dep_object = false;
                in_source_object = false;
                in_confidential = false;
                if i == current_line {
                    return AdlContext::TopLevel;
                }
                continue;
            }

            // Track confidential block
            if trimmed.starts_with("confidential") && trimmed.contains('=') && trimmed.contains('{') {
                in_confidential = true;
                if i == current_line {
                    return AdlContext::ConfidentialObject;
                }
                continue;
            }
            if in_confidential {
                // Simple single-level object for confidential
                if trimmed == "}" {
                    in_confidential = false;
                }
                if i == current_line {
                    return AdlContext::ConfidentialObject;
                }
                continue;
            }

            // Track dependencies array
            if trimmed.starts_with("dependencies") && trimmed.contains('=') && trimmed.contains('[') {
                in_dep_array = true;
                bracket_depth = 1;
                // Count closing brackets on same line
                for ch in trimmed.chars().skip(trimmed.find('[').unwrap_or(0) + 1) {
                    match ch {
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                }
                if bracket_depth <= 0 {
                    in_dep_array = false;
                }
                if i == current_line {
                    return AdlContext::LockFile;
                }
                continue;
            }
            if in_dep_array {
                for ch in trimmed.chars() {
                    match ch {
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                }
                if bracket_depth <= 0 {
                    in_dep_array = false;
                    if i == current_line {
                        return AdlContext::LockFile;
                    }
                    continue;
                }
                // Track individual dependency objects
                if trimmed.starts_with('{') {
                    in_dep_object = true;
                    brace_depth = 1;
                    // Count closing braces on same line
                    for ch in trimmed.chars().skip(1) {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' => brace_depth -= 1,
                            _ => {}
                        }
                    }
                    if brace_depth <= 0 {
                        in_dep_object = false;
                    }
                    if i == current_line {
                        // Check if this line has a source object
                        if trimmed.contains("source") && trimmed.contains("{") {
                            return AdlContext::SourceObject;
                        }
                        return if in_dep_object { AdlContext::LockDependency } else { AdlContext::LockFile };
                    }
                    continue;
                }
                if in_dep_object {
                    for ch in trimmed.chars() {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' => brace_depth -= 1,
                            _ => {}
                        }
                    }
                    if brace_depth <= 0 {
                        in_dep_object = false;
                        in_source_object = false;
                        if i == current_line {
                            return AdlContext::LockFile;
                        }
                        continue;
                    }
                    // Check if we're inside a source object
                    if trimmed.contains("source") && trimmed.contains('{') {
                        // This is the source = { line
                        in_source_object = true;
                        if i == current_line {
                            return AdlContext::SourceObject;
                        }
                        continue;
                    }
                    if in_source_object {
                        // Check if source object ends on this line
                        if trimmed.contains('}') && !trimmed.contains('{') {
                            in_source_object = false;
                        }
                        if i == current_line {
                            return AdlContext::SourceObject;
                        }
                        continue;
                    }
                    if i == current_line {
                        return AdlContext::LockDependency;
                    }
                    continue;
                }
                if i == current_line {
                    return AdlContext::LockFile;
                }
                continue;
            }
            if i == current_line {
                return AdlContext::LockFile;
            }
            continue;
        }

        // If we've reached the cursor line, return the current context.
        if i == current_line {
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                return AdlContext::TopLevel;
            }
            return match current_section {
                Some(name) => AdlContext::Section(name),
                None => AdlContext::TopLevel,
            };
        }

        // Track section headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section_name = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']');
            current_section = SECTIONS
                .iter()
                .find(|s| s.name == section_name)
                .map(|s| s.name);
            continue;
        }
    }

    current_context(current_section, in_lock, in_dep_object, false, in_confidential)
}

fn current_context(
    section: Option<&'static str>,
    in_lock: bool,
    in_dep: bool,
    in_source: bool,
    in_conf: bool,
) -> AdlContext {
    if in_conf {
        return AdlContext::ConfidentialObject;
    }
    if in_source {
        return AdlContext::SourceObject;
    }
    if in_dep {
        return AdlContext::LockDependency;
    }
    if in_lock {
        return AdlContext::LockFile;
    }
    match section {
        Some(name) => AdlContext::Section(name),
        None => AdlContext::TopLevel,
    }
}

/// Determine whether the cursor is on the key or value part of a key-value line.
fn detect_value_position(line: &str, character: u32) -> ValuePosition {
    let char_idx = character as usize;
    let eq_pos = line.find('=');
    match eq_pos {
        Some(pos) if char_idx > pos => ValuePosition::Value,
        Some(_) => ValuePosition::Key,
        None => ValuePosition::Other,
    }
}

// ============================================================================
// COMPLETION
// ============================================================================

/// Generate completion items for an ADL file at the given position.
pub fn get_adl_completions(
    content: &str,
    line: u32,
    character: u32,
) -> Vec<CompletionItem> {
    let context = detect_context(content, line, character);
    let lines: Vec<&str> = content.lines().collect();
    let line_text = if (line as usize) < lines.len() { lines[line as usize] } else { "" };
    let value_pos = detect_value_position(line_text, character);

    let mut items = Vec::new();

    match context {
        AdlContext::TopLevel => {
            // Suggest section headers
            for section in SECTIONS {
                items.push(section_to_completion(section));
            }
            // Also suggest import and conditional
            items.push(keyword_completion(
                "import",
                "import statement",
                "Import another ADL manifest file.\n\n```adl\nimport \"./path/to/file.adl\"\n```",
                "import \"${1:./path}\"",
            ));
            items.push(keyword_completion(
                "if",
                "conditional block",
                "Conditional block for platform/target-specific configuration.\n\n```adl\nif target == \"wasm\" {\n  // ...\n}\n```",
                "if ${1:condition} {\n  $0\n}",
            ));
        }
        AdlContext::Section(section_name) => {
            if let Some(section) = SECTIONS.iter().find(|s| s.name == section_name) {
                // If we're on the value side of a key-value line, suggest enum values
                if value_pos == ValuePosition::Value {
                    let key = extract_key_from_line(line_text);
                    if let Some(field) = section.fields.iter().find(|f| f.key == key) {
                        if !field.enum_values.is_empty() {
                            for (val, desc) in field.enum_values {
                                items.push(enum_value_completion(val, desc));
                            }
                        }
                    }
                } else {
                    // Suggest field keys
                    for field in section.fields {
                        items.push(field_to_completion(field, None));
                    }
                }
            }
        }
        AdlContext::LockFile => {
            if value_pos == ValuePosition::Value {
                let key = extract_key_from_line(line_text);
                if let Some(field) = LOCKFILE_FIELDS.iter().find(|f| f.key == key) {
                    if !field.enum_values.is_empty() {
                        for (val, desc) in field.enum_values {
                            items.push(enum_value_completion(val, desc));
                        }
                    }
                }
            } else {
                for field in LOCKFILE_FIELDS {
                    items.push(field_to_completion(field, None));
                }
            }
        }
        AdlContext::LockDependency => {
            if value_pos == ValuePosition::Value {
                let key = extract_key_from_line(line_text);
                if let Some(field) = LOCK_DEP_FIELDS.iter().find(|f| f.key == key) {
                    if !field.enum_values.is_empty() {
                        for (val, desc) in field.enum_values {
                            items.push(enum_value_completion(val, desc));
                        }
                    }
                }
            } else {
                for field in LOCK_DEP_FIELDS {
                    items.push(field_to_completion(field, None));
                }
            }
        }
        AdlContext::SourceObject => {
            if value_pos == ValuePosition::Value {
                let key = extract_key_from_line(line_text);
                if key == "kind" {
                    for (val, desc) in SOURCE_FIELDS[0].enum_values {
                        items.push(enum_value_completion(val, desc));
                    }
                }
            } else {
                for field in SOURCE_FIELDS {
                    items.push(field_to_completion(field, None));
                }
            }
        }
        AdlContext::ConfidentialObject => {
            // Confidential objects accept any key-value pairs
            items.push(CompletionItem {
                label: "<key>".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some("string key".to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "A confidential key-value pair. Keys containing 'token', 'secret', 'password', 'key', 'credential' are automatically detected and stripped from published manifests.".to_string(),
                })),
                insert_text: Some("${1:key} = \"${2:value}\"".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }
        AdlContext::Unknown => {}
    }

    items
}

fn section_to_completion(section: &AdlSection) -> CompletionItem {
    CompletionItem {
        label: format!("[{}]", section.name),
        kind: Some(CompletionItemKind::MODULE),
        detail: Some("section".to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: section.doc.to_string(),
        })),
        insert_text: Some(format!("[{}]\n", section.name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text: Some(format!("0_{}", section.name)),
        ..Default::default()
    }
}

fn field_to_completion(field: &AdlField, override_label: Option<&str>) -> CompletionItem {
    let label = override_label.unwrap_or(field.label).to_string();
    let insert_text = field.snippet.unwrap_or(field.key).to_string();

    let mut item = CompletionItem {
        label: label.clone(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(field.detail.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: field.doc.to_string(),
        })),
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    };

    if field.required {
        item.label_details = Some(lsp_types::CompletionItemLabelDetails {
            detail: Some(" (required)".to_string()),
            description: None,
        });
    }

    item
}

fn enum_value_completion(value: &str, desc: &str) -> CompletionItem {
    CompletionItem {
        label: value.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(desc.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: desc.to_string(),
        })),
        insert_text: Some(format!("\"{}\"", value)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }
}

fn keyword_completion(label: &str, detail: &str, doc: &str, snippet: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(detail.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.to_string(),
        })),
        insert_text: Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn extract_key_from_line(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(eq_pos) = trimmed.find('=') {
        let key = trimmed[..eq_pos].trim();
        // Remove section prefix if present
        key.to_string()
    } else {
        String::new()
    }
}

// ============================================================================
// HOVER
// ============================================================================

/// Generate hover information for an ADL file at the given position.
pub fn get_adl_hover(content: &str, line: u32, character: u32) -> Option<Hover> {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = line as usize;
    if line_idx >= lines.len() {
        return None;
    }

    let line_text = lines[line_idx];
    let context = detect_context(content, line, character);

    let char_idx = character as usize;
    let word = extract_word(line_text, char_idx)?;

    // Look up the word in the appropriate schema
    let fields = match context {
        AdlContext::Section(section_name) => {
            SECTIONS.iter().find(|s| s.name == section_name)?.fields
        }
        AdlContext::LockFile => LOCKFILE_FIELDS,
        AdlContext::LockDependency => LOCK_DEP_FIELDS,
        AdlContext::SourceObject => SOURCE_FIELDS,
        AdlContext::TopLevel => {
            if let Some(section) = SECTIONS.iter().find(|s| s.name == word) {
                return Some(make_hover(&format!("[{}]", section.name), "section", section.doc));
            }
            return None;
        }
        AdlContext::ConfidentialObject => {
            return Some(make_hover(&word, "confidential key", "A confidential key-value pair. This entry will be stripped from the manifest when publishing, and stored only in the lockfile's `confidential` block."));
        }
        AdlContext::Unknown => return None,
    };

    for field in fields {
        if field.key == word || field.label == word {
            return Some(make_hover(field.label, field.detail, field.doc));
        }
    }

    // Check if the word is a section name
    if let Some(section) = SECTIONS.iter().find(|s| s.name == word) {
        return Some(make_hover(&format!("[{}]", section.name), "section", section.doc));
    }

    // Check if the word is a known lockfile key
    if KNOWN_LOCK_KEYS.contains(&word.as_str()) {
        if let Some(field) = LOCKFILE_FIELDS.iter().find(|f| f.key == word) {
            return Some(make_hover(field.label, field.detail, field.doc));
        }
    }

    None
}

fn make_hover(label: &str, detail: &str, doc: &str) -> Hover {
    let markdown = format!(
        "```adl\n{}\n```\n\n{}\n\n---\n**Type:** `{}`",
        label, doc, detail
    );
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    }
}

fn extract_word(line: &str, char_idx: usize) -> Option<String> {
    if line.is_empty() || char_idx > line.len() {
        return None;
    }

    let bytes = line.as_bytes();
    let mut start = char_idx.min(bytes.len().saturating_sub(1));
    let mut end = start;

    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'_'
            || bytes[start - 1] == b'-')
    {
        start -= 1;
    }

    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'-')
    {
        end += 1;
    }

    if start < end {
        Some(String::from_utf8_lossy(&bytes[start..end]).to_string())
    } else {
        None
    }
}

// ============================================================================
// DIAGNOSTICS / VALIDATION
// ============================================================================

/// Compute validation diagnostics for an ADL file.
pub fn compute_adl_diagnostics(content: &str, uri: &Url) -> Vec<Diagnostic> {
    if is_lockfile_uri(uri) {
        validate_lockfile(content)
    } else {
        validate_manifest(content)
    }
}

/// Validate a manifest file (`adesh.adl`).
fn validate_manifest(content: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = content.lines().collect();
    let mut diagnostics = Vec::new();

    let mut found_sections: Vec<String> = Vec::new();
    let mut current_section: Option<String> = None;
    let mut seen_keys: Vec<(String, String)> = Vec::new(); // (section, key)

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section_name = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();

            // Check for unknown sections
            if !KNOWN_SECTIONS.contains(&section_name.as_str()) {
                diagnostics.push(Diagnostic {
                    range: line_range(i, trimmed),
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(lsp_types::NumberOrString::String("ADL001".to_string())),
                    code_description: None,
                    source: Some("adl".to_string()),
                    message: format!(
                        "Unknown section `[{}]`. Known sections: {}",
                        section_name,
                        KNOWN_SECTIONS.join(", ")
                    ),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            }

            // Check for duplicate sections
            if found_sections.contains(&section_name) {
                diagnostics.push(Diagnostic {
                    range: line_range(i, trimmed),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(lsp_types::NumberOrString::String("ADL002".to_string())),
                    code_description: None,
                    source: Some("adl".to_string()),
                    message: format!("Duplicate section `[{}]`", section_name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
            } else {
                found_sections.push(section_name.clone());
            }

            current_section = Some(section_name);
            seen_keys.clear();
            continue;
        }

        // Skip import, if/else, let/const bindings (not key-value)
        if trimmed.starts_with("import ")
            || trimmed.starts_with("if ")
            || trimmed.starts_with("else")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("const ")
        {
            continue;
        }

        // Key-value pair
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();

            if let Some(ref section) = current_section {
                // Check for duplicate keys within the same section
                let seen_key = (section.clone(), key.clone());
                if seen_keys.contains(&seen_key) {
                    diagnostics.push(Diagnostic {
                        range: line_range(i, trimmed),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(lsp_types::NumberOrString::String("ADL003".to_string())),
                        code_description: None,
                        source: Some("adl".to_string()),
                        message: format!("Duplicate key `{}` in section `[{}]`", key, section),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                } else {
                    seen_keys.push(seen_key);
                }

                // Validate known keys and values for known sections
                if let Some(section_def) = SECTIONS.iter().find(|s| s.name == section.as_str()) {
                    if let Some(field) = section_def.fields.iter().find(|f| f.key == key) {
                        // Validate enum values
                        if !field.enum_values.is_empty() {
                            let val_clean = value.trim_matches('"');
                            if !field.enum_values.iter().any(|(v, _)| *v == val_clean) {
                                let allowed: Vec<&str> =
                                    field.enum_values.iter().map(|(v, _)| *v).collect();
                                diagnostics.push(Diagnostic {
                                    range: value_range(i, trimmed, eq_pos),
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    code: Some(lsp_types::NumberOrString::String("ADL004".to_string())),
                                    code_description: None,
                                    source: Some("adl".to_string()),
                                    message: format!(
                                        "Invalid value `{}` for `{}`. Allowed: {}",
                                        val_clean,
                                        key,
                                        allowed.join(", ")
                                    ),
                                    related_information: None,
                                    tags: None,
                                    data: None,
                                });
                            }
                        }

                        // Validate semver
                        if field.is_semver {
                            let val_clean = value.trim_matches('"');
                            if !is_valid_semver(val_clean) {
                                diagnostics.push(Diagnostic {
                                    range: value_range(i, trimmed, eq_pos),
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    code: Some(lsp_types::NumberOrString::String("ADL005".to_string())),
                                    code_description: None,
                                    source: Some("adl".to_string()),
                                    message: format!(
                                        "Invalid semantic version `{}`. Expected format: `MAJOR.MINOR.PATCH` (e.g. `0.1.0`)",
                                        val_clean
                                    ),
                                    related_information: None,
                                    tags: None,
                                    data: None,
                                });
                            }
                        }

                        // Validate version requirement
                        if field.is_version {
                            let val_clean = value.trim_matches('"');
                            if !is_valid_version_req(val_clean) {
                                diagnostics.push(Diagnostic {
                                    range: value_range(i, trimmed, eq_pos),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    code: Some(lsp_types::NumberOrString::String("ADL006".to_string())),
                                    code_description: None,
                                    source: Some("adl".to_string()),
                                    message: format!(
                                        "Potentially invalid version requirement `{}`. Expected: `^1.0.0`, `~1.2.3`, `>=2.0.0`, `*`, etc.",
                                        val_clean
                                    ),
                                    related_information: None,
                                    tags: None,
                                    data: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Check for required sections
    for section in SECTIONS.iter() {
        if section.required && !found_sections.iter().any(|s| s == section.name) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(lsp_types::NumberOrString::String("ADL007".to_string())),
                code_description: None,
                source: Some("adl".to_string()),
                message: format!(
                    "Missing required section `[{}]`. Add `[{}]` to the manifest.",
                    section.name, section.name
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    // Check for required fields in found sections
    for section in SECTIONS.iter() {
        if found_sections.iter().any(|s| s == section.name) {
            for field in section.fields {
                if field.required {
                    // Check if the key exists in the file
                    let key_exists = content.lines().any(|line| {
                        let trimmed = line.trim();
                        if let Some(eq_pos) = trimmed.find('=') {
                            let key = trimmed[..eq_pos].trim();
                            key == field.key
                        } else {
                            false
                        }
                    });
                    if !key_exists {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position { line: 0, character: 0 },
                                end: Position { line: 0, character: 1 },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(lsp_types::NumberOrString::String("ADL008".to_string())),
                            code_description: None,
                            source: Some("adl".to_string()),
                            message: format!(
                                "Missing required field `{}` in section `[{}]`",
                                field.key, section.name
                            ),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
        }
    }

    diagnostics
}

/// Validate a lockfile (`adesh.lock.adl`).
fn validate_lockfile(content: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = content.lines().collect();
    let mut diagnostics = Vec::new();

    let mut in_lock = false;
    let mut in_dep_array = false;
    let mut in_dep_object = false;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut seen_keys: Vec<String> = Vec::new();
    let mut dep_keys: Vec<String> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("lock") && trimmed.contains('{') {
            in_lock = true;
            brace_depth = 1;
            continue;
        }

        if in_lock {
            for ch in trimmed.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
            if brace_depth <= 0 {
                in_lock = false;
                continue;
            }

            if trimmed.starts_with("dependencies") && trimmed.contains('=') && trimmed.contains('[') {
                in_dep_array = true;
                bracket_depth = 1;
                continue;
            }

            if in_dep_array {
                for ch in trimmed.chars() {
                    match ch {
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                }
                if bracket_depth <= 0 {
                    in_dep_array = false;
                    continue;
                }

                if trimmed.starts_with('{') {
                    in_dep_object = true;
                    brace_depth = 1;
                    dep_keys.clear();
                    continue;
                }

                if in_dep_object {
                    for ch in trimmed.chars() {
                        match ch {
                            '{' => brace_depth += 1,
                            '}' => brace_depth -= 1,
                            _ => {}
                        }
                    }
                    if brace_depth <= 0 {
                        in_dep_object = false;
                        // Check required fields in dependency entry
                        for field in LOCK_DEP_FIELDS {
                            if field.required && !dep_keys.contains(&field.key.to_string()) {
                                diagnostics.push(Diagnostic {
                                    range: line_range(i, trimmed),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    code: Some(lsp_types::NumberOrString::String("ADL010".to_string())),
                                    code_description: None,
                                    source: Some("adl".to_string()),
                                    message: format!(
                                        "Dependency entry missing required field `{}`",
                                        field.key
                                    ),
                                    related_information: None,
                                    tags: None,
                                    data: None,
                                });
                            }
                        }
                        continue;
                    }

                    if let Some(eq_pos) = trimmed.find('=') {
                        let key = trimmed[..eq_pos].trim().to_string();
                        if !KNOWN_LOCK_DEP_KEYS.contains(&key.as_str()) && key != "source" {
                            diagnostics.push(Diagnostic {
                                range: line_range(i, trimmed),
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: Some(lsp_types::NumberOrString::String("ADL011".to_string())),
                                code_description: None,
                                source: Some("adl".to_string()),
                                message: format!("Unknown key `{}` in dependency entry", key),
                                related_information: None,
                                tags: None,
                                data: None,
                            });
                        }
                        dep_keys.push(key);
                    }
                    continue;
                }
                continue;
            }

            // Top-level lockfile keys
            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim().to_string();

                if !KNOWN_LOCK_KEYS.contains(&key.as_str()) && key != "dependencies" {
                    diagnostics.push(Diagnostic {
                        range: line_range(i, trimmed),
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(lsp_types::NumberOrString::String("ADL012".to_string())),
                        code_description: None,
                        source: Some("adl".to_string()),
                        message: format!("Unknown key `{}` in lock block", key),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }

                if seen_keys.contains(&key) {
                    diagnostics.push(Diagnostic {
                        range: line_range(i, trimmed),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(lsp_types::NumberOrString::String("ADL013".to_string())),
                        code_description: None,
                        source: Some("adl".to_string()),
                        message: format!("Duplicate key `{}` in lock block", key),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                } else {
                    seen_keys.push(key);
                }
            }
            continue;
        }
    }

    diagnostics
}

fn is_valid_semver(s: &str) -> bool {
    // Match: MAJOR.MINOR.PATCH with optional pre-release and build metadata
    let parts: Vec<&str> = s.split('-').collect();
    let main = parts[0];
    let main_parts: Vec<&str> = main.split('.').collect();
    if main_parts.len() != 3 {
        return false;
    }
    main_parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty())
}

fn is_valid_version_req(s: &str) -> bool {
    let s = s.trim();
    if s == "*" || s == "any" {
        return true;
    }
    // Remove leading operator
    let version_part = s
        .trim_start_matches(['^', '~', '>', '<', '='])
        .trim();
    // Check if the remaining part looks like a semver
    let parts: Vec<&str> = version_part.split('-').collect();
    let main = parts[0];
    let main_parts: Vec<&str> = main.split('.').collect();
    if main_parts.is_empty() {
        return false;
    }
    main_parts.iter().all(|p| {
        !p.is_empty() && p.chars().all(|c| c.is_ascii_digit() || c == '*')
    })
}

// ============================================================================
// CODE ACTIONS
// ============================================================================

/// Generate code actions (quick fixes) for ADL diagnostics.
pub fn get_adl_code_actions(
    content: &str,
    uri: &Url,
    diagnostics: &[Diagnostic],
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        // ADL007: Missing required section
        if let Some(lsp_types::NumberOrString::String(code)) = &diag.code {
            match code.as_str() {
                "ADL007" => {
                    // Extract section name from message
                    if let Some(section_name) = extract_section_from_message(&diag.message) {
                        let snippet = generate_section_snippet(&section_name);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Add [{}] section", section_name),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some({
                                    let mut m = std::collections::HashMap::new();
                                    m.insert(uri.clone(), vec![TextEdit {
                                        range: Range {
                                            start: Position { line: 0, character: 0 },
                                            end: Position { line: 0, character: 0 },
                                        },
                                        new_text: format!("{}\n", snippet),
                                    }]);
                                    m
                                }),
                                document_changes: None,
                                change_annotations: None,
                            }),
                            command: None,
                            is_preferred: Some(true),
                            disabled: None,
                            data: None,
                        }));
                    }
                }
                "ADL008" => {
                    // Extract field name and section from message
                    if let Some((field_key, section_name)) = extract_field_from_message(&diag.message) {
                        if let Some(section) = SECTIONS.iter().find(|s| s.name == section_name.as_str()) {
                            if let Some(field) = section.fields.iter().find(|f| f.key == field_key.as_str()) {
                                let snippet = field.snippet.unwrap_or(field.key);
                                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                    title: format!("Add `{}` field to [{}]", field_key, section_name),
                                    kind: Some(CodeActionKind::QUICKFIX),
                                    diagnostics: Some(vec![diag.clone()]),
                                    edit: Some(WorkspaceEdit {
                                        changes: Some({
                                            let mut m = std::collections::HashMap::new();
                                            m.insert(uri.clone(), vec![TextEdit {
                                                range: Range {
                                                    start: Position { line: 0, character: 0 },
                                                    end: Position { line: 0, character: 0 },
                                                },
                                                new_text: format!("{}\n", snippet),
                                            }]);
                                            m
                                        }),
                                        document_changes: None,
                                        change_annotations: None,
                                    }),
                                    command: None,
                                    is_preferred: Some(true),
                                    disabled: None,
                                    data: None,
                                }));
                            }
                        }
                    }
                }
                "ADL004" => {
                    // Invalid enum value — suggest valid values
                    // Message format: "Invalid value `{value}` for `{key}`. Allowed: ..."
                    if let Some(key) = extract_key_from_invalid_value_message(&diag.message) {
                        // Search all sections for the field
                        for section in SECTIONS {
                            if let Some(field) = section.fields.iter().find(|f| f.key == key.as_str()) {
                                for (val, _desc) in field.enum_values {
                                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                        title: format!("Use \"{}\" for `{}`", val, key),
                                        kind: Some(CodeActionKind::QUICKFIX),
                                        diagnostics: Some(vec![diag.clone()]),
                                        edit: Some(WorkspaceEdit {
                                            changes: Some({
                                                let mut m = std::collections::HashMap::new();
                                                m.insert(uri.clone(), vec![TextEdit {
                                                    range: diag.range,
                                                    new_text: format!("\"{}\"", val),
                                                }]);
                                                m
                                            }),
                                            document_changes: None,
                                            change_annotations: None,
                                        }),
                                        command: None,
                                        is_preferred: Some(false),
                                        disabled: None,
                                        data: None,
                                    }));
                                }
                            }
                        }
                        // Also check lockfile dependency fields
                        if let Some(field) = LOCK_DEP_FIELDS.iter().find(|f| f.key == key.as_str()) {
                            for (val, _desc) in field.enum_values {
                                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                    title: format!("Use \"{}\" for `{}`", val, key),
                                    kind: Some(CodeActionKind::QUICKFIX),
                                    diagnostics: Some(vec![diag.clone()]),
                                    edit: Some(WorkspaceEdit {
                                        changes: Some({
                                            let mut m = std::collections::HashMap::new();
                                            m.insert(uri.clone(), vec![TextEdit {
                                                range: diag.range,
                                                new_text: format!("\"{}\"", val),
                                            }]);
                                            m
                                        }),
                                        document_changes: None,
                                        change_annotations: None,
                                    }),
                                    command: None,
                                    is_preferred: Some(false),
                                    disabled: None,
                                    data: None,
                                }));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Always offer to create a full manifest template if the file is empty or has no [project]
    if !content.lines().any(|l| l.trim().starts_with("[project]")) {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Generate full manifest template".to_string(),
            kind: Some(CodeActionKind::REFACTOR),
            diagnostics: None,
            edit: Some(WorkspaceEdit {
                changes: Some({
                    let mut m = std::collections::HashMap::new();
                    m.insert(uri.clone(), vec![TextEdit {
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end: Position { line: 0, character: 0 },
                        },
                        new_text: DEFAULT_MANIFEST_TEMPLATE.to_string(),
                    }]);
                    m
                }),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: None,
        }));
    }

    actions
}

const DEFAULT_MANIFEST_TEMPLATE: &str = r#"[project]
name = "my_project"
version = "0.1.0"
template = "app"

[compiler]
backend = "interpreter"
opt-level = "debug"

[dependencies]

[scripts]
const build = "default"
const test = "default"
"#;

fn extract_section_from_message(msg: &str) -> Option<String> {
    // "Missing required section `[project]`. Add `[project]` to the manifest."
    let start = msg.find("[")?;
    let end = msg[start..].find("]")? + start;
    let section = &msg[start + 1..end];
    Some(section.to_string())
}

fn extract_field_from_message(msg: &str) -> Option<(String, String)> {
    // "Missing required field `name` in section `[project]`"
    let backtick_start = msg.find('`')?;
    let backtick_end = msg[backtick_start + 1..].find('`')? + backtick_start + 1;
    let field = &msg[backtick_start + 1..backtick_end];

    let section_start = msg[backtick_end..].find('[')? + backtick_end;
    let section_end = msg[section_start..].find(']')? + section_start;
    let section = &msg[section_start + 1..section_end];

    Some((field.to_string(), section.to_string()))
}

/// Extract the field key from an "Invalid value `{value}` for `{key}`" message.
fn extract_key_from_invalid_value_message(msg: &str) -> Option<String> {
    // Find the second backtick-enclosed word (the key, not the value)
    let first_start = msg.find('`')?;
    let first_end = msg[first_start + 1..].find('`')? + first_start + 1;
    let second_start = msg[first_end + 1..].find('`')? + first_end + 1;
    let second_end = msg[second_start + 1..].find('`')? + second_start + 1;
    Some(msg[second_start + 1..second_end].to_string())
}

fn generate_section_snippet(section_name: &str) -> String {
    match section_name {
        "project" => "[project]\nname = \"my_project\"\nversion = \"0.1.0\"\ntemplate = \"app\"".to_string(),
        "compiler" => "[compiler]\nbackend = \"interpreter\"\nopt-level = \"debug\"".to_string(),
        "dependencies" => "[dependencies]".to_string(),
        "scripts" => "[scripts]\nconst build = \"default\"".to_string(),
        "workspace" => "[workspace]\nmembers = [\"packages/*\"]".to_string(),
        _ => format!("[{}]", section_name),
    }
}

// ============================================================================
// DOCUMENT FORMATTING
// ============================================================================

/// Format an ADL document.
pub fn format_adl_document(content: &str) -> Vec<TextEdit> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let formatted = format_adl_text(content);
    if formatted == content {
        return Vec::new();
    }

    vec![TextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: lines.len() as u32,
                character: 0,
            },
        },
        new_text: formatted,
    }]
}

fn format_adl_text(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut output = String::new();
    let mut prev_was_section = false;
    let mut in_lock = false;
    let mut brace_depth = 0i32;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines but preserve section spacing
        if trimmed.is_empty() {
            continue;
        }

        // Add blank line before sections (except the first line)
        if trimmed.starts_with('[') && i > 0 && !prev_was_section && !output.is_empty() {
            output.push('\n');
        }

        // Add blank line before lock block
        if trimmed.starts_with("lock") && i > 0 && !output.is_empty() {
            output.push('\n');
        }

        // Normalize indentation: 2 spaces per level
        let indent_level = compute_indent_level(trimmed, &mut in_lock, &mut brace_depth);
        let indent = "  ".repeat(indent_level);
        output.push_str(&indent);
        output.push_str(trimmed);
        output.push('\n');

        prev_was_section = trimmed.starts_with('[') && trimmed.ends_with(']');
    }

    // Ensure file ends with newline
    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

fn compute_indent_level(trimmed: &str, in_lock: &mut bool, brace_depth: &mut i32) -> usize {
    // Track lock block
    if trimmed.starts_with("lock") && trimmed.contains('{') {
        *in_lock = true;
        *brace_depth = 1;
        return 0;
    }

    if *in_lock {
        for ch in trimmed.chars() {
            match ch {
                '{' => *brace_depth += 1,
                '}' => *brace_depth -= 1,
                _ => {}
            }
        }
        if *brace_depth <= 0 {
            *in_lock = false;
            return 0;
        }

        // Inside lock block: 1 indent
        // Inside dependency object: 2 indent
        // Inside source object: 3 indent
        if trimmed == "}" || trimmed.starts_with('}') {
            // Closing brace of a dep object
            return 1;
        }
        if trimmed.starts_with('{') {
            return 1;
        }
        if trimmed.contains("source") && trimmed.contains('{') {
            return 2;
        }
        if trimmed.starts_with("kind") || trimmed.starts_with("location") {
            return 2;
        }
        return 1;
    }

    // Section headers at level 0
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return 0;
    }

    // Key-value pairs inside sections at level 1
    1
}

// ============================================================================
// SEMANTIC TOKENS
// ============================================================================

/// Custom semantic token types for ADL.
pub const ADL_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::PROPERTY,      // 0: property keys
    SemanticTokenType::KEYWORD,       // 1: keywords (lock, import, if, let, const)
    SemanticTokenType::STRING,       // 2: string values
    SemanticTokenType::NUMBER,        // 3: numeric values
    SemanticTokenType::ENUM,          // 4: enum values
    SemanticTokenType::MACRO,         // 5: section headers
    SemanticTokenType::COMMENT,       // 6: comments
    SemanticTokenType::OPERATOR,      // 7: operators (=)
    SemanticTokenType::NAMESPACE,     // 8: section names
];

pub const ADL_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,   // 0: declaration
    SemanticTokenModifier::READONLY,      // 1: readonly (const)
    SemanticTokenModifier::MODIFICATION,  // 2: modification (let)
];

/// Compute semantic tokens for an ADL file.
pub fn get_adl_semantic_tokens(content: &str) -> SemanticTokens {
    let lines: Vec<&str> = content.lines().collect();
    let mut tokens: Vec<SemanticToken> = Vec::new();

    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let leading_ws = line.len() - trimmed.len();

        // Comments
        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                trimmed.len() as u32,
                6, // COMMENT
                0,
            );
            continue;
        }

        // Section headers [section]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // The brackets
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                1,
                5, // MACRO (section header)
                0,
            );
            // The section name
            let name_start = leading_ws + 1;
            let name_len = trimmed.len() - 2;
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                name_start as u32,
                name_len as u32,
                8, // NAMESPACE (section name)
                0,
            );
            // Closing bracket
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                (leading_ws + trimmed.len() - 1) as u32,
                1,
                5, // MACRO
                0,
            );
            continue;
        }

        // Lock keyword
        if trimmed.starts_with("lock") && trimmed.contains('{') {
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                4,
                1, // KEYWORD
                0,
            );
            continue;
        }

        // Import keyword
        if trimmed.starts_with("import ") {
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                6,
                1, // KEYWORD
                0,
            );
            // String after import
            if let Some(str_start) = trimmed.find('"') {
                if let Some(str_end) = trimmed[str_start + 1..].find('"') {
                    push_token(
                        &mut tokens,
                        &mut prev_line,
                        &mut prev_char,
                        line_idx as u32,
                        (leading_ws + str_start) as u32,
                        (str_end + 2) as u32,
                        2, // STRING
                        0,
                    );
                }
            }
            continue;
        }

        // If/else keywords
        if trimmed.starts_with("if ") || trimmed.starts_with("else") {
            let kw_len = if trimmed.starts_with("if ") { 2 } else { 4 };
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                kw_len as u32,
                1, // KEYWORD
                0,
            );
            continue;
        }

        // Let/const keywords
        if trimmed.starts_with("let ") || trimmed.starts_with("const ") {
            let (kw_len, modifier) = if trimmed.starts_with("const ") {
                (5, 1) // readonly modifier
            } else {
                (3, 2) // modification modifier
            };
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                leading_ws as u32,
                kw_len as u32,
                1, // KEYWORD
                modifier,
            );
            // Variable name
            let name_start = leading_ws + kw_len + 1;
            if let Some(eq_pos) = trimmed.find('=') {
                let name_len = eq_pos - kw_len - 1;
                push_token(
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_char,
                    line_idx as u32,
                    name_start as u32,
                    name_len as u32,
                    0, // PROPERTY
                    0,
                );
            }
            continue;
        }

        // Key-value pairs
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim();
            let key_start = leading_ws + trimmed.find(key).unwrap_or(0);
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                key_start as u32,
                key.len() as u32,
                0, // PROPERTY
                0,
            );

            // Operator
            let op_start = leading_ws + eq_pos;
            push_token(
                &mut tokens,
                &mut prev_line,
                &mut prev_char,
                line_idx as u32,
                op_start as u32,
                1,
                7, // OPERATOR
                0,
            );

            // Value
            let value = trimmed[eq_pos + 1..].trim();
            let value_start = leading_ws + trimmed.find(value).unwrap_or(eq_pos + 1);
            if value.starts_with('"') {
                // String value
                push_token(
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_char,
                    line_idx as u32,
                    value_start as u32,
                    value.len() as u32,
                    2, // STRING
                    0,
                );
            } else if value.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
                // Numeric value
                push_token(
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_char,
                    line_idx as u32,
                    value_start as u32,
                    value.len() as u32,
                    3, // NUMBER
                    0,
                );
            } else if !value.is_empty() {
                // Identifier/enum value
                push_token(
                    &mut tokens,
                    &mut prev_line,
                    &mut prev_char,
                    line_idx as u32,
                    value_start as u32,
                    value.len() as u32,
                    4, // ENUM
                    0,
                );
            }
        }
    }

    SemanticTokens {
        result_id: None,
        data: tokens,
    }
}

fn push_token(
    tokens: &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_char: &mut u32,
    line: u32,
    char: u32,
    length: u32,
    token_type: u32,
    modifier: u32,
) {
    let delta_line = line - *prev_line;
    let delta_start = if delta_line == 0 {
        char - *prev_char
    } else {
        char
    };
    tokens.push(SemanticToken {
        delta_line,
        delta_start,
        length,
        token_type,
        token_modifiers_bitset: modifier,
    });
    *prev_line = line;
    *prev_char = char;
}

// ============================================================================
// DOCUMENT LINKS
// ============================================================================

/// Generate document links for path and URL values in ADL files.
pub fn get_adl_document_links(content: &str, uri: &Url) -> Vec<DocumentLink> {
    let lines: Vec<&str> = content.lines().collect();
    let mut links = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // import "path" — link to the imported file
        if trimmed.starts_with("import ") {
            if let Some(str_start_rel) = trimmed.find('"') {
                if let Some(str_end_rel) = trimmed[str_start_rel + 1..].find('"') {
                    let path = &trimmed[str_start_rel + 1..str_start_rel + 1 + str_end_rel];
                    if let Some(target) = resolve_path(uri, path) {
                        let abs_start = line.len() - line.trim_start().len() + str_start_rel;
                        links.push(DocumentLink {
                            range: Range {
                                start: Position {
                                    line: i as u32,
                                    character: abs_start as u32,
                                },
                                end: Position {
                                    line: i as u32,
                                    character: (abs_start + str_end_rel + 2) as u32,
                                },
                            },
                            target: Some(target),
                            tooltip: Some(format!("Open {}", path)),
                            data: None,
                        });
                    }
                }
            }
        }

        // location = "path/to/dir" or location = "https://..." in source objects
        if trimmed.contains("location") && trimmed.contains('=') {
            if let Some(str_start_rel) = trimmed.find('"') {
                if let Some(str_end_rel) = trimmed[str_start_rel + 1..].find('"') {
                    let location = &trimmed[str_start_rel + 1..str_start_rel + 1 + str_end_rel];
                    let target = if location.starts_with("http://") || location.starts_with("https://") {
                        Url::parse(location).ok()
                    } else {
                        resolve_path(uri, location)
                    };
                    if let Some(target) = target {
                        let abs_start = line.len() - line.trim_start().len() + str_start_rel;
                        links.push(DocumentLink {
                            range: Range {
                                start: Position {
                                    line: i as u32,
                                    character: abs_start as u32,
                                },
                                end: Position {
                                    line: i as u32,
                                    character: (abs_start + str_end_rel + 2) as u32,
                                },
                            },
                            target: Some(target),
                            tooltip: Some(format!("Open {}", location)),
                            data: None,
                        });
                    }
                }
            }
        }
    }

    links
}

fn resolve_path(base: &Url, relative: &str) -> Option<Url> {
    // Resolve a relative path against the base URI's directory
    let base_path = base.path();
    let base_dir = base_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let resolved = if relative.starts_with('/') {
        relative.to_string()
    } else {
        format!("{}/{}", base_dir, relative)
    };
    let scheme = base.scheme();
    let authority = base.authority();
    Url::parse(&format!("{}://{}{}", scheme, authority, resolved)).ok()
}

// ============================================================================
// SELECTION RANGES
// ============================================================================

/// Compute selection ranges for smart selection expansion in ADL files.
pub fn get_adl_selection_ranges(content: &str, positions: &[Position]) -> Vec<Option<SelectionRange>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut results = Vec::new();

    for pos in positions {
        let line_idx = pos.line as usize;
        if line_idx >= lines.len() {
            results.push(None);
            continue;
        }

        let line = lines[line_idx];
        let char_idx = pos.character as usize;
        let trimmed = line.trim();

        // Build a chain of selection ranges from innermost to outermost
        let mut ranges: Vec<Range> = Vec::new();

        // 1. Word at cursor
        if let Some(word_range) = word_range_at(line, line_idx, char_idx) {
            ranges.push(word_range);
        }

        // 2. Key or value (the whole side of the = )
        if let Some(eq_pos) = trimmed.find('=') {
            let leading_ws = line.len() - line.trim_start().len();
            if char_idx <= eq_pos + leading_ws {
                // Key side
                let key = trimmed[..eq_pos].trim();
                let key_start = leading_ws + trimmed.find(key).unwrap_or(0);
                ranges.push(Range {
                    start: Position { line: pos.line, character: key_start as u32 },
                    end: Position { line: pos.line, character: (key_start + key.len()) as u32 },
                });
            } else {
                // Value side
                let value = trimmed[eq_pos + 1..].trim();
                let val_start = leading_ws + trimmed.find(value).unwrap_or(eq_pos + 1);
                ranges.push(Range {
                    start: Position { line: pos.line, character: val_start as u32 },
                    end: Position { line: pos.line, character: (val_start + value.len()) as u32 },
                });
            }
        }

        // 3. Entire line
        ranges.push(Range {
            start: Position { line: pos.line, character: 0 },
            end: Position { line: pos.line, character: line.len() as u32 },
        });

        // 4. Entire section (from section header to next section or EOF)
        let section_range = find_section_range(&lines, line_idx);
        if let Some(sr) = section_range {
            ranges.push(sr);
        }

        // 5. Entire document
        ranges.push(Range {
            start: Position { line: 0, character: 0 },
            end: Position {
                line: lines.len() as u32,
                character: 0,
            },
        });

        // Build the chain
        let mut selection_range: Option<Box<SelectionRange>> = None;
        for r in ranges.into_iter().rev() {
            selection_range = Some(Box::new(SelectionRange {
                range: r,
                parent: selection_range,
            }));
        }
        results.push(selection_range.map(|b| *b));
    }

    results
}

fn word_range_at(line: &str, line_idx: usize, char_idx: usize) -> Option<Range> {
    let bytes = line.as_bytes();
    if char_idx >= bytes.len() {
        return None;
    }
    let mut start = char_idx;
    let mut end = char_idx;
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'_'
            || bytes[start - 1] == b'-')
    {
        start -= 1;
    }
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'-')
    {
        end += 1;
    }
    if start < end {
        Some(Range {
            start: Position {
                line: line_idx as u32,
                character: start as u32,
            },
            end: Position {
                line: line_idx as u32,
                character: end as u32,
            },
        })
    } else {
        None
    }
}

fn find_section_range(lines: &[&str], current_line: usize) -> Option<Range> {
    // Find the section header above the current line
    let mut section_start = 0;
    for (i, line) in lines.iter().enumerate().take(current_line + 1).rev() {
        let trimmed = line.trim();
        if (trimmed.starts_with('[') && trimmed.ends_with(']'))
            || (trimmed.starts_with("lock") && trimmed.contains('{'))
        {
            section_start = i;
            break;
        }
    }

    // Find the end of the section (next section header or EOF)
    let mut section_end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(section_start + 1) {
        let trimmed = line.trim();
        if (trimmed.starts_with('[') && trimmed.ends_with(']'))
            || (trimmed.starts_with("lock") && trimmed.contains('{'))
        {
            section_end = i;
            break;
        }
    }

    if section_end > section_start {
        Some(Range {
            start: Position {
                line: section_start as u32,
                character: 0,
            },
            end: Position {
                line: section_end as u32,
                character: 0,
            },
        })
    } else {
        None
    }
}

// ============================================================================
// FOLDING RANGES
// ============================================================================

/// Compute folding ranges for ADL files.
pub fn get_adl_folding_ranges(content: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = content.lines().collect();
    let mut ranges = Vec::new();
    let mut stack: Vec<(u32, char)> = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' | '[' => stack.push((line_idx as u32, ch)),
                '}' | ']' => {
                    if let Some((start_line, open_ch)) = stack.pop() {
                        let matching = match open_ch {
                            '{' => '}',
                            '[' => ']',
                            _ => continue,
                        };
                        if matching == ch && line_idx as u32 > start_line {
                            ranges.push(FoldingRange {
                                start_line,
                                end_line: line_idx as u32,
                                start_character: None,
                                end_character: None,
                                kind: Some(FoldingRangeKind::Region),
                                collapsed_text: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    ranges
}

// ============================================================================
// DOCUMENT SYMBOLS
// ============================================================================

/// Generate document symbols for an ADL file (for outline view).
#[allow(deprecated)]
pub fn get_adl_document_symbols(content: &str, _uri: &Url) -> Vec<DocumentSymbol> {
    let lines: Vec<&str> = content.lines().collect();
    let mut symbols = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Section headers [section]
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            symbols.push(DocumentSymbol {
                name: format!("[{}]", name),
                detail: Some("section".to_string()),
                kind: SymbolKind::MODULE,
                range: Range {
                    start: Position { line: i as u32, character: 0 },
                    end: Position { line: i as u32, character: trimmed.len() as u32 },
                },
                selection_range: Range {
                    start: Position { line: i as u32, character: 0 },
                    end: Position { line: i as u32, character: trimmed.len() as u32 },
                },
                children: None,
                tags: None,
                deprecated: None,
            });
        }

        // Lock block
        if trimmed.starts_with("lock") && trimmed.contains('{') {
            symbols.push(DocumentSymbol {
                name: "lock".to_string(),
                detail: Some("lockfile block".to_string()),
                kind: SymbolKind::OBJECT,
                range: Range {
                    start: Position { line: i as u32, character: 0 },
                    end: Position { line: i as u32, character: trimmed.len() as u32 },
                },
                selection_range: Range {
                    start: Position { line: i as u32, character: 0 },
                    end: Position { line: i as u32, character: 4 },
                },
                children: None,
                tags: None,
                deprecated: None,
            });
        }

        // Key-value pairs (as fields within sections)
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            if !key.is_empty()
                && !key.starts_with("//")
                && !key.starts_with('#')
                && !trimmed.starts_with("let ")
                && !trimmed.starts_with("const ")
            {
                let col = line.find(&key).unwrap_or(0);
                symbols.push(DocumentSymbol {
                    name: key.clone(),
                    detail: Some("field".to_string()),
                    kind: SymbolKind::FIELD,
                    range: Range {
                        start: Position { line: i as u32, character: 0 },
                        end: Position { line: i as u32, character: trimmed.len() as u32 },
                    },
                    selection_range: Range {
                        start: Position { line: i as u32, character: col as u32 },
                        end: Position { line: i as u32, character: (col + key.len()) as u32 },
                    },
                    children: None,
                    tags: None,
                    deprecated: None,
                });
            }
        }

        // Script bindings (const/let)
        if (trimmed.starts_with("const ") || trimmed.starts_with("let ")) && trimmed.contains('=') {
            let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
            if parts.len() == 2 {
                let binding_parts: Vec<&str> = parts[0].trim().splitn(2, ' ').collect();
                if binding_parts.len() == 2 {
                    let name = binding_parts[1].trim().to_string();
                    let col = line.find(&name).unwrap_or(0);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("script binding".to_string()),
                        kind: SymbolKind::CONSTANT,
                        range: Range {
                            start: Position { line: i as u32, character: 0 },
                            end: Position { line: i as u32, character: trimmed.len() as u32 },
                        },
                        selection_range: Range {
                            start: Position { line: i as u32, character: col as u32 },
                            end: Position { line: i as u32, character: (col + binding_parts[1].trim().len()) as u32 },
                        },
                        children: None,
                        tags: None,
                        deprecated: None,
                    });
                }
            }
        }
    }

    symbols
}

// ============================================================================
// HELPERS
// ============================================================================

fn line_range(line: usize, text: &str) -> Range {
    Range {
        start: Position { line: line as u32, character: 0 },
        end: Position { line: line as u32, character: text.len() as u32 },
    }
}

fn value_range(line: usize, line_text: &str, eq_pos: usize) -> Range {
    let value_start = line_text[eq_pos + 1..].trim_start();
    let value_offset = eq_pos + 1 + (line_text[eq_pos + 1..].len() - value_start.len());
    Range {
        start: Position { line: line as u32, character: value_offset as u32 },
        end: Position { line: line as u32, character: line_text.trim().len() as u32 },
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_url(name: &str) -> Url {
        Url::parse(&format!("file:///{}", name)).unwrap()
    }

    // --- Context detection ---

    #[test]
    fn test_detect_context_top_level() {
        let content = "[project]\nname = \"test\"\n";
        assert_eq!(detect_context(content, 0, 0), AdlContext::TopLevel);
    }

    #[test]
    fn test_detect_context_section() {
        let content = "[project]\nname = \"test\"\n";
        assert_eq!(detect_context(content, 1, 0), AdlContext::Section("project"));
    }

    #[test]
    fn test_detect_context_compiler_section() {
        let content = "[compiler]\nbackend = \"interpreter\"\n";
        assert_eq!(detect_context(content, 1, 0), AdlContext::Section("compiler"));
    }

    #[test]
    fn test_detect_context_lock() {
        let content = "lock {\n  version = \"1\"\n}\n";
        assert_eq!(detect_context(content, 1, 0), AdlContext::LockFile);
    }

    #[test]
    fn test_detect_context_lock_dependency() {
        let content = "lock {\n  dependencies = [\n    {\n      name = \"Test\"\n    }\n  ]\n}\n";
        assert_eq!(detect_context(content, 3, 0), AdlContext::LockDependency);
    }

    // --- Completion ---

    #[test]
    fn test_completion_top_level() {
        let content = "";
        let items = get_adl_completions(content, 0, 0);
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label.contains("project")));
        assert!(items.iter().any(|i| i.label.contains("compiler")));
        assert!(items.iter().any(|i| i.label == "import"));
        assert!(items.iter().any(|i| i.label == "if"));
    }

    #[test]
    fn test_completion_project_section() {
        let content = "[project]\n";
        let items = get_adl_completions(content, 1, 0);
        assert!(items.iter().any(|i| i.label == "name"));
        assert!(items.iter().any(|i| i.label == "version"));
        assert!(items.iter().any(|i| i.label == "template"));
    }

    #[test]
    fn test_completion_compiler_section() {
        let content = "[compiler]\n";
        let items = get_adl_completions(content, 1, 0);
        assert!(items.iter().any(|i| i.label == "backend"));
        assert!(items.iter().any(|i| i.label == "opt-level"));
    }

    #[test]
    fn test_completion_lockfile() {
        let content = "lock {\n";
        let items = get_adl_completions(content, 1, 0);
        assert!(items.iter().any(|i| i.label == "version"));
        assert!(items.iter().any(|i| i.label == "dependencies"));
        assert!(items.iter().any(|i| i.label == "signature"));
    }

    #[test]
    fn test_completion_enum_values_backend() {
        // Cursor after "backend = " should suggest enum values
        let content = "[compiler]\nbackend = \n";
        let items = get_adl_completions(content, 1, 10);
        assert!(items.iter().any(|i| i.label == "interpreter"));
        assert!(items.iter().any(|i| i.label == "llvm"));
        assert!(items.iter().any(|i| i.label == "native"));
    }

    #[test]
    fn test_completion_enum_values_template() {
        let content = "[project]\ntemplate = \n";
        let items = get_adl_completions(content, 1, 11);
        assert!(items.iter().any(|i| i.label == "app"));
        assert!(items.iter().any(|i| i.label == "lib"));
        assert!(items.iter().any(|i| i.label == "workspace"));
    }

    #[test]
    fn test_completion_enum_values_optlevel() {
        let content = "[compiler]\nopt-level = \n";
        let items = get_adl_completions(content, 1, 12);
        assert!(items.iter().any(|i| i.label == "debug"));
        assert!(items.iter().any(|i| i.label == "release"));
        assert!(items.iter().any(|i| i.label == "fast"));
    }

    #[test]
    fn test_completion_source_kind() {
        let content = "lock {\n  dependencies = [\n    {\n      source = { kind = \n    }\n  ]\n}\n";
        let items = get_adl_completions(content, 3, 0);
        // Should suggest source object keys
        assert!(items.iter().any(|i| i.label == "kind" || i.label == "location"));
    }

    // --- Hover ---

    #[test]
    fn test_hover_project_name() {
        let content = "[project]\nname = \"test\"\n";
        let hover = get_adl_hover(content, 1, 1);
        assert!(hover.is_some());
    }

    #[test]
    fn test_hover_compiler_backend() {
        let content = "[compiler]\nbackend = \"interpreter\"\n";
        let hover = get_adl_hover(content, 1, 1);
        assert!(hover.is_some());
    }

    #[test]
    fn test_hover_section_name() {
        let content = "[project]\nname = \"test\"\n";
        let hover = get_adl_hover(content, 0, 2);
        assert!(hover.is_some());
    }

    // --- Diagnostics ---

    #[test]
    fn test_diagnostics_valid_manifest() {
        let content = "[project]\nname = \"test\"\nversion = \"0.1.0\"\ntemplate = \"app\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.is_empty(), "Expected no diagnostics for valid manifest");
    }

    #[test]
    fn test_diagnostics_missing_project_section() {
        let content = "[compiler]\nbackend = \"interpreter\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Missing required section `[project]`")));
    }

    #[test]
    fn test_diagnostics_missing_required_field() {
        let content = "[project]\nversion = \"0.1.0\"\ntemplate = \"app\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Missing required field `name`")));
    }

    #[test]
    fn test_diagnostics_invalid_enum_value() {
        let content = "[project]\nname = \"test\"\nversion = \"0.1.0\"\ntemplate = \"invalid\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Invalid value") && d.message.contains("template")));
    }

    #[test]
    fn test_diagnostics_invalid_semver() {
        let content = "[project]\nname = \"test\"\nversion = \"not-a-version\"\ntemplate = \"app\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Invalid semantic version")));
    }

    #[test]
    fn test_diagnostics_unknown_section() {
        let content = "[project]\nname = \"test\"\nversion = \"0.1.0\"\ntemplate = \"app\"\n[unknown]\nkey = \"value\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Unknown section")));
    }

    #[test]
    fn test_diagnostics_duplicate_section() {
        let content = "[project]\nname = \"test\"\nversion = \"0.1.0\"\ntemplate = \"app\"\n[project]\nname = \"test2\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Duplicate section")));
    }

    #[test]
    fn test_diagnostics_duplicate_key() {
        let content = "[project]\nname = \"test\"\nname = \"test2\"\nversion = \"0.1.0\"\ntemplate = \"app\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Duplicate key")));
    }

    #[test]
    fn test_diagnostics_lockfile_valid() {
        let content = "lock {\n  version = \"1\"\n  generated-at = \"2026-01-01T00:00:00Z\"\n  compiler-version = \"0.3.0\"\n  adl-version = \"0.3.0\"\n  package-id = \"test\"\n  dependencies = []\n  signature = \"abc123\"\n}\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.lock.adl"));
        assert!(diags.is_empty(), "Expected no diagnostics for valid lockfile, got: {:?}", diags);
    }

    #[test]
    fn test_diagnostics_lockfile_unknown_key() {
        let content = "lock {\n  version = \"1\"\n  unknown-key = \"value\"\n}\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.lock.adl"));
        assert!(diags.iter().any(|d| d.message.contains("Unknown key")));
    }

    // --- Code actions ---

    #[test]
    fn test_code_actions_missing_section() {
        let content = "[compiler]\nbackend = \"interpreter\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        let actions = get_adl_code_actions(content, &make_url("adesh.adl"), &diags);
        assert!(actions.iter().any(|a| {
            if let CodeActionOrCommand::CodeAction(ca) = a {
                ca.title.contains("Add [project] section")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_code_actions_invalid_enum() {
        let content = "[project]\nname = \"test\"\nversion = \"0.1.0\"\ntemplate = \"invalid\"\n";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        let actions = get_adl_code_actions(content, &make_url("adesh.adl"), &diags);
        assert!(actions.iter().any(|a| {
            if let CodeActionOrCommand::CodeAction(ca) = a {
                ca.title.contains("Use \"app\"")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_code_actions_generate_template() {
        let content = "";
        let diags = compute_adl_diagnostics(content, &make_url("adesh.adl"));
        let actions = get_adl_code_actions(content, &make_url("adesh.adl"), &diags);
        assert!(actions.iter().any(|a| {
            if let CodeActionOrCommand::CodeAction(ca) = a {
                ca.title.contains("Generate full manifest")
            } else {
                false
            }
        }));
    }

    // --- Formatting ---

    #[test]
    fn test_format_adds_blank_lines_between_sections() {
        let content = "[project]\nname = \"test\"\n[compiler]\nbackend = \"interpreter\"\n";
        let edits = format_adl_document(content);
        assert!(!edits.is_empty());
        let formatted = &edits[0].new_text;
        assert!(formatted.contains("[project]\n  name = \"test\"\n\n[compiler]"));
    }

    #[test]
    fn test_format_normalizes_indentation() {
        let content = "[project]\n    name = \"test\"\n";
        let edits = format_adl_document(content);
        assert!(!edits.is_empty());
        let formatted = &edits[0].new_text;
        assert!(formatted.contains("  name = \"test\""));
    }

    // --- Semantic tokens ---

    #[test]
    fn test_semantic_tokens_section_header() {
        let content = "[project]\nname = \"test\"\n";
        let tokens = get_adl_semantic_tokens(content);
        assert!(!tokens.data.is_empty());
    }

    #[test]
    fn test_semantic_tokens_key_value() {
        let content = "[project]\nname = \"test\"\n";
        let tokens = get_adl_semantic_tokens(content);
        // Should have tokens for: [, project, ], name, =, "test"
        assert!(tokens.data.len() >= 3);
    }

    // --- Document symbols ---

    #[test]
    fn test_document_symbols() {
        let content = "[project]\nname = \"test\"\n[compiler]\nbackend = \"interpreter\"\n";
        let uri = make_url("test.adl");
        let symbols = get_adl_document_symbols(content, &uri);
        // Should have: [project], name, [compiler], backend
        assert!(symbols.iter().any(|s| s.name == "[project]"));
        assert!(symbols.iter().any(|s| s.name == "[compiler]"));
        assert!(symbols.iter().any(|s| s.name == "name"));
        assert!(symbols.iter().any(|s| s.name == "backend"));
    }

    #[test]
    fn test_document_symbols_with_scripts() {
        let content = "[scripts]\nconst build = \"default\"\nlet test = \"default\"\n";
        let uri = make_url("test.adl");
        let symbols = get_adl_document_symbols(content, &uri);
        assert!(symbols.iter().any(|s| s.name == "build" && s.kind == SymbolKind::CONSTANT));
        assert!(symbols.iter().any(|s| s.name == "test" && s.kind == SymbolKind::CONSTANT));
    }

    // --- Folding ranges ---

    #[test]
    fn test_folding_ranges() {
        let content = "lock {\n  dependencies = [\n    {\n      name = \"test\"\n    }\n  ]\n}\n";
        let ranges = get_adl_folding_ranges(content);
        assert!(ranges.iter().any(|r| r.start_line == 0 && r.end_line == 6));
    }

    // --- Document links ---

    #[test]
    fn test_document_links_import() {
        let content = "import \"./other.adl\"\n";
        let uri = make_url("adesh.adl");
        let links = get_adl_document_links(content, &uri);
        assert_eq!(links.len(), 1);
        assert!(links[0].target.is_some());
    }

    #[test]
    fn test_document_links_location() {
        let content = "lock {\n  dependencies = [\n    {\n      source = { kind = \"git\", location = \"https://github.com/user/repo\" }\n    }\n  ]\n}\n";
        let uri = make_url("adesh.lock.adl");
        let links = get_adl_document_links(content, &uri);
        assert_eq!(links.len(), 1);
        assert!(links[0].target.is_some());
    }

    // --- Selection ranges ---

    #[test]
    fn test_selection_ranges() {
        let content = "[project]\nname = \"test\"\n[compiler]\nbackend = \"interpreter\"\n";
        let positions = vec![Position { line: 1, character: 3 }];
        let ranges = get_adl_selection_ranges(content, &positions);
        assert!(ranges[0].is_some());
        let sr = ranges[0].as_ref().unwrap();
        // Should have at least: word -> key -> line -> section -> document
        let mut depth = 0;
        let mut current = Some(sr);
        while current.is_some() {
            depth += 1;
            current = current.unwrap().parent.as_deref();
        }
        assert!(depth >= 3);
    }

    // --- Version validation ---

    #[test]
    fn test_is_valid_semver() {
        assert!(is_valid_semver("0.1.0"));
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("10.20.30"));
        assert!(!is_valid_semver("0.1"));
        assert!(!is_valid_semver("0.1.0.0"));
        assert!(!is_valid_semver("abc"));
    }

    #[test]
    fn test_is_valid_version_req() {
        assert!(is_valid_version_req("^1.0.0"));
        assert!(is_valid_version_req("~1.2.3"));
        assert!(is_valid_version_req(">=2.0.0"));
        assert!(is_valid_version_req("*"));
        assert!(is_valid_version_req("1.0.0"));
        assert!(!is_valid_version_req("abc"));
    }
}
