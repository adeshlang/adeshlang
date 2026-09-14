//! I/O Operations
//!
//! File and stream I/O operations built on the runtime stdlib

/// Re-export from runtime stdlib for now
pub use crate::runtime::stdlib_src::io::*;

/// Read trait for readable streams
pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

/// Write trait for writable streams
pub trait Write {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize>;
    fn flush(&mut self) -> std::io::Result<()>;
}

/// File handle
pub struct File {
    inner: std::fs::File,
}

impl File {
    pub fn open(path: &str) -> std::io::Result<Self> {
        Ok(File {
            inner: std::fs::File::open(path)?,
        })
    }

    pub fn create(path: &str) -> std::io::Result<Self> {
        Ok(File {
            inner: std::fs::File::create(path)?,
        })
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.inner, buf)
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut self.inner, buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.inner)
    }
}
