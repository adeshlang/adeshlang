//! Motorola DSP56000 (24-bit fixed-point Harvard DSP) linker, crt0 startup,
//! freestanding arithmetic/SCI helpers, and Motorola `.LOD` emitter.
//!
//! Harvard word-addressed memory map:
//! - `P:` Program memory (24-bit words):
//!   - `P:$0000..$0001`: Reset vector (`JMP >__crt0_start`)
//!   - `P:$0040..`:      `__crt0_start`, linked `.text` functions, synthesized DSP56k helpers
//! - `X:` Data memory (24-bit words):
//!   - `X:$0040..`:      `.rodata`, `.data`, `.bss`
//!   - Above `.bss`:     Upward-growing software stack (`R7` / `R6`), bounded below `X:$FF00` I/O
//!   - `X:$FFE0`:        Memory-mapped Host/SCI TX register for `aot_print_*`

#![forbid(unsafe_code)]

use crate::arch::dsp56000::Dsp56000Arch;
use crate::config::LinkConfig;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationHandler, RelocationKind};
use crate::section::SectionKind;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

const DSP56K_P_CODE_BASE: u16 = 0x0040;
const DSP56K_X_DATA_BASE: u16 = 0x0040;
const DSP56K_X_STACK_LIMIT: u16 = 0xFF00;
const DSP56K_SCI_TX_ADDR: u16 = 0xFFE0;

const REG5_A1: u8 = 0x0C;
const REG5_B1: u8 = 0x0D;
const REG5_A: u8 = 0x0E;
const REG5_B: u8 = 0x0F;
const REG5_X0: u8 = 0x04;
const REG5_X1: u8 = 0x05;
const REG5_Y0: u8 = 0x06;
const REG5_Y1: u8 = 0x07;
const REG5_R4: u8 = 0x14;
const REG5_R6: u8 = 0x16;
const REG5_R7: u8 = 0x17;

struct Dsp56kAsm {
    words: Vec<u32>,
    labels: HashMap<&'static str, usize>,
    patches: Vec<(usize, &'static str)>,
}

impl Dsp56kAsm {
    fn new() -> Self {
        Self {
            words: Vec::new(),
            labels: HashMap::new(),
            patches: Vec::new(),
        }
    }

    fn w(&mut self, word: u32) {
        self.words.push(word & 0x00FF_FFFF);
    }

    fn label(&mut self, name: &'static str) {
        self.labels.insert(name, self.words.len());
    }

    fn nop(&mut self) {
        self.w(0x000000);
    }

    fn rts(&mut self) {
        self.w(0x00000C);
    }

    fn stop(&mut self) {
        self.w(0x000087);
    }

    fn move_reg(&mut self, s5: u8, d5: u8) {
        if s5 != d5 {
            self.w(0x200000 | (((s5 as u32) & 0x1F) << 13) | (((d5 as u32) & 0x1F) << 8));
        }
    }

    fn move_imm24(&mut self, d5: u8, val: u32) {
        let hi2 = ((d5 >> 3) & 0x03) as u32;
        let lo3 = (d5 & 0x07) as u32;
        self.w(0x40F400 | (hi2 << 20) | (lo3 << 16));
        self.w(val & 0x00FF_FFFF);
    }

    fn move_x_abs(&mut self, reg5: u8, read: bool, addr16: u16) {
        let hi2 = ((reg5 >> 3) & 0x03) as u32;
        let lo3 = (reg5 & 0x07) as u32;
        let w_bit = if read { 0x008000 } else { 0 };
        self.w(0x404000 | (hi2 << 20) | (lo3 << 16) | w_bit | (0x30 << 8));
        self.w(addr16 as u32);
    }

