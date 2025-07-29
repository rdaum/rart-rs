//! # RART - Ryan's Adaptive Radix Tree
//!
//! A high-performance, memory-efficient implementation of Adaptive Radix Trees (ART) in Rust.
//!
//! ## Overview
//!
//! Adaptive Radix Trees are a type of trie data structure that automatically adjusts
//! its internal representation based on the number of children at each node, providing
//! excellent performance characteristics:
//!
//! - **Space efficient**: Compact representation that adapts to data density
//! - **Cache friendly**: Optimized memory layout for modern CPU architectures  
//! - **Fast operations**: O(k) complexity where k is the key length
//! - **Range queries**: Efficient iteration over key ranges
//!
//! ## Quick Start
//!
//! ```rust
//! use rart::{AdaptiveRadixTree, ArrayKey, TreeTrait};
//!
//! // Create a new tree with fixed-size keys
//! let mut tree = AdaptiveRadixTree::<ArrayKey<16>, String>::new();
//!
//! // Insert some data
//! tree.insert("hello", "world".to_string());
//! tree.insert("foo", "bar".to_string());
//!
//! // Query the tree
//! assert_eq!(tree.get("hello"), Some(&"world".to_string()));
//! assert_eq!(tree.get("missing"), None);
//!
//! // Iterate over entries
//! for (key, value) in tree.iter() {
//!     println!("{:?} -> {}", key.as_ref(), value);
//! }
//! ```
//!
//! ## Key Types
//!
//! RART supports two main key types:
//!
//! - [`ArrayKey<N>`]: Fixed-size keys up to N bytes, stack-allocated
//! - [`VectorKey`]: Variable-size keys, heap-allocated  
//!
//! Both key types support automatic conversion from common Rust types:
//!
//! ```rust
//! use rart::{ArrayKey, VectorKey};
//!
//! // From string literals
//! let key1: ArrayKey<16> = "hello".into();
//! let key2: VectorKey = "world".into();
//!
//! // From numeric types
//! let key3: ArrayKey<8> = 42u64.into();
//! let key4: VectorKey = 1337u32.into();
//! ```

use crate::iter::Iter;
use crate::range::Range;
use std::ops::RangeBounds;

// Private implementation modules
mod node;

// Internal modules (public for benchmarking, not part of stable API)
#[doc(hidden)]
pub mod mapping;
#[doc(hidden)]
pub mod utils;

// Public API modules
pub mod iter;
pub mod keys;
pub mod partials;
pub mod range;
pub mod stats;
pub mod tree;

// Re-export main types for convenience
pub use keys::{KeyTrait, array_key::ArrayKey, vector_key::VectorKey};
pub use partials::Partial;
pub use tree::AdaptiveRadixTree;

pub trait TreeTrait<KeyType, ValueType>
where
    KeyType: keys::KeyTrait,
{
    type NodeType;

    fn get<Key>(&self, key: Key) -> Option<&ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_k(&key.into())
    }
    fn get_k(&self, key: &KeyType) -> Option<&ValueType>;
    fn get_mut<Key>(&mut self, key: Key) -> Option<&mut ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_mut_k(&key.into())
    }
    fn get_mut_k(&mut self, key: &KeyType) -> Option<&mut ValueType>;
    fn insert<KV>(&mut self, key: KV, value: ValueType) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.insert_k(&key.into(), value)
    }
    fn insert_k(&mut self, key: &KeyType, value: ValueType) -> Option<ValueType>;

    fn remove<KV>(&mut self, key: KV) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.remove_k(&key.into())
    }
    fn remove_k(&mut self, key: &KeyType) -> Option<ValueType>;

    fn iter(&self) -> Iter<'_, KeyType, KeyType::PartialType, ValueType>;

    fn range<'a, R>(&'a self, range: R) -> Range<'a, KeyType, ValueType>
    where
        R: RangeBounds<KeyType> + 'a;

    fn is_empty(&self) -> bool;
}
