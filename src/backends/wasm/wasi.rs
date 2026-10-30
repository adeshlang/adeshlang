//! WASI Preview 1 adapters for the Adesh→WASM compiler.
//!
//! The compiler's default ABI imports `env.*` helpers that a generated Node
//! loader provides. Under `wasm32-*-wasi` those imports do not exist, so the
//! module imports only `wasi_snapshot_preview1.fd_write` and defines small
//! in-module adapters with the same shape as the `env.*` calls the code
//! generator already emits:
//!
//! | index | function | replaces |
//! |-------|----------|----------|
//! | 0 | `fd_write` (import) | — |
//! | 1 | `__adesh_write(i32,i32)` | `env.print_str` |
//! | 2 | `__adesh_alloc(i32)->i32` | `env.alloc` (also exported as `malloc`) |
//! | 3 | `__adesh_print_u64(i64)` | integer half of `env.print_f64` |
//! | 4 | `__adesh_print_frac(i64)` | fractional half of `env.print_f64` |
//! | 5 | `__adesh_print_f64(f64)` | `env.print_f64` |
//!
//! The adapters use fixed scratch addresses below the string data segment
//! (which starts at 1024) and a mutable heap global initialised past the data.

use super::encoder::{write_i32_leb, write_u32_leb};
use std::vec::Vec;

/// iovec `{ ptr, len }` used by every `fd_write` call.
const IOVEC: u32 = 8;
/// `nwritten` out-parameter; its value is ignored.
const NWRITTEN: u32 = 16;
/// Integer digits are written backwards, ending at this address.
const INT_BUF_END: u32 = 96;
/// Six fractional digits (padded, then trimmed) live here.
const FRAC_BUF: u32 = 96;

/// Function indices the compiler needs while lowering user code.
#[derive(Clone, Copy, Debug)]
pub struct WasiAbi {
    pub fd_write: u32,
    pub write: u32,
    pub alloc: u32,
    pub print_u64: u32,
    pub print_frac: u32,
    pub print_f64: u32,
    /// First user-defined function index; adapters occupy 1..=5.
    pub func_base: u32,
    /// Exported as `malloc`.
    pub malloc: u32,
}

impl Default for WasiAbi {
    fn default() -> Self {
        Self {
            fd_write: 0,
            write: 1,
            alloc: 2,
            print_u64: 3,
            print_frac: 4,
            print_f64: 5,
            func_base: 6,
            malloc: 2,
        }
    }
}

/// Type-section indices used by the adapter signatures (see `compiler.rs`).
const TYPE_PRINT_F64: u32 = 0; // (f64)->()
const TYPE_WRITE: u32 = 1; // (i32,i32)->()
const TYPE_ALLOC: u32 = 3; // (i32)->i32
const TYPE_I64_VOID: u32 = 7; // (i64)->()

impl WasiAbi {
    /// Type indices of the five defined adapters, in definition order.
    pub fn function_types(&self) -> [u32; 5] {
        [
            TYPE_WRITE,
            TYPE_ALLOC,
            TYPE_I64_VOID,
            TYPE_I64_VOID,
            TYPE_PRINT_F64,
        ]
    }
}

/// Offsets (and lengths) of the literal strings the adapters print.
#[derive(Clone, Copy, Debug)]
pub struct WasiLiterals {
    pub nan: (u32, u32),
    pub infinity: (u32, u32),
    pub neg_infinity: (u32, u32),
    pub minus: (u32, u32),
    pub dot: (u32, u32),
}

/// Body of `__adesh_write(ptr, len)`: one `fd_write(1, &iov, 1, &nwritten)`.
pub fn write_body(abi: WasiAbi) -> Vec<u8> {
    let mut a = Asm::new();
    a.i32_const(IOVEC as i32).local_get(0).i32_store();
    a.i32_const((IOVEC + 4) as i32).local_get(1).i32_store();
    a.i32_const(1); // stdout
    a.i32_const(IOVEC as i32);
    a.i32_const(1);
    a.i32_const(NWRITTEN as i32);
    a.call(abi.fd_write).drop();
    a.bytes
}

