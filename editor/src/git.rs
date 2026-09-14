use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileGitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Clean,
}

impl FileGitStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            FileGitStatus::Modified => "M",
            FileGitStatus::Added => "A",
            FileGitStatus::Deleted => "D",
            FileGitStatus::Renamed => "R",
            FileGitStatus::Untracked => "U",
            FileGitStatus::Ignored => "!",
            FileGitStatus::Clean => " ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterDiffKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct GitCommitInfo {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct GitManager {
    pub is_repo: bool,
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub file_statuses: HashMap<PathBuf, FileGitStatus>,
    pub staged_files: Vec<(PathBuf, FileGitStatus)>,
    pub unstaged_files: Vec<(PathBuf, FileGitStatus)>,
    pub untracked_files: Vec<PathBuf>,
    pub commit_history: Vec<GitCommitInfo>,
    pub current_diff: String,
    pub repo_root: PathBuf,
}

impl GitManager {
    pub fn new(workspace_root: &Path) -> Self {
        let mut manager = Self {
            is_repo: false,
            branch: "main".to_string(),
            ahead: 0,
            behind: 0,
            file_statuses: HashMap::new(),
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            untracked_files: Vec::new(),
            commit_history: Vec::new(),
            current_diff: String::new(),
            repo_root: workspace_root.to_path_buf(),
        };
        manager.refresh(workspace_root);
        manager
    }

    pub fn refresh(&mut self, workspace_root: &Path) {
        self.repo_root = workspace_root.to_path_buf();
        // Check if repo
        let check = Command::new("git")
            .arg("rev-parse")
            .arg("--is-inside-work-tree")
            .current_dir(workspace_root)
            .output();

        if let Ok(out) = check {
            if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true" {
                self.is_repo = true;
            } else {
                self.is_repo = false;
                return;
            }
        } else {
            self.is_repo = false;
            return;
        }

        // Get branch name
        if let Ok(out) = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(workspace_root)
            .output()
        {
            let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
            self.branch = if b.is_empty() {
                "HEAD (detached)".to_string()
            } else {
                b
            };
        }

        // Get status porcelain
        self.file_statuses.clear();
        self.staged_files.clear();
        self.unstaged_files.clear();
        self.untracked_files.clear();

        if let Ok(out) = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(workspace_root)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.len() < 3 {
                    continue;
                }
                let index_status = line.chars().next().unwrap_or(' ');
                let work_status = line.chars().nth(1).unwrap_or(' ');
                let path_str = line[3..].trim();
                let full_path = workspace_root.join(path_str);

                if index_status == '?' && work_status == '?' {
                    self.file_statuses
                        .insert(full_path.clone(), FileGitStatus::Untracked);
                    self.untracked_files.push(full_path);
                    continue;
                }

                if index_status != ' ' && index_status != '?' {
                    let st = match index_status {
                        'M' => FileGitStatus::Modified,
                        'A' => FileGitStatus::Added,
                        'D' => FileGitStatus::Deleted,
                        'R' => FileGitStatus::Renamed,
                        _ => FileGitStatus::Modified,
                    };
                    self.staged_files.push((full_path.clone(), st));
                    self.file_statuses.insert(full_path.clone(), st);
                }

