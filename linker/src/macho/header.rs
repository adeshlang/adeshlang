//! Mach-O 64-bit binary headers, load commands, and constants.

pub const MH_MAGIC_64: u32 = 0xFEEDFACF;
pub const MH_CIGAM_64: u32 = 0xCFFAEDFE;

pub const CPU_TYPE_X86_64: u32 = 0x01000007;
pub const CPU_TYPE_ARM64: u32 = 0x0100000C;

pub const CPU_SUBTYPE_LIB64: u32 = 0x80000000;
pub const CPU_SUBTYPE_X86_64_ALL: u32 = 3;
pub const CPU_SUBTYPE_ARM64_ALL: u32 = 0;

pub const MH_OBJECT: u32 = 1;
pub const MH_EXECUTE: u32 = 2;
pub const MH_DYLIB: u32 = 6;

pub const MH_NOUNDEFS: u32 = 0x1;
pub const MH_DYLDLINK: u32 = 0x4;
pub const MH_TWOLEVEL: u32 = 0x80;
pub const MH_PIE: u32 = 0x200000;

// Load commands
pub const LC_SYMTAB: u32 = 0x02;
pub const LC_DYSYMTAB: u32 = 0x0B;
pub const LC_LOAD_DYLIB: u32 = 0x0C;
pub const LC_ID_DYLIB: u32 = 0x0D;
pub const LC_LOAD_DYLINKER: u32 = 0x0E;
pub const LC_SEGMENT_64: u32 = 0x19;
pub const LC_DYLD_INFO: u32 = 0x22;
pub const LC_DYLD_INFO_ONLY: u32 = 0x80000022;
pub const LC_LOAD_WEAK_DYLIB: u32 = 0x80000018;
pub const LC_RPATH: u32 = 0x8000001C;
pub const LC_MAIN: u32 = 0x80000028;
pub const LC_BUILD_VERSION: u32 = 0x32;

// Platform constants for LC_BUILD_VERSION
pub const PLATFORM_MACOS: u32 = 1;
pub const PLATFORM_IOS: u32 = 2;
pub const PLATFORM_TVOS: u32 = 3;
pub const PLATFORM_WATCHOS: u32 = 4;

// VM Protections
pub const VM_PROT_NONE: u32 = 0;
pub const VM_PROT_READ: u32 = 1;
pub const VM_PROT_WRITE: u32 = 2;
pub const VM_PROT_EXECUTE: u32 = 4;

/// Mach-O 64-bit File Header (32 bytes).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MachHeader64 {
    pub magic: u32,
    pub cputype: u32,
    pub cpusubtype: u32,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
    pub flags: u32,
    pub reserved: u32,
}

/// 64-bit Symbol table entry (`nlist_64`, 16 bytes).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Nlist64 {
    pub n_strx: u32,
    pub n_type: u8,
    pub n_sect: u8,
    pub n_desc: u16,
    pub n_value: u64,
}

/// Dynamic symbol table command (`LC_DYSYMTAB`, 80 bytes).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DysymtabCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub ilocalsym: u32,
    pub nlocalsym: u32,
    pub iextdefsym: u32,
    pub nextdefsym: u32,
    pub iundefsym: u32,
    pub nundefsym: u32,
    pub tocoff: u32,
    pub ntoc: u32,
    pub modtaboff: u32,
    pub nmodtab: u32,
    pub extrefsymoff: u32,
    pub nextrefsyms: u32,
    pub indirectsymoff: u32,
    pub nindirectsyms: u32,
    pub extreloff: u32,
    pub nextrel: u32,
    pub locreloff: u32,
    pub nlocrel: u32,
}

/// Dynamic Library load/ID command (`LC_LOAD_DYLIB` / `LC_ID_DYLIB`, 24 bytes + path).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DylibCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub name_offset: u32,
    pub timestamp: u32,
    pub current_version: u32,
    pub compatibility_version: u32,
}

/// Modern OS version requirements (`LC_BUILD_VERSION`, 24 bytes).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BuildVersionCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub platform: u32,
    pub minos: u32,
    pub sdk: u32,
    pub ntools: u32,
}

/// Dyld info command (`LC_DYLD_INFO` / `LC_DYLD_INFO_ONLY`, 48 bytes).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DyldInfoCommand {
    pub cmd: u32,
    pub cmdsize: u32,
    pub rebase_off: u32,
    pub rebase_size: u32,
    pub bind_off: u32,
    pub bind_size: u32,
    pub weak_bind_off: u32,
    pub weak_bind_size: u32,
    pub lazy_bind_off: u32,
    pub lazy_bind_size: u32,
    pub export_off: u32,
    pub export_size: u32,
}

