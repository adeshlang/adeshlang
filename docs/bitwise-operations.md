# Bitwise Operations

AdeshLang treats bitwise operations as a first-class, width-preserving language
subsystem. One semantic definition — implemented in the shared runtime ABI
(`src/runtime/abi/bitwise.rs`) — is consumed by the type checker, the constant
folder, the interpreter, the bytecode VM, the JIT executor tiers, and the
native AOT/JIT code generators. The same source program behaves identically on
every backend that supports an operation.

## Operators

| Operation    | Expression | Compound   |
|--------------|------------|------------|
| AND          | `a & b`    | `a &= b`   |
| OR           | `a \| b`   | `a \|= b`  |
| XOR          | `a ^ b`    | `a ^= b`   |
| NOT          | `~a`       | —          |
| Shift left   | `a << n`   | `a <<= n`  |
| Shift right  | `a >> n`   | `a >>= n`  |

Compound assignments evaluate the target place (variable, field, or index
expression) exactly once, so `arr[compute_index()] &= mask;` calls
`compute_index()` a single time.

## Types and width preservation

Bitwise operators support `u8 u16 u32 u64 u128 i8 i16 i32 i64 i128`, arbitrary
`BigInt` values, and unsuffixed integer literals. The operand width is
preserved: `u8 & u8` yields `u8`, `u128 << 3` yields `u128`. Values are never
silently widened to 64 bits or truncated at the language level.

Operands must have compatible integer types. Mixing two different fixed widths
(for example `u32 & u64`) is a type error. Floating-point operands are always
rejected — `1.5 & 2.0` does not compile.

An unsuffixed integer literal (type `number`) is accepted next to a fixed-width
operand when its value fits the fixed width; the fixed-width operand decides
the result type. Unsuffixed operands among themselves behave as signed 64-bit
integers.

## Signedness

* `&`, `|`, `^` are sign-agnostic two's-complement bit operations.
* `>>` is **arithmetic** (sign-extending) for signed types and for unsuffixed
  literals, and **logical** (zero-filling) for unsigned types.
* `<<` discards bits shifted past the width of the result type.
* `~x` complements the bits of the operand's width: `~0u8` is `255u8`,
  `~0i8` is `-1i8`.

## Shift policy

For a fixed-width type of `N` bits, the shift count must satisfy
`0 <= count < N`. A count that is negative, equal to the width, or larger is an
error — not a host-dependent result and not a silently masked shift:

```adesh
let x: u8 = 1;
let y = x << 8;   // error: invalid shift amount: expected 0 <= count < 8
```

Evaluation layers (interpreter, VM, JIT executor tiers) report a runtime error
with that message. Native code generators (AOT / Native JIT) enforce the same
bound with a single predicted-compare-and-trap sequence in generated machine
code, so an out-of-range shift deterministically traps instead of producing
undefined native shift behavior. `BigInt` shifts are unbounded upward (limited
only by memory); negative counts are rejected.

## BigInt

`BigInt` uses infinite two's-complement semantics:

* `~x == -(x + 1)` for any `BigInt`, exactly as for fixed-width types read in
  two's complement.
* `&`, `|`, `^` combine arbitrary-precision values without narrowing.
* `<<` and `>>` extend or consume as many bits as requested; `>>` is
  arithmetic, so `(-1n) >> 1000` is still `-1n`.

## Constant folding

The AST optimizer folds constant bitwise expressions with exactly the same
semantics as runtime evaluation, including `~`:

```adesh
const MASK: u32 = 0xff & 0x0f;      // 15
const FLAGS: u32 = 1 << 5;          // 32
const INV: u32 = ~0u32;             // 4294967295
const MIXED: u8 = 0b1010 ^ 0b1100; // 6
```

Binary literals (`0b1010`, `0b1111_0000`) combine with the numeric separator
and width suffixes (`0b1010u8`, `0xF0u32`) like any other integer literal.

## Bit-manipulation intrinsics

All intrinsics live under the `std` namespace and are called as methods on it:
`std.bit_count(x)`, `std.rotate_left(x, n)`, and so on. The interpreter injects
`std` (also `Std` and `STD`) into every program's scope, so no import is needed.
The bare names are intentionally **not** installed as global functions, so they
cannot shadow or collide with user-defined functions. They operate on the
operand's width, reject out-of-range indices/ranges with clear errors, and
return new values (they never mutate in place):

