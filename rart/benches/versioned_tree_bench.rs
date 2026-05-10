/// Comprehensive benchmarks comparing VersionedAdaptiveRadixTree against persistent data structures
/// from the `im` crate (imbl::HashMap and imbl::OrdMap) for MVCC-style workloads.
use std::cmp::Ordering;
use std::time::{Duration, Instant};

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::{Rng, rng};

use imbl::{HashMap as ImHashMap, OrdMap as ImOrdMap};
use rart::keys::array_key::ArrayKey;
use rart::{KeyTrait, OverflowKey, VersionedAdaptiveRadixTree};

const TREE_SIZES: [usize; 4] = [1 << 8, 1 << 10, 1 << 12, 1 << 14];
const DENSE_SEQUENTIAL_QUICK_SIZES: [usize; 5] = [16, 48, 64, 256, 4096];
const DENSE_SEQUENTIAL_FULL_SIZES: [usize; 6] = [16, 48, 64, 256, 4096, 65536];
const ITERATION_SIZES: [usize; 3] = [1 << 10, 1 << 12, 1 << 14];
const JOIN_SIZES: [usize; 2] = [10_000, 100_000];
const JOIN_OVERLAPS: [f64; 3] = [0.1, 0.5, 0.9];
const PREFIX_OBJECTS: usize = 1024;
const PREFIX_SYMBOLS_PER_OBJECT: usize = 64;

type DenseSequentialKey = OverflowKey<8, 4>;
type DenseSequentialTree = VersionedAdaptiveRadixTree<DenseSequentialKey, usize>;
type CacheKey = [u8; 12];

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn criterion_config() -> Criterion {
    if full_bench_profile() {
        Criterion::default()
    } else {
        Criterion::default()
            .sample_size(30)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(2))
    }
}

fn dense_sequential_sizes() -> &'static [usize] {
    if full_bench_profile() {
        &DENSE_SEQUENTIAL_FULL_SIZES
    } else {
        &DENSE_SEQUENTIAL_QUICK_SIZES
    }
}

fn dense_sequential_key(i: u32) -> DenseSequentialKey {
    let mut bytes = [0; 5];
    bytes[0] = 8;
    bytes[1..].copy_from_slice(&i.to_be_bytes());
    DenseSequentialKey::new_from_slice(&bytes)
}

fn dense_sequential_keys(size: usize) -> Vec<DenseSequentialKey> {
    (0..size).map(|i| dense_sequential_key(i as u32)).collect()
}

fn build_dense_sequential_tree(keys: &[DenseSequentialKey]) -> DenseSequentialTree {
    let mut tree = DenseSequentialTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert_k(key, i);
    }
    tree
}

fn cache_key_bytes(obj: u64, symbol: u32) -> CacheKey {
    let mut bytes = [0; 12];
    bytes[..8].copy_from_slice(&obj.to_be_bytes());
    bytes[8..].copy_from_slice(&symbol.to_be_bytes());
    bytes
}

fn cache_key(obj: u64, symbol: u32) -> ArrayKey<16> {
    ArrayKey::new_from_slice(&cache_key_bytes(obj, symbol))
}

fn cache_prefix(obj: u64) -> ArrayKey<16> {
    ArrayKey::new_from_slice(&obj.to_be_bytes())
}

fn build_iteration_inputs(
    size: usize,
) -> (
    VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    ImHashMap<usize, usize>,
    ImOrdMap<usize, usize>,
) {
    let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut im_hashmap = ImHashMap::new();
    let mut im_ordmap = ImOrdMap::new();

    for i in 0..size {
        versioned_tree.insert(i, i);
        im_hashmap = im_hashmap.update(i, i);
        im_ordmap = im_ordmap.update(i, i);
    }

    (versioned_tree, im_hashmap, im_ordmap)
}

fn build_prefix_inputs() -> (
    VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    ImHashMap<CacheKey, usize>,
    ImOrdMap<CacheKey, usize>,
) {
    let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut im_hashmap = ImHashMap::new();
    let mut im_ordmap = ImOrdMap::new();

    for obj in 0..PREFIX_OBJECTS {
        for symbol in 0..PREFIX_SYMBOLS_PER_OBJECT {
            let key = cache_key_bytes(obj as u64, symbol as u32);
            let value = obj * PREFIX_SYMBOLS_PER_OBJECT + symbol;
            versioned_tree.insert_k(&ArrayKey::new_from_slice(&key), value);
            im_hashmap = im_hashmap.update(key, value);
            im_ordmap = im_ordmap.update(key, value);
        }
    }

    (versioned_tree, im_hashmap, im_ordmap)
}

