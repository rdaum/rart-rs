# ChangeLog

All notable changes to this project are documented in this file.

## [Unreleased]

### Added

### Changed

### Fixed

### Performance

## [0.9.0] - 2026-05-16

### Added

- Values-only prefix traversal APIs for scanning matching subtrees without key reconstruction:
  - `prefix_values_for_each` / `prefix_values_for_each_k` on `AdaptiveRadixTree`
  - `try_prefix_values_for_each` / `try_prefix_values_for_each_k` on `AdaptiveRadixTree`
  - `prefix_values_for_each` / `prefix_values_for_each_k` on `VersionedAdaptiveRadixTree`
  - `try_prefix_values_for_each` / `try_prefix_values_for_each_k` on
    `VersionedAdaptiveRadixTree`
- `VisitControl` for callback traversal early-stop control.
- `get_mut` / `get_mut_k` on `VersionedAdaptiveRadixTree` for CoW-aware mutable value lookup that
  preserves snapshot isolation.
- `OverflowKeyBuilder` and `OverflowKey::builder()` for direct byte construction of
  `OverflowKey` without a temporary `Vec` on common inline-key paths.

### Changed

### Fixed

### Performance

## [0.8.0] - 2026-05-15

### Added

- Prefix-match traversal APIs for finding stored keys that are prefixes of a probe key:
  - `prefix_match_iter` / `prefix_match_iter_k` on `AdaptiveRadixTree`
  - `prefix_match_iter` / `prefix_match_iter_k` on `VersionedAdaptiveRadixTree`
- Callback-oriented prefix-match APIs that visit matches from shortest to longest while borrowing
  matched key slices from the supplied probe key:
  - `prefix_match_for_each` / `prefix_match_for_each_k` on `AdaptiveRadixTree`
  - `prefix_match_for_each` / `prefix_match_for_each_k` on `VersionedAdaptiveRadixTree`
- Regression and property coverage for owned iterator and callback prefix-match traversal.

## [0.7.0] - 2026-05-10

### Added

- Traversal APIs on `VersionedAdaptiveRadixTree` matching the unversioned traversal surface:
  - `iter`
  - `for_each_view`
  - `values_iter`
  - `prefix_iter` / `prefix_iter_k`
  - `prefix_for_each_view` / `prefix_for_each_view_k`
  - `range`
  - `for_each_range_view`
- ART-native intersection/join APIs on `VersionedAdaptiveRadixTree`:
  - `intersect_with`
  - `intersect_lending_with`
  - `intersect_values_with`
  - `intersect_count`
- Versioned traversal regression coverage for empty trees, root leaves, prefix keys, compressed
  prefixes, no-match prefixes, ordered traversal, ranges, snapshot isolation, wide node layouts, and
  Moor-shaped object/symbol cache keys.
- Versioned intersection regression coverage for owned-key, lending-key, values-only, count,
  empty-tree, and snapshot-isolated join paths.
- Versioned-tree benchmark coverage for fair iteration comparisons against `imbl::HashMap` and
  `imbl::OrdMap`, separating owned-key traversal, lending traversal, and values-only traversal.
- Versioned-tree join benchmark coverage comparing ART-native intersections with `imbl::OrdMap`
  merge joins and `imbl::HashMap` probe joins across 10%, 50%, and 90% overlap ratios.
- Versioned prefix-invalidation benchmarks for object/symbol-shaped keys comparing ART prefix
  traversal, `imbl::OrdMap::range`, hash-map full scans, and expected point lookups.

### Changed

- Updated README versioned-tree performance guidance with fresh May 10, 2026 quick-profile benchmark
  numbers and clearer guidance around iteration and prefix-invalidation tradeoffs.

### Performance

