//! Extension Table for forward-compatible, future-proof ADOB extensions.

/// ADOB Extension record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobExtension {
    /// 16-byte UUID identifying the extension kind.
    pub uuid: [u8; 16],
    /// Extension semantic version (major, minor).
    pub version: (u16, u16),
    /// Human-readable identifier/tag.
    pub tag: String,
    /// Extension binary payload.
    pub payload: Vec<u8>,
}

impl AdobExtension {
    pub fn new(
        uuid: [u8; 16],
        version: (u16, u16),
        tag: impl Into<String>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            uuid,
            version,
            tag: tag.into(),
            payload,
        }
    }
}

/// Collection of extensions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtensionTable {
    pub extensions: Vec<AdobExtension>,
}

impl ExtensionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, ext: AdobExtension) {
        self.extensions.push(ext);
    }

    pub fn find_by_uuid(&self, uuid: &[u8; 16]) -> Option<&AdobExtension> {
        self.extensions.iter().find(|e| &e.uuid == uuid)
    }

    pub fn find_by_tag(&self, tag: &str) -> Option<&AdobExtension> {
        self.extensions.iter().find(|e| e.tag == tag)
    }
}
