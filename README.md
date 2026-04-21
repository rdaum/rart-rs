# `rart` - Ryan's Adaptive Radix Tree

A high-performance, memory-efficient implementation of Adaptive Radix Trees (ART) in Rust, with
support for both single-threaded and versioned concurrent data structures.

[![Crates.io](https://img.shields.io/crates/v/rart.svg)](https://crates.io/crates/rart)
[![Documentation](https://docs.rs/rart/badge.svg)](https://docs.rs/rart)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/rdaum/rart-rs/blob/main/LICENSE)
[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%9D%A4-pink)](https://github.com/sponsors/rdaum)

If `rart` is useful in your work, consider sponsoring development on GitHub Sponsors.
> [!NOTE]
> I am also available for consulting in systems engineering, profiling and performance tuning, and
> Rust development (10 years at Google, 25+ years in software development). If this project is
> useful or interesting for your team, feel free to reach out.

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

let mut tree = AdaptiveRadixTree::<ArrayKey<16 >, String>::new();
tree.insert("apple", "fruit".to_string());
tree.insert("application", "software".to_string());

assert_eq!(tree.get("apple"), Some(&"fruit".to_string()));

// Range queries and iteration
for (key, value) in tree.iter() {
println ! ("{:?} -> {}", key.as_ref(), value);
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

let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16 >, String>::new();
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
let key1: ArrayKey<16 > = "hello".into();
let key2: ArrayKey<8 > = 42u64.into();

// Variable-size keys (for dynamic content)
let key3: VectorKey = "hello world".into();
let key4: VectorKey = 1337u32.into();
```

## Prefix Operations

`AdaptiveRadixTree` now exposes explicit prefix-oriented APIs:

- `longest_prefix_match` / `longest_prefix_match_k`
- `prefix_iter` / `prefix_iter_k`

These are useful when exact lookup is not enough:

- `longest_prefix_match*`: find the deepest stored key that is a prefix of a probe key
- `prefix_iter*`: iterate only the subtree under a prefix, in sorted key order

```rust
use rart::{AdaptiveRadixTree, KeyTrait, VectorKey};

let mut tree = AdaptiveRadixTree::<VectorKey, u32>::new();
tree.insert_k(&VectorKey::new_from_slice(b"cat"), 1);
tree.insert_k(&VectorKey::new_from_slice(b"catalog"), 2);
tree.insert_k(&VectorKey::new_from_slice(b"dog"), 3);

let (k, v) = tree
    .longest_prefix_match_k(&VectorKey::new_from_slice(b"catalogue"))
    .unwrap();
assert_eq!(k.as_ref(), b"catalog");
assert_eq!(*v, 2);

let prefix = VectorKey::new_from_slice(b"cat");
let matches: Vec<_> = tree.prefix_iter_k(&prefix).map(|(k, _)| k).collect();
assert_eq!(matches.len(), 2);
```

Typical uses:

- URL/path routing: match `/api/v1/users/42` to the best registered prefix
- Network prefix tables: longest-prefix lookup for address-like keys
- Policy/config lookup: most specific override wins
- Autocomplete/search narrowing: iterate all keys under a typed prefix
- Prefix cache reuse: find best existing cached prefix before extending

How this differs from standard maps:

- `HashMap`: no ordered prefix traversal; prefix queries require scanning keys
- `BTreeMap`: prefix ranges are possible, but longest-prefix matching is not a built-in operation

## Intersection Operations

`AdaptiveRadixTree` also exposes ART-native intersection/join APIs for finding keys present in two
trees:

- `intersect_with`: visit matching keys and both values
- `intersect_values_with`: visit only value pairs, avoiding key reconstruction
- `intersect_count`: count overlapping keys

These methods walk both radix tries in lockstep and prune mismatched prefixes early rather than
merging two fully materialized key streams.

```rust
use rart::{AdaptiveRadixTree, ArrayKey};

let mut left = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
let mut right = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

left.insert("ab", 1);
left.insert("abc", 2);
left.insert("dog", 3);

right.insert("abc", 20);
right.insert("dog", 30);
right.insert("zzz", 40);

let mut joined = Vec::new();
left.intersect_with(&right, |key, left_value, right_value| {
    joined.push((key, *left_value, *right_value));
});

assert_eq!(left.intersect_count(&right), 2);

let mut value_pairs = Vec::new();
left.intersect_values_with(&right, |left_value, right_value| {
    value_pairs.push((*left_value, *right_value));
});
assert_eq!(value_pairs.len(), 2);
```

Typical uses:

- Joining two in-memory indexes by shared key
- Counting overlap between sparse keysets
- Intersecting filtered working sets before more expensive processing

Performance tradeoff:

- Low overlap: the ART-native intersection can outperform a `BTreeMap` merge join by pruning
  whole subtrees early
- High overlap: a `BTreeMap` merge join can still be faster
- If you only need counts or value pairs, prefer `intersect_count` or `intersect_values_with`
  over reconstructing keys

## Performance

Benchmark environment: NVIDIA GB10 (NVIDIA Spark equivalent, ASUS GX10 variant), ARM Cortex-X925, Criterion.rs.
Numbers below are from the default quick benchmark profile (`RART_BENCH_FULL` unset). For longer high-confidence runs, use `RART_BENCH_FULL=1`.

### Single-threaded Performance (AdaptiveRadixTree)

Representative current results from the quick Criterion profile:

**Point Lookup / Mutation** (`art_bench`, quick profile):

- `seq_insert`: ~24.3ns
- `seq_get`: ~6.7ns at 1k keys, ~7.3ns at 32k keys, ~9.5ns at 131k keys
- `seq_remove`: ~16.5ns at 1k keys, ~19.0ns at 32k keys, ~26.2ns at 131k keys
- `rand_get`: ~19.9ns at 1k keys, ~17.9ns at 32k keys, ~24.2ns at 131k keys
- In these quick-profile runs, ART remains strongest on ordered and lookup-heavy point operations

**Iteration** (key discovery):

- ART: ~8.2ns (slower than peers)
- HashMap: ~0.6ns
- BTree: ~0.9ns
- *Note: ART is heavily optimized for ordered key probes from the caller (leveraging cache locality of prefixes). Iterating the ART itself requires reconstructing keys from compressed paths, which is more expensive than BTree leaf traversal.*
- *ART still provides ordered key semantics (sorted traversal/range behavior), unlike `HashMap`.*

**Value-only Iteration** (`values_iter`, 32k elements):

- ART: ~2.05ns/element
- BLART: ~1.96ns/element
- BTreeMap: ~0.87ns/element
- HashMap: ~0.63ns/element
- *`values_iter` avoids key reconstruction and is ~4x faster than ART full iteration in this run (~8.2ns/element).*

**Prefix-specific Operations** (`prefix_bench`, quick profile):

- `longest_prefix_match` (32k probes):
- ART: ~3.45ms total (~9.5M probes/sec)
- BTreeMap baseline: ~6.73ms total (~4.9M probes/sec)
- HashMap baseline: ~1.31ms total (~25.1M probes/sec)
- *HashMap baseline uses repeated exact lookups on shorter prefixes; this is fast but does not provide ordered subtree traversal.*

- `prefix_iter` (narrow prefixes, 32k tree, 1024 queries):
- ART: ~852µs total (~1.20M queries/sec)
- BTreeMap baseline: ~122µs total (~8.4M queries/sec)
- HashMap baseline: ~100ms total (~10K queries/sec)
- *ART is much faster than hash-scan for prefix enumeration, while BTree range iteration is still faster in this benchmark.*

**Tree Intersection / Join** (`intersection_join_bench`, quick profile):

- Low overlap workloads favor ART intersection because mismatched prefixes let it skip subtrees
  early
- High overlap workloads favor `BTreeMap` merge join more often
- `intersect_count` is the lightest-weight option when you only need cardinality

**Comparison-oriented ART workloads** (`art_compare_bench`, quick profile):

- `rand_insert/art`: ~259ns
- `rand_delete/art`: ~82ns
- `seq_delete/art`: ~27.6ns
- `random_get_str/art`: ~186ns at 1k keys, ~187ns at 4k keys, ~243ns at 32k keys, ~188ns at 131k
  keys
- `random_get_str/art_cached_keys`: ~112ns at 1k, 4k, and 32k keys, ~112ns at 131k keys

Current caveats from the same quick-profile comparison:

- Some cached-key and mid-sized string-lookup workloads are less favorable than the best point
  lookup cases in the suite
- As usual with ART, workload shape matters: externally supplied ordered probes are a much better
  fit than full-tree key discovery

### Versioned Tree Performance (VersionedAdaptiveRadixTree)

Optimized for transactional workloads with copy-on-write semantics:

**Lookup Performance** (vs persistent collections from the [im crate](https://crates.io/crates/im)):

_Comparison against im::HashMap (HAMT) and im::OrdMap (B-tree), both persistent data structures with
structural sharing:_

- Small datasets (256 elements): VersionedART 8.9ns vs im::HashMap 18.8ns and im::OrdMap 17.4ns
- Medium datasets (16k elements): VersionedART 16.4ns vs im::HashMap 28.5ns and im::OrdMap 32.0ns
- In these benchmarks, 1.3-1.9x faster for lookup-heavy workloads

**Sequential Scanning**:

- Better cache locality due to radix tree structure vs hash-based (HAMT) and tree-based access
- 256 elements: VersionedART 1.0µs vs im types 1.9-2.7µs (2x faster)
- 16k elements: VersionedART 122µs vs im::HashMap 209µs/im::OrdMap 398µs (1.7-3.3x faster)

**Snapshot Operations**:

- O(1) snapshots: ~8.6ns consistently regardless of tree size
- im::HashMap clone: ~16.2ns (2x slower)
- im::OrdMap clone: ~8.6ns (comparable performance)

**Persistent Structure Trade-offs**:

- **Write-heavy workloads**: im types excel due to mature, optimized persistent implementations
- **Read-heavy workloads**: VersionedART's radix structure provides better cache locality
- **Both provide structural sharing** - VersionedART via CoW radix nodes, im types via HAMT/B-tree
  sharing
- **Sequential access**: VersionedART's prefix compression provides significant advantages

**Best suited for**: Read-heavy versioned workloads, database snapshots, concurrent systems
requiring point-in-time consistency and efficient structural sharing.

**[📊 View Complete Performance Analysis](benchmarks/PERFORMANCE_ANALYSIS.md)** - Detailed
benchmarks, technical insights, and workload recommendations.

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