- In local `versioned_tree_bench` quick runs:
  - `lookup_comparison/16384`: versioned rart ~`14.9 ns`, `imbl::HashMap` ~`22.6 ns`,
    `imbl::OrdMap` ~`38.0 ns`
  - `sequential_scan/16384`: versioned rart ~`132.5 us`, `imbl::HashMap` ~`187.5 us`,
    `imbl::OrdMap` ~`463.9 us`
  - `mutations_per_snapshot/100`: versioned rart ~`19.3 us`, `imbl::HashMap` ~`58.6 us`,
    `imbl::OrdMap` ~`35.7 us`
  - `full_iteration/16384`: owned versioned rart ~`169.5 us`, lending versioned rart ~`95.6 us`,
    values-only versioned rart ~`43.9 us`, `imbl::HashMap` ~`48.3 us`, `imbl::OrdMap`
    ~`44.9 us`
  - `join_comparison/n100000_o10`: lending versioned rart ~`58.5 us`, owned versioned rart
    ~`60.9 us`, `imbl::OrdMap` merge join ~`309.7 us`, `imbl::HashMap` probe join ~`3.00 ms`
  - `join_comparison/n100000_o50` values-only: versioned rart ~`259.1 us`, `imbl::OrdMap`
    ~`426.7 us`, `imbl::HashMap` ~`6.27 ms`
  - `join_comparison/n100000_o90` count-only: versioned rart ~`532.7 us`, `imbl::OrdMap`
    ~`512.3 us`, `imbl::HashMap` ~`3.87 ms`
  - object-prefix invalidation over 64 symbols: versioned rart lending prefix ~`352 ns`, owned
    prefix iterator ~`704 ns`, `imbl::OrdMap::range` ~`268 ns`, `imbl::HashMap` full scan
    ~`402 us`

## [0.6.1] - 2026-05-10

### Added

- Dense sequential tagged-key mutation benchmark coverage for `VersionedAdaptiveRadixTree`:
  - replacement of existing keys
  - insert/remove steady-state mutation
  - remove/reinsert steady-state mutation
  - each operation measured for uniquely owned trees and trees with a live snapshot
- Regression coverage for snapshot isolation after COW mutation through wide versioned nodes.
- Mapping clone regression tests for `IndexedMapping` and `DirectMapping` sparse-slot preservation.

### Changed

- Optimized `VersionedAdaptiveRadixTree` owned mutation paths to take root and child ownership before
  recursive descent, allowing `Arc::try_unwrap` to succeed when the mutated tree is uniquely owned.
- Optimized `VersionedNode` COW cloning for `Node48` and `Node256` by cloning mapping metadata and
  live child slots directly instead of rebuilding through repeated `add_child` calls.
- Added an internal `BitArray::clone_live_slots` helper so mapping clones preserve existing slot
  layout while cloning only initialized entries.

### Fixed

- Removed avoidable copy-on-write cloning during versioned insert/remove when no snapshot or other
  shared owner exists for the mutated path.
- Preserved snapshot isolation while mutating dense sequential tagged-key trees that force `Node48`
  and `Node256` layouts.

### Performance

- In local `versioned_tree_bench` quick runs for 4096 dense sequential tagged keys, comparing the
  previous implementation at `f16dbb9` against this change with the same benchmark file:
  - replace existing, owned tree: ~`10.681 ms` -> ~`838.7 us` (`12.7x` faster)
  - replace existing, live snapshot: ~`10.807 ms` -> ~`1.022 ms` (`10.6x` faster)
  - insert/remove, owned tree: ~`2.545 ms` -> ~`754.9 us` (`3.4x` faster)
  - insert/remove, live snapshot: ~`2.540 ms` -> ~`749.2 us` (`3.4x` faster)
  - remove/reinsert, owned tree: ~`20.861 ms` -> ~`1.355 ms` (`15.4x` faster)
  - remove/reinsert, live snapshot: ~`21.055 ms` -> ~`1.475 ms` (`14.3x` faster)

## [0.6.0] - 2026-05-03

### Added

- Sorted bulk-load APIs for `AdaptiveRadixTree`:
  - `bulk_load_sorted`
  - `bulk_load_sorted_unique`
  - `bulk_load_sorted_unique_by_index`
- Added `OverflowKey<K, P>` as a supported key type for variable-size keys with inline storage for
  short keys and boxed overflow for longer keys.
  - Added `OverflowPartial<P>` as the associated node-prefix representation.
  - Documented `OverflowKey<32, 8>` as a useful starting point for mixed dynamic-key workloads.
- Whole-tree `micromeasure` key-type macrobench coverage comparing `ArrayKey`, `VectorKey`, and
  `OverflowKey`.
- Key-storage `micromeasure` coverage for focused construction, insertion, lookup, iteration, byte
  scan, and prefix comparison paths.
- Additional examples covering key-type selection, prefix/range traversal, lending key views, bulk
  loading, intersection, and versioned snapshots.

### Changed

- Optimized `AdaptiveRadixTree` lookup paths to reuse a single borrowed key byte slice while walking
  the tree, reducing repeated key representation checks for dynamic key types.

### Performance

- In local `key_type_macrobenches` quick runs, `OverflowKey<32, 8>` reduced build cost versus
  `VectorKey` while keeping lookup close:
  - mixed 90% short keys: build ~`57.5 ns/key` versus ~`83.4 ns/key`; lookup ~`11.4 ns/key` versus
    ~`11.0 ns/key`
  - common-prefix 48-byte keys: build ~`59.9 ns/key` versus ~`80.4 ns/key`; lookup
    ~`10.1 ns/key` versus ~`9.1 ns/key`
  - lending iteration was slightly faster for `OverflowKey<32, 8>` in both sampled workloads.