fn prefix_range(obj: u64) -> (CacheKey, CacheKey) {
    (cache_key_bytes(obj, 0), cache_key_bytes(obj + 1, 0))
}

fn generate_overlapping_keys(size: usize, overlap_ratio: f64) -> (Vec<u64>, Vec<u64>) {
    let overlap = ((size as f64) * overlap_ratio) as usize;
    let unique = size - overlap;

    let mut left = Vec::with_capacity(size);
    let mut right = Vec::with_capacity(size);

    for i in 0..overlap as u64 {
        left.push(i);
        right.push(i);
    }

    for i in 0..unique as u64 {
        left.push(1_000_000_000 + i);
        right.push(2_000_000_000 + i);
    }

    (left, right)
}

fn build_join_tree(keys: &[u64]) -> VersionedAdaptiveRadixTree<ArrayKey<16>, usize> {
    let mut tree = VersionedAdaptiveRadixTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert(*key, i);
    }
    tree
}

fn build_join_hashmap(keys: &[u64]) -> ImHashMap<u64, usize> {
    let mut map = ImHashMap::new();
    for (i, key) in keys.iter().enumerate() {
        map = map.update(*key, i);
    }
    map
}

fn build_join_ordmap(keys: &[u64]) -> ImOrdMap<u64, usize> {
    let mut map = ImOrdMap::new();
    for (i, key) in keys.iter().enumerate() {
        map = map.update(*key, i);
    }
    map
}

fn versioned_art_join_checksum(
    left: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    right: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
) -> usize {
    let mut checksum = 0usize;
    left.intersect_with(right, |key, left_value, right_value| {
        checksum = checksum.wrapping_add(key.as_ref().len());
        checksum = checksum.wrapping_add(*left_value);
        checksum = checksum.wrapping_add(*right_value);
    });
    checksum
}

fn versioned_art_lending_join_checksum(
    left: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    right: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
) -> usize {
    let mut checksum = 0usize;
    left.intersect_lending_with(right, |key, left_value, right_value| {
        checksum = checksum.wrapping_add(key.len());
        checksum = checksum.wrapping_add(*left_value);
        checksum = checksum.wrapping_add(*right_value);
    });
    checksum
}

fn versioned_art_values_join_checksum(
    left: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    right: &VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
) -> usize {
    let mut checksum = 0usize;
    left.intersect_values_with(right, |left_value, right_value| {
        checksum = checksum.wrapping_add(*left_value);
        checksum = checksum.wrapping_add(*right_value);
    });
    checksum
}

fn im_ordmap_merge_join_checksum(
    left: &ImOrdMap<u64, usize>,
    right: &ImOrdMap<u64, usize>,
) -> usize {
    let mut left_it = left.iter().peekable();
    let mut right_it = right.iter().peekable();
    let mut checksum = 0usize;

    loop {
        let (Some((left_key, left_value)), Some((right_key, right_value))) =
            (left_it.peek().copied(), right_it.peek().copied())
        else {
            return checksum;
        };

        match left_key.cmp(right_key) {
            Ordering::Less => {
                let _ = left_it.next();
            }
            Ordering::Greater => {
                let _ = right_it.next();
            }
            Ordering::Equal => {
                checksum = checksum.wrapping_add(std::mem::size_of::<u64>());
                checksum = checksum.wrapping_add(*left_value);
                checksum = checksum.wrapping_add(*right_value);
                let _ = left_it.next();
                let _ = right_it.next();
            }
        }
    }
}

