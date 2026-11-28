//! Error types and structured error codes for the Adesh Linker.

use std::fmt;

/// Standard Result type for linking operations.
pub type LinkResult<T> = Result<T, LinkError>;

/// Structured diagnostic error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// LNK001: Undefined symbol referenced by an input object.
    UndefinedSymbol,
    /// LNK002: Duplicate strong symbol definition found across objects.
    DuplicateSymbol,
    /// LNK003: Architecture mismatch between linked object files and target.
    ArchitectureMismatch,
    /// LNK004: Binary format mismatch between object files and target.
    FormatMismatch,
    /// LNK005: Relocation calculation overflowed target bit width or range.
    RelocationOverflow,
    /// LNK006: Object file or binary payload is invalid or malformed.
    InvalidObject,
    /// LNK007: Incompatible ABI or calling convention between objects.
    AbiMismatch,
    /// LNK008: Relocation type not supported for the active architecture.
    UnsupportedRelocation,
    /// LNK009: Section configuration, alignment, or layout is invalid.
    InvalidSection,
    /// LNK010: Specified target triple is invalid or unsupported.
    InvalidTarget,
    /// LNK011: Program entry point symbol could not be found.
    EntryPointNotFound,
    /// LNK012: Static archive member or symbol table is corrupted.
    InvalidArchive,
    /// LNK013: Layout or memory permission security violation (e.g. W^X).
    SecurityViolation,
    /// LNK014: Adesh binary metadata (.adesh.meta) is incompatible.
    MetadataMismatch,
    /// LNK015: File I/O or output emission failure.
    IoError,
    /// LNK016: Memory region or target address space exhausted.
    MemoryExhausted,
    /// LNK017: Output format is not implemented by this linker.
    UnsupportedFormat,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::UndefinedSymbol => "LNK001",
            ErrorCode::DuplicateSymbol => "LNK002",
            ErrorCode::ArchitectureMismatch => "LNK003",
            ErrorCode::FormatMismatch => "LNK004",
            ErrorCode::RelocationOverflow => "LNK005",
            ErrorCode::InvalidObject => "LNK006",
            ErrorCode::AbiMismatch => "LNK007",
            ErrorCode::UnsupportedRelocation => "LNK008",
            ErrorCode::InvalidSection => "LNK009",
            ErrorCode::InvalidTarget => "LNK010",
            ErrorCode::EntryPointNotFound => "LNK011",
            ErrorCode::InvalidArchive => "LNK012",
            ErrorCode::SecurityViolation => "LNK013",
            ErrorCode::MetadataMismatch => "LNK014",
            ErrorCode::IoError => "LNK015",
            ErrorCode::MemoryExhausted => "LNK016",
            ErrorCode::UnsupportedFormat => "LNK017",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ErrorCode::UndefinedSymbol => "undefined symbol",
            ErrorCode::DuplicateSymbol => "duplicate symbol",
            ErrorCode::ArchitectureMismatch => "architecture mismatch",
            ErrorCode::FormatMismatch => "binary format mismatch",
            ErrorCode::RelocationOverflow => "relocation overflow",
            ErrorCode::InvalidObject => "invalid or malformed object",
            ErrorCode::AbiMismatch => "ABI mismatch",
            ErrorCode::UnsupportedRelocation => "unsupported relocation",
            ErrorCode::InvalidSection => "invalid section configuration",
            ErrorCode::InvalidTarget => "invalid target triple",
            ErrorCode::EntryPointNotFound => "entry point not found",
            ErrorCode::InvalidArchive => "invalid static archive",
            ErrorCode::SecurityViolation => "security layout violation",
            ErrorCode::MetadataMismatch => "Adesh metadata mismatch",
            ErrorCode::IoError => "I/O error",
            ErrorCode::MemoryExhausted => "memory region exhausted",
            ErrorCode::UnsupportedFormat => "output format not implemented",
        }
    }
}

/// Optional contextual metadata attached to a [`LinkError`], boxed so `LinkResult<T>` stays small.
#[derive(Debug, Clone, Default)]
pub struct LinkErrorDetails {
    pub symbol: Option<String>,
    pub file: Option<String>,
    pub section: Option<String>,
    pub offset: Option<u64>,
    pub notes: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Rich structured Linker Error.
#[derive(Debug, Clone)]
pub struct LinkError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Box<LinkErrorDetails>,
}