                if work_status != ' ' && work_status != '?' {
                    let st = match work_status {
                        'M' => FileGitStatus::Modified,
                        'D' => FileGitStatus::Deleted,
                        _ => FileGitStatus::Modified,
                    };
                    self.unstaged_files.push((full_path.clone(), st));
                    self.file_statuses.insert(full_path, st);
                }
            }
        }
    }

    pub fn get_file_status(&self, path: &Path) -> FileGitStatus {
        self.file_statuses
            .get(path)
            .copied()
            .unwrap_or(FileGitStatus::Clean)
    }

    pub fn stage_file(&mut self, path: &Path) -> Result<(), String> {
        let out = Command::new("git")
            .arg("add")
            .arg(path)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn unstage_file(&mut self, path: &Path) -> Result<(), String> {
        let out = Command::new("git")
            .arg("restore")
            .arg("--staged")
            .arg(path)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn stage_all(&mut self) -> Result<(), String> {
        let out = Command::new("git")
            .arg("add")
            .arg("-A")
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn commit(&mut self, message: &str) -> Result<String, String> {
        if message.trim().is_empty() {
            return Err("Commit message cannot be empty".to_string());
        }

        let out = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn push(&mut self) -> Result<String, String> {
        let out = Command::new("git")
            .arg("push")
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok("Pushed successfully to remote".to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn pull(&mut self) -> Result<String, String> {
        let out = Command::new("git")
            .arg("pull")
            .current_dir(&self.repo_root)
            .output()
            .map_err(|e| e.to_string())?;

        if out.status.success() {
            let root = self.repo_root.clone();
            self.refresh(&root);
            Ok("Pulled changes from remote".to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }

    pub fn load_commit_history(&mut self, count: usize) {
        self.commit_history.clear();
        if let Ok(out) = Command::new("git")
            .arg("log")
            .arg(format!("-n{}", count))
            .arg("--pretty=format:%h|%an|%ar|%s")
            .current_dir(&self.repo_root)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    self.commit_history.push(GitCommitInfo {
                        hash: parts[0].to_string(),
                        author: parts[1].to_string(),
                        date: parts[2].to_string(),
                        message: parts[3..].join("|"),
                    });
                }
            }
        }
    }

    pub fn load_diff(&mut self, path: Option<&Path>) {
        let mut cmd = Command::new("git");
        cmd.arg("diff");
        if let Some(p) = path {
            cmd.arg(p);
        }
        cmd.current_dir(&self.repo_root);

        if let Ok(out) = cmd.output() {
            self.current_diff = String::from_utf8_lossy(&out.stdout).to_string();
        }
    }

    /// List all local git branches
    pub fn list_branches(&self) -> Vec<String> {
        let mut branches = Vec::new();
        if let Ok(out) = Command::new("git")
            .arg("branch")
            .current_dir(&self.repo_root)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim_start_matches('*').trim();
                if !trimmed.is_empty() {
                    branches.push(trimmed.to_string());
                }
            }
        }
        if branches.is_empty() {
            branches.push(self.branch.clone());
        }
        branches
    }

    /// Calculate gutter diff markers (Added/Modified/Deleted) for lines in an open buffer.
    pub fn compute_gutter_diff(
        &self,
        file_path: Option<&Path>,
        lines: &[String],
    ) -> HashMap<usize, GutterDiffKind> {
        let mut map = HashMap::new();
        let path = match file_path {
            Some(p) => p,
            None => return map,
        };
        if !self.is_repo {
            return map;
        }

        // Get git diff for this specific file
        if let Ok(out) = Command::new("git")
            .arg("diff")
            .arg("-U0")
            .arg(path)
            .current_dir(&self.repo_root)
            .output()
        {
            let diff_text = String::from_utf8_lossy(&out.stdout);
            for line in diff_text.lines() {
                if line.starts_with("@@ ") {
                    // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                    let parts: Vec<&str> = line.split(' ').collect();
                    if parts.len() >= 3 {
                        let new_hunk = parts[2].trim_start_matches('+');
                        let hunk_parts: Vec<&str> = new_hunk.split(',').collect();
                        if let Ok(start_line) = hunk_parts[0].parse::<usize>() {
                            let count = if hunk_parts.len() > 1 {
                                hunk_parts[1].parse::<usize>().unwrap_or(1)
                            } else {
                                1
                            };

                            let start_idx = start_line.saturating_sub(1);
                            if count == 0 {
                                map.insert(start_idx, GutterDiffKind::Deleted);
                            } else {
                                for i in 0..count {
                                    if start_idx + i < lines.len() {
                                        map.insert(start_idx + i, GutterDiffKind::Modified);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        map
    }
}
