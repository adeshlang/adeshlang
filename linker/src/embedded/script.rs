//! Linker Script parser and memory region layout engine for embedded targets.

use crate::error::{ErrorCode, LinkError, LinkResult};
use std::collections::HashMap;

/// Embedded memory region attributes and boundary constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    pub name: String,
    pub attributes: String, // e.g. "rx", "rwx", "w"
    pub origin: u64,
    pub length: u64,
    pub current_offset: u64,
}

impl MemoryRegion {
    pub fn new(
        name: impl Into<String>,
        attributes: impl Into<String>,
        origin: u64,
        length: u64,
    ) -> Self {
        Self {
            name: name.into(),
            attributes: attributes.into(),
            origin,
            length,
            current_offset: 0,
        }
    }

    pub fn available_space(&self) -> u64 {
        self.length.saturating_sub(self.current_offset)
    }

    pub fn allocate(&mut self, size: u64, align: u64) -> LinkResult<u64> {
        let align_mask = align - 1;
        let aligned_offset = (self.current_offset + align_mask) & !align_mask;
        let new_offset = aligned_offset + size;
        if new_offset > self.length {
            return Err(LinkError::new(
                ErrorCode::MemoryExhausted,
                format!(
                    "Region `{}` overflowed by {} bytes (origin=0x{:08X}, length=0x{:08X})",
                    self.name,
                    new_offset - self.length,
                    self.origin,
                    self.length
                ),
            ));
        }
        self.current_offset = new_offset;
        Ok(self.origin + aligned_offset)
    }
}

/// Linker Section assignment mapping an output section to VMA/LMA memory regions.
#[derive(Debug, Clone)]
pub struct SectionAssignment {
    pub section_name: String,
    pub target_region: String,      // VMA (e.g. RAM or FLASH)
    pub lma_region: Option<String>, // LMA (e.g. AT > FLASH for .data)
    pub alignment: u64,
    pub keep: bool,
}

/// Linker script container managing memory regions and section mapping.
#[derive(Debug, Clone, Default)]
pub struct LinkerScript {
    pub memory_regions: HashMap<String, MemoryRegion>,
    pub section_assignments: Vec<SectionAssignment>,
    pub symbols: HashMap<String, u64>,
}

impl LinkerScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Standard ARM Cortex-M micro-controller template (e.g., STM32 / nRF52 / SAMD).
    pub fn standard_cortex_m(
        flash_origin: u64,
        flash_size: u64,
        ram_origin: u64,
        ram_size: u64,
    ) -> Self {
        let mut script = Self::new();
        script.memory_regions.insert(
            "FLASH".to_string(),
            MemoryRegion::new("FLASH", "rx", flash_origin, flash_size),
        );
        script.memory_regions.insert(
            "RAM".to_string(),
            MemoryRegion::new("RAM", "rwx", ram_origin, ram_size),
        );

        // Standard Cortex-M sections
        script.section_assignments.push(SectionAssignment {
            section_name: ".vectors".to_string(),
            target_region: "FLASH".to_string(),
            lma_region: None,
            alignment: 256,
            keep: true,
        });
        script.section_assignments.push(SectionAssignment {
            section_name: ".text".to_string(),
            target_region: "FLASH".to_string(),
            lma_region: None,
            alignment: 4,
            keep: false,
        });
        script.section_assignments.push(SectionAssignment {
            section_name: ".rodata".to_string(),
            target_region: "FLASH".to_string(),
            lma_region: None,
            alignment: 4,
            keep: false,
        });
        script.section_assignments.push(SectionAssignment {
            section_name: ".data".to_string(),
            target_region: "RAM".to_string(),
            lma_region: Some("FLASH".to_string()),
            alignment: 4,
            keep: false,
        });
        script.section_assignments.push(SectionAssignment {
            section_name: ".bss".to_string(),
            target_region: "RAM".to_string(),
            lma_region: None,
            alignment: 4,
            keep: false,
        });

        // Top of stack symbol
        script
            .symbols
            .insert("_estack".to_string(), ram_origin + ram_size);
        script
    }

    /// Standard RISC-V 32/64 bare-metal embedded template.
    pub fn standard_riscv(
        flash_origin: u64,
        flash_size: u64,
        ram_origin: u64,
        ram_size: u64,
    ) -> Self {
        let mut script = Self::standard_cortex_m(flash_origin, flash_size, ram_origin, ram_size);
        script
            .symbols
            .insert("__global_pointer$".to_string(), ram_origin + 0x800);
        script
    }

    /// Parses a minimal GNU ld linker script.
    pub fn parse_script(content: &str) -> LinkResult<Self> {
        let mut script = Self::new();
        let mut in_memory = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("/*") || line.starts_with("//") {
                continue;
            }

            if line.starts_with("MEMORY") {
                in_memory = true;
                continue;
            }

            if in_memory {
                if line.contains('}') {
                    in_memory = false;
                    continue;
                }
                // Example: FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512K
                if let Some(colon_idx) = line.find(':') {
                    let head = line[..colon_idx].trim();
                    let tail = line[colon_idx + 1..].trim();

                    let name = head.split_whitespace().next().unwrap_or("MEM").to_string();
                    let attrs = if let Some(p1) = head.find('(') {
                        if let Some(p2) = head.find(')') {
                            head[p1 + 1..p2].to_string()
                        } else {
                            "rwx".to_string()
                        }
                    } else {
                        "rwx".to_string()
                    };

                    let mut origin = 0u64;
                    let mut length = 0u64;

                    for part in tail.split(',') {
                        let part = part.trim();
                        if part.starts_with("ORIGIN") {
                            if let Some(eq) = part.find('=') {
                                origin = parse_size_or_hex(part[eq + 1..].trim());
                            }
                        } else if part.starts_with("LENGTH") {
                            if let Some(eq) = part.find('=') {
                                length = parse_size_or_hex(part[eq + 1..].trim());
                            }
                        }
                    }

                    script
                        .memory_regions
                        .insert(name.clone(), MemoryRegion::new(name, attrs, origin, length));
                }
            }
        }

        Ok(script)
    }
}

fn parse_size_or_hex(s: &str) -> u64 {
    let s = s.trim().trim_end_matches(';').trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).unwrap_or(0)
    } else if s.ends_with('K') || s.ends_with('k') {
        let num: u64 = s[..s.len() - 1].parse().unwrap_or(0);
        num * 1024
    } else if s.ends_with('M') || s.ends_with('m') {
        let num: u64 = s[..s.len() - 1].parse().unwrap_or(0);
        num * 1024 * 1024
    } else {
        s.parse().unwrap_or(0)
    }
}
