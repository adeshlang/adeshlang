//! 16-bit MS-DOS `.COM` and `.MZ` EXE linker and freestanding runtime stubs.
//!
//! Emits pure Intel 8086 real-mode executables:
//! - `.COM`: flat 64 KiB single-segment binary starting at `CS:0100` (`CS=DS=SS=ES`).
//! - `.MZ`: relocatable 16-bit DOS `.EXE` with separate Code (`CS`), Data (`DS`/`ES`),
//!   and Stack (`SS`) segments, and an MZ header relocation fixing up `DS`/`ES` at startup.

#![forbid(unsafe_code)]

use crate::arch::i8086::I8086Arch;
use crate::config::LinkConfig;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};
use crate::section::SectionKind;
use crate::target::ObjectFormat;
use std::collections::{HashMap, HashSet};

fn emit_dos_putchar(out: &mut Vec<u8>, ch: u8) {
    out.extend_from_slice(&[0xB4, 0x02, 0xB2, ch, 0xCD, 0x21]);
}

fn emit_dos_newline_seq(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0xB4, 0x02, 0xB2, 13, 0xCD, 0x21, 0xB2, 10, 0xCD, 0x21]);
}

fn synthesize_dos_print_newline() -> Vec<u8> {
    let mut out = Vec::new();
    emit_dos_newline_seq(&mut out);
    out.push(0xC3);
    out
}

fn synthesize_dos_print_space() -> Vec<u8> {
    let mut out = Vec::new();
    emit_dos_putchar(&mut out, b' ');
    out.push(0xC3);
    out
}

fn synthesize_dos_print_cstr() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x55, 0x89, 0xE5, 0x56]); // push bp; mov bp, sp; push si
    out.extend_from_slice(&[0x8B, 0x76, 0x04]); // mov si, [bp+4]
    let loop_start = out.len();
    out.extend_from_slice(&[0x8A, 0x14]); // mov dl, [si]
    out.extend_from_slice(&[0x84, 0xD2]); // test dl, dl
    let jz_off = out.len();
    out.extend_from_slice(&[0x74, 0x00]); // jz check_nl
    out.extend_from_slice(&[0xB4, 0x02, 0xCD, 0x21]); // mov ah, 2; int 21h
    out.push(0x46); // inc si
    let jmp_back_rel = (loop_start as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0xEB, jmp_back_rel as u8]);
    let check_nl = out.len();
    out[jz_off + 1] = (check_nl - (jz_off + 2)) as u8;
    out.extend_from_slice(&[0x8B, 0x46, 0x06]); // mov ax, [bp+6]
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jz_done = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    emit_dos_newline_seq(&mut out);
    let done = out.len();
    out[jz_done + 1] = (done - (jz_done + 2)) as u8;
    out.extend_from_slice(&[0x5E, 0x5D, 0xC3]); // pop si; pop bp; ret
    out
}

fn synthesize_dos_print_int(signed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x55, 0x89, 0xE5, 0x53]); // push bp; mov bp, sp; push bx
    out.extend_from_slice(&[0x8B, 0x46, 0x04]); // mov ax, [bp+4]
    if signed {
        out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
        let jns_pos = out.len();
        out.extend_from_slice(&[0x79, 0x00]); // jns positive
        out.push(0x50); // push ax
        emit_dos_putchar(&mut out, b'-');
        out.push(0x58); // pop ax
        out.extend_from_slice(&[0xF7, 0xD8]); // neg ax
        let pos = out.len();
        out[jns_pos + 1] = (pos - (jns_pos + 2)) as u8;
    }
    out.extend_from_slice(&[0xB9, 0x00, 0x00]); // mov cx, 0
    out.extend_from_slice(&[0xBB, 0x0A, 0x00]); // mov bx, 10
    let div_loop = out.len();
    out.extend_from_slice(&[0x31, 0xD2]); // xor dx, dx
    out.extend_from_slice(&[0xF7, 0xF3]); // div bx
    out.extend_from_slice(&[0x80, 0xC2, b'0']); // add dl, '0'
    out.push(0x52); // push dx
    out.push(0x41); // inc cx
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jnz_rel = (div_loop as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0x75, jnz_rel as u8]);
    let print_loop = out.len();
    out.push(0x5A); // pop dx
    out.extend_from_slice(&[0xB4, 0x02, 0xCD, 0x21]); // mov ah, 2; int 21h
    let loop_rel = (print_loop as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0xE2, loop_rel as u8]); // loop print_loop
    out.extend_from_slice(&[0x8B, 0x46, 0x06]); // mov ax, [bp+6]
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jz_done = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    emit_dos_newline_seq(&mut out);
    let done = out.len();
    out[jz_done + 1] = (done - (jz_done + 2)) as u8;
    out.extend_from_slice(&[0x5B, 0x5D, 0xC3]); // pop bx; pop bp; ret
    out
}