/// Body of `__adesh_alloc(size) -> ptr`.
///
/// Aligns to 8, bumps the `__heap` global, grows linear memory a page at a
/// time when needed, and traps loudly if the host refuses to grow.
pub fn alloc_body(abi: WasiAbi) -> Vec<u8> {
    let mut a = Asm::new();
    // local 1 = aligned size
    a.local_get(0)
        .i32_const(7)
        .i32_add()
        .i32_const(-8)
        .i32_and();
    a.local_set(1);
    // local 2 = end of the new allocation
    a.global_get(0).local_get(1).i32_add().local_set(2);
    a.block();
    a.r#loop();
    a.local_get(2);
    a.memory_size().i32_const(16).i32_shl().i32_le_u();
    a.br_if(1);
    a.i32_const(1).memory_grow().i32_const(-1).i32_eq();
    a.if_void();
    a.unreachable();
    a.end(); // if
    a.br(0);
    a.end(); // loop
    a.end(); // block
    a.local_get(2).global_set(0);
    a.global_get(0).local_get(1).i32_sub();
    let _ = abi;
    a.bytes
}

/// Body of `__adesh_print_u64(v)`: decimal digits then one `write`.
pub fn print_u64_body(abi: WasiAbi) -> Vec<u8> {
    let mut a = Asm::new();
    a.i32_const(INT_BUF_END as i32).local_set(1);
    a.block();
    a.r#loop();
    a.local_get(1);
    a.i32_const(1).i32_sub();
    a.local_tee(1);
    a.local_get(0).i64_const(10).i64_rem_u().i32_wrap_i64();
    a.i32_const(b'0' as i32).i32_add();
    a.i32_store8();
    a.local_get(0).i64_const(10).i64_div_u().local_set(0);
    a.local_get(0).i64_eqz().i32_eqz();
    a.br_if(0);
    a.end(); // loop
    a.end(); // block
    a.local_get(1);
    a.i32_const(INT_BUF_END as i32).local_get(1).i32_sub();
    a.call(abi.write);
    a.bytes
}

/// Body of `__adesh_print_frac(u)`: `.` plus up to six trimmed digits.
///
/// `u` is in `1..=999_999`; it carries the already-rounded fractional digits.
pub fn print_frac_body(abi: WasiAbi, dot: (u32, u32)) -> Vec<u8> {
    let mut a = Asm::new();
    // Build six zero-padded digits at FRAC_BUF, least significant last.
    a.i32_const(6).local_set(1);
    a.block();
    a.r#loop();
    a.local_get(1);
    a.i32_const(1).i32_sub();
    a.local_tee(1);
    a.i32_const(FRAC_BUF as i32).i32_add();
    a.local_get(0).i64_const(10).i64_rem_u().i32_wrap_i64();
    a.i32_const(b'0' as i32).i32_add();
    a.i32_store8();
    a.local_get(0).i64_const(10).i64_div_u().local_set(0);
    a.local_get(1);
    a.br_if(0);
    a.end(); // loop
    a.end(); // block

    // Trim trailing zeros: local 2 = length.
    a.i32_const(6).local_set(2);
    a.block();
    a.r#loop();
    a.local_get(2).i32_eqz();
    a.br_if(1);
    a.i32_const(FRAC_BUF as i32)
        .local_get(2)
        .i32_add()
        .i32_const(1)
        .i32_sub();
    a.i32_load8_u();
    a.i32_const(b'0' as i32).i32_ne();
    a.br_if(1);
    a.local_get(2).i32_const(1).i32_sub().local_set(2);
    a.br(0);
    a.end(); // loop
    a.end(); // block

    a.local_get(2);
    a.if_void();
    a.i32_const(dot.0 as i32)
        .i32_const(dot.1 as i32)
        .call(abi.write);
    a.i32_const(FRAC_BUF as i32).local_get(2).call(abi.write);
    a.end(); // if
    a.bytes
}

