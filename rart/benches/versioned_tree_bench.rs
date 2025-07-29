/// Comprehensive benchmarks comparing VersionedAdaptiveRadixTree against persistent data structures
/// from the `im` crate (im::HashMap and im::OrdMap) for MVCC-style workloads.
use std::time::Instant;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::{Rng, rng};

use im::{HashMap as ImHashMap, OrdMap as ImOrdMap};
use rart::VersionedAdaptiveRadixTree;
use rart::keys::array_key::ArrayKey;

const TREE_SIZES: [usize; 4] = [1 << 8, 1 << 10, 1 << 12, 1 << 14];
const SNAPSHOT_COUNTS: [usize; 3] = [1, 5, 10];

/// Benchmark lookup operations
pub fn lookup_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_comparison");
    group.throughput(Throughput::Elements(1));

    for size in TREE_SIZES {
        // Pre-populate data structures
        let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        let mut im_hashmap = ImHashMap::new();
        let mut im_ordmap = ImOrdMap::new();

        for i in 0..size {
            versioned_tree.insert(i, i);
            im_hashmap = im_hashmap.update(i, i);
            im_ordmap = im_ordmap.update(i, i);
        }

        group.bench_with_input(
            BenchmarkId::new("versioned_art", size),
            &size,
            |b, &size| {
                let mut rng = rng();
                b.iter(|| {
                    let key = rng.random_range(0..size);
                    std::hint::black_box(versioned_tree.get(key));
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("im_hashmap", size), &size, |b, &size| {
            let mut rng = rng();
            b.iter(|| {
                let key = rng.random_range(0..size);
                std::hint::black_box(im_hashmap.get(&key));
            })
        });

        group.bench_with_input(BenchmarkId::new("im_ordmap", size), &size, |b, &size| {
            let mut rng = rng();
            b.iter(|| {
                let key = rng.random_range(0..size);
                std::hint::black_box(im_ordmap.get(&key));
            })
        });
    }

    group.finish();
}

/// Benchmark snapshot creation - the key feature for MVCC
pub fn snapshot_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_creation");
    group.throughput(Throughput::Elements(1));

    for size in TREE_SIZES {
        // Pre-populate with data
        let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        let mut im_hashmap = ImHashMap::new();
        let mut im_ordmap = ImOrdMap::new();

        for i in 0..size {
            versioned_tree.insert(i, i);
            im_hashmap = im_hashmap.update(i, i);
            im_ordmap = im_ordmap.update(i, i);
        }

        group.bench_with_input(
            BenchmarkId::new("versioned_art_snapshot", size),
            &size,
            |b, _size| {
                b.iter(|| {
                    std::hint::black_box(versioned_tree.snapshot());
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_hashmap_clone", size),
            &size,
            |b, _size| {
                b.iter(|| {
                    std::hint::black_box(im_hashmap.clone());
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_ordmap_clone", size),
            &size,
            |b, _size| {
                b.iter(|| {
                    std::hint::black_box(im_ordmap.clone());
                })
            },
        );
    }

    group.finish();
}

/// Benchmark sequential scanning where versioned ART should excel due to cache locality
pub fn sequential_scan_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_scan");
    group.throughput(Throughput::Elements(1));

    for size in TREE_SIZES {
        // Pre-populate data structures with sequential keys
        let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        let mut im_hashmap = ImHashMap::new();
        let mut im_ordmap = ImOrdMap::new();

        for i in 0..size {
            versioned_tree.insert(i, i);
            im_hashmap = im_hashmap.update(i, i);
            im_ordmap = im_ordmap.update(i, i);
        }

        group.bench_with_input(
            BenchmarkId::new("versioned_art", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    // Sequential scan through all keys
                    for i in 0..size {
                        std::hint::black_box(versioned_tree.get(i));
                    }
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("im_hashmap", size), &size, |b, &size| {
            b.iter(|| {
                // Sequential scan through all keys
                for i in 0..size {
                    std::hint::black_box(im_hashmap.get(&i));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("im_ordmap", size), &size, |b, &size| {
            b.iter(|| {
                // Sequential scan through all keys
                for i in 0..size {
                    std::hint::black_box(im_ordmap.get(&i));
                }
            })
        });
    }

    group.finish();
}

/// Benchmark the key advantage: multiple mutations per snapshot
/// This shows where versioned ART should excel vs im types that copy on every mutation
pub fn mutations_per_snapshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("mutations_per_snapshot");
    group.throughput(Throughput::Elements(1));

    let base_size = 1000;
    let mutations_per_snapshot = [10, 50, 100, 200];

    for mutation_count in mutations_per_snapshot {
        group.bench_with_input(
            BenchmarkId::new("versioned_art", mutation_count),
            &mutation_count,
            |b, &mutation_count| {
                b.iter_custom(|iters| {
                    let mut base_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
                    // Pre-populate base data
                    for i in 0..base_size {
                        base_tree.insert(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Take ONE snapshot, then do many mutations
                        let mut snapshot = base_tree.snapshot(); // O(1) 
                        for j in 0..mutation_count {
                            let key = base_size + j;
                            snapshot.insert(key, key); // Only CoW on modified paths
                        }
                        std::hint::black_box(snapshot);
                    }
                    start.elapsed()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_hashmap", mutation_count),
            &mutation_count,
            |b, &mutation_count| {
                b.iter_custom(|iters| {
                    let mut base_map = ImHashMap::new();
                    // Pre-populate base data
                    for i in 0..base_size {
                        base_map = base_map.update(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Clone once, then do many mutations (each creates a new copy)
                        let mut map_copy = base_map.clone(); // Full structural copy
                        for j in 0..mutation_count {
                            let key = base_size + j;
                            map_copy = map_copy.update(key, key); // Full copy every time!
                        }
                        std::hint::black_box(map_copy);
                    }
                    start.elapsed()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_ordmap", mutation_count),
            &mutation_count,
            |b, &mutation_count| {
                b.iter_custom(|iters| {
                    let mut base_map = ImOrdMap::new();
                    // Pre-populate base data
                    for i in 0..base_size {
                        base_map = base_map.update(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Clone once, then do many mutations
                        let mut map_copy = base_map.clone();
                        for j in 0..mutation_count {
                            let key = base_size + j;
                            map_copy = map_copy.update(key, key);
                        }
                        std::hint::black_box(map_copy);
                    }
                    start.elapsed()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark snapshot reuse with structural sharing
/// This tests the scenario where many snapshots share most structure but diverge slightly
/// Versioned ART should excel due to structural sharing, while im types make full copies
pub fn snapshot_structural_sharing(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_structural_sharing");
    group.throughput(Throughput::Elements(1));

    let base_size = 2000;
    let snapshot_counts = [5, 10, 20];
    let mutations_per_snapshot = 5; // Small mutations to maximize sharing benefit

    for snapshot_count in snapshot_counts {
        group.bench_with_input(
            BenchmarkId::new("versioned_art", snapshot_count),
            &snapshot_count,
            |b, &snapshot_count| {
                b.iter_custom(|iters| {
                    let mut base_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
                    // Create substantial base structure
                    for i in 0..base_size {
                        base_tree.insert(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Create many snapshots from same base (O(1) each)
                        let mut snapshots = Vec::new();
                        for _ in 0..snapshot_count {
                            snapshots.push(base_tree.snapshot()); // All share structure
                        }

                        // Each snapshot gets unique small modifications
                        for (snap_idx, snapshot) in snapshots.iter_mut().enumerate() {
                            for mut_idx in 0..mutations_per_snapshot {
                                let key = base_size + snap_idx * mutations_per_snapshot + mut_idx;
                                snapshot.insert(key, key); // Minimal CoW, maximum sharing
                            }
                        }

                        // Do some lookups to test that sharing still works
                        let mut rng = rng();
                        for snapshot in &snapshots {
                            for _ in 0..10 {
                                let key = rng.random_range(0..base_size);
                                std::hint::black_box(snapshot.get(key));
                            }
                        }

                        std::hint::black_box(snapshots);
                    }
                    start.elapsed()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_hashmap", snapshot_count),
            &snapshot_count,
            |b, &snapshot_count| {
                b.iter_custom(|iters| {
                    let mut base_map = ImHashMap::new();
                    // Create substantial base structure
                    for i in 0..base_size {
                        base_map = base_map.update(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Each clone makes a full copy
                        let mut maps = Vec::new();
                        for _ in 0..snapshot_count {
                            maps.push(base_map.clone()); // Full copy each time
                        }

                        // Each map gets unique modifications
                        for (map_idx, map) in maps.iter_mut().enumerate() {
                            for mut_idx in 0..mutations_per_snapshot {
                                let key = base_size + map_idx * mutations_per_snapshot + mut_idx;
                                *map = map.update(key, key); // More full copies
                            }
                        }

                        // Do some lookups
                        let mut rng = rng();
                        for map in &maps {
                            for _ in 0..10 {
                                let key = rng.random_range(0..base_size);
                                std::hint::black_box(map.get(&key));
                            }
                        }

                        std::hint::black_box(maps);
                    }
                    start.elapsed()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("im_ordmap", snapshot_count),
            &snapshot_count,
            |b, &snapshot_count| {
                b.iter_custom(|iters| {
                    let mut base_map = ImOrdMap::new();
                    // Create substantial base structure
                    for i in 0..base_size {
                        base_map = base_map.update(i, i);
                    }

                    let start = Instant::now();
                    for _iter in 0..iters {
                        // Each clone makes a full copy
                        let mut maps = Vec::new();
                        for _ in 0..snapshot_count {
                            maps.push(base_map.clone()); // Full copy each time
                        }

                        // Each map gets unique modifications
                        for (map_idx, map) in maps.iter_mut().enumerate() {
                            for mut_idx in 0..mutations_per_snapshot {
                                let key = base_size + map_idx * mutations_per_snapshot + mut_idx;
                                *map = map.update(key, key);
                            }
                        }

                        // Do some lookups
                        let mut rng = rng();
                        for map in &maps {
                            for _ in 0..10 {
                                let key = rng.random_range(0..base_size);
                                std::hint::black_box(map.get(&key));
                            }
                        }

                        std::hint::black_box(maps);
                    }
                    start.elapsed()
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    versioned_benches,
    lookup_comparison,
    snapshot_creation,
    sequential_scan_comparison,
    mutations_per_snapshot,
    snapshot_structural_sharing
);
criterion_main!(versioned_benches);