fn synthesize_dos_print_int32(signed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    // push bp; mov bp, sp; push bx; push si
    out.extend_from_slice(&[0x55, 0x89, 0xE5, 0x53, 0x56]);
    out.extend_from_slice(&[0x8B, 0x46, 0x04]); // mov ax, [bp+4] (lo)
    out.extend_from_slice(&[0x8B, 0x56, 0x06]); // mov dx, [bp+6] (hi)
    if signed {
        out.extend_from_slice(&[0x85, 0xD2]); // test dx, dx
        let jns_pos = out.len();
        out.extend_from_slice(&[0x79, 0x00]); // jns positive
        out.extend_from_slice(&[0x50, 0x52]); // push ax; push dx
        emit_dos_putchar(&mut out, b'-');
        out.extend_from_slice(&[0x5A, 0x58]); // pop dx; pop ax
        out.extend_from_slice(&[0xF7, 0xD0, 0xF7, 0xD2]); // not ax; not dx
        out.extend_from_slice(&[0x05, 0x01, 0x00, 0x83, 0xD2, 0x00]); // add ax, 1; adc dx, 0
        let pos = out.len();
        out[jns_pos + 1] = (pos - (jns_pos + 2)) as u8;
    }
    out.extend_from_slice(&[0xBE, 0x00, 0x00]); // mov si, 0 (digit count)
    out.extend_from_slice(&[0xBB, 0x0A, 0x00]); // mov bx, 10
    let div_loop = out.len();
    out.extend_from_slice(&[0x89, 0xC1]); // mov cx, ax
    out.extend_from_slice(&[0x89, 0xD0]); // mov ax, dx
    out.extend_from_slice(&[0x31, 0xD2]); // xor dx, dx
    out.extend_from_slice(&[0xF7, 0xF3]); // div bx
    out.push(0x91); // xchg ax, cx
    out.extend_from_slice(&[0xF7, 0xF3]); // div bx
    out.extend_from_slice(&[0x80, 0xC2, b'0']); // add dl, '0'
    out.push(0x52); // push dx
    out.push(0x46); // inc si
    out.extend_from_slice(&[0x89, 0xCA]); // mov dx, cx
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jnz1 = out.len();
    out.extend_from_slice(&[0x75, 0x00]); // jnz div_loop
    out.extend_from_slice(&[0x85, 0xD2]); // test dx, dx
    let jnz2 = out.len();
    out.extend_from_slice(&[0x75, 0x00]); // jnz div_loop
    out[jnz1 + 1] = (div_loop as isize - (jnz1 as isize + 2)) as i8 as u8;
    out[jnz2 + 1] = (div_loop as isize - (jnz2 as isize + 2)) as i8 as u8;

    out.extend_from_slice(&[0x89, 0xF1]); // mov cx, si
    let print_loop = out.len();
    out.push(0x5A); // pop dx
    out.extend_from_slice(&[0xB4, 0x02, 0xCD, 0x21]); // mov ah, 2; int 21h
    let loop_rel = (print_loop as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0xE2, loop_rel as u8]); // loop print_loop

    out.extend_from_slice(&[0x8B, 0x46, 0x08]); // mov ax, [bp+8] (newline flag)
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jz_done = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    emit_dos_newline_seq(&mut out);
    let done = out.len();
    out[jz_done + 1] = (done - (jz_done + 2)) as u8;
    out.extend_from_slice(&[0x5E, 0x5B, 0x5D, 0xC3]); // pop si; pop bx; pop bp; ret
    out
}

