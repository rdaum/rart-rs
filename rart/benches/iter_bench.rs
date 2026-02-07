//! Iteration performance benchmarks for RART vs HashMap/BTreeMap.
//! These benchmarks compare full tree traversal and range iteration performance.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use blart::TreeMap;
use rart::keys::array_key::ArrayKey;
use rart::tree::AdaptiveRadixTree;

// Test different tree sizes to see how iteration scales
const TREE_SIZES: [u64; 4] = [1 << 8, 1 << 10, 1 << 12, 1 << 15];

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

/// Full tree iteration - consume all key-value pairs
pub fn full_iteration_numeric(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_iteration_numeric");

    for size in TREE_SIZES {
        group.throughput(Throughput::Elements(size));

        // ART iteration
        group.bench_with_input(BenchmarkId::new("art", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
            for i in 0..*size {
                tree.insert(i, i * 2);
            }

            b.iter(|| {
                let sum: u64 = tree.iter().map(|(_, v)| *v).sum();
                std::hint::black_box(sum);
            })
        });

        // HashMap iteration
        group.bench_with_input(BenchmarkId::new("hashmap", size), &size, |b, size| {
            let mut map = HashMap::new();
            for i in 0..*size {
                map.insert(i, i * 2);
            }

            b.iter(|| {
                let sum: u64 = map.values().copied().sum();
                std::hint::black_box(sum);
            })
        });

        // BTreeMap iteration
        group.bench_with_input(BenchmarkId::new("btreemap", size), &size, |b, size| {
            let mut map = BTreeMap::new();
            for i in 0..*size {
                map.insert(i, i * 2);
            }

            b.iter(|| {
                let sum: u64 = map.values().copied().sum();
                std::hint::black_box(sum);
            })
        });

        // BLART iteration
        group.bench_with_input(BenchmarkId::new("blart", size), &size, |b, size| {
            let mut tree = TreeMap::<Box<[u8]>, u64>::new();
            for i in 0..*size {
                tree.try_insert(i.to_be_bytes().into(), i * 2).unwrap();
            }

            b.iter(|| {
                let sum: u64 = tree.iter().map(|(_, v)| *v).sum();
                std::hint::black_box(sum);
            })
        });
    }

    group.finish();
}

/// Range iteration benchmarks - test bounded iteration performance
pub fn range_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("range_iteration");
    let size = 1 << 12; // 4K elements
    group.throughput(Throughput::Elements(size / 4)); // Iterate over ~1/4 of elements

    // ART range iteration
    group.bench_function("art_range", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        for i in 0..size {
            tree.insert(i, i * 2);
        }

        let start: ArrayKey<16> = (size / 4).into();
        let end: ArrayKey<16> = ((size * 3) / 4).into();

        b.iter(|| {
            let sum: u64 = tree.range(start..end).map(|(_, v)| *v).sum();
            std::hint::black_box(sum);
        })
    });

    // BTreeMap range iteration
    group.bench_function("btreemap_range", |b| {
        let mut map = BTreeMap::new();
        for i in 0..size {
            map.insert(i, i * 2);
        }

        let start = size / 4;
        let end = (size * 3) / 4;

        b.iter(|| {
            let sum: u64 = map.range(start..end).map(|(_, v)| *v).sum();
            std::hint::black_box(sum);
        })
    });

    // BLART range iteration
    group.bench_function("blart_range", |b| {
        let mut tree = TreeMap::<Box<[u8]>, u64>::new();
        for i in 0..size {
            tree.try_insert(i.to_be_bytes().into(), i * 2).unwrap();
        }

        let start_key: Box<[u8]> = (size / 4).to_be_bytes().into();
        let end_key: Box<[u8]> = ((size * 3) / 4).to_be_bytes().into();

        b.iter(|| {
            let sum: u64 = tree
                .range(start_key.clone()..end_key.clone())
                .map(|(_, v)| *v)
                .sum();
            std::hint::black_box(sum);
        })
    });

    group.finish();
}

