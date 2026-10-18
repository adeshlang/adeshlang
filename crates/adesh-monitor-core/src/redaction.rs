//! Pattern-based secret redaction engine for ARO telemetry.
//!
//! Every captured command line is passed through [`redact_text`] *before* it is
//! persisted, so raw credentials never reach the SQLite audit database at rest.
//! The engine is idempotent: values that are already masked are left untouched,
//! which allows history rescans over previously redacted rows.

use adesh_monitor_protocol::{RedactionFinding, RedactionRuleInfo};
use regex::{Captures, Regex};
use std::sync::LazyLock;

/// The canonical mask written over every secret
pub const MASK: &str = "***";

struct RuleDef {
    info: RedactionRuleInfo,
    regex: Regex,
    /// Capture group holding the secret value that must already be masked for
    /// the rule to be considered a no-op (idempotency guard)
    value_group: usize,
    /// Replacement template with `$N` group references, e.g. `"$1=***"`
    template: &'static str,
}

static RULES: LazyLock<Vec<RuleDef>> = LazyLock::new(|| {
    vec![
        RuleDef {
            info: RedactionRuleInfo {
                id: "cred_flags".into(),
                label: "Credential CLI Flags".into(),
                description: "Masks --password / --token / --secret style command line flags (quoted or unquoted)".into(),
                example: "--password=***".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(
                r#"(?i)(--?(?:password|passwd|pwd|token|api[-_]?key|apikey|secret|client[-_]?secret|access[-_]?key|private[-_]?key|credential)s?[= \t]+)("[^"\r\n]*"|'[^'\r\n]*'|[^\s'"]+)"#,
            )
            .expect("cred_flags regex"),
            value_group: 2,
            template: "$1***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "env_secrets".into(),
                label: "Secret Environment Assignments".into(),
                description: "Masks TOKEN= / SECRET= / PASSWORD= style environment variable assignments (quoted or unquoted)".into(),
                example: "GITHUB_TOKEN=***".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(
                r#"(?i)((?:[A-Za-z0-9_]*(?:secret|token|password|passwd|apikey|api_key|private_key|access_key|credential|auth_key)[A-Za-z0-9_]*)[ \t]*=[ \t]*)("[^"\r\n]*"|'[^'\r\n]*'|[^\s'"]+)"#,
            )
            .expect("env_secrets regex"),
            value_group: 2,
            template: "$1***",
        },


        RuleDef {
            info: RedactionRuleInfo {
                id: "bearer".into(),
                label: "Bearer / Authorization Headers".into(),
                description: "Masks bearer tokens and inline authorization credentials".into(),
                example: "Bearer ***".into(),
                severity: "high".into(),
            },
            regex: Regex::new(r"(?i)(\b(?:bearer|basic)[ \t]+)([A-Za-z0-9\-._~+/]+=*)")
                .expect("bearer regex"),
            value_group: 2,
            template: "$1***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "url_creds".into(),
                label: "URL Embedded Credentials".into(),
                description: "Masks user:password pairs embedded in scheme:// URLs".into(),
                example: "https://user:***@host".into(),
                severity: "high".into(),
            },
            regex: Regex::new(r"(\b[a-zA-Z][a-zA-Z0-9+.-]*://[^/\s:@]+):([^@\s]+)@")
                .expect("url_creds regex"),
            value_group: 2,
            template: "$1:***@",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "jwt".into(),
                label: "JSON Web Tokens".into(),
                description: "Masks JWT session tokens (eyJ... header pattern)".into(),
                example: "eyJhbGciOi...***".into(),
                severity: "high".into(),
            },
            regex: Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b")
                .expect("jwt regex"),
            value_group: 0,
            template: "***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "generic_api_key".into(),
                label: "Generic Service API Keys".into(),
                description: "Masks OpenAI, Anthropic, Stripe, and Google API keys".into(),
                example: "sk-***".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(r"\b(?:sk-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z-_]{35})\b")
                .expect("generic_api_key regex"),
            value_group: 0,
            template: "***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "aws_key".into(),
                label: "AWS Access Key IDs".into(),
                description: "Masks AKIA/ASIA style AWS access key identifiers".into(),
                example: "AKIA************".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b").expect("aws_key regex"),
            value_group: 0,
            template: "***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "github_pat".into(),
                label: "GitHub Tokens".into(),
                description: "Masks GitHub personal access tokens (ghp_/gho_/github_pat_)".into(),
                example: "ghp_**********************".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})\b")
                .expect("github_pat regex"),
            value_group: 0,
            template: "***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "slack_token".into(),
                label: "Slack Tokens".into(),
                description: "Masks Slack xoxb-/xoxp-/xoxa- style API tokens".into(),
                example: "xoxb-**********************".into(),
                severity: "high".into(),
            },
            regex: Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").expect("slack_token regex"),
            value_group: 0,
            template: "***",
        },
        RuleDef {
            info: RedactionRuleInfo {
                id: "private_key".into(),
                label: "PEM Private Key Blocks".into(),
                description: "Masks multiline PEM/OpenSSH private key material or headers".into(),
                example: "-----BEGIN *** PRIVATE KEY-----".into(),
                severity: "critical".into(),
            },
            regex: Regex::new(
                r"(?s)-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----.*?-----END (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----|-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----",
            )
            .expect("private_key regex"),
            value_group: 0,
            template: "***",
        },
    ]
});

