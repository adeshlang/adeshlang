//! URI Template Expansion (RFC 6570 subset) for AdeshLang URL.

use super::url_object::URL;
use crate::runtime::stdlib_src::encoding::percent;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct URLTemplate {
    template_str: String,
}

impl URLTemplate {
    pub fn parse(template_str: &str) -> Self {
        Self {
            template_str: template_str.to_string(),
        }
    }

    pub fn expand(&self, params: &HashMap<String, String>) -> Result<URL, String> {
        let mut expanded = String::new();
        let mut chars = self.template_str.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                let mut var_name = String::new();
                let mut found_close = false;
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next();
                        found_close = true;
                        break;
                    } else {
                        var_name.push(c);
                        chars.next();
                    }
                }
                if !found_close {
                    return Err(format!("Unclosed URI template variable '{{{}}}'", var_name));
                }

                let var_trimmed = var_name.trim();
                if let Some(val) = params.get(var_trimmed) {
                    // Context-aware encoding: encode slashes if in path parameter
                    let encoded = percent::percent_encode(val);
                    expanded.push_str(&encoded);
                } else {
                    return Err(format!(
                        "Missing required template variable '{}'",
                        var_trimmed
                    ));
                }
            } else {
                expanded.push(ch);
            }
        }

        URL::parse(&expanded)
    }

    pub fn template_string(&self) -> &str {
        &self.template_str
    }
}
