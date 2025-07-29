# `rart` - Ryan's Adaptive Radix Tree

A high-performance, memory-efficient implementation of Adaptive Radix Trees (ART) in Rust.

[![Crates.io](https://img.shields.io/crates/v/rart.svg)](https://crates.io/crates/rart)
[![Documentation](https://docs.rs/rart/badge.svg)](https://docs.rs/rart)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/rdaum/rart-rs/blob/main/LICENSE)

## Overview

Adaptive Radix Trees are a type of trie data structure that automatically adjusts its internal representation based on
the number of children at each node, providing excellent performance characteristics for ordered associative data
structures.

**Key Features:**

- **Space efficient**: Compact representation that adapts to data density
- **Cache friendly**: Optimized memory layout for modern CPU architectures
- **Fast operations**: O(k) complexity where k is the key length
- **Range queries**: Efficient iteration over key ranges with proper ordering
- **Memory conscious**: Designed to minimize allocations during operation
- **SIMD support**: Vectorized operations for x86 SSE and ARM NEON

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
rart = "0.1"
```

### Basic Usage

```rust
use rart::{AdaptiveRadixTree, ArrayKey};

// Create a new tree with fixed-size keys
let mut tree = AdaptiveRadixTree::<ArrayKey<16 >, String>::new();

// Insert some data
tree.insert("apple", "fruit".to_string());
tree.insert("application", "software".to_string());
tree.insert("apply", "action".to_string());

// Query the tree
assert_eq!(tree.get("apple"), Some(&"fruit".to_string()));
assert_eq!(tree.get("orange"), None);

// Iterate over all entries (in lexicographic order)
for (key, value) in tree.iter() {
println ! ("{:?} -> {}", key.as_ref(), value);
}

// Range queries
let start: ArrayKey<16 > = "app".into();
let end: ArrayKey<16 > = "apq".into();
let apps: Vec<_ > = tree.range(start..end).collect();
// Contains: application, apply
```

### Key Types

RART provides two main key types optimized for different use cases:

- **`ArrayKey<N>`**: Fixed-size keys up to N bytes, stack-allocated for better performance
- **`VectorKey`**: Variable-size keys, heap-allocated for flexibility

```rust
use rart::{ArrayKey, VectorKey};

// Fixed-size keys (recommended for performance)
let key1: ArrayKey<16 > = "hello".into();
let key2: ArrayKey<8 > = 42u64.into();

// Variable-size keys (for dynamic content)
let key3: VectorKey = "hello world".into();
let key4: VectorKey = 1337u32.into();
```

## Performance

rart shows strong performance characteristics, particularly in sequential access patterns where it achieves significantly faster lookups compared to random access.

### Benchmark Results

**Sequential Get Performance** (32k elements):
- **ART: 2.2ns** (10x faster than random)
- HashMap: 10ns
- BTree: 22ns

![Sequential Get Performance](https://github.com/rdaum/rart-rs/blob/main/benchmarks/graphs/seq_get_violin.svg)

**Random Get Performance** (32k elements):  
- **ART: 14ns** (comparable to HashMap)
- HashMap: 14ns  
- BTree: 55ns

![Random Get Performance](https://github.com/rdaum/rart-rs/blob/main/benchmarks/graphs/random_get_violin.svg)

**Key Performance Characteristics:**
- **Sequential operations**: Strong performance due to prefix compression and cache locality
- **Random operations**: Competitive with HashMap, faster than BTree
- **Range queries**: Native support with efficient ordered iteration
- **Memory usage**: Adaptive structure scales efficiently with data density
- **Predictable scaling**: Consistent performance across dataset sizes

**[View Complete Performance Analysis](https://github.com/rdaum/rart-rs/blob/main/benchmarks/PERFORMANCE_ANALYSIS.md)** - Detailed benchmarks vs HashMap and BTree across all operations.

*Benchmarks run on AMD Ryzen 9 7940HS using Criterion.rs*

## Architecture

The implementation uses several optimizations:

- **Adaptive node types**: 4, 16, 48, and 256-child nodes based on density
- **Path compression**: Stores common prefixes to reduce tree height
- **SIMD acceleration**: Vectorized search for 16-child nodes
- **Attention to allocations**: Minimizes allocations during iteration and queries

## Implementation Notes

This implementation is based on the
paper ["The Adaptive Radix Tree: ARTful Indexing for Main-Memory Databases"](https://db.in.tum.de/~leis/papers/ART.pdf)
by Viktor Leis, Alfons Kemper, and Thomas Neumann.

**Technical Details:**

- Compiles on stable Rust
- Minimal external dependencies
- Safe public API with compartmentalized unsafe code for performance
- Comprehensive test suite including property-based fuzzing
- Extensive benchmarks comparing against standard library collections

## Documentation

For detailed API documentation and examples, visit [docs.rs/rart](https://docs.rs/rart).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](https://github.com/rdaum/rart-rs/blob/main/LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.