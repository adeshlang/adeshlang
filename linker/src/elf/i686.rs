//! ELF i686 architecture and entry point synthesis.

use crate::relocation::{Relocation, RelocationKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};

/// Synthesize a minimal Linux ELF i386 startup stub: `_start` -> `main` -> `sys_exit`.
///
/// IA-32 System V ABI and Linux kernel execution requirements:
/// - Clear EBP for outermost-frame recognition.
/// - Read argc/argv/envp from the initial process stack.
/// - Align ESP to 16 bytes and pass argc/argv/envp on the stack.
/// - Call the program entry through `PcRelative32`.
/// - Pass main's EAX return value to Linux `sys_exit` in EBX.
pub fn synthesize_elf_i686_entry(
    program_entry_symbol: &str,
    file_index: usize,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    synthesize_entry(program_entry_symbol, file_index, None)
}

/// Like [`synthesize_elf_i686_entry`], but the stub first builds the static
/// thread-local storage block described by the image's `PT_TLS` header.
///
/// The Linux kernel does not set up TLS for a static executable, so `_start`
/// does it before `main` runs (IA-32 TLS variant II, local-exec model):
/// - Locate `PT_TLS` by scanning the program headers mapped at `image_base`.
/// - `mmap2` an anonymous block of `align_up(p_memsz, p_align) + 16` bytes.
/// - Place the thread pointer (TCB) at the end of the TLS data and store its
///   own address at `%gs:0`.
/// - Install the TCB base with `set_thread_area` and load `%gs`.
/// - Copy the `.tdata` initialization image (`p_filesz` bytes) to
///   `TP - align_up(p_memsz, p_align)`; the anonymous mapping already zero-fills `.tbss`.
///
/// Images without `PT_TLS` skip all of this. Failures exit with status 127.
pub fn synthesize_elf_i686_entry_with_tls(
    program_entry_symbol: &str,
    file_index: usize,
    image_base: u32,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    synthesize_entry(program_entry_symbol, file_index, Some(image_base))
}

/// Static TLS setup prologue. The two image-base dependent immediates (the
/// program header table address and the `e_phnum` address) are patched at
/// `TLS_PHDR_IMM` and `TLS_PHNUM_IMM`.
const TLS_PROLOGUE: [u8; 157] = [
    0xbb, 0x34, 0x80, 0x04, 0x08, // mov ebx, image_base + 52 (first phdr)
    0x0f, 0xb7, 0x0d, 0x2c, 0x80, 0x04, 0x08, // movzx ecx, word [image_base + 44] (e_phnum)
    0x85, 0xc9, // scan: test ecx, ecx
    0x0f, 0x84, 0x89, 0x00, 0x00, 0x00, // jz done
    0x83, 0x3b, 0x07, // cmp dword [ebx], PT_TLS
    0x74, 0x06, // je found
    0x83, 0xc3, 0x20, // add ebx, 32
    0x49, // dec ecx
    0xeb, 0xed, // jmp scan
    0x53, // found: push ebx
    0x8b, 0x4b, 0x14, // mov ecx, [ebx+20] (p_memsz)
    0x8b, 0x53, 0x1c, // mov edx, [ebx+28] (p_align)
    0x83, 0xfa, 0x01, // cmp edx, 1
    0x83, 0xd2, 0x00, // adc edx, 0 (align 0 -> 1)
    0x8d, 0x4c, 0x11, 0xff, // lea ecx, [ecx+edx-1]
    0xf7, 0xda, // neg edx
    0x21, 0xd1, // and ecx, edx (ecx = align_up(memsz, align))
    0x51, // push ecx
    0x83, 0xc1, 0x10, // add ecx, 16 (TCB)
    0x31, 0xdb, // xor ebx, ebx
    0xba, 0x03, 0x00, 0x00, 0x00, // mov edx, PROT_READ|PROT_WRITE
    0xbe, 0x22, 0x00, 0x00, 0x00, // mov esi, MAP_PRIVATE|MAP_ANONYMOUS
    0xbf, 0xff, 0xff, 0xff, 0xff, // mov edi, -1
    0xb8, 0xc0, 0x00, 0x00, 0x00, // mov eax, SYS_mmap2
    0xcd, 0x80, // int 0x80
    0x3d, 0x00, 0xf0, 0xff, 0xff, // cmp eax, -4096
    0x77, 0x3a, // ja fail
    0x59, // pop ecx
    0x8d, 0x14, 0x08, // lea edx, [eax+ecx] (thread pointer)
    0x89, 0x12, // mov [edx], edx (%gs:0 self pointer)
    0x50, // push eax (mmap_base = TP - align_up(p_memsz, p_align))
    0x6a, 0x51, // push flags (32-bit, pages, useable)
    0x68, 0xff, 0xff, 0x0f, 0x00, // push limit
    0x52, // push base
    0x6a, 0xff, // push entry_number = -1
    0x89, 0xe3, // mov ebx, esp
    0xb8, 0xf3, 0x00, 0x00, 0x00, // mov eax, SYS_set_thread_area
    0xcd, 0x80, // int 0x80
    0x85, 0xc0, // test eax, eax
    0x75, 0x1c, // jnz fail
    0x8b, 0x04, 0x24, // mov eax, [esp] (allocated entry)
    0x8d, 0x04, 0xc5, 0x03, 0x00, 0x00, 0x00, // lea eax, [eax*8+3]
    0x8e, 0xe8, // mov gs, eax
    0x83, 0xc4, 0x10, // add esp, 16 (user_desc)
    0x5f, // pop edi (mmap_base = TP - align_up(p_memsz, p_align))
    0x5b, // pop ebx (PT_TLS header)
    0x8b, 0x4b, 0x10, // mov ecx, [ebx+16] (p_filesz)
    0x8b, 0x73, 0x08, // mov esi, [ebx+8] (p_vaddr)
    0xfc, // cld
    0xf3, 0xa4, // rep movsb
    0xeb, 0x0c, // jmp done
    0xbb, 0x7f, 0x00, 0x00, 0x00, // fail: mov ebx, 127
    0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, SYS_exit
    0xcd, 0x80, // int 0x80
];
const TLS_PHDR_IMM: usize = 1;
const TLS_PHNUM_IMM: usize = 8;

