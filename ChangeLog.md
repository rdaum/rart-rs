# ChangeLog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed

- Optimized Node48/Node256 child iteration to traverse only occupied slots while preserving sorted key order.
  - Added a set-bit iterator in `utils/bitset.rs` and switched `DirectMapping`/`IndexedMapping` iterators to use it.
  - Removed linear `0..255` probing from these hot iteration paths.

### Added

- Regression tests to verify sparse-key iteration order is preserved:
  - `mapping::direct_mapping::tests::iter_preserves_key_order_for_sparse_children`
  - `mapping::indexed_mapping::test::iter_preserves_key_order_for_sparse_children`

### Performance

- `values_iteration_numeric/art_values` improved in local Criterion runs after sparse iteration changes:
  - 256: ~2.02 ns/elem
  - 1024: ~2.02 ns/elem
  - 4096: ~1.97 ns/elem
  - 32768: ~2.09 ns/elem
- On 32768 elements, `values_iter()` is ~4x faster than ART full iteration (~2.09 vs ~8.25 ns/elem).

## [0.3.1] - 2026-02-06

### Fixed

- **Critical**: Fixed a signed vs unsigned comparison bug in `SortedKeyedMapping` (Node4/Node16) SIMD implementation.
  - Keys with the high bit set (e.g., `>= 128`) were incorrectly treated as negative integers during insertion search, breaking the sorted order of children.
  - This caused iteration and range queries to return results out of lexicographical order or terminate early.
  - Fixed by flipping the sign bit before SIMD comparison to enforce unsigned ordering.
- Restored and validated O(log N) range iteration optimizations (stopping immediately at end bound, skipping redundant start bound checks) which rely on correct sorted order.
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
