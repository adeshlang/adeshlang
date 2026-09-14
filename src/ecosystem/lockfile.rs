use crate::ecosystem::version::{SemVer, VersionReq};
use chrono::{SecondsFormat, Utc};
use std::collections::BTreeMap;
use std::fmt::{self, Display};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSource {
    pub kind: String,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockDependency {
    pub package_id: String,
    pub name: String,
    pub version: SemVer,
    pub requirement: VersionReq,
    pub checksum: String,
    pub sha256: String,
    pub source: PackageSource,
    pub features: Vec<String>,
    pub registries: Vec<String>,
    pub transitive: Vec<String>,
    pub target: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFile {
    pub lock_version: String,
    pub generated_at: String,
    pub compiler_version: String,
    pub adl_version: String,
    pub package_id: String,
    pub dependencies: Vec<LockDependency>,
    pub metadata: BTreeMap<String, String>,
    pub confidential: BTreeMap<String, String>,
    pub signature: Option<String>,
}

impl LockFile {
    pub fn new(package_id: impl Into<String>) -> Self {
        Self {
            lock_version: "1".to_string(),
            generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            adl_version: env!("CARGO_PKG_VERSION").to_string(),
            package_id: package_id.into(),
            dependencies: Vec::new(),
            metadata: BTreeMap::new(),
            confidential: BTreeMap::new(),
            signature: None,
        }
    }

    pub fn add_dependency(&mut self, dependency: LockDependency) {
        self.dependencies.push(dependency);
        self.dependencies
            .sort_by(|left, right| left.package_id.cmp(&right.package_id));
    }

    pub fn set_confidential(&mut self, confidential: BTreeMap<String, String>) {
        self.confidential = confidential;
    }

    pub fn compute_signature(&self) -> String {
        let mut text = String::new();
        text.push_str(&self.lock_version);
        text.push_str(&self.generated_at);
        text.push_str(&self.compiler_version);
        text.push_str(&self.adl_version);
        text.push_str(&self.package_id);
        for dep in &self.dependencies {
            text.push_str(&dep.package_id);
            text.push_str(&dep.name);
            text.push_str(&dep.version.to_string());
            text.push_str(&dep.checksum);
        }
        let mut hash = 0u64;
        for byte in text.bytes() {
            hash = hash.wrapping_mul(16777619) ^ (byte as u64);
        }
        format!("{:016x}", hash)
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        let mut lock = Self::new("unknown");
        let mut current_dep: Option<LockDependency> = None;
        let mut in_dependencies = false;
        let mut in_confidential = false;

        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "lock {" {
                continue;
            }
            if trimmed == "}" {
                if in_confidential {
                    in_confidential = false;
                    continue;
                }
                if !in_dependencies {
                    continue;
                }
            }

            if trimmed == "dependencies = [" {
                in_dependencies = true;
                continue;
            }
            if trimmed == "]" {
                in_dependencies = false;
                continue;
            }
            if trimmed == "confidential = {" {
                in_confidential = true;
                continue;
            }

            if in_dependencies {
                if trimmed == "{" {
                    current_dep = Some(LockDependency {
                        package_id: String::new(),
                        name: String::new(),
                        version: SemVer {
                            major: 0,
                            minor: 0,
                            patch: 0,
                            pre_release: Vec::new(),
                            build_metadata: Vec::new(),
                        },
                        requirement: VersionReq::Any,
                        checksum: String::new(),
                        sha256: String::new(),
                        source: PackageSource {
                            kind: String::new(),
                            location: String::new(),
                        },
                        features: Vec::new(),
                        registries: Vec::new(),
                        transitive: Vec::new(),
                        target: None,
                        profile: None,
                    });
                } else if trimmed == "}" {
                    if let Some(dep) = current_dep.take() {
                        lock.add_dependency(dep);
                    }
                } else if let Some(ref mut dep) = current_dep {
                    if let Some((key, val)) = trimmed.split_once('=') {
                        let key = key.trim();
                        let val = val.trim().trim_matches('"');
                        match key {
                            "package-id" => dep.package_id = val.to_string(),
                            "name" => dep.name = val.to_string(),
                            "version" => dep.version = SemVer::parse(val)?,
                            "requirement" => dep.requirement = VersionReq::parse(val)?,
                            "checksum" => dep.checksum = val.to_string(),
                            "sha256" => dep.sha256 = val.to_string(),
                            "target" => dep.target = Some(val.to_string()),
                            "profile" => dep.profile = Some(val.to_string()),
                            "source" => {
                                let cleaned = val.trim_matches('{').trim_matches('}');
                                for part in cleaned.split(',') {
                                    if let Some((k, v)) = part.split_once('=') {
                                        let k = k.trim();
                                        let v = v.trim().trim_matches('"');
                                        if k == "kind" {
                                            dep.source.kind = v.to_string();
                                        } else if k == "location" {
                                            dep.source.location = v.to_string();
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }

            if in_confidential {
                if let Some((key, val)) = trimmed.split_once('=') {
                    lock.confidential.insert(
                        key.trim().to_string(),
                        val.trim().trim_matches('"').to_string(),
                    );
                }
                continue;
            }

            if let Some((key, val)) = trimmed.split_once('=') {
                let key = key.trim();
                let val = val.trim().trim_matches('"');
                match key {
                    "version" => lock.lock_version = val.to_string(),
                    "generated-at" => lock.generated_at = val.to_string(),
                    "compiler-version" => lock.compiler_version = val.to_string(),
                    "adl-version" => lock.adl_version = val.to_string(),
                    "package-id" => lock.package_id = val.to_string(),
                    "signature" => lock.signature = Some(val.to_string()),
                    _ => {
                        lock.metadata.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }

        Ok(lock)
    }

    pub fn verify_signature(&self) -> Result<(), String> {
        if let Some(ref sig) = self.signature {
            let computed = self.compute_signature();
            if sig != &computed {
                return Err(format!(
                    "lockfile integrity check failed: signature mismatch. expected: {}, computed: {}. lock_version: {}, generated_at: {}, compiler_version: {}, adl_version: {}, package_id: {}, dependencies count: {}",
                    sig,
                    computed,
                    self.lock_version,
                    self.generated_at,
                    self.compiler_version,
                    self.adl_version,
                    self.package_id,
                    self.dependencies.len()
                ));
            }
        }
        Ok(())
    }
}

impl Display for LockFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "lock {{")?;
        writeln!(f, "  version = \"{}\"", self.lock_version)?;
        writeln!(f, "  generated-at = \"{}\"", self.generated_at)?;
        writeln!(f, "  compiler-version = \"{}\"", self.compiler_version)?;
        writeln!(f, "  adl-version = \"{}\"", self.adl_version)?;
        writeln!(f, "  package-id = \"{}\"", self.package_id)?;
        writeln!(f, "  dependencies = [")?;
        for dependency in &self.dependencies {
            writeln!(f, "    {{")?;
            writeln!(f, "      package-id = \"{}\"", dependency.package_id)?;
            writeln!(f, "      name = \"{}\"", dependency.name)?;
            writeln!(f, "      version = \"{}\"", dependency.version)?;
            writeln!(f, "      requirement = \"{}\"", dependency.requirement)?;
            writeln!(f, "      checksum = \"{}\"", dependency.checksum)?;
            writeln!(f, "      sha256 = \"{}\"", dependency.sha256)?;
            writeln!(
                f,
                "      source = {{ kind = \"{}\", location = \"{}\" }}",
                dependency.source.kind, dependency.source.location
            )?;
            if !dependency.features.is_empty() {
                writeln!(
                    f,
                    "      features = [{}]",
                    dependency
                        .features
                        .iter()
                        .map(|feature| format!("\"{}\"", feature))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }
            if !dependency.registries.is_empty() {
                writeln!(
                    f,
                    "      registries = [{}]",
                    dependency
                        .registries
                        .iter()
                        .map(|registry| format!("\"{}\"", registry))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }
            if !dependency.transitive.is_empty() {
                writeln!(
                    f,
                    "      transitive = [{}]",
                    dependency
                        .transitive
                        .iter()
                        .map(|item| format!("\"{}\"", item))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }
            if let Some(target) = &dependency.target {
                writeln!(f, "      target = \"{}\"", target)?;
            }
            if let Some(profile) = &dependency.profile {
                writeln!(f, "      profile = \"{}\"", profile)?;
            }
            writeln!(f, "    }}")?;
        }
        writeln!(f, "  ]")?;
        for (key, value) in &self.metadata {
            writeln!(f, "  {} = \"{}\"", key, value)?;
        }
        if !self.confidential.is_empty() {
            writeln!(f, "  confidential = {{")?;
            for (key, value) in &self.confidential {
                writeln!(f, "    {} = \"{}\"", key, value)?;
            }
            writeln!(f, "  }}")?;
        }
        let signature = self
            .signature
            .clone()
            .unwrap_or_else(|| self.compute_signature());
        writeln!(f, "  signature = \"{}\"", signature)?;
        writeln!(f, "}}")
    }
}
