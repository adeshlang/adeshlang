use std::fmt::{self, Display};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre_release: Vec<String>,
    pub build_metadata: Vec<String>,
}

impl SemVer {
    pub fn parse(input: &str) -> Result<Self, String> {
        let (core_and_pre, build_metadata) = match input.split_once('+') {
            Some((core, build)) => (core, build.split('.').map(|s| s.to_string()).collect()),
            None => (input, Vec::new()),
        };
        let (core, pre_release) = match core_and_pre.split_once('-') {
            Some((core, pre)) => (core, pre.split('.').map(|s| s.to_string()).collect()),
            None => (core_and_pre, Vec::new()),
        };

        let mut parts = core.split('.');
        let major = parts
            .next()
            .ok_or_else(|| format!("invalid semantic version: {input}"))?
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version: {input}"))?;
        let minor = parts
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version: {input}"))?;
        let patch = parts
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .map_err(|_| format!("invalid semantic version: {input}"))?;

        Ok(Self {
            major,
            minor,
            patch,
            pre_release,
            build_metadata,
        })
    }
}

impl Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre_release.is_empty() {
            write!(f, "-{}", self.pre_release.join("."))?;
        }
        if !self.build_metadata.is_empty() {
            write!(f, "+{}", self.build_metadata.join("."))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReq {
    Any,
    Exact(SemVer),
    Caret(SemVer),
    Tilde(SemVer),
    GreaterThan(SemVer),
    GreaterOrEqual(SemVer),
    LessThan(SemVer),
    LessOrEqual(SemVer),
    Range {
        min: Option<SemVer>,
        max: Option<SemVer>,
    },
}

impl VersionReq {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed == "*" {
            return Ok(Self::Any);
        }

        if let Some((left, right)) = trimmed.split_once(',') {
            let mut min = None;
            let mut max = None;
            for part in [left.trim(), right.trim()] {
                if let Some(value) = part.strip_prefix(">=") {
                    min = Some(SemVer::parse(value.trim())?);
                } else if let Some(value) = part.strip_prefix(">") {
                    min = Some(SemVer::parse(value.trim())?);
                } else if let Some(value) = part.strip_prefix("<=") {
                    max = Some(SemVer::parse(value.trim())?);
                } else if let Some(value) = part.strip_prefix("<") {
                    max = Some(SemVer::parse(value.trim())?);
                }
            }
            return Ok(Self::Range { min, max });
        }

        if let Some(value) = trimmed.strip_prefix('^') {
            return Ok(Self::Caret(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix('~') {
            return Ok(Self::Tilde(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix(">=") {
            return Ok(Self::GreaterOrEqual(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix("<=") {
            return Ok(Self::LessOrEqual(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix('>') {
            return Ok(Self::GreaterThan(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix('<') {
            return Ok(Self::LessThan(SemVer::parse(value.trim())?));
        }
        if let Some(value) = trimmed.strip_prefix('=') {
            return Ok(Self::Exact(SemVer::parse(value.trim())?));
        }
        Ok(Self::Exact(SemVer::parse(trimmed)?))
    }

    pub fn matches(&self, version: &SemVer) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => version == expected,
            Self::Caret(base) => caret_matches(base, version),
            Self::Tilde(base) => tilde_matches(base, version),
            Self::GreaterThan(base) => version > base,
            Self::GreaterOrEqual(base) => version >= base,
            Self::LessThan(base) => version < base,
            Self::LessOrEqual(base) => version <= base,
            Self::Range { min, max } => {
                let min_ok = min.as_ref().map(|value| version >= value).unwrap_or(true);
                let max_ok = max.as_ref().map(|value| version <= value).unwrap_or(true);
                min_ok && max_ok
            }
        }
    }
}

impl Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "*"),
            Self::Exact(version) => write!(f, "={version}"),
            Self::Caret(version) => write!(f, "^{version}"),
            Self::Tilde(version) => write!(f, "~{version}"),
            Self::GreaterThan(version) => write!(f, ">{version}"),
            Self::GreaterOrEqual(version) => write!(f, ">={version}"),
            Self::LessThan(version) => write!(f, "<{version}"),
            Self::LessOrEqual(version) => write!(f, "<={version}"),
            Self::Range { min, max } => {
                let mut parts = Vec::new();
                if let Some(value) = min {
                    parts.push(format!(">={value}"));
                }
                if let Some(value) = max {
                    parts.push(format!("<={value}"));
                }
                write!(f, "{}", parts.join(", "))
            }
        }
    }
}

fn caret_matches(base: &SemVer, version: &SemVer) -> bool {
    if version < base {
        return false;
    }
    if base.major > 0 {
        version.major == base.major
    } else if base.minor > 0 {
        version.major == 0 && version.minor == base.minor
    } else {
        version.major == 0 && version.minor == 0 && version.patch == base.patch
    }
}

fn tilde_matches(base: &SemVer, version: &SemVer) -> bool {
    version >= base && version.major == base.major && version.minor == base.minor
}