/// Body of `__adesh_print_f64(x)`.
///
/// Rounds to six fractional digits, so `0.1 + 0.2` prints `0.3` where
/// JavaScript's `String()` would print `0.30000000000000004`. Integral values
/// print without a decimal point, matching the Node loader.
pub fn print_f64_body(abi: WasiAbi, lits: WasiLiterals) -> Vec<u8> {
    let mut a = Asm::new();

    // NaN
    a.local_get(0).local_get(0).f64_ne();
    a.if_void();
    a.i32_const(lits.nan.0 as i32)
        .i32_const(lits.nan.1 as i32)
        .call(abi.write);
    a.return_();
    a.end();

    // +Infinity / -Infinity
    a.local_get(0).f64_const(f64::INFINITY).f64_eq();
    a.if_void();
    a.i32_const(lits.infinity.0 as i32)
        .i32_const(lits.infinity.1 as i32)
        .call(abi.write);
    a.return_();
    a.end();
    a.local_get(0).f64_const(f64::NEG_INFINITY).f64_eq();
    a.if_void();
    a.i32_const(lits.neg_infinity.0 as i32)
        .i32_const(lits.neg_infinity.1 as i32)
        .call(abi.write);
    a.return_();
    a.end();

    // Sign (zero is not negative, matching String(-0) == "0")
    a.local_get(0).f64_const(0.0).f64_lt();
    a.if_void();
    a.i32_const(lits.minus.0 as i32)
        .i32_const(lits.minus.1 as i32)
        .call(abi.write);
    a.local_get(0).f64_neg().local_set(0);
    a.end();

    // local 1 (i64) = truncated integer part
    a.local_get(0)
        .f64_trunc()
        .i64_trunc_sat_f64_u()
        .local_set(1);
    // local 2 (i64) = fractional digits, rounded half up to the nearest millionth
    a.local_get(0).local_get(0).f64_trunc().f64_sub();
    a.f64_const(1_000_000.0)
        .f64_mul()
        .f64_const(0.5)
        .f64_add()
        .f64_floor();
    a.i64_trunc_sat_f64_u().local_set(2);

    // A rounded fraction of 1.0 carries into the integer part.
    a.local_get(2).i64_const(1_000_000).i64_eq();
    a.if_void();
    a.local_get(1).i64_const(1).i64_add().local_set(1);
    a.i64_const(0).local_set(2);
    a.end();

    a.local_get(1).call(abi.print_u64);
    a.local_get(2).i64_eqz().i32_eqz();
    a.if_void();
    a.local_get(2).call(abi.print_frac);
    a.end();
    a.bytes
}

/// Code-section entries for the five adapters, in index order.
///
/// Each entry is `(i32_locals, i64_locals, f64_locals, body)`.
pub fn adapter_bodies(abi: WasiAbi, lits: WasiLiterals) -> [(u32, u32, u32, Vec<u8>); 5] {
    [
        (0, 0, 0, write_body(abi)),
        (2, 0, 0, alloc_body(abi)),
        (1, 0, 0, print_u64_body(abi)),
        (2, 0, 0, print_frac_body(abi, lits.dot)),
        (0, 2, 0, print_f64_body(abi, lits)),
    ]
}

/// Minimal bytecode builder so the adapters read as instruction lists.
struct Asm {
    bytes: Vec<u8>,
}

#[allow(dead_code)]
impl Asm {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn op(&mut self, opcode: u8) -> &mut Self {
        self.bytes.push(opcode);
        self
    }

    fn i32_const(&mut self, v: i32) -> &mut Self {
        self.bytes.push(0x41);
        write_i32_leb(v, &mut self.bytes);
        self
    }

    fn i64_const(&mut self, v: i64) -> &mut Self {
        self.bytes.push(0x42);
        write_i64_leb(v, &mut self.bytes);
        self
    }

    fn f64_const(&mut self, v: f64) -> &mut Self {
        self.bytes.push(0x44);
        self.bytes.extend_from_slice(&v.to_bits().to_le_bytes());
        self
    }

    fn local_get(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x20);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn local_set(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x21);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn local_tee(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x22);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn global_get(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x23);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn global_set(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x24);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn call(&mut self, i: u32) -> &mut Self {
        self.bytes.push(0x10);
        write_u32_leb(i, &mut self.bytes);
        self
    }

    fn drop(&mut self) -> &mut Self {
        self.op(0x1A)
    }

    fn unreachable(&mut self) -> &mut Self {
        self.op(0x00)
    }

