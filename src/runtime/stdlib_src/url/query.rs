//! Query Parameters & Form URL Encoding for AdeshLang URL.

use crate::runtime::stdlib_src::encoding::percent;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueryParams {
    entries: Vec<(String, String)>,
}

impl QueryParams {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn parse(query_str: &str) -> Self {
        let mut params = Self::new();
        let trimmed = query_str.strip_prefix('?').unwrap_or(query_str);
        if trimmed.is_empty() {
            return params;
        }

        for pair in trimmed.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                let decoded_k = percent::percent_decode(k).unwrap_or_else(|_| k.to_string());
                let decoded_v = percent::percent_decode(v).unwrap_or_else(|_| v.to_string());
                params.append(&decoded_k, &decoded_v);
            } else {
                let decoded_k = percent::percent_decode(pair).unwrap_or_else(|_| pair.to_string());
                params.append(&decoded_k, "");
            }
        }

        params
    }

    pub fn parse_form(form_str: &str) -> Self {
        let mut params = Self::new();
        let trimmed = form_str.strip_prefix('?').unwrap_or(form_str);
        if trimmed.is_empty() {
            return params;
        }

        for pair in trimmed.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                let decoded_k = percent::form_decode(k).unwrap_or_else(|_| k.to_string());
                let decoded_v = percent::form_decode(v).unwrap_or_else(|_| v.to_string());
                params.append(&decoded_k, &decoded_v);
            } else {
                let decoded_k = percent::form_decode(pair).unwrap_or_else(|_| pair.to_string());
                params.append(&decoded_k, "");
            }
        }

        params
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn get_all(&self, key: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let mut replaced = false;
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].0 == key {
                if !replaced {
                    self.entries[i].1 = value.to_string();
                    replaced = true;
                    i += 1;
                } else {
                    self.entries.remove(i);
                }
            } else {
                i += 1;
            }
        }
        if !replaced {
            self.entries.push((key.to_string(), value.to_string()));
        }
    }

    pub fn append(&mut self, key: &str, value: &str) {
        self.entries.push((key.to_string(), value.to_string()));
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(k, _)| k != key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn sort(&mut self) {
        self.entries.sort_by(|a, b| match a.0.cmp(&b.0) {
            std::cmp::Ordering::Equal => a.1.cmp(&b.1),
            other => other,
        });
    }

    pub fn to_string(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&percent::percent_encode(k));
            out.push('=');
            out.push_str(&percent::percent_encode(v));
        }
        out
    }

    pub fn to_form_string(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&percent::form_encode(k));
            out.push('=');
            out.push_str(&percent::form_encode(v));
        }
        out
    }

    pub fn remove_keys(&mut self, keys: &[&str]) {
        self.entries.retain(|(k, _)| !keys.contains(&k.as_str()));
    }

    pub fn to_canonical_string(&self) -> String {
        let mut sorted = self.clone();
        sorted.sort();
        sorted.to_string()
    }
}
