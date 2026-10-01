//! Error types for ADOB (Adesh Native Object Binary) processing.

use std::fmt;

/// Result type alias for ADOB operations.
pub type AdobResult<T> = Result<T, AdobError>;

/// Specific error codes for ADOB operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdobErrorCode {
    InvalidMagic,
    UnsupportedVersion,
    CorruptHeader,
    InvalidTarget,
    InvalidArchitecture,
    InvalidAbi,
    InvalidSection,
    InvalidSectionOffset,
    SectionOverlap,
    InvalidAlignment,
    InvalidSymbol,
    DuplicateSymbol,
    UndefinedSymbol,
    InvalidRelocation,
    RelocationOutOfRange,
    RelocationTargetNotFound,
    InvalidMetadata,
    CorruptExtension,
    BufferUnderflow,
    IntegerOverflow,
    IoError,
    UnsupportedFeature,
}

/// Structured error returned by ADOB validation, reading, and writing.
#[derive(Debug, Clone)]
pub struct AdobError {
    pub code: AdobErrorCode,
    pub message: String,
    pub context: Option<String>,
    pub offset: Option<usize>,
    pub section: Option<String>,
    pub symbol: Option<String>,
}

impl AdobError {
    pub fn new(code: AdobErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: None,
            offset: None,
            section: None,
            symbol: None,
        }
    }

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }
}

impl fmt::Display for AdobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ADOB error [{:?}]: {}", self.code, self.message)?;
        if let Some(ref sec) = self.section {
            write!(f, " (section: `{}`)", sec)?;
        }
        if let Some(ref sym) = self.symbol {
            write!(f, " (symbol: `{}`)", sym)?;
        }
        if let Some(off) = self.offset {
            write!(f, " at byte offset 0x{:X}", off)?;
        }
        if let Some(ref ctx) = self.context {
            write!(f, " [{}]", ctx)?;
        }
        Ok(())
    }
}

impl std::error::Error for AdobError {}
