//! Runtime configuration for the Adesh Language Server (ALS).
//!
//! Parsed from LSP `initializationOptions` and `workspace/didChangeConfiguration`
//! notifications, supporting both nested `{ "adesh": { ... } }` and flat layouts.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level ALS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlsConfig {
    pub format: FormatSettings,
    pub borrow_checker: BorrowCheckerSettings,
    #[serde(rename = "unsafe")]
    pub unsafe_cfg: UnsafeSettings,
    pub inlay_hints: InlayHintSettings,
    pub completion: CompletionSettings,
    pub diagnostics: DiagnosticsSettings,
    pub type_assistance: TypeAssistanceSettings,
    pub code_lens: CodeLensSettings,
}

impl Default for AlsConfig {
    fn default() -> Self {
        Self {
            format: FormatSettings::default(),
            borrow_checker: BorrowCheckerSettings::default(),
            unsafe_cfg: UnsafeSettings::default(),
            inlay_hints: InlayHintSettings::default(),
            completion: CompletionSettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            type_assistance: TypeAssistanceSettings::default(),
            code_lens: CodeLensSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatSettings {
    pub indent_size: usize,
    pub use_tabs: bool,
    pub enable: bool,
}

impl Default for FormatSettings {
    fn default() -> Self {
        Self {
            indent_size: 4,
            use_tabs: false,
            enable: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BorrowCheckerSettings {
    pub enabled: bool,
    pub show_ownership_hints: bool,
    pub highlight_borrows: bool,
}

impl Default for BorrowCheckerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_ownership_hints: true,
            highlight_borrows: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UnsafeSettings {
    pub warn_on_usage: bool,
}

impl Default for UnsafeSettings {
    fn default() -> Self {
        Self {
            warn_on_usage: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InlayHintSettings {
    pub enabled: bool,
    pub show_types: bool,
    pub show_parameter_names: bool,
    pub show_return_types: bool,
    pub show_ownership: bool,
    pub show_borrow_state: bool,
    pub show_chaining_hints: bool,
    pub max_length: usize,
}

impl Default for InlayHintSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_types: true,
            show_parameter_names: true,
            show_return_types: true,
            show_ownership: true,
            show_borrow_state: false,
            show_chaining_hints: true,
            max_length: 32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompletionSettings {
    pub enabled: bool,
    pub include_keywords: bool,
    pub include_builtins: bool,
    pub auto_import: bool,
    pub postfix: bool,
}

impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            include_keywords: true,
            include_builtins: true,
            auto_import: true,
            postfix: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiagnosticsSettings {
    pub enabled: bool,
    pub refresh_on_save: bool,
    pub unused_warnings: bool,
    pub unreachable_warnings: bool,
    pub arity_checks: bool,
    pub member_checks: bool,
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_on_save: true,
            unused_warnings: true,
            unreachable_warnings: true,
            arity_checks: true,
            member_checks: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TypeAssistanceSettings {
    pub enabled: bool,
    pub show_inferred_types: bool,
    pub strict_mode: bool,
}

impl Default for TypeAssistanceSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_inferred_types: true,
            strict_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeLensSettings {
    pub enabled: bool,
    pub references: bool,
    pub implementations: bool,
    pub run_tests: bool,
}

impl Default for CodeLensSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            references: true,
            implementations: true,
            run_tests: true,
        }
    }
}

impl AlsConfig {
    /// Merge settings from a JSON payload (`initializationOptions` or `didChangeConfiguration`).
    pub fn update_from_value(&mut self, value: &Value) {
        // Unwrap `{ "adesh": { ... } }` if present
        let root = value.get("adesh").unwrap_or(value);
        if !root.is_object() {
            return;
        }
        // Merge onto the current serialized config so omitted keys retain their existing values.
        let mut current = serde_json::to_value(&*self).unwrap_or(Value::Object(Default::default()));
        merge_json(&mut current, root);
        if let Ok(parsed) = serde_json::from_value::<AlsConfig>(current) {
            *self = parsed;
        }
    }
}

fn merge_json(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t_map), Value::Object(p_map)) => {
            for (k, v) in p_map {
                if v.is_null() {
                    continue;
                }
                match t_map.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => {
                        merge_json(existing, v);
                    }
                    _ => {
                        t_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (t, p) if !p.is_null() => *t = p.clone(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_partial_update_nested_and_flat() {
        let mut cfg = AlsConfig::default();
        assert!(cfg.inlay_hints.enabled);
        assert!(!cfg.type_assistance.strict_mode);

        cfg.update_from_value(&json!({
            "adesh": {
                "typeAssistance": { "strictMode": true },
                "inlayHints": { "showParameterNames": false }
            }
        }));
        assert!(cfg.type_assistance.strict_mode);
        assert!(cfg.type_assistance.enabled);
        assert!(!cfg.inlay_hints.show_parameter_names);
        assert!(cfg.inlay_hints.enabled);
    }
}