- Added direct sorted child append paths for bulk construction of `Node4`, `Node16`, and `Node48`,
  avoiding incremental child search/growth work when the final fanout is known.
- In local `bulk_load_bench` runs, sorted callback bulk loading improved large tree construction
  versus incremental insertion:
  - sorted `u64` 131K: ~`1.39 ms` bulk load versus ~`3.6 ms` incremental baseline
  - sorted prefixed 131K: ~`1.38 ms` bulk load versus ~`2.76 ms` incremental baseline

## [0.5.0] - 2026-04-21

### Added

- Optional `triomphe-arc` feature for `VersionedAdaptiveRadixTree`, allowing the versioned tree to
  use `triomphe::Arc` instead of `std::sync::Arc`.
- Lending traversal APIs on `AdaptiveRadixTree` for perf-sensitive iteration, prefix, range,
  longest-prefix-match, and intersection paths:
  - `for_each_view`
  - `prefix_for_each_view` / `prefix_for_each_view_k`
  - `for_each_range_view`
  - `with_longest_prefix_match_view` / `with_longest_prefix_match_view_k`
  - `intersect_lending_with`
- Partial-prefix microbench coverage for `ArrPartial` and `VectorPartial` using `micromeasure`.
- Focused SIMD microbench coverage for node key search paths and `SortedKeyedMapping` probe
  patterns.
- Bitset microbench coverage for the production-relevant `Bitset64<1>` and `Bitset64<4>` node cases,
  including cross-width comparisons against narrower word sizes.
- Production node growth microbench coverage for `Node4 -> Node16`, `Node16 -> Node48`, and
  `Node48 -> Node256` transitions.

### Changed

- Switched several `MaybeUninit` extraction paths in the mappings and slot arrays to
  `assume_init_read()`, clarifying intent and simplifying move-out code.
- Mapping growth and shrink conversion helpers now move children directly between layouts instead of
  rebuilding through repeated trait-level `add_child` / `delete_child` operations.
- Replaced the earlier cloned borrowed-key traversal experiment with lending callback APIs, so
  traversal can reuse internal key reconstruction state instead of cloning per-item segment lists.

### Fixed

- Corrected `Bitset::last()` for multiword bitsets so it returns the true highest set bit.

### Performance

- In local `versioned_tree_bench` runs, `triomphe-arc` improved versioned mutation and
  snapshot-sharing workloads by roughly `2-4%` while leaving lookup and scan workloads close to
  flat.
- In local `borrowed_view_bench` runs, the lending traversal APIs materially outperformed owned-key
  traversal:
  - full iteration: ~`2.6x` faster at `1024`, ~`1.7x` faster at `4096`, ~`2.6x` faster at `32768`
  - ranged traversal: ~`1.9x` faster at `1024`, ~`1.9x` faster at `4096`, ~`2.0x` faster at `32768`
  - start-bounded traversal: ~`3.3x` faster at `1024`, ~`3.5x` faster at `4096`, ~`3.5x` faster at
    `32768`
- Switched partial-prefix comparisons to a shared chunked byte matcher, substantially improving long
  common-prefix cases in local `partial_prefix_microbenches` runs.
- Kept SIMD-enabled key search for sorted `Node16` paths after local microbench runs showed strong
  wins for misses, edge hits, and mixed hit/miss probe distributions.
- Specialized `Bitset64<1>` / `Bitset64<4>` scan paths after local microbench runs confirmed they
  remain the best fit for `Node48` and `Node256`.
- Reduced node growth conversion costs in local `grow_node_production` microbench runs:
  - `Node16 -> Node48`: ~`7.6%` faster (`32.62 ns -> 30.50 ns`)
  - `Node48 -> Node256`: ~`30.0%` faster (`154.62 ns -> 118.81 ns`)

## [0.4.0] - 2026-04-21

### Added

- Prefix-oriented APIs on `AdaptiveRadixTree`:
  - `longest_prefix_match` / `longest_prefix_match_k`
  - `prefix_iter` / `prefix_iter_k`
- ART-native tree intersection/join APIs on `AdaptiveRadixTree`:
  - `intersect_with`
  - `intersect_values_with`
  - `intersect_count`
- Prefix and intersection benchmark coverage:
  - `rart/benches/prefix_bench.rs`
  - `rart/benches/intersection_join_bench.rs`
