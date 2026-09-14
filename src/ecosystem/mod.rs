//! Adesh ecosystem support.

pub mod cli;
pub mod lockfile;
pub mod manifest;
pub mod project;
pub mod resolver;
pub mod tasks;
pub mod version;

pub use cli::{AdlCommand, CliOptions, run_from_env};
pub use lockfile::{LockDependency, LockFile, PackageSource};
pub use manifest::{BindingKind, Manifest, ManifestItem, ManifestSection, ManifestValue};
pub use project::{ProjectLayout, ProjectTemplate};
pub use resolver::{DependencyResolution, DependencyResolver, DependencySource, DependencySpec};
pub use tasks::{TaskDefinition, TaskRunner};
pub use version::{SemVer, VersionReq};