fn im_ordmap_merge_join_values_checksum(
    left: &ImOrdMap<u64, usize>,
    right: &ImOrdMap<u64, usize>,
) -> usize {
    let mut left_it = left.iter().peekable();
    let mut right_it = right.iter().peekable();
    let mut checksum = 0usize;

    loop {
        let (Some((left_key, left_value)), Some((right_key, right_value))) =
            (left_it.peek().copied(), right_it.peek().copied())
        else {
            return checksum;
        };

        match left_key.cmp(right_key) {
            Ordering::Less => {
                let _ = left_it.next();
            }
            Ordering::Greater => {
                let _ = right_it.next();
            }
            Ordering::Equal => {
                checksum = checksum.wrapping_add(*left_value);
                checksum = checksum.wrapping_add(*right_value);
                let _ = left_it.next();
                let _ = right_it.next();
            }
        }
    }
}

fn im_ordmap_merge_join_count(left: &ImOrdMap<u64, usize>, right: &ImOrdMap<u64, usize>) -> usize {
    let mut left_it = left.iter().peekable();
    let mut right_it = right.iter().peekable();
    let mut count = 0usize;

    loop {
        let (Some((left_key, _)), Some((right_key, _))) =
            (left_it.peek().copied(), right_it.peek().copied())
        else {
            return count;
        };

        match left_key.cmp(right_key) {
            Ordering::Less => {
                let _ = left_it.next();
            }
            Ordering::Greater => {
                let _ = right_it.next();
            }
            Ordering::Equal => {
                count += 1;
                let _ = left_it.next();
                let _ = right_it.next();
            }
        }
    }
}

fn im_hashmap_probe_join_checksum(
    left: &ImHashMap<u64, usize>,
    right: &ImHashMap<u64, usize>,
) -> usize {
    let (probe, lookup) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut checksum = 0usize;

    for (key, probe_value) in probe.iter() {
        if let Some(lookup_value) = lookup.get(key) {
            checksum = checksum.wrapping_add(std::mem::size_of::<u64>());
            checksum = checksum.wrapping_add(*probe_value);
            checksum = checksum.wrapping_add(*lookup_value);
        }
    }

    checksum
}

fn im_hashmap_probe_join_values_checksum(
    left: &ImHashMap<u64, usize>,
    right: &ImHashMap<u64, usize>,
) -> usize {
    let (probe, lookup) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut checksum = 0usize;

    for (key, probe_value) in probe.iter() {
        if let Some(lookup_value) = lookup.get(key) {
            checksum = checksum.wrapping_add(*probe_value);
            checksum = checksum.wrapping_add(*lookup_value);
        }
    }

    checksum
}

fn im_hashmap_probe_join_count(
    left: &ImHashMap<u64, usize>,
    right: &ImHashMap<u64, usize>,
) -> usize {
    let (probe, lookup) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut count = 0usize;

    for key in probe.keys() {
        if lookup.contains_key(key) {
            count += 1;
        }
    }

    count
}

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

/// Benchmark full persistent-container iteration.
///
/// The owned ART iterator reconstructs keys. The lending and values-only variants represent the
/// fairer comparison when callers can consume borrowed key views or only need values.
pub fn full_iteration_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_iteration");

    for size in ITERATION_SIZES {
        let (versioned_tree, im_hashmap, im_ordmap) = build_iteration_inputs(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("versioned_art_owned_iter", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut sum = 0usize;
                    for (key, value) in versioned_tree.iter() {
                        sum = sum.wrapping_add(key.as_ref().len());
                        sum = sum.wrapping_add(*value);
                    }
                    std::hint::black_box(sum);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("versioned_art_lending_view", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut sum = 0usize;
                    versioned_tree.for_each_view(|key, value| {
                        sum = sum.wrapping_add(key.len());
                        sum = sum.wrapping_add(*value);
                    });
                    std::hint::black_box(sum);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("versioned_art_values_iter", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut sum = 0usize;
                    for value in versioned_tree.values_iter() {
                        sum = sum.wrapping_add(*value);
                    }
                    std::hint::black_box(sum);
                })
            },
        );

        group.bench_with_input(BenchmarkId::new("im_hashmap_iter", size), &size, |b, _| {
            b.iter(|| {
                let mut sum = 0usize;
                for (key, value) in im_hashmap.iter() {
                    sum = sum.wrapping_add(*key);
                    sum = sum.wrapping_add(*value);
                }
                std::hint::black_box(sum);
            })
        });

        group.bench_with_input(BenchmarkId::new("im_ordmap_iter", size), &size, |b, _| {
            b.iter(|| {
                let mut sum = 0usize;
                for (key, value) in im_ordmap.iter() {
                    sum = sum.wrapping_add(*key);
                    sum = sum.wrapping_add(*value);
                }
                std::hint::black_box(sum);
            })
        });
    }

    group.finish();
}

