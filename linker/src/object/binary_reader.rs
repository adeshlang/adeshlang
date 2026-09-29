//! Safe checked binary reader for parsing object files without unchecked indexing or panic risk.

use crate::error::{ErrorCode, LinkError, LinkResult};

/// Zero-copy, checked binary buffer reader.
#[derive(Clone, Copy, Debug)]
pub struct BinaryReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn at_offset(data: &'a [u8], offset: usize) -> LinkResult<Self> {
        if offset > data.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("offset {} exceeds buffer length {}", offset, data.len()),
            ));
        }
        Ok(Self { data, offset })
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn set_offset(&mut self, offset: usize) -> LinkResult<()> {
        if offset > self.data.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "seek offset {} exceeds buffer length {}",
                    offset,
                    self.data.len()
                ),
            ));
        }
        self.offset = offset;
        Ok(())
    }

    pub fn read_u8(&mut self) -> LinkResult<u8> {
        let bytes = self.read_bytes(1)?;
        Ok(bytes[0])
    }

    pub fn read_i8(&mut self) -> LinkResult<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_u16_le(&mut self) -> LinkResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u16_be(&mut self) -> LinkResult<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_i16_le(&mut self) -> LinkResult<i16> {
        Ok(self.read_u16_le()? as i16)
    }

    pub fn read_i16_be(&mut self) -> LinkResult<i16> {
        Ok(self.read_u16_be()? as i16)
    }

    pub fn read_u32_le(&mut self) -> LinkResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_u32_be(&mut self) -> LinkResult<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_i32_le(&mut self) -> LinkResult<i32> {
        Ok(self.read_u32_le()? as i32)
    }

    pub fn read_i32_be(&mut self) -> LinkResult<i32> {
        Ok(self.read_u32_be()? as i32)
    }

    pub fn read_u64_le(&mut self) -> LinkResult<u64> {
        let bytes = self.read_bytes(8)?;
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| LinkError::new(ErrorCode::InvalidObject, "failed to parse u64"))?;
        Ok(u64::from_le_bytes(arr))
    }

    pub fn read_u64_be(&mut self) -> LinkResult<u64> {
        let bytes = self.read_bytes(8)?;
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| LinkError::new(ErrorCode::InvalidObject, "failed to parse u64"))?;
        Ok(u64::from_be_bytes(arr))
    }

    pub fn read_i64_le(&mut self) -> LinkResult<i64> {
        Ok(self.read_u64_le()? as i64)
    }

    pub fn read_i64_be(&mut self) -> LinkResult<i64> {
        Ok(self.read_u64_be()? as i64)
    }

    pub fn read_bytes(&mut self, count: usize) -> LinkResult<&'a [u8]> {
        let end = self.checked_add(self.offset, count)?;
        if end > self.data.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "read range {}..{} out of bounds (len {})",
                    self.offset,
                    end,
                    self.data.len()
                ),
            ));
        }
        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    pub fn read_string(&mut self, len: usize) -> LinkResult<String> {
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8_lossy(bytes).to_string())
    }

    pub fn read_null_terminated_string(&mut self) -> LinkResult<String> {
        let start = self.offset;
        let mut curr = start;
        while curr < self.data.len() {
            if self.data[curr] == 0 {
                let slice = &self.data[start..curr];
                self.offset = curr + 1;
                return Ok(String::from_utf8_lossy(slice).to_string());
            }
            curr += 1;
        }
        Err(LinkError::new(
            ErrorCode::InvalidObject,
            "unterminated string in binary buffer",
        ))
    }

    pub fn checked_range(&self, start: usize, len: usize) -> LinkResult<&'a [u8]> {
        let end = self.checked_add(start, len)?;
        if end > self.data.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "range {}..{} out of bounds (buffer len {})",
                    start,
                    end,
                    self.data.len()
                ),
            ));
        }
        Ok(&self.data[start..end])
    }

    pub fn checked_add(&self, a: usize, b: usize) -> LinkResult<usize> {
        a.checked_add(b).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                "integer overflow during buffer offset calculation",
            )
        })
    }

    pub fn checked_mul(&self, a: usize, b: usize) -> LinkResult<usize> {
        a.checked_mul(b).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                "integer overflow during size calculation",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_reader_primitive_reads() {
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut reader = BinaryReader::new(&data);
        assert_eq!(reader.read_u8().unwrap(), 0x01);
        assert_eq!(reader.read_u16_le().unwrap(), 0x0302);
        assert_eq!(reader.read_u32_le().unwrap(), 0x07060504);
        assert_eq!(reader.remaining(), 1);
        assert_eq!(reader.read_u8().unwrap(), 0x08);
        assert!(reader.read_u8().is_err());
    }

    #[test]
    fn test_binary_reader_bounds_check() {
        let data = [0x41, 0x42, 0x43, 0x00];
        let mut reader = BinaryReader::new(&data);
        assert_eq!(reader.read_null_terminated_string().unwrap(), "ABC");
        assert!(reader.read_bytes(10).is_err());
    }
}
