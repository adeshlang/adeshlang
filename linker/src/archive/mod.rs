//! Static archive handling.

pub mod ar;
pub mod index;

pub use ar::{AR_MAGIC, Archive, ArchiveMember};
pub use index::ArchiveIndex;
