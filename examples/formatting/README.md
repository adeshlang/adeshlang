# String Formatting in AdeshLang

AdeshLang supports powerful string formatting capabilities through template strings (backtick strings) with format specifiers.

## Template String Syntax

Template strings use backticks (`` ` ``) and allow embedded expressions with `${expression}` syntax:

```adesh
let name = "World";
print(`Hello, ${name}!`);  // Output: Hello, World!

let x = 10;
let y = 20;
print(`Sum: ${x + y}`);    // Output: Sum: 30
```

## Format Specifiers

Format specifiers follow a colon after the expression: `${expression:format_spec}`

### Alignment and Width

| Specifier | Description | Example | Output |
|-----------|-------------|---------|--------|
| `<N` | Left align with width N | `${msg:<20}` | `"left                "` |
| `^N` | Center align with width N | `${msg:^20}` | `"       center       "` |
| `>N` | Right align with width N | `${msg:>20}` | `"               right"` |
| `0N` | Zero-pad to width N | `${num:08}` | `"00001234"` |

```adesh
let msg = "left";
print(`[${msg:<20}]`);   // [left                ]

msg = "center";
print(`[${msg:^20}]`);   // [       center       ]

msg = "right";
print(`[${msg:>20}]`);   // [               right]

let num = 1234;
print(`[${num:08}]`);    // [00001234]
```

### Number Sign

| Specifier | Description | Example | Output |
|-----------|-------------|---------|--------|
| `+` | Show sign for positive numbers | `${n:+}` | `"+10"` |

```adesh
let n = 10;
print(`${n:+}`);  // +10
```

### Type Conversion

| Specifier | Description | Example | Output |
|-----------|-------------|---------|--------|
| `int` | Convert to integer | `${val:int}` | `"123"` (from 123.9) |
| `float(N)` | Format as float with N decimals | `${pi:float(2)}` | `"3.14"` |
| `bin` | Format as binary | `${n:bin}` | `"1010"` (from 10) |
| `hex` | Format as lowercase hexadecimal | `${m:hex}` | `"ff"` (from 255) |
| `HEX` | Format as uppercase hexadecimal | `${m:HEX}` | `"FF"` (from 255) |
| `oct` | Format as octal | `${n:oct}` | `"12"` (from 10) |

```adesh
let val = 123.9;
print(`Value: ${val:int}`);       // Value: 123

let pi = 3.14159;
print(`Pi: ${pi:float(2)}`);      // Pi: 3.14

let n = 10;
print(`Binary: ${n:bin}`);        // Binary: 1010

let m = 255;
print(`Hex: ${m:hex}`);           // Hex: ff
```

### Currency Formatting

| Specifier | Description | Example | Output |
|-----------|-------------|---------|--------|
| `currency(CODE)` | Format as currency | `${price:currency(USD)}` | `"$1,234.50"` |

Supported currency codes:
- `USD` - US Dollar ($)
- `EUR` - Euro (€)
- `GBP` - British Pound (£)
- `JPY` - Japanese Yen (¥) - no decimals
- `INR` - Indian Rupee (₹)
- `CNY` - Chinese Yuan (¥)
- `KRW` - Korean Won (₩) - no decimals
- `RUB` - Russian Ruble (₽)
- `BRL` - Brazilian Real (R$)
- `CAD` - Canadian Dollar (C$)
- `AUD` - Australian Dollar (A$)
- `CHF` - Swiss Franc (CHF)
- `MXN` - Mexican Peso (MX$)

```adesh
let price = 1234.5;
print(`USD: ${price:currency(USD)}`);   // USD: $1,234.50
print(`EUR: ${price:currency(EUR)}`);   // EUR: €1,234.50
print(`INR: ${price:currency(INR)}`);   // INR: ₹1,234.50
print(`JPY: ${price:currency(JPY)}`);   // JPY: ¥1,235
```

### Combining Format Specifiers

You can combine alignment/width with type specifiers:

```adesh
let amt = 1234.5;
print(`[${amt:>12+currency(INR)}]`);    // [    +₹1,234.50]
print(`[${amt:^16currency(EUR)}]`);     // [   €1,234.50   ]
```

### Fill Character

You can specify a custom fill character before the alignment specifier:

```adesh
let n = 42;
print(`[${n:0>8}]`);   // [00000042] - zero-padded right align
print(`[${n:_^10}]`);  // [____42____] - underscore-padded center
```

## Expressions in Templates

Template strings support any valid expression:

```adesh
let a = 12;
let b = 3;
print(`Result: ${(a + b) * 2}`);           // Result: 30
print(`Formatted: ${((a * 10) + b):int}`); // Formatted: 123
```

## Examples

Run the example files in this folder:

```bash
adesh run examples/formatting/basic.adesh
adesh run examples/formatting/align_width.adesh
adesh run examples/formatting/types.adesh
adesh run examples/formatting/expressions.adesh
adesh run examples/formatting/currency_variants.adesh
```

## Notes

- Template strings must use backticks (`` ` ``), not regular quotes
- Regular strings (`"..."` or `'...'`) do not support interpolation
- Format specifiers are optional - `${expr}` works without a colon
- All formatting features work with both the interpreter and JIT backends
