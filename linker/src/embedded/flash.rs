//! Flash Firmware Image Writers: Flat Binary (`.bin`), Intel HEX (`.hex`), and Motorola S-Records (`.srec`).

use crate::error::LinkResult;
use std::fs;
use std::path::Path;

/// Flat Binary Image Writer (Direct flash memory dump).
pub struct FlashBinaryWriter;

impl FlashBinaryWriter {
    pub fn write_bin(path: &Path, base_address: u64, sections: &[(u64, &[u8])]) -> LinkResult<()> {
        if sections.is_empty() {
            fs::write(path, &[])?;
            return Ok(());
        }

        let mut max_end = base_address;
        for (addr, data) in sections {
            let end = *addr + data.len() as u64;
            if end > max_end {
                max_end = end;
            }
        }

        let total_size = (max_end - base_address) as usize;
        let mut image = vec![0xFFu8; total_size]; // 0xFF is standard erased flash value

        for (addr, data) in sections {
            let offset = (*addr - base_address) as usize;
            image[offset..offset + data.len()].copy_from_slice(data);
        }

        fs::write(path, image)?;
        Ok(())
    }
}

/// Intel HEX format writer (`.hex`).
pub struct IntelHexWriter;

impl IntelHexWriter {
    /// Formats contiguous address data chunks into valid Intel HEX records.
    pub fn encode_records(sections: &[(u64, &[u8])]) -> String {
        let mut hex = String::new();
        let mut current_upper_address = 0xFFFF_FFFFu64;

        for &(addr, data) in sections {
            let mut offset = 0usize;
            while offset < data.len() {
                let current_addr = addr + offset as u64;
                let upper_16 = (current_addr >> 16) as u16;

                // Emit Extended Linear Address Record (Type 04) if upper 16 bits changed
                if (upper_16 as u64) != current_upper_address {
                    current_upper_address = upper_16 as u64;
                    let upper_bytes = upper_16.to_be_bytes();
                    let record = format_record(0x0000, 0x04, &upper_bytes);
                    hex.push_str(&record);
                    hex.push('\n');
                }

                // Chunk up to 32 bytes per Data Record (Type 00)
                let chunk_len = std::cmp::min(32, data.len() - offset);
                let chunk = &data[offset..offset + chunk_len];
                let low_16 = (current_addr & 0xFFFF) as u16;

                let record = format_record(low_16, 0x00, chunk);
                hex.push_str(&record);
                hex.push('\n');

                offset += chunk_len;
            }
        }

        // End of File Record (Type 01)
        hex.push_str(":00000001FF\n");
        hex
    }

    pub fn write_hex(path: &Path, sections: &[(u64, &[u8])]) -> LinkResult<()> {
        let hex_str = Self::encode_records(sections);
        fs::write(path, hex_str)?;
        Ok(())
    }
}

/// Helper function to format an Intel HEX record with two's complement checksum.
fn format_record(addr: u16, record_type: u8, data: &[u8]) -> String {
    let mut checksum_sum: u32 =
        (data.len() as u32) + ((addr >> 8) as u32) + ((addr & 0xFF) as u32) + (record_type as u32);

    for &b in data {
        checksum_sum = checksum_sum.wrapping_add(b as u32);
    }

    let checksum = ((!(checksum_sum & 0xFF)).wrapping_add(1)) as u8;

    let mut out = format!(":{:02X}{:04X}{:02X}", data.len(), addr, record_type);
    for &b in data {
        out.push_str(&format!("{:02X}", b));
    }
    out.push_str(&format!("{:02X}", checksum));
    out
}

/// Motorola S-Record format writer (`.srec`).
pub struct MotorolaSrecWriter;

impl MotorolaSrecWriter {
    pub fn encode_records(sections: &[(u64, &[u8])]) -> String {
        let mut srec = String::new();
        // S0 Header Record
        srec.push_str("S00600004144455374\n"); // "ADES" header

        for &(addr, data) in sections {
            let mut offset = 0usize;
            while offset < data.len() {
                let chunk_len = std::cmp::min(32, data.len() - offset);
                let chunk = &data[offset..offset + chunk_len];
                let current_addr = (addr + offset as u64) as u32;

                // S3 Record (32-bit address)
                let count = (chunk_len + 5) as u8; // 4 bytes addr + data + 1 byte chk
                let mut sum: u32 = count as u32;
                for b in current_addr.to_be_bytes() {
                    sum = sum.wrapping_add(b as u32);
                }
                for &b in chunk {
                    sum = sum.wrapping_add(b as u32);
                }
                let checksum = !(sum & 0xFF) as u8;

                let mut line = format!("S3{:02X}{:08X}", count, current_addr);
                for &b in chunk {
                    line.push_str(&format!("{:02X}", b));
                }
                line.push_str(&format!("{:02X}\n", checksum));
                srec.push_str(&line);

                offset += chunk_len;
            }
        }

        // S7 Termination Record (32-bit entry point at 0)
        srec.push_str("S70500000000FA\n");
        srec
    }

    pub fn write_srec(path: &Path, sections: &[(u64, &[u8])]) -> LinkResult<()> {
        let srec_str = Self::encode_records(sections);
        fs::write(path, srec_str)?;
        Ok(())
    }
}
