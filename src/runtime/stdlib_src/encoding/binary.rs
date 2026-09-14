//! Binary Serialization, Endianness, Integers, Floats, BinaryReader and BinaryWriter for AdeshLang.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

// ----------------------------------------------------------------------------
// Fixed-width Integers (LE & BE)
// ----------------------------------------------------------------------------

pub fn u16_to_bytes_le(val: u16) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn u16_to_bytes_be(val: u16) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_u16_le(bytes: &[u8]) -> Result<u16, String> {
    if bytes.len() < 2 {
        return Err("Insufficient bytes for u16 (need 2)".to_string());
    }
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}
pub fn bytes_to_u16_be(bytes: &[u8]) -> Result<u16, String> {
    if bytes.len() < 2 {
        return Err("Insufficient bytes for u16 (need 2)".to_string());
    }
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

pub fn u32_to_bytes_le(val: u32) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn u32_to_bytes_be(val: u32) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_u32_le(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for u32 (need 4)".to_string());
    }
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
pub fn bytes_to_u32_be(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for u32 (need 4)".to_string());
    }
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub fn u64_to_bytes_le(val: u64) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn u64_to_bytes_be(val: u64) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_u64_le(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for u64 (need 8)".to_string());
    }
    Ok(u64::from_le_bytes(bytes[0..8].try_into().unwrap()))
}
pub fn bytes_to_u64_be(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for u64 (need 8)".to_string());
    }
    Ok(u64::from_be_bytes(bytes[0..8].try_into().unwrap()))
}

pub fn u128_to_bytes_le(val: u128) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn u128_to_bytes_be(val: u128) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_u128_le(bytes: &[u8]) -> Result<u128, String> {
    if bytes.len() < 16 {
        return Err("Insufficient bytes for u128 (need 16)".to_string());
    }
    Ok(u128::from_le_bytes(bytes[0..16].try_into().unwrap()))
}
pub fn bytes_to_u128_be(bytes: &[u8]) -> Result<u128, String> {
    if bytes.len() < 16 {
        return Err("Insufficient bytes for u128 (need 16)".to_string());
    }
    Ok(u128::from_be_bytes(bytes[0..16].try_into().unwrap()))
}

pub fn i16_to_bytes_le(val: i16) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn i16_to_bytes_be(val: i16) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_i16_le(bytes: &[u8]) -> Result<i16, String> {
    if bytes.len() < 2 {
        return Err("Insufficient bytes for i16 (need 2)".to_string());
    }
    Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
}
pub fn bytes_to_i16_be(bytes: &[u8]) -> Result<i16, String> {
    if bytes.len() < 2 {
        return Err("Insufficient bytes for i16 (need 2)".to_string());
    }
    Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
}

pub fn i32_to_bytes_le(val: i32) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn i32_to_bytes_be(val: i32) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_i32_le(bytes: &[u8]) -> Result<i32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for i32 (need 4)".to_string());
    }
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
pub fn bytes_to_i32_be(bytes: &[u8]) -> Result<i32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for i32 (need 4)".to_string());
    }
    Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub fn i64_to_bytes_le(val: i64) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn i64_to_bytes_be(val: i64) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_i64_le(bytes: &[u8]) -> Result<i64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for i64 (need 8)".to_string());
    }
    Ok(i64::from_le_bytes(bytes[0..8].try_into().unwrap()))
}
pub fn bytes_to_i64_be(bytes: &[u8]) -> Result<i64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for i64 (need 8)".to_string());
    }
    Ok(i64::from_be_bytes(bytes[0..8].try_into().unwrap()))
}

// ----------------------------------------------------------------------------
// Floats (IEEE-754 LE & BE)
// ----------------------------------------------------------------------------

pub fn f32_to_bytes_le(val: f32) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn f32_to_bytes_be(val: f32) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_f32_le(bytes: &[u8]) -> Result<f32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for f32 (need 4)".to_string());
    }
    Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
pub fn bytes_to_f32_be(bytes: &[u8]) -> Result<f32, String> {
    if bytes.len() < 4 {
        return Err("Insufficient bytes for f32 (need 4)".to_string());
    }
    Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub fn f64_to_bytes_le(val: f64) -> Vec<u8> {
    val.to_le_bytes().to_vec()
}
pub fn f64_to_bytes_be(val: f64) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}
pub fn bytes_to_f64_le(bytes: &[u8]) -> Result<f64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for f64 (need 8)".to_string());
    }
    Ok(f64::from_le_bytes(bytes[0..8].try_into().unwrap()))
}
pub fn bytes_to_f64_be(bytes: &[u8]) -> Result<f64, String> {
    if bytes.len() < 8 {
        return Err("Insufficient bytes for f64 (need 8)".to_string());
    }
    Ok(f64::from_be_bytes(bytes[0..8].try_into().unwrap()))
}

// ----------------------------------------------------------------------------
// Slice Read/Write at Offset with Bounds Protection
// ----------------------------------------------------------------------------