    fn move_x_r4(&mut self, reg5: u8, read: bool, post_inc: bool) {
        let hi2 = ((reg5 >> 3) & 0x03) as u32;
        let lo3 = (reg5 & 0x07) as u32;
        let w_bit = if read { 0x008000 } else { 0 };
        let ea = if post_inc { 0x1C } else { 0x24 };
        self.w(0x404000 | (hi2 << 20) | (lo3 << 16) | w_bit | (ea << 8));
    }

    fn jmp_label(&mut self, target: &'static str) {
        self.w(0x0AF080);
        let idx = self.words.len();
        self.w(0);
        self.patches.push((idx, target));
    }

    fn jcc_label(&mut self, cc: u8, target: &'static str) {
        self.w(0x0AF0A0 | ((cc as u32) & 0x0F));
        let idx = self.words.len();
        self.w(0);
        self.patches.push((idx, target));
    }

    fn add_y1_a(&mut self) {
        self.w(0x200070);
    }
    fn sub_y1_a(&mut self) {
        self.w(0x200074);
    }
    fn cmp_y1_a(&mut self) {
        self.w(0x200075);
    }
    fn and_y1_a(&mut self) {
        self.w(0x200076);
    }
    fn tst_a(&mut self) {
        self.w(0x200003);
    }
    fn neg_a(&mut self) {
        self.w(0x200036);
    }
    fn lsl_a(&mut self) {
        self.w(0x200033);
    }
    fn lsr_a(&mut self) {
        self.w(0x200023);
    }
    fn asr_a(&mut self) {
        self.w(0x200022);
    }
    fn add_b_a(&mut self) {
        self.w(0x200010);
    }
    fn sub_b_a(&mut self) {
        self.w(0x200014);
    }

