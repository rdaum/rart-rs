# ChangeLog

All notable changes to this project are documented in this file.

## [Unreleased] - 2026-02-06

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
- Start-bound iteration filtering now disables itself after the first
  satisfying key (ordered traversal guarantee), reducing repeated
  bound checks in range scans.

### Fixed
- Fixed range correctness around start/end bounds and start positioning:
  - End-bound termination behavior.
  - Start-seek positioning behavior.
  - Added/updated regression tests for both.
- Stabilized panic-based range regression tests under parallel test
  execution via test synchronization.

### Performance
- `range_iteration/art_range` improved significantly in targeted runs after iterator/range work.

## [0.2.1]

### Summary
- Current published release line for the `rart` workspace/crate version.
