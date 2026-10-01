//! Complete ADOB symbol model.

pub type SymbolId = u32;

/// Symbol binding rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolBinding {
    Local,
    #[default]
    Global,
    Weak,
    Unique,
}

/// Symbol visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolVisibility {
    #[default]
    Default,
    Hidden,
    Protected,
    Internal,
}

/// Category/Kind of symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolKind {
    #[default]
    Function,
    Object,
    Section,
    TLS,
    Import,
    Export,
    AcceleratorKernel,
    Runtime,
}

/// Complete ADOB Symbol descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobSymbol {
    pub id: SymbolId,
    pub name: String,
    pub binding: SymbolBinding,
    pub visibility: SymbolVisibility,
    pub kind: SymbolKind,
    pub is_defined: bool,
    pub section_index: Option<u32>,
    pub value: u64,
    pub size: u64,
    pub version: Option<String>,
    pub alias_target: Option<String>,
}

impl AdobSymbol {
    pub fn new(id: SymbolId, name: impl Into<String>, kind: SymbolKind) -> Self {
        Self {
            id,
            name: name.into(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            kind,
            is_defined: true,
            section_index: None,
            value: 0,
            size: 0,
            version: None,
            alias_target: None,
        }
    }

    pub fn new_defined(
        id: SymbolId,
        name: impl Into<String>,
        kind: SymbolKind,
        section_index: u32,
        value: u64,
        size: u64,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            kind,
            is_defined: true,
            section_index: Some(section_index),
            value,
            size,
            version: None,
            alias_target: None,
        }
    }

    pub fn new_undefined(id: SymbolId, name: impl Into<String>, kind: SymbolKind) -> Self {
        Self {
            id,
            name: name.into(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            kind,
            is_defined: false,
            section_index: None,
            value: 0,
            size: 0,
            version: None,
            alias_target: None,
        }
    }

    pub fn with_binding(mut self, binding: SymbolBinding) -> Self {
        self.binding = binding;
        self
    }

    pub fn with_visibility(mut self, visibility: SymbolVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_alias(mut self, target: impl Into<String>) -> Self {
        self.alias_target = Some(target.into());
        self
    }
}