/// Isolate the cost of positioning range iteration at a start bound.
/// We only consume the first element so work is dominated by seek/setup.
pub fn start_seek_positioning(c: &mut Criterion) {
    let mut group = c.benchmark_group("start_seek_positioning");
    const SIZES: [u64; 3] = [1 << 12, 1 << 15, 1 << 17];

    for size in SIZES {
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::new("art_unbounded_first", size),
            &size,
            |b, size| {
                let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
                for i in 0..*size {
                    tree.insert(i, i * 2);
                }

                b.iter(|| {
                    let first = tree.range(..).next().map(|(_, v)| *v);
                    std::hint::black_box(first);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("art_start_mid_first", size),
            &size,
            |b, size| {
                let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
                for i in 0..*size {
                    tree.insert(i, i * 2);
                }
                let start: ArrayKey<16> = (size / 2).into();

                b.iter(|| {
                    let first = tree.range(start..).next().map(|(_, v)| *v);
                    std::hint::black_box(first);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("art_start_high_first", size),
            &size,
            |b, size| {
                let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
                for i in 0..*size {
                    tree.insert(i, i * 2);
                }
                let start: ArrayKey<16> = ((size * 15) / 16).into();

                b.iter(|| {
                    let first = tree.range(start..).next().map(|(_, v)| *v);
                    std::hint::black_box(first);
                })
            },
        );
    }

    group.finish();
}

/// Values-only iteration benchmarks - test iteration without key reconstruction
pub fn values_iteration_numeric(c: &mut Criterion) {
    let mut group = c.benchmark_group("values_iteration_numeric");

    for size in TREE_SIZES {
        group.throughput(Throughput::Elements(size));

        // ART values iteration (no key reconstruction)
        group.bench_with_input(BenchmarkId::new("art_values", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
            for i in 0..*size {
                tree.insert(i, i * 2);
            }

            b.iter(|| {
                let sum: u64 = tree.values_iter().copied().sum();
                std::hint::black_box(sum);
            })
        });

        // ART full iteration (with key reconstruction) for comparison
        group.bench_with_input(BenchmarkId::new("art_full", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
            for i in 0..*size {
                tree.insert(i, i * 2);
            }

            b.iter(|| {
                let sum: u64 = tree.iter().map(|(_, v)| *v).sum();
                std::hint::black_box(sum);
            })
        });

        // HashMap values iteration for comparison
        group.bench_with_input(
            BenchmarkId::new("hashmap_values", size),
            &size,
            |b, size| {
                let mut map = HashMap::new();
                for i in 0..*size {
                    map.insert(i, i * 2);
                }

                b.iter(|| {
                    let sum: u64 = map.values().copied().sum();
                    std::hint::black_box(sum);
                })
            },
        );

        // BTreeMap values iteration for comparison
        group.bench_with_input(
            BenchmarkId::new("btreemap_values", size),
            &size,
            |b, size| {
                let mut map = BTreeMap::new();
                for i in 0..*size {
                    map.insert(i, i * 2);
                }

                b.iter(|| {
                    let sum: u64 = map.values().copied().sum();
                    std::hint::black_box(sum);
                })
            },
        );

        // BLART values iteration for comparison
        group.bench_with_input(BenchmarkId::new("blart_values", size), &size, |b, size| {
            let mut tree = TreeMap::<Box<[u8]>, u64>::new();
            for i in 0..*size {
                tree.try_insert(i.to_be_bytes().into(), i * 2).unwrap();
            }

            b.iter(|| {
                let sum: u64 = tree.values().copied().sum();
                std::hint::black_box(sum);
            })
        });

        // BLART full iteration for comparison
        group.bench_with_input(BenchmarkId::new("blart_full", size), &size, |b, size| {
            let mut tree = TreeMap::<Box<[u8]>, u64>::new();
            for i in 0..*size {
                tree.try_insert(i.to_be_bytes().into(), i * 2).unwrap();
            }

            b.iter(|| {
                let sum: u64 = tree.iter().map(|(_, v)| *v).sum();
                std::hint::black_box(sum);
            })
        });
    }

    group.finish();
}

criterion_group!(
    name = iteration_benches;
    config = criterion_config();
    targets = full_iteration_numeric, range_iteration, start_seek_positioning, values_iteration_numeric
);
criterion_main!(iteration_benches);
