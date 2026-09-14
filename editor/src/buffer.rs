use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    pub desired_col: usize,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            line: 0,
            col: 0,
            desired_col: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.start_line == self.end_line && self.start_col == self.end_col
    }

    pub fn normalized(&self) -> (usize, usize, usize, usize) {
        if (self.start_line, self.start_col) <= (self.end_line, self.end_col) {
            (self.start_line, self.start_col, self.end_line, self.end_col)
        } else {
            (self.end_line, self.end_col, self.start_line, self.start_col)
        }
    }
}

#[derive(Debug, Clone)]
struct BufferState {
    lines: Vec<String>,
    cursor: Cursor,
    secondary_cursors: Vec<Cursor>,
}

pub struct Buffer {
    pub path: Option<PathBuf>,
    pub lines: Vec<String>,
    pub cursor: Cursor,
    pub secondary_cursors: Vec<Cursor>,
    pub selection: Option<Selection>,
    pub scroll_top: usize,
    pub scroll_left: usize,
    pub modified: bool,
    undo_stack: Vec<BufferState>,
    redo_stack: Vec<BufferState>,
}

impl Buffer {
    pub fn new_empty() -> Self {
        Self {
            path: None,
            lines: vec![String::new()],
            cursor: Cursor::default(),
            secondary_cursors: Vec::new(),
            selection: None,
            scroll_top: 0,
            scroll_left: 0,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let p = path.as_ref().to_path_buf();
        let content = fs::read_to_string(&p)?;
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };

        Ok(Self {
            path: Some(p),
            lines,
            cursor: Cursor::default(),
            secondary_cursors: Vec::new(),
            selection: None,
            scroll_top: 0,
            scroll_left: 0,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(ref path) = self.path {
            let content = self.lines.join("\n");
            fs::write(path, content)?;
            self.modified = false;
        }
        Ok(())
    }

    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let p = path.as_ref().to_path_buf();
        self.path = Some(p);
        self.save()
    }

    pub fn file_name(&self) -> String {
        if let Some(ref p) = self.path {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            "Untitled".to_string()
        }
    }

    pub fn record_snapshot(&mut self) {
        self.undo_stack.push(BufferState {
            lines: self.lines.clone(),
            cursor: self.cursor,
            secondary_cursors: self.secondary_cursors.clone(),
        });
        self.redo_stack.clear();
        self.modified = true;
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(BufferState {
                lines: self.lines.clone(),
                cursor: self.cursor,
                secondary_cursors: self.secondary_cursors.clone(),
            });
            self.lines = prev.lines;
            self.cursor = prev.cursor;
            self.secondary_cursors = prev.secondary_cursors;
            self.clamp_cursor();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(BufferState {
                lines: self.lines.clone(),
                cursor: self.cursor,
                secondary_cursors: self.secondary_cursors.clone(),
            });
            self.lines = next.lines;
            self.cursor = next.cursor;
            self.secondary_cursors = next.secondary_cursors;
            self.clamp_cursor();
        }
    }

    pub fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor.line >= self.lines.len() {
            self.cursor.line = self.lines.len() - 1;
        }
        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.col > line_len {
            self.cursor.col = line_len;
        }

