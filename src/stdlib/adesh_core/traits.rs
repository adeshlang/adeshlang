//! Primitive Traits for Adesh Core
//!
//! Fundamental traits that define behavior for primitive types.
//! These are compile-time contracts with no runtime overhead.

/// Trait for types that can be copied bitwise
pub trait Copy {}

/// Trait for types that can be cloned
pub trait Clone {
    fn clone(&self) -> Self;
}

/// Trait for types that can be compared for equality
pub trait Eq {
    fn eq(&self, other: &Self) -> bool;
    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

/// Trait for types that can be ordered
pub trait Ord: Eq {
    fn lt(&self, other: &Self) -> bool;
    fn le(&self, other: &Self) -> bool {
        self.lt(other) || self.eq(other)
    }
    fn gt(&self, other: &Self) -> bool {
        !self.le(other)
    }
    fn ge(&self, other: &Self) -> bool {
        !self.lt(other)
    }
}

/// Trait for types that have a default value
pub trait Default {
    fn default() -> Self;
}

/// Trait for types that can be converted from another type
pub trait From<T> {
    fn from(value: T) -> Self;
}

/// Trait for types that can be converted into another type
pub trait Into<T> {
    fn into(self) -> T;
}

/// Trait for displaying types
pub trait Display {
    fn fmt(&self) -> String;
}

/// Trait for debugging types
pub trait Debug {
    fn debug_fmt(&self) -> String;
}

/// Trait for sized types (known size at compile time)
pub trait Sized {}

/// Trait for types that can be sent across threads
pub trait Send {}

/// Trait for types that can be shared across threads
pub trait Sync {}

/// Trait for types that have drop semantics
pub trait Drop {
    fn drop(&mut self);
}

/// Trait for numeric types
pub trait Numeric: Copy + Eq + Ord {
    fn add(self, other: Self) -> Self;
    fn sub(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
    fn div(self, other: Self) -> Self;
}

/// Trait for integer types
pub trait Integer: Numeric {
    fn rem(self, other: Self) -> Self;
    fn bit_and(self, other: Self) -> Self;
    fn bit_or(self, other: Self) -> Self;
    fn bit_xor(self, other: Self) -> Self;
}

/// Trait for floating point types
pub trait Float: Numeric {
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn sqrt(self) -> Self;
    fn abs(self) -> Self;
}