/// Benchmark persistent-container join/intersection shapes over the same overlapping key sets.
pub fn join_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("join_comparison");

    for size in JOIN_SIZES {
        for overlap in JOIN_OVERLAPS {
            let (left_keys, right_keys) = generate_overlapping_keys(size, overlap);
            let left_tree = build_join_tree(&left_keys);
            let right_tree = build_join_tree(&right_keys);
            let left_hashmap = build_join_hashmap(&left_keys);
            let right_hashmap = build_join_hashmap(&right_keys);
            let left_ordmap = build_join_ordmap(&left_keys);
            let right_ordmap = build_join_ordmap(&right_keys);
            let case = format!("n{}_o{}", size, (overlap * 100.0) as u32);

            group.throughput(Throughput::Elements(size as u64));

            group.bench_with_input(
                BenchmarkId::new("versioned_art_intersect_with", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = versioned_art_join_checksum(
                            std::hint::black_box(&left_tree),
                            std::hint::black_box(&right_tree),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("versioned_art_intersect_lending", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = versioned_art_lending_join_checksum(
                            std::hint::black_box(&left_tree),
                            std::hint::black_box(&right_tree),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_ordmap_merge_join", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = im_ordmap_merge_join_checksum(
                            std::hint::black_box(&left_ordmap),
                            std::hint::black_box(&right_ordmap),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_hashmap_probe_join", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = im_hashmap_probe_join_checksum(
                            std::hint::black_box(&left_hashmap),
                            std::hint::black_box(&right_hashmap),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("versioned_art_intersect_values", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = versioned_art_values_join_checksum(
                            std::hint::black_box(&left_tree),
                            std::hint::black_box(&right_tree),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_ordmap_merge_join_values", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = im_ordmap_merge_join_values_checksum(
                            std::hint::black_box(&left_ordmap),
                            std::hint::black_box(&right_ordmap),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_hashmap_probe_join_values", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let checksum = im_hashmap_probe_join_values_checksum(
                            std::hint::black_box(&left_hashmap),
                            std::hint::black_box(&right_hashmap),
                        );
                        std::hint::black_box(checksum);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("versioned_art_intersect_count", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count = std::hint::black_box(&left_tree)
                            .intersect_count(std::hint::black_box(&right_tree));
                        std::hint::black_box(count);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_ordmap_merge_join_count", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count = im_ordmap_merge_join_count(
                            std::hint::black_box(&left_ordmap),
                            std::hint::black_box(&right_ordmap),
                        );
                        std::hint::black_box(count);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("im_hashmap_probe_join_count", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count = im_hashmap_probe_join_count(
                            std::hint::black_box(&left_hashmap),
                            std::hint::black_box(&right_hashmap),
                        );
                        std::hint::black_box(count);
                    })
                },
            );
        }
    }

    group.finish();
}

/// Benchmark object-prefix invalidation over cache-shaped keys:
/// [obj: u64 big-endian][symbol: u32 big-endian].
pub fn prefix_invalidation_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("prefix_invalidation");
    let (versioned_tree, im_hashmap, im_ordmap) = build_prefix_inputs();
    let target_obj = (PREFIX_OBJECTS / 2) as u64;
    let prefix = cache_prefix(target_obj);
    let (range_start, range_end) = prefix_range(target_obj);

    group.throughput(Throughput::Elements(PREFIX_SYMBOLS_PER_OBJECT as u64));

    group.bench_function("versioned_art_prefix_lending", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            versioned_tree.prefix_for_each_view_k(&prefix, |key, value| {
                sum = sum.wrapping_add(key.len());
                sum = sum.wrapping_add(*value);
            });
            std::hint::black_box(sum);
        })
    });

    group.bench_function("versioned_art_prefix_owned_iter", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for (key, value) in versioned_tree.prefix_iter_k(&prefix) {
                sum = sum.wrapping_add(key.as_ref().len());
                sum = sum.wrapping_add(*value);
            }
            std::hint::black_box(sum);
        })
    });

    group.bench_function("im_ordmap_range", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for (key, value) in im_ordmap.range(range_start..range_end) {
                sum = sum.wrapping_add(key.len());
                sum = sum.wrapping_add(*value);
            }
            std::hint::black_box(sum);
        })
    });

    group.bench_function("im_hashmap_full_scan", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for (key, value) in im_hashmap.iter() {
                if key[..8] == target_obj.to_be_bytes() {
                    sum = sum.wrapping_add(key.len());
                    sum = sum.wrapping_add(*value);
                }
            }
            std::hint::black_box(sum);
        })
    });

    group.bench_function("point_lookup_expected_prefix_entries", |b| {
        b.iter(|| {
            let mut sum = 0usize;
            for symbol in 0..PREFIX_SYMBOLS_PER_OBJECT {
                let key = cache_key(target_obj, symbol as u32);
                if let Some(value) = versioned_tree.get_k(&key) {
                    sum = sum.wrapping_add(*value);
                }
            }
            std::hint::black_box(sum);
        })
    });

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

