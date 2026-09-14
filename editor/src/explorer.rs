use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub expanded: bool,
    pub children: Vec<FileNode>,
    pub children_loaded: bool,
}

impl FileNode {
    pub fn icon(&self) -> &'static str {
        if self.is_dir {
            if self.expanded { "📂" } else { "📁" }
        } else {
            let ext = self
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "ad" | "adl" | "adesh" => "✨",
                "rs" => "🦀",
                "py" => "🐍",
                "c" | "h" | "cpp" | "hpp" => "⚙",
                "js" | "jsx" | "ts" | "tsx" => "⚡",
                "json" | "toml" | "yaml" | "yml" => "🔧",
                "md" | "txt" => "📝",
                "html" | "css" => "🌐",
                "sh" | "bash" | "bat" | "ps1" => "💻",
                _ => "📄",
            }
        }
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut FileNode> {
        if self.path == path {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(path) {
                return Some(found);
            }
        }
        None
    }

    pub fn find(&self, path: &Path) -> Option<&FileNode> {
        if self.path == path {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(path) {
                return Some(found);
            }
        }
        None
    }
}

pub struct FileExplorer {
    pub root_path: PathBuf,
    pub root_node: FileNode,
    pub selected_index: usize,
    pub flat_nodes: Vec<(PathBuf, usize)>, // (path, depth)
    pub scroll_offset: usize,
    pub show_hidden: bool,
    pub filter_query: String,
}

impl FileExplorer {
    pub fn new(root: PathBuf) -> Self {
        let mut explorer = Self {
            root_path: root.clone(),
            root_node: FileNode {
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "PROJECT".to_string()),
                path: root,
                is_dir: true,
                expanded: true,
                children: Vec::new(),
                children_loaded: false,
            },
            selected_index: 0,
            flat_nodes: Vec::new(),
            scroll_offset: 0,
            show_hidden: false,
            filter_query: String::new(),
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        let show_hidden = self.show_hidden;
        Self::load_children(&mut self.root_node, show_hidden);
        self.rebuild_flat_list();
    }

    fn load_children(node: &mut FileNode, show_hidden: bool) {
        if !node.is_dir {
            return;
        }
        let mut children = Vec::new();
        if let Ok(entries) = fs::read_dir(&node.path) {
            let mut entries_vec: Vec<_> = entries.flatten().collect();
            entries_vec.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));

