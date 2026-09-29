//! Thread-Local Storage (TLS) model representations and layouts.

/// TLS linkage and access models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsModel {
    /// Local Exec (static binary, fixed thread offset)
    LocalExec,
    /// Initial Exec (dynamic executable or DLL loaded at process start)
    InitialExec,
    /// General Dynamic (runtime dlopen TLS resolution via __tls_get_addr)
    GeneralDynamic,
    /// Local Dynamic (module-relative TLS resolution)
    LocalDynamic,
}

/// TLS segment bounds and alignment.
#[derive(Debug, Clone, Default)]
pub struct TlsLayout {
    pub image_size: u64,
    pub file_size: u64,
    pub alignment: u64,
    pub offset: u64,
}