impl LinkError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Box::default(),
        }
    }

    pub fn undefined_symbol(
        symbol: impl Into<String>,
        referenced_by_file: impl Into<String>,
        referenced_by_symbol: Option<&str>,
        offset: Option<u64>,
    ) -> Self {
        let sym_name = symbol.into();
        let file_name = referenced_by_file.into();
        let mut err = Self::new(
            ErrorCode::UndefinedSymbol,
            format!("undefined reference to `{}`", sym_name),
        );
        err.details.symbol = Some(sym_name.clone());
        err.details.file = Some(file_name.clone());
        err.details.offset = offset;

        let mut ref_desc = format!("referenced in {}", file_name);
        if let Some(r_sym) = referenced_by_symbol {
            ref_desc.push_str(&format!(" (in symbol `{}`)", r_sym));
        }
        if let Some(off) = offset {
            ref_desc.push_str(&format!(" at offset 0x{:x}", off));
        }
        err.details.notes.push(ref_desc);
        err.details.suggestions.push(format!(
            "Check if the object or static archive containing `{}` was passed to the linker.",
            sym_name
        ));
        err.details
            .suggestions
            .push("Ensure symbol visibility is global and not static/internal.".to_string());
        err
    }

    pub fn duplicate_symbol(
        symbol: impl Into<String>,
        first_file: impl Into<String>,
        second_file: impl Into<String>,
    ) -> Self {
        let sym_name = symbol.into();
        let first = first_file.into();
        let second = second_file.into();
        let mut err = Self::new(
            ErrorCode::DuplicateSymbol,
            format!("multiple definitions of symbol `{}`", sym_name),
        );
        err.details.symbol = Some(sym_name);
        err.details
            .notes
            .push(format!("first defined in: {}", first));
        err.details.notes.push(format!("redefined in: {}", second));
        err.details.suggestions.push(
            "Ensure only one definition of the strong symbol exists or mark one as weak."
                .to_string(),
        );
        err
    }

    pub fn architecture_mismatch(
        file: impl Into<String>,
        file_arch: impl Into<String>,
        target_arch: impl Into<String>,
    ) -> Self {
        let f = file.into();
        let fa = file_arch.into();
        let ta = target_arch.into();
        let mut err = Self::new(
            ErrorCode::ArchitectureMismatch,
            format!(
                "cannot link object `{}`: architecture `{}` does not match target `{}`",
                f, fa, ta
            ),
        );
        err.details.file = Some(f);
        err.details
            .notes
            .push(format!("object architecture: {}", fa));
        err.details
            .notes
            .push(format!("target architecture: {}", ta));
        err.details
            .suggestions
            .push("Recompile the input object for the target architecture.".to_string());
        err
    }

    pub fn relocation_overflow(
        reloc_name: &str,
        symbol: &str,
        value: i64,
        min: i64,
        max: i64,
        file: Option<&str>,
        offset: Option<u64>,
    ) -> Self {
        let mut err = Self::new(
            ErrorCode::RelocationOverflow,
            format!(
                "relocation `{}` for symbol `{}` overflowed (value: 0x{:x}, valid range: [0x{:x}, 0x{:x}])",
                reloc_name, symbol, value, min, max
            ),
        );
        err.details.symbol = Some(symbol.to_string());
        if let Some(f) = file {
            err.details.file = Some(f.to_string());
        }
        err.details.offset = offset;
        err.details.suggestions.push(
            "Consider using a larger code model (e.g. medium/large) or PIC relocation model."
                .to_string(),
        );
        err
    }

    pub fn entry_point_not_found(entry_name: &str, searched_files: &[String]) -> Self {
        let mut err = Self::new(
            ErrorCode::EntryPointNotFound,
            format!(
                "entry point `{}` was not found in any linked object",
                entry_name
            ),
        );
        err.details.symbol = Some(entry_name.to_string());
        err.details.notes.push(format!(
            "searched in {} input file(s): {}",
            searched_files.len(),
            searched_files.join(", ")
        ));
        err.details.suggestions.push(format!(
            "Define an entry function named `{}` or specify `--entry <symbol>`.",
            entry_name
        ));
        err
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.details.notes.push(note.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.details.suggestions.push(suggestion.into());
        self
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code.as_str(), self.message)?;
        for note in &self.details.notes {
            write!(f, "\n  note: {}", note)?;
        }
        for sug in &self.details.suggestions {
            write!(f, "\n  help: {}", sug)?;
        }
        Ok(())
    }
}

impl std::error::Error for LinkError {}

impl From<std::io::Error> for LinkError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorCode::IoError, err.to_string())
    }
}
