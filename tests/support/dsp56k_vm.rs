//! Emitted-bytes Motorola DSP56000 interpreter harness for Milestone 2.
//!
//! Parses Motorola `.LOD` files produced by `adesh build --target dsp56000-unknown-unknown`
//! and executes the actual 24-bit/48-bit big-endian machine words in `P:`/`X:` memory
//! starting from reset vector `P:$0000` until `STOP` (`0x000087`).

#![forbid(unsafe_code)]

use std::fmt;

pub const DSP56K_SCI_TX_ADDR: u16 = 0xFFE0;

#[derive(Debug)]
pub struct Dsp56kVmError {
    pub pc: u16,
    pub word: u32,
    pub message: String,
}

impl fmt::Display for Dsp56kVmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DSP56000 VM error at P:${:04X} (word {:06X}): {}",
            self.pc, self.word, self.message
        )
    }
}

pub struct Dsp56kVm {
    pub p_mem: Vec<u32>,
    pub x_mem: Vec<u32>,
    pub y_mem: Vec<u32>,
    pub pc: u16,
    pub a: i64, // 56-bit signed accumulator (A2:A1:A0)
    pub b: i64, // 56-bit signed accumulator (B2:B1:B0)
    pub x0: u32,
    pub x1: u32,
    pub y0: u32,
    pub y1: u32,
    pub r: [u16; 8],
    pub n: [u16; 8],
    #[allow(dead_code)]
    pub m: [u16; 8],
    pub ssh: Vec<u16>,
    pub c_flag: bool,
    pub z_flag: bool,
    pub n_flag: bool,
    pub v_flag: bool,
    pub sci_tx_words: Vec<u32>,
    pub halted: bool,
    pub steps: usize,
}

#[inline]
fn sign_ext_56(v: i64) -> i64 {
    (v << 8) >> 8
}

#[inline]
fn sign_ext_24(v: u32) -> i64 {
    let w = (v & 0x00FF_FFFF) as i32;
    ((w << 8) >> 8) as i64
}

