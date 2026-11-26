//! 8-bit AVR (ATmega328P `avr5`) bare-metal linker, crt0 startup, runtime helpers,
//! ELF32 (`EM_AVR = 83`) writer, and Intel HEX (`.hex`) emitter.
//!
//! Harvard memory model (ATmega328P):
//! - Program Flash: `0x0000..0x7FFF` (32 KiB, byte-addressed for `LPM`, word-addressed for `CALL`/`JMP`/`ICALL`)
//!   Layout: `[26-entry IVT | __ctors_end crt0 | .text + helpers | .rodata | .data LMA]`
//! - Data SRAM:     `0x0100..0x08FF` (2 KiB, ELF VMA `0x00800100..0x008008FF`)
//!   Layout: `[.data | .bss | ... free RAM ... <- SP (0x08FF downward)]`

#![forbid(unsafe_code)]

use crate::arch::avr::AvrArch;
use crate::config::LinkConfig;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};
use crate::section::SectionKind;
use std::collections::{HashMap, HashSet};

const ATMEGA328P_FLASH_SIZE: usize = 32 * 1024;
const ATMEGA328P_SRAM_START: usize = 0x0100;
const ATMEGA328P_SRAM_END: usize = 0x0900; // RAMEND = 0x08FF (exclusive end = 0x0900)
const AVR_ELF_SRAM_TAG: u32 = 0x0080_0000;
const EM_AVR: u16 = 83;
const EF_AVR_ARCH_AVR5: u32 = 0x05;

const SREG_IO: u8 = 0x3F;
const SPL_IO: u8 = 0x3D;
const SPH_IO: u8 = 0x3E;

// ATmega328P UART0 memory-mapped registers (SRAM/IO data space)
const UCSR0A_ADDR: u16 = 0x00C0;
const UCSR0B_ADDR: u16 = 0x00C1;
const UCSR0C_ADDR: u16 = 0x00C2;
const UBRR0L_ADDR: u16 = 0x00C4;
const UBRR0H_ADDR: u16 = 0x00C5;
const UDR0_ADDR: u16 = 0x00C6;

struct AvrAsm {
    code: Vec<u8>,
    labels: HashMap<&'static str, usize>,
    patches: Vec<(usize, &'static str, PatchKind)>,
}

#[derive(Clone, Copy)]
enum PatchKind {
    Rjmp,
    Rcall,
    Brbs(u8),
    Brbc(u8),
}

impl AvrAsm {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: HashMap::new(),
            patches: Vec::new(),
        }
    }

    fn label(&mut self, name: &'static str) {
        self.labels.insert(name, self.code.len());
    }

    #[inline]
    fn w(&mut self, word: u16) {
        self.code.extend_from_slice(&word.to_le_bytes());
    }

    #[inline]
    fn rd_rr(&mut self, base: u16, d: u8, r: u8) {
        self.w(base | (((r as u16) & 0x10) << 5) | (((d as u16) & 0x1F) << 4) | ((r as u16) & 0x0F));
    }

    #[inline]
    fn rd_imm(&mut self, base: u16, d: u8, k: u8) {
        debug_assert!((16..=31).contains(&d));
        self.w(base | (((k as u16) & 0xF0) << 4) | (((d as u16) & 0x0F) << 4) | ((k as u16) & 0x0F));
    }

    fn mov(&mut self, d: u8, r: u8) {
        self.rd_rr(0x2C00, d, r);
    }
    fn movw(&mut self, d_even: u8, r_even: u8) {
        self.w(0x0100 | (((d_even >> 1) as u16) << 4) | ((r_even >> 1) as u16));
    }
    fn ldi(&mut self, d: u8, k: u8) {
        self.rd_imm(0xE000, d, k);
    }
    fn add(&mut self, d: u8, r: u8) {
        self.rd_rr(0x0C00, d, r);
    }
    fn adc(&mut self, d: u8, r: u8) {
        self.rd_rr(0x1C00, d, r);
    }
    fn sub(&mut self, d: u8, r: u8) {
        self.rd_rr(0x1800, d, r);
    }
    fn sbc(&mut self, d: u8, r: u8) {
        self.rd_rr(0x0800, d, r);
    }
    fn subi(&mut self, d: u8, k: u8) {
        self.rd_imm(0x5000, d, k);
    }
    #[allow(dead_code)]
    fn sbci(&mut self, d: u8, k: u8) {
        self.rd_imm(0x4000, d, k);
    }
    #[allow(dead_code)]
    fn and(&mut self, d: u8, r: u8) {
        self.rd_rr(0x2000, d, r);
    }
    fn andi(&mut self, d: u8, k: u8) {
        self.rd_imm(0x7000, d, k);
    }
    fn or(&mut self, d: u8, r: u8) {
        self.rd_rr(0x2800, d, r);
    }
    fn eor(&mut self, d: u8, r: u8) {
        self.rd_rr(0x2400, d, r);
    }
    fn com(&mut self, d: u8) {
        self.w(0x9400 | (((d as u16) & 0x1F) << 4));
    }
    fn inc(&mut self, d: u8) {
        self.w(0x9403 | (((d as u16) & 0x1F) << 4));
    }
    fn dec(&mut self, d: u8) {
        self.w(0x940A | (((d as u16) & 0x1F) << 4));
    }
    fn cp(&mut self, d: u8, r: u8) {
        self.rd_rr(0x1400, d, r);
    }
    fn cpc(&mut self, d: u8, r: u8) {
        self.rd_rr(0x0400, d, r);
    }
    fn mul(&mut self, d: u8, r: u8) {
        self.rd_rr(0x9C00, d, r);
    }
    fn lsl(&mut self, d: u8) {
        self.add(d, d);
    }
    fn rol(&mut self, d: u8) {
        self.adc(d, d);
    }
    fn lsr(&mut self, d: u8) {
        self.w(0x9406 | (((d as u16) & 0x1F) << 4));
    }
    fn ror(&mut self, d: u8) {
        self.w(0x9407 | (((d as u16) & 0x1F) << 4));
    }
    fn asr(&mut self, d: u8) {
        self.w(0x9405 | (((d as u16) & 0x1F) << 4));
    }
    fn adiw(&mut self, d_pair: u8, k: u8) {
        let dd = ((d_pair - 24) / 2) as u16;
        let ku = k as u16;
        self.w(0x9600 | ((ku & 0x30) << 2) | (dd << 4) | (ku & 0x0F));
    }
    fn push(&mut self, r: u8) {
        self.w(0x920F | (((r as u16) & 0x1F) << 4));
    }
    fn pop(&mut self, d: u8) {
        self.w(0x900F | (((d as u16) & 0x1F) << 4));
    }
    fn out_io(&mut self, a: u8, r: u8) {
        self.w(0xB800 | (((a as u16) & 0x30) << 5) | (((r as u16) & 0x1F) << 4) | ((a as u16) & 0x0F));
    }
    fn lds(&mut self, d: u8, addr: u16) {
        self.w(0x9000 | (((d as u16) & 0x1F) << 4));
        self.w(addr);
    }
    fn sts(&mut self, addr: u16, r: u8) {
        self.w(0x9200 | (((r as u16) & 0x1F) << 4));
        self.w(addr);
    }
    fn st_x_inc(&mut self, r: u8) {
        self.w(0x920D | (((r as u16) & 0x1F) << 4));
    }
    fn lpm_z_inc(&mut self, d: u8) {
        self.w(0x9005 | (((d as u16) & 0x1F) << 4));
    }
    fn sbrs(&mut self, r: u8, bit: u8) {
        self.w(0xFE00 | (((r as u16) & 0x1F) << 4) | ((bit as u16) & 7));
    }
    fn sbrc(&mut self, r: u8, bit: u8) {
        self.w(0xFC00 | (((r as u16) & 0x1F) << 4) | ((bit as u16) & 7));
    }
    fn ret(&mut self) {
        self.w(0x9508);
    }
    fn cli(&mut self) {
        self.w(0x94F8);
    }
    fn sleep(&mut self) {
        self.w(0x9588);
    }
    fn rjmp(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xC000);
        self.patches.push((off, target, PatchKind::Rjmp));
    }
    fn rcall(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xD000);
        self.patches.push((off, target, PatchKind::Rcall));
    }
    fn breq(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xF001);
        self.patches.push((off, target, PatchKind::Brbs(1)));
    }
    fn brne(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xF401);
        self.patches.push((off, target, PatchKind::Brbc(1)));
    }
    fn brcs(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xF000);
        self.patches.push((off, target, PatchKind::Brbs(0)));
    }
    fn brcc(&mut self, target: &'static str) {
        let off = self.code.len();
        self.w(0xF400);
        self.patches.push((off, target, PatchKind::Brbc(0)));
    }

    fn finish(mut self) -> Vec<u8> {
        for (off, target, kind) in self.patches {
            let target_off = *self
                .labels
                .get(target)
                .unwrap_or_else(|| panic!("missing AVR asm label `{target}`"));
            let disp_words = ((target_off as isize) - ((off + 2) as isize)) / 2;
            let word = match kind {
                PatchKind::Rjmp => {
                    assert!((-2048..=2047).contains(&disp_words));
                    0xC000 | ((disp_words as u16) & 0x0FFF)
                }
                PatchKind::Rcall => {
                    assert!((-2048..=2047).contains(&disp_words));
                    0xD000 | ((disp_words as u16) & 0x0FFF)
                }
                PatchKind::Brbs(bit) => {
                    assert!((-64..=63).contains(&disp_words));
                    0xF000 | (((disp_words as u16) & 0x007F) << 3) | ((bit as u16) & 7)
                }
                PatchKind::Brbc(bit) => {
                    assert!((-64..=63).contains(&disp_words));
                    0xF400 | (((disp_words as u16) & 0x007F) << 3) | ((bit as u16) & 7)
                }
            };
            let bytes = word.to_le_bytes();
            self.code[off] = bytes[0];
            self.code[off + 1] = bytes[1];
        }
        self.code
    }
}

