//! Width-preserving bitwise semantics shared by evaluation and constant folding.
//!
//! Fixed-width shifts require `0 <= count < width`. Left shift discards high
//! bits; signed right shift sign-extends. BigInt uses infinite two's complement.
use crate::parsing::ast::{TokenKind, Value};
use num_traits::ToPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

impl BitOp {
    pub fn from_token(token: TokenKind) -> Option<Self> {
        Some(match token {
            TokenKind::Ampersand | TokenKind::AmpersandEqual => Self::And,
            TokenKind::Pipe | TokenKind::PipeEqual => Self::Or,
            TokenKind::Caret | TokenKind::CaretEqual => Self::Xor,
            TokenKind::ShiftLeft | TokenKind::ShiftLeftEqual => Self::Shl,
            TokenKind::ShiftRight | TokenKind::ShiftRightEqual => Self::Shr,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Integer {
    bits: u128,
    width: u32,
    signed: bool,
    dynamic: bool,
}

pub fn mask(width: u32) -> Result<u128, String> {
    match width {
        0 => Ok(0),
        1..=127 => Ok((1u128 << width) - 1),
        128 => Ok(u128::MAX),
        _ => Err("bit width exceeds 128".into()),
    }
}

impl Integer {
    fn read(value: &Value) -> Result<Self, String> {
        let (bits, width, signed, dynamic) = match value {
            Value::U8(n) => (*n as u128, 8, false, false),
            Value::U16(n) => (*n as u128, 16, false, false),
            Value::U32(n) => (*n as u128, 32, false, false),
            Value::U64(n) => (*n as u128, 64, false, false),
            Value::U128(n) => (*n, 128, false, false),
            Value::I8(n) => (*n as u128, 8, true, false),
            Value::I16(n) => (*n as u128, 16, true, false),
            Value::I32(n) => (*n as u128, 32, true, false),
            Value::I64(n) => (*n as u128, 64, true, false),
            Value::I128(n) => (*n as u128, 128, true, false),
            Value::Number(n)
                if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 =>
            {
                (*n as i64 as u128, 64, true, true)
            }
            _ => return Err("bitwise operation requires an integer operand (use a typed integer for values beyond the exact number range)".into()),
        };
        Ok(Self {
            bits: bits & mask(width)?,
            width,
            signed,
            dynamic,
        })
    }

    fn signed_value(self) -> i128 {
        ((self.bits << (128 - self.width)) as i128) >> (128 - self.width)
    }

    fn value(self, bits: u128) -> Value {
        if self.dynamic {
            let n = bits as i64;
            return if (n as f64) as i128 == n as i128 {
                Value::Number(n as f64)
            } else {
                Value::I64(n)
            };
        }
        match (self.width, self.signed) {
            (8, false) => Value::U8(bits as u8),
            (16, false) => Value::U16(bits as u16),
            (32, false) => Value::U32(bits as u32),
            (64, false) => Value::U64(bits as u64),
            (128, false) => Value::U128(bits),
            (8, true) => Value::I8(bits as i8),
            (16, true) => Value::I16(bits as i16),
            (32, true) => Value::I32(bits as i32),
            (64, true) => Value::I64(bits as i64),
            (128, true) => Value::I128(bits as i128),
            _ => unreachable!("validated integer width"),
        }
    }

    fn compatible(self, other: Self) -> Result<(Self, Self), String> {
        if self.dynamic && !other.dynamic {
            let (b, a) = other.compatible(self)?;
            return Ok((a, b));
        }
        if !self.dynamic && other.dynamic {
            let n = other.signed_value();
            let fits = if self.signed {
                self.width == 128
                    || (n >= -(1i128 << (self.width - 1)) && n < (1i128 << (self.width - 1)))
            } else {
                n >= 0 && (n as u128) <= mask(self.width)?
            };
            if fits {
                return Ok((
                    self,
                    Self {
                        bits: (n as u128) & mask(self.width)?,
                        ..self
                    },
                ));
            }
        } else if self.width == other.width && self.signed == other.signed {
            return Ok((self, other));
        }
        Err("bitwise operands must have compatible integer types".into())
    }
}

/// Validate counts without truncating a wide integer or accepting a float type.
pub fn count(value: &Value) -> Result<u128, String> {
    if let Value::BigInt(n) = value {
        return n
            .to_u128()
            .ok_or_else(|| "invalid shift amount or bit range".into());
    }
    let n = Integer::read(value)?;
    if n.signed && n.signed_value() < 0 {
        return Err("invalid shift amount or bit range: negative value".into());
    }
    Ok(n.bits)
}

pub fn checked_shift(count: i64, width: u32) -> Result<u32, String> {
    if count < 0 || count as u64 >= u64::from(width) {
        Err(format!(
            "invalid shift amount: expected 0 <= count < {width}"
        ))
    } else {
        Ok(count as u32)
    }
}

pub fn binary(op: BitOp, lhs: &Value, rhs: &Value) -> Result<Value, String> {
    if let Value::BigInt(a) = lhs {
        if matches!(op, BitOp::Shl | BitOp::Shr) {
            let n = usize::try_from(count(rhs)?).map_err(|_| "invalid shift amount")?;
            return Ok(Value::BigInt(if op == BitOp::Shl {
                a << n
            } else {
                a >> n
            }));
        }
        let b = match rhs {
            Value::BigInt(b) => b.clone(),
            Value::Number(_) => num_bigint::BigInt::from(Integer::read(rhs)?.signed_value()),
            _ => return Err("bitwise operands must have compatible integer types".into()),
        };
        return Ok(Value::BigInt(match op {
            BitOp::And => a & b,
            BitOp::Or => a | b,
            BitOp::Xor => a ^ b,
            _ => unreachable!(),
        }));
    }
    if matches!(rhs, Value::BigInt(_)) && matches!(op, BitOp::And | BitOp::Or | BitOp::Xor) {
        return binary(op, rhs, lhs);
    }
    let a = Integer::read(lhs)?;
    if matches!(op, BitOp::Shl | BitOp::Shr) {
        let n = count(rhs)?;
        if n >= u128::from(a.width) {
            return Err(format!(
                "invalid shift amount: expected 0 <= count < {}",
                a.width
            ));
        }
        return Ok(a.value(match op {
            BitOp::Shl => a.bits << n as u32,
            BitOp::Shr if a.signed => (a.signed_value() >> n as u32) as u128,
            BitOp::Shr => a.bits >> n as u32,
            _ => unreachable!(),
        }));
    }
    let (a, b) = a.compatible(Integer::read(rhs)?)?;
    // The typed operand determines the result type for unsuffixed numbers.
    let result_type = if a.dynamic && !b.dynamic { b } else { a };
    Ok(result_type.value(match op {
        BitOp::And => a.bits & b.bits,
        BitOp::Or => a.bits | b.bits,
        BitOp::Xor => a.bits ^ b.bits,
        _ => unreachable!(),
    }))
}

pub fn complement(value: &Value) -> Result<Value, String> {
    if let Value::BigInt(n) = value {
        return Ok(Value::BigInt(!n));
    }
    let n = Integer::read(value)?;
    Ok(n.value(!n.bits))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitIntrinsic {
    Count,
    LeadingZeros,
    TrailingZeros,
    LeadingOnes,
    TrailingOnes,
    Width,
    Reverse,
    SwapBytes,
    RotateLeft,
    RotateRight,
    Test,
    Set,
    Clear,
    Toggle,
    Extract,
    Insert,
    Mask,
    MaskAt,
}

impl BitIntrinsic {
    pub const ALL: &'static [(&'static str, Self)] = &[
        ("bit_count", Self::Count),
        ("leading_zeros", Self::LeadingZeros),
        ("trailing_zeros", Self::TrailingZeros),
        ("leading_ones", Self::LeadingOnes),
        ("trailing_ones", Self::TrailingOnes),
        ("bit_width", Self::Width),
        ("reverse_bits", Self::Reverse),
        ("byte_swap", Self::SwapBytes),
        ("rotate_left", Self::RotateLeft),
        ("rotate_right", Self::RotateRight),
        ("bit_test", Self::Test),
        ("bit_set", Self::Set),
        ("bit_clear", Self::Clear),
        ("bit_toggle", Self::Toggle),
        ("bit_extract", Self::Extract),
        ("bit_insert", Self::Insert),
        ("bit_mask", Self::Mask),
        ("bit_mask_at", Self::MaskAt),
    ];

    pub fn arity(self) -> usize {
        match self {
            Self::Insert => 4,
            Self::Extract => 3,
            Self::RotateLeft
            | Self::RotateRight
            | Self::Test
            | Self::Set
            | Self::Clear
            | Self::Toggle
            | Self::MaskAt => 2,
            _ => 1,
        }
    }

    pub fn eval(self, args: &[Value]) -> Result<Value, String> {
        if args.len() != self.arity() {
            return Err(format!("bit intrinsic requires {} arguments", self.arity()));
        }
        if matches!(self, Self::Mask | Self::MaskAt) {
            let (offset, width) = if self == Self::Mask {
                (0, count(&args[0])?)
            } else {
                (count(&args[0])?, count(&args[1])?)
            };
            check_range(offset, width, 64)?;
            return Ok(Value::U64(if width == 0 {
                0
            } else {
                (mask(width as u32)? << offset as u32) as u64
            }));
        }
        let n = Integer::read(&args[0])?;
        let inverted = !n.bits & mask(n.width)?;
        let zeros = |bits: u128| bits.leading_zeros() - (128 - n.width);
        let trailing = |bits: u128| bits.trailing_zeros().min(n.width);
        let result = match self {
            Self::Count => return Ok(Value::U32(n.bits.count_ones())),
            Self::LeadingZeros => return Ok(Value::U32(zeros(n.bits))),
            Self::TrailingZeros => return Ok(Value::U32(trailing(n.bits))),
            Self::LeadingOnes => return Ok(Value::U32(zeros(inverted))),
            Self::TrailingOnes => return Ok(Value::U32(trailing(inverted))),
            Self::Width => return Ok(Value::U32(n.width - zeros(n.bits))),
            Self::Reverse => n.bits.reverse_bits() >> (128 - n.width),
            Self::SwapBytes => n.bits.swap_bytes() >> (128 - n.width),
            Self::RotateLeft | Self::RotateRight => {
                let shift = (count(&args[1])? % u128::from(n.width)) as u32;
                if shift == 0 {
                    n.bits
                } else if self == Self::RotateLeft {
                    (n.bits << shift) | (n.bits >> (n.width - shift))
                } else {
                    (n.bits >> shift) | (n.bits << (n.width - shift))
                }
            }
            Self::Test | Self::Set | Self::Clear | Self::Toggle => {
                let index = count(&args[1])?;
                check_range(index, 1, n.width)?;
                let bit = 1u128 << index as u32;
                match self {
                    Self::Test => return Ok(Value::Bool(n.bits & bit != 0)),
                    Self::Set => n.bits | bit,
                    Self::Clear => n.bits & !bit,
                    _ => n.bits ^ bit,
                }
            }
            Self::Extract | Self::Insert => {
                let offset = count(&args[1])?;
                let width = count(&args[2])?;
                check_range(offset, width, n.width)?;
                let field_mask = mask(width as u32)?;
                if self == Self::Extract {
                    if width == 0 {
                        0
                    } else {
                        (n.bits >> offset as u32) & field_mask
                    }
                } else {
                    let (_, replacement) = n.compatible(Integer::read(&args[3])?)?;
                    if width == 0 {
                        n.bits
                    } else {
                        (n.bits & !(field_mask << offset as u32))
                            | ((replacement.bits & field_mask) << offset as u32)
                    }
                }
            }
            Self::Mask | Self::MaskAt => unreachable!(),
        };
        Ok(n.value(result & mask(n.width)?))
    }
}

fn check_range(offset: u128, width: u128, bits: u32) -> Result<(), String> {
    if offset > u128::from(bits) || width > u128::from(bits) - offset {
        Err(format!(
            "invalid bit range: offset + width must not exceed {bits}"
        ))
    } else {
        Ok(())
    }
}
