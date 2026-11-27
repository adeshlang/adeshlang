# Motorola DSP56000 (24-bit Fixed-Point DSP) Architecture & Feasibility Study

Target triple: `dsp56000-unknown-unknown`  
Verification Tier: **Tier 2 (Experimental — Emission-Tested Only)**  
Reference Hardware: Motorola DSP56000 / DSP56001 Family (24-bit Fixed-Point DSP)

---

## 1. Executive Feasibility Summary

The Motorola DSP56000 is a specialized 24-bit fixed-point Digital Signal Processor with a modified Harvard architecture featuring **three independent word-addressed memory spaces** (`P:` program, `X:` data, `Y:` data), **56-bit dual accumulators** (`A`, `B`), **24-bit data registers** (`X0`, `X1`, `Y0`, `Y1`), and **16-bit Address Generation Unit (AGU) registers** (`R0..R7`, `N0..N7`, `M0..M7`).

### Key Findings
1. **Not Byte-Addressed**: Every address in `P:`, `X:`, and `Y:` indexes a **24-bit (3-byte) word**, not an 8-bit byte. Any compiler backend or linker that assumes `sizeof(ptr) == 8` or byte-addressable flat memory is fundamentally invalid on DSP56000.
2. **Non-Uniform Register Widths**: General data registers are 24-bit (`X0/X1/Y0/Y1`), accumulators are 56-bit (`A2:A1:A0`, `B2:B1:B0`), AGU address/offset/modifier registers are 16-bit (`R/N/M`), and instructions are 24-bit (1 word) or 48-bit (2 words, opcode + 24-bit extension word).
3. **Fractional vs. Integer Multiplication**: The hardware `MPY`/`MAC` unit operates in signed **Q1.23 fractional mode** by default, producing `(a * b) << 1` in the 56-bit accumulator. Integer multiplication therefore requires `MPY` followed by `ASR` (`>> 1`) and reading the low 24-bit word `A0` rather than blindly reading `A1`.
4. **Object/Executable Format**: Standard desktop ELF/PE/Mach-O containers cannot natively represent three disjoint 16-bit word-addressed spaces (`P:`, `X:`, `Y:`) for DSP56000 bootloaders/simulators without non-standard conventions. AdeshLang uses `ADOB` internally and emits the canonical Motorola **`.LOD`** ASCII load format (`_START`, `_DATA P/X/Y`, `_SYMBOL`, `_END`).
5. **Simulator Availability Blocker**: No `asm56000`, `a56`, or `sim56000` simulator is installed in the build/CI environment. Therefore `dsp56000-unknown-unknown` MUST remain **Tier 2 Experimental (emission-tested)** and must NOT be advertised as execution-verified until validated against a real DSP56000 simulator or hardware target.

---

## 2. Hardware Architecture Characteristics

### 2.1 Bus, Address, and Instruction Widths
| Property | Specification |
|---|---|
| Data Bus Width | 24 bits (big-endian word order) |
| Minimum Addressable Unit | 24-bit word (1 address increment = 24 bits) |
| Pointer / Address Width | 16 bits (`$0000..$FFFF` = 65,536 words per space) |
| Memory Spaces | `P:` Program (64K x 24-bit), `X:` Data (64K x 24-bit), `Y:` Data (64K x 24-bit) |
| Instruction Width | 24 bits (1 word) or 48 bits (2 words with extension word) |
| Hardware Stack | 15-level x 32-bit `SSH:SSL` (`PC:SR`) on-chip stack (`SP` 6-bit) |
| Software Stack | `X:` memory addressed via `R7` (grows upward via `(R7)+` / `-(R7)`) |
| Hardware Floating Point | None |
| Hardware Integer Divide | None (uses conditional subtract loop `__dsp56k_div24`) |

### 2.2 Register File Arrangement
- **Data ALU Input Registers (24-bit each, 48-bit paired)**: `X0`, `X1` (`X = X1:X0`), `Y0`, `Y1` (`Y = Y1:Y0`).
- **Accumulators (56-bit each)**:
  - `A = A2 (8-bit extension) : A1 (24-bit MSP) : A0 (24-bit LSP)`
  - `B = B2 (8-bit extension) : B1 (24-bit MSP) : B0 (24-bit LSP)`
  - Reading `A` or `B` onto the 24-bit `X`/`Y` bus invokes **saturation limiting** (clamping to `+$7FFFFF` or `-$800000` when `A2` overflows); reading `A1` or `B1` bypasses limiting and reads the raw 24-bit middle word.
