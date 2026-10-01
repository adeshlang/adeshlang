//! Detailed error diagnostics for the Adesh CodeGen pipeline.

use std::fmt;

/// Structured CodeGen Error identifying exact target, architecture, function, instruction, and offset.
#[derive(Debug, Clone)]
pub struct CodegenError {
    pub target: String,
    pub architecture: String,
    pub abi: String,
    pub function: Option<String>,
    pub instruction: Option<String>,
    pub reason: String,
    pub symbol: Option<String>,
    pub section: Option<String>,
    pub offset: Option<usize>,
    pub suggested_action: Option<String>,
}

impl CodegenError {
    pub fn new(target: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            architecture: String::new(),
            abi: String::new(),
            function: None,
            instruction: None,
            reason: reason.into(),
            symbol: None,
            section: None,
            offset: None,
            suggested_action: None,
        }
    }

    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.architecture = arch.into();
        self
    }

    pub fn with_abi(mut self, abi: impl Into<String>) -> Self {
        self.abi = abi.into();
        self
    }

    pub fn with_function(mut self, func: impl Into<String>) -> Self {
        self.function = Some(func.into());
        self
    }

    pub fn with_instruction(mut self, inst: impl Into<String>) -> Self {
        self.instruction = Some(inst.into());
        self
    }

    pub fn with_symbol(mut self, sym: impl Into<String>) -> Self {
        self.symbol = Some(sym.into());
        self
    }

    pub fn with_section(mut self, sec: impl Into<String>) -> Self {
        self.section = Some(sec.into());
        self
    }

    pub fn with_offset(mut self, off: usize) -> Self {
        self.offset = Some(off);
        self
    }

    pub fn with_suggestion(mut self, action: impl Into<String>) -> Self {
        self.suggested_action = Some(action.into());
        self
    }
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Adesh CodeGen Error")?;
        writeln!(f, "Target: {}", self.target)?;
        if !self.architecture.is_empty() {
            writeln!(f, "Architecture: {}", self.architecture)?;
        }
        if !self.abi.is_empty() {
            writeln!(f, "ABI: {}", self.abi)?;
        }
        if let Some(ref func) = self.function {
            writeln!(f, "Function: {}", func)?;
        }
        if let Some(ref inst) = self.instruction {
            writeln!(f, "Instruction: {}", inst)?;
        }
        writeln!(f, "Reason: {}", self.reason)?;
        if let Some(ref sym) = self.symbol {
            writeln!(f, "Symbol: {}", sym)?;
        }
        if let Some(ref sec) = self.section {
            if let Some(off) = self.offset {
                writeln!(f, "Offset: {} + 0x{:X}", sec, off)?;
            } else {
                writeln!(f, "Section: {}", sec)?;
            }
        }
        if let Some(ref action) = self.suggested_action {
            writeln!(f, "Suggested action: {}", action)?;
        }
        Ok(())
    }
}

impl std::error::Error for CodegenError {}