fn synthesize_i8086_mul32() -> Vec<u8> {
    vec![
        0x55, 0x89, 0xE5, // push bp; mov bp, sp
        0x53, 0x56, // push bx; push si
        0x89, 0xC3, // mov bx, ax (lo0)
        0x89, 0xD6, // mov si, dx (hi0)
        0xF7, 0x66, 0x04, // mul word [bp+4] (dx:ax = lo0 * lo1)
        0x89, 0xD1, // mov cx, dx
        0x96, // xchg ax, si (ax = hi0, si = low_prod)
        0xF7, 0x6E, 0x04, // imul word [bp+4] (ax = hi0 * lo1)
        0x01, 0xC1, // add cx, ax
        0x89, 0xD8, // mov ax, bx (lo0)
        0xF7, 0x6E, 0x06, // imul word [bp+6] (ax = lo0 * hi1)
        0x01, 0xC1, // add cx, ax
        0x89, 0xF0, // mov ax, si
        0x89, 0xCA, // mov dx, cx
        0x5E, 0x5B, 0x5D, // pop si; pop bx; pop bp
        0xC2, 0x04, 0x00, // ret 4
    ]
}

fn synthesize_i8086_shift32(op: u8) -> Vec<u8> {
    // op: 0 = shl, 1 = shr, 2 = sar
    let mut out = vec![
        0x55, 0x89, 0xE5, // push bp; mov bp, sp
        0x8B, 0x4E, 0x04, // mov cx, [bp+4]
        0x83, 0xE1, 0x1F, // and cx, 31
        0xE3, 0x06, // jcxz +6 (done)
    ];
    match op {
        0 => out.extend_from_slice(&[0xD1, 0xE0, 0xD1, 0xD2]), // shl ax, 1; rcl dx, 1
        1 => out.extend_from_slice(&[0xD1, 0xEA, 0xD1, 0xD8]), // shr dx, 1; rcr ax, 1
        _ => out.extend_from_slice(&[0xD1, 0xFA, 0xD1, 0xD8]), // sar dx, 1; rcr ax, 1
    }
    out.extend_from_slice(&[0xE2, 0xFA]); // loop -6
    out.extend_from_slice(&[0x5D, 0xC2, 0x04, 0x00]); // pop bp; ret 4
    out
}

