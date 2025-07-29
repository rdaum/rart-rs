//! Key types and traits for RART.
//!
//! This module provides the key types and traits that can be used with Adaptive Radix Trees.
//! Keys represent the lookup values and must implement the [`KeyTrait`] to work with the tree.
//!
//! ## Available Key Types
//!
//! - [`ArrayKey<N>`](array_key::ArrayKey): Fixed-size keys up to N bytes, stored on the stack
//! - [`VectorKey`](vector_key::VectorKey): Variable-size keys stored on the heap
//!
//! ## Custom Keys
//!
//! You can implement custom key types by implementing the [`KeyTrait`]:
//!
//! ```rust
//! use rart::keys::KeyTrait;
//! use rart::partials::Partial;
//!
//! // Example: A wrapper around a string that implements KeyTrait
//! // (You would need to implement all required methods)
//! ```

use crate::partials::Partial;

pub mod array_key;
pub mod vector_key;

/// Trait for types that can be used as keys in an Adaptive Radix Tree.
///
/// This trait defines the interface that all key types must implement to work with RART.
/// Keys are the lookup values stored in the tree and must be convertible to byte sequences
/// for trie navigation.
///
/// ## Requirements
///
/// - Must be cloneable, comparable, and orderable
/// - Must provide access to underlying bytes via `AsRef<[u8]>`
/// - Must have an associated `PartialType` for prefix operations
/// - Must support conversion to/from partials for tree operations
///
/// ## Example Implementation
///
/// ```rust
/// use rart::keys::KeyTrait;
/// use rart::partials::{Partial, array_partial::ArrPartial};
///
/// // Note: This is a simplified example. Real implementations require
/// // implementing all trait methods properly.
/// ```
pub trait KeyTrait: Clone + PartialEq + Eq + PartialOrd + Ord + AsRef<[u8]> {
    /// The partial type associated with this key type.
    type PartialType: Partial + From<Self> + Clone + PartialEq;

    /// Maximum size of this key type, if any.
    const MAXIMUM_SIZE: Option<usize>;

    /// Create a new key from a byte slice.
    fn new_from_slice(slice: &[u8]) -> Self;
    /// Create a new key from a partial.
    fn new_from_partial(partial: &Self::PartialType) -> Self;

    /// Extend this key with bytes from a partial.
    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self;
    /// Truncate this key to the specified depth.
    fn truncate(&self, at_depth: usize) -> Self;
    /// Get the byte at the specified position.
    fn at(&self, pos: usize) -> u8;
    /// Get the length of the key starting from the specified depth.
    fn length_at(&self, at_depth: usize) -> usize;
    /// Convert part of this key to a partial starting from the specified depth.
    fn to_partial(&self, at_depth: usize) -> Self::PartialType;
    /// Check if this key matches the given byte slice exactly.
    fn matches_slice(&self, slice: &[u8]) -> bool;
}
