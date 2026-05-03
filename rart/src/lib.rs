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
//! use rart::{AdaptiveRadixTree, ArrayKey};
//!
//! // Create a new tree with fixed-size keys
//! let mut tree = AdaptiveRadixTree::<ArrayKey<16>, String>::new();
//!
//! // Insert some data
//! tree.insert("hello", "world".to_string());
//! tree.insert("foo", "bar".to_string());
//!
//! // Query the tree
//! debug_assert_eq!(tree.get("hello"), Some(&"world".to_string()));
//! debug_assert_eq!(tree.get("missing"), None);
//!
//! // Iterate over entries
//! for (key, value) in tree.iter() {
//!     println!("{:?} -> {}", key.as_ref(), value);
//! }
//! ```
//!
//! ## Key Types
//!
//! RART supports three main key types:
//!
//! - [`ArrayKey<N>`]: Fixed-size keys up to N bytes, stack-allocated
//! - [`VectorKey`]: Variable-size keys, heap-allocated
//! - [`OverflowKey<K, P>`]: Variable-size keys with inline storage for short keys and boxed
//!   overflow for longer keys
//!
//! All key types support automatic conversion from common Rust types:
//!
//! ```rust
//! use rart::{ArrayKey, OverflowKey, VectorKey};
//!
//! // From string literals
//! let key1: ArrayKey<16> = "hello".into();
//! let key2: VectorKey = "world".into();
//! let key3: OverflowKey<32, 8> = "mostly short".into();
//!
//! // From numeric types
//! let key4: ArrayKey<8> = 42u64.into();
//! let key5: VectorKey = 1337u32.into();
//! ```

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
pub mod versioned_tree;

// Re-export main types for convenience
pub use iter::LendingKeyView;
pub use keys::{KeyTrait, array_key::ArrayKey, overflow_key::OverflowKey, vector_key::VectorKey};
pub use partials::Partial;
pub use tree::AdaptiveRadixTree;
pub use versioned_tree::VersionedAdaptiveRadixTree;