fn synthesize_i8086_divmod32(is_mod: bool) -> Vec<u8> {
    let mut out = Vec::new();
    // push bp; mov bp, sp; push bx; push si; push di
    out.extend_from_slice(&[0x55, 0x89, 0xE5, 0x53, 0x56, 0x57]);
    // Check divisor == 0
    out.extend_from_slice(&[0x8B, 0x4E, 0x04, 0x0B, 0x4E, 0x06]); // mov cx, [bp+4]; or cx, [bp+6]
    let jnz_nonzero = out.len();
    out.extend_from_slice(&[0x75, 0x00]);
    out.extend_from_slice(&[0x31, 0xC0, 0x31, 0xD2]); // xor ax, ax; xor dx, dx
    let jmp_ret_zero = out.len();
    out.extend_from_slice(&[0xE9, 0x00, 0x00]);
    let nonzero_off = out.len();
    out[jnz_nonzero + 1] = (nonzero_off - (jnz_nonzero + 2)) as u8;

    // BH = hi0 ^ hi1 (quot sign), BL = hi0 (rem sign)
    out.extend_from_slice(&[0x88, 0xF3]); // mov bl, dh (hi0 >> 8)
    out.extend_from_slice(&[0x8A, 0x7E, 0x07]); // mov bh, [bp+7] (hi1 >> 8)
    out.extend_from_slice(&[0x30, 0xF7]); // xor bh, dh

    // If DX < 0, negate DX:AX
    out.extend_from_slice(&[0x85, 0xD2]); // test dx, dx
    let jns_dividend = out.len();
    out.extend_from_slice(&[0x79, 0x00]);
    out.extend_from_slice(&[0xF7, 0xD0, 0xF7, 0xD2]); // not ax; not dx
    out.extend_from_slice(&[0x05, 0x01, 0x00, 0x83, 0xD2, 0x00]); // add ax, 1; adc dx, 0
    let pos_dividend = out.len();
    out[jns_dividend + 1] = (pos_dividend - (jns_dividend + 2)) as u8;

    // If [bp+6] < 0, negate [bp+6]:[bp+4] in place
    out.extend_from_slice(&[0x83, 0x7E, 0x06, 0x00]); // cmp word [bp+6], 0
    let jnl_divisor = out.len();
    out.extend_from_slice(&[0x7D, 0x00]);
    out.extend_from_slice(&[0xF7, 0x56, 0x04, 0xF7, 0x56, 0x06]); // not word [bp+4]; not word [bp+6]
    out.extend_from_slice(&[0x83, 0x46, 0x04, 0x01, 0x83, 0x56, 0x06, 0x00]); // add word [bp+4], 1; adc word [bp+6], 0
    let pos_divisor = out.len();
    out[jnl_divisor + 1] = (pos_divisor - (jnl_divisor + 2)) as u8;

    // SI:DI = 0 (remainder), CX = 32
    out.extend_from_slice(&[0x31, 0xF6, 0x31, 0xFF]); // xor si, si; xor di, di
    out.extend_from_slice(&[0xB9, 0x20, 0x00]); // mov cx, 32
    let loop_top = out.len();
    out.extend_from_slice(&[0xD1, 0xE0, 0xD1, 0xD2]); // shl ax, 1; rcl dx, 1
    out.extend_from_slice(&[0xD1, 0xD7, 0xD1, 0xD6]); // rcl di, 1; rcl si, 1
    out.extend_from_slice(&[0x3B, 0x76, 0x06]); // cmp si, [bp+6]
    let jb_skip1 = out.len();
    out.extend_from_slice(&[0x72, 0x00]);
    let ja_sub = out.len();
    out.extend_from_slice(&[0x77, 0x00]);
    out.extend_from_slice(&[0x3B, 0x7E, 0x04]); // cmp di, [bp+4]
    let jb_skip2 = out.len();
    out.extend_from_slice(&[0x72, 0x00]);
    let do_sub = out.len();
    out[ja_sub + 1] = (do_sub - (ja_sub + 2)) as u8;
    out.extend_from_slice(&[0x2B, 0x7E, 0x04, 0x1B, 0x76, 0x06]); // sub di, [bp+4]; sbb si, [bp+6]
    out.push(0x40); // inc ax
    let skip_off = out.len();
    out[jb_skip1 + 1] = (skip_off - (jb_skip1 + 2)) as u8;
    out[jb_skip2 + 1] = (skip_off - (jb_skip2 + 2)) as u8;
    let loop_rel = (loop_top as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0xE2, loop_rel as u8]); // loop loop_top

    if is_mod {
        out.extend_from_slice(&[0x89, 0xF8, 0x89, 0xF2]); // mov ax, di; mov dx, si
        out.extend_from_slice(&[0x84, 0xDB]); // test bl, bl
    } else {
        out.extend_from_slice(&[0x84, 0xFF]); // test bh, bh
    }
    let jns_done = out.len();
    out.extend_from_slice(&[0x79, 0x00]);
    out.extend_from_slice(&[0xF7, 0xD0, 0xF7, 0xD2]); // not ax; not dx
    out.extend_from_slice(&[0x05, 0x01, 0x00, 0x83, 0xD2, 0x00]); // add ax, 1; adc dx, 0
    let done_off = out.len();
    out[jns_done + 1] = (done_off - (jns_done + 2)) as u8;
    let jmp_rel = (done_off as isize - (jmp_ret_zero as isize + 3)) as i16;
    out[jmp_ret_zero + 1..jmp_ret_zero + 3].copy_from_slice(&jmp_rel.to_le_bytes());

    out.extend_from_slice(&[0x5F, 0x5E, 0x5B, 0x5D, 0xC2, 0x04, 0x00]); // pop di; pop si; pop bx; pop bp; ret 4
    out
}

fn synthesize_dos_print_bool() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x55, 0x89, 0xE5]); // push bp; mov bp, sp
    out.extend_from_slice(&[0x8B, 0x46, 0x04]); // mov ax, [bp+4]
    out.extend_from_slice(&[0x85, 0xC0]); // test ax, ax
    let jz_false = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    for &b in b"true" {
        emit_dos_putchar(&mut out, b);
    }
    let jmp_nl = out.len();
    out.extend_from_slice(&[0xEB, 0x00]);
    let false_lbl = out.len();
    out[jz_false + 1] = (false_lbl - (jz_false + 2)) as u8;
    for &b in b"false" {
        emit_dos_putchar(&mut out, b);
    }
    let nl_lbl = out.len();
    out[jmp_nl + 1] = (nl_lbl - (jmp_nl + 2)) as u8;
    out.extend_from_slice(&[0x8B, 0x46, 0x06]); // mov ax, [bp+6]
    out.extend_from_slice(&[0x85, 0xC0]);
    let jz_done = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    emit_dos_newline_seq(&mut out);
    let done = out.len();
    out[jz_done + 1] = (done - (jz_done + 2)) as u8;
    out.extend_from_slice(&[0x5D, 0xC3]); // pop bp; ret
    out
}

