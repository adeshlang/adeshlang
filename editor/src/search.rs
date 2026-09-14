use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub replace_query: String,
    pub is_replace_mode: bool,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub matches: Vec<(usize, usize)>, // (line, col)
    pub current_match_index: usize,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            replace_query: String::new(),
            is_replace_mode: false,
            case_sensitive: false,
            use_regex: false,
            matches: Vec::new(),
            current_match_index: 0,
        }
    }
}

impl SearchState {
    pub fn update_matches(&mut self, lines: &[String]) {
        self.matches.clear();
        if self.query.is_empty() {
            self.current_match_index = 0;
            return;
        }

        if self.use_regex {
            let re_res = if self.case_sensitive {
                Regex::new(&self.query)
            } else {
                Regex::new(&format!("(?i){}", self.query))
            };

            if let Ok(re) = re_res {
                for (line_idx, line) in lines.iter().enumerate() {
                    for mat in re.find_iter(line) {
                        self.matches.push((line_idx, mat.start()));
                    }
                }
            }
        } else {
            let q = if self.case_sensitive {
                self.query.clone()
            } else {
                self.query.to_lowercase()
            };

            for (line_idx, line) in lines.iter().enumerate() {
                let haystack = if self.case_sensitive {
                    line.clone()
                } else {
                    line.to_lowercase()
                };

                let mut start = 0;
                while let Some(pos) = haystack[start..].find(&q) {
                    let actual_col = start + pos;
                    self.matches.push((line_idx, actual_col));
                    start = actual_col + q.len().max(1);
                }
            }
        }

        if self.current_match_index >= self.matches.len() {
            self.current_match_index = 0;
        }
    }

    pub fn next_match(&mut self) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_match_index = (self.current_match_index + 1) % self.matches.len();
        Some(self.matches[self.current_match_index])
    }

    pub fn prev_match(&mut self) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            return None;
        }
        if self.current_match_index == 0 {
            self.current_match_index = self.matches.len() - 1;
        } else {
            self.current_match_index -= 1;
        }
        Some(self.matches[self.current_match_index])
    }
}

/// Calculate fuzzy match score between a pattern and candidate string.
/// Returns Some(score) if matched, None if pattern characters are not found in order.
pub fn fuzzy_match_score(pattern: &str, candidate: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    let p_chars: Vec<char> = pattern.to_lowercase().chars().collect();
    let c_chars: Vec<char> = candidate.to_lowercase().chars().collect();

    let mut p_idx = 0;
    let mut score: i64 = 0;
    let mut prev_matched_idx: Option<usize> = None;

    for (c_idx, &c) in c_chars.iter().enumerate() {
        if p_idx < p_chars.len() && c == p_chars[p_idx] {
            // Consecutive bonus
            if let Some(prev) = prev_matched_idx {
                if prev + 1 == c_idx {
                    score += 15;
                } else {
                    score += 5;
                }
            } else {
                score += 10;
            }

            // Word start bonus (after '/', '_', '-', '.')
            if c_idx == 0 || matches!(c_chars[c_idx - 1], '/' | '\\' | '_' | '-' | '.') {
                score += 20;
            }

            prev_matched_idx = Some(c_idx);
            p_idx += 1;
        }
    }

    if p_idx == p_chars.len() {
        Some(score - (candidate.len() as i64))
    } else {
        None
    }
}

/// Fast recursive workspace ripgrep searching for text matches across files.
pub fn search_workspace(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    max_results: usize,
) -> Vec<(PathBuf, usize, String)> {
    let mut results = Vec::new();
    if query.is_empty() {
        return results;
    }

    let q = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut dirs_to_visit = vec![root.to_path_buf()];

    while let Some(dir) = dirs_to_visit.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if name.starts_with('.')
                    || name == "target"
                    || name == "node_modules"
                    || name == "build"
                {
                    continue;
                }

                if path.is_dir() {
                    dirs_to_visit.push(path);
                } else if path.is_file() {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for (idx, line) in content.lines().enumerate() {
                            let haystack = if case_sensitive {
                                line.to_string()
                            } else {
                                line.to_lowercase()
                            };

                            if haystack.contains(&q) {
                                results.push((path.clone(), idx + 1, line.to_string()));
                                if results.len() >= max_results {
                                    return results;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match() {
        let score1 = fuzzy_match_score("buf", "src/buffer.rs");
        assert!(score1.is_some());

        let score2 = fuzzy_match_score("xyz", "src/buffer.rs");
        assert!(score2.is_none());
    }
}