fn synthesize_entry(
    program_entry_symbol: &str,
    file_index: usize,
    tls_image_base: Option<u32>,
) -> (Vec<u8>, Vec<Relocation>, Vec<Symbol>) {
    let mut code_bytes = vec![0x31, 0xed]; // xor ebp, ebp
    if let Some(image_base) = tls_image_base {
        let mut prologue = TLS_PROLOGUE;
        prologue[TLS_PHDR_IMM..TLS_PHDR_IMM + 4]
            .copy_from_slice(&image_base.wrapping_add(52).to_le_bytes());
        prologue[TLS_PHNUM_IMM..TLS_PHNUM_IMM + 4]
            .copy_from_slice(&image_base.wrapping_add(44).to_le_bytes());
        code_bytes.extend_from_slice(&prologue);
    }
    let call_disp_offset = code_bytes.len() as u64 + 29;
    code_bytes.extend_from_slice(&[
        0x8b, 0x04, 0x24, // mov eax, [esp] (argc)
        0x8d, 0x54, 0x24, 0x04, // lea edx, [esp+4] (argv)
        0x8d, 0x4c, 0x84, 0x08, // lea ecx, [esp+eax*4+8] (envp)
        0x83, 0xe4, 0xf0, // and esp, -16
        0x83, 0xec, 0x10, // sub esp, 16
        0x89, 0x04, 0x24, // mov [esp], eax
        0x89, 0x54, 0x24, 0x04, // mov [esp+4], edx
        0x89, 0x4c, 0x24, 0x08, // mov [esp+8], ecx
        0xe8, 0x00, 0x00, 0x00, 0x00, // call <program_entry> (disp32)
        0x83, 0xc4, 0x10, // add esp, 16
        0x89, 0xc3, // mov ebx, eax
        0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1 (sys_exit)
        0xcd, 0x80, // int 0x80
        0xf4, // hlt
    ]);

    let relocs = vec![Relocation {
        offset: call_disp_offset,
        symbol_name: program_entry_symbol.to_string(),
        symbol_index: None,
        file_index: None,
        kind: RelocationKind::PcRelative32,
        addend: -4,
    }];

    let symbols = vec![Symbol {
        name: "_start".to_string(),
        binding: SymbolBinding::Global,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0,
        size: code_bytes.len() as u64,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    }];

    (code_bytes, relocs, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_stub_has_pc32_call_relocation_at_disp32() {
        let (bytes, relocs, symbols) = synthesize_elf_i686_entry("main", 7);
        assert_eq!(bytes.len(), 48);
        assert_eq!(bytes[30], 0xE8);
        assert_eq!(bytes[45], 0xCD);
        assert_eq!(bytes[46], 0x80);
        assert_eq!(relocs.len(), 1);
        assert_eq!(relocs[0].offset, 31);
        assert_eq!(relocs[0].kind, RelocationKind::PcRelative32);
        assert_eq!(relocs[0].addend, -4);
        assert_eq!(symbols[0].name, "_start");
        assert_eq!(symbols[0].file_index, Some(7));
    }

    #[test]
    fn tls_entry_stub_patches_image_base_and_shifts_the_call_relocation() {
        let (plain, _, _) = synthesize_elf_i686_entry("main", 0);
        let (bytes, relocs, symbols) = synthesize_elf_i686_entry_with_tls("main", 3, 0x0804_8000);
        assert_eq!(bytes.len(), plain.len() + TLS_PROLOGUE.len());
        assert_eq!(&bytes[..2], &[0x31, 0xed]);
        assert_eq!(&bytes[3..7], &0x0804_8034u32.to_le_bytes());
        assert_eq!(&bytes[10..14], &0x0804_802Cu32.to_le_bytes());
        let reloc = &relocs[0];
        assert_eq!(reloc.offset as usize, plain.len() - 17 + TLS_PROLOGUE.len());
        assert_eq!(bytes[reloc.offset as usize - 1], 0xE8);
        assert_eq!(reloc.kind, RelocationKind::PcRelative32);
        assert_eq!(symbols[0].size as usize, bytes.len());
        assert_eq!(&bytes[bytes.len() - 3..], &[0xCD, 0x80, 0xF4]);
    }
}
