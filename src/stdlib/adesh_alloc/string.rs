//! String - Growable UTF-8 Encoded String
//!
//! A heap-allocated UTF-8 encoded string.

use crate::stdlib::adesh_alloc::vec::Vec;

/// A growable, heap-allocated UTF-8 string
pub struct String {
    vec: Vec<u8>,
}

impl String {
    /// Creates a new empty String
    pub fn new() -> Self {
        String { vec: Vec::new() }
    }

    /// Creates a String with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        String {
            vec: Vec::with_capacity(capacity),
        }
    }

    /// Creates a String from a byte vector (assumes valid UTF-8)
    ///
    /// # Safety
    /// The caller must ensure the bytes are valid UTF-8
    pub unsafe fn from_utf8_unchecked(bytes: Vec<u8>) -> Self {
        String { vec: bytes }
    }

    /// Returns the length of the string in bytes
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns true if the string is empty
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Returns the capacity of the string
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    /// Appends a character to the string
    pub fn push(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        for &byte in s.as_bytes() {
            self.vec.push(byte);
        }
    }

    /// Appends a string slice to the string
    pub fn push_str(&mut self, s: &str) {
        for &byte in s.as_bytes() {
            self.vec.push(byte);
        }
    }

    /// Removes and returns the last character
    pub fn pop(&mut self) -> Option<char> {
        if self.is_empty() {
            return None;
        }

        let s = self.as_str();
        let ch = s.chars().last()?;
        let new_len = self.len() - ch.len_utf8();

        // Clear the bytes we're removing
        for i in new_len..self.vec.len() {
            if let Some(byte) = self.vec.get_mut(i) {
                *byte = 0;
            }
        }

        while self.vec.len() > new_len {
            self.vec.pop();
        }

        Some(ch)
    }

    /// Returns the string as a string slice
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(self.vec.as_slice()) }
    }

    /// Returns a mutable string slice
    pub fn as_mut_str(&mut self) -> &mut str {
        unsafe { std::str::from_utf8_unchecked_mut(self.vec.as_mut_slice()) }
    }

    /// Clears the string, removing all characters
    pub fn clear(&mut self) {
        self.vec.clear();
    }
}

impl From<&str> for String {
    fn from(s: &str) -> Self {
        let mut string = String::with_capacity(s.len());
        string.push_str(s);
        string
    }
}

impl std::ops::Deref for String {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::DerefMut for String {
    fn deref_mut(&mut self) -> &mut str {
        self.as_mut_str()
    }
}

impl Default for String {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_str(), f)
    }
}

impl std::fmt::Debug for String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}