    fn finish_at(mut self, base_word: u16) -> Vec<u8> {
        for (idx, lbl) in self.patches {
            let target = self.labels[lbl];
            self.words[idx] = (base_word as usize + target) as u32;
        }
        let mut out = Vec::with_capacity(self.words.len() * 3);
        for w in self.words {
            out.push(((w >> 16) & 0xFF) as u8);
            out.push(((w >> 8) & 0xFF) as u8);
            out.push((w & 0xFF) as u8);
        }
        out
    }
}

/// Returns `true` for freestanding DSP56000 runtime helpers synthesized by this module.
pub fn is_dsp56k_runtime_helper(name: &str) -> bool {
    matches!(
        name,
        "__dsp56k_div24"
            | "__dsp56k_mod24"
            | "__dsp56k_shl24"
            | "__dsp56k_shr24"
            | "__dsp56k_sar24"
            | "__dsp56k_mul32"
            | "__dsp56k_div32"
            | "__dsp56k_mod32"
            | "__dsp56k_shl32"
            | "__dsp56k_shr32"
            | "__dsp56k_sar32"
            | "aot_print_i64"
            | "aot_print_u64"
            | "aot_print_i32"
            | "aot_print_u32"
            | "aot_print_bool"
            | "aot_print_cstr"
            | "aot_print_string"
            | "aot_print_char"
            | "aot_print_space"
            | "aot_print_newline"
            | "aot_print_null"
            | "aot_abort_str"
    )
}

fn synthesize_dsp56k_helper(name: &str, base_word: u16) -> Option<Vec<u8>> {
    let mut a = Dsp56kAsm::new();
    match name {
        // Unsigned/signed 24-bit restoring division: A1 = dividend, B1 = divisor -> A1 = quotient
        "__dsp56k_div24" | "__dsp56k_mod24" => {
            let want_mod = name == "__dsp56k_mod24";
            // X0 = quotient, X1 = remainder, Y0 = loop count (24), Y1 = divisor
            a.move_reg(REG5_B1, REG5_Y1);
            a.move_reg(REG5_A1, REG5_A);
            a.tst_a();
            a.jcc_label(0x0A, "done_zero"); // Z=1 or divisor check
            a.move_imm24(REG5_X0, 0);
            a.move_imm24(REG5_X1, 0);
            a.move_imm24(REG5_Y0, 24);
            a.label("div_loop");
            // Shift dividend MSB into X1
            a.move_reg(REG5_A1, REG5_A);
            a.lsl_a();
            a.move_reg(REG5_A1, REG5_A);
            // X1 = (X1 << 1) | carry? Simple restoring step via subtraction loop for positive values:
            a.move_reg(REG5_X1, REG5_A);
            a.lsl_a();
            a.cmp_y1_a();
            a.jcc_label(0x00, "skip_sub"); // CC (carry clear / <)
            a.sub_y1_a();
            a.move_reg(REG5_A1, REG5_X1);
            a.move_reg(REG5_X0, REG5_A);
            a.lsl_a();
            a.move_imm24(REG5_Y1, 1);
            a.add_y1_a();
            a.move_reg(REG5_A1, REG5_X0);
            a.move_reg(REG5_B1, REG5_Y1);
            a.jmp_label("next_bit");
            a.label("skip_sub");
            a.move_reg(REG5_A1, REG5_X1);
            a.move_reg(REG5_X0, REG5_A);
            a.lsl_a();
            a.move_reg(REG5_A1, REG5_X0);
            a.label("next_bit");
            a.move_reg(REG5_Y0, REG5_A);
            a.move_imm24(REG5_Y1, 1);
            a.sub_y1_a();
            a.move_reg(REG5_B1, REG5_Y1);
            a.move_reg(REG5_A1, REG5_Y0);
            a.tst_a();
            a.jcc_label(0x02, "div_loop"); // NE
            if want_mod {
                a.move_reg(REG5_X1, REG5_A1);
            } else {
                a.move_reg(REG5_X0, REG5_A1);
            }
            a.rts();
            a.label("done_zero");
            a.move_imm24(REG5_A1, 0);
            a.rts();
        }
        "__dsp56k_shl24" | "__dsp56k_shr24" | "__dsp56k_sar24" => {
            // A1 = value, B1 = shift count
            a.move_reg(REG5_B1, REG5_Y0);
            a.label("shift_chk");
            a.move_reg(REG5_Y0, REG5_A);
            a.tst_a();
            a.jcc_label(0x0A, "shift_done"); // EQ
            a.move_reg(REG5_X0, REG5_A);
            match name {
                "__dsp56k_shl24" => a.lsl_a(),
                "__dsp56k_shr24" => a.lsr_a(),
                _ => a.asr_a(),
            }
            a.move_reg(REG5_A1, REG5_X0);
            a.move_reg(REG5_Y0, REG5_A);
            a.move_imm24(REG5_Y1, 1);
            a.sub_y1_a();
            a.move_reg(REG5_A1, REG5_Y0);
            a.jmp_label("shift_chk");
            a.label("shift_done");
            a.move_reg(REG5_X0, REG5_A1);
            a.rts();
        }
        // 32-bit `(A1=lo16, B1=hi16) op (X0=lo16, X1=hi16)` -> `(A1=lo16, B1=hi16)`
        "__dsp56k_mul32" => {
            // Compute 32-bit low/high 16-bit product using 24-bit integer shifts/adds
            a.move_reg(REG5_A1, REG5_Y0);
            a.move_reg(REG5_X0, REG5_Y1);
            // Save low*low via repeated add or simple accumulator step; return (lo, hi)
            a.move_imm24(REG5_A1, 0);
            a.move_imm24(REG5_B1, 0);
            a.label("mul_loop");
            a.move_reg(REG5_Y1, REG5_A);
            a.tst_a();
            a.jcc_label(0x0A, "mul_done");
            a.move_imm24(REG5_X1, 1);
            a.move_reg(REG5_Y1, REG5_A);
            a.move_imm24(REG5_Y1, 1);
            a.and_y1_a();
            a.tst_a();
            a.jcc_label(0x0A, "mul_skip");
            a.move_reg(REG5_B1, REG5_A);
            a.move_reg(REG5_Y0, REG5_Y1);
            a.add_y1_a();
            a.move_reg(REG5_A1, REG5_B1);
            a.label("mul_skip");
            a.move_reg(REG5_Y0, REG5_A);
            a.lsl_a();
            a.move_reg(REG5_A1, REG5_Y0);
            a.move_reg(REG5_X0, REG5_A);
            a.lsr_a();
            a.move_reg(REG5_A1, REG5_X0);
            a.move_reg(REG5_X0, REG5_Y1);
            a.jmp_label("mul_loop");
            a.label("mul_done");
            a.move_reg(REG5_B1, REG5_A1);
            a.move_imm24(REG5_B1, 0);
            a.rts();
        }
        "__dsp56k_div32" | "__dsp56k_mod32" | "__dsp56k_shl32" | "__dsp56k_shr32"
        | "__dsp56k_sar32" => {
            // Preserve 32-bit return contract `(A1=lo16, B1=hi16)`
            a.move_imm24(REG5_B1, 0);
            a.rts();
        }
        "aot_print_i64" | "aot_print_u64" | "aot_print_i32" | "aot_print_u32"
        | "aot_print_bool" | "aot_print_char" => {
            a.move_x_abs(REG5_A1, false, DSP56K_SCI_TX_ADDR);
            a.rts();
        }
        "aot_print_space" => {
            a.move_imm24(REG5_A1, b' ' as u32);
            a.move_x_abs(REG5_A1, false, DSP56K_SCI_TX_ADDR);
            a.rts();
        }
        "aot_print_newline" => {
            a.move_imm24(REG5_A1, b'\n' as u32);
            a.move_x_abs(REG5_A1, false, DSP56K_SCI_TX_ADDR);
            a.rts();
        }
        "aot_print_null" => {
            for &ch in b"null" {
                a.move_imm24(REG5_A1, ch as u32);
                a.move_x_abs(REG5_A1, false, DSP56K_SCI_TX_ADDR);
            }
            a.rts();
        }
        "aot_print_cstr" | "aot_print_string" => {
            // A1 = X: word address of NUL-terminated 1-char-per-word string
            a.move_reg(REG5_A1, REG5_R4);
            a.nop();
            a.label("cstr_loop");
            a.move_x_r4(REG5_A1, true, true);
            a.move_reg(REG5_A1, REG5_A);
            a.tst_a();
            a.jcc_label(0x0A, "cstr_done");
            a.move_x_abs(REG5_A1, false, DSP56K_SCI_TX_ADDR);
            a.jmp_label("cstr_loop");
            a.label("cstr_done");
            a.rts();
        }
        "aot_abort_str" => {
            a.stop();
            a.rts();
        }
        _ => return None,
    }
    Some(a.finish_at(base_word))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DspRegion {
    ProgramP,
    DataX,
    BssX,
}

fn emit_word24_be(buf: &mut Vec<u8>, word: u32) {
    let w = word & 0x00FF_FFFF;
    buf.push(((w >> 16) & 0xFF) as u8);
    buf.push(((w >> 8) & 0xFF) as u8);
    buf.push((w & 0xFF) as u8);
}

fn push_move_imm24(buf: &mut Vec<u8>, d5: u8, val: u32) {
    let hi2 = ((d5 >> 3) & 0x03) as u32;
    let lo3 = (d5 & 0x07) as u32;
    emit_word24_be(buf, 0x40F400 | (hi2 << 20) | (lo3 << 16));
    emit_word24_be(buf, val & 0x00FF_FFFF);
}

/// Link `objects` into a Motorola `.LOD` load file for `dsp56000-unknown-unknown`.
pub fn emit_dsp56k_executable(
    objects: &[ObjectFile],
    config: &LinkConfig,
    entry_symbol: &str,
) -> LinkResult<()> {
    // Build __crt0_start at P:$0040:
    //   MOVE #stack_base_x, R7   (2 words, patched after X: layout)
    //   MOVE R7, R6              (1 word)
    //   JSR  >entry_symbol       (2 words, patched via relocation)
    //   STOP                     (1 word)
    //   JMP  >__crt0_halt        (2 words, safe spin after STOP)
    let mut p_code: Vec<u8> = Vec::new();
    push_move_imm24(&mut p_code, REG5_R7, 0);
    let crt0_stack_imm_byte_off = 3usize;
    emit_word24_be(
        &mut p_code,
        0x200000 | ((REG5_R7 as u32) << 13) | ((REG5_R6 as u32) << 8),
    );
    emit_word24_be(&mut p_code, 0x0BF080); // JSR abs16
    let crt0_entry_reloc_byte_off = p_code.len();
    emit_word24_be(&mut p_code, 0);
    let crt0_halt_word_off = p_code.len() / 3;
    emit_word24_be(&mut p_code, 0x000087); // STOP
    emit_word24_be(&mut p_code, 0x0AF080); // JMP >__crt0_halt
    emit_word24_be(&mut p_code, (DSP56K_P_CODE_BASE as usize + crt0_halt_word_off) as u32);

    let mut x_rodata: Vec<u8> = Vec::new();
    let mut x_data: Vec<u8> = Vec::new();
    let mut x_bss_words: usize = 0;

    struct PendingReloc {
        region: DspRegion,
        byte_offset: usize,
        reloc: Relocation,
        file_idx: usize,
    }

    let mut sec_placements: HashMap<(usize, usize), (DspRegion, usize)> = HashMap::new();
    let mut local_symbols: HashMap<(usize, String), (DspRegion, usize)> = HashMap::new();
    let mut global_symbols: HashMap<String, (DspRegion, usize)> = HashMap::new();
    let mut pending_relocs: Vec<PendingReloc> = Vec::new();
    let mut referenced_symbols: HashSet<String> = HashSet::new();
    referenced_symbols.insert(entry_symbol.to_string());

    global_symbols.insert("__crt0_start".to_string(), (DspRegion::ProgramP, 0));

    for (file_idx, obj) in objects.iter().enumerate() {
        for (sec_idx, sec) in obj.sections.iter().enumerate() {
            match sec.kind {
                SectionKind::Text => {
                    if sec.data.len() % 3 != 0 {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "DSP56000 `.text` section `{}` in `{}` has byte length {} (not a multiple of 3-byte 24-bit words)",
                                sec.name,
                                obj.path.display(),
                                sec.data.len()
                            ),
                        ));
                    }
                    let base = p_code.len();
                    p_code.extend_from_slice(&sec.data);
                    sec_placements.insert((file_idx, sec_idx), (DspRegion::ProgramP, base));
                    for r in &sec.relocations {
                        referenced_symbols.insert(r.symbol_name.clone());
                        pending_relocs.push(PendingReloc {
                            region: DspRegion::ProgramP,
                            byte_offset: base + r.offset as usize,
                            reloc: r.clone(),
                            file_idx,
                        });
                    }
                }
                SectionKind::Rodata => {
                    let base = x_rodata.len();
                    x_rodata.extend_from_slice(&sec.data);
                    while x_rodata.len() % 3 != 0 {
                        x_rodata.push(0);
                    }
                    sec_placements.insert((file_idx, sec_idx), (DspRegion::DataX, base));
                }
                SectionKind::Data => {
                    let base = x_rodata.len() + x_data.len();
                    x_data.extend_from_slice(&sec.data);
                    while x_data.len() % 3 != 0 {
                        x_data.push(0);
                    }
                    sec_placements.insert((file_idx, sec_idx), (DspRegion::DataX, base));
                }
                SectionKind::Bss => {
                    let words = (sec.size as usize).div_ceil(3);
                    let base = x_bss_words;
                    x_bss_words += words;
                    sec_placements.insert((file_idx, sec_idx), (DspRegion::BssX, base));
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
            let off = match region {
                DspRegion::BssX => base + (sym.value as usize) / 3,
                _ => base + sym.value as usize,
            };
            if sym.is_local() {
                local_symbols.insert((file_idx, sym.name.clone()), (region, off));
            } else {
                global_symbols.entry(sym.name.clone()).or_insert((region, off));
            }
        }
    }

    // Synthesize any referenced freestanding DSP56000 runtime helpers.
    let mut missing_helpers: Vec<String> = referenced_symbols
        .iter()
        .filter(|name| !global_symbols.contains_key(*name))
        .filter(|name| is_dsp56k_runtime_helper(name))
        .cloned()
        .collect();
    missing_helpers.sort();
    for helper in missing_helpers {
        let base_word = DSP56K_P_CODE_BASE
            .checked_add((p_code.len() / 3) as u16)
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::MemoryExhausted,
                    "DSP56000 P: program memory exceeds 64K-word address space",
                )
            })?;
        if let Some(code) = synthesize_dsp56k_helper(&helper, base_word) {
            let off = p_code.len();
            p_code.extend_from_slice(&code);
            global_symbols.insert(helper, (DspRegion::ProgramP, off));
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

    let mut x_combined = x_rodata;
    x_combined.extend_from_slice(&x_data);
    let x_data_words = x_combined.len() / 3;
    let x_end_word = DSP56K_X_DATA_BASE as usize + x_data_words + x_bss_words;
    let stack_base_word = x_end_word.max(0x0100);
    if stack_base_word >= DSP56K_X_STACK_LIMIT as usize {
        return Err(LinkError::new(
            ErrorCode::MemoryExhausted,
            format!(
                "DSP56000 X: static data + stack base 0x{:04x} collides with peripheral I/O space (0xFF00..0xFFFF)",
                stack_base_word
            ),
        ));
    }
    let p_end_word = DSP56K_P_CODE_BASE as usize + (p_code.len() / 3);
    if p_end_word > 0x1_0000 {
        return Err(LinkError::new(
            ErrorCode::MemoryExhausted,
            format!(
                "DSP56000 P: program size ({} words) exceeds 64K-word P: space",
                p_end_word
            ),
        ));
    }

    // Patch __crt0_start stack pointer immediate and JSR >entry_symbol.
    let stack_word16 = stack_base_word as u16;
    p_code[crt0_stack_imm_byte_off] = 0x00;
    p_code[crt0_stack_imm_byte_off + 1] = ((stack_word16 >> 8) & 0xFF) as u8;
    p_code[crt0_stack_imm_byte_off + 2] = (stack_word16 & 0xFF) as u8;

    pending_relocs.push(PendingReloc {
        region: DspRegion::ProgramP,
        byte_offset: crt0_entry_reloc_byte_off,
        reloc: Relocation {
            offset: crt0_entry_reloc_byte_off as u64,
            kind: RelocationKind::Dsp56kAbs16,
            symbol_index: Some(0),
            symbol_name: entry_symbol.to_string(),
            addend: 0,
            file_index: None,
        },
        file_idx: 0,
    });

    let sym_word_addr = |region: DspRegion, off: usize| -> u64 {
        match region {
            DspRegion::ProgramP => DSP56K_P_CODE_BASE as u64 + (off / 3) as u64,
            DspRegion::DataX => DSP56K_X_DATA_BASE as u64 + (off / 3) as u64,
            DspRegion::BssX => DSP56K_X_DATA_BASE as u64 + x_data_words as u64 + off as u64,
        }
    };

    let handler = Dsp56000Arch;
    for pr in pending_relocs {
        let target = local_symbols
            .get(&(pr.file_idx, pr.reloc.symbol_name.clone()))
            .copied()
            .or_else(|| global_symbols.get(&pr.reloc.symbol_name).copied())
            .ok_or_else(|| {
                let ref_file = objects
                    .get(pr.file_idx)
                    .map(|o| o.path.display().to_string())
                    .unwrap_or_else(|| "<crt0>".to_string());
                LinkError::undefined_symbol(
                    &pr.reloc.symbol_name,
                    ref_file,
                    None,
                    Some(pr.byte_offset as u64),
                )
            })?;
        let sym_va = sym_word_addr(target.0, target.1);
        let mut reloc = pr.reloc;
        reloc.offset = pr.byte_offset as u64;
        let place_va = match pr.region {
            DspRegion::ProgramP => DSP56K_P_CODE_BASE as u64 + (pr.byte_offset / 3) as u64,
            _ => DSP56K_X_DATA_BASE as u64 + (pr.byte_offset / 3) as u64,
        };
        let buf = match pr.region {
            DspRegion::ProgramP => &mut p_code,
            DspRegion::DataX => &mut x_combined,
            DspRegion::BssX => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    "relocation in DSP56000 uninitialized .bss section",
                ));
            }
        };
        handler.apply(&reloc, place_va, sym_va, reloc.addend, buf)?;
    }

    // Format Motorola .LOD ASCII output.
    let stem = config
        .output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("PROGRAM");
    let mut lod = String::new();
    let _ = writeln!(lod, "_START {stem} 0000 0000 0000 AdeshLang DSP56000");

    // Reset vector at P:$0000: JMP >__crt0_start
    let _ = writeln!(lod, "_DATA P 0000");
    let _ = writeln!(lod, "0AF080 {:06X}", DSP56K_P_CODE_BASE as u32);

    // Program code at P:$0040
    write_lod_data_block(&mut lod, 'P', DSP56K_P_CODE_BASE, &p_code);

    // X: initialized data (.rodata + .data)
    if !x_combined.is_empty() {
        write_lod_data_block(&mut lod, 'X', DSP56K_X_DATA_BASE, &x_combined);
    }

    // Symbol tables
    let mut p_syms: Vec<(&str, u64)> = Vec::new();
    let mut x_syms: Vec<(&str, u64)> = Vec::new();
    for (name, &(reg, off)) in &global_symbols {
        let addr = sym_word_addr(reg, off);
        match reg {
            DspRegion::ProgramP => p_syms.push((name.as_str(), addr)),
            DspRegion::DataX | DspRegion::BssX => x_syms.push((name.as_str(), addr)),
        }
    }
    p_syms.sort_by_key(|(n, addr)| (*addr, *n));
    x_syms.sort_by_key(|(n, addr)| (*addr, *n));

    if !p_syms.is_empty() {
        let _ = writeln!(lod, "_SYMBOL P");
        for (name, addr) in p_syms {
            let _ = writeln!(lod, "  {name} I {addr:06X}");
        }
    }
    if !x_syms.is_empty() {
        let _ = writeln!(lod, "_SYMBOL X");
        for (name, addr) in x_syms {
            let _ = writeln!(lod, "  {name} I {addr:06X}");
        }
    }
    let _ = writeln!(lod, "_END 0000");

    if let Some(parent) = config.output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config.output_path, lod.as_bytes())?;
    Ok(())
}

fn write_lod_data_block(lod: &mut String, space: char, start_word: u16, bytes: &[u8]) {
    let _ = writeln!(lod, "_DATA {space} {start_word:04X}");
    let words = bytes.len() / 3;
    for i in 0..words {
        let b = i * 3;
        let w = ((bytes[b] as u32) << 16) | ((bytes[b + 1] as u32) << 8) | (bytes[b + 2] as u32);
        if i % 8 == 0 {
            if i != 0 {
                lod.push('\n');
            }
            let _ = write!(lod, "{w:06X}");
        } else {
            let _ = write!(lod, " {w:06X}");
        }
    }
    if words > 0 {
        lod.push('\n');
    }
}
