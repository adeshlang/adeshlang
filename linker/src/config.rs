//! Linker configuration options, optimization levels, and CLI flags.

use crate::target::Target;
use std::path::PathBuf;

/// Identical Code Folding mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcfMode {
    None,
    Safe,
    All,
}

/// Build ID generation style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildIdStyle {
    None,
    Sha256,
    Fast,
    Uuid,
}

/// Link map output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapFormat {
    Text,
    Json,
}

/// Linker configuration.
#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub output_path: PathBuf,
    pub target: Target,
    pub entry_point: Option<String>,
    pub library_search_paths: Vec<PathBuf>,
    pub libraries: Vec<String>,
    pub gc_sections: bool,
    pub print_gc_sections: bool,
    pub icf: IcfMode,
    pub print_icf: bool,
    pub shared: bool,
    pub static_link: bool,
    pub strip: bool,
    pub strip_debug: bool,
    pub debug_info: bool,
    pub deterministic: bool,
    pub build_id: BuildIdStyle,
    pub map_file: Option<PathBuf>,
    pub map_format: MapFormat,
    pub exports: Vec<String>,
    pub export_all: bool,
    pub imports: Vec<String>,
    pub hardened: bool,
    pub report: bool,
    pub dependency_graph: bool,
    pub incremental: bool,
    pub cache_dir: Option<PathBuf>,
    pub verbose: bool,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            output_path: PathBuf::from("a.out"),
            target: Target::host(),
            entry_point: None,
            library_search_paths: Vec::new(),
            libraries: Vec::new(),
            gc_sections: true,
            print_gc_sections: false,
            icf: IcfMode::None,
            print_icf: false,
            shared: false,
            static_link: true,
            strip: false,
            strip_debug: false,
            debug_info: true,
            deterministic: true,
            build_id: BuildIdStyle::Sha256,
            map_file: None,
            map_format: MapFormat::Text,
            exports: Vec::new(),
            export_all: false,
            imports: Vec::new(),
            hardened: false,
            report: false,
            dependency_graph: false,
            incremental: false,
            cache_dir: None,
            verbose: false,
        }
    }
}

impl LinkConfig {
    pub fn new(output_path: PathBuf, target: Target) -> Self {
        Self {
            output_path,
            target,
            ..Default::default()
        }
    }

    pub fn effective_entry(&self) -> &str {
        if let Some(ref entry) = self.entry_point {
            entry.as_str()
        } else {
            &self.target.default_entry
        }
    }
}