- Internal microbench coverage for node mappings and versioned-tree internals using `micromeasure`.
- Density-sweep microbenches for `Node48`/`Node256` child lookup, miss lookup, insertion, and
  deletion behavior.
- Regression tests for sparse-key iteration order in `DirectMapping` and `IndexedMapping`.
- Property-style coverage for prefix-key edge cases and related regressions.

### Changed

- Optimized Node48/Node256 child iteration to traverse only occupied slots while preserving sorted
  key order.
  - Added a set-bit iterator in `utils/bitset.rs` and switched `DirectMapping`/`IndexedMapping`
    iterators to use it.
  - Removed linear `0..255` probing from these hot iteration paths.
- Added a quick/default benchmark profile with optional full-mode runs via `RART_BENCH_FULL=1`.
- Raised the workspace MSRV to Rust `1.86`.
- Switched internal microbench targets from Criterion to `micromeasure` while keeping higher-level
  workload benchmarks on Criterion.
- Boxed node-content variants in both `Content` and `VersionedContent` so node headers are no longer
  sized by the largest inline mapping variant.

### Fixed

- Corrected handling of keys that are prefixes of other keys in both `AdaptiveRadixTree` and
  `VersionedAdaptiveRadixTree`.
  - Insert/remove paths now preserve values stored on inner nodes.
  - Prefix lookups and subtree iteration now correctly return entries rooted at those inner nodes.
- Fixed `values_iter()` so it yields a value stored on the root node even when the root also has
  children.
- Fixed nested snapshot reference tracking in the multithreaded versioned-tree fuzz target.
- Tightened `IndexedMapping` and `DirectMapping` hot-path slot access to avoid redundant checks once
  slot presence is already proven.
- Corrected `BitArray::first_empty()` range handling for partially-used backing storage.

### Performance

- `values_iteration_numeric/art_values` improved in local Criterion runs after sparse iteration
  changes:
  - 256: ~2.02 ns/elem
  - 1024: ~2.02 ns/elem
  - 4096: ~1.97 ns/elem
  - 32768: ~2.09 ns/elem
- On 32768 elements, `values_iter()` is ~4x faster than ART full iteration (~2.09 vs ~8.25 ns/elem).
- Reduced `DefaultNode`/`VersionedNode` header size by storing node-kind payloads behind boxes.
- Improved `IndexedMapping` hit lookup and add-child hot paths in local node-mapping density
  benchmarks.

## [0.3.1] - 2026-02-06

### Fixed

- **Critical**: Fixed a signed vs unsigned comparison bug in `SortedKeyedMapping` (Node4/Node16)
  SIMD implementation.
  - Keys with the high bit set (e.g., `>= 128`) were incorrectly treated as negative integers during
    insertion search, breaking the sorted order of children.
  - This caused iteration and range queries to return results out of lexicographical order or
    terminate early.
  - Fixed by flipping the sign bit before SIMD comparison to enforce unsigned ordering.
- Restored and validated O(log N) range iteration optimizations (stopping immediately at end bound,
  skipping redundant start bound checks) which rely on correct sorted order.
- Added regression tests for `RangeToInclusive` and `RangeFrom` edge cases discovered via fuzzing.

## [0.3.0] - 2026-02-06

### Added

- Added regression test for start-bound range parity with `BTreeMap`:
  - `test_range_start_sequence_matches_btreemap_seeded`
  - Uses a fixed RNG seed and verifies `range(start..)` sequence equality.
- Added start-seek benchmark coverage in `rart/benches/iter_bench.rs`:
  - `start_seek_positioning/art_unbounded_first/*`
  - `start_seek_positioning/art_start_mid_first/*`
  - `start_seek_positioning/art_start_high_first/*`

### Changed

- Iterator internals were refactored to remove dynamic iterator dispatch in traversal:
  - Replaced boxed trait-object iterator stack usage with concrete iterator types and enum dispatch.
  - Updated mapping/node iterator plumbing accordingly.
- Start-bound iteration filtering now disables itself after the first satisfying key (ordered
  traversal guarantee), reducing repeated bound checks in range scans.

### Fixed

- Fixed range correctness around start/end bounds and start positioning:
  - End-bound termination behavior.
  - Start-seek positioning behavior.
  - Added/updated regression tests for both.
- Stabilized panic-based range regression tests under parallel test execution via test
  synchronization.

### Performance

- `range_iteration/art_range` improved significantly in targeted runs after iterator/range work.

## [0.2.1]

### Summary

- Current published release line for the `rart` workspace/crate version.