/// Emits `__avr_uart_putc` (r24 = byte, preserves r25, clobbers r0)
fn emit_uart_putc_sub(a: &mut AvrAsm) {
    a.label("uart_putc");
    a.lds(0, UCSR0A_ADDR);
    a.sbrs(0, 5); // UDRE0
    a.rjmp("uart_putc");
    a.sts(UDR0_ADDR, 24);
    a.ret();
}

fn synth_print_newline() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.ldi(24, b'\n');
    a.rjmp("uart_putc");
    emit_uart_putc_sub(&mut a);
    a.finish()
}

fn synth_print_space() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.ldi(24, b' ');
    a.rjmp("uart_putc");
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_print_cstr(ptr in r25:r24 [Flash byte addr], nl in r23:r22)`
fn synth_print_cstr() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.movw(30, 24); // Z = r25:r24
    a.label("loop");
    a.lpm_z_inc(24);
    a.cp(24, 1);
    a.breq("done_str");
    a.rcall("uart_putc");
    a.rjmp("loop");
    a.label("done_str");
    a.mov(24, 22);
    a.or(24, 23);
    a.breq("ret_lbl");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("ret_lbl");
    a.ret();
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_abort_str(ptr in r25:r24 [Flash byte addr])` -> print string + '\n' + halt (`cli; sleep`)
fn synth_abort_str() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.movw(30, 24);
    a.label("loop");
    a.lpm_z_inc(24);
    a.cp(24, 1);
    a.breq("halt");
    a.rcall("uart_putc");
    a.rjmp("loop");
    a.label("halt");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("stop");
    a.cli();
    a.sleep();
    a.rjmp("stop");
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_print_bool(val in r25:r24, nl in r23:r22)`
fn synth_print_bool() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.mov(18, 24);
    a.or(18, 25);
    a.breq("is_false");
    for &ch in b"true" {
        a.ldi(24, ch);
        a.rcall("uart_putc");
    }
    a.rjmp("check_nl");
    a.label("is_false");
    for &ch in b"false" {
        a.ldi(24, ch);
        a.rcall("uart_putc");
    }
    a.label("check_nl");
    a.mov(24, 22);
    a.or(24, 23);
    a.breq("done");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("done");
    a.ret();
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_print_null(nl in r25:r24)`
fn synth_print_null() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.movw(22, 24);
    for &ch in b"null" {
        a.ldi(24, ch);
        a.rcall("uart_putc");
    }
    a.mov(24, 22);
    a.or(24, 23);
    a.breq("done");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("done");
    a.ret();
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_print_i16` / `aot_print_u16` (`val in r25:r24`, `nl in r23:r22`).
/// Uses only caller-saved r18..r27 and r0/r1.
fn synth_print_int16(signed: bool) -> Vec<u8> {
    let mut a = AvrAsm::new();
    // Save nl (r22|r23 != 0) in r21 (0 or 1)
    a.ldi(21, 0);
    a.mov(18, 22);
    a.or(18, 23);
    a.breq("nl_zero");
    a.ldi(21, 1);
    a.label("nl_zero");

    if signed {
        a.sbrc(25, 7);
        a.rjmp("is_neg");
        a.rjmp("do_digits");
        a.label("is_neg");
        a.push(24);
        a.push(25);
        a.ldi(24, b'-');
        a.rcall("uart_putc");
        a.pop(25);
        a.pop(24);
        a.com(24);
        a.com(25);
        a.adiw(24, 1);
    }

    a.label("do_digits");
    // Digit count in r20
    a.ldi(20, 0);
    // Special case val == 0 -> push '0', count = 1
    a.mov(18, 24);
    a.or(18, 25);
    a.brne("div_loop");
    a.ldi(18, b'0');
    a.push(18);
    a.inc(20);
    a.rjmp("pop_loop");

    // Repeated unsigned 16-bit divide r25:r24 by 10 -> r25:r24 = quotient, r26 = remainder
    a.label("div_loop");
    a.mov(18, 24);
    a.or(18, 25);
    a.breq("pop_loop");
    a.ldi(26, 0); // rem
    a.ldi(19, 16); // bit count
    a.label("bit_loop");
    a.lsl(24);
    a.rol(25);
    a.rol(26);
    a.ldi(18, 10);
    a.cp(26, 18);
    a.brcs("no_sub");
    a.subi(26, 10);
    a.inc(24);
    a.label("no_sub");
    a.dec(19);
    a.brne("bit_loop");
    a.ldi(18, b'0');
    a.add(26, 18);
    a.push(26);
    a.inc(20);
    a.rjmp("div_loop");

    a.label("pop_loop");
    a.cp(20, 1);
    a.breq("check_nl");
    a.pop(24);
    a.rcall("uart_putc");
    a.dec(20);
    a.rjmp("pop_loop");

    a.label("check_nl");
    a.cp(21, 1);
    a.breq("done");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("done");
    a.ret();
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// `aot_print_i32` / `aot_print_u32` (`val in r25:r24:r23:r22`, `nl in r21:r20`).
fn synth_print_int32(signed: bool) -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.push(16);
    a.push(17);
    a.ldi(16, 0);
    a.mov(18, 20);
    a.or(18, 21);
    a.breq("nl_zero");
    a.ldi(16, 1);
    a.label("nl_zero");

    if signed {
        a.sbrc(25, 7);
        a.rjmp("is_neg");
        a.rjmp("do_digits");
        a.label("is_neg");
        a.push(22);
        a.push(23);
        a.push(24);
        a.push(25);
        a.ldi(24, b'-');
        a.rcall("uart_putc");
        a.pop(25);
        a.pop(24);
        a.pop(23);
        a.pop(22);
        a.com(22);
        a.com(23);
        a.com(24);
        a.com(25);
        a.ldi(18, 1);
        a.add(22, 18);
        a.adc(23, 1);
        a.adc(24, 1);
        a.adc(25, 1);
    }

    a.label("do_digits");
    a.ldi(17, 0); // digit count
    a.mov(18, 22);
    a.or(18, 23);
    a.or(18, 24);
    a.or(18, 25);
    a.brne("div_loop");
    a.ldi(18, b'0');
    a.push(18);
    a.inc(17);
    a.rjmp("pop_loop");

    a.label("div_loop");
    a.mov(18, 22);
    a.or(18, 23);
    a.or(18, 24);
    a.or(18, 25);
    a.breq("pop_loop");
    a.ldi(26, 0); // rem
    a.ldi(19, 32); // bit count
    a.label("bit_loop");
    a.lsl(22);
    a.rol(23);
    a.rol(24);
    a.rol(25);
    a.rol(26);
    a.ldi(18, 10);
    a.cp(26, 18);
    a.brcs("no_sub");
    a.subi(26, 10);
    a.inc(22);
    a.label("no_sub");
    a.dec(19);
    a.brne("bit_loop");
    a.ldi(18, b'0');
    a.add(26, 18);
    a.push(26);
    a.inc(17);
    a.rjmp("div_loop");

    a.label("pop_loop");
    a.cp(17, 1);
    a.breq("check_nl");
    a.pop(24);
    a.rcall("uart_putc");
    a.dec(17);
    a.rjmp("pop_loop");

    a.label("check_nl");
    a.cp(16, 1);
    a.breq("done");
    a.ldi(24, b'\n');
    a.rcall("uart_putc");
    a.label("done");
    a.pop(17);
    a.pop(16);
    a.ret();
    emit_uart_putc_sub(&mut a);
    a.finish()
}

/// Signed 16-bit division / modulo:
/// `r25:r24` = dividend, `r23:r22` = divisor -> `r25:r24` = quotient (`is_mod == false`) or remainder (`is_mod == true`).
/// Preserves all callee-saved registers `r2..r17`, `Y`.
fn synth_divmod16(is_mod: bool) -> Vec<u8> {
    let mut a = AvrAsm::new();
    // Guard div-by-zero -> return 0
    a.mov(18, 22);
    a.or(18, 23);
    a.brne("non_zero");
    a.mov(24, 1);
    a.mov(25, 1);
    a.ret();

    a.label("non_zero");
    // r19 bit 7 = sign of result
    a.mov(19, 25);
    if !is_mod {
        a.eor(19, 23);
    }
    // abs(dividend in r25:r24)
    a.sbrc(25, 7);
    a.rjmp("neg_a");
    a.rjmp("chk_b");
    a.label("neg_a");
    a.com(24);
    a.com(25);
    a.adiw(24, 1);

    a.label("chk_b");
    a.sbrc(23, 7);
    a.rjmp("neg_b");
    a.rjmp("udiv");
    a.label("neg_b");
    a.com(22);
    a.com(23);
    a.ldi(18, 1);
    a.add(22, 18);
    a.adc(23, 1);

    a.label("udiv");
    a.mov(26, 1); // R_lo = 0
    a.mov(27, 1); // R_hi = 0
    a.ldi(18, 16);
    a.label("loop");
    a.lsl(24);
    a.rol(25);
    a.rol(26);
    a.rol(27);
    a.cp(26, 22);
    a.cpc(27, 23);
    a.brcs("skip");
    a.sub(26, 22);
    a.sbc(27, 23);
    a.inc(24);
    a.label("skip");
    a.dec(18);
    a.brne("loop");

    if is_mod {
        a.movw(24, 26);
    }
    a.sbrc(19, 7);
    a.rjmp("neg_res");
    a.ret();
    a.label("neg_res");
    a.com(24);
    a.com(25);
    a.adiw(24, 1);
    a.ret();
    a.finish()
}

/// 32x32 -> 32 multiply using ATmega328P hardware `MUL`:
/// `A = r25:r24:r23:r22`, `B = r21:r20:r19:r18` -> `r25:r24:r23:r22`.
fn synth_mul32() -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.push(12);
    a.push(13);
    a.push(14);
    a.push(15);
    a.ldi(26, 0); // dedicated 0 byte since MUL overwrites r1!

    // i+j = 0: a0*b0 -> r13:r12
    a.mul(22, 18);
    a.movw(12, 0);
    a.mov(14, 26);
    a.mov(15, 26);

    // i+j = 1: a0*b1, a1*b0 -> add into r15:r14:r13
    for (ai, bi) in [(22, 19), (23, 18)] {
        a.mul(ai, bi);
        a.add(13, 0);
        a.adc(14, 1);
        a.adc(15, 26);
    }
    // i+j = 2: a0*b2, a1*b1, a2*b0 -> add into r15:r14
    for (ai, bi) in [(22, 20), (23, 19), (24, 18)] {
        a.mul(ai, bi);
        a.add(14, 0);
        a.adc(15, 1);
    }
    // i+j = 3: a0*b3, a1*b2, a2*b1, a3*b0 -> add into r15
    for (ai, bi) in [(22, 21), (23, 20), (24, 19), (25, 18)] {
        a.mul(ai, bi);
        a.add(15, 0);
    }

    a.eor(1, 1); // restore __zero_reg__!
    a.movw(22, 12);
    a.movw(24, 14);
    a.pop(15);
    a.pop(14);
    a.pop(13);
    a.pop(12);
    a.ret();
    a.finish()
}

/// Signed 32-bit division / modulo:
/// `A = r25:r24:r23:r22`, `B = r21:r20:r19:r18` -> `r25:r24:r23:r22`.
fn synth_divmod32(is_mod: bool) -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.mov(26, 18);
    a.or(26, 19);
    a.or(26, 20);
    a.or(26, 21);
    a.brne("non_zero");
    a.mov(22, 1);
    a.mov(23, 1);
    a.mov(24, 1);
    a.mov(25, 1);
    a.ret();

    a.label("non_zero");
    for r in 12..=17 {
        a.push(r);
    }
    a.mov(16, 25);
    if !is_mod {
        a.eor(16, 21);
    }
    a.sbrc(25, 7);
    a.rjmp("neg_a");
    a.rjmp("chk_b");
    a.label("neg_a");
    a.com(22);
    a.com(23);
    a.com(24);
    a.com(25);
    a.ldi(26, 1);
    a.add(22, 26);
    a.adc(23, 1);
    a.adc(24, 1);
    a.adc(25, 1);

    a.label("chk_b");
    a.sbrc(21, 7);
    a.rjmp("neg_b");
    a.rjmp("udiv");
    a.label("neg_b");
    a.com(18);
    a.com(19);
    a.com(20);
    a.com(21);
    a.ldi(26, 1);
    a.add(18, 26);
    a.adc(19, 1);
    a.adc(20, 1);
    a.adc(21, 1);

    a.label("udiv");
    for r in 12..=15 {
        a.mov(r, 1);
    }
    a.ldi(26, 32);
    a.label("loop");
    a.lsl(22);
    a.rol(23);
    a.rol(24);
    a.rol(25);
    a.rol(12);
    a.rol(13);
    a.rol(14);
    a.rol(15);
    a.cp(12, 18);
    a.cpc(13, 19);
    a.cpc(14, 20);
    a.cpc(15, 21);
    a.brcs("skip");
    a.sub(12, 18);
    a.sbc(13, 19);
    a.sbc(14, 20);
    a.sbc(15, 21);
    a.inc(22);
    a.label("skip");
    a.dec(26);
    a.brne("loop");

    if is_mod {
        a.movw(22, 12);
        a.movw(24, 14);
    }
    a.sbrc(16, 7);
    a.rjmp("neg_res");
    a.rjmp("done");
    a.label("neg_res");
    a.com(22);
    a.com(23);
    a.com(24);
    a.com(25);
    a.ldi(26, 1);
    a.add(22, 26);
    a.adc(23, 1);
    a.adc(24, 1);
    a.adc(25, 1);

    a.label("done");
    for r in (12..=17).rev() {
        a.pop(r);
    }
    a.ret();
    a.finish()
}

#[derive(Clone, Copy)]
enum Shift32Kind {
    Shl,
    Shr,
    Sar,
}

fn synth_shift32(kind: Shift32Kind) -> Vec<u8> {
    let mut a = AvrAsm::new();
    a.andi(18, 0x1F);
    a.label("test");
    a.cp(18, 1);
    a.breq("done");
    match kind {
        Shift32Kind::Shl => {
            a.lsl(22);
            a.rol(23);
            a.rol(24);
            a.rol(25);
        }
        Shift32Kind::Shr => {
            a.lsr(25);
            a.ror(24);
            a.ror(23);
            a.ror(22);
        }
        Shift32Kind::Sar => {
            a.asr(25);
            a.ror(24);
            a.ror(23);
            a.ror(22);
        }
    }
    a.dec(18);
    a.rjmp("test");
    a.label("done");
    a.ret();
    a.finish()
}

pub fn is_avr_runtime_helper(name: &str) -> bool {
    matches!(
        name,
        "aot_print_newline"
            | "aot_print_space"
            | "aot_print_cstr"
            | "aot_abort_str"
            | "aot_print_bool"
            | "aot_print_null"
            | "aot_print_i64"
            | "aot_print_i16"
            | "aot_print_u64"
            | "aot_print_u16"
            | "aot_print_i32"
            | "aot_print_u32"
            | "__avr_div16"
            | "__avr_mod16"
            | "__avr_mul32"
            | "__avr_div32"
            | "__avr_mod32"
            | "__avr_shl32"
            | "__avr_shr32"
            | "__avr_sar32"
    )
}

fn synthesize_avr_helper(name: &str) -> Option<Vec<u8>> {
    match name {
        "aot_print_newline" => Some(synth_print_newline()),
        "aot_print_space" => Some(synth_print_space()),
        "aot_print_cstr" => Some(synth_print_cstr()),
        "aot_abort_str" => Some(synth_abort_str()),
        "aot_print_bool" => Some(synth_print_bool()),
        "aot_print_null" => Some(synth_print_null()),
        "aot_print_i64" | "aot_print_i16" => Some(synth_print_int16(true)),
        "aot_print_u64" | "aot_print_u16" => Some(synth_print_int16(false)),
        "aot_print_i32" => Some(synth_print_int32(true)),
        "aot_print_u32" => Some(synth_print_int32(false)),
        "__avr_div16" => Some(synth_divmod16(false)),
        "__avr_mod16" => Some(synth_divmod16(true)),
        "__avr_mul32" => Some(synth_mul32()),
        "__avr_div32" => Some(synth_divmod32(false)),
        "__avr_mod32" => Some(synth_divmod32(true)),
        "__avr_shl32" => Some(synth_shift32(Shift32Kind::Shl)),
        "__avr_shr32" => Some(synth_shift32(Shift32Kind::Shr)),
        "__avr_sar32" => Some(synth_shift32(Shift32Kind::Sar)),
        _ => None,
    }
}

/// Emit ATmega328P crt0 (26-vector IVT, SREG=0, SP=0x08FF, `.data` copy, `.bss` zero,
/// UART0 9600 8N1 @ 16 MHz init, `call main`, `cli; sleep; rjmp .-2`).
/// Returns `(crt0_bytes, entry_call_offset, copy_data_patch_offsets, clear_bss_patch_offsets)`.
struct Crt0Offsets {
    bytes: Vec<u8>,
    entry_call_off: usize,
    data_lma_lo_off: usize,
    data_lma_hi_off: usize,
    data_end_lo_off: usize,
    data_end_hi_off: usize,
    bss_start_lo_off: usize,
    bss_start_hi_off: usize,
    bss_end_lo_off: usize,
    bss_end_hi_off: usize,
}

fn build_atmega328p_crt0() -> Crt0Offsets {
    let mut a = AvrAsm::new();
    // 26 interrupt vectors × 4 bytes (`jmp` = 0x940C + word_addr)
    // Vector 0: reset -> __ctors_end (patched below)
    a.w(0x940C);
    a.w(0x0000);
    // Vectors 1..25: -> __bad_interrupt (patched below)
    for _ in 1..26 {
        a.w(0x940C);
        a.w(0x0000);
    }

    let ctors_end_off = a.code.len();
    // __ctors_end:
    a.eor(1, 1); // r1 = 0 (__zero_reg__)
    a.out_io(SREG_IO, 1); // SREG = 0
    a.ldi(28, 0xFF); // SP = 0x08FF
    a.ldi(29, 0x08);
    a.out_io(SPH_IO, 29);
    a.out_io(SPL_IO, 28);

    // __do_copy_data:
    let data_lma_lo_off = a.code.len();
    a.ldi(30, 0);
    let data_lma_hi_off = a.code.len();
    a.ldi(31, 0);
    a.ldi(26, (ATMEGA328P_SRAM_START & 0xFF) as u8);
    a.ldi(27, (ATMEGA328P_SRAM_START >> 8) as u8);
    let data_end_lo_off = a.code.len();
    a.ldi(24, 0);
    let data_end_hi_off = a.code.len();
    a.ldi(25, 0);
    a.label("copy_loop");
    a.cp(26, 24);
    a.cpc(27, 25);
    a.brcc("copy_done");
    a.lpm_z_inc(0);
    a.st_x_inc(0);
    a.rjmp("copy_loop");
    a.label("copy_done");

    // __do_clear_bss:
    let bss_start_lo_off = a.code.len();
    a.ldi(26, 0);
    let bss_start_hi_off = a.code.len();
    a.ldi(27, 0);
    let bss_end_lo_off = a.code.len();
    a.ldi(24, 0);
    let bss_end_hi_off = a.code.len();
    a.ldi(25, 0);
    a.label("bss_loop");
    a.cp(26, 24);
    a.cpc(27, 25);
    a.brcc("bss_done");
    a.st_x_inc(1);
    a.rjmp("bss_loop");
    a.label("bss_done");

    // Initialize ATmega328P UART0 (16 MHz, 9600 baud 8N1)
    a.ldi(24, 0);
    a.sts(UBRR0H_ADDR, 24);
    a.ldi(24, 103);
    a.sts(UBRR0L_ADDR, 24);
    a.ldi(24, 1 << 3); // TXEN0
    a.sts(UCSR0B_ADDR, 24);
    a.ldi(24, 3 << 1); // 8N1
    a.sts(UCSR0C_ADDR, 24);

    // call <entry>
    let entry_call_off = a.code.len();
    a.w(0x940E);
    a.w(0x0000);

    // _exit / __bad_interrupt:
    let exit_off = a.code.len();
    a.label("exit");
    a.cli();
    a.sleep();
    a.rjmp("exit");

    let mut bytes = a.finish();
    // Patch vector 0 -> ctors_end_off, vectors 1..25 -> exit_off
    let ctors_word = (ctors_end_off / 2) as u16;
    bytes[2..4].copy_from_slice(&ctors_word.to_le_bytes());
    let exit_word = (exit_off / 2) as u16;
    for vec in 1..26 {
        let base = vec * 4;
        bytes[base + 2..base + 4].copy_from_slice(&exit_word.to_le_bytes());
    }

    Crt0Offsets {
        bytes,
        entry_call_off,
        data_lma_lo_off,
        data_lma_hi_off,
        data_end_lo_off,
        data_end_hi_off,
        bss_start_lo_off,
        bss_start_hi_off,
        bss_end_lo_off,
        bss_end_hi_off,
    }
}

fn patch_ldi_imm(buf: &mut [u8], off: usize, imm: u8) {
    let w = u16::from_le_bytes([buf[off], buf[off + 1]]);
    let b = imm as u16;
    let patched = (w & 0xF0F0) | ((b & 0xF0) << 4) | (b & 0x0F);
    buf[off..off + 2].copy_from_slice(&patched.to_le_bytes());
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SecRegion {
    Text,
    Rodata,
    Data,
    Bss,
}

/// Link `objects` into an ATmega328P `EM_AVR` ELF32 executable or Intel HEX image.
pub fn emit_avr_executable(
    objects: &[ObjectFile],
    config: &LinkConfig,
    entry_symbol: &str,
) -> LinkResult<()> {
    let crt0 = build_atmega328p_crt0();
    let mut text_data = crt0.bytes;
    let mut rodata_data: Vec<u8> = Vec::new();
    let mut data_data: Vec<u8> = Vec::new();
    let mut bss_size: usize = 0;

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
                    let align = (sec.alignment as usize).max(2);
                    let aligned = (text_data.len() + align - 1) & !(align - 1);
                    text_data.resize(aligned, 0x00);
                    let base = text_data.len();
                    text_data.extend_from_slice(&sec.data);
                    if text_data.len() % 2 != 0 {
                        text_data.push(0x00);
                    }
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
                SectionKind::Rodata => {
                    let align = (sec.alignment as usize).max(1);
                    let aligned = (rodata_data.len() + align - 1) & !(align - 1);
                    rodata_data.resize(aligned, 0x00);
                    let base = rodata_data.len();
                    rodata_data.extend_from_slice(&sec.data);
                    sec_placements.insert((file_idx, sec_idx), (SecRegion::Rodata, base));
                    for r in &sec.relocations {
                        referenced_symbols.insert(r.symbol_name.clone());
                        pending_relocs.push(PendingReloc {
                            region: SecRegion::Rodata,
                            offset: base + r.offset as usize,
                            reloc: r.clone(),
                            file_idx,
                        });
                    }
                }
                SectionKind::Data => {
                    let align = (sec.alignment as usize).max(1);
                    let aligned = (data_data.len() + align - 1) & !(align - 1);
                    data_data.resize(aligned, 0x00);
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
                global_symbols.entry(sym.name.clone()).or_insert((region, off));
            }
        }
    }

    // Synthesize referenced AVR freestanding runtime helpers.
    let mut missing_helpers: Vec<String> = referenced_symbols
        .iter()
        .filter(|name| !global_symbols.contains_key(*name))
        .filter(|name| synthesize_avr_helper(name).is_some())
        .cloned()
        .collect();
    missing_helpers.sort();
    for helper in missing_helpers {
        if let Some(code) = synthesize_avr_helper(&helper) {
            if text_data.len() % 2 != 0 {
                text_data.push(0x00);
            }
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

    // Check every referenced symbol resolves before patching.
    for pr in &pending_relocs {
        let found = local_symbols.contains_key(&(pr.file_idx, pr.reloc.symbol_name.clone()))
            || global_symbols.contains_key(&pr.reloc.symbol_name);
        if !found {
            return Err(LinkError::undefined_symbol(
                &pr.reloc.symbol_name,
                objects
                    .get(pr.file_idx)
                    .map(|o| o.path.display().to_string())
                    .unwrap_or_default(),
                None,
                Some(pr.reloc.offset),
            ));
        }
    }

    // Compute final Flash & SRAM addresses.
    // Ensure .rodata starts on a 2-byte boundary inside Flash.
    if text_data.len() % 2 != 0 {
        text_data.push(0x00);
    }
    let rodata_flash_base = text_data.len();
    let mut flash_image = text_data;
    flash_image.extend_from_slice(&rodata_data);
    if flash_image.len() % 2 != 0 {
        flash_image.push(0x00);
    }
    let data_load_start = flash_image.len();

    let total_flash = data_load_start + data_data.len();
    if total_flash > ATMEGA328P_FLASH_SIZE {
        return Err(LinkError::new(
            ErrorCode::MemoryExhausted,
            format!(
                "ATmega328P Flash overflow: image uses {total_flash} bytes (max {} bytes)",
                ATMEGA328P_FLASH_SIZE
            ),
        ));
    }
    let sram_end = ATMEGA328P_SRAM_START + data_data.len() + bss_size;
    if sram_end > ATMEGA328P_SRAM_END {
        return Err(LinkError::new(
            ErrorCode::MemoryExhausted,
            format!(
                "ATmega328P SRAM overflow: static data + bss reach 0x{sram_end:04x} (RAMEND = 0x08ff)"
            ),
        ));
    }

    // Patch crt0 `.data` copy and `.bss` clear bounds.
    let data_end_sram = (ATMEGA328P_SRAM_START + data_data.len()) as u16;
    let bss_start_sram = data_end_sram;
    let bss_end_sram = (ATMEGA328P_SRAM_START + data_data.len() + bss_size) as u16;
    patch_ldi_imm(&mut flash_image, crt0.data_lma_lo_off, (data_load_start & 0xFF) as u8);
    patch_ldi_imm(&mut flash_image, crt0.data_lma_hi_off, (data_load_start >> 8) as u8);
    patch_ldi_imm(&mut flash_image, crt0.data_end_lo_off, (data_end_sram & 0xFF) as u8);
    patch_ldi_imm(&mut flash_image, crt0.data_end_hi_off, (data_end_sram >> 8) as u8);
    patch_ldi_imm(&mut flash_image, crt0.bss_start_lo_off, (bss_start_sram & 0xFF) as u8);
    patch_ldi_imm(&mut flash_image, crt0.bss_start_hi_off, (bss_start_sram >> 8) as u8);
    patch_ldi_imm(&mut flash_image, crt0.bss_end_lo_off, (bss_end_sram & 0xFF) as u8);
    patch_ldi_imm(&mut flash_image, crt0.bss_end_hi_off, (bss_end_sram >> 8) as u8);

    // Patch crt0 `call <entry_symbol>`.
    let handler = AvrArch;
    let (_, entry_off) = global_symbols[entry_symbol];
    handler.apply(
        &Relocation::new(
            crt0.entry_call_off as u64,
            entry_symbol,
            RelocationKind::AvrCall22,
            0,
        ),
        crt0.entry_call_off as u64,
        entry_off as u64,
        0,
        &mut flash_image,
    )?;

    let data_len = data_data.len();
    let resolve_addr = |region: SecRegion, off: usize, kind: RelocationKind| -> u64 {
        match region {
            SecRegion::Text => match kind {
                // Function pointers loaded via LDI/16-bit for ICALL take Flash word addresses.
                RelocationKind::AvrLo8 | RelocationKind::AvrHi8 | RelocationKind::AvrAddr16 => {
                    (off / 2) as u64
                }
                _ => off as u64,
            },
            SecRegion::Rodata => (rodata_flash_base + off) as u64,
            SecRegion::Data => (ATMEGA328P_SRAM_START + off) as u64,
            SecRegion::Bss => (ATMEGA328P_SRAM_START + data_len + off) as u64,
        }
    };

    for pr in &pending_relocs {
        let (sym_reg, sym_off) = local_symbols
            .get(&(pr.file_idx, pr.reloc.symbol_name.clone()))
            .or_else(|| global_symbols.get(&pr.reloc.symbol_name))
            .copied()
            .expect("symbol checked above");
        let sym_addr = resolve_addr(sym_reg, sym_off, pr.reloc.kind);

        match pr.region {
            SecRegion::Text => {
                let mut r = pr.reloc.clone();
                r.offset = pr.offset as u64;
                handler.apply(&r, pr.offset as u64, sym_addr, pr.reloc.addend, &mut flash_image)?;
            }
            SecRegion::Rodata => {
                let abs_off = rodata_flash_base + pr.offset;
                let mut r = pr.reloc.clone();
                r.offset = abs_off as u64;
                handler.apply(&r, abs_off as u64, sym_addr, pr.reloc.addend, &mut flash_image)?;
            }
            SecRegion::Data => {
                let mut r = pr.reloc.clone();
                r.offset = pr.offset as u64;
                let place_va = (ATMEGA328P_SRAM_START + pr.offset) as u64;
                handler.apply(&r, place_va, sym_addr, pr.reloc.addend, &mut data_data)?;
            }
            SecRegion::Bss => {}
        }
    }

    let is_hex = config
        .output_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("hex"));

    let out_bytes = if is_hex {
        let mut combined_flash = flash_image.clone();
        combined_flash.extend_from_slice(&data_data);
        encode_intel_hex(&combined_flash)
    } else {
        encode_avr_elf32(
            &flash_image,
            &data_data,
            bss_size,
            rodata_flash_base,
            &global_symbols,
        )
    };

    if let Some(parent) = config.output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&config.output_path, &out_bytes)?;
    Ok(())
}

/// Emit Intel HEX (I8HEX) format for Flash + initialized `.data` LMA image.
pub fn encode_intel_hex(image: &[u8]) -> Vec<u8> {
    let mut out = String::new();
    let mut addr: usize = 0;
    while addr < image.len() {
        let chunk_len = (image.len() - addr).min(16);
        let addr16 = addr as u16;
        let mut sum: u8 = (chunk_len as u8)
            .wrapping_add((addr16 >> 8) as u8)
            .wrapping_add((addr16 & 0xFF) as u8);
        out.push_str(&format!(":{:02X}{:04X}00", chunk_len, addr16));
        for &b in &image[addr..addr + chunk_len] {
            sum = sum.wrapping_add(b);
            out.push_str(&format!("{:02X}", b));
        }
        let checksum = (!sum).wrapping_add(1);
        out.push_str(&format!("{:02X}\n", checksum));
        addr += chunk_len;
    }
    out.push_str(":00000001FF\n");
    out.into_bytes()
}

/// Emit a valid little-endian ELF32 executable (`EM_AVR = 83`, `EF_AVR_ARCH_AVR5 = 0x05`).
/// Layout matches `avr-gcc` / `avr-ld` so `simavr`, `readelf`, `avr-objdump`, and `avr-objcopy`
/// all recognize it natively.
fn encode_avr_elf32(
    flash_text_rodata: &[u8],
    data_bytes: &[u8],
    bss_size: usize,
    rodata_off: usize,
    global_symbols: &HashMap<String, (SecRegion, usize)>,
) -> Vec<u8> {
    const EHDR_SIZE: usize = 52;
    const PHDR_SIZE: usize = 32;
    const SHDR_SIZE: usize = 40;
    const NUM_PHDRS: usize = 2;

    let text_file_off = EHDR_SIZE + NUM_PHDRS * PHDR_SIZE;
    let data_file_off = text_file_off + flash_text_rodata.len();
    let data_lma = flash_text_rodata.len() as u32;
    let sram_vma = AVR_ELF_SRAM_TAG | (ATMEGA328P_SRAM_START as u32);
    let bss_vma = sram_vma + data_bytes.len() as u32;

    // Build .strtab and .symtab
    let mut strtab: Vec<u8> = vec![0];
    let mut add_str = |s: &str| -> u32 {
        let off = strtab.len() as u32;
        strtab.extend_from_slice(s.as_bytes());
        strtab.push(0);
        off
    };

    let mut symtab: Vec<u8> = vec![0u8; 16]; // STN_UNDEF
    let mut sorted_syms: Vec<(&String, &(SecRegion, usize))> = global_symbols.iter().collect();
    sorted_syms.sort_by(|a, b| a.0.cmp(b.0));

    let push_sym = |symtab: &mut Vec<u8>, name_off: u32, val: u32, shndx: u16, info: u8| {
        symtab.extend_from_slice(&name_off.to_le_bytes());
        symtab.extend_from_slice(&val.to_le_bytes());
        symtab.extend_from_slice(&0u32.to_le_bytes()); // st_size
        symtab.push(info);
        symtab.push(0);
        symtab.extend_from_slice(&shndx.to_le_bytes());
    };

    for &(name, &(reg, off)) in &sorted_syms {
        let name_off = add_str(name);
        let (val, shndx, st_type) = match reg {
            SecRegion::Text => (off as u32, 1u16, 2u8), // STT_FUNC
            SecRegion::Rodata => ((rodata_off + off) as u32, 1u16, 1u8), // STT_OBJECT in .text (avr-ld Flash layout)
            SecRegion::Data => (sram_vma + off as u32, 2u16, 1u8),
            SecRegion::Bss => (bss_vma + off as u32, 3u16, 1u8),
        };
        push_sym(&mut symtab, name_off, val, shndx, (1 << 4) | st_type); // STB_GLOBAL
    }
    let rodata_sym_off = add_str("__rodata_start");
    push_sym(&mut symtab, rodata_sym_off, rodata_off as u32, 1u16, (1 << 4) | 1u8);

    // Build .shstrtab
    let mut shstrtab: Vec<u8> = vec![0];
    let mut add_shstr = |s: &str| -> u32 {
        let off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(s.as_bytes());
        shstrtab.push(0);
        off
    };
    let sh_text_name = add_shstr(".text");
    let sh_data_name = add_shstr(".data");
    let sh_bss_name = add_shstr(".bss");
    let sh_symtab_name = add_shstr(".symtab");
    let sh_strtab_name = add_shstr(".strtab");
    let sh_shstrtab_name = add_shstr(".shstrtab");

    let symtab_off = data_file_off + data_bytes.len();
    let strtab_off = symtab_off + symtab.len();
    let shstrtab_off = strtab_off + strtab.len();
    let shoff_raw = shstrtab_off + shstrtab.len();
    let shoff = (shoff_raw + 3) & !3;

    let mut out = Vec::with_capacity(shoff + 7 * SHDR_SIZE);

    // ELF32 Header (52 bytes)
    out.extend_from_slice(&[0x7F, b'E', b'L', b'F', 1, 1, 1, 0]);
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    out.extend_from_slice(&EM_AVR.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
    out.extend_from_slice(&0u32.to_le_bytes()); // e_entry = 0x0000 (reset vector)
    out.extend_from_slice(&(EHDR_SIZE as u32).to_le_bytes()); // e_phoff
    out.extend_from_slice(&(shoff as u32).to_le_bytes()); // e_shoff
    out.extend_from_slice(&EF_AVR_ARCH_AVR5.to_le_bytes()); // e_flags = avr:5
    out.extend_from_slice(&(EHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&(PHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&(NUM_PHDRS as u16).to_le_bytes());
    out.extend_from_slice(&(SHDR_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&7u16.to_le_bytes()); // e_shnum
    out.extend_from_slice(&6u16.to_le_bytes()); // e_shstrndx

    // Program Header 0: Flash (.text + .rodata) RX at VA 0, PA 0
    let write_phdr = |out: &mut Vec<u8>,
                      off: u32,
                      vaddr: u32,
                      paddr: u32,
                      filesz: u32,
                      memsz: u32,
                      flags: u32,
                      align: u32| {
        out.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&vaddr.to_le_bytes());
        out.extend_from_slice(&paddr.to_le_bytes());
        out.extend_from_slice(&filesz.to_le_bytes());
        out.extend_from_slice(&memsz.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&align.to_le_bytes());
    };
    write_phdr(
        &mut out,
        text_file_off as u32,
        0,
        0,
        flash_text_rodata.len() as u32,
        flash_text_rodata.len() as u32,
        5, // PF_R | PF_X
        2,
    );
    // Program Header 1: SRAM (.data + .bss) RW at VMA 0x00800100, LMA = end of Flash
    write_phdr(
        &mut out,
        data_file_off as u32,
        sram_vma,
        data_lma,
        data_bytes.len() as u32,
        (data_bytes.len() + bss_size) as u32,
        6, // PF_R | PF_W
        1,
    );

    // Payload bytes
    out.extend_from_slice(flash_text_rodata);
    out.extend_from_slice(data_bytes);
    out.extend_from_slice(&symtab);
    out.extend_from_slice(&strtab);
    out.extend_from_slice(&shstrtab);
    out.resize(shoff, 0);

    // Section Headers (7 entries)
    #[allow(clippy::too_many_arguments)]
    let write_shdr = |out: &mut Vec<u8>,
                      name: u32,
                      sh_type: u32,
                      flags: u32,
                      addr: u32,
                      off: u32,
                      size: u32,
                      link: u32,
                      info: u32,
                      align: u32,
                      entsize: u32| {
        out.extend_from_slice(&name.to_le_bytes());
        out.extend_from_slice(&sh_type.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&addr.to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&link.to_le_bytes());
        out.extend_from_slice(&info.to_le_bytes());
        out.extend_from_slice(&align.to_le_bytes());
        out.extend_from_slice(&entsize.to_le_bytes());
    };
    // 0: SHT_NULL
    write_shdr(&mut out, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    // 1: .text (PROGBITS, ALLOC | EXECINSTR; includes Flash .rodata just like avr-ld)
    write_shdr(
        &mut out,
        sh_text_name,
        1,
        6,
        0,
        text_file_off as u32,
        flash_text_rodata.len() as u32,
        0,
        0,
        2,
        0,
    );
    // 2: .data (PROGBITS, WRITE | ALLOC)
    write_shdr(
        &mut out,
        sh_data_name,
        1,
        3,
        sram_vma,
        data_file_off as u32,
        data_bytes.len() as u32,
        0,
        0,
        1,
        0,
    );
    // 3: .bss (NOBITS, WRITE | ALLOC)
    write_shdr(
        &mut out,
        sh_bss_name,
        8,
        3,
        bss_vma,
        data_file_off as u32 + data_bytes.len() as u32,
        bss_size as u32,
        0,
        0,
        1,
        0,
    );
    // 4: .symtab (SYMTAB)
    write_shdr(
        &mut out,
        sh_symtab_name,
        2,
        0,
        0,
        symtab_off as u32,
        symtab.len() as u32,
        5, // .strtab
        1, // first global
        4,
        16,
    );
    // 5: .strtab (STRTAB)
    write_shdr(
        &mut out,
        sh_strtab_name,
        3,
        0,
        0,
        strtab_off as u32,
        strtab.len() as u32,
        0,
        0,
        1,
        0,
    );
    // 6: .shstrtab (STRTAB)
    write_shdr(
        &mut out,
        sh_shstrtab_name,
        3,
        0,
        0,
        shstrtab_off as u32,
        shstrtab.len() as u32,
        0,
        0,
        1,
        0,
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_hex_format_valid_checksums() {
        let data = [0x0C, 0x94, 0x34, 0x00];
        let hex = String::from_utf8(encode_intel_hex(&data)).unwrap();
        let lines: Vec<&str> = hex.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], ":040000000C94340028");
        assert_eq!(lines[1], ":00000001FF");
    }

    #[test]
    fn avr_crt0_and_elf32_header_valid() {
        let crt0 = build_atmega328p_crt0();
        assert!(crt0.bytes.len() > 26 * 4);
        let mut syms = HashMap::new();
        syms.insert("main".to_string(), (SecRegion::Text, 0x80usize));
        let elf = encode_avr_elf32(&crt0.bytes, &[1, 2], 4, crt0.bytes.len(), &syms);
        assert_eq!(&elf[0..4], b"\x7fELF");
        assert_eq!(elf[4], 1); // ELF32
        assert_eq!(elf[5], 1); // LSB
        assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), EM_AVR);
    }
}