/// Benchmark dense sequential byte keys with a fixed tag followed by a big-endian integer.
pub fn dense_sequential_key_mutation(c: &mut Criterion) {
    let mut group = c.benchmark_group("dense_sequential_key_mutation");

    for &size in dense_sequential_sizes() {
        group.throughput(Throughput::Elements(size as u64));
        let keys = dense_sequential_keys(size);
        let scratch_keys = dense_sequential_keys(size.saturating_add(1024));

        group.bench_with_input(
            BenchmarkId::new("replace_existing_owned", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || build_dense_sequential_tree(&keys),
                    |mut tree| {
                        for (i, key) in keys.iter().enumerate() {
                            std::hint::black_box(tree.insert_k(key, i.wrapping_add(1)));
                        }
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("replace_existing_with_snapshot", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || {
                        let tree = build_dense_sequential_tree(&keys);
                        let snapshot = tree.snapshot();
                        (tree, snapshot)
                    },
                    |(mut tree, snapshot)| {
                        for (i, key) in keys.iter().enumerate() {
                            std::hint::black_box(tree.insert_k(key, i.wrapping_add(1)));
                        }
                        std::hint::black_box(snapshot);
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("insert_remove_owned", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || build_dense_sequential_tree(&keys),
                    |mut tree| {
                        for i in 0..size {
                            let key = &scratch_keys[size + (i % 1024)];
                            std::hint::black_box(tree.insert_k(key, i));
                            std::hint::black_box(tree.remove_k(key));
                        }
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("insert_remove_with_snapshot", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || {
                        let tree = build_dense_sequential_tree(&keys);
                        let snapshot = tree.snapshot();
                        (tree, snapshot)
                    },
                    |(mut tree, snapshot)| {
                        for i in 0..size {
                            let key = &scratch_keys[size + (i % 1024)];
                            std::hint::black_box(tree.insert_k(key, i));
                            std::hint::black_box(tree.remove_k(key));
                        }
                        std::hint::black_box(snapshot);
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("remove_reinsert_owned", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || build_dense_sequential_tree(&keys),
                    |mut tree| {
                        for (i, key) in keys.iter().enumerate() {
                            std::hint::black_box(tree.remove_k(key));
                            std::hint::black_box(tree.insert_k(key, i));
                        }
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("remove_reinsert_with_snapshot", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || {
                        let tree = build_dense_sequential_tree(&keys);
                        let snapshot = tree.snapshot();
                        (tree, snapshot)
                    },
                    |(mut tree, snapshot)| {
                        for (i, key) in keys.iter().enumerate() {
                            std::hint::black_box(tree.remove_k(key));
                            std::hint::black_box(tree.insert_k(key, i));
                        }
                        std::hint::black_box(snapshot);
                        std::hint::black_box(tree);
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = versioned_benches;
    config = criterion_config();
    targets = lookup_comparison,
        snapshot_creation,
        sequential_scan_comparison,
        full_iteration_comparison,
        join_comparison,
        prefix_invalidation_comparison,
        mutations_per_snapshot,
        snapshot_structural_sharing,
        dense_sequential_key_mutation
);
criterion_main!(versioned_benches);
