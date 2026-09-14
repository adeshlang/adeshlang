use crate::ecosystem::lockfile::LockFile;
use crate::ecosystem::manifest::{
    BindingKind, Manifest, ManifestItem, ManifestSection, ManifestValue,
};
use crate::ecosystem::version::SemVer;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTemplate {
    App,
    Library,
    Workspace,
    Plugin,
    Package,
}

#[derive(Debug, Clone)]
pub struct ProjectLayout {
    pub root: PathBuf,
}

impl ProjectLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("adesh.adl")
    }

    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("adesh.lock.adl")
    }

    pub fn adl_dir(&self) -> PathBuf {
        self.root.join(".adl")
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.adl_dir().join("packages")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.adl_dir().join("cache")
    }

    pub fn registry_dir(&self) -> PathBuf {
        self.adl_dir().join("registry")
    }

    pub fn git_dir(&self) -> PathBuf {
        self.adl_dir().join("git")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.adl_dir().join("downloads")
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.adl_dir().join("artifacts")
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.adl_dir().join("bin")
    }

    pub fn target_dir(&self) -> PathBuf {
        self.root.join("target")
    }

    pub fn target_bin_dir(&self) -> PathBuf {
        self.target_dir().join("bin")
    }

    pub fn src_bin_dir(&self) -> PathBuf {
        self.root.join("src").join("bin")
    }

    pub fn binary_search_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.bin_dir(),
            self.target_bin_dir(),
            self.target_dir().join("release"),
            self.target_dir().join("debug"),
            self.target_dir(),
            self.root.join("bin"),
            self.artifacts_dir(),
        ]
    }

    pub fn ensure_layout(&self) -> Result<(), String> {
        for dir in [
            self.packages_dir(),
            self.cache_dir(),
            self.registry_dir(),
            self.git_dir(),
            self.downloads_dir(),
            self.artifacts_dir(),
            self.bin_dir(),
        ] {
            fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn locate_binary(&self, name: &str) -> Option<PathBuf> {
        let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
        let candidate_names = [
            format!("{}{}", name, exe_suffix),
            name.to_string(),
            format!("{}.exe", name),
            format!("lib{}.so", name),
            format!("lib{}.dylib", name),
            format!("lib{}.a", name),
            format!("{}.dll", name),
            format!("{}.lib", name),
        ];

        for search_dir in self.binary_search_dirs() {
            if !search_dir.exists() {
                continue;
            }
            for candidate in &candidate_names {
                let candidate_path = search_dir.join(candidate);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }

        // Fallback: search via locate_binary_builds
        let builds = self.locate_binary_builds(Some(name));
        builds.into_iter().next().map(|b| b.path)
    }

    pub fn locate_binary_builds(&self, name_filter: Option<&str>) -> Vec<BinaryBuildInfo> {
        let mut results = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        for search_dir in self.binary_search_dirs() {
            if !search_dir.exists() || !search_dir.is_dir() {
                continue;
            }

            let entries = match fs::read_dir(&search_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                // Canonicalize or use path for deduplication
                let norm_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen_paths.insert(norm_path) {
                    continue;
                }

                let file_name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };

                let file_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&file_name)
                    .to_string();

                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase());

                // Ignore auxiliary/build metadata files
                if let Some(ref ext_str) = ext {
                    if matches!(
                        ext_str.as_str(),
                        "d" | "pdb"
                            | "tag"
                            | "lock"
                            | "adl"
                            | "adesh"
                            | "tmp"
                            | "rs"
                            | "md"
                            | "txt"
                    ) {
                        continue;
                    }
                }
                if file_name == "CACHEDIR.TAG" || file_name.starts_with('.') {
                    continue;
                }

                if let Some(filter) = name_filter {
                    let filter_lower = filter.to_lowercase();
                    if !file_stem.to_lowercase().contains(&filter_lower)
                        && !file_name.to_lowercase().contains(&filter_lower)
                    {
                        continue;
                    }
                }

                let (kind, is_executable) = classify_binary_path(&path, ext.as_deref());
                let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                results.push(BinaryBuildInfo {
                    name: file_stem,
                    path,
                    extension: ext,
                    size_bytes,
                    is_executable,
                    kind,
                });
            }
        }

        results
    }

    pub fn list_binary_sources(&self) -> Vec<String> {
        let mut sources = Vec::new();

        // 1. Check src/main.adesh
        if self.root.join("src").join("main.adesh").exists() {
            let name = self
                .load_manifest()
                .ok()
                .and_then(|m| m.project_name())
                .unwrap_or_else(|| "main".to_string());
            sources.push(name);
        }

        // 2. Check src/bin/*.adesh
        let src_bin = self.src_bin_dir();
        if src_bin.exists() && src_bin.is_dir() {
            if let Ok(entries) = fs::read_dir(src_bin) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("adesh") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            sources.push(stem.to_string());
                        }
                    }
                }
            }
        }

        // 3. Check bin/*.adesh
        let root_bin = self.root.join("bin");
        if root_bin.exists() && root_bin.is_dir() {
            if let Ok(entries) = fs::read_dir(root_bin) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("adesh") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            if !sources.contains(&stem.to_string()) {
                                sources.push(stem.to_string());
                            }
                        }
                    }
                }
            }
        }

        sources.sort();
        sources.dedup();
        sources
    }

    pub fn discover(start: impl AsRef<Path>) -> Option<Self> {
        let mut current = start.as_ref().to_path_buf();
        if current.is_file() {
            current.pop();
        }
        loop {
            if current.join("adesh.adl").exists() {
                return Some(Self::new(current));
            }
            if !current.pop() {
                break;
            }
        }
        None
    }

    pub fn load_manifest(&self) -> Result<Manifest, String> {
        Manifest::load(self.manifest_path())
    }

    pub fn load_lockfile(&self) -> Result<LockFile, String> {
        let content =
            fs::read_to_string(self.lockfile_path()).map_err(|error| error.to_string())?;
        let lockfile = LockFile::parse(&content)?;
        lockfile.verify_signature()?;
        Ok(lockfile)
    }

    pub fn write_lockfile(&self, lockfile: &LockFile) -> Result<(), String> {
        fs::write(self.lockfile_path(), lockfile.to_string()).map_err(|error| error.to_string())
    }

    pub fn write_manifest(&self, manifest: &Manifest) -> Result<(), String> {
        manifest.save(self.manifest_path())
    }

    pub fn persist_sanitized_state(
        &self,
        manifest: &Manifest,
        lockfile: &mut LockFile,
        confidential: BTreeMap<String, String>,
    ) -> Result<(), String> {
        lockfile.set_confidential(confidential);
        self.write_manifest(manifest)?;
        self.write_lockfile(lockfile)
    }

    pub fn create_template(&self, template: ProjectTemplate, name: &str) -> Result<(), String> {
        self.ensure_layout()?;
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        fs::create_dir_all(self.root.join("src")).map_err(|error| error.to_string())?;

        self.template_manifest(template, name)
            .save(self.manifest_path())?;

        // Create standard .gitignore
        let gitignore_path = self.root.join(".gitignore");
        if !gitignore_path.exists() {
            let gitignore_content = "\
# ADL build artifacts and dependencies
target/
.adl/
adl_modules/
.adesh_cache/
*.exe
*.lib
*.dll
*.so
*.dylib
*.o
*.obj
";
            let _ = fs::write(gitignore_path, gitignore_content);
        }

        // Create README.md
        let readme_path = self.root.join("README.md");
        if !readme_path.exists() {
            let readme_content = format!(
                "# {name}\n\n\
A modern AdeshLang project managed with ADL (AdeshLang Package Manager).\n\n\
## 🚀 Quick Start\n\n\
### Run the project\n\
```bash\n\
adl run\n\
# or\n\
adesh run src/main.adesh\n\
```\n\n\
### Build native executable (AOT)\n\
```bash\n\
adl build\n\
# or\n\
adesh build src/main.adesh\n\
```\n\n\
### Add dependencies\n\
```bash\n\
adl add <package_name> <version>\n\
adl install\n\
```\n\n\
## 📁 Project Structure\n\
- `src/` — Main source code files\n\
- `adesh.adl` — Project configuration and dependency manifest\n"
            );
            let _ = fs::write(readme_path, readme_content);
        }

        match template {
            ProjectTemplate::App | ProjectTemplate::Package | ProjectTemplate::Plugin => {
                fs::write(
                    self.root.join("src").join("main.adesh"),
                    default_app_source(name),
                )
                .map_err(|error| error.to_string())?;
            }
            ProjectTemplate::Library => {
                fs::write(
                    self.root.join("src").join("lib.adesh"),
                    default_library_source(name),
                )
                .map_err(|error| error.to_string())?;
            }
            ProjectTemplate::Workspace => {
                fs::create_dir_all(self.root.join("packages"))
                    .map_err(|error| error.to_string())?;
            }
        }

        Ok(())
    }

    pub fn add_dependency_stub(&self, name: &str, requirement: &str) -> Result<(), String> {
        let mut manifest = self.load_manifest()?;
        let mut deps = if let Some(section) =
            manifest.root.items.iter_mut().find_map(|item| match item {
                ManifestItem::Section(section) if section.name == "dependencies" => Some(section),
                _ => None,
            }) {
            section.clone()
        } else {
            ManifestSection::new("dependencies")
        };
        deps.items.push(ManifestItem::Field {
            key: name.to_string(),
            value: ManifestValue::Version(crate::ecosystem::version::VersionReq::parse(
                requirement,
            )?),
        });
        manifest.root.items.retain(|item| !matches!(item, ManifestItem::Section(section) if section.name == "dependencies"));
        manifest.root.items.push(ManifestItem::Section(deps));
        manifest.save(self.manifest_path())
    }

    pub fn remove_dependency_stub(&self, name: &str) -> Result<(), String> {
        let mut manifest = self.load_manifest()?;
        if let Some(section) = manifest.root.items.iter_mut().find_map(|item| match item {
            ManifestItem::Section(section) if section.name == "dependencies" => Some(section),
            _ => None,
        }) {
            section
                .items
                .retain(|item| !matches!(item, ManifestItem::Field { key, .. } if key == name));
        }
        manifest.save(self.manifest_path())
    }

    pub fn workspace_member_paths(&self, manifest: &Manifest) -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        if let Some(members) = manifest.workspace_members() {
            for member in members {
                if member.ends_with("/*") {
                    let parent_dir = self.root.join(member.trim_end_matches("/*"));
                    if parent_dir.exists() && parent_dir.is_dir() {
                        for entry in fs::read_dir(parent_dir).map_err(|e| e.to_string())? {
                            let entry = entry.map_err(|e| e.to_string())?;
                            let path = entry.path();
                            if path.is_dir() && path.join("adesh.adl").exists() {
                                paths.push(path);
                            }
                        }
                    }
                } else {
                    let path = self.root.join(&member);
                    if path.join("adesh.adl").exists() {
                        paths.push(path);
                    }
                }
            }
        }
        Ok(paths)
    }

    fn template_manifest(&self, template: ProjectTemplate, name: &str) -> Manifest {
        let mut manifest = Manifest::empty();
        let mut project = ManifestSection::new("project");
        project.items.push(ManifestItem::Field {
            key: "name".to_string(),
            value: ManifestValue::String(name.to_string()),
        });
        project.items.push(ManifestItem::Field {
            key: "version".to_string(),
            value: ManifestValue::SemanticVersion(SemVer {
                major: 0,
                minor: 1,
                patch: 0,
                pre_release: Vec::new(),
                build_metadata: Vec::new(),
            }),
        });
        project.items.push(ManifestItem::Field {
            key: "template".to_string(),
            value: ManifestValue::Identifier(
                match template {
                    ProjectTemplate::App => "app",
                    ProjectTemplate::Library => "lib",
                    ProjectTemplate::Workspace => "workspace",
                    ProjectTemplate::Plugin => "plugin",
                    ProjectTemplate::Package => "package",
                }
                .to_string(),
            ),
        });
        manifest.root.items.push(ManifestItem::Section(project));

        let mut compiler = ManifestSection::new("compiler");
        compiler.items.push(ManifestItem::Field {
            key: "backend".to_string(),
            value: ManifestValue::Identifier("interpreter".to_string()),
        });
        compiler.items.push(ManifestItem::Field {
            key: "opt-level".to_string(),
            value: ManifestValue::Identifier("debug".to_string()),
        });
        manifest.root.items.push(ManifestItem::Section(compiler));

        let mut tasks = ManifestSection::new("scripts");
        tasks.items.push(ManifestItem::Binding {
            kind: BindingKind::Const,
            name: "build".to_string(),
            value: ManifestValue::Identifier("default".to_string()),
        });
        manifest.root.items.push(ManifestItem::Section(tasks));
        manifest
    }
    pub fn adl_modules_dir(&self) -> PathBuf {
        self.root.join("adl_modules")
    }

    pub fn populate_adl_modules(&self, lockfile: &LockFile) -> Result<(), String> {
        let modules_dir = self.adl_modules_dir();
        fs::create_dir_all(&modules_dir).map_err(|e| e.to_string())?;

        for dep in &lockfile.dependencies {
            let target_dir = modules_dir.join(&dep.name);
            if target_dir.exists() {
                let _ = fs::remove_dir_all(&target_dir);
            }
            fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

            match dep.source.kind.as_str() {
                "path" | "local" | "workspace" => {
                    let src_path = PathBuf::from(&dep.source.location);
                    let src_resolved = if src_path.is_absolute() {
                        src_path
                    } else {
                        self.root.join(&src_path)
                    };
                    if src_resolved.exists() {
                        copy_dir_recursive(&src_resolved, &target_dir)?;
                    }
                }
                "git" => {
                    let hash = crate::ecosystem::resolver::synthetic_checksum(&dep.source.location);
                    let git_clone_path = self.git_dir().join(&hash);
                    if git_clone_path.exists() {
                        copy_dir_recursive(&git_clone_path, &target_dir)?;
                    }
                }
                "registry" => {
                    let registry_path = self
                        .registry_dir()
                        .join(&dep.source.location)
                        .join(&dep.name)
                        .join(dep.version.to_string());
                    if registry_path.exists() {
                        copy_dir_recursive(&registry_path, &target_dir)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    if src.is_file() {
        fs::copy(src, dst).map_err(|e| e.to_string())?;
        return Ok(());
    }
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path.file_name().unwrap();
        copy_dir_recursive(&path, &dst.join(name))?;
    }
    Ok(())
}

fn default_app_source(name: &str) -> String {
    format!(
        "// Welcome to your AdeshLang project: {name}!\n\
//\n\
// 🚀 Useful commands:\n\
//   adl run              Run this project\n\
//   adl build            Compile to native binary\n\
//   adl test             Run project tests\n\
//   adl add <pkg> <ver>  Add a package dependency\n\n\
fn greet(name: String): String {{\n\
    return \"Hello, \" + name + \"! Welcome to AdeshLang.\";\n\
}}\n\n\
fn calculate_fibonacci(n: i64): i64 {{\n\
    if n <= 1 {{\n\
        return n;\n\
    }}\n\
    return calculate_fibonacci(n - 1) + calculate_fibonacci(n - 2);\n\
}}\n\n\
fn main(): void {{\n\
    let app_title: String = \"{name}\";\n\
    print(\"🚀 Initializing project:\", app_title);\n\
    print(greet(\"Developer\"));\n\n\
    // Type-safe collections and transformations\n\
    let numbers: [i64] = [1, 2, 3, 4, 5];\n\
    let squares: [i64] = map(numbers, fn(x: i64): i64 {{\n\
        return x * x;\n\
    }});\n\n\
    print(\"🔢 Numbers:\", numbers);\n\
    print(\"✨ Squares:\", squares);\n\
    print(\"📈 Fibonacci(10):\", calculate_fibonacci(10));\n\
}}\n"
    )
}

fn default_library_source(name: &str) -> String {
    format!(
        "// AdeshLang Library Module: {name}\n\
//\n\
// 🚀 Useful commands:\n\
//   adl build --lib      Compile this library\n\
//   adl test             Run library unit tests\n\n\
export fn add(a: i64, b: i64): i64 {{\n\
    return a + b;\n\
}}\n\n\
export fn multiply(a: i64, b: i64): i64 {{\n\
    return a * b;\n\
}}\n\n\
export fn greet(name: String): String {{\n\
    return \"Hello, \" + name + \" from the {name} library!\";\n\
}}\n\n\
export class MathHelper {{\n\
    fn square(x: i64): i64 {{\n\
        return x * x;\n\
    }}\n\
}}\n"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryBuildInfo {
    pub name: String,
    pub path: PathBuf,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub is_executable: bool,
    pub kind: BinaryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryKind {
    Executable,
    SharedLibrary,
    StaticLibrary,
    ObjectFile,
    Artifact,
}

impl BinaryKind {
    pub fn description(&self) -> &'static str {
        match self {
            BinaryKind::Executable => "Executable",
            BinaryKind::SharedLibrary => "Shared Library",
            BinaryKind::StaticLibrary => "Static Library",
            BinaryKind::ObjectFile => "Object File",
            BinaryKind::Artifact => "Build Artifact",
        }
    }
}

fn classify_binary_path(path: &Path, ext: Option<&str>) -> (BinaryKind, bool) {
    match ext {
        Some("exe") => (BinaryKind::Executable, true),
        Some("dll") | Some("so") | Some("dylib") => (BinaryKind::SharedLibrary, false),
        Some("lib") | Some("a") => (BinaryKind::StaticLibrary, false),
        Some("obj") | Some("o") => (BinaryKind::ObjectFile, false),
        Some("bin") => (BinaryKind::Executable, true),
        Some(_) => (BinaryKind::Artifact, false),
        None => {
            // Unix binary without extension: check if executable or not Windows
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let is_exec = fs::metadata(path)
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false);
                if is_exec {
                    (BinaryKind::Executable, true)
                } else {
                    (BinaryKind::Artifact, false)
                }
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                (BinaryKind::Executable, true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_layout_directories() {
        let layout = ProjectLayout::new(PathBuf::from("/test/project"));
        assert_eq!(
            layout.manifest_path(),
            PathBuf::from("/test/project/adesh.adl")
        );
        assert_eq!(
            layout.lockfile_path(),
            PathBuf::from("/test/project/adesh.lock.adl")
        );
        assert_eq!(layout.adl_dir(), PathBuf::from("/test/project/.adl"));
        assert_eq!(layout.bin_dir(), PathBuf::from("/test/project/.adl/bin"));
        assert_eq!(layout.target_dir(), PathBuf::from("/test/project/target"));
        assert_eq!(
            layout.target_bin_dir(),
            PathBuf::from("/test/project/target/bin")
        );
        assert_eq!(layout.src_bin_dir(), PathBuf::from("/test/project/src/bin"));
        assert_eq!(
            layout.artifacts_dir(),
            PathBuf::from("/test/project/.adl/artifacts")
        );
    }

    #[test]
    fn test_locate_binaries_and_sources() {
        let temp_dir = std::env::temp_dir().join(format!(
            "adl_test_bin_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let layout = ProjectLayout::new(&temp_dir);
        layout.ensure_layout().unwrap();

        // Create src/main.adesh and src/bin/cli.adesh
        fs::create_dir_all(layout.src_bin_dir()).unwrap();
        fs::write(
            layout.root.join("src").join("main.adesh"),
            "print(\"hello\");",
        )
        .unwrap();
        fs::write(layout.src_bin_dir().join("tool.adesh"), "print(\"tool\");").unwrap();

        let sources = layout.list_binary_sources();
        assert!(sources.contains(&"main".to_string()));
        assert!(sources.contains(&"tool".to_string()));

        // Create mock built binaries in .adl/bin and target/bin
        fs::create_dir_all(layout.target_bin_dir()).unwrap();
        let target_exe = if cfg!(windows) {
            layout.target_bin_dir().join("tool.exe")
        } else {
            layout.target_bin_dir().join("tool")
        };
        fs::write(&target_exe, b"mock_exe").unwrap();

        let adl_bin_exe = if cfg!(windows) {
            layout.bin_dir().join("main.exe")
        } else {
            layout.bin_dir().join("main")
        };
        fs::write(&adl_bin_exe, b"mock_main_exe").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&target_exe).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&target_exe, perms).unwrap();

            let mut perms_main = fs::metadata(&adl_bin_exe).unwrap().permissions();
            perms_main.set_mode(0o755);
            fs::set_permissions(&adl_bin_exe, perms_main).unwrap();
        }

        let located = layout.locate_binary("tool");
        assert!(located.is_some());
        assert_eq!(located.unwrap(), target_exe);

        let located_main = layout.locate_binary("main");
        assert!(located_main.is_some());
        assert_eq!(located_main.unwrap(), adl_bin_exe);

        let all_builds = layout.locate_binary_builds(None);
        assert!(all_builds.len() >= 2);
        assert!(
            all_builds
                .iter()
                .any(|b| b.name == "tool" && b.is_executable)
        );
        assert!(
            all_builds
                .iter()
                .any(|b| b.name == "main" && b.is_executable)
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
