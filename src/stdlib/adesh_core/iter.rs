//! Iterator Trait and Utilities
//!
//! Core iterator abstraction with no allocations.

/// Iterator trait for types that produce a sequence of values
pub trait Iterator {
    /// The type of items being iterated over
    type Item;

    /// Advances the iterator and returns the next value
    fn next(&mut self) -> Option<Self::Item>;

    /// Returns the size hint (lower bound, optional upper bound)
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Consumes the iterator, counting the number of iterations
    fn count(self) -> usize
    where
        Self: Sized,
    {
        let mut count = 0;
        let mut iter = self;
        while iter.next().is_some() {
            count += 1;
        }
        count
    }

    /// Advances the iterator by n elements
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        for _ in 0..n {
            self.next()?;
        }
        self.next()
    }

    /// Applies a function to each element
    fn for_each<F>(self, mut f: F)
    where
        Self: Sized,
        F: FnMut(Self::Item),
    {
        let mut iter = self;
        while let Some(item) = iter.next() {
            f(item);
        }
    }

    /// Tests if every element matches a predicate
    fn all<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(Self::Item) -> bool,
    {
        while let Some(item) = self.next() {
            if !f(item) {
                return false;
            }
        }
        true
    }

    /// Tests if any element matches a predicate
    fn any<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(Self::Item) -> bool,
    {
        while let Some(item) = self.next() {
            if f(item) {
                return true;
            }
        }
        false
    }

    /// Finds the first element matching a predicate
    fn find<F>(&mut self, mut f: F) -> Option<Self::Item>
    where
        F: FnMut(&Self::Item) -> bool,
    {
        while let Some(item) = self.next() {
            if f(&item) {
                return Some(item);
            }
        }
        None
    }

    /// Map adapter
    fn map<B, F>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: FnMut(Self::Item) -> B,
    {
        Map { iter: self, f }
    }

    /// Filter adapter
    fn filter<P>(self, predicate: P) -> Filter<Self, P>
    where
        Self: Sized,
        P: FnMut(&Self::Item) -> bool,
    {
        Filter {
            iter: self,
            predicate,
        }
    }

    /// Take adapter
    fn take(self, n: usize) -> Take<Self>
    where
        Self: Sized,
    {
        Take { iter: self, n }
    }

    /// Skip adapter
    fn skip(self, n: usize) -> Skip<Self>
    where
        Self: Sized,
    {
        Skip { iter: self, n }
    }

    /// Enumerate adapter
    fn enumerate(self) -> Enumerate<Self>
    where
        Self: Sized,
    {
        Enumerate {
            iter: self,
            count: 0,
        }
    }
}

/// Map iterator
pub struct Map<I, F> {
    iter: I,
    f: F,
}

impl<B, I: Iterator, F: FnMut(I::Item) -> B> Iterator for Map<I, F> {
    type Item = B;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(&mut self.f)
    }
}

/// Filter iterator
pub struct Filter<I, P> {
    iter: I,
    predicate: P,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for Filter<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(x) = self.iter.next() {
            if (self.predicate)(&x) {
                return Some(x);
            }
        }
        None
    }
}

/// Take iterator
pub struct Take<I> {
    iter: I,
    n: usize,
}

impl<I: Iterator> Iterator for Take<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            self.iter.next()
        }
    }
}

/// Skip iterator
pub struct Skip<I> {
    iter: I,
    n: usize,
}

impl<I: Iterator> Iterator for Skip<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        while self.n > 0 {
            self.n -= 1;
            self.iter.next()?;
        }
        self.iter.next()
    }
}

/// Enumerate iterator
pub struct Enumerate<I> {
    iter: I,
    count: usize,
}

impl<I: Iterator> Iterator for Enumerate<I> {
    type Item = (usize, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let a = self.iter.next()?;
        let i = self.count;
        self.count += 1;
        Some((i, a))
    }
}
