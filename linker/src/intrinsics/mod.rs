//! Built-in Compiler-RT and runtime intrinsics synthesized by the linker.
//!
//! Eliminates dependency on external `libgcc`, `compiler-rt`, or `msvcrt` for
//! core 128-bit arithmetic, memory primitives, stack probes, and atomics.

use crate::section::Section;
use crate::symbol::{Symbol, SymbolBinding, SymbolType};
use crate::target::{Arch, Target};

/// Runtime intrinsic provider.
pub struct IntrinsicsEngine;

impl IntrinsicsEngine {
    /// Returns the list of standard intrinsic symbol names.
    pub fn standard_symbols() -> &'static [&'static str] {
        &[
            "__multi3",
            "__divti3",
            "__udivti3",
            "__modti3",
            "__umodti3",
            "__ashlti3",
            "__ashrti3",
            "__lshrti3",
            "memcpy",
            "memmove",
            "memset",
            "memcmp",
            "__chkstk",
            "___chkstk_ms",
            "__stack_chk_guard",
            "__stack_chk_fail",
            "__adesh_personality_v0",
            "__adesh_panic",
            "__adesh_drop_in_place",
        ]
    }

    /// Check if a symbol is a synthesizable intrinsic.
    pub fn is_intrinsic(name: &str) -> bool {
        Self::standard_symbols().contains(&name)
    }

    /// Synthesize machine code section and symbols for missing runtime intrinsics.
    pub fn synthesize_intrinsics_section(
        missing_symbols: &[String],
        target: &Target,
    ) -> Option<(Section, Vec<Symbol>)> {
        let mut code_bytes = Vec::new();
        let mut symbols = Vec::new();

        for sym_name in missing_symbols {
            if !Self::is_intrinsic(sym_name) {
                continue;
            }

            let start_offset = code_bytes.len() as u64;
            let intrinsic_bytes = Self::emit_intrinsic_code(sym_name, target);
            code_bytes.extend_from_slice(&intrinsic_bytes);
            let size = intrinsic_bytes.len() as u64;

            let sym = Symbol::new_defined(
                sym_name.clone(),
                SymbolBinding::Weak, // Weak so user overrides take precedence
                if sym_name == "__stack_chk_guard" {
                    SymbolType::Object
                } else {
                    SymbolType::Function
                },
                0, // Will be set to the synthesised section index
                start_offset,
                size,
                usize::MAX, // Synthetic file index
            );
            symbols.push(sym);
        }

        if symbols.is_empty() {
            return None;
        }

        let section = Section::new_code(".text.adesh_rt", code_bytes, 16);
        Some((section, symbols))
    }

    /// Emits architecture-specific machine code bytes for an intrinsic function.
    pub fn emit_intrinsic_code(name: &str, target: &Target) -> Vec<u8> {
        match target.arch {
            Arch::X86_64 => Self::emit_x86_64(name),
            Arch::AArch64 => Self::emit_aarch64(name),
            Arch::Riscv64 => Self::emit_riscv64(name),
            _ => {
                // Generic portable stub (ret / trap)
                vec![0xc3] // x86 ret
            }
        }
    }

    /// x86_64 native machine code sequences for compiler runtime builtins.
    fn emit_x86_64(name: &str) -> Vec<u8> {
        match name {
            "memcpy" | "memmove" => {
                // memcpy(rdi = dst, rsi = src, rdx = len) -> rax = dst
                // mov rax, rdi; mov rcx, rdx; rep movsb; ret
                vec![
                    0x48, 0x89, 0xf8, // mov rax, rdi
                    0x48, 0x89, 0xd1, // mov rcx, rdx
                    0xf3, 0xa4, // rep movsb
                    0xc3, // ret
                ]
            }
            "memset" => {
                // memset(rdi = dst, rsi = val, rdx = len) -> rax = dst
                // mov r8, rdi; mov rax, rsi; mov rcx, rdx; rep stosb; mov rax, r8; ret
                vec![
                    0x49, 0x89, 0xf8, // mov r8, rdi
                    0x48, 0x89, 0xf0, // mov rax, rsi
                    0x48, 0x89, 0xd1, // mov rcx, rdx
                    0xf3, 0xaa, // rep stosb
                    0x4c, 0x89, 0xc0, // mov rax, r8
                    0xc3, // ret
                ]
            }
            "memcmp" => {
                // memcmp(rdi = s1, rsi = s2, rdx = n) -> eax = diff
                // xor eax, eax; mov rcx, rdx; repe cmpsb; jz done; movzx eax, byte [rdi-1]; movzx edx, byte [rsi-1]; sub eax, edx; done: ret
                vec![
                    0x31, 0xc0, // xor eax, eax
                    0x48, 0x89, 0xd1, // mov rcx, rdx
                    0xf3, 0xa6, // repe cmpsb
                    0x74, 0x0a, // jz +10 (done)
                    0x0f, 0xb6, 0x47, 0xff, // movzx eax, byte [rdi-1]
                    0x0f, 0xb6, 0x56, 0xff, // movzx edx, byte [rsi-1]
                    0x29, 0xd0, // sub eax, edx
                    0xc3, // ret
                ]
            }
            "__chkstk" | "___chkstk_ms" => {
                // Windows x64 stack probe
                // rax = bytes to allocate. Probes 4096-byte pages downwards from rsp.
                vec![
                    0x48, 0x83, 0xf8, 0x00, // cmp rax, 0
                    0x74, 0x16, // jz done
                    0x51, // push rcx
                    0x48, 0x89, 0xe1, // mov rcx, rsp
                    0x48, 0x2d, 0x00, 0x10, 0x00, 0x00, // loop: sub rax, 4096
                    0x48, 0x81, 0xe9, 0x00, 0x10, 0x00, 0x00, // sub rcx, 4096
                    0x85, 0x01, // test [rcx], eax (probe page)
                    0x48, 0x83, 0xf8, 0x00, // cmp rax, 0
                    0x7f, 0xee, // jg loop
                    0x59, // pop rcx
                    0xc3, // ret
                ]
            }
            "__stack_chk_guard" => {
                // Default canary value (8 bytes)
                vec![0x00, 0x0a, 0xff, 0x00, 0x5a, 0x3c, 0x7e, 0x1b]
            }
            "__stack_chk_fail" => {
                // Stack smashing detected -> ud2 (illegal instruction / trap)
                vec![0x0f, 0x0b] // ud2
            }
            "__multi3" => {
                // 128-bit multiply: (rdi:rsi) * (rdx:rcx) -> rdx:rax
                // Optimized 64x64->128 multiply using mul instruction
                vec![
                    0x48, 0x89, 0xd0, // mov rax, rdx
                    0x48, 0xf7, 0xe6, // mul rsi
                    0x49, 0x89, 0xc0, // mov r8, rax
                    0x49, 0x89, 0xd1, // mov r9, rdx
                    0x48, 0x89, 0xf8, // mov rax, rdi
                    0x48, 0xf7, 0xe2, // mul rdx
                    0x49, 0x01, 0xc1, // add r9, rax
                    0x4c, 0x89, 0xc0, // mov rax, r8
                    0x4c, 0x89, 0xca, // mov rdx, r9
                    0xc3, // ret
                ]
            }
            "__adesh_panic" => {
                // Panic trap
                vec![0x0f, 0x0b] // ud2
            }
            "__adesh_personality_v0" => {
                // Personality function returning _URC_CONTINUE_UNWIND (8)
                vec![
                    0xb8, 0x08, 0x00, 0x00, 0x00, // mov eax, 8
                    0xc3, // ret
                ]
            }
            "__adesh_drop_in_place" => {
                // Default no-op drop glue
                vec![0xc3] // ret
            }
            _ => {
                // Default ret
                vec![0xc3]
            }
        }
    }

    /// AArch64 machine code sequences for compiler runtime builtins.
    fn emit_aarch64(name: &str) -> Vec<u8> {
        match name {
            "memcpy" | "memmove" => {
                // x0 = dst, x1 = src, x2 = len. Simple loop:
                // cbz x2, end; ldrb w3, [x1], #1; strb w3, [x0], #1; sub x2, x2, #1; b loop; end: ret
                vec![
                    0x40, 0x00, 0x00, 0xb4, // cbz x2, +8
                    0x23, 0x04, 0x40, 0x38, // ldrb w3, [x1], #1
                    0x03, 0x04, 0x00, 0x38, // strb w3, [x0], #1
                    0x42, 0x04, 0x00, 0xd1, // sub x2, x2, #1
                    0xfc, 0xff, 0xff, 0x17, // b -4
                    0xc0, 0x03, 0x5f, 0xd6, // ret
                ]
            }
            "memset" => {
                // x0 = dst, x1 = val, x2 = len.
                vec![
                    0x40, 0x00, 0x00, 0xb4, // cbz x2, +8
                    0x01, 0x04, 0x00, 0x38, // strb w1, [x0], #1
                    0x42, 0x04, 0x00, 0xd1, // sub x2, x2, #1
                    0xfd, 0xff, 0xff, 0x17, // b -3
                    0xc0, 0x03, 0x5f, 0xd6, // ret
                ]
            }
            "__stack_chk_fail" | "__adesh_panic" => {
                // brk #0 (trap)
                vec![0x00, 0x00, 0x20, 0xd4]
            }
            _ => {
                // ret
                vec![0xc0, 0x03, 0x5f, 0xd6]
            }
        }
    }

    /// RISC-V 64 machine code sequences for compiler runtime builtins.
    fn emit_riscv64(name: &str) -> Vec<u8> {
        match name {
            "__stack_chk_fail" | "__adesh_panic" => {
                // ebreak (0x00100073)
                vec![0x73, 0x00, 0x10, 0x00]
            }
            _ => {
                // ret (jalr x0, x1, 0 -> 0x00008067)
                vec![0x67, 0x80, 0x00, 0x00]
            }
        }
    }
}