- **Address Generation Unit (AGU, 16-bit)**:
  - `R0..R7`: Address pointers (`R6` reserved as frame pointer, `R7` reserved as software stack pointer, `R4..R5` reserved as codegen AGU scratch).
  - `N0..N7`: Signed 16-bit address offsets (`(Rn)+Nn`, `(Rn+Nn)`).
  - `M0..M7`: Address modifiers (`$FFFF` = linear addressing; `0 <= M <= 32767` = modulo circular buffer of length `M + 1`; `$0000` = bit-reversed FFT addressing).
- **Program Control Unit (PCU)**: `PC` (16-bit), `SR` (16-bit `MR:CCR`), `LA` (16-bit hardware loop end address), `LC` (16-bit loop counter), `OMR` (8-bit operating mode register).

### 2.3 Addressing Modes & Pipeline Behavior
- **Addressing Modes**:
  - Register direct (`X0`, `X1`, `Y0`, `Y1`, `A1`, `B1`, `A`, `B`, `R0..R7`)
  - Indirect no-update `(Rn)`
  - Post-increment `(Rn)+`, post-decrement `(Rn)-`, post-offset `(Rn)+Nn`, pre-decrement `-(Rn)`
  - Indexed `(Rn+Nn)` (note: DSP56000 has no `(Rn+imm)` displacement mode in a single instruction; frame offsets load into `Nn` or adjust `R4` via `MOVE #off, N6; MOVE X:(R6+N6), D`!)
  - Modulo circular buffer addressing when `Mn != $FFFF`
  - Absolute short (`<aa` 6-bit / `<aaaa` 12-bit) and absolute long (`>xxxx` 16-bit in extension word)
  - Immediate short (`#xx` 8-bit) and immediate long (`#xxxxxx` 24-bit in extension word)
- **3-Stage Pipeline & Hazards**:
  - Fetch -> Decode -> Execute (1 instruction cycle = 2 clock cycles).
  - **AGU Pipeline Stall Hazard**: Writing an AGU register (`Rn`, `Nn`, or `Mn`) in instruction `t` and immediately using that same `Rn` for indirect memory access in instruction `t+1` causes an extra instruction-cycle stall or stale AGU value on early DSP56000 steppings. The codegen backend inserts a `NOP` or schedules an independent instruction between AGU register load (`MOVE #off, N6`) and `(R6+N6)` memory access.
  - **Hardware Loops (`DO`, `REP`)**: Zero-overhead loops using `LA`/`LC` and `SSH:SSL`, avoiding branch penalties for counted inner loops.

---

## 3. Language Integration & `i24` / `u24` Design Specification

AdeshLang preserves existing `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64` semantics on all architectures. **`i32` and `u32` are NEVER silently truncated to 24 bits.**

### 3.1 Existing Type Mapping on `dsp56000-unknown-unknown`
- **`bool`, `i8`, `u8`, `i16`, `u16`**: Held in a single 24-bit Data ALU register (`X0`, `X1`, `Y0`, `Y1`, `A1`, `B1`) and 1 `X:` memory word (`24` bits). Explicit masking (`AND #$FF` / `AND #$FFFF`) and sign extension preserve exact 8-bit / 16-bit wrap semantics.
- **Pointers (`&T`, `usize`)**: 16-bit word address in `X:` space, occupying 1 word (`16` significant bits in a 24-bit word).
- **`i32`, `u32`**: Legalized via `legalize_32bit_to_16bit_pairs` into two 16-bit half-words stored in 2 consecutive `X:` words (`i32_align = 2`), using carry-propagated 2-word arithmetic and `__dsp56k_{mul,div,mod,shl,shr,sar}32` helpers. Zero silent truncation.
- **`i64`, `u64`, `i128`, `u128`, `f32`, `f64`**: Explicitly refused at lowering time with a target diagnostic naming `dsp56000-unknown-unknown`.

