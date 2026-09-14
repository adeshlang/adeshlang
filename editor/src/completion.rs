use lsp_types::CompletionItemKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateKind {
    Keyword,
    Function,
    Variable,
    Struct,
    Enum,
    Module,
    Snippet,
    Constant,
    Type,
    Field,
    File,
}

impl CandidateKind {
    pub fn badge(&self) -> &'static str {
        match self {
            CandidateKind::Keyword => "[kw]",
            CandidateKind::Function => "[fn]",
            CandidateKind::Variable => "[var]",
            CandidateKind::Struct => "[struct]",
            CandidateKind::Enum => "[enum]",
            CandidateKind::Module => "[mod]",
            CandidateKind::Snippet => "[snip]",
            CandidateKind::Constant => "[const]",
            CandidateKind::Type => "[type]",
            CandidateKind::Field => "[field]",
            CandidateKind::File => "[file]",
        }
    }

    pub fn from_lsp_kind(kind: Option<CompletionItemKind>) -> Self {
        match kind {
            Some(CompletionItemKind::FUNCTION) | Some(CompletionItemKind::METHOD) => {
                CandidateKind::Function
            }
            Some(CompletionItemKind::VARIABLE) => CandidateKind::Variable,
            Some(CompletionItemKind::STRUCT)
            | Some(CompletionItemKind::CLASS)
            | Some(CompletionItemKind::INTERFACE) => CandidateKind::Struct,
            Some(CompletionItemKind::ENUM) | Some(CompletionItemKind::ENUM_MEMBER) => {
                CandidateKind::Enum
            }
            Some(CompletionItemKind::MODULE) => CandidateKind::Module,
            Some(CompletionItemKind::SNIPPET) => CandidateKind::Snippet,
            Some(CompletionItemKind::CONSTANT) => CandidateKind::Constant,
            Some(CompletionItemKind::KEYWORD) => CandidateKind::Keyword,
            Some(CompletionItemKind::FIELD) | Some(CompletionItemKind::PROPERTY) => {
                CandidateKind::Field
            }
            Some(CompletionItemKind::FILE) => CandidateKind::File,
            _ => CandidateKind::Keyword,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionCandidate {
    pub label: String,
    pub insert_text: Option<String>,
    pub kind: CandidateKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompletionState {
    pub is_visible: bool,
    pub trigger_col: usize,
    pub prefix: String,
    pub candidates: Vec<CompletionCandidate>,
    pub filtered: Vec<CompletionCandidate>,
    pub selected_index: usize,
}

impl Default for CompletionState {
    fn default() -> Self {
        Self {
            is_visible: false,
            trigger_col: 0,
            prefix: String::new(),
            candidates: Vec::new(),
            filtered: Vec::new(),
            selected_index: 0,
        }
    }
}

impl CompletionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, trigger_col: usize, prefix: &str, candidates: Vec<CompletionCandidate>) {
        self.trigger_col = trigger_col;
        self.prefix = prefix.to_string();
        self.candidates = candidates;
        self.selected_index = 0;
        self.filter();
        self.is_visible = !self.filtered.is_empty();
    }

    pub fn dismiss(&mut self) {
        self.is_visible = false;
        self.prefix.clear();
        self.candidates.clear();
        self.filtered.clear();
        self.selected_index = 0;
    }

    pub fn update_prefix(&mut self, prefix: &str) {
        self.prefix = prefix.to_string();
        self.filter();
        if self.filtered.is_empty() {
            self.is_visible = false;
        }
    }

    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.filtered.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.filtered.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn selected_candidate(&self) -> Option<&CompletionCandidate> {
        self.filtered.get(self.selected_index)
    }

    fn filter(&mut self) {
        let prefix_lower = self.prefix.to_lowercase();
        if prefix_lower.is_empty() {
            self.filtered = self.candidates.clone();
        } else {
            self.filtered = self
                .candidates
                .iter()
                .filter(|c| {
                    let lbl_lower = c.label.to_lowercase();
                    lbl_lower.contains(&prefix_lower)
                })
                .cloned()
                .collect();

            // Sort: exact prefix match first, then fuzzy match
            self.filtered.sort_by(|a, b| {
                let a_starts = a.label.to_lowercase().starts_with(&prefix_lower);
                let b_starts = b.label.to_lowercase().starts_with(&prefix_lower);
                match (a_starts, b_starts) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.label.cmp(&b.label),
                }
            });
        }

        if self.selected_index >= self.filtered.len() {
            self.selected_index = 0;
        }
    }
}
