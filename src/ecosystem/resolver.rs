use crate::ecosystem::lockfile::{LockDependency, LockFile, PackageSource};
use crate::ecosystem::manifest::{Manifest, ManifestSection, ManifestValue};
use crate::ecosystem::project::ProjectLayout;
use crate::ecosystem::version::{SemVer, VersionReq};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DependencySpec {
    pub name: String,
    pub requirement: VersionReq,
    pub source: DependencySource,
    pub optional: bool,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DependencySource {
    Registry { registry: String },
    Git { url: String, rev: Option<String> },
    Path { path: PathBuf },
    Workspace { member: String },
    Archive { path: PathBuf },
    Local { path: PathBuf },
    Mirror { registry: String },
}

#[derive(Debug, Clone)]
pub struct DependencyResolution {
    pub package_id: String,
    pub name: String,
    pub version: SemVer,
    pub checksum: String,
    pub sha256: String,
    pub source: PackageSource,
    pub transitive: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub root: String,
    pub nodes: BTreeMap<String, DependencyResolution>,
    pub edges: BTreeMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }

    pub fn add_edge(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.edges.entry(from.into()).or_default().push(to.into());
    }

    pub fn add_node(&mut self, node: DependencyResolution) {
        self.nodes.insert(node.package_id.clone(), node);
    }

    pub fn tree_lines(&self) -> Vec<String> {
        let mut lines = vec![self.root.clone()];
        if let Some(children) = self.edges.get(&self.root) {
            for child in children {
                self.push_tree(child, 1, &mut lines);
            }
        }
        lines
    }

    fn push_tree(&self, package_id: &str, depth: usize, lines: &mut Vec<String>) {
        let indent = "  ".repeat(depth);
        if let Some(node) = self.nodes.get(package_id) {
            lines.push(format!(
                "{}{} {} [{}]",
                indent, node.name, node.version, node.source.kind
            ));
        } else {
            lines.push(format!("{}{}", indent, package_id));
        }
        if let Some(children) = self.edges.get(package_id) {
            for child in children {
                self.push_tree(child, depth + 1, lines);
            }
        }
    }

    pub fn to_graph_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "{} -> {}",
            self.root,
            self.edges
                .get(&self.root)
                .map(|items| items.join(", "))
                .unwrap_or_default()
        ));
        for (from, children) in &self.edges {
            if from == &self.root {
                continue;
            }
            lines.push(format!("{} -> {}", from, children.join(", ")));
        }
        lines
    }

    pub fn find_by_name(&self, name: &str) -> Vec<&DependencyResolution> {
        self.nodes
            .values()
            .filter(|node| node.name == name)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedManifest {
    pub lockfile: LockFile,
    pub graph: DependencyGraph,
}

#[derive(Debug, Default, Clone)]
pub struct DependencyResolver {
    pub registries: BTreeMap<String, PathBuf>,
    pub offline: bool,
}

impl DependencyResolver {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_available_versions(
        &self,
        project_root: &Path,
        spec: &DependencySpec,
    ) -> Result<Vec<SemVer>, String> {
        let mut versions = Vec::new();
        match &spec.source {
            DependencySource::Path { path }
            | DependencySource::Local { path }
            | DependencySource::Archive { path } => {
                if let Ok(v) = self.resolve_local(path, &spec.name) {
                    versions.push(v);
                }
            }
            DependencySource::Workspace { member } => {
                if let Ok(v) = self.resolve_workspace_member(project_root, member, &spec.name) {
                    versions.push(v);
                }
            }
            DependencySource::Registry { registry } | DependencySource::Mirror { registry } => {
                let registry_dir =
                    self.registries.get(registry).cloned().unwrap_or_else(|| {
                        project_root.join(".adl").join("registry").join(registry)
                    });
                let package_dir = registry_dir.join(&spec.name);
                if package_dir.exists() {
                    for entry in fs::read_dir(&package_dir).map_err(|error| error.to_string())? {
                        let entry = entry.map_err(|error| error.to_string())?;
                        if entry
                            .file_type()
                            .map_err(|error| error.to_string())?
                            .is_dir()
                        {
                            if let Ok(version) = SemVer::parse(&entry.file_name().to_string_lossy())
                            {
                                versions.push(version);
                            }
                        }
                    }
                }
            }
            DependencySource::Git { url, rev } => {
                if let Ok((v, _)) = self.resolve_git(project_root, url, rev, &spec.name) {
                    versions.push(v);
                }
            }
        }
        versions.sort();
        Ok(versions)
    }

    pub fn resolve_manifest(
        &self,
        manifest: &Manifest,
        project_root: &Path,
    ) -> Result<ResolvedManifest, String> {
        let root_id = manifest
            .project_name()
            .unwrap_or_else(|| "workspace-root".to_string());
        let mut lockfile = LockFile::new(root_id.clone());
        let mut graph_nodes = BTreeMap::new();
        let mut graph_edges: BTreeMap<String, Vec<String>> = BTreeMap::new();

        // Queue of (parent_id, spec, relative_project_root, path_history)
        let mut queue = Vec::new();

        // Track resolved versions and requirements
        let mut package_requirements: BTreeMap<String, Vec<VersionReq>> = BTreeMap::new();
        let mut resolved_versions: BTreeMap<String, SemVer> = BTreeMap::new();

        // 1. Check if it's a workspace
        let layout = ProjectLayout::new(project_root);
        let member_paths = layout.workspace_member_paths(manifest)?;

        if !member_paths.is_empty() {
            // Workspace mode
            for path in member_paths {
                let member_manifest = Manifest::load(&path.join("adesh.adl"))?;
                let member_name = member_manifest
                    .project_name()
                    .ok_or_else(|| "Workspace member missing project name".to_string())?;

                // Add the member as a direct edge of the workspace root
                let member_version = member_manifest.project_version().unwrap_or_else(|| SemVer {
                    major: 0,
                    minor: 1,
                    patch: 0,
                    pre_release: Vec::new(),
                    build_metadata: Vec::new(),
                });
                let member_id = format!("{}@{}", member_name, member_version);

                // Add to graph nodes as a workspace dependency
                graph_nodes.insert(
                    member_id.clone(),
                    DependencyResolution {
                        package_id: member_id.clone(),
                        name: member_name.clone(),
                        version: member_version.clone(),
                        checksum: synthetic_checksum(&member_id),
                        sha256: synthetic_checksum(&member_id),
                        source: PackageSource {
                            kind: "workspace".to_string(),
                            location: path
                                .strip_prefix(project_root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string(),
                        },
                        transitive: Vec::new(),
                    },
                );
                graph_edges
                    .entry(root_id.clone())
                    .or_default()
                    .push(member_id.clone());
                resolved_versions.insert(member_name.clone(), member_version);

                if let Some(section) = member_manifest.dependencies() {
                    for spec in self.extract_specs(section, &path)? {
                        queue.push((
                            member_id.clone(),
                            spec,
                            path.clone(),
                            vec![root_id.clone(), member_id.clone()],
                        ));
                    }
                }
            }
        } else {
            // Non-workspace mode
            if let Some(section) = manifest.dependencies() {
                for spec in self.extract_specs(section, project_root)? {
                    queue.push((
                        root_id.clone(),
                        spec,
                        project_root.to_path_buf(),
                        vec![root_id.clone()],
                    ));
                }
            }
        }

        // 2. Process queue (transitive resolution)
        let mut visited = std::collections::HashSet::new();
        while let Some((parent_id, mut spec, relative_root, path_history)) = queue.pop() {
            if spec.optional && self.offline {
                continue;
            }

            // Cycle detection check
            if path_history.contains(&spec.name) {
                return Err(format!(
                    "dependency cycle detected: {} -> {}",
                    path_history.join(" -> "),
                    spec.name
                ));
            }

            // Update requirements list
            package_requirements
                .entry(spec.name.clone())
                .or_default()
                .push(spec.requirement.clone());

            // Version unification and conflict resolution check
            if let Some(resolved_v) = resolved_versions.get(&spec.name) {
                if spec.requirement.matches(resolved_v) {
                    // Re-use matching version
                    let package_id = format!("{}@{}", spec.name, resolved_v);
                    graph_edges
                        .entry(parent_id.clone())
                        .or_default()
                        .push(package_id.clone());
                    continue;
                } else {
                    // Try to find a unified version satisfying all requirements
                    let reqs = package_requirements.get(&spec.name).unwrap();
                    let available = self.get_available_versions(project_root, &spec)?;
                    let mut unified_version = None;
                    for v in available.into_iter().rev() {
                        if reqs.iter().all(|req| req.matches(&v)) {
                            unified_version = Some(v);
                            break;
                        }
                    }
                    if let Some(v) = unified_version {
                        resolved_versions.insert(spec.name.clone(), v.clone());
                        spec.requirement = VersionReq::Exact(v);
                    } else {
                        return Err(format!(
                            "dependency conflict for package '{}': no version satisfies requirements: {}",
                            spec.name,
                            reqs.iter()
                                .map(|r| r.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            } else {
                // First time resolving: pick the latest satisfying version
                let available = self.get_available_versions(project_root, &spec)?;
                let mut chosen_version = None;
                for v in available.into_iter().rev() {
                    if spec.requirement.matches(&v) {
                        chosen_version = Some(v);
                        break;
                    }
                }
                if let Some(v) = chosen_version {
                    resolved_versions.insert(spec.name.clone(), v.clone());
                    spec.requirement = VersionReq::Exact(v);
                }
            }

            let (resolution, local_path) = self.resolve_spec(&spec, &relative_root)?;
            let package_id = resolution.package_id.clone();
            resolved_versions.insert(resolution.name.clone(), resolution.version.clone());

            // Add edge from parent to this dependency
            graph_edges
                .entry(parent_id.clone())
                .or_default()
                .push(package_id.clone());

            // If already resolved, we skip processing its transitives again
            if visited.contains(&package_id) {
                continue;
            }
            visited.insert(package_id.clone());

            // Add node to graph and lockfile
            graph_nodes.insert(package_id.clone(), resolution.clone());
            lockfile.add_dependency(LockDependency {
                package_id: resolution.package_id.clone(),
                name: resolution.name.clone(),
                version: resolution.version.clone(),
                requirement: spec.requirement.clone(),
                checksum: resolution.checksum.clone(),
                sha256: resolution.sha256.clone(),
                source: resolution.source.clone(),
                features: spec.features.clone(),
                registries: self.registries.keys().cloned().collect(),
                transitive: resolution.transitive.clone(),
                target: spec.target.clone(),
                profile: spec.profile.clone(),
            });

            // Read transitive dependencies
            let dep_manifest_path = if local_path.is_dir() {
                local_path.join("adesh.adl")
            } else {
                local_path.clone()
            };

            if dep_manifest_path.exists() {
                if let Ok(dep_manifest) = Manifest::load(&dep_manifest_path) {
                    if let Some(section) = dep_manifest.dependencies() {
                        let dep_root = if local_path.is_dir() {
                            local_path.clone()
                        } else {
                            local_path.parent().unwrap().to_path_buf()
                        };
                        for dep_spec in self.extract_specs(section, &dep_root)? {
                            let mut next_history = path_history.clone();
                            next_history.push(spec.name.clone());
                            queue.push((
                                package_id.clone(),
                                dep_spec,
                                dep_root.clone(),
                                next_history,
                            ));
                        }
                    }
                }
            }
        }

        // Populate transitive list on lock dependencies & resolution nodes
        for (parent, children) in &graph_edges {
            if let Some(node) = graph_nodes.get_mut(parent) {
                node.transitive = children.clone();
            }
            if let Some(lock_dep) = lockfile
                .dependencies
                .iter_mut()
                .find(|d| &d.package_id == parent)
            {
                lock_dep.transitive = children.clone();
            }
        }

        let graph = DependencyGraph {
            root: root_id,
            nodes: graph_nodes,
            edges: graph_edges,
        };
        Ok(ResolvedManifest { lockfile, graph })
    }

    pub fn extract_specs(
        &self,
        section: &ManifestSection,
        project_root: &Path,
    ) -> Result<Vec<DependencySpec>, String> {
        let mut specs = Vec::new();
        for item in &section.items {
            if let crate::ecosystem::manifest::ManifestItem::Field { key, value } = item {
                specs.push(self.parse_dependency_spec(key, value, project_root)?);
            }
        }
        Ok(specs)
    }

    fn parse_dependency_spec(
        &self,
        name: &str,
        value: &ManifestValue,
        project_root: &Path,
    ) -> Result<DependencySpec, String> {
        let mut requirement = VersionReq::Any;
        let mut source = DependencySource::Registry {
            registry: "official".to_string(),
        };
        let mut optional = false;
        let mut features = Vec::new();
        let mut target = None;
        let mut profile = None;

        match value {
            ManifestValue::Version(req) => {
                requirement = req.clone();
            }
            ManifestValue::SemanticVersion(version) => {
                requirement = VersionReq::Exact(version.clone());
            }
            ManifestValue::String(value) | ManifestValue::Identifier(value) => {
                requirement = VersionReq::parse(value)?;
            }
            ManifestValue::Object(fields) => {
                if let Some(ManifestValue::String(version)) = fields.get("version") {
                    requirement = VersionReq::parse(version)?;
                }
                if let Some(ManifestValue::SemanticVersion(version)) = fields.get("version") {
                    requirement = VersionReq::Exact(version.clone());
                }
                if let Some(ManifestValue::String(path)) = fields.get("path") {
                    source = DependencySource::Path {
                        path: project_root.join(path),
                    };
                }
                if let Some(ManifestValue::String(path)) = fields.get("local") {
                    source = DependencySource::Local {
                        path: project_root.join(path),
                    };
                }
                if let Some(ManifestValue::String(member)) = fields.get("workspace") {
                    source = DependencySource::Workspace {
                        member: member.clone(),
                    };
                }
                if let Some(ManifestValue::String(url)) = fields.get("git") {
                    let rev = fields.get("rev").and_then(|value| match value {
                        ManifestValue::String(rev) | ManifestValue::Identifier(rev) => {
                            Some(rev.clone())
                        }
                        _ => None,
                    });
                    source = DependencySource::Git {
                        url: url.clone(),
                        rev,
                    };
                }
                if let Some(ManifestValue::String(registry)) = fields.get("registry") {
                    source = DependencySource::Registry {
                        registry: registry.clone(),
                    };
                }
                if let Some(ManifestValue::String(registry)) = fields.get("mirror") {
                    source = DependencySource::Mirror {
                        registry: registry.clone(),
                    };
                }
                if let Some(ManifestValue::Bool(value)) = fields.get("optional") {
                    optional = *value;
                }
                if let Some(ManifestValue::Array(values)) = fields.get("features") {
                    for value in values {
                        if let ManifestValue::String(feature) | ManifestValue::Identifier(feature) =
                            value
                        {
                            features.push(feature.clone());
                        }
                    }
                }
                if let Some(ManifestValue::String(value)) = fields.get("target") {
                    target = Some(value.clone());
                }
                if let Some(ManifestValue::String(value)) = fields.get("profile") {
                    profile = Some(value.clone());
                }
            }
            _ => {}
        }

        Ok(DependencySpec {
            name: name.to_string(),
            requirement,
            source,
            optional,
            features,
            target,
            profile,
        })
    }

    fn resolve_git(
        &self,
        project_root: &Path,
        url: &str,
        rev: &Option<String>,
        name: &str,
    ) -> Result<(SemVer, PathBuf), String> {
        let hash = synthetic_checksum(url);
        let git_dir = project_root.join(".adl").join("git");
        let clone_path = git_dir.join(&hash);

        if self.offline {
            if clone_path.exists() {
                let manifest = Manifest::load(clone_path.join("adesh.adl"))?;
                let version = manifest.project_version().ok_or_else(|| {
                    format!("git dependency {name} is missing a semantic version")
                })?;
                return Ok((version, clone_path));
            }
            return Err(format!(
                "Offline mode: git dependency '{url}' is not cached locally."
            ));
        }

        fs::create_dir_all(&git_dir).map_err(|e| e.to_string())?;

        use std::process::Command;
        if clone_path.exists() {
            let _ = Command::new("git")
                .arg("fetch")
                .arg("--all")
                .current_dir(&clone_path)
                .status();
        } else {
            let status = Command::new("git")
                .arg("clone")
                .arg(url)
                .arg(&clone_path)
                .status()
                .map_err(|e| format!("failed to execute git clone: {e}"))?;
            if !status.success() {
                return Err(format!("git clone failed for {url}"));
            }
        }

        if let Some(revision) = rev {
            let status = Command::new("git")
                .arg("checkout")
                .arg(revision)
                .current_dir(&clone_path)
                .status()
                .map_err(|e| format!("failed to execute git checkout: {e}"))?;
            if !status.success() {
                return Err(format!("git checkout failed for {revision} in {url}"));
            }
        } else {
            let _ = Command::new("git")
                .arg("checkout")
                .arg("main")
                .current_dir(&clone_path)
                .status();
        }

        let manifest_path = clone_path.join("adesh.adl");
        if !manifest_path.exists() {
            return Err(format!(
                "git dependency at {url} does not contain an adesh.adl file"
            ));
        }

        let manifest = Manifest::load(manifest_path)?;
        let version = manifest
            .project_version()
            .ok_or_else(|| format!("git dependency {name} is missing a semantic version"))?;
        Ok((version, clone_path))
    }

    pub fn resolve_spec(
        &self,
        spec: &DependencySpec,
        project_root: &Path,
    ) -> Result<(DependencyResolution, PathBuf), String> {
        let (version, path) = match &spec.source {
            DependencySource::Path { path }
            | DependencySource::Local { path }
            | DependencySource::Archive { path } => {
                let v = self.resolve_local(path, &spec.name)?;
                (v, path.clone())
            }
            DependencySource::Workspace { member } => {
                let v = self.resolve_workspace_member(project_root, member, &spec.name)?;
                (v, project_root.join(member))
            }
            DependencySource::Registry { registry } | DependencySource::Mirror { registry } => {
                let v =
                    self.resolve_registry(project_root, registry, &spec.name, &spec.requirement)?;
                let p = self
                    .registries
                    .get(registry)
                    .cloned()
                    .unwrap_or_else(|| project_root.join(".adl").join("registry").join(registry))
                    .join(&spec.name)
                    .join(v.to_string());
                (v, p)
            }
            DependencySource::Git { url, rev } => {
                self.resolve_git(project_root, url, rev, &spec.name)?
            }
        };
        let package_id = format!("{}@{}", spec.name, version);
        let checksum = synthetic_checksum(&package_id);
        let res = DependencyResolution {
            package_id,
            name: spec.name.clone(),
            version,
            checksum: checksum.clone(),
            sha256: checksum,
            source: source_from_spec(spec),
            transitive: Vec::new(),
        };
        Ok((res, path))
    }

    fn resolve_registry(
        &self,
        project_root: &Path,
        registry: &str,
        name: &str,
        requirement: &VersionReq,
    ) -> Result<SemVer, String> {
        let registry_dir = self
            .registries
            .get(registry)
            .cloned()
            .unwrap_or_else(|| project_root.join(".adl").join("registry").join(registry));
        let package_dir = registry_dir.join(name);
        if package_dir.exists() {
            let mut versions = Vec::new();
            for entry in fs::read_dir(&package_dir).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_dir()
                {
                    if let Ok(version) = SemVer::parse(&entry.file_name().to_string_lossy()) {
                        if requirement.matches(&version) {
                            versions.push(version);
                        }
                    }
                }
            }
            versions.sort();
            if let Some(version) = versions.pop() {
                return Ok(version);
            }
        }
        self.synthetic_version(&format!("registry:{}:{}", registry, name))
    }

    fn resolve_local(&self, path: &Path, name: &str) -> Result<SemVer, String> {
        let manifest = if path.is_dir() {
            Manifest::load(path.join("adesh.adl")).or_else(|_| Manifest::load(path))?
        } else {
            Manifest::load(path)?
        };
        manifest
            .project_version()
            .ok_or_else(|| format!("local dependency {name} is missing a semantic version"))
    }

    fn resolve_workspace_member(
        &self,
        project_root: &Path,
        member: &str,
        name: &str,
    ) -> Result<SemVer, String> {
        let manifest = Manifest::load(project_root.join(member).join("adesh.adl"))?;
        manifest
            .project_version()
            .ok_or_else(|| format!("workspace member {name} is missing a semantic version"))
    }

    fn synthetic_version(&self, seed: &str) -> Result<SemVer, String> {
        let mut hash = 0u64;
        for byte in seed.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        Ok(SemVer {
            major: 0,
            minor: hash % 100,
            patch: hash % 1000,
            pre_release: Vec::new(),
            build_metadata: Vec::new(),
        })
    }
}

fn source_from_spec(spec: &DependencySpec) -> PackageSource {
    match &spec.source {
        DependencySource::Registry { registry } => PackageSource {
            kind: "registry".to_string(),
            location: registry.clone(),
        },
        DependencySource::Git { url, .. } => PackageSource {
            kind: "git".to_string(),
            location: url.clone(),
        },
        DependencySource::Path { path } => PackageSource {
            kind: "path".to_string(),
            location: path.to_string_lossy().to_string(),
        },
        DependencySource::Workspace { member } => PackageSource {
            kind: "workspace".to_string(),
            location: member.clone(),
        },
        DependencySource::Archive { path } => PackageSource {
            kind: "archive".to_string(),
            location: path.to_string_lossy().to_string(),
        },
        DependencySource::Local { path } => PackageSource {
            kind: "local".to_string(),
            location: path.to_string_lossy().to_string(),
        },
        DependencySource::Mirror { registry } => PackageSource {
            kind: "mirror".to_string(),
            location: registry.clone(),
        },
    }
}

pub fn synthetic_checksum(input: &str) -> String {
    let mut hash = 0u64;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(131).wrapping_add(byte as u64);
    }
    format!("{:016x}", hash)
}
