//! IBM AIX XCOFF (XCOFF32 / XCOFF64) binary headers and constants.

pub const U802TOCMAGIC: u16 = 0x01DF; // 32-bit XCOFF
pub const U64_TOCMAGIC: u16 = 0x01F7; // 64-bit XCOFF

pub const _XCOFF32: u16 = 1;
pub const _XCOFF64: u16 = 2;

// Section flags
pub const STYP_TEXT: u32 = 0x0020;
pub const STYP_DATA: u32 = 0x0040;
pub const STYP_BSS: u32  = 0x0080;
pub const STYP_LOADER: u32 = 0x1000;

/// XCOFF File Header.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct XcoffFileHeader {
    pub f_magic: u16,
    pub f_nscns: u16,
    pub f_timdat: u32,
    pub f_symptr: u64,
    pub f_nsyms: u32,
    pub f_opthdr: u16,
    pub f_flags: u16,
}