### 3.2 Specified `i24` / `u24` Native DSP Scalar Semantics (Design Specification)
| Property | `i24` | `u24` |
|---|---|---|
| Storage Size | 1 word (24 bits in `P:/X:/Y:`) | 1 word (24 bits in `P:/X:/Y:`) |
| Alignment | 1 word | 1 word |
| Value Range | `-8_388_608 ..= 8_388_607` | `0 ..= 16_777_215` |
| Arithmetic | Native 24-bit `A1`/`B1` ALU ops (`ADD`, `SUB`, `AND`, `OR`, `EOR`) | Native 24-bit `A1`/`B1` ALU ops |
| Multiplication | `MPY S1, S2, A` + `ASR A` -> low 24 bits in `A0` (no Q1.23 shift error) | Same (low 24 bits identical mod $2^{24}$) |
| Overflow Semantics | Two's complement wrap mod $2^{24}$ via `A1` (un-limited) or saturating `A` | Unsigned wrap mod $2^{24}$ via `A1` |
| Conversions | Widens losslessly to `i32`/`i48`; narrowing from `i32` requires explicit mask/truncation | Widens losslessly to `u32`/`u48`; explicit cast required to narrow |
| Memory Representation | 1 big-endian 24-bit word in `X:` (or `Y:`) | 1 big-endian 24-bit word in `X:` (or `Y:`) |
| ABI Passing | Passed directly in `A1, B1, X0, X1, Y0, Y1`; overflow args on `X:` stack (`R7`), 1 word each; return in `A1` | Same |

---

## 4. Calling Convention (`dsp56k-reg-stack`) & Memory Layout

- **Argument Registers (in order)**: `A1` (id 0), `B1` (id 1), `X0` (id 2), `X1` (id 3), `Y0` (id 4), `Y1` (id 5).
- **Return Register**: `A1` (id 0) for 1-word scalars; `(A1, B1)` for 2-word (`i32`/`u32`) values.
- **Stack Pointer**: `R7` (id 15), pointing to `X:` software stack (`$0100` upward, `M7 = $FFFF`).
- **Frame Pointer**: `R6` (id 14), saving caller `R6` on `X:(R7)+` during prologue; locals sit at `X:(R6 + N6)` (`N6` = positive word offset).
- **Hardware Call Return Stack**: `JSR >addr` pushes `PC:SR` onto hardware `SSH:SSL`; `RTS` pops `PC` from `SSH`. Maximum hardware nesting depth without `SSH` spill is 15 calls.
- **Memory Regions**:
  - `P:$0000..$0001`: Reset vector (`JMP >__crt0_start`)
  - `P:$0002..$FFFF`: Program `.text` + synthesized runtime helpers
  - `X:$0040..`: `.rodata` (24-bit word-per-char NUL-terminated strings), `.data`, `.bss`, then software stack (`R7` initialized at `$0100` or above `.bss`)
  - `Y:$0000..`: `.ydata` (optional Y-memory coefficient tables)

---

## 5. Motorola `.LOD` Output Format & Runtime Profile

The linker (`linker/src/dsp56k/mod.rs`) emits Motorola ASCII `.LOD` records:
- `_START <stem> 0000 0000 0000 AdeshLang DSP56000`
- `_DATA P <addr4>` with big-endian 6-hex-digit 24-bit instruction words
- `_DATA X <addr4>` with big-endian 6-hex-digit 24-bit data/rodata words
- `_DATA Y <addr4>` (when Y-space data is present)
- `_SYMBOL P` / `_SYMBOL X` symbol tables
- `_END <entry4>`

Runtime profile is `RuntimeProfile::SpecializedDsp`:
- Supported: static allocation/locals, integer arithmetic (`i8/u8/i16/u16/i24/u24/i32/u32`), hardware `MPY`+`ASR`, software `__dsp56k_div24`/`__dsp56k_mod24` & 32-bit helpers, serial/host output via memory-mapped `X:$FFE0` SCI port.
- Refused at compile/link time: OS filesystem, networking, threads, atomics, TLS, dynamic allocation, exceptions, async, floating point, 64-bit integers.
- Printing: `specialized_dsp()` advertises `printing` through the `X:$FFE0` SCI TX abstraction. In-repo `Dsp56kVm` (`tests/support/dsp56k_vm.rs`) parses the emitted `.LOD` `P:/X:/Y:` words and executes the emitted instruction subset (`dsp56000_codegen_lod_test` 6/6 in default and `--fast` modes, including `X:$FFE0` SCI TX capture and loud invalid-opcode non-vacuity). No external third-party `sim56000` simulator or hardware target runs in CI, so `dsp56000-unknown-unknown` remains **Tier 2 Experimental**, and `__dsp56k_{div,mod,shl,shr,sar}32` in `linker/src/dsp56k/mod.rs` remain stubs (`B1=0`).