            for entry in entries_vec {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if !show_hidden
                    && (name.starts_with('.') || name == "target" || name == "node_modules")
                {
                    continue;
                }
                let is_dir = p.is_dir();

                let (expanded, children_loaded, existing_children) =
                    if let Some(existing) = node.children.iter().find(|c| c.path == p) {
                        (
                            existing.expanded,
                            existing.children_loaded,
                            existing.children.clone(),
                        )
                    } else {
                        (false, false, Vec::new())
                    };

                let mut child = FileNode {
                    name,
                    path: p,
                    is_dir,
                    expanded,
                    children: existing_children,
                    children_loaded,
                };

                if is_dir && expanded {
                    Self::load_children(&mut child, show_hidden);
                }

                children.push(child);
            }
        }
        node.children = children;
        node.children_loaded = true;
    }

    pub fn toggle_expand(&mut self) {
        let path = match self.flat_nodes.get(self.selected_index) {
            Some((p, _)) => p.clone(),
            None => return,
        };
        let show_hidden = self.show_hidden;
        if let Some(node) = self.root_node.find_mut(&path) {
            if node.is_dir {
                node.expanded = !node.expanded;
                if node.expanded {
                    Self::load_children(node, show_hidden);
                }
            }
        }
        self.rebuild_flat_list();
    }

    pub fn toggle_expand_at(&mut self, idx: usize) {
        let path = match self.flat_nodes.get(idx) {
            Some((p, _)) => p.clone(),
            None => return,
        };
        let show_hidden = self.show_hidden;
        if let Some(node) = self.root_node.find_mut(&path) {
            if node.is_dir {
                node.expanded = !node.expanded;
                if node.expanded {
                    Self::load_children(node, show_hidden);
                }
            }
        }
        self.rebuild_flat_list();
    }

    pub fn expand_all(&mut self) {
        Self::set_expanded_recursive(&mut self.root_node, true, self.show_hidden);
        self.rebuild_flat_list();
    }

    pub fn collapse_all(&mut self) {
        Self::set_expanded_recursive(&mut self.root_node, false, self.show_hidden);
        self.root_node.expanded = true; // Keep root open
        self.rebuild_flat_list();
    }

    fn set_expanded_recursive(node: &mut FileNode, expanded: bool, show_hidden: bool) {
        if node.is_dir {
            node.expanded = expanded;
            if expanded {
                Self::load_children(node, show_hidden);
            }
            for child in &mut node.children {
                Self::set_expanded_recursive(child, expanded, show_hidden);
            }
        }
    }

    pub fn selected_is_dir(&self) -> bool {
        self.flat_nodes
            .get(self.selected_index)
            .map(|(p, _)| p.is_dir())
            .unwrap_or(false)
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.flat_nodes.get(self.selected_index).map(|(p, _)| p)
    }

    pub fn select_next(&mut self) {
        if !self.flat_nodes.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.flat_nodes.len();
            self.adjust_scroll();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.flat_nodes.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.flat_nodes.len() - 1;
            } else {
                self.selected_index -= 1;
            }
            self.adjust_scroll();
        }
    }

    pub fn scroll_up(&mut self, delta: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(delta);
    }

    pub fn scroll_down(&mut self, delta: usize) {
        let max_scroll = self.flat_nodes.len().saturating_sub(5);
        self.scroll_offset = (self.scroll_offset + delta).min(max_scroll);
    }

    pub fn adjust_scroll(&mut self) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + 25 {
            self.scroll_offset = self.selected_index.saturating_sub(24);
        }
    }

    pub fn create_file(&mut self, relative_name: &str) -> std::io::Result<PathBuf> {
        let parent = if let Some(sel) = self.selected_path() {
            if sel.is_dir() {
                sel.clone()
            } else {
                sel.parent().unwrap_or(&self.root_path).to_path_buf()
            }
        } else {
            self.root_path.clone()
        };

        let new_path = parent.join(relative_name);
        if let Some(p) = new_path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&new_path, "")?;
        self.refresh();
        Ok(new_path)
    }

    pub fn create_dir(&mut self, relative_name: &str) -> std::io::Result<PathBuf> {
        let parent = if let Some(sel) = self.selected_path() {
            if sel.is_dir() {
                sel.clone()
            } else {
                sel.parent().unwrap_or(&self.root_path).to_path_buf()
            }
        } else {
            self.root_path.clone()
        };

        let new_path = parent.join(relative_name);
        fs::create_dir_all(&new_path)?;
        self.refresh();
        Ok(new_path)
    }

    pub fn rename_file(&mut self, new_name: &str) -> std::io::Result<PathBuf> {
        let src = match self.selected_path() {
            Some(p) => p.clone(),
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No selection",
                ));
            }
        };
        let parent = src.parent().unwrap_or(&self.root_path);
        let dest = parent.join(new_name);
        fs::rename(&src, &dest)?;
        self.refresh();
        Ok(dest)
    }

    pub fn delete_file(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
        self.refresh();
        if self.selected_index >= self.flat_nodes.len() {
            self.selected_index = self.flat_nodes.len().saturating_sub(1);
        }
        Ok(())
    }

    fn rebuild_flat_list(&mut self) {
        self.flat_nodes.clear();
        let filter = self.filter_query.to_lowercase();
        Self::collect_nodes(&self.root_node, 0, &mut self.flat_nodes, &filter);
        if self.selected_index >= self.flat_nodes.len() {
            self.selected_index = self.flat_nodes.len().saturating_sub(1);
        }
    }

    fn collect_nodes(
        node: &FileNode,
        depth: usize,
        flat: &mut Vec<(PathBuf, usize)>,
        filter: &str,
    ) {
        let matches = filter.is_empty() || node.name.to_lowercase().contains(filter);
        if matches {
            flat.push((node.path.clone(), depth));
        }

        if node.is_dir && node.expanded {
            for child in &node.children {
                Self::collect_nodes(child, depth + 1, flat, filter);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_explorer_expand_collapse() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("test.ad"), "let a = 1;").unwrap();

        let mut explorer = FileExplorer::new(dir.path().to_path_buf());
        assert!(!explorer.flat_nodes.is_empty());

        let count_before = explorer.flat_nodes.len();
        explorer.expand_all();
        assert!(explorer.flat_nodes.len() > count_before);

        explorer.collapse_all();
        assert_eq!(explorer.flat_nodes.len(), 2); // Root + direct subdir (unexpanded)
    }
}
