# `rart` - Ryan's Adaptive Radix Tree

A high-performance, memory-efficient implementation of Adaptive Radix Trees (ART) in Rust, with
support for both single-threaded and versioned concurrent data structures.

[![Crates.io](https://img.shields.io/crates/v/rart.svg)](https://crates.io/crates/rart)
[![Documentation](https://docs.rs/rart/badge.svg)](https://docs.rs/rart)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/rdaum/rart-rs/blob/main/LICENSE)
[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%9D%A4-pink)](https://github.com/sponsors/rdaum)

If `rart` is useful in your work, consider sponsoring development on GitHub Sponsors.

> [!NOTE] I am also available for consulting in systems engineering, profiling and performance
> tuning, and Rust development (10 years at Google, 25+ years in software development). If this
> project is useful or interesting for your team, feel free to reach out.

## Overview

If you just want the short version: `rart` is a very fast ordered key-value store for workloads
where keys share structure and you care about more than plain exact-match lookup.

It is a good fit when you want things like:

- fast exact lookup without giving up sorted order
- prefix queries such as routing, path matching, or subtree scans
- prefix-structured joins or intersections where shared structure should let you skip work
- longest-prefix-match behavior
- snapshotting / structural sharing in the versioned tree

Typical examples of shared-prefix keyspaces:

- HTTP routes and URL paths such as `/api/v1/users/...`
- filesystem or object-store paths
- metric names, tags, or hierarchical telemetry keys
- DNS names, hostnames, and reversed-domain identifiers
- network prefixes, binary protocol prefixes, or trie-friendly encoded IDs
- multi-tenant keys where the tenant or partition is a leading prefix
- LLM / inference-system cache and routing keys where many requests share a prompt, model, tenant,
  or session prefix
- "datalog" or relational tuple indexes, for example `(relation, entity, attribute, value)` or
  `(tenant, table, primary_key)`, where leading bound columns form natural trie prefixes

If all you need is “give me the value for this key” with no ordering or prefix behavior, a plain
hash table is often simpler. If you need lots of full ordered scans, a `BTreeMap` may still be the
better fit. `rart` is for the middle ground where order and prefix structure matter and you want
them to be fast.

An Adaptive Radix Tree is an ordered map built on trie semantics rather than comparison-based tree
rotation or hashing. Keys are treated as byte sequences, shared prefixes are stored once, and each
inner node changes shape as fanout grows (`4`, `16`, `48`, `256` children). In practice that gives
you a data structure with a distinctive profile:

- exact lookup and insert costs scale with key length rather than collection size
- ordered traversal and range queries come naturally
- prefix operations are first-class rather than bolted on
- shared prefixes improve locality and can cut repeated key work

ARTs are a good fit when your keys are naturally byte-addressable and you care about one or more of
the following:

- very fast point lookup on ordered keys
- prefix search, subtree iteration, or longest-prefix match
- prefix-aware join/intersection behavior that can prune whole subtrees early
- stable ordered semantics without `BTreeMap`'s comparison-heavy path
- structural sharing over radix nodes for versioned or snapshot-oriented workloads

They are usually a worse fit when your workload is mostly:

- full-map scans where key reconstruction cost dominates
- short-lived tiny maps where simpler structures win on constant factors
- pure exact-match hashing workloads with no need for order or prefix semantics

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
- Optional `triomphe-arc` feature for lower-overhead shared ownership in the versioned tree

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

### Lending traversal APIs

For perf-sensitive traversal, prefer the lending callback APIs over materializing owned keys:

- `for_each_view`
- `prefix_for_each_view` / `prefix_for_each_view_k`
- `for_each_range_view`
- `with_longest_prefix_match_view` / `with_longest_prefix_match_view_k`
- `intersect_lending_with`

These expose a `LendingKeyView` tied to the callback invocation, so the tree can reuse traversal
scratch state instead of rebuilding or cloning per-item key views.

Performance tradeoff:

- Low overlap: the ART-native intersection can outperform a `BTreeMap` merge join by pruning whole
  subtrees early
- High overlap: a `BTreeMap` merge join can still be faster
- If you only need counts or value pairs, prefer `intersect_count` or `intersect_values_with` over
  reconstructing keys

## Performance

Benchmark environment: NVIDIA GB10 (NVIDIA Spark equivalent, ASUS GX10 variant), ARM Cortex-X925,
Criterion.rs. Numbers below are from the default quick benchmark profile (`RART_BENCH_FULL` unset).
For longer high-confidence runs, use `RART_BENCH_FULL=1`.

Comparison baselines in this section:

- `HashMap`: Rust's standard hash table
- `BTreeMap`: Rust's standard ordered map
- `BLART`: the [`blart`](https://crates.io/crates/blart) crate, another Adaptive Radix Tree
  implementation and the most directly comparable external radix-tree baseline in this benchmark set

### Single-threaded Performance (AdaptiveRadixTree)

Quick read on this machine:

- `rart` is excellent at point lookup.
  - `seq_get` at `32768`: rart `3.1ns`, `HashMap` `7.4ns`, `BLART` `8.8ns`, `BTreeMap` `21.9ns`
- Inserts are competitive with `HashMap`.
  - `seq_insert`: rart `33.4ns`, `HashMap` `34.3ns`, `BTreeMap` `43.6ns`
- Prefix-structured workloads are one of the big reasons to choose it.
  - `longest_prefix_match` at `32768`: rart `1.83ms`, `BTreeMap` `6.78ms`
  - low-overlap intersection at `n100000/o10`: `intersect_with` `64.1us`, `BTreeMap` merge join
    `133.8us`
- Full-key iteration is the main weak spot.
  - full iteration at `32768`: rart `310us`, `BLART` `61.8us`, `BTreeMap` `29.1us`, `HashMap`
    `20.5us`
- The lending traversal APIs are the preferred fast path when you can consume keys inside a
  callback.
  - full traversal at `32768`: owned `587.8us`, lending `223.0us`
  - ranged traversal at `32768`: owned `297.8us`, lending `147.9us`
  - narrow prefix traversal at `32768`: owned `591.8us`, lending `217.4us`

Short version:

- choose `rart` for lookup-heavy, prefix-aware, or low-overlap join workloads
- choose `BTreeMap` when broad ordered scans dominate
- choose `HashMap` when you only need flat exact-match lookup

### Versioned Tree Performance (VersionedAdaptiveRadixTree)

The versioned tree has a similarly clear profile: it is read-leaning, lookup-strong, and not the
best choice for heavy persistent mutation bursts.

Comparisons in this section use the [`imbl`](https://crates.io/crates/imbl) crate, specifically its
persistent `HashMap` and `OrdMap`.

Quick read on this machine:

- persistent lookup is strong
  - `lookup_comparison/16384`: versioned rart `15.1ns`, `imbl::HashMap` `23.4ns`, `imbl::OrdMap`
    `38.6ns`
- sequential scan is also strong
  - `sequential_scan/16384`: versioned rart `126.2us`, `imbl::HashMap` `191.2us`, `imbl::OrdMap`
    `470.3us`
- mutation-heavy snapshot workloads still favor `imbl`
  - `mutations_per_snapshot/100`: versioned rart `102.8us`, `imbl::HashMap` `58.1us`, `imbl::OrdMap`
    `35.5us`

Short version:

- choose `VersionedAdaptiveRadixTree` for read-heavy versioned workloads
- choose `imbl` when repeated persistent mutation bursts dominate

Optional feature:

- Enable `triomphe-arc` to replace `std::sync::Arc` with `triomphe::Arc` in the versioned tree
- In local quick-profile `versioned_tree_bench` runs this improved mutation/snapshot-sharing
  workloads by roughly `2-4%`, while lookup and scan workloads stayed approximately flat

**Best suited for**: Read-heavy versioned workloads, database snapshots, concurrent systems
requiring point-in-time consistency and efficient structural sharing.

Detailed benchmark analysis, graphs, and workload notes live in
[benchmarks/PERFORMANCE_ANALYSIS.md](https://github.com/rdaum/rart-rs/blob/main/benchmarks/PERFORMANCE_ANALYSIS.md).

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
- **Optional `triomphe` backend**: `triomphe-arc` swaps the shared pointer implementation used by
  versioned nodes

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
- Extensive benchmarks against standard library, `imbl`, and other radix-tree baselines

## Documentation

For detailed API documentation and examples, visit [docs.rs/rart](https://docs.rs/rart).

## License

Licensed under the Apache License, Version 2.0. See
[LICENSE](https://github.com/rdaum/rart-rs/blob/main/LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.