impl Dsp56kVm {
    pub fn from_lod(lod: &str) -> Result<Self, String> {
        let mut vm = Self {
            p_mem: vec![0; 65536],
            x_mem: vec![0; 65536],
            y_mem: vec![0; 65536],
            pc: 0,
            a: 0,
            b: 0,
            x0: 0,
            x1: 0,
            y0: 0,
            y1: 0,
            r: [0; 8],
            n: [0; 8],
            m: [0xFFFF; 8],
            ssh: Vec::new(),
            c_flag: false,
            z_flag: false,
            n_flag: false,
            v_flag: false,
            sci_tx_words: Vec::new(),
            halted: false,
            steps: 0,
        };

        #[derive(Clone, Copy)]
        enum Space {
            None,
            P,
            X,
            Y,
        }
        let mut space = Space::None;
        let mut cursor: usize = 0;

        for raw in lod.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("_START") || line.starts_with("_SYMBOL") {
                space = Space::None;
                continue;
            }
            if let Some(rest) = line.strip_prefix("_END") {
                let entry = rest.trim();
                vm.pc = u16::from_str_radix(entry, 16)
                    .map_err(|e| format!("invalid _END address `{entry}`: {e}"))?;
                break;
            }
            if let Some(rest) = line.strip_prefix("_DATA ") {
                let mut parts = rest.split_whitespace();
                let sp = parts.next().ok_or("missing _DATA space")?;
                let addr_s = parts.next().ok_or("missing _DATA addr")?;
                cursor = u16::from_str_radix(addr_s, 16)
                    .map_err(|e| format!("invalid _DATA addr `{addr_s}`: {e}"))?
                    as usize;
                space = match sp {
                    "P" => Space::P,
                    "X" => Space::X,
                    "Y" => Space::Y,
                    other => return Err(format!("unsupported _DATA space `{other}`")),
                };
                continue;
            }
            match space {
                Space::P => {
                    for tok in line.split_whitespace() {
                        let w = u32::from_str_radix(tok, 16)
                            .map_err(|e| format!("bad P word `{tok}`: {e}"))?;
                        vm.p_mem[cursor & 0xFFFF] = w & 0x00FF_FFFF;
                        cursor += 1;
                    }
                }
                Space::X => {
                    for tok in line.split_whitespace() {
                        let w = u32::from_str_radix(tok, 16)
                            .map_err(|e| format!("bad X word `{tok}`: {e}"))?;
                        vm.x_mem[cursor & 0xFFFF] = w & 0x00FF_FFFF;
                        cursor += 1;
                    }
                }
                Space::Y => {
                    for tok in line.split_whitespace() {
                        let w = u32::from_str_radix(tok, 16)
                            .map_err(|e| format!("bad Y word `{tok}`: {e}"))?;
                        vm.y_mem[cursor & 0xFFFF] = w & 0x00FF_FFFF;
                        cursor += 1;
                    }
                }
                Space::None => {}
            }
        }
        Ok(vm)
    }

    #[allow(dead_code)]
    pub fn sci_tx_ascii(&self) -> String {
        self.sci_tx_words
            .iter()
            .filter_map(|&w| char::from_u32(w & 0xFF))
            .collect()
    }

    fn read_reg5(&self, reg5: u8) -> Result<u32, String> {
        match reg5 & 0x1F {
            0x04 => Ok(self.x0 & 0x00FF_FFFF),
            0x05 => Ok(self.x1 & 0x00FF_FFFF),
            0x06 => Ok(self.y0 & 0x00FF_FFFF),
            0x07 => Ok(self.y1 & 0x00FF_FFFF),
            0x08 => Ok((self.a as u64 & 0x00FF_FFFF) as u32), // A0
            0x09 => Ok((self.b as u64 & 0x00FF_FFFF) as u32), // B0
            0x0C => Ok(((self.a >> 24) as u64 & 0x00FF_FFFF) as u32), // A1
            0x0D => Ok(((self.b >> 24) as u64 & 0x00FF_FFFF) as u32), // B1
            0x0E => Ok(((self.a >> 24) as u64 & 0x00FF_FFFF) as u32), // A (saturating read -> A1 for 24-bit range)
            0x0F => Ok(((self.b >> 24) as u64 & 0x00FF_FFFF) as u32), // B
            0x10..=0x17 => Ok(self.r[(reg5 & 0x07) as usize] as u32),
            0x18..=0x1F => Ok(self.n[(reg5 & 0x07) as usize] as u32),
            other => Err(format!("unhandled source reg5 0x{other:02X}")),
        }
    }

    fn write_reg5(&mut self, reg5: u8, val: u32) -> Result<(), String> {
        let v24 = val & 0x00FF_FFFF;
        match reg5 & 0x1F {
            0x04 => self.x0 = v24,
            0x05 => self.x1 = v24,
            0x06 => self.y0 = v24,
            0x07 => self.y1 = v24,
            0x08 => self.a = sign_ext_56((self.a & !0x00FF_FFFF) | (v24 as i64)), // A0
            0x09 => self.b = sign_ext_56((self.b & !0x00FF_FFFF) | (v24 as i64)), // B0
            0x0C => {
                // A1
                let hi = self.a & !((0x00FF_FFFFi64) << 24);
                self.a = sign_ext_56(hi | ((v24 as i64) << 24));
            }
            0x0D => {
                // B1
                let hi = self.b & !((0x00FF_FFFFi64) << 24);
                self.b = sign_ext_56(hi | ((v24 as i64) << 24));
            }
            0x0E => self.a = sign_ext_24(v24) << 24, // A (sign-extends into A2, clears A0)
            0x0F => self.b = sign_ext_24(v24) << 24, // B
            0x10..=0x17 => self.r[(reg5 & 0x07) as usize] = v24 as u16,
            0x18..=0x1F => self.n[(reg5 & 0x07) as usize] = v24 as u16,
            other => return Err(format!("unhandled dest reg5 0x{other:02X}")),
        }
        Ok(())
    }

    fn write_x(&mut self, addr: u16, val: u32) {
        let v24 = val & 0x00FF_FFFF;
        if addr == DSP56K_SCI_TX_ADDR {
            self.sci_tx_words.push(v24);
        } else {
            self.x_mem[addr as usize] = v24;
        }
    }

    fn resolve_ea(&mut self, mmmrrr: u8) -> Result<u16, String> {
        let rn = (mmmrrr & 0x07) as usize;
        let mode = (mmmrrr >> 3) & 0x07;
        match mode {
            0b010 => {
                // (Rn)+Nn
                let addr = self.r[rn];
                self.r[rn] = addr.wrapping_add(self.n[rn]);
                Ok(addr)
            }
            0b011 => {
                // (Rn)+
                let addr = self.r[rn];
                self.r[rn] = addr.wrapping_add(1);
                Ok(addr)
            }
            0b100 => {
                // (Rn)
                Ok(self.r[rn])
            }
            0b101 => {
                // (Rn + Nn)
                Ok(self.r[rn].wrapping_add(self.n[rn]))
            }
            0b111 => {
                // -(Rn)
                self.r[rn] = self.r[rn].wrapping_sub(1);
                Ok(self.r[rn])
            }
            0b110 => {
                // >abs16 extension word
                let addr = self.p_mem[self.pc as usize] as u16;
                self.pc = self.pc.wrapping_add(1);
                Ok(addr)
            }
            other => Err(format!("unsupported EA mode 0b{other:03b} (mmmrrr=0x{mmmrrr:02X})")),
        }
    }

    fn cond_holds(&self, cc: u8) -> Result<bool, String> {
        match cc & 0x0F {
            0x0 => Ok(!self.c_flag),                          // CC
            0x1 => Ok(self.n_flag == self.v_flag),            // GE
            0x2 => Ok(!self.z_flag),                          // NE
            0x7 => Ok(!self.z_flag && (self.n_flag == self.v_flag)), // GT
            0x8 => Ok(self.c_flag),                           // CS
            0x9 => Ok(self.n_flag != self.v_flag),            // LT
            0xA => Ok(self.z_flag),                           // EQ
            0xE => Ok(self.c_flag || self.z_flag),            // LS
            0xF => Ok(self.z_flag || (self.n_flag != self.v_flag)), // LE
            other => Err(format!("unsupported condition code 0x{other:X}")),
        }
    }

    fn update_flags_56(&mut self, res: i64, carry: bool, overflow: bool) {
        let r = sign_ext_56(res);
        self.a = r;
        self.z_flag = r == 0;
        self.n_flag = r < 0;
        self.c_flag = carry;
        self.v_flag = overflow;
    }

    fn exec_alu_byte(&mut self, op: u8) -> Result<(), String> {
        match op {
            0x03 => {
                // TST A
                self.z_flag = self.a == 0;
                self.n_flag = self.a < 0;
                self.v_flag = false;
                self.c_flag = false;
            }
            0x17 => {
                // NOT A (bitwise NOT of A1)
                let a1 = (((self.a >> 24) as u32) ^ 0x00FF_FFFF) & 0x00FF_FFFF;
                self.write_reg5(0x0C, a1)?;
                self.z_flag = a1 == 0;
                self.n_flag = (a1 & 0x0080_0000) != 0;
                self.v_flag = false;
            }
            0x1B => {
                // CLR B
                self.b = 0;
                self.z_flag = true;
                self.n_flag = false;
                self.v_flag = false;
                self.c_flag = false;
            }
            0x22 => {
                // ASR A
                let c = (self.a & 1) != 0;
                let r = sign_ext_56(self.a >> 1);
                self.update_flags_56(r, c, false);
            }
            0x23 => {
                // LSR A (logical right shift of A1)
                let a1 = ((self.a >> 24) as u32) & 0x00FF_FFFF;
                let c = (a1 & 1) != 0;
                let shifted = a1 >> 1;
                self.write_reg5(0x0C, shifted)?;
                self.c_flag = c;
                self.z_flag = shifted == 0;
                self.n_flag = false;
                self.v_flag = false;
            }
            0x32 => {
                // ASL A
                let c = (self.a & (1i64 << 55)) != 0;
                let r = sign_ext_56(self.a << 1);
                self.update_flags_56(r, c, false);
            }
            0x33 => {
                // LSL A (logical left shift of A1)
                let a1 = ((self.a >> 24) as u32) & 0x00FF_FFFF;
                let c = (a1 & 0x0080_0000) != 0;
                let shifted = (a1 << 1) & 0x00FF_FFFF;
                self.write_reg5(0x0C, shifted)?;
                self.c_flag = c;
                self.z_flag = shifted == 0;
                self.n_flag = (shifted & 0x0080_0000) != 0;
                self.v_flag = false;
            }
            0x36 => {
                // NEG A
                let r = sign_ext_56(-self.a);
                self.update_flags_56(r, self.a != 0, false);
            }
            0xA0 => {
                // MPY +X0, Y0, A: A = (X0 * Y0) << 1
                let prod = sign_ext_24(self.x0) * sign_ext_24(self.y0);
                let r = sign_ext_56(prod << 1);
                self.update_flags_56(r, false, false);
            }
            0xA2 => {
                // MAC +X0, Y0, A: A += (X0 * Y0) << 1
                let prod = sign_ext_24(self.x0) * sign_ext_24(self.y0);
                let r = sign_ext_56(self.a + (prod << 1));
                self.update_flags_56(r, false, false);
            }
            _ => {
                let jjj = op & 0xF0;
                let alu = op & 0x0F;
                let s_val_24 = match jjj {
                    0x10 => ((self.b >> 24) as u32) & 0x00FF_FFFF, // B1
                    0x40 => self.x0 & 0x00FF_FFFF,
                    0x50 => self.y0 & 0x00FF_FFFF,
                    0x60 => self.x1 & 0x00FF_FFFF,
                    0x70 => self.y1 & 0x00FF_FFFF,
                    _ => return Err(format!("unknown Data ALU opcode byte 0x{op:02X}")),
                };
                let s_56 = sign_ext_24(s_val_24) << 24;
                let a1_24 = ((self.a >> 24) as u32) & 0x00FF_FFFF;
                match alu {
                    0x0 => {
                        // ADD S, A
                        let sum = self.a + s_56;
                        let u_a = (self.a as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        let u_s = (s_56 as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        let carry = u_a.checked_add(u_s).is_none_or(|v| v > 0x00FF_FFFF_FFFF_FFFF);
                        let ov = ((self.a ^ sum) & (s_56 ^ sum)) < 0;
                        self.update_flags_56(sum, carry, ov);
                    }
                    0x2 => {
                        // OR S, A (A1 |= S)
                        let r = (a1_24 | s_val_24) & 0x00FF_FFFF;
                        self.write_reg5(0x0C, r)?;
                        self.z_flag = r == 0;
                        self.n_flag = (r & 0x0080_0000) != 0;
                        self.v_flag = false;
                    }
                    0x3 => {
                        // EOR S, A (A1 ^= S)
                        let r = (a1_24 ^ s_val_24) & 0x00FF_FFFF;
                        self.write_reg5(0x0C, r)?;
                        self.z_flag = r == 0;
                        self.n_flag = (r & 0x0080_0000) != 0;
                        self.v_flag = false;
                    }
                    0x4 => {
                        // SUB S, A
                        let diff = self.a - s_56;
                        let u_a = (self.a as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        let u_s = (s_56 as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        let carry = u_a < u_s;
                        let ov = ((self.a ^ s_56) & (self.a ^ diff)) < 0;
                        self.update_flags_56(diff, carry, ov);
                    }
                    0x5 => {
                        // CMP S, A
                        let diff = sign_ext_56(self.a - s_56);
                        let u_a = (self.a as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        let u_s = (s_56 as u64) & 0x00FF_FFFF_FFFF_FFFF;
                        self.c_flag = u_a < u_s;
                        self.z_flag = diff == 0;
                        self.n_flag = diff < 0;
                        self.v_flag = ((self.a ^ s_56) & (self.a ^ diff)) < 0;
                    }
                    0x6 => {
                        // AND S, A (A1 &= S)
                        let r = (a1_24 & s_val_24) & 0x00FF_FFFF;
                        self.write_reg5(0x0C, r)?;
                        self.z_flag = r == 0;
                        self.n_flag = (r & 0x0080_0000) != 0;
                        self.v_flag = false;
                    }
                    _ => return Err(format!("unknown Data ALU op nibble 0x{alu:X} in 0x{op:02X}")),
                }
            }
        }
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), Dsp56kVmError> {
        if self.halted {
            return Ok(());
        }
        let pc = self.pc;
        let word = self.p_mem[pc as usize] & 0x00FF_FFFF;
        self.pc = self.pc.wrapping_add(1);
        self.steps += 1;

        let res = self.exec_word(word);
        res.map_err(|message| Dsp56kVmError { pc, word, message })
    }

    fn exec_word(&mut self, word: u32) -> Result<(), String> {
        match word {
            0x000000 => Ok(()), // NOP
            0x00000C => {
                // RTS
                self.pc = self
                    .ssh
                    .pop()
                    .ok_or_else(|| "RTS on empty hardware SSH stack".to_string())?;
                Ok(())
            }
            0x000087 => {
                // STOP
                self.halted = true;
                Ok(())
            }
            w if (w & 0xFF00FF) == 0x0600A0 => {
                // REP #count
                let count = ((w >> 8) & 0xFF) as usize;
                let rep_word = self.p_mem[self.pc as usize] & 0x00FF_FFFF;
                self.pc = self.pc.wrapping_add(1);
                for _ in 0..count {
                    self.exec_word(rep_word)?;
                }
                Ok(())
            }
            0x0AF080 => {
                // JMP >abs16
                let target = self.p_mem[self.pc as usize] as u16;
                self.pc = target;
                Ok(())
            }
            w if (w & 0xFFFFF0) == 0x0AF0A0 => {
                // Jcc >abs16
                let target = self.p_mem[self.pc as usize] as u16;
                self.pc = self.pc.wrapping_add(1);
                if self.cond_holds((w & 0x0F) as u8)? {
                    self.pc = target;
                }
                Ok(())
            }
            0x0BF080 => {
                // JSR >abs16
                let target = self.p_mem[self.pc as usize] as u16;
                let ret_pc = self.pc.wrapping_add(1);
                if self.ssh.len() >= 15 {
                    return Err("DSP56000 hardware SSH stack overflow (>15 levels)".to_string());
                }
                self.ssh.push(ret_pc);
                self.pc = target;
                Ok(())
            }
            w if (w & 0xFFFF00) == 0x200000 && (w & 0x0000FF) != 0 => {
                // Pure Data ALU op (0x2000xx)
                self.exec_alu_byte((w & 0xFF) as u8)
            }
            w if (w & 0xFC00FF) == 0x200000 => {
                // MOVE S, D: 0x200000 | (s5 << 13) | (d5 << 8)
                let s5 = ((w >> 13) & 0x1F) as u8;
                let d5 = ((w >> 8) & 0x1F) as u8;
                let v = self.read_reg5(s5)?;
                self.write_reg5(d5, v)
            }
            w if (w & 0xE000FF) == 0x200000 => {
                // MOVE #imm8, D (d5 >= 4): 0x200000 | (d5 << 16) | (u8 << 8)
                let d5 = ((w >> 16) & 0x1F) as u8;
                let imm8 = (w >> 8) & 0xFF;
                self.write_reg5(d5, imm8)
            }
            w if (w & 0xC8FFFF) == 0x40F400 => {
                // MOVE #imm24, D: 0x40F400 | (hi2 << 20) | (lo3 << 16)
                let hi2 = ((w >> 20) & 0x03) as u8;
                let lo3 = ((w >> 16) & 0x07) as u8;
                let d5 = (hi2 << 3) | lo3;
                let imm24 = self.p_mem[self.pc as usize] & 0x00FF_FFFF;
                self.pc = self.pc.wrapping_add(1);
                self.write_reg5(d5, imm24)
            }
            w if (w & 0xC840FF) == 0x404000 => {
                // MOVE X:ea, D / MOVE S, X:ea
                let hi2 = ((w >> 20) & 0x03) as u8;
                let lo3 = ((w >> 16) & 0x07) as u8;
                let reg5 = (hi2 << 3) | lo3;
                let read = (w & 0x008000) != 0;
                let mmmrrr = ((w >> 8) & 0x3F) as u8;
                let addr = self.resolve_ea(mmmrrr)?;
                if read {
                    let v = self.x_mem[addr as usize];
                    self.write_reg5(reg5, v)
                } else {
                    let v = self.read_reg5(reg5)?;
                    self.write_x(addr, v);
                    Ok(())
                }
            }
            other => Err(format!("unrecognized DSP56000 instruction word 0x{other:06X}")),
        }
    }

    pub fn run(&mut self, max_steps: usize) -> Result<(), Dsp56kVmError> {
        while !self.halted {
            if self.steps >= max_steps {
                return Err(Dsp56kVmError {
                    pc: self.pc,
                    word: self.p_mem[self.pc as usize],
                    message: format!("exceeded max step budget ({max_steps})"),
                });
            }
            self.step()?;
        }
        Ok(())
    }
}
