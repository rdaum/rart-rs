# `rart` - Ryan's Adaptive Radix Tree

A high-performance, memory-efficient implementation of Adaptive Radix Trees (ART) in Rust, with
support for both single-threaded and versioned concurrent data structures.

[![Crates.io](https://img.shields.io/crates/v/rart.svg)](https://crates.io/crates/rart)
[![Documentation](https://docs.rs/rart/badge.svg)](https://docs.rs/rart)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/rdaum/rart-rs/blob/main/LICENSE)

## Overview

This crate provides two high-performance tree implementations:

1. **`AdaptiveRadixTree`** - Single-threaded radix tree optimized for speed
2. **`VersionedAdaptiveRadixTree`** - Thread-safe versioned tree with copy-on-write snapshots for
   concurrent workloads

Both trees automatically adjust their internal representation based on data density for ordered
associative data structures.

## Tree Types

### AdaptiveRadixTree - Single-threaded Performance

**Key Features:**

- Optimized for single-threaded performance
- Cache-friendly memory layout for modern CPU architectures
- SIMD support for vectorized operations (x86 SSE and ARM NEON)
- Efficient iteration over key ranges with proper ordering

**Best for:** Single-threaded applications.

```rust
use rart::{AdaptiveRadixTree, ArrayKey};

let mut tree = AdaptiveRadixTree::<ArrayKey<16>, String>::new();
tree.insert("apple", "fruit".to_string());
tree.insert("application", "software".to_string());

assert_eq!(tree.get("apple"), Some(&"fruit".to_string()));

// Range queries and iteration
for (key, value) in tree.iter() {
    println!("{:?} -> {}", key.as_ref(), value);
}
```

### VersionedAdaptiveRadixTree - Concurrent Versioning

**Key Features:**

- O(1) snapshots: Create new versions without copying data
- Copy-on-write mutations: Only copy nodes along modified paths
- Structural sharing: Unmodified subtrees shared between versions
- Thread-safe: Snapshots can be moved across threads safely
- Multiversion support for database and concurrent applications

**Best for:** Concurrent versioned workloads, databases, multi-reader systems.

```rust
use rart::{VersionedAdaptiveRadixTree, ArrayKey};

let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, String>::new();
tree.insert("key1", "value1".to_string());

// O(1) snapshot creation
let mut snapshot = tree.snapshot();

// Independent mutations
tree.insert("key2", "value2".to_string());      // Only in original
snapshot.insert("key3", "value3".to_string());  // Only in snapshot

assert_eq!(tree.get("key3"), None);
assert_eq!(snapshot.get("key2"), None);
assert_eq!(snapshot.get("key3"), Some(&"value3".to_string()));
```

## Key Types

Both trees support flexible key types optimized for different use cases:

- **`ArrayKey<N>`**: Fixed-size keys up to N bytes, stack-allocated for performance
- **`VectorKey`**: Variable-size keys, heap-allocated for flexibility

```rust
use rart::{ArrayKey, VectorKey};

// Fixed-size keys (recommended for performance)
let key1: ArrayKey<16> = "hello".into();
let key2: ArrayKey<8> = 42u64.into();

// Variable-size keys (for dynamic content)
let key3: VectorKey = "hello world".into();
let key4: VectorKey = 1337u32.into();
```

## Performance

### Single-threaded Performance (AdaptiveRadixTree)

Performance characteristics for sequential and random access patterns:

**Sequential access**:

- ART: ~2ns (10x faster than random access)
- HashMap: ~10ns
- BTree: ~22ns

**Random access**:

- ART: ~14ns (comparable to HashMap)
- HashMap: ~14ns
- BTree: ~55ns

### Versioned Tree Performance (VersionedAdaptiveRadixTree)

Optimized for transactional workloads with copy-on-write semantics:

**Lookup Performance** (vs [im crate](https://crates.io/crates/im) persistent collections):

- Small datasets (256-1024 elements): VersionedART 8.7ns vs im::HashMap 15.2ns and im::OrdMap 13.6ns
- Medium datasets (16k elements): VersionedART 17.1ns vs im::HashMap 21.5ns and im::OrdMap 27.5ns
- Generally 1.3-1.7x faster than alternatives across most workloads

**Sequential Scanning**:

- Good cache locality for most dataset sizes
- 256 elements: VersionedART 1.2µs vs im types 2.2µs (1.8x faster)
- 1024 elements: VersionedART 7.2µs vs im::HashMap 9.9µs/im::OrdMap 10.7µs (1.4-1.5x faster)
- 16k elements: VersionedART 149µs vs im::HashMap 260µs/im::OrdMap 289µs (1.7-1.9x faster)

**Snapshot Operations**:

- O(1) snapshots: ~2.8ns consistently regardless of tree size (256-16k elements)
- im::HashMap clone: ~6.2ns (2.2x slower)
- im::OrdMap clone: ~2.8ns (comparable, but lacks structural sharing)

**Copy-on-Write Efficiency**:

- Multiple mutations per snapshot: im types excel here due to different design trade-offs
- Structural sharing: Memory advantages for concurrent access patterns
- Versioned workloads: Better for read-heavy scenarios with occasional snapshots

**Best suited for**: Read-heavy versioned workloads, database snapshots, concurrent systems
requiring point-in-time consistency and efficient structural sharing.

**[📊 View Complete Performance Analysis](benchmarks/PERFORMANCE_ANALYSIS.md)** - Detailed benchmarks, technical insights, and workload recommendations.

_Benchmarks run on AMD Ryzen 9 7940HS using Criterion.rs_

## Architecture

Both implementations use several key optimizations:

- **Adaptive node types**: 4, 16, 48, and 256-child nodes based on density
- **Path compression**: Stores common prefixes to reduce tree height
- **SIMD acceleration**: Vectorized search operations
- **Memory efficiency**: Minimizes allocations during operations

**Additional for VersionedAdaptiveRadixTree:**

- **Arc-based sharing**: Safe structural sharing across snapshots
- **Version tracking**: Efficient copy-on-write detection
- **Optimized CoW**: Only copies when nodes are actually shared

## Implementation Notes

Based on
["The Adaptive Radix Tree: ARTful Indexing for Main-Memory Databases"](https://db.in.tum.de/~leis/papers/ART.pdf)
by Viktor Leis, Alfons Kemper, and Thomas Neumann, with additional optimizations for Rust and
versioning support.

**Technical Details:**

- Compiles on stable Rust
- Minimal external dependencies
- Safe public API with compartmentalized unsafe code for performance
- Comprehensive test suite including property-based fuzzing
- Multi-threaded fuzz testing for versioned trees
- Extensive benchmarks against standard library and `im` crate collections

## Documentation

For detailed API documentation and examples, visit [docs.rs/rart](https://docs.rs/rart).

## License

Licensed under the Apache License, Version 2.0. See
[LICENSE](https://github.com/rdaum/rart-rs/blob/main/LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.