    fn block(&mut self) -> &mut Self {
        self.bytes.push(0x02);
        self.bytes.push(0x40);
        self
    }

    fn r#loop(&mut self) -> &mut Self {
        self.bytes.push(0x03);
        self.bytes.push(0x40);
        self
    }

    fn if_void(&mut self) -> &mut Self {
        self.bytes.push(0x04);
        self.bytes.push(0x40);
        self
    }

    fn end(&mut self) -> &mut Self {
        self.op(0x0B)
    }

    fn br(&mut self, depth: u32) -> &mut Self {
        self.bytes.push(0x0C);
        write_u32_leb(depth, &mut self.bytes);
        self
    }

    fn br_if(&mut self, depth: u32) -> &mut Self {
        self.bytes.push(0x0D);
        write_u32_leb(depth, &mut self.bytes);
        self
    }

    fn return_(&mut self) -> &mut Self {
        self.op(0x0F)
    }

    fn memory_size(&mut self) -> &mut Self {
        self.bytes.push(0x3F);
        self.bytes.push(0x00);
        self
    }

    fn memory_grow(&mut self) -> &mut Self {
        self.bytes.push(0x40);
        self.bytes.push(0x00);
        self
    }

    fn i32_store(&mut self) -> &mut Self {
        self.bytes.push(0x36);
        self.bytes.push(0x02); // align 2^2 = 4
        self.bytes.push(0x00); // offset 0
        self
    }

    fn i32_store8(&mut self) -> &mut Self {
        self.bytes.push(0x3A);
        self.bytes.push(0x00);
        self.bytes.push(0x00);
        self
    }

    fn i32_load8_u(&mut self) -> &mut Self {
        self.bytes.push(0x2D);
        self.bytes.push(0x00);
        self.bytes.push(0x00);
        self
    }

    fn i32_add(&mut self) -> &mut Self {
        self.op(0x6A)
    }

    fn i32_sub(&mut self) -> &mut Self {
        self.op(0x6B)
    }

    fn i32_and(&mut self) -> &mut Self {
        self.op(0x71)
    }

    fn i32_shl(&mut self) -> &mut Self {
        self.op(0x74)
    }

    fn i32_eqz(&mut self) -> &mut Self {
        self.op(0x45)
    }

    fn i32_ne(&mut self) -> &mut Self {
        self.op(0x47)
    }

    fn i32_eq(&mut self) -> &mut Self {
        self.op(0x46)
    }

    fn i32_le_u(&mut self) -> &mut Self {
        self.op(0x4D)
    }

    fn i32_wrap_i64(&mut self) -> &mut Self {
        self.op(0xA7)
    }

    fn i64_eqz(&mut self) -> &mut Self {
        self.op(0x50)
    }

    fn i64_eq(&mut self) -> &mut Self {
        self.op(0x51)
    }

    fn i64_add(&mut self) -> &mut Self {
        self.op(0x7C)
    }

    fn i64_div_u(&mut self) -> &mut Self {
        self.op(0x80)
    }

    fn i64_rem_u(&mut self) -> &mut Self {
        self.op(0x82)
    }

    fn i64_trunc_sat_f64_u(&mut self) -> &mut Self {
        self.bytes.push(0xFC);
        write_u32_leb(7, &mut self.bytes);
        self
    }

    fn f64_eq(&mut self) -> &mut Self {
        self.op(0x61)
    }

    fn f64_ne(&mut self) -> &mut Self {
        self.op(0x62)
    }

    fn f64_lt(&mut self) -> &mut Self {
        self.op(0x63)
    }

    fn f64_neg(&mut self) -> &mut Self {
        self.op(0x9A)
    }

    fn f64_trunc(&mut self) -> &mut Self {
        self.op(0x9D)
    }

    fn f64_floor(&mut self) -> &mut Self {
        self.op(0x9C)
    }

    fn f64_add(&mut self) -> &mut Self {
        self.op(0xA0)
    }

    fn f64_sub(&mut self) -> &mut Self {
        self.op(0xA1)
    }

    fn f64_mul(&mut self) -> &mut Self {
        self.op(0xA2)
    }
}

fn write_i64_leb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (v == 0 && !sign_bit) || (v == -1 && sign_bit) {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }
}
