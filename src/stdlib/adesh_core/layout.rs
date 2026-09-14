//! Memory Layout Traits
//!
//! Defines memory layout characteristics for types.

/// Represents the memory layout of a type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    size: usize,
    align: usize,
}

impl Layout {
    /// Creates a new layout from size and alignment
    pub const fn from_size_align(size: usize, align: usize) -> Option<Layout> {
        if !align.is_power_of_two() {
            return None;
        }
        if size > isize::MAX as usize - (align - 1) {
            return None;
        }
        Some(Layout { size, align })
    }

    /// Creates a layout for a type
    pub const fn new<T>() -> Layout {
        Layout {
            size: std::mem::size_of::<T>(),
            align: std::mem::align_of::<T>(),
        }
    }

    /// Returns the size of the layout
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns the alignment of the layout
    pub const fn align(&self) -> usize {
        self.align
    }

    /// Rounds up size to the next multiple of alignment
    pub const fn pad_to_align(&self) -> Layout {
        let new_size = (self.size + self.align - 1) & !(self.align - 1);
        Layout {
            size: new_size,
            align: self.align,
        }
    }
}

/// Alignment requirement
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align(usize);

impl Align {
    /// Creates a new alignment
    pub const fn of<T>() -> Align {
        Align(std::mem::align_of::<T>())
    }

    /// Returns the alignment value
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// Aligns a size up to this alignment
    pub const fn align_up(&self, size: usize) -> usize {
        (size + self.0 - 1) & !(self.0 - 1)
    }
}