fn synthesize_dos_print_null() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x55, 0x89, 0xE5]); // push bp; mov bp, sp
    for &b in b"null" {
        emit_dos_putchar(&mut out, b);
    }
    out.extend_from_slice(&[0x8B, 0x46, 0x04]); // mov ax, [bp+4]
    out.extend_from_slice(&[0x85, 0xC0]);
    let jz_done = out.len();
    out.extend_from_slice(&[0x74, 0x00]);
    emit_dos_newline_seq(&mut out);
    let done = out.len();
    out[jz_done + 1] = (done - (jz_done + 2)) as u8;
    out.extend_from_slice(&[0x5D, 0xC3]); // pop bp; ret
    out
}

fn synthesize_dos_abort_str() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x55, 0x89, 0xE5]); // push bp; mov bp, sp
    out.extend_from_slice(&[0x8B, 0x76, 0x04]); // mov si, [bp+4]
    let loop_start = out.len();
    out.extend_from_slice(&[0x8A, 0x14]); // mov dl, [si]
    out.extend_from_slice(&[0x84, 0xD2]); // test dl, dl
    let jz_off = out.len();
    out.extend_from_slice(&[0x74, 0x00]); // jz done
    out.extend_from_slice(&[0xB4, 0x02, 0xCD, 0x21]); // mov ah, 2; int 21h
    out.push(0x46); // inc si
    let jmp_back_rel = (loop_start as isize - (out.len() as isize + 2)) as i8;
    out.extend_from_slice(&[0xEB, jmp_back_rel as u8]);
    let done = out.len();
    out[jz_off + 1] = (done - (jz_off + 2)) as u8;
    emit_dos_newline_seq(&mut out);
    out.extend_from_slice(&[0xB8, 0x01, 0x4C, 0xCD, 0x21]); // mov ax, 4C01h; int 21h
    out
}

fn synthesize_dos_helper(name: &str) -> Option<Vec<u8>> {
    match name {
        "aot_print_newline" | "dos_print_newline" => Some(synthesize_dos_print_newline()),
        "aot_print_space" | "dos_print_space" => Some(synthesize_dos_print_space()),
        "aot_print_cstr" | "dos_print_cstr" => Some(synthesize_dos_print_cstr()),
        "aot_print_i64" | "dos_print_i16" => Some(synthesize_dos_print_int(true)),
        "aot_print_u64" | "dos_print_u16" => Some(synthesize_dos_print_int(false)),
        "aot_print_i32" => Some(synthesize_dos_print_int32(true)),
        "aot_print_u32" => Some(synthesize_dos_print_int32(false)),
        "__i8086_mul32" => Some(synthesize_i8086_mul32()),
        "__i8086_div32" => Some(synthesize_i8086_divmod32(false)),
        "__i8086_mod32" => Some(synthesize_i8086_divmod32(true)),
        "__i8086_shl32" => Some(synthesize_i8086_shift32(0)),
        "__i8086_shr32" => Some(synthesize_i8086_shift32(1)),
        "__i8086_sar32" => Some(synthesize_i8086_shift32(2)),
        "aot_print_bool" | "dos_print_bool" => Some(synthesize_dos_print_bool()),
        "aot_print_null" | "dos_print_null" => Some(synthesize_dos_print_null()),
        "aot_abort_str" => Some(synthesize_dos_abort_str()),
        _ => None,
    }
}

pub fn is_dos_runtime_helper(name: &str) -> bool {
    synthesize_dos_helper(name).is_some()
}

pub fn effective_dos_format(config: &LinkConfig) -> ObjectFormat {
    match config
        .output_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("com") => ObjectFormat::DosCom,
        Some("exe") => ObjectFormat::DosMz,
        _ => config.target.format,
    }
}

