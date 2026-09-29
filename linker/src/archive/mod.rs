//! Static archive handling.

pub mod ar;
pub mod index;

pub use ar::{Archive, ArchiveMember, AR_MAGIC};
pub use index::ArchiveIndex;