        // Clamp secondary cursors
        let num_lines = self.lines.len();
        for c in &mut self.secondary_cursors {
            if c.line >= num_lines {
                c.line = num_lines - 1;
            }
            let len = self.lines[c.line].chars().count();
            if c.col > len {
                c.col = len;
            }
        }
    }

    pub fn adjust_scroll(&mut self, view_height: usize, view_width: usize) {
        self.clamp_cursor();
        if view_height == 0 || view_width == 0 {
            return;
        }

        if self.cursor.line < self.scroll_top {
            self.scroll_top = self.cursor.line;
        } else if self.cursor.line >= self.scroll_top + view_height {
            self.scroll_top = self.cursor.line - view_height + 1;
        }

        if self.cursor.col < self.scroll_left {
            self.scroll_left = self.cursor.col;
        } else if self.cursor.col >= self.scroll_left + view_width {
            self.scroll_left = self.cursor.col - view_width + 1;
        }
    }

    // --- Multi-Cursor Operations ---

    pub fn add_cursor_above(&mut self) {
        if self.cursor.line > 0 {
            let target_line = self.cursor.line - 1;
            let target_len = self.lines[target_line].chars().count();
            let new_c = Cursor {
                line: target_line,
                col: self.cursor.desired_col.min(target_len),
                desired_col: self.cursor.desired_col,
            };
            if !self.secondary_cursors.contains(&new_c) {
                self.secondary_cursors.push(new_c);
            }
        }
    }

    pub fn add_cursor_below(&mut self) {
        if self.cursor.line + 1 < self.lines.len() {
            let target_line = self.cursor.line + 1;
            let target_len = self.lines[target_line].chars().count();
            let new_c = Cursor {
                line: target_line,
                col: self.cursor.desired_col.min(target_len),
                desired_col: self.cursor.desired_col,
            };
            if !self.secondary_cursors.contains(&new_c) {
                self.secondary_cursors.push(new_c);
            }
        }
    }

    pub fn clear_secondary_cursors(&mut self) {
        self.secondary_cursors.clear();
    }

    pub fn all_cursors(&self) -> Vec<Cursor> {
        let mut list = vec![self.cursor];
        list.extend(self.secondary_cursors.iter().copied());
        list.sort_by_key(|c| (c.line, c.col));
        list.dedup();
        list
    }

    // --- Cursor Navigation ---

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.lines[self.cursor.line].chars().count();
        }
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.col = self.cursor.desired_col.min(line_len);
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            let line_len = self.lines[self.cursor.line].chars().count();
            self.cursor.col = self.cursor.desired_col.min(line_len);
        }
    }

    pub fn move_start_of_line(&mut self) {
        self.cursor.col = 0;
        self.cursor.desired_col = 0;
    }

    pub fn move_first_non_blank(&mut self) {
        let line = &self.lines[self.cursor.line];
        let idx = line.chars().take_while(|c| c.is_whitespace()).count();
        self.cursor.col = idx;
        self.cursor.desired_col = idx;
    }

    pub fn move_end_of_line(&mut self) {
        self.cursor.col = self.lines[self.cursor.line].chars().count();
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn move_top_of_file(&mut self) {
        self.cursor.line = 0;
        self.cursor.col = 0;
        self.cursor.desired_col = 0;
    }

    pub fn move_bottom_of_file(&mut self) {
        self.cursor.line = self.lines.len().saturating_sub(1);
        self.cursor.col = self.lines[self.cursor.line].chars().count();
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.cursor.line = self.cursor.line.saturating_sub(page_size);
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.col = self.cursor.desired_col.min(line_len);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.cursor.line = (self.cursor.line + page_size).min(self.lines.len().saturating_sub(1));
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.col = self.cursor.desired_col.min(line_len);
    }

    pub fn move_word_next(&mut self) {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.col;

        if col >= chars.len() {
            if self.cursor.line + 1 < self.lines.len() {
                self.cursor.line += 1;
                self.cursor.col = 0;
                self.cursor.desired_col = 0;
            }
            return;
        }

        let in_word = chars[col].is_alphanumeric() || chars[col] == '_';
        while col < chars.len() && ((chars[col].is_alphanumeric() || chars[col] == '_') == in_word) {
            col += 1;
        }
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }

        if col < chars.len() {
            self.cursor.col = col;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        } else {
            self.cursor.col = chars.len();
        }
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn move_word_end(&mut self) {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.col;

        if col + 1 < chars.len() {
            col += 1;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
            self.cursor.desired_col = 0;
            return;
        } else {
            return;
        }

        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        if col < chars.len() {
            let in_word = chars[col].is_alphanumeric() || chars[col] == '_';
            while col + 1 < chars.len() && ((chars[col + 1].is_alphanumeric() || chars[col + 1] == '_') == in_word) && !chars[col + 1].is_whitespace() {
                col += 1;
            }
        }
        self.cursor.col = col.min(chars.len());
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn move_word_prev(&mut self) {
        if self.cursor.col == 0 {
            if self.cursor.line > 0 {
                self.cursor.line -= 1;
                self.cursor.col = self.lines[self.cursor.line].chars().count();
                self.cursor.desired_col = self.cursor.col;
            }
            return;
        }

        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.col.min(chars.len());

        if col > 0 {
            col -= 1;
        }
        while col > 0 && chars[col].is_whitespace() {
            col -= 1;
        }

        let in_word = chars[col].is_alphanumeric() || chars[col] == '_';
        while col > 0 && ((chars[col - 1].is_alphanumeric() || chars[col - 1] == '_') == in_word) {
            col -= 1;
        }

        self.cursor.col = col;
        self.cursor.desired_col = col;
    }

    /// Find character in line (for Vim `f`, `F`, `t`, `T` motions).
    pub fn find_char_in_line(&mut self, ch: char, forward: bool, till: bool) -> bool {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if forward {
            let start = self.cursor.col + 1;
            for i in start..len {
                if chars[i] == ch {
                    self.cursor.col = if till { i.saturating_sub(1) } else { i };
                    self.cursor.desired_col = self.cursor.col;
                    return true;
                }
            }
        } else {
            if self.cursor.col == 0 {
                return false;
            }
            let start = self.cursor.col - 1;
            for i in (0..=start).rev() {
                if chars[i] == ch {
                    self.cursor.col = if till { (i + 1).min(len) } else { i };
                    self.cursor.desired_col = self.cursor.col;
                    return true;
                }
            }
        }
        false
    }

    // --- Editing Operations ---

    pub fn insert_char(&mut self, ch: char) {
        self.record_snapshot();
        self.delete_selection_if_any();

        let line = &mut self.lines[self.cursor.line];
        let mut chars: Vec<char> = line.chars().collect();
        let idx = self.cursor.col.min(chars.len());
        chars.insert(idx, ch);
        *line = chars.into_iter().collect();

        self.cursor.col += 1;
        self.cursor.desired_col = self.cursor.col;

        // Apply to secondary cursors if any
        if !self.secondary_cursors.is_empty() {
            let mut updated_cursors = Vec::new();
            for c in &self.secondary_cursors {
                if c.line < self.lines.len() {
                    let mut l_chars: Vec<char> = self.lines[c.line].chars().collect();
                    let c_idx = c.col.min(l_chars.len());
                    l_chars.insert(c_idx, ch);
                    self.lines[c.line] = l_chars.into_iter().collect();
                    updated_cursors.push(Cursor {
                        line: c.line,
                        col: c_idx + 1,
                        desired_col: c_idx + 1,
                    });
                }
            }
            self.secondary_cursors = updated_cursors;
        }
    }

    pub fn insert_newline(&mut self) {
        self.insert_newline_with_tab_width(4);
    }

    pub fn insert_newline_with_tab_width(&mut self, tab_width: usize) {
        self.record_snapshot();
        self.delete_selection_if_any();
        self.clear_secondary_cursors();

        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let idx = self.cursor.col.min(chars.len());

        let before_ch = if idx > 0 { Some(chars[idx - 1]) } else { None };
        let after_ch = if idx < chars.len() { Some(chars[idx]) } else { None };

        // Smart Enter between brackets: {|} or (|) or [|]
        let is_bracket_pair = match (before_ch, after_ch) {
            (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']')) => true,
            _ => false,
        };

        let current_part: String = chars[..idx].iter().collect();
        let next_part: String = chars[idx..].iter().collect();
        let base_indent: String = current_part.chars().take_while(|c| c.is_whitespace()).collect();

        if is_bracket_pair {
            let extra_indent = " ".repeat(tab_width);
            let inside_indent = format!("{}{}", base_indent, extra_indent);

            self.lines[self.cursor.line] = current_part;
            self.lines.insert(self.cursor.line + 1, inside_indent.clone());
            self.lines.insert(self.cursor.line + 2, format!("{}{}", base_indent, next_part.trim_start()));

            self.cursor.line += 1;
            self.cursor.col = inside_indent.chars().count();
            self.cursor.desired_col = self.cursor.col;
        } else {
            let indent: String = base_indent;
            self.lines[self.cursor.line] = current_part;
            self.lines.insert(self.cursor.line + 1, format!("{}{}", indent, next_part));

            self.cursor.line += 1;
            self.cursor.col = indent.chars().count();
            self.cursor.desired_col = self.cursor.col;
        }
    }

    pub fn backspace(&mut self) {
        if self.selection.is_some() {
            self.record_snapshot();
            self.delete_selection_if_any();
            return;
        }

        self.record_snapshot();

        if self.cursor.col > 0 {
            let line = &mut self.lines[self.cursor.line];
            let mut chars: Vec<char> = line.chars().collect();
            let col = self.cursor.col;

            // VSCode-style smart pair backspace deletion: (|), "", '', [], {}
            if col < chars.len() {
                let prev = chars[col - 1];
                let curr = chars[col];
                let is_pair = matches!(
                    (prev, curr),
                    ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
                );
                if is_pair {
                    chars.remove(col);     // Remove closing
                    chars.remove(col - 1); // Remove opening
                    *line = chars.into_iter().collect();
                    self.cursor.col -= 1;
                    self.cursor.desired_col = self.cursor.col;
                    return;
                }
            }

            chars.remove(col - 1);
            *line = chars.into_iter().collect();

            self.cursor.col -= 1;
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line > 0 {
            let current_line = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            let prev_len = self.lines[self.cursor.line].chars().count();
            self.lines[self.cursor.line].push_str(&current_line);

            self.cursor.col = prev_len;
            self.cursor.desired_col = prev_len;
        }

        // Secondary cursors
        let mut updated = Vec::new();
        for c in &self.secondary_cursors {
            if c.line < self.lines.len() && c.col > 0 {
                let mut chars: Vec<char> = self.lines[c.line].chars().collect();
                if c.col - 1 < chars.len() {
                    chars.remove(c.col - 1);
                    self.lines[c.line] = chars.into_iter().collect();
                    updated.push(Cursor {
                        line: c.line,
                        col: c.col - 1,
                        desired_col: c.col - 1,
                    });
                }
            }
        }
        self.secondary_cursors = updated;
    }

    pub fn backspace_word(&mut self) {
        if self.selection.is_some() {
            self.record_snapshot();
            self.delete_selection_if_any();
            return;
        }

        if self.cursor.col == 0 {
            self.backspace();
            return;
        }

        self.record_snapshot();
        let line = &mut self.lines[self.cursor.line];
        let mut chars: Vec<char> = line.chars().collect();
        let start_col = self.cursor.col.min(chars.len());
        let mut target_col = start_col;

        // Skip preceding whitespace
        while target_col > 0 && chars[target_col - 1].is_whitespace() {
            target_col -= 1;
        }

        if target_col > 0 {
            let is_word_char = chars[target_col - 1].is_alphanumeric() || chars[target_col - 1] == '_';
            while target_col > 0
                && ((chars[target_col - 1].is_alphanumeric() || chars[target_col - 1] == '_') == is_word_char)
                && !chars[target_col - 1].is_whitespace()
            {
                target_col -= 1;
            }
        }

        chars.drain(target_col..start_col);
        *line = chars.into_iter().collect();
        self.cursor.col = target_col;
        self.cursor.desired_col = target_col;

        // Apply to secondary cursors if any
        let mut updated = Vec::new();
        for c in &self.secondary_cursors {
            if c.line < self.lines.len() && c.col > 0 {
                let mut l_chars: Vec<char> = self.lines[c.line].chars().collect();
                let c_start = c.col.min(l_chars.len());
                let mut c_target = c_start;
                while c_target > 0 && l_chars[c_target - 1].is_whitespace() {
                    c_target -= 1;
                }
                if c_target > 0 {
                    let is_w = l_chars[c_target - 1].is_alphanumeric() || l_chars[c_target - 1] == '_';
                    while c_target > 0
                        && ((l_chars[c_target - 1].is_alphanumeric() || l_chars[c_target - 1] == '_') == is_w)
                        && !l_chars[c_target - 1].is_whitespace()
                    {
                        c_target -= 1;
                    }
                }
                l_chars.drain(c_target..c_start);
                self.lines[c.line] = l_chars.into_iter().collect();
                updated.push(Cursor {
                    line: c.line,
                    col: c_target,
                    desired_col: c_target,
                });
            }
        }
        self.secondary_cursors = updated;
    }

    pub fn delete_char(&mut self) {
        if self.selection.is_some() {
            self.record_snapshot();
            self.delete_selection_if_any();
            return;
        }

        let line_len = self.lines[self.cursor.line].chars().count();
        if self.cursor.col < line_len {
            self.record_snapshot();
            let line = &mut self.lines[self.cursor.line];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(self.cursor.col);
            *line = chars.into_iter().collect();
        } else if self.cursor.line + 1 < self.lines.len() {
            self.record_snapshot();
            let next_line = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next_line);
        }
    }

    pub fn indent(&mut self, tab_width: usize) {
        self.record_snapshot();
        let indent_str = " ".repeat(tab_width);

        if let Some(sel) = self.selection {
            let (start_line, _, end_line, _) = sel.normalized();
            for l in start_line..=end_line {
                self.lines[l].insert_str(0, &indent_str);
            }
            self.cursor.col += tab_width;
            self.cursor.desired_col = self.cursor.col;
        } else {
            let line = &mut self.lines[self.cursor.line];
            let mut chars: Vec<char> = line.chars().collect();
            let idx = self.cursor.col.min(chars.len());
            for (i, c) in indent_str.chars().enumerate() {
                chars.insert(idx + i, c);
            }
            *line = chars.into_iter().collect();
            self.cursor.col += tab_width;
            self.cursor.desired_col = self.cursor.col;
        }
    }

    pub fn unindent(&mut self, tab_width: usize) {
        self.record_snapshot();
        let (start_line, end_line) = if let Some(sel) = self.selection {
            let (sl, _, el, _) = sel.normalized();
            (sl, el)
        } else {
            (self.cursor.line, self.cursor.line)
        };

        for l in start_line..=end_line {
            let line = &self.lines[l];
            let spaces = line.chars().take(tab_width).take_while(|c| *c == ' ').count();
            if spaces > 0 {
                self.lines[l] = line[spaces..].to_string();
                if l == self.cursor.line {
                    self.cursor.col = self.cursor.col.saturating_sub(spaces);
                    self.cursor.desired_col = self.cursor.col;
                }
            }
        }
    }

    pub fn delete_selection_if_any(&mut self) -> bool {
        if let Some(sel) = self.selection.take() {
            let (sl, sc, el, ec) = sel.normalized();

            if sl == el {
                let line = &mut self.lines[sl];
                let chars: Vec<char> = line.chars().collect();
                let new_chars: String = chars[..sc].iter().chain(&chars[ec..]).collect();
                *line = new_chars;
            } else {
                let start_part: String = self.lines[sl].chars().take(sc).collect();
                let end_part: String = self.lines[el].chars().skip(ec).collect();

                self.lines.drain(sl + 1..=el);
                self.lines[sl] = format!("{}{}", start_part, end_part);
            }

            self.cursor.line = sl;
            self.cursor.col = sc;
            self.cursor.desired_col = sc;
            return true;
        }
        false
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let sel = self.selection?;
        let (sl, sc, el, ec) = sel.normalized();

        if sl == el {
            let line = &self.lines[sl];
            let chars: Vec<char> = line.chars().collect();
            Some(chars[sc..ec.min(chars.len())].iter().collect())
        } else {
            let mut result = Vec::new();
            let start_line_chars: Vec<char> = self.lines[sl].chars().collect();
            result.push(start_line_chars[sc..].iter().collect::<String>());

            for l in sl + 1..el {
                result.push(self.lines[l].clone());
            }

            let end_line_chars: Vec<char> = self.lines[el].chars().collect();
            result.push(end_line_chars[..ec.min(end_line_chars.len())].iter().collect::<String>());

            Some(result.join("\n"))
        }
    }

    // --- Pro Editing Operations ---

    pub fn yank_line(&self) -> String {
        self.lines[self.cursor.line].clone()
    }

    pub fn cut_line(&mut self) -> String {
        let line = self.lines[self.cursor.line].clone();
        self.record_snapshot();
        if self.lines.len() > 1 {
            self.lines.remove(self.cursor.line);
            if self.cursor.line >= self.lines.len() {
                self.cursor.line = self.lines.len() - 1;
            }
        } else {
            self.lines[0] = String::new();
        }
        let line_len = self.lines[self.cursor.line].chars().count();
        self.cursor.col = self.cursor.col.min(line_len);
        self.cursor.desired_col = self.cursor.col;
        line
    }

    pub fn cut_selection(&mut self) -> Option<String> {
        let text = self.get_selected_text();
        if text.is_some() {
            self.record_snapshot();
            self.delete_selection_if_any();
        }
        text
    }

    pub fn paste(&mut self, text: &str) {
        self.record_snapshot();
        if text.contains('\n') {
            let parts: Vec<&str> = text.split('\n').collect();
            let current_line = self.lines[self.cursor.line].clone();
            let col = self.cursor.col;
            let chars: Vec<char> = current_line.chars().collect();
            let before: String = chars[..col.min(chars.len())].iter().collect();
            let after: String = chars[col.min(chars.len())..].iter().collect();

            if parts.len() == 1 {
                self.lines[self.cursor.line] = format!("{}{}{}", before, parts[0], after);
                self.cursor.col = col + parts[0].chars().count();
            } else {
                self.lines[self.cursor.line] = format!("{}{}", before, parts[0]);
                for (i, part) in parts[1..].iter().enumerate() {
                    let line_text = if i == parts.len() - 2 {
                        format!("{}{}", part, after)
                    } else {
                        part.to_string()
                    };
                    self.lines.insert(self.cursor.line + 1 + i, line_text);
                }
                self.cursor.line += parts.len() - 1;
                self.cursor.col = parts.last().unwrap().chars().count();
            }
        } else {
            let line = &mut self.lines[self.cursor.line];
            let chars: Vec<char> = line.chars().collect();
            let idx = self.cursor.col.min(chars.len());
            let new_chars: Vec<char> = chars[..idx]
                .iter()
                .cloned()
                .chain(text.chars())
                .chain(chars[idx..].iter().cloned())
                .collect();
            *line = new_chars.into_iter().collect();
            self.cursor.col = idx + text.chars().count();
        }
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn paste_before(&mut self, text: &str) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
        self.paste(text);
    }

    pub fn duplicate_line(&mut self) {
        self.record_snapshot();
        let line = self.lines[self.cursor.line].clone();
        self.lines.insert(self.cursor.line + 1, line);
        self.cursor.line += 1;
    }

    pub fn move_line_up(&mut self) {
        if self.cursor.line == 0 {
            return;
        }
        self.record_snapshot();
        self.lines.swap(self.cursor.line, self.cursor.line - 1);
        self.cursor.line -= 1;
    }

    pub fn move_line_down(&mut self) {
        if self.cursor.line + 1 >= self.lines.len() {
            return;
        }
        self.record_snapshot();
        self.lines.swap(self.cursor.line, self.cursor.line + 1);
        self.cursor.line += 1;
    }

    pub fn join_lines(&mut self) {
        if self.cursor.line + 1 >= self.lines.len() {
            return;
        }
        self.record_snapshot();
        let next = self.lines.remove(self.cursor.line + 1);
        let join_col = self.lines[self.cursor.line].chars().count();
        let next_trimmed = next.trim_start();
        if !self.lines[self.cursor.line].is_empty() && !next_trimmed.is_empty() {
            self.lines[self.cursor.line].push(' ');
        }
        self.lines[self.cursor.line].push_str(next_trimmed);
        self.cursor.col = join_col;
        self.cursor.desired_col = self.cursor.col;
    }

    pub fn replace_char(&mut self, ch: char) {
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        if self.cursor.col < chars.len() {
            self.record_snapshot();
            let mut new_chars = chars.clone();
            new_chars[self.cursor.col] = ch;
            self.lines[self.cursor.line] = new_chars.into_iter().collect();
        }
    }

    pub fn delete_word(&mut self) {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        if self.cursor.col >= chars.len() {
            return;
        }
        self.record_snapshot();

        let mut end = self.cursor.col;
        let in_word = chars[end].is_alphanumeric() || chars[end] == '_';
        while end < chars.len() && ((chars[end].is_alphanumeric() || chars[end] == '_') == in_word) {
            end += 1;
        }
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }

        let new_line: String = chars[..self.cursor.col]
            .iter()
            .chain(chars[end..].iter())
            .collect();
        self.lines[self.cursor.line] = new_line;
    }

    pub fn change_word(&mut self) -> bool {
        self.delete_word();
        true
    }

    pub fn toggle_case(&mut self) {
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        if self.cursor.col < chars.len() {
            self.record_snapshot();
            let c = chars[self.cursor.col];
            let toggled = if c.is_uppercase() {
                c.to_lowercase().next().unwrap_or(c)
            } else if c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            };
            let mut new_chars = chars.clone();
            new_chars[self.cursor.col] = toggled;
            self.lines[self.cursor.line] = new_chars.into_iter().collect();
            self.cursor.col += 1;
            self.cursor.desired_col = self.cursor.col;
        }
    }

    pub fn transform_selection_case(&mut self, uppercase: bool) {
        if let Some(sel) = self.selection {
            let (sl, sc, el, ec) = sel.normalized();
            self.record_snapshot();
            for l in sl..=el {
                let line_chars: Vec<char> = self.lines[l].chars().collect();
                let start = if l == sl { sc } else { 0 };
                let end = if l == el { ec.min(line_chars.len()) } else { line_chars.len() };
                let mut new_line: Vec<char> = Vec::new();
                for (i, &ch) in line_chars.iter().enumerate() {
                    if i >= start && i < end {
                        new_line.push(if uppercase {
                            ch.to_uppercase().next().unwrap_or(ch)
                        } else {
                            ch.to_lowercase().next().unwrap_or(ch)
                        });
                    } else {
                        new_line.push(ch);
                    }
                }
                self.lines[l] = new_line.into_iter().collect();
            }
        }
    }

    pub fn increment_number(&mut self) {
        self.modify_number_at_cursor(1);
    }

    pub fn decrement_number(&mut self) {
        self.modify_number_at_cursor(-1);
    }

    pub fn modify_number_at_cursor(&mut self, delta: i64) {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let mut start = self.cursor.col;
        while start < chars.len() && !chars[start].is_ascii_digit() {
            start += 1;
        }
        if start >= chars.len() {
            return;
        }
        let mut end = start;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
        let num_str: String = chars[start..end].iter().collect();
        if let Ok(num) = num_str.parse::<i64>() {
            self.record_snapshot();
            let new_num = num + delta;
            let new_str = new_num.to_string();
            let before: String = chars[..start].iter().collect();
            let after: String = chars[end..].iter().collect();
            let new_line = format!("{}{}{}", before, new_str, after);
            self.lines[self.cursor.line] = new_line;
            self.cursor.col = start;
            self.cursor.desired_col = self.cursor.col;
        }
    }

    pub fn comment_toggle(&mut self, comment_str: &str) {
        let (start_line, end_line) = if let Some(sel) = self.selection {
            let (sl, _, el, _) = sel.normalized();
            (sl, el)
        } else {
            (self.cursor.line, self.cursor.line)
        };

        let all_commented = (start_line..=end_line).all(|l| {
            self.lines[l].trim_start().starts_with(comment_str)
        });

        self.record_snapshot();
        if all_commented {
            for l in start_line..=end_line {
                let line = &self.lines[l];
                let trimmed_start = line.len() - line.trim_start().len();
                let after_indent = &line.trim_start();
                if after_indent.starts_with(comment_str) {
                    let new_line = format!(
                        "{}{}",
                        &line[..trimmed_start],
                        &after_indent[comment_str.len()..]
                    );
                    self.lines[l] = new_line;
                }
            }
        } else {
            for l in start_line..=end_line {
                let line = &self.lines[l];
                let indent_len = line.len() - line.trim_start().len();
                let new_line = format!(
                    "{}{} {}",
                    &line[..indent_len],
                    comment_str,
                    &line[indent_len..]
                );
                self.lines[l] = new_line;
            }
        }
    }

    // --- Vim Text Objects ---

    pub fn find_inner_word(&self) -> Option<(usize, usize)> {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return None;
        }
        let col = self.cursor.col.min(chars.len() - 1);
        let in_word = chars[col].is_alphanumeric() || chars[col] == '_';

        let mut start = col;
        while start > 0 && ((chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') == in_word) && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        let mut end = col;
        while end + 1 < chars.len() && ((chars[end + 1].is_alphanumeric() || chars[end + 1] == '_') == in_word) && !chars[end + 1].is_whitespace() {
            end += 1;
        }
        Some((start, end + 1))
    }

    pub fn find_around_word(&self) -> Option<(usize, usize)> {
        let (mut start, mut end) = self.find_inner_word()?;
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        while end < chars.len() && chars[end].is_whitespace() {
            end += 1;
        }
        if end == chars.len() {
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
        }
        Some((start, end))
    }

    pub fn find_quotes(&self, quote_ch: char, inner: bool) -> Option<(usize, usize)> {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let col = self.cursor.col;

        let mut quote_indices = Vec::new();
        for (i, &c) in chars.iter().enumerate() {
            if c == quote_ch && (i == 0 || chars[i - 1] != '\\') {
                quote_indices.push(i);
            }
        }

        for i in 0..quote_indices.len().saturating_sub(1) {
            let q1 = quote_indices[i];
            let q2 = quote_indices[i + 1];
            if col >= q1 && col <= q2 {
                return if inner {
                    Some((q1 + 1, q2))
                } else {
                    Some((q1, q2 + 1))
                };
            }
        }
        None
    }

    pub fn find_brackets(&self, open_ch: char, close_ch: char, inner: bool) -> Option<(usize, usize, usize, usize)> {
        // Search forward and backward for surrounding brackets
        let mut depth = 0;
        let mut start_pos = None;

        // Backward search for opening bracket
        let mut l = self.cursor.line;
        let mut c = self.cursor.col;
        loop {
            let chars: Vec<char> = self.lines[l].chars().collect();
            while c > 0 {
                c -= 1;
                if chars[c] == close_ch {
                    depth += 1;
                } else if chars[c] == open_ch {
                    if depth == 0 {
                        start_pos = Some((l, c));
                        break;
                    } else {
                        depth -= 1;
                    }
                }
            }
            if start_pos.is_some() || l == 0 {
                break;
            }
            l -= 1;
            c = self.lines[l].chars().count();
        }

        let (sl, sc) = start_pos?;

        // Forward search for matching closing bracket
        let mut end_pos = None;
        depth = 0;
        l = sl;
        c = sc;
        loop {
            let chars: Vec<char> = self.lines[l].chars().collect();
            while c < chars.len() {
                if chars[c] == open_ch {
                    depth += 1;
                } else if chars[c] == close_ch {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some((l, c));
                        break;
                    }
                }
                c += 1;
            }
            if end_pos.is_some() || l + 1 >= self.lines.len() {
                break;
            }
            l += 1;
            c = 0;
        }

        let (el, ec) = end_pos?;

        if inner {
            Some((sl, sc + 1, el, ec))
        } else {
            Some((sl, sc, el, ec + 1))
        }
    }

    pub fn surround_selection(&mut self, open_ch: char, close_ch: char) {
        if let Some(sel) = self.selection {
            let (sl, sc, el, ec) = sel.normalized();
            self.record_snapshot();
            if sl == el {
                let line = &mut self.lines[sl];
                let chars: Vec<char> = line.chars().collect();
                let before: String = chars[..sc].iter().collect();
                let selected: String = chars[sc..ec].iter().collect();
                let after: String = chars[ec..].iter().collect();
                *line = format!("{}{}{}{}{}", before, open_ch, selected, close_ch, after);
            }
        }
    }

    pub fn matching_bracket_pos(&self) -> Option<(usize, usize)> {
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        if self.cursor.col >= chars.len() {
            return None;
        }

        let open = chars[self.cursor.col];
        let (close, forward) = match open {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            '<' => ('>', true),
            ')' => ('(', false),
            ']' => ('[', false),
            '}' => ('{', false),
            '>' => ('<', false),
            _ => return None,
        };

        let mut depth = 0i32;
        if forward {
            let mut l = self.cursor.line;
            let mut c = self.cursor.col;
            loop {
                let line_chars: Vec<char> = self.lines[l].chars().collect();
                while c < line_chars.len() {
                    if line_chars[c] == open {
                        depth += 1;
                    } else if line_chars[c] == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some((l, c));
                        }
                    }
                    c += 1;
                }
                l += 1;
                if l >= self.lines.len() {
                    break;
                }
                c = 0;
            }
        } else {
            let mut l = self.cursor.line;
            let mut c = self.cursor.col;
            loop {
                let line_chars: Vec<char> = self.lines[l].chars().collect();
                loop {
                    if c == 0 {
                        break;
                    }
                    c -= 1;
                    if line_chars[c] == open {
                        depth += 1;
                    } else if line_chars[c] == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some((l, c));
                        }
                    }
                }
                if l == 0 {
                    break;
                }
                l -= 1;
                c = self.lines[l].chars().count();
            }
        }
        None
    }

    pub fn get_word_under_cursor(&self) -> Option<String> {
        let (start, end) = self.find_inner_word()?;
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        Some(chars[start..end].iter().collect())
    }

    pub fn goto_line(&mut self, line_num: usize) {
        if line_num == 0 {
            return;
        }
        let target = (line_num - 1).min(self.lines.len().saturating_sub(1));
        self.cursor.line = target;
        self.cursor.col = 0;
        self.cursor.desired_col = 0;
    }

    pub fn scroll_center(&mut self, view_height: usize) {
        if view_height == 0 {
            return;
        }
        self.scroll_top = self.cursor.line.saturating_sub(view_height / 2);
    }

    pub fn scroll_top(&mut self) {
        self.scroll_top = self.cursor.line;
    }

    pub fn scroll_bottom(&mut self, view_height: usize) {
        if view_height == 0 {
            return;
        }
        self.scroll_top = self.cursor.line.saturating_sub(view_height - 1);
    }

    pub fn word_count(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.split_whitespace().count())
            .sum()
    }

    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.chars().count()).sum()
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn has_trailing_whitespace(&self) -> bool {
        self.lines[self.cursor.line].ends_with(' ') || self.lines[self.cursor.line].ends_with('\t')
    }

    pub fn auto_close_pair(ch: char) -> Option<char> {
        match ch {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        }
    }

    pub fn insert_char_auto_close(&mut self, ch: char) {
        // 1. Auto-wrap selection if text is selected
        if let Some(sel) = self.selection {
            if let Some(close) = Self::auto_close_pair(ch) {
                let (sl, sc, el, ec) = sel.normalized();
                if sl == el {
                    self.record_snapshot();
                    let line = &mut self.lines[sl];
                    let chars: Vec<char> = line.chars().collect();
                    let before: String = chars[..sc].iter().collect();
                    let selected: String = chars[sc..ec.min(chars.len())].iter().collect();
                    let after: String = chars[ec.min(chars.len())..].iter().collect();
                    *line = format!("{}{}{}{}{}", before, ch, selected, close, after);
                    self.selection = None;
                    self.cursor.col = sc + selected.chars().count() + 2;
                    self.cursor.desired_col = self.cursor.col;
                    return;
                }
            }
        }

        // 2. VSCode Overtyping / Skip-Over check:
        // If cursor is immediately before a closing bracket or quote and the user types that exact character, skip over it!
        let line = &self.lines[self.cursor.line];
        let chars: Vec<char> = line.chars().collect();
        let idx = self.cursor.col;
        if idx < chars.len() && chars[idx] == ch {
            let is_closable = matches!(ch, ')' | ']' | '}' | '"' | '\'' | '`');
            if is_closable {
                self.cursor.col += 1;
                self.cursor.desired_col = self.cursor.col;
                return;
            }
        }

        // 3. Auto-pair insertion
        if let Some(close) = Self::auto_close_pair(ch) {
            self.record_snapshot();
            self.delete_selection_if_any();
            let line = &mut self.lines[self.cursor.line];
            let chars: Vec<char> = line.chars().collect();
            let idx = self.cursor.col.min(chars.len());
            let mut new_chars: Vec<char> = chars[..idx].iter().cloned().collect();
            new_chars.push(ch);
            new_chars.push(close);
            new_chars.extend(chars[idx..].iter());
            *line = new_chars.into_iter().collect();
            self.cursor.col += 1; // Position cursor in between the pair
            self.cursor.desired_col = self.cursor.col;
        } else {
            self.insert_char(ch);
        }
    }

    pub fn comment_string(&self) -> &'static str {
        if let Some(ref path) = self.path {
            if let Some(ext) = path.extension() {
                match ext.to_string_lossy().as_ref() {
                    "ad" | "adl" | "rs" | "c" | "cpp" | "h" | "hpp" | "java" | "go" | "js" | "ts" | "swift" | "kt" | "scala" => "//",
                    "py" | "sh" | "bash" | "yaml" | "yml" | "toml" | "rb" | "pl" => "#",
                    "sql" => "--",
                    _ => "//",
                }
            } else {
                "//"
            }
        } else {
            "//"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_editing() {
        let mut buf = Buffer::new_empty();
        buf.insert_char('a');
        buf.insert_char('b');
        buf.insert_char('c');
        assert_eq!(buf.lines[0], "abc");
        assert_eq!(buf.cursor.col, 3);

        buf.backspace();
        assert_eq!(buf.lines[0], "ab");

        buf.insert_newline();
        buf.insert_char('d');
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[1], "d");

        buf.undo();
        assert_eq!(buf.lines.len(), 2);
        buf.undo();
        assert_eq!(buf.lines.len(), 1);
        assert_eq!(buf.lines[0], "ab");
    }

    #[test]
    fn test_multi_cursor_and_text_objects() {
        let mut buf = Buffer::new_empty();
        buf.lines = vec![
            "fn calculate(x: i32) {".to_string(),
            "    let result = \"hello\";".to_string(),
            "}".to_string(),
        ];
        buf.cursor = Cursor { line: 0, col: 4, desired_col: 4 };

        // Test inner word at "calculate"
        let (start, end) = buf.find_inner_word().unwrap();
        assert_eq!(&buf.lines[0][start..end], "calculate");

        // Test quotes
        buf.cursor = Cursor { line: 1, col: 19, desired_col: 19 };
        let (q_start, q_end) = buf.find_quotes('"', true).unwrap();
        assert_eq!(&buf.lines[1][q_start..q_end], "hello");

        // Test matching bracket
        buf.cursor = Cursor { line: 0, col: 12, desired_col: 12 };
        let match_pos = buf.matching_bracket_pos();
        assert_eq!(match_pos, Some((0, 19)));
    }
}