/// Link `objects` into a real-mode 8086 MS-DOS `.COM` or `.MZ` `.EXE` executable.
pub fn emit_dos_executable(
    objects: &[ObjectFile],
    config: &LinkConfig,
    entry_symbol: &str,
) -> LinkResult<()> {
    let format = effective_dos_format(config);
    let is_com = format == ObjectFormat::DosCom;

    let mut text_data = Vec::new();
    let mut data_data = Vec::new();
    let mut bss_size: usize = 0;

    // Synthesize `_start` first at offset 0 of `.text`.
    let start_stub_len = if is_com { 7 } else { 12 };
    if is_com {
        // call rel16 <entry>; mov ah, 0x4C; int 0x21
        text_data.extend_from_slice(&[0xE8, 0x00, 0x00, 0xB4, 0x4C, 0xCD, 0x21]);
    } else {
        // mov ax, <data_para>; mov ds, ax; mov es, ax; call rel16 <entry>; mov ah, 0x4C; int 0x21
        text_data.extend_from_slice(&[
            0xB8, 0x00, 0x00, 0x8E, 0xD8, 0x8E, 0xC0, 0xE8, 0x00, 0x00, 0xB4, 0x4C, 0xCD, 0x21,
        ]);
    }
    let _ = start_stub_len;
    let entry_call_disp_off: usize = if is_com { 1 } else { 8 };

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SecRegion {
        Text,
        Data,
        Bss,
    }

    struct PendingReloc {
        region: SecRegion,
        offset: usize,
        reloc: Relocation,
        file_idx: usize,
    }

    let mut sec_placements: HashMap<(usize, usize), (SecRegion, usize)> = HashMap::new();
    let mut local_symbols: HashMap<(usize, String), (SecRegion, usize)> = HashMap::new();
    let mut global_symbols: HashMap<String, (SecRegion, usize)> = HashMap::new();
    let mut pending_relocs: Vec<PendingReloc> = Vec::new();
    let mut referenced_symbols: HashSet<String> = HashSet::new();
    referenced_symbols.insert(entry_symbol.to_string());

    for (file_idx, obj) in objects.iter().enumerate() {
        for (sec_idx, sec) in obj.sections.iter().enumerate() {
            match sec.kind {
                SectionKind::Text => {
                    let align = (sec.alignment as usize).max(1);
                    let aligned = (text_data.len() + align - 1) & !(align - 1);
                    text_data.resize(aligned, 0x90);
                    let base = text_data.len();
                    text_data.extend_from_slice(&sec.data);
                    sec_placements.insert((file_idx, sec_idx), (SecRegion::Text, base));
                    for r in &sec.relocations {
                        referenced_symbols.insert(r.symbol_name.clone());
                        pending_relocs.push(PendingReloc {
                            region: SecRegion::Text,
                            offset: base + r.offset as usize,
                            reloc: r.clone(),
                            file_idx,
                        });
                    }
                }
                SectionKind::Rodata | SectionKind::Data => {
                    let align = (sec.alignment as usize).max(1);
                    let aligned = (data_data.len() + align - 1) & !(align - 1);
                    data_data.resize(aligned, 0);
                    let base = data_data.len();
                    data_data.extend_from_slice(&sec.data);
                    sec_placements.insert((file_idx, sec_idx), (SecRegion::Data, base));
                    for r in &sec.relocations {
                        referenced_symbols.insert(r.symbol_name.clone());
                        pending_relocs.push(PendingReloc {
                            region: SecRegion::Data,
                            offset: base + r.offset as usize,
                            reloc: r.clone(),
                            file_idx,
                        });
                    }
                }
                SectionKind::Bss => {
                    let align = (sec.alignment as usize).max(1);
                    bss_size = (bss_size + align - 1) & !(align - 1);
                    let base = bss_size;
                    bss_size += sec.size as usize;
                    sec_placements.insert((file_idx, sec_idx), (SecRegion::Bss, base));
                }
                _ => {}
            }
        }

        for sym in &obj.symbols {
            let Some(sec_idx) = sym.section_index else {
                continue;
            };
            let Some(&(region, base)) = sec_placements.get(&(file_idx, sec_idx)) else {
                continue;
            };
            let off = base + sym.value as usize;
            if sym.is_local() {
                local_symbols.insert((file_idx, sym.name.clone()), (region, off));
            } else {
                global_symbols
                    .entry(sym.name.clone())
                    .or_insert((region, off));
            }
        }
    }

    // Synthesize any referenced minimal DOS print helpers not provided by user objects.
    let mut missing_helpers: Vec<String> = referenced_symbols
        .iter()
        .filter(|name| !global_symbols.contains_key(*name))
        .filter(|name| is_dos_runtime_helper(name))
        .cloned()
        .collect();
    missing_helpers.sort();
    for helper in missing_helpers {
        if let Some(code) = synthesize_dos_helper(&helper) {
            let off = text_data.len();
            text_data.extend_from_slice(&code);
            global_symbols.insert(helper, (SecRegion::Text, off));
        }
    }

    if !global_symbols.contains_key(entry_symbol) {
        return Err(LinkError::entry_point_not_found(
            entry_symbol,
            &objects
                .iter()
                .map(|o| o.path.display().to_string())
                .collect::<Vec<_>>(),
        ));
    }

    // Verify all referenced symbols exist before patching.
    for pr in &pending_relocs {
        let found = local_symbols.contains_key(&(pr.file_idx, pr.reloc.symbol_name.clone()))
            || global_symbols.contains_key(&pr.reloc.symbol_name);
        if !found {
            return Err(LinkError::undefined_symbol(
                &pr.reloc.symbol_name,
                "undefined symbol in 16-bit DOS link",
                None,
                None,
            ));
        }
    }

    let handler = I8086Arch;
    let out_bytes = if is_com {
        let origin = 0x0100usize;
        let data_base = origin + text_data.len();
        let bss_base = data_base + data_data.len();
        let total_mem = data_base + data_data.len() + bss_size;
        if total_mem > 0xFE00 {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                format!(
                    "DOS .COM image size ({total_mem} bytes) exceeds 64 KiB single-segment limit; \
                     use a .exe (DOS MZ) output path instead"
                ),
            ));
        }

        let mut image = Vec::with_capacity(text_data.len() + data_data.len());
        image.extend_from_slice(&text_data);
        image.extend_from_slice(&data_data);

        let resolve_com_va = |region: SecRegion, off: usize| -> u64 {
            match region {
                SecRegion::Text => (origin + off) as u64,
                SecRegion::Data => (data_base + off) as u64,
                SecRegion::Bss => (bss_base + off) as u64,
            }
        };

        // Patch `_start` -> `call entry_symbol`
        let (entry_reg, entry_off) = global_symbols[entry_symbol];
        let entry_va = resolve_com_va(entry_reg, entry_off);
        handler.apply(
            &Relocation::new(
                entry_call_disp_off as u64,
                entry_symbol,
                RelocationKind::PcRelative16,
                -2,
            ),
            (origin + entry_call_disp_off) as u64,
            entry_va,
            -2,
            &mut image,
        )?;

        for pr in &pending_relocs {
            let (sym_reg, sym_off) = local_symbols
                .get(&(pr.file_idx, pr.reloc.symbol_name.clone()))
                .or_else(|| global_symbols.get(&pr.reloc.symbol_name))
                .copied()
                .expect("checked above");
            let img_off = match pr.region {
                SecRegion::Text => pr.offset,
                SecRegion::Data => text_data.len() + pr.offset,
                SecRegion::Bss => continue,
            };
            let place_va = origin as u64 + img_off as u64;
            let sym_va = resolve_com_va(sym_reg, sym_off);
            let mut patched_reloc = pr.reloc.clone();
            patched_reloc.offset = img_off as u64;
            handler.apply(
                &patched_reloc,
                place_va,
                sym_va,
                pr.reloc.addend,
                &mut image,
            )?;
        }
        image
    } else {
        // DOS MZ .EXE (Small Model):
        // - Code segment at paragraph 0 (`CS = LOAD_SEG`, up to 64 KiB).
        // - DGROUP at `data_para` (`DS = ES = SS = LOAD_SEG + data_para`, up to 64 KiB)
        //   so near 16-bit pointers seamlessly address both globals and stack locals.
        let text_padded_len = (text_data.len() + 15) & !15;
        text_data.resize(text_padded_len, 0x90);
        let data_para = (text_padded_len / 16) as u16;
        let data_total = data_data.len() + bss_size;
        if text_padded_len > 0xFFFF || data_total + 0x0400 > 0xFFFE {
            return Err(LinkError::new(
                ErrorCode::RelocationOverflow,
                "8086 DOS MZ segment exceeds 64 KiB limit",
            ));
        }
        let ss_rel_seg = data_para;
        let sp_init: u16 = ((data_total + 0x1000).min(0xFFFE) & !1) as u16;

        // Patch `mov ax, data_para` at CS:0001 (relocated by MZ loader)
        text_data[1..3].copy_from_slice(&data_para.to_le_bytes());

        // Patch `_start` -> `call entry_symbol`
        let (entry_reg, entry_off) = global_symbols[entry_symbol];
        if entry_reg != SecRegion::Text {
            return Err(LinkError::new(
                ErrorCode::EntryPointNotFound,
                format!("entry symbol `{entry_symbol}` is not in executable code"),
            ));
        }
        handler.apply(
            &Relocation::new(
                entry_call_disp_off as u64,
                entry_symbol,
                RelocationKind::PcRelative16,
                -2,
            ),
            entry_call_disp_off as u64,
            entry_off as u64,
            -2,
            &mut text_data,
        )?;

        for pr in &pending_relocs {
            let (sym_reg, sym_off) = local_symbols
                .get(&(pr.file_idx, pr.reloc.symbol_name.clone()))
                .or_else(|| global_symbols.get(&pr.reloc.symbol_name))
                .copied()
                .expect("checked above");
            let sym_va = match sym_reg {
                SecRegion::Text => sym_off as u64,
                SecRegion::Data => sym_off as u64,
                SecRegion::Bss => (data_data.len() + sym_off) as u64,
            };
            let mut r = pr.reloc.clone();
            r.offset = pr.offset as u64;
            match pr.region {
                SecRegion::Text => {
                    handler.apply(
                        &r,
                        pr.offset as u64,
                        sym_va,
                        pr.reloc.addend,
                        &mut text_data,
                    )?;
                }
                SecRegion::Data => {
                    handler.apply(
                        &r,
                        pr.offset as u64,
                        sym_va,
                        pr.reloc.addend,
                        &mut data_data,
                    )?;
                }
                SecRegion::Bss => {}
            }
        }

        let mut payload = Vec::with_capacity(text_data.len() + data_data.len());
        payload.extend_from_slice(&text_data);
        payload.extend_from_slice(&data_data);

        let hdr_bytes = 32usize; // 2 paragraphs: 28-byte MZ header + 1 reloc entry (4 bytes)
        let total_file_bytes = hdr_bytes + payload.len();
        let pages = ((total_file_bytes + 511) / 512) as u16;
        let last_page_bytes = (total_file_bytes % 512) as u16;
        let extra_dgroup_bytes = (sp_init as usize).saturating_sub(data_data.len());
        let min_alloc = ((extra_dgroup_bytes + 15) / 16) as u16;

        let mut mz = Vec::with_capacity(total_file_bytes);
        mz.extend_from_slice(&0x5A4Du16.to_le_bytes()); // e_magic = "MZ"
        mz.extend_from_slice(&last_page_bytes.to_le_bytes()); // e_cblp
        mz.extend_from_slice(&pages.to_le_bytes()); // e_cp
        mz.extend_from_slice(&1u16.to_le_bytes()); // e_crlc (1 reloc)
        mz.extend_from_slice(&2u16.to_le_bytes()); // e_cparhdr (32 bytes = 2 paras)
        mz.extend_from_slice(&min_alloc.to_le_bytes()); // e_minalloc
        mz.extend_from_slice(&0xFFFFu16.to_le_bytes()); // e_maxalloc
        mz.extend_from_slice(&ss_rel_seg.to_le_bytes()); // e_ss
        mz.extend_from_slice(&sp_init.to_le_bytes()); // e_sp
        mz.extend_from_slice(&0u16.to_le_bytes()); // e_csum
        mz.extend_from_slice(&0u16.to_le_bytes()); // e_ip (0)
        mz.extend_from_slice(&0u16.to_le_bytes()); // e_cs (0)
        mz.extend_from_slice(&0x001Cu16.to_le_bytes()); // e_lfarlc (28)
        mz.extend_from_slice(&0u16.to_le_bytes()); // e_ovno
        // MZ relocation entry #0: CS:0001 (`mov ax, data_para`)
        mz.extend_from_slice(&0x0001u16.to_le_bytes());
        mz.extend_from_slice(&0x0000u16.to_le_bytes());
        debug_assert_eq!(mz.len(), 32);
        mz.extend_from_slice(&payload);
        mz
    };

    if let Some(parent) = config.output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&config.output_path, &out_bytes)?;
    Ok(())
}
