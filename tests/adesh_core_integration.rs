//! Integration tests for adesh_core layer
//!
//! Tests the core primitives, layouts, slices, and ownership helpers

use adeshlang::stdlib::adesh_core::*;

#[test]
fn test_layout_basic() {
    let layout = Layout::new::<u64>();
    assert_eq!(layout.size(), 8);
    assert_eq!(layout.align(), 8);
}

#[test]
fn test_layout_from_size_align() {
    let layout = Layout::from_size_align(16, 8).unwrap();
    assert_eq!(layout.size(), 16);
    assert_eq!(layout.align(), 8);
}

#[test]
fn test_layout_pad_to_align() {
    let layout = Layout::from_size_align(10, 8).unwrap();
    let padded = layout.pad_to_align();
    assert_eq!(padded.size(), 16); // Padded to next multiple of 8
}

#[test]
fn test_align_of_type() {
    let align = Align::of::<u64>();
    assert_eq!(align.as_usize(), 8);
}

#[test]
fn test_align_up() {
    let align = Align::of::<u64>(); // 8-byte alignment
    assert_eq!(align.align_up(10), 16);
    assert_eq!(align.align_up(16), 16);
    assert_eq!(align.align_up(17), 24);
}

#[test]
fn test_slice_from_vec() {
    let data = [1, 2, 3, 4, 5];
    let slice = unsafe { Slice::from_raw_parts(data.as_ptr(), data.len()) };

    assert_eq!(slice.len(), 5);
    assert!(!slice.is_empty());
    assert_eq!(*slice.get(0).unwrap(), 1);
    assert_eq!(*slice.get(4).unwrap(), 5);
    assert!(slice.get(5).is_none());
}

#[test]
fn test_slice_first_last() {
    let data = [10, 20, 30];
    let slice = unsafe { Slice::from_raw_parts(data.as_ptr(), data.len()) };

    assert_eq!(*slice.first().unwrap(), 10);
    assert_eq!(*slice.last().unwrap(), 30);
}

#[test]
fn test_slice_split_at() {
    let data = [1, 2, 3, 4, 5];
    let slice = unsafe { Slice::from_raw_parts(data.as_ptr(), data.len()) };

    let (left, right) = slice.split_at(2);
    assert_eq!(left.len(), 2);
    assert_eq!(right.len(), 3);
    assert_eq!(*left.get(0).unwrap(), 1);
    assert_eq!(*right.get(0).unwrap(), 3);
}

#[test]
fn test_slice_empty() {
    let data: Vec<i32> = vec![];
    let slice = unsafe { Slice::from_raw_parts(data.as_ptr(), data.len()) };

    assert!(slice.is_empty());
    assert_eq!(slice.len(), 0);
    assert!(slice.first().is_none());
    assert!(slice.last().is_none());
}

#[test]
fn test_ownership_own() {
    let value = 42;
    let owned = Own::new(value);

    assert_eq!(*owned.as_ref(), 42);
    assert_eq!(owned.into_inner(), 42);
}

#[test]
fn test_ownership_borrow() {
    let mut value = 100;
    let owned = Own::new(&mut value);
    let borrowed = owned.borrow();

    assert_eq!(**borrowed.get(), 100);
}

#[test]
fn test_ownership_borrow_mut() {
    let value = 50;
    let mut owned = Own::new(value);
    let mut borrowed_mut = owned.borrow_mut();

    *borrowed_mut.get_mut() = 75;
    assert_eq!(*owned.as_ref(), 75);
}

#[test]
fn test_intrinsics_size_of() {
    use adeshlang::stdlib::adesh_core::intrinsics::*;

    assert_eq!(size_of::<u8>(), 1);
    assert_eq!(size_of::<u16>(), 2);
    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(size_of::<u64>(), 8);
}

#[test]
fn test_intrinsics_align_of() {
    use adeshlang::stdlib::adesh_core::intrinsics::*;

    assert_eq!(align_of::<u8>(), 1);
    assert_eq!(align_of::<u16>(), 2);
    assert_eq!(align_of::<u32>(), 4);
    assert_eq!(align_of::<u64>(), 8);
}

#[test]
fn test_intrinsics_swap() {
    use adeshlang::stdlib::adesh_core::intrinsics::*;

    let mut a = 10;
    let mut b = 20;
    swap(&mut a, &mut b);

    assert_eq!(a, 20);
    assert_eq!(b, 10);
}

#[test]
fn test_intrinsics_replace() {
    use adeshlang::stdlib::adesh_core::intrinsics::*;

    let mut value = 42;
    let old = replace(&mut value, 100);

    assert_eq!(old, 42);
    assert_eq!(value, 100);
}

#[test]
fn test_iterator_count() {
    let vec = [1, 2, 3, 4, 5];
    let count = vec.len();
    assert_eq!(count, 5);
}

#[test]
fn test_iterator_nth() {
    let vec = [10, 20, 30, 40, 50];
    let mut iter = vec.iter();
    assert_eq!(iter.nth(2), Some(&30));
    assert_eq!(iter.next(), Some(&40));
}

#[test]
fn test_iterator_all() {
    let vec = [2, 4, 6, 8];
    assert!(vec.iter().all(|&x| x % 2 == 0));
    assert!(!vec.iter().all(|&x| x > 5));
}

#[test]
fn test_iterator_any() {
    let vec = [1, 3, 5, 7];
    assert!(vec.contains(&5));
    assert!(!vec.iter().any(|&x| x % 2 == 0));
}

#[test]
fn test_iterator_find() {
    let vec = [1, 2, 3, 4, 5];
    let found = vec.iter().find(|&&x| x > 3);
    assert_eq!(found, Some(&4));
}
