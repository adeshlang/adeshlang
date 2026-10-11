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

/// Requested IR link-time optimization mode. Native object inputs cannot be
/// optimized across modules yet; the linker rejects these modes explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LtoMode {
    Off,
    Thin,
    Full,
}

/// Linker optimization level. These govern format-neutral link-time passes,
/// independently of code generator optimization settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
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
    pub opt_level: OptLevel,
    pub lto: LtoMode,
    pub print_gc_sections: bool,
    pub icf: IcfMode,
    pub print_icf: bool,
    pub shared: bool,
    pub static_link: bool,
    /// Produce a dynamic ELF executable: PT_INTERP + dynamic symbol/relocation
    /// tables, unresolved libc/POSIX symbols imported from the target C
    /// library via PLT/GOT (`R_X86_64_JUMP_SLOT`), bound eagerly
    /// (`DT_BIND_NOW`). When false (default), a symbol that would have to be
    /// imported is a loud link error instead of a silently broken stub.
    pub dynamic: bool,
    /// Produce a position-independent ELF executable (`ET_DYN` with
    /// `DF_1_PIE` and `R_X86_64_RELATIVE` dynamic relocations). Implies
    /// `dynamic` layout treatment.
    pub pie: bool,
    pub strip: bool,
    /// Strip non-global (local/file/section) symbols from the emitted symbol
    /// table. Enabled by default for executables: local symbols are not needed
    /// at run time and dominate symbol-table size. `--no-strip` disables it.
    pub strip_symbols: bool,
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
            opt_level: OptLevel::O2,
            lto: LtoMode::Off,
            print_gc_sections: false,
            // The default optimization policy is `-O2`, and the portable
            // `-O2` policy implies safe ICF (see `apply_optimization_level`).
            // Leaving this at `None` made the documented default diverge from
            // the actual default configuration.
            icf: IcfMode::Safe,
            print_icf: false,
            shared: false,
            static_link: true,
            dynamic: false,
            pie: false,
            strip: false,
            strip_symbols: true,
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

    /// Whether this link must emit ELF dynamic-linking metadata (dynamic
    /// symbol table, relocations, `.dynamic`). PIE executables, dynamic
    /// executables, shared libraries, and any link with required shared
    /// libraries all need it.
    pub fn needs_elf_dynamic_tables(&self) -> bool {
        self.target.format == crate::target::ObjectFormat::Elf
            && (self.shared || self.pie || self.dynamic)
    }

    /// Whether non-global symbols should be omitted from the emitted symbol
    /// table. `-s/--strip-all` implies it; `--no-strip` opts out.
    pub fn should_strip_symbols(&self) -> bool {
        self.strip || self.strip_symbols
    }

    /// Apply the portable optimization policy for a requested level. Explicit
    /// CLI/API settings applied afterwards can still override these defaults.
    pub fn apply_optimization_level(&mut self, level: OptLevel) {
        self.opt_level = level;
        match level {
            OptLevel::O0 => {
                self.gc_sections = false;
                self.icf = IcfMode::None;
                // An unoptimized link keeps local symbols so a debugger can
                // still resolve them.
                self.strip_symbols = false;
            }
            OptLevel::O1 => {
                self.gc_sections = true;
                self.icf = IcfMode::Safe;
            }
            OptLevel::O2 => {
                self.gc_sections = true;
                self.icf = IcfMode::Safe;
            }
            OptLevel::O3 => {
                self.gc_sections = true;
                self.icf = IcfMode::All;
            }
            OptLevel::Os => {
                self.gc_sections = true;
                self.icf = IcfMode::Safe;
                self.strip_debug = true;
            }
            OptLevel::Oz => {
                self.gc_sections = true;
                self.icf = IcfMode::All;
                self.strip_debug = true;
                self.strip = true;
            }
        }
    }
}
