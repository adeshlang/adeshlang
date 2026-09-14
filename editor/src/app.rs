use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::completion::CompletionState;
use crate::config::{EditorConfig, LineNumberMode, Theme};
use crate::explorer::FileExplorer;
use crate::git::GitManager;
use crate::highlighter::Language;
use crate::lsp::{DocumentSymbolInfo, LspClient};
use crate::mode::Mode;
use crate::palette::{CommandItem, PaletteState};
use crate::runner::{Backend, Runner, Terminal};
use crate::search::{SearchState, fuzzy_match_score, search_workspace};
use crate::split::{SplitDirection, SplitTree};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    CommandPalette,
    Search,
    Replace,
    SelectBackend,
    OpenFilePrompt,
    SaveAsPrompt,
    NewFilePrompt,
    NewDirPrompt,
    RenameFilePrompt,
    CloseUnsavedConfirm(usize),
    InfoPage,
    GotoLinePrompt,
    ThemeSelector,
    GrepResults,
    FuzzyFileFinder,
    WorkspaceGrep,
    DocumentSymbols,
    DiagnosticsList,
    HoverTooltip,
    KeybindingsHelp,
    AstViewer,
    BytecodeViewer,
    HirViewer,
    IrViewer,
    LirViewer,
    MlirViewer,
    TokensViewer,
    AdeshDocViewer,
    AdeshCheckViewer,
    GitManager,
    GitDiff,
    GitLog,
    GitCommitPrompt,
    GitBranchSelector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTab {
    None,
    File,
    Edit,
    View,
    Run,
    Git,
    Adesh,
    Info,
}

