//! Object file format serializer and binary generator.

use crate::error::LinkResult;
use crate::object::ObjectFile;
use std::fs;
use std::path::Path;

pub struct ObjectWriter;

impl ObjectWriter {
    /// Write an ObjectFile into the Adesh native object binary format.
    pub fn write_to_file(obj: &ObjectFile, path: &Path) -> LinkResult<()> {
        let bytes = Self::encode(obj)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Encode an ObjectFile into bytes (ADOB v2).
    pub fn encode(obj: &ObjectFile) -> LinkResult<Vec<u8>> {
        crate::object::AdobV2::encode(obj)
    }
}