| Intrinsic | Result |
|---|---|
| `bit_count(x)` | number of set bits |
| `leading_zeros(x)` / `leading_ones(x)` | leading zero/one bits within the width |
| `trailing_zeros(x)` / `trailing_ones(x)` | trailing zero/one bits |
| `bit_width(x)` | index of the highest set bit plus one |
| `reverse_bits(x)` | bit-reversal across the width |
| `byte_swap(x)` | byte-order swap (16/32/64/128-bit widths) |
| `rotate_left(x, n)` / `rotate_right(x, n)` | rotation; `n` may be any non-negative count |
| `bit_test(x, i)` | `true` when bit `i` is set |
| `bit_set(x, i)` / `bit_clear(x, i)` / `bit_toggle(x, i)` | value with bit `i` set/cleared/flipped |
| `bit_extract(x, offset, width)` | low `width` bits of `x >> offset` |
| `bit_insert(x, offset, width, value)` | value with that field replaced |
| `bit_mask(width)` / `bit_mask_at(offset, width)` | `u64` mask of `width` low bits, optionally shifted |

```adesh
let flags: u32 = 0;
let flags = std.bit_set(flags, 3);
let enabled = std.bit_test(flags, 3);      // true
let mode = std.bit_extract(flags, 0, 4);   // 8
let packed = std.bit_insert(0u32, 4, 8, 200);
```

## Backend support

| Capability | Interpreter | Bytecode VM | JIT tiers | AOT / Native JIT |
|---|---|---|---|---|
| `&` `\|` `^` `~` | shared ABI | shared ABI | shared ABI | native `and/or/xor` instructions |
| `<<` `>>` | checked, error | checked, error | checked, error | native shift + width-mask, traps out-of-range |
| Intrinsics | shared ABI | via builtin call | via builtin call | builtin call (vectorizable) |

Native notes:

* The AOT / Native-JIT generators emit one machine instruction per operator
  (`band`, `bor`, `bxor`, `ishl`, `sshr` for signed lanes, `ushr` for unsigned
  lanes) with an optional lane mask — no helper calls, no boxing.
* Out-of-range shifts compile to a compare-and-trap; correct programs pay one
  well-predicted branch.
* GPU lowering (MLIR) maps to `arith.andi/ori/xori/shli` and the
  count-zeros intrinsics; GPU `>>` is currently always the arithmetic form.
* The SIMD IR already carries `BitAnd/BitOr/BitXor/BitNot/Shl/Shr` for vector
  lanes; atomics already provide `fetch_and/fetch_or/fetch_xor`.

Known limitation: the native code generators currently materialize 128-bit
values through the legacy 64-bit lane, so `u128/i128` bitwise results are exact
on the interpreter/ABI layer and on constant folding, but the native backends
do not yet support full 128-bit bitwise lanes. This is a documented gap, not a
silent truncation of the language semantics.

## Examples

```adesh
const READ: u8 = 1u8 << 0;
const WRITE: u8 = 1u8 << 1;
const EXECUTE: u8 = 1u8 << 2;

fn has_permission(flags: u8, permission: u8): bool {
    return (flags & permission) != 0;
}

let flags: u8 = READ | WRITE;
flags |= EXECUTE;
flags &= ~WRITE;

print("can read:   {}", has_permission(flags, READ));
print("can write:  {}", has_permission(flags, WRITE));
print("set bits:   {}", bit_count(flags));
```

Runnable examples live in `examples/operators/` and are exercised in debug
mode by `tests/examples_integration.rs`:

* `examples/operators/bitwise_operations.adesh` — permission flags: `&`, `|`,
  `&=`, `~`, plus `std.bit_count`, `std.rotate_left`, `std.bit_extract`.
* `examples/operators/bitwise_rgba_packing.adesh` — packing four 8-bit color
  channels into one integer with shifts, masks, `|=`, `~`, and unpacking with
  `std.bit_extract`, `std.bit_test`.
* `examples/operators/bitwise_hashes.adesh` — XOR folding, `std.rotate_left`,
  `std.bit_count`, `std.leading_zeros`, `std.bit_mask`, `std.bit_test`.
* `examples/operators/bitwise_toggle_and_test.adesh` — toggling feature flags
  with `^=` and querying them with `std.bit_test`.
* `examples/operators/bitwise_ipv4_packing.adesh` — encoding 192.168.1.10 as
  one integer with shifts and masks, plus `std.byte_swap` round trips.
* `examples/operators/bitwise_rotations_endianness.adesh` — `std.rotate_left`,
  `std.rotate_right`, `std.reverse_bits`, `std.byte_swap`, `std.bit_mask`, and
  `std.bit_mask_at`.
* `examples/operators/bitwise_twos_complement.adesh` — two's-complement behavior
  of plain numbers (`~0` is `-1`, arithmetic `>>`), plus `std.bit_count`,
  `std.bit_mask`, `std.bit_extract`.
