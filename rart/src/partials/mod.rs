//! Partial key types and traits for RART.
//!
//! This module provides partial key types that represent fragments of keys used internally
//! by the Adaptive Radix Tree for efficient trie operations. Partials are used for prefix
//! compression and node navigation.
//!
//! ## Available Partial Types
//!
//! - [`ArrPartial<N>`](array_partial::ArrPartial): Fixed-size partial keys up to N bytes
//! - [`VectorPartial`](vector_partial::VectorPartial): Variable-size partial keys
//!
//! ## Usage
//!
//! Partials are typically created automatically by the tree implementation, but you can
//! work with them directly:
//!
//! ```rust
//! use rart::partials::{Partial, array_partial::ArrPartial};
//!
//! let partial: ArrPartial<16> = "hello".as_bytes().into();
//! debug_assert_eq!(partial.len(), 5);
//! debug_assert!(partial.starts_with(b"hel"));
//! ```

use crate::keys::KeyTrait;

pub mod array_partial;
pub mod vector_partial;

#[inline]
pub(crate) fn prefix_length_bytes(lhs: &[u8], rhs: &[u8]) -> usize {
    let len = lhs.len().min(rhs.len());
    let mut idx = 0;

    while idx + 8 <= len {
        let lhs_word = u64::from_ne_bytes(lhs[idx..idx + 8].try_into().unwrap());
        let rhs_word = u64::from_ne_bytes(rhs[idx..idx + 8].try_into().unwrap());
        let diff = lhs_word ^ rhs_word;
        if diff != 0 {
            return idx + (diff.trailing_zeros() as usize / 8);
        }
        idx += 8;
    }

    while idx < len {
        if lhs[idx] != rhs[idx] {
            break;
        }
        idx += 1;
    }
    idx
}

pub trait Partial: AsRef<[u8]> {
    /// Returns a partial up to `length` bytes.
    fn partial_before(&self, length: usize) -> Self;
    /// Returns a partial from `src_offset` onwards with `length` bytes.
    fn partial_from(&self, src_offset: usize, length: usize) -> Self;
    /// Returns a partial from `start` onwards.
    fn partial_after(&self, start: usize) -> Self;
    /// Extends the partial with another partial.
    fn partial_extended_with(&self, other: &Self) -> Self;
    /// Returns the byte at `pos`.
    fn at(&self, pos: usize) -> u8;
    /// Returns the length of the partial.
    fn len(&self) -> usize;
    /// Returns true if the partial is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Returns the length of the common prefix between `self` and `other`.
    fn prefix_length_common(&self, other: &Self) -> usize;
    /// Returns the length of the common prefix between `self` and `key`.
    fn prefix_length_key<'a, K>(&self, key: &'a K, at_depth: usize) -> usize
    where
        K: KeyTrait<PartialType = Self> + 'a;
    /// Returns the length of the common prefix between `self` and `slice`.
    fn prefix_length_slice(&self, slice: &[u8]) -> usize;
    /// Return a slice form of the partial. Warning: could take copy, depending on the implementation.
    /// Really just for debugging purposes.
    fn to_slice(&self) -> &[u8];

    /// Returns an iterator over the bytes in the partial.
    fn iter(&self) -> std::slice::Iter<'_, u8> {
        self.as_ref().iter()
    }

    /// Returns true if the partial starts with the given prefix.
    fn starts_with(&self, prefix: &[u8]) -> bool {
        self.as_ref().starts_with(prefix)
    }

    /// Returns true if the partial ends with the given suffix.
    fn ends_with(&self, suffix: &[u8]) -> bool {
        self.as_ref().ends_with(suffix)
    }
}
