//! Ownership Helpers
//!
//! Types and traits for managing ownership and borrowing at compile time.

use std::marker::PhantomData;

/// Represents exclusive ownership of a value
pub struct Own<T> {
    value: T,
}

impl<T> Own<T> {
    /// Takes ownership of a value
    pub fn new(value: T) -> Self {
        Own { value }
    }

    /// Borrows the owned value
    pub fn borrow(&self) -> Borrow<'_, T> {
        Borrow {
            reference: &self.value,
            _phantom: PhantomData,
        }
    }

    /// Mutably borrows the owned value
    pub fn borrow_mut(&mut self) -> BorrowMut<'_, T> {
        BorrowMut {
            reference: &mut self.value,
            _phantom: PhantomData,
        }
    }

    /// Consumes the Own and returns the inner value
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Gets a reference to the inner value
    pub fn as_ref(&self) -> &T {
        &self.value
    }

    /// Gets a mutable reference to the inner value
    pub fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Represents a shared borrow of a value
pub struct Borrow<'a, T> {
    reference: &'a T,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> Borrow<'a, T> {
    /// Creates a new borrow from a reference
    pub fn new(reference: &'a T) -> Self {
        Borrow {
            reference,
            _phantom: PhantomData,
        }
    }

    /// Gets the borrowed reference
    pub fn get(&self) -> &T {
        self.reference
    }
}

impl<'a, T> std::ops::Deref for Borrow<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.reference
    }
}

/// Represents an exclusive mutable borrow of a value
pub struct BorrowMut<'a, T> {
    reference: &'a mut T,
    _phantom: PhantomData<&'a mut T>,
}

impl<'a, T> BorrowMut<'a, T> {
    /// Creates a new mutable borrow from a mutable reference
    pub fn new(reference: &'a mut T) -> Self {
        BorrowMut {
            reference,
            _phantom: PhantomData,
        }
    }

    /// Gets the borrowed mutable reference
    pub fn get_mut(&mut self) -> &mut T {
        self.reference
    }
}

impl<'a, T> std::ops::Deref for BorrowMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.reference
    }
}

impl<'a, T> std::ops::DerefMut for BorrowMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.reference
    }
}

/// Marker trait for types that can be moved
pub trait Move: Sized {
    /// Moves the value, consuming it
    fn move_value(self) -> Self {
        self
    }
}

// Implement Move for all sized types
impl<T: Sized> Move for T {}
