#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandItem {
    Run,
    RunWithBackend,
    Save,
    SaveAs,
    FormatDocument,
    GoToDefinition,
    FindReferences,
    ToggleFileExplorer,
    ToggleOutput,
    ChangeBackend,
    OpenFile,
    NewFile,
    Quit,
}

impl CommandItem {
    pub fn name(&self) -> &'static str {
        match self {
            CommandItem::Run => "Run Program",
            CommandItem::RunWithBackend => "Run with Backend",
            CommandItem::Save => "Save File",
            CommandItem::SaveAs => "Save File As",
            CommandItem::FormatDocument => "Format Document",
            CommandItem::GoToDefinition => "Go to Definition",
            CommandItem::FindReferences => "Find References",
            CommandItem::ToggleFileExplorer => "Toggle File Explorer",
            CommandItem::ToggleOutput => "Toggle Output Panel",
            CommandItem::ChangeBackend => "Change Backend",
            CommandItem::OpenFile => "Open File",
            CommandItem::NewFile => "New File",
            CommandItem::Quit => "Quit Editor",
        }
    }

    pub fn shortcut(&self) -> &'static str {
        match self {
            CommandItem::Run => "F5",
            CommandItem::RunWithBackend => "Ctrl+F5",
            CommandItem::Save => "Ctrl+S",
            CommandItem::SaveAs => ":w <path>",
            CommandItem::FormatDocument => "Alt+Shift+F",
            CommandItem::GoToDefinition => "gd / F12",
            CommandItem::FindReferences => "gr",
            CommandItem::ToggleFileExplorer => "F3",
            CommandItem::ToggleOutput => "Ctrl+`",
            CommandItem::ChangeBackend => ":backend",
            CommandItem::OpenFile => "Ctrl+P",
            CommandItem::NewFile => "Ctrl+N",
            CommandItem::Quit => "Ctrl+Q",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            CommandItem::Run => "Run (F5)",
            CommandItem::RunWithBackend => "Run with Backend (Ctrl+F5)",
            CommandItem::Save => "Save (Ctrl+S)",
            CommandItem::SaveAs => "Save As",
            CommandItem::FormatDocument => "Format Document",
            CommandItem::GoToDefinition => "Go to Definition",
            CommandItem::FindReferences => "Find References",
            CommandItem::ToggleFileExplorer => "Toggle File Explorer",
            CommandItem::ToggleOutput => "Toggle Output (Ctrl+`)",
            CommandItem::ChangeBackend => "Change Backend",
            CommandItem::OpenFile => "Open File (Ctrl+O)",
            CommandItem::NewFile => "New File",
            CommandItem::Quit => "Quit (Ctrl+Q)",
        }
    }

    pub fn all() -> Vec<CommandItem> {
        vec![
            CommandItem::Run,
            CommandItem::RunWithBackend,
            CommandItem::Save,
            CommandItem::SaveAs,
            CommandItem::FormatDocument,
            CommandItem::GoToDefinition,
            CommandItem::FindReferences,
            CommandItem::ToggleFileExplorer,
            CommandItem::ToggleOutput,
            CommandItem::ChangeBackend,
            CommandItem::OpenFile,
            CommandItem::NewFile,
            CommandItem::Quit,
        ]
    }
}

pub struct PaletteState {
    pub query: String,
    pub selected_index: usize,
    pub filtered_items: Vec<CommandItem>,
}

impl PaletteState {
    pub fn new() -> Self {
        let all = CommandItem::all();
        Self {
            query: String::new(),
            selected_index: 0,
            filtered_items: all,
        }
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
        let q = self.query.to_lowercase();
        self.filtered_items = CommandItem::all()
            .into_iter()
            .filter(|item| {
                item.name().to_lowercase().contains(&q)
                    || item.shortcut().to_lowercase().contains(&q)
            })
            .collect();
        self.selected_index = 0;
    }

    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else if !self.filtered_items.is_empty() {
            self.selected_index = self.filtered_items.len() - 1;
        }
    }

    pub fn select_next(&mut self) {
        if !self.filtered_items.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.filtered_items.len();
        }
    }

    pub fn move_up(&mut self) {
        self.select_prev();
    }

    pub fn move_down(&mut self) {
        self.select_next();
    }

    pub fn selected_item(&self) -> Option<CommandItem> {
        self.filtered_items.get(self.selected_index).cloned()
    }
}