/// Metadata for every built-in rule, in enforcement order
pub fn builtin_rules() -> Vec<RedactionRuleInfo> {
    RULES.iter().map(|r| r.info.clone()).collect()
}

fn is_already_masked(value: &str) -> bool {
    let v = value.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    (!v.is_empty() && v.chars().all(|c| c == '*')) || v == "<redacted>"
}

/// Result of running the redaction engine over a piece of text
pub struct RedactionOutcome {
    /// The input with every detected secret replaced by [`MASK`]
    pub text: String,
    /// One entry per secret that was masked by this pass
    pub findings: Vec<RedactionFinding>,
}

fn truncate_preview(s: &str) -> String {
    const MAX: usize = 96;
    if s.len() <= MAX {
        return s.to_string();
    }
    let mut end = MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Mask every secret in `input`. Safe to call repeatedly: already-masked
/// values are recognized and left untouched without producing findings.
pub fn redact_text(input: &str) -> RedactionOutcome {
    let mut findings = Vec::new();
    let mut text = input.to_string();

    for rule in RULES.iter() {
        let next = rule.regex.replace_all(&text, |caps: &Captures| {
            let original = caps[0].to_string();
            let value = caps
                .get(rule.value_group)
                .map(|m| m.as_str())
                .unwrap_or_default();
            if is_already_masked(value) {
                return original;
            }
            let mut out = String::new();
            caps.expand(rule.template, &mut out);
            findings.push(RedactionFinding {
                rule_id: rule.info.id.clone(),
                rule_label: rule.info.label.clone(),
                masked_preview: truncate_preview(&out),
            });
            out
        });
        text = next.into_owned();
    }

    RedactionOutcome { text, findings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_credential_flags() {
        let out = redact_text("adesh run --password=hunter2 main.adesh");
        assert_eq!(out.text, "adesh run --password=*** main.adesh");
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].rule_id, "cred_flags");
    }

    #[test]
    fn masks_secret_env_assignments() {
        let out = redact_text("GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz012345 adesh build");
        assert!(out.text.contains("GITHUB_TOKEN=***"), "{}", out.text);
        assert!(out.text.contains("adesh build"));
    }

    #[test]
    fn masks_url_credentials() {
        let out = redact_text("git push https://admin:s3cr3t@example.com/repo.git");
        assert_eq!(out.text, "git push https://admin:***@example.com/repo.git");
    }

    #[test]
    fn masks_provider_tokens() {
        let out = redact_text(
            "curl -H \"Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N",
        );
        assert!(out.text.contains("Bearer ***"), "{}", out.text);
        assert!(!out.text.contains("eyJzdWIi"), "{}", out.text);
    }

    #[test]
    fn is_idempotent_over_masked_text() {
        let first = redact_text("tool --api-key=abcd1234 --password=secret");
        assert_eq!(first.findings.len(), 2);

        let second = redact_text(&first.text);
        assert_eq!(second.text, first.text);
        assert!(second.findings.is_empty(), "{:?}", second.findings);
    }

    #[test]
    fn leaves_clean_command_lines_untouched() {
        let out = redact_text("adesh build main.adesh --release");
        assert_eq!(out.text, "adesh build main.adesh --release");
        assert!(out.findings.is_empty());
    }

    #[test]
    fn masks_quoted_credential_flags_and_env() {
        let out = redact_text("adesh compile --password=\"secret 123\" --token='foo_bar_token'");
        assert!(out.text.contains("--password=***"), "{}", out.text);
        assert!(out.text.contains("--token=***"), "{}", out.text);
        assert!(!out.text.contains("secret 123"));

        let env_out = redact_text("API_KEY=\"super_secret_api_key\" adesh run");
        assert!(env_out.text.contains("API_KEY=***"), "{}", env_out.text);
        assert!(!env_out.text.contains("super_secret_api_key"));
    }

    #[test]
    fn masks_generic_api_keys_and_basic_auth() {
        let out = redact_text(
            "error communicating with sk-1234567890abcdef1234567890 and Basic dXNlcjpwYXNzd29yZA==",
        );
        assert!(!out.text.contains("sk-1234567890abcdef1234567890"));
        assert!(out.text.contains("Basic ***"), "{}", out.text);
    }

    #[test]
    fn masks_multiline_private_key_blocks() {
        let multiline = "Header\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0...\n...more...\n-----END RSA PRIVATE KEY-----\nFooter";
        let out = redact_text(multiline);
        assert!(!out.text.contains("MIIEowIBAAKCAQEA0"), "{}", out.text);
        assert!(out.text.contains("***"), "{}", out.text);
        assert!(out.text.contains("Header"));
        assert!(out.text.contains("Footer"));
    }
}