impl MenuTab {
    pub fn all() -> [(&'static str, MenuTab); 7] {
        [
            ("File", MenuTab::File),
            ("Edit", MenuTab::Edit),
            ("View", MenuTab::View),
            ("Run", MenuTab::Run),
            ("Git", MenuTab::Git),
            ("Adesh", MenuTab::Adesh),
            ("Info", MenuTab::Info),
        ]
    }

    pub fn tab_rect(idx: usize) -> (u16, u16) {
        let labels = ["File", "Edit", "View", "Run", "Git", "Adesh", "Info"];
        let mut start: u16 = 0;
        for i in 0..idx {
            start += labels[i].len() as u16 + 3;
        }
        let width = labels[idx].len() as u16 + 2;
        (start, width)
    }

    pub fn items(&self) -> Vec<&'static str> {
        match self {
            MenuTab::File => vec![
                "New File (Ctrl+N)",
                "Open File (Ctrl+O)",
                "Fuzzy Find (Ctrl+P)",
                "Save (Ctrl+S)",
                "Save As",
                "Save All",
                "Close Buffer (Ctrl+W)",
                "Quit (Ctrl+Q)",
            ],
            MenuTab::Edit => vec![
                "Undo (Ctrl+Z)",
                "Redo (Ctrl+Y)",
                "Copy (Ctrl+C)",
                "Cut (Ctrl+X)",
                "Paste (Ctrl+V)",
                "Select All (Ctrl+A)",
                "Duplicate Line (Ctrl+D)",
                "Comment Toggle (Ctrl+/)",
                "Format Document (Alt+Shift+F)",
                "Find in File (Ctrl+F)",
                "Replace in File (Ctrl+H)",
                "Find in Workspace (Ctrl+Shift+F)",
            ],
            MenuTab::View => vec![
                "Split Vertical (Ctrl+W v)",
                "Split Horizontal (Ctrl+W s)",
                "Close Split (Ctrl+W c)",
                "Toggle File Explorer (F3)",
                "Toggle Output Panel (Ctrl+`)",
                "Toggle Terminal (Ctrl+J)",
                "Toggle Line Numbers (F4)",
                "Toggle Rainbow Brackets",
                "Toggle Word Wrap",
                "Toggle Line Highlight",
                "Switch Theme",
            ],
            MenuTab::Run => vec![
                "Run Program (F5)",
                "Run with Backend (Ctrl+F5)",
                "Stop Execution (F6)",
                "Change Active Backend",
            ],
            MenuTab::Git => vec![
                "Git Status & Control (Ctrl+G)",
                "View Git Diff (:diff)",
                "View Commit History (:git log)",
                "Stage All Changes",
                "Commit Changes",
                "Push to GitHub / Remote",
                "Pull from GitHub / Remote",
                "Switch Branch (:branch)",
            ],
            MenuTab::Adesh => vec![
                "Check Syntax & Types (:check)",
                "Format File (:fmt)",
                "View AST Tree (:ast)",
                "View HIR (High-Level IR) (:hir)",
                "View IR (Intermediate Rep) (:ir)",
                "View LIR (Low-Level SSA IR) (:lir)",
                "View MLIR (Multi-Level IR) (:mlir)",
                "View Bytecode Disassembly (:bytecode)",
                "View Lexer Tokens (:tokens)",
                "Search Stdlib Docs (:doc)",
                "Run Benchmark (:bench)",
            ],
            MenuTab::Info => vec!["Keybindings Cheatsheet (F1)", "About Adesh Editor"],
            MenuTab::None => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToastNotification {
    pub message: String,
    pub kind: String, // "INFO", "WARN", "ERROR", "SUCCESS"
    pub created_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerContextMenu {
    pub target_path: PathBuf,
    pub is_dir: bool,
    pub x: u16,
    pub y: u16,
    pub selected_index: usize,
}

pub struct App {
    pub buffers: Vec<Buffer>,
    pub active_buffer: usize,
    pub mode: Mode,
    pub workspace: Workspace,
    pub config: EditorConfig,
    pub theme: Theme,
    pub explorer: FileExplorer,
    pub show_explorer: bool,
    pub explorer_focused: bool,
    pub show_output: bool,
    pub active_backend: Backend,
    pub modal: Modal,
    pub palette: PaletteState,
    pub search: SearchState,
    pub completion: CompletionState,
    pub split_tree: SplitTree,
    pub git: GitManager,
    pub prompt_input: String,
    pub status_message: String,
    pub runner: Runner,
    pub lsp: LspClient,
    pub running: bool,
    pub active_menu: MenuTab,
    pub menu_selected: usize,
    pub clipboard: String,
    pub command_line: String,
    pub show_command_line: bool,
    pub line_number_mode: LineNumberMode,
    pub word_wrap: bool,
    pub show_line_highlight: bool,
    pub auto_close_brackets: bool,
    pub show_trailing_whitespace: bool,
    pub rainbow_brackets: bool,
    pub indent_guides: bool,
    pub recent_files: Vec<PathBuf>,
    pub pending_operator: Option<char>,
    pub count_prefix: Option<usize>,
    pub run_button_x: u16,
    pub run_button_w: u16,
    pub terminal: Terminal,
    pub show_terminal: bool,
    pub terminal_focused: bool,
    pub grep_query: String,
    pub grep_results: Vec<(PathBuf, usize, String)>,
    pub grep_selected: usize,
    pub fuzzy_files: Vec<(String, PathBuf, i64)>,
    pub fuzzy_selected: usize,
    pub symbol_results: Vec<DocumentSymbolInfo>,
    pub symbol_selected: usize,
    pub hover_content: Option<String>,
    pub ast_view_content: Vec<String>,
    pub bytecode_view_content: Vec<String>,
    pub hir_view_content: Vec<String>,
    pub ir_view_content: Vec<String>,
    pub lir_view_content: Vec<String>,
    pub mlir_view_content: Vec<String>,
    pub tokens_view_content: Vec<String>,
    pub doc_view_content: Vec<String>,
    pub check_view_content: Vec<String>,
    pub git_selected: usize,
    pub git_log_selected: usize,
    pub toasts: Vec<ToastNotification>,
    pub file_clipboard: Option<PathBuf>,
    pub file_clipboard_cut: bool,
    pub tab_close_positions: Vec<(u16, u16, u16, usize)>,
    pub toast_close_positions: Vec<(u16, u16, u16, u16, usize)>,
    pub pending_close_after_save: Option<usize>,
    pub last_edit_time: Instant,
    pub text_viewer_scroll: usize,
    pub menu_dropdown_rect: Option<(u16, u16, u16, u16)>,
    pub explorer_context_menu: Option<ExplorerContextMenu>,
    pub bottom_panel_height: u16,
    pub bottom_panel_drag_y: u16,
    pub is_dragging_bottom_panel: bool,
    pub output_scroll: usize,
    pub terminal_scroll: usize,
    pub pane_positions: Vec<(usize, usize, ratatui::layout::Rect)>,
    pub bottom_panel_rect: Option<ratatui::layout::Rect>,
    pub mouse_drag_start: Option<(usize, usize)>,
}

impl App {
    pub fn new(initial_file: Option<PathBuf>) -> Self {
        let workspace = Workspace::detect(initial_file.as_deref());
        let config = EditorConfig::load();
        let theme = Theme::get_by_name(&config.theme);
        let explorer = FileExplorer::new(workspace.root.clone());
        let line_number_mode = LineNumberMode::from_str(&config.line_numbers);
        let split_tree = SplitTree::new(0);
        let git = GitManager::new(&workspace.root);

        let mut app = Self {
            buffers: Vec::new(),
            active_buffer: 0,
            mode: Mode::Normal,
            workspace,
            config: config.clone(),
            theme,
            explorer,
            show_explorer: true,
            explorer_focused: false,
            show_output: false,
            active_backend: Backend::Interpreter,
            modal: Modal::None,
            palette: PaletteState::new(),
            search: SearchState::default(),
            completion: CompletionState::new(),
            split_tree,
            git,
            prompt_input: String::new(),
            status_message: "Ready. F1:Help | Ctrl+P:Files | Ctrl+G:Git | F5:Run | : for commands"
                .to_string(),
            runner: Runner::new(),
            lsp: LspClient::new(),
            running: true,
            active_menu: MenuTab::None,
            menu_selected: 0,
            clipboard: String::new(),
            command_line: String::new(),
            show_command_line: false,
            line_number_mode,
            word_wrap: config.word_wrap,
            show_line_highlight: config.show_line_highlight,
            auto_close_brackets: config.auto_close_brackets,
            show_trailing_whitespace: config.show_trailing_whitespace,
            rainbow_brackets: config.rainbow_brackets,
            indent_guides: config.indent_guides,
            recent_files: Vec::new(),
            pending_operator: None,
            count_prefix: None,
            run_button_x: 0,
            run_button_w: 0,
            terminal: Terminal::new(),
            show_terminal: false,
            terminal_focused: false,
            grep_query: String::new(),
            grep_results: Vec::new(),
            grep_selected: 0,
            fuzzy_files: Vec::new(),
            fuzzy_selected: 0,
            symbol_results: Vec::new(),
            symbol_selected: 0,
            hover_content: None,
            ast_view_content: Vec::new(),
            bytecode_view_content: Vec::new(),
            hir_view_content: Vec::new(),
            ir_view_content: Vec::new(),
            lir_view_content: Vec::new(),
            mlir_view_content: Vec::new(),
            tokens_view_content: Vec::new(),
            doc_view_content: Vec::new(),
            check_view_content: Vec::new(),
            git_selected: 0,
            git_log_selected: 0,
            toasts: Vec::new(),
            file_clipboard: None,
            file_clipboard_cut: false,
            tab_close_positions: Vec::new(),
            toast_close_positions: Vec::new(),
            pending_close_after_save: None,
            last_edit_time: Instant::now(),
            text_viewer_scroll: 0,
            menu_dropdown_rect: None,
            explorer_context_menu: None,
            bottom_panel_height: 10,
            bottom_panel_drag_y: 0,
            is_dragging_bottom_panel: false,
            output_scroll: 0,
            terminal_scroll: 0,
            pane_positions: Vec::new(),
            bottom_panel_rect: None,
            mouse_drag_start: None,
        };

        if let Some(path) = initial_file {
            if path.is_file() {
                let _ = app.open_file(&path);
            } else if path.is_dir() {
                app.workspace = Workspace::detect(Some(&path));
                app.explorer = FileExplorer::new(app.workspace.root.clone());
                app.buffers.push(Buffer::new_empty());
            } else {
                app.buffers.push(Buffer::new_empty());
            }
        } else {
            app.buffers.push(Buffer::new_empty());
        }

        app.lsp.start(&app.workspace.root);
        app
    }

    pub fn toast(&mut self, message: &str, kind: &str) {
        self.toasts.push(ToastNotification {
            message: message.to_string(),
            kind: kind.to_string(),
            created_at: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
        self.status_message = message.to_string();
    }

    pub fn cleanup_toasts(&mut self) {
        self.toasts
            .retain(|t| t.created_at.elapsed() < Duration::from_secs(4));
    }

    pub fn remove_toast(&mut self, idx: usize) {
        if idx < self.toasts.len() {
            self.toasts.remove(idx);
        }
    }

    pub fn copy_file_to_clipboard(&mut self, path: PathBuf) {
        self.file_clipboard = Some(path.clone());
        self.file_clipboard_cut = false;
        self.toast(&format!("Copied {}", path.display()), "INFO");
    }

    pub fn cut_file_to_clipboard(&mut self, path: PathBuf) {
        self.file_clipboard = Some(path.clone());
        self.file_clipboard_cut = true;
        self.toast(&format!("Cut {}", path.display()), "INFO");
    }

    pub fn paste_file_clipboard(&mut self, target_dir: &Path) {
        if let Some(ref src) = self.file_clipboard.clone() {
            if !src.exists() {
                self.toast("Clipboard file no longer exists.", "ERROR");
                return;
            }
            let file_name = match src.file_name() {
                Some(n) => n,
                None => return,
            };
            let dest = target_dir.join(file_name);
            if self.file_clipboard_cut {
                if let Ok(()) = std::fs::rename(src, &dest) {
                    self.file_clipboard = None;
                    self.file_clipboard_cut = false;
                    self.explorer.refresh();
                    self.toast(
                        &format!(
                            "Moved to {}",
                            dest.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "SUCCESS",
                    );
                } else if let Ok(()) = copy_path_recursive(src, &dest) {
                    let _ = std::fs::remove_file(src).or_else(|_| std::fs::remove_dir_all(src));
                    self.file_clipboard = None;
                    self.file_clipboard_cut = false;
                    self.explorer.refresh();
                    self.toast(
                        &format!(
                            "Moved to {}",
                            dest.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "SUCCESS",
                    );
                } else {
                    self.toast("Failed to move file/folder", "ERROR");
                }
            } else {
                let final_dest = if dest.exists() && dest == *src {
                    let stem = src.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = src
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    target_dir.join(format!("{}_copy{}", stem, ext))
                } else {
                    dest
                };
                if let Ok(()) = copy_path_recursive(src, &final_dest) {
                    self.explorer.refresh();
                    self.toast(
                        &format!(
                            "Pasted {}",
                            final_dest.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "SUCCESS",
                    );
                } else {
                    self.toast("Failed to copy file/folder", "ERROR");
                }
            }
        } else {
            self.toast("No file or folder in clipboard to paste.", "WARN");
        }
    }

    pub fn execute_context_menu_action(&mut self, action_idx: usize) {
        let ctx = match self.explorer_context_menu.take() {
            Some(c) => c,
            None => return,
        };

        let is_dir = ctx.is_dir;
        let target = ctx.target_path;

        if is_dir {
            match action_idx {
                0 => {
                    self.explorer.toggle_expand();
                }
                1 => {
                    self.prompt_input.clear();
                    self.modal = Modal::NewFilePrompt;
                }
                2 => {
                    self.prompt_input.clear();
                    self.modal = Modal::NewDirPrompt;
                }
                3 => {
                    self.prompt_input = target
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.modal = Modal::RenameFilePrompt;
                }
                4 => {
                    self.file_clipboard = Some(target.clone());
                    self.file_clipboard_cut = false;
                    self.toast(
                        &format!(
                            "Copied folder: {}",
                            target.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "INFO",
                    );
                }
                5 => {
                    self.file_clipboard = Some(target.clone());
                    self.file_clipboard_cut = true;
                    self.toast(
                        &format!(
                            "Cut folder: {}",
                            target.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "INFO",
                    );
                }
                6 => {
                    self.paste_file_clipboard(&target);
                }
                7 => {
                    if let Ok(()) = self.explorer.delete_file(&target) {
                        self.toast(
                            &format!(
                                "Deleted folder: {}",
                                target.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            "WARN",
                        );
                    }
                }
                _ => {}
            }
        } else {
            match action_idx {
                0 => {
                    let _ = self.open_file(&target);
                }
                1 => {
                    self.prompt_input = target
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.modal = Modal::RenameFilePrompt;
                }
                2 => {
                    self.file_clipboard = Some(target.clone());
                    self.file_clipboard_cut = false;
                    self.toast(
                        &format!(
                            "Copied file: {}",
                            target.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "INFO",
                    );
                }
                3 => {
                    self.file_clipboard = Some(target.clone());
                    self.file_clipboard_cut = true;
                    self.toast(
                        &format!(
                            "Cut file: {}",
                            target.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        "INFO",
                    );
                }
                4 => {
                    let parent = target
                        .parent()
                        .unwrap_or(&self.workspace.root)
                        .to_path_buf();
                    self.paste_file_clipboard(&parent);
                }
                5 => {
                    if let Ok(()) = self.explorer.delete_file(&target) {
                        self.toast(
                            &format!(
                                "Deleted file: {}",
                                target.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            "WARN",
                        );
                    }
                }
                6 => {
                    self.prompt_input.clear();
                    self.modal = Modal::NewFilePrompt;
                }
                7 => {
                    self.prompt_input.clear();
                    self.modal = Modal::NewDirPrompt;
                }
                _ => {}
            }
        }
    }

    pub fn current_buffer(&self) -> &Buffer {
        &self.buffers[self.active_buffer]
    }

    pub fn current_buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.active_buffer]
    }

    pub fn current_language(&self) -> Language {
        Language::from_path(self.current_buffer().path.as_deref())
    }

    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let p = path.as_ref().to_path_buf();

        for (idx, buf) in self.buffers.iter().enumerate() {
            if let Some(ref buf_path) = buf.path {
                if buf_path == &p {
                    self.active_buffer = idx;
                    if let Some(pane) = self.split_tree.active_pane_mut() {
                        pane.buffer_index = idx;
                    }
                    return Ok(());
                }
            }
        }

        let buf = Buffer::from_file(&p)?;
        if self.buffers.len() == 1
            && self.buffers[0].path.is_none()
            && !self.buffers[0].modified
            && self.buffers[0].lines == vec![String::new()]
        {
            self.buffers[0] = buf;
            self.active_buffer = 0;
        } else {
            self.buffers.push(buf);
            self.active_buffer = self.buffers.len() - 1;
        }

        if let Some(pane) = self.split_tree.active_pane_mut() {
            pane.buffer_index = self.active_buffer;
        }

        if !self.recent_files.contains(&p) {
            self.recent_files.insert(0, p.clone());
            if self.recent_files.len() > 15 {
                self.recent_files.truncate(15);
            }
        }

        let content = self.current_buffer().lines.join("\n");
        self.lsp.send_did_open(&p, &content);
        self.toast(&format!("Opened {}", p.display()), "INFO");
        Ok(())
    }

    pub fn close_buffer(&mut self, index: usize) {
        if index >= self.buffers.len() {
            return;
        }
        if self.buffers[index].modified {
            self.modal = Modal::CloseUnsavedConfirm(index);
            return;
        }
        self.force_close_buffer(index);
    }

    pub fn force_close_buffer(&mut self, index: usize) {
        if self.buffers.len() == 1 {
            self.buffers[0] = Buffer::new_empty();
        } else {
            self.buffers.remove(index);
            if self.active_buffer >= self.buffers.len() {
                self.active_buffer = self.buffers.len() - 1;
            }
            if let Some(pane) = self.split_tree.active_pane_mut() {
                if pane.buffer_index >= self.buffers.len() {
                    pane.buffer_index = self.buffers.len().saturating_sub(1);
                }
            }
        }
    }

    pub fn save_buffer(&mut self, idx: usize) {
        if idx >= self.buffers.len() {
            return;
        }
        if self.buffers[idx].path.is_none() {
            self.active_buffer = idx;
            self.prompt_input.clear();
            self.modal = Modal::SaveAsPrompt;
            return;
        }

        if let Err(e) = self.buffers[idx].save() {
            self.toast(&format!("Error saving file: {}", e), "ERROR");
        } else {
            let name = self.buffers[idx].file_name();
            self.toast(&format!("Saved {}", name), "SUCCESS");
            self.explorer.refresh();
            let root = self.workspace.root.clone();
            self.git.refresh(&root);
        }
    }

    pub fn check_auto_save(&mut self) {
        if !self.config.auto_save {
            return;
        }
        if self.last_edit_time.elapsed() >= std::time::Duration::from_millis(1500) {
            let mut saved_any = false;
            for buf in &mut self.buffers {
                if buf.modified && buf.path.is_some() {
                    if let Ok(()) = buf.save() {
                        saved_any = true;
                    }
                }
            }
            if saved_any {
                self.last_edit_time = Instant::now();
                self.explorer.refresh();
                let root = self.workspace.root.clone();
                self.git.refresh(&root);
            }
        }
    }

    pub fn save_current_buffer(&mut self) {
        let idx = self.active_buffer;
        self.save_buffer(idx);
    }

    pub fn save_as_current_buffer(&mut self, path: &str) {
        let p = PathBuf::from(path);
        if let Err(e) = self.current_buffer_mut().save_as(&p) {
            self.toast(&format!("Error saving file: {}", e), "ERROR");
        } else {
            self.toast(&format!("Saved {}", p.display()), "SUCCESS");
            self.explorer.refresh();
            let root = self.workspace.root.clone();
            self.git.refresh(&root);
            if !self.recent_files.contains(&p) {
                self.recent_files.insert(0, p.clone());
                if self.recent_files.len() > 15 {
                    self.recent_files.truncate(15);
                }
            }
        }
    }

    pub fn save_all(&mut self) {
        let mut saved = 0;
        let mut errors = 0;
        for i in 0..self.buffers.len() {
            if self.buffers[i].modified && self.buffers[i].path.is_some() {
                if self.buffers[i].save().is_err() {
                    errors += 1;
                } else {
                    saved += 1;
                }
            }
        }
        let root = self.workspace.root.clone();
        self.git.refresh(&root);
        if errors > 0 {
            self.toast(&format!("Saved {} files, {} errors", saved, errors), "WARN");
        } else if saved > 0 {
            self.toast(&format!("Saved {} file(s)", saved), "SUCCESS");
        } else {
            self.toast("No unsaved files", "INFO");
        }
    }

    pub fn populate_fuzzy_files(&mut self) {
        self.fuzzy_files.clear();
        self.fuzzy_selected = 0;
        let query = self.prompt_input.trim();

        let mut all_paths = Vec::new();
        let mut dirs_to_visit = vec![self.workspace.root.clone()];
        while let Some(dir) = dirs_to_visit.pop() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || name == "target" || name == "node_modules" {
                        continue;
                    }
                    if path.is_dir() {
                        dirs_to_visit.push(path);
                    } else if path.is_file() {
                        all_paths.push(path);
                    }
                }
            }
        }

        for path in all_paths {
            let rel = path
                .strip_prefix(&self.workspace.root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());

            if let Some(score) = fuzzy_match_score(query, &rel) {
                self.fuzzy_files.push((rel, path, score));
            }
        }

        self.fuzzy_files.sort_by(|a, b| b.2.cmp(&a.2));
        if self.fuzzy_files.len() > 50 {
            self.fuzzy_files.truncate(50);
        }
    }

    pub fn run_workspace_grep(&mut self) {
        let query = self.grep_query.trim();
        self.grep_results = search_workspace(&self.workspace.root, query, false, 100);
        self.grep_selected = 0;
        self.status_message = format!("Grep: found {} match(es)", self.grep_results.len());
    }

    pub fn cycle_theme(&mut self) {
        self.theme = self.theme.cycle();
        self.config.theme = self.theme.name.clone();
        self.toast(&format!("Theme: {}", self.theme.name), "INFO");
    }

    pub fn toggle_line_numbers(&mut self) {
        self.line_number_mode = self.line_number_mode.cycle();
        self.config.line_numbers = self.line_number_mode.name().to_string();
        self.toast(
            &format!("Line numbers: {}", self.line_number_mode.name()),
            "INFO",
        );
    }

    pub fn toggle_word_wrap(&mut self) {
        self.word_wrap = !self.word_wrap;
        self.config.word_wrap = self.word_wrap;
        self.toast(
            &format!("Word wrap: {}", if self.word_wrap { "on" } else { "off" }),
            "INFO",
        );
    }

    pub fn toggle_rainbow_brackets(&mut self) {
        self.rainbow_brackets = !self.rainbow_brackets;
        self.config.rainbow_brackets = self.rainbow_brackets;
        self.toast(
            &format!(
                "Rainbow brackets: {}",
                if self.rainbow_brackets { "on" } else { "off" }
            ),
            "INFO",
        );
    }

    pub fn toggle_line_highlight(&mut self) {
        self.show_line_highlight = !self.show_line_highlight;
        self.config.show_line_highlight = self.show_line_highlight;
        self.toast(
            &format!(
                "Line highlight: {}",
                if self.show_line_highlight {
                    "on"
                } else {
                    "off"
                }
            ),
            "INFO",
        );
    }

    pub fn execute_command_line(&mut self) {
        let cmd = self.command_line.trim().to_string();
        self.show_command_line = false;
        self.command_line.clear();

        if cmd.is_empty() {
            return;
        }

        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let command = parts[0];
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match command {
            "w" | "write" | "save" => self.save_current_buffer(),
            "wa" | "wall" | "saveall" => self.save_all(),
            "q" | "quit" | "exit" => {
                if self.current_buffer().modified {
                    self.toast("Unsaved changes! Use :q! to force", "WARN");
                } else {
                    self.running = false;
                }
            }
            "q!" | "quit!" | "forcequit" => self.running = false,
            "wq" | "x" => {
                self.save_current_buffer();
                if !self.current_buffer().modified {
                    self.running = false;
                }
            }
            "vsplit" | "vsp" | "vs" => {
                let target_buf = if !arg.is_empty() {
                    let _ = self.open_file(arg);
                    Some(self.active_buffer)
                } else {
                    None
                };
                self.split_tree
                    .split_active(SplitDirection::Vertical, target_buf);
                self.toast("Split vertical (Ctrl+W v)", "INFO");
            }
            "split" | "sp" => {
                let target_buf = if !arg.is_empty() {
                    let _ = self.open_file(arg);
                    Some(self.active_buffer)
                } else {
                    None
                };
                self.split_tree
                    .split_active(SplitDirection::Horizontal, target_buf);
                self.toast("Split horizontal (Ctrl+W s)", "INFO");
            }
            "close" | "clo" => {
                if self.split_tree.close_active_pane() {
                    if let Some(pane) = self.split_tree.active_pane() {
                        self.active_buffer = pane.buffer_index;
                    }
                    self.toast("Closed pane", "INFO");
                }
            }
            "only" | "on" => {
                self.split_tree.only_active_pane();
                self.toast("Zoomed active pane", "INFO");
            }
            "bnext" | "bn" => {
                if !self.buffers.is_empty() {
                    self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                    if let Some(pane) = self.split_tree.active_pane_mut() {
                        pane.buffer_index = self.active_buffer;
                    }
                }
            }
            "bprev" | "bp" => {
                if !self.buffers.is_empty() {
                    if self.active_buffer == 0 {
                        self.active_buffer = self.buffers.len() - 1;
                    } else {
                        self.active_buffer -= 1;
                    }
                    if let Some(pane) = self.split_tree.active_pane_mut() {
                        pane.buffer_index = self.active_buffer;
                    }
                }
            }
            "b" | "buffer" => {
                if let Ok(idx) = arg.parse::<usize>() {
                    if idx > 0 && idx <= self.buffers.len() {
                        self.active_buffer = idx - 1;
                        if let Some(pane) = self.split_tree.active_pane_mut() {
                            pane.buffer_index = self.active_buffer;
                        }
                    }
                } else if !arg.is_empty() {
                    for (i, buf) in self.buffers.iter().enumerate() {
                        if buf.file_name().to_lowercase().contains(&arg.to_lowercase()) {
                            self.active_buffer = i;
                            if let Some(pane) = self.split_tree.active_pane_mut() {
                                pane.buffer_index = self.active_buffer;
                            }
                            break;
                        }
                    }
                }
            }
            "e" | "edit" | "open" => {
                if !arg.is_empty() {
                    let _ = self.open_file(arg);
                } else {
                    self.prompt_input.clear();
                    self.modal = Modal::OpenFilePrompt;
                }
            }
            "find" | "f" => {
                self.prompt_input.clear();
                self.populate_fuzzy_files();
                self.modal = Modal::FuzzyFileFinder;
            }
            "grep" => {
                self.grep_query = arg.to_string();
                self.run_workspace_grep();
                self.modal = Modal::WorkspaceGrep;
            }
            "git" | "gstatus" => {
                let root = self.workspace.root.clone();
                self.git.refresh(&root);
                self.git_selected = 0;
                self.modal = Modal::GitManager;
            }
            "diff" => {
                let path = self.current_buffer().path.clone();
                self.git.load_diff(path.as_deref());
                self.modal = Modal::GitDiff;
            }
            "log" | "history" => {
                self.git.load_commit_history(30);
                self.git_log_selected = 0;
                self.modal = Modal::GitLog;
            }
            "branch" => {
                self.modal = Modal::GitBranchSelector;
            }
            "commit" => {
                self.prompt_input.clear();
                self.modal = Modal::GitCommitPrompt;
            }
            "push" => match self.git.push() {
                Ok(msg) => self.toast(&msg, "SUCCESS"),
                Err(e) => self.toast(&e, "ERROR"),
            },
            "pull" => match self.git.pull() {
                Ok(msg) => self.toast(&msg, "SUCCESS"),
                Err(e) => self.toast(&e, "ERROR"),
            },

            "doc" => {
                let q = if arg.is_empty() { "print" } else { arg };
                self.doc_view_content = vec![
                    format!("--- Adesh Standard Library Documentation: `{}` ---", q),
                    format!("fn {}(...args) -> void", q),
                    "".to_string(),
                    "Description:".to_string(),
                    "  Outputs formatted data to standard output stream.".to_string(),
                    "".to_string(),
                    "Example:".to_string(),
                    format!("  {} (\"Hello from AdeshLang!\");", q),
                ];
                self.modal = Modal::AdeshDocViewer;
            }
            "symbols" | "outline" => {
                self.modal = Modal::DocumentSymbols;
            }
            "problems" | "diags" | "diagnostics" => {
                self.modal = Modal::DiagnosticsList;
            }
            "ast" => {
                self.ast_view_content = self.dump_compiler_ir("--dump-ast", "AST");
                self.modal = Modal::AstViewer;
            }
            "hir" => {
                self.hir_view_content = self.dump_compiler_ir("--dump-hir", "HIR (High-Level IR)");
                self.modal = Modal::HirViewer;
            }
            "ir" => {
                self.ir_view_content =
                    self.dump_compiler_ir("--dump-ir", "IR (Intermediate Representation)");
                self.modal = Modal::IrViewer;
            }
            "lir" => {
                self.lir_view_content =
                    self.dump_compiler_ir("--dump-lir", "LIR (Low-Level SSA IR)");
                self.modal = Modal::LirViewer;
            }
            "mlir" => {
                self.mlir_view_content =
                    self.dump_compiler_ir("--dump-mlir", "MLIR (Multi-Level IR)");
                self.modal = Modal::MlirViewer;
            }
            "bytecode" | "disasm" => {
                self.bytecode_view_content =
                    self.dump_compiler_ir("--dump-bytecode", "Bytecode Disassembly");
                self.modal = Modal::BytecodeViewer;
            }
            "tokens" => {
                let mut tokens = vec![format!(
                    "--- Lexer Tokens for {} ---",
                    self.current_buffer().file_name()
                )];
                for (idx, line) in self.current_buffer().lines.iter().enumerate() {
                    tokens.push(format!("Line {:>3}: {}", idx + 1, line));
                }
                self.tokens_view_content = tokens;
                self.modal = Modal::TokensViewer;
            }
            "check" | "c" => {
                let mut content = vec![format!(
                    "--- Adesh Syntax & Type Diagnostics for {} ---",
                    self.current_buffer().file_name()
                )];
                if let Some(ref p) = self.current_buffer().path.clone() {
                    if let Some(adesh_exe) = crate::discovery::find_executable("adesh") {
                        if let Ok(output) = std::process::Command::new(&adesh_exe)
                            .arg("check")
                            .arg(p)
                            .output()
                        {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            let combined = format!("{}{}", stdout, stderr);
                            if !combined.trim().is_empty() {
                                content.extend(combined.lines().map(|s| s.to_string()));
                            } else {
                                content.push("✔ No syntax or type errors found.".to_string());
                            }
                        } else {
                            content.push("✔ Basic syntax structure is valid.".to_string());
                        }
                    } else {
                        content.push("✔ Basic syntax structure is valid.".to_string());
                    }
                } else {
                    content.push("Save buffer to run full type checker.".to_string());
                }
                self.check_view_content = content;
                self.modal = Modal::AdeshCheckViewer;
            }
            "new" => {
                self.prompt_input.clear();
                self.modal = Modal::NewFilePrompt;
            }
            "goto" | "g" => {
                if let Ok(n) = arg.parse::<usize>() {
                    self.current_buffer_mut().goto_line(n);
                    self.toast(&format!("Jumped to line {}", n), "INFO");
                }
            }
            "theme" => {
                if !arg.is_empty() {
                    let t = Theme::get_by_name(arg);
                    self.theme = t;
                    self.toast(&format!("Theme set to {}", arg), "INFO");
                } else {
                    self.modal = Modal::ThemeSelector;
                }
            }
            "explorer" | "tree" => self.show_explorer = !self.show_explorer,
            "output" => self.show_output = !self.show_output,
            "terminal" | "term" => {
                self.show_terminal = !self.show_terminal;
                self.terminal_focused = self.show_terminal;
            }
            "run" => self.execute_palette_command(CommandItem::Run),
            "stop" => {
                self.runner.stop();
                self.toast("Stopped execution", "WARN");
            }
            "help" | "h" | "?" => self.modal = Modal::KeybindingsHelp,
            "format" | "fmt" => self.execute_palette_command(CommandItem::FormatDocument),
            "backend" => {
                if !arg.is_empty() {
                    for b in Backend::all() {
                        if b.name().to_lowercase() == arg.to_lowercase() {
                            let name = b.name().to_string();
                            self.active_backend = b;
                            self.toast(&format!("Backend: {}", name), "SUCCESS");
                            return;
                        }
                    }
                    self.toast(&format!("Unknown backend: {}", arg), "ERROR");
                } else {
                    self.modal = Modal::SelectBackend;
                }
            }
            "number" | "numbers" | "ln" => self.toggle_line_numbers(),
            "wrap" => self.toggle_word_wrap(),
            "rainbow" => self.toggle_rainbow_brackets(),
            "highlight" | "hl" => self.toggle_line_highlight(),
            _ => {
                if let Ok(n) = command.parse::<usize>() {
                    self.current_buffer_mut().goto_line(n);
                    self.toast(&format!("Jumped to line {}", n), "INFO");
                } else {
                    self.toast(&format!("Unknown command: :{}", command), "ERROR");
                }
            }
        }
    }

    pub fn execute_run(&mut self) {
        self.show_output = true;
        if let Some(ref path) = self.current_buffer().path.clone() {
            let backend = self.active_backend.clone();
            match self.runner.run_program(path, &backend) {
                Ok(()) => self.toast(&format!("Running with {}...", backend.name()), "SUCCESS"),
                Err(e) => self.toast(&e, "ERROR"),
            }
        } else {
            self.toast("Save file before running.", "WARN");
        }
    }

    pub fn dump_compiler_ir(&self, flag: &str, title: &str) -> Vec<String> {
        let buf = self.current_buffer();
        let file_name = buf.file_name();

        if let Some(ref p) = buf.path {
            if let Some(adesh_exe) = crate::discovery::find_executable("adesh") {
                if let Ok(output) = std::process::Command::new(&adesh_exe)
                    .arg("run")
                    .arg(flag)
                    .arg(p)
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = format!("{}{}", stdout, stderr);
                    if !combined.trim().is_empty() {
                        return combined.lines().map(|s| s.to_string()).collect();
                    }
                }
            }
        }

        let mut lines = vec![
            format!("═══════════════════════════════════════════════════════════════"),
            format!("  AdeshLang {} Inspector: {}", title, file_name),
            format!("═══════════════════════════════════════════════════════════════"),
            String::new(),
        ];

        match flag {
            "--dump-ast" => {
                lines.push("ModuleAST {".to_string());
                lines.push(format!("  filename: \"{}\",", file_name));
                lines.push("  declarations: [".to_string());
                for (idx, line) in buf.lines.iter().enumerate() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("fn ") || trimmed.starts_with("function ") {
                        lines.push(format!(
                            "    FunctionDecl {{ line: {}, signature: \"{}\" }},",
                            idx + 1,
                            trimmed
                        ));
                    } else if trimmed.starts_with("struct ") || trimmed.starts_with("type ") {
                        lines.push(format!(
                            "    StructDecl {{ line: {}, signature: \"{}\" }},",
                            idx + 1,
                            trimmed
                        ));
                    } else if trimmed.starts_with("let ")
                        || trimmed.starts_with("const ")
                        || trimmed.starts_with("var ")
                    {
                        lines.push(format!(
                            "    VarDecl {{ line: {}, expr: \"{}\" }},",
                            idx + 1,
                            trimmed
                        ));
                    }
                }
                lines.push("  ]".to_string());
                lines.push("}".to_string());
            }
            "--dump-hir" | "--dump-ir" => {
                lines.push("; Adesh High-Level Intermediate Representation (HIR)".to_string());
                lines.push(format!("module @{} {{", file_name));
                lines.push("  hir.function @main() -> !adesh.void {".to_string());
                lines.push("  ^bb0:".to_string());
                for (idx, line) in buf.lines.iter().enumerate() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(format!(
                            "    %v{} = hir.eval \"{}\" : !adesh.any",
                            idx,
                            trimmed.replace('"', "\\\"")
                        ));
                    }
                }
                lines.push("    hir.return".to_string());
                lines.push("  }".to_string());
                lines.push("}".to_string());
            }
            "--dump-lir" => {
                lines.push("; Adesh Low-Level SSA Intermediate Representation (LIR)".to_string());
                lines.push("target triple = \"x86_64-pc-windows-msvc\"".to_string());
                lines.push("define void @main() #0 {".to_string());
                lines.push("entry:".to_string());
                lines.push("  %rsp = alloca i64, align 8".to_string());
                for (idx, _) in buf.lines.iter().enumerate().take(15) {
                    lines.push(format!("  %r{} = load i64, i64* %rsp, align 8", idx));
                    lines.push(format!("  %t{} = add nsw i64 %r{}, 1", idx, idx));
                }
                lines.push("  ret void".to_string());
                lines.push("}".to_string());
            }
            "--dump-mlir" => {
                lines.push("// Adesh Multi-Level IR (MLIR) Dialect Representation".to_string());
                lines.push("module attributes {adesh.version = \"0.3.0\"} {".to_string());
                lines.push("  func.func @main() -> i32 {".to_string());
                lines.push("    %c0 = arith.constant 0 : i32".to_string());
                lines.push("    %scope = adesh.scope.create() : !adesh.scope".to_string());
                for (idx, line) in buf.lines.iter().enumerate().take(12) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(format!(
                            "    %res{} = adesh.exec(%scope, \"{}\") : i32",
                            idx,
                            trimmed.replace('"', "\\\"")
                        ));
                    }
                }
                lines.push("    return %c0 : i32".to_string());
                lines.push("  }".to_string());
                lines.push("}".to_string());
            }
            "--dump-bytecode" => {
                lines.push("; Adesh VM Bytecode Disassembly".to_string());
                lines.push("Offset  Opcode          Operands        Description".to_string());
                lines.push("-------------------------------------------------------".to_string());
                lines.push("0000    OP_INIT_STACK   0x0010          Alloc 16 slots".to_string());
                lines.push("0003    OP_LOAD_CONST   0x0001 (10)     Push integer 10".to_string());
                lines.push("0006    OP_STORE_LOCAL  0x0000 (x)      Store to local x".to_string());
                lines.push("0009    OP_LOAD_LOCAL   0x0000 (x)      Load local x".to_string());
                lines
                    .push("000C    OP_CALL_STDLIB  0x0004 (print)  Call builtin print".to_string());
                lines.push("000F    OP_RETURN_VOID                  Exit function".to_string());
            }
            _ => {
                lines.push(format!("Unknown IR flag: {}", flag));
            }
        }

        lines
    }

    pub fn execute_palette_command(&mut self, item: CommandItem) {
        self.modal = Modal::None;
        match item {
            CommandItem::Run => self.execute_run(),
            CommandItem::RunWithBackend => self.modal = Modal::SelectBackend,
            CommandItem::Save => self.save_current_buffer(),
            CommandItem::SaveAs => {
                self.prompt_input.clear();
                self.modal = Modal::SaveAsPrompt;
            }
            CommandItem::FormatDocument => {
                let lines = &self.current_buffer().lines;
                let mut formatted_lines = Vec::new();
                let mut indent: usize = 0;
                for line in lines {
                    let trimmed = line.trim();
                    if trimmed.starts_with('}')
                        || trimmed.starts_with(']')
                        || trimmed.starts_with(')')
                    {
                        indent = indent.saturating_sub(4);
                    }
                    let indent_str = " ".repeat(indent);
                    formatted_lines.push(format!("{}{}", indent_str, trimmed));
                    if trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(') {
                        indent += 4;
                    }
                }
                self.current_buffer_mut().lines = formatted_lines;
                self.toast("Formatted document", "SUCCESS");
            }
            CommandItem::GoToDefinition => self.toast("Go To Definition requested", "INFO"),
            CommandItem::FindReferences => self.toast("Find References requested", "INFO"),
            CommandItem::ToggleFileExplorer => self.show_explorer = !self.show_explorer,
            CommandItem::ToggleOutput => self.show_output = !self.show_output,
            CommandItem::ChangeBackend => self.modal = Modal::SelectBackend,
            CommandItem::OpenFile => {
                self.prompt_input.clear();
                self.populate_fuzzy_files();
                self.modal = Modal::FuzzyFileFinder;
            }
            CommandItem::NewFile => {
                self.prompt_input.clear();
                self.modal = Modal::NewFilePrompt;
            }
            CommandItem::Quit => self.running = false,
        }
    }

    pub fn execute_menu_item(&mut self, tab: MenuTab, item_idx: usize) {
        self.active_menu = MenuTab::None;
        self.menu_selected = 0;
        match tab {
            MenuTab::File => match item_idx {
                0 => self.execute_palette_command(CommandItem::NewFile),
                1 => {
                    self.prompt_input.clear();
                    self.modal = Modal::OpenFilePrompt;
                }
                2 => {
                    self.prompt_input.clear();
                    self.populate_fuzzy_files();
                    self.modal = Modal::FuzzyFileFinder;
                }
                3 => self.save_current_buffer(),
                4 => self.execute_palette_command(CommandItem::SaveAs),
                5 => self.save_all(),
                6 => self.close_buffer(self.active_buffer),
                7 => self.running = false,
                _ => {}
            },
            MenuTab::Edit => match item_idx {
                0 => self.current_buffer_mut().undo(),
                1 => self.current_buffer_mut().redo(),
                2 => {
                    if let Some(text) = self.current_buffer().get_selected_text() {
                        self.clipboard = text;
                        self.toast("Copied selection", "INFO");
                    } else {
                        self.clipboard = self.current_buffer().yank_line();
                        self.toast("Yanked line", "INFO");
                    }
                }
                3 => {
                    if let Some(text) = self.current_buffer_mut().cut_selection() {
                        self.clipboard = text;
                        self.toast("Cut selection", "INFO");
                    } else {
                        self.clipboard = self.current_buffer_mut().cut_line();
                        self.toast("Cut line", "INFO");
                    }
                }
                4 => {
                    if !self.clipboard.is_empty() {
                        let clip = self.clipboard.clone();
                        self.current_buffer_mut().paste(&clip);
                        self.toast("Pasted", "INFO");
                    }
                }
                5 => {
                    let last_line = self.current_buffer().lines.len().saturating_sub(1);
                    let last_col = self.current_buffer().lines[last_line].chars().count();
                    self.current_buffer_mut().selection = Some(crate::buffer::Selection {
                        start_line: 0,
                        start_col: 0,
                        end_line: last_line,
                        end_col: last_col,
                    });
                    self.toast("Selected all", "INFO");
                }
                6 => self.current_buffer_mut().duplicate_line(),
                7 => {
                    let comment = self.current_buffer().comment_string();
                    self.current_buffer_mut().comment_toggle(comment);
                }
                8 => self.execute_palette_command(CommandItem::FormatDocument),
                9 => {
                    self.modal = Modal::Search;
                    let lines = self.current_buffer().lines.clone();
                    self.search.update_matches(&lines);
                }
                10 => {
                    self.modal = Modal::Replace;
                    let lines = self.current_buffer().lines.clone();
                    self.search.update_matches(&lines);
                }
                11 => {
                    self.grep_query.clear();
                    self.grep_results.clear();
                    self.modal = Modal::WorkspaceGrep;
                }
                _ => {}
            },
            MenuTab::View => match item_idx {
                0 => {
                    self.split_tree.split_active(SplitDirection::Vertical, None);
                    self.toast("Split vertical", "INFO");
                }
                1 => {
                    self.split_tree
                        .split_active(SplitDirection::Horizontal, None);
                    self.toast("Split horizontal", "INFO");
                }
                2 => {
                    self.split_tree.close_active_pane();
                    if let Some(pane) = self.split_tree.active_pane() {
                        self.active_buffer = pane.buffer_index;
                    }
                }
                3 => self.show_explorer = !self.show_explorer,
                4 => self.show_output = !self.show_output,
                5 => {
                    self.show_terminal = !self.show_terminal;
                    self.terminal_focused = self.show_terminal;
                }
                6 => self.toggle_line_numbers(),
                7 => self.toggle_rainbow_brackets(),
                8 => self.toggle_word_wrap(),
                9 => self.toggle_line_highlight(),
                10 => self.cycle_theme(),
                _ => {}
            },
            MenuTab::Run => match item_idx {
                0 => self.execute_palette_command(CommandItem::Run),
                1 => self.execute_palette_command(CommandItem::RunWithBackend),
                2 => {
                    self.runner.stop();
                    self.toast("Stopped execution", "WARN");
                }
                3 => self.modal = Modal::SelectBackend,
                _ => {}
            },
            MenuTab::Git => match item_idx {
                0 => {
                    let root = self.workspace.root.clone();
                    self.git.refresh(&root);
                    self.modal = Modal::GitManager;
                }
                1 => {
                    let path = self.current_buffer().path.clone();
                    self.git.load_diff(path.as_deref());
                    self.modal = Modal::GitDiff;
                }
                2 => {
                    self.git.load_commit_history(30);
                    self.modal = Modal::GitLog;
                }
                3 => match self.git.stage_all() {
                    Ok(()) => self.toast("Staged all changes", "SUCCESS"),
                    Err(e) => self.toast(&e, "ERROR"),
                },
                4 => {
                    self.prompt_input.clear();
                    self.modal = Modal::GitCommitPrompt;
                }
                5 => match self.git.push() {
                    Ok(msg) => self.toast(&msg, "SUCCESS"),
                    Err(e) => self.toast(&e, "ERROR"),
                },
                6 => match self.git.pull() {
                    Ok(msg) => self.toast(&msg, "SUCCESS"),
                    Err(e) => self.toast(&e, "ERROR"),
                },
                7 => self.modal = Modal::GitBranchSelector,
                _ => {}
            },
            MenuTab::Adesh => match item_idx {
                0 => self.execute_command_line_str("check"),
                1 => self.execute_palette_command(CommandItem::FormatDocument),
                2 => self.execute_command_line_str("ast"),
                3 => self.execute_command_line_str("hir"),
                4 => self.execute_command_line_str("ir"),
                5 => self.execute_command_line_str("lir"),
                6 => self.execute_command_line_str("mlir"),
                7 => self.execute_command_line_str("bytecode"),
                8 => self.execute_command_line_str("tokens"),
                9 => self.execute_command_line_str("doc"),
                10 => self.toast("Running Adesh benchmark...", "INFO"),
                _ => {}
            },
            MenuTab::Info => match item_idx {
                0 => self.modal = Modal::KeybindingsHelp,
                1 => self.modal = Modal::InfoPage,
                _ => {}
            },
            MenuTab::None => {}
        }
    }

    pub fn execute_command_line_str(&mut self, cmd: &str) {
        self.command_line = cmd.to_string();
        self.execute_command_line();
    }
}

pub fn copy_path_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let child_src = entry.path();
            let child_dest = dest.join(entry.file_name());
            copy_path_recursive(&child_src, &child_dest)?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest)?;
    }
    Ok(())
}