pub fn read_u16_le(buffer: &[u8], offset: usize) -> Result<u16, String> {
    if offset + 2 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 2 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u16_le(&buffer[offset..offset + 2])
}
pub fn read_u16_be(buffer: &[u8], offset: usize) -> Result<u16, String> {
    if offset + 2 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 2 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u16_be(&buffer[offset..offset + 2])
}
pub fn read_u32_le(buffer: &[u8], offset: usize) -> Result<u32, String> {
    if offset + 4 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 4 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u32_le(&buffer[offset..offset + 4])
}
pub fn read_u32_be(buffer: &[u8], offset: usize) -> Result<u32, String> {
    if offset + 4 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 4 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u32_be(&buffer[offset..offset + 4])
}
pub fn read_u64_le(buffer: &[u8], offset: usize) -> Result<u64, String> {
    if offset + 8 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 8 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u64_le(&buffer[offset..offset + 8])
}
pub fn read_u64_be(buffer: &[u8], offset: usize) -> Result<u64, String> {
    if offset + 8 > buffer.len() {
        return Err(format!(
            "Offset out of bounds: {} + 8 > {}",
            offset,
            buffer.len()
        ));
    }
    bytes_to_u64_be(&buffer[offset..offset + 8])
}

pub fn write_u16_le(buffer: &mut [u8], offset: usize, value: u16) -> Result<(), String> {
    if offset + 2 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 2 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}
pub fn write_u16_be(buffer: &mut [u8], offset: usize, value: u16) -> Result<(), String> {
    if offset + 2 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 2 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    Ok(())
}
pub fn write_u32_le(buffer: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    if offset + 4 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 4 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}
pub fn write_u32_be(buffer: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    if offset + 4 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 4 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    Ok(())
}
pub fn write_u64_le(buffer: &mut [u8], offset: usize, value: u64) -> Result<(), String> {
    if offset + 8 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 8 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    Ok(())
}
pub fn write_u64_be(buffer: &mut [u8], offset: usize, value: u64) -> Result<(), String> {
    if offset + 8 > buffer.len() {
        return Err(format!(
            "Write out of bounds: {} + 8 > {}",
            offset,
            buffer.len()
        ));
    }
    buffer[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

// ----------------------------------------------------------------------------
// BinaryReader
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BinaryReader {
    data: Vec<u8>,
    pos: usize,
}

impl BinaryReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, new_pos: usize) -> Result<(), String> {
        if new_pos > self.data.len() {
            return Err(format!(
                "Cannot seek to position {} (length is {})",
                new_pos,
                self.data.len()
            ));
        }
        self.pos = new_pos;
        Ok(())
    }

    pub fn skip(&mut self, count: usize) -> Result<(), String> {
        self.seek(self.pos + count)
    }

    pub fn read_u8(&mut self) -> Result<u8, String> {
        if self.pos >= self.data.len() {
            return Err("Unexpected EOF while reading u8".to_string());
        }
        let val = self.data[self.pos];
        self.pos += 1;
        Ok(val)
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<Vec<u8>, String> {
        if self.pos + count > self.data.len() {
            return Err(format!(
                "Unexpected EOF: requested {} bytes, remaining {}",
                count,
                self.remaining()
            ));
        }
        let slice = self.data[self.pos..self.pos + count].to_vec();
        self.pos += count;
        Ok(slice)
    }

    pub fn read_u16_le(&mut self) -> Result<u16, String> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn read_u16_be(&mut self) -> Result<u16, String> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    pub fn read_u32_le(&mut self) -> Result<u32, String> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn read_u32_be(&mut self) -> Result<u32, String> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn read_u64_le(&mut self) -> Result<u64, String> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes(b[0..8].try_into().unwrap()))
    }
    pub fn read_u64_be(&mut self) -> Result<u64, String> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_be_bytes(b[0..8].try_into().unwrap()))
    }
    pub fn read_f32_le(&mut self) -> Result<f32, String> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn read_f32_be(&mut self) -> Result<f32, String> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn read_f64_le(&mut self) -> Result<f64, String> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(b[0..8].try_into().unwrap()))
    }
    pub fn read_f64_be(&mut self) -> Result<f64, String> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_be_bytes(b[0..8].try_into().unwrap()))
    }
}

// ----------------------------------------------------------------------------
// BinaryWriter
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct BinaryWriter {
    data: Vec<u8>,
}

impl BinaryWriter {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn write_u8(&mut self, val: u8) {
        self.data.push(val);
    }
    pub fn write_bytes(&mut self, slice: &[u8]) {
        self.data.extend_from_slice(slice);
    }

    pub fn write_u16_le(&mut self, val: u16) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }
    pub fn write_u16_be(&mut self, val: u16) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }
    pub fn write_u32_le(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }
    pub fn write_u32_be(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }
    pub fn write_u64_le(&mut self, val: u64) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }
    pub fn write_u64_be(&mut self, val: u64) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }
    pub fn write_f32_le(&mut self, val: f32) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }
    pub fn write_f32_be(&mut self, val: f32) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }
    pub fn write_f64_le(&mut self, val: f64) {
        self.data.extend_from_slice(&val.to_le_bytes());
    }
    pub fn write_f64_be(&mut self, val: f64) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        self.data
    }
}
