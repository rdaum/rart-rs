//! Full-tree construction benchmarks for bulk-load work.

use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rart::{AdaptiveRadixTree, ArrayKey, KeyTrait};

const TREE_SIZES: [usize; 4] = [1 << 10, 1 << 12, 1 << 15, 1 << 17];

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn criterion_config() -> Criterion {
    if full_bench_profile() {
        Criterion::default()
    } else {
        Criterion::default()
            .sample_size(20)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(2))
    }
}

fn build_incremental(items: &[(ArrayKey<16>, u64)]) -> AdaptiveRadixTree<ArrayKey<16>, u64> {
    let mut tree = AdaptiveRadixTree::new();
    for &(key, value) in items {
        tree.insert_k(&key, value);
    }
    tree
}

fn build_bulk_load_sorted(items: &[(ArrayKey<16>, u64)]) -> AdaptiveRadixTree<ArrayKey<16>, u64> {
    AdaptiveRadixTree::bulk_load_sorted(items.iter().copied())
}

fn build_bulk_load_sorted_unique(
    items: &[(ArrayKey<16>, u64)],
) -> AdaptiveRadixTree<ArrayKey<16>, u64> {
    AdaptiveRadixTree::bulk_load_sorted_unique(items.iter().copied())
}

fn build_bulk_load_sorted_callback(
    items: &[(ArrayKey<16>, u64)],
) -> AdaptiveRadixTree<ArrayKey<16>, u64> {
    AdaptiveRadixTree::bulk_load_sorted_unique_by_index(
        items.len(),
        |index| &items[index].0,
        |index| items[index].1,
    )
}

fn sequential_u64_items(size: usize) -> Vec<(ArrayKey<16>, u64)> {
    (0..size).map(|i| ((i as u64).into(), i as u64)).collect()
}

fn prefixed_items(size: usize) -> Vec<(ArrayKey<16>, u64)> {
    (0..size)
        .map(|i| {
            let mut bytes = [0u8; 16];
            bytes[..6].copy_from_slice(b"tenant");
            bytes[6..8].copy_from_slice(&((i / 1024) as u16).to_be_bytes());
            bytes[8..16].copy_from_slice(&(i as u64).to_be_bytes());
            (ArrayKey::new_from_slice(&bytes), i as u64)
        })
        .collect()
}

fn shuffled(mut items: Vec<(ArrayKey<16>, u64)>) -> Vec<(ArrayKey<16>, u64)> {
    let mut rng = StdRng::seed_from_u64(0x5eed);
    items.shuffle(&mut rng);
    items
}

pub fn incremental_build_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_load_baseline_incremental_u64");

    for size in TREE_SIZES {
        let sorted = sequential_u64_items(size);
        let shuffled = shuffled(sorted.clone());

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("sorted", size), &sorted, |b, items| {
            b.iter_batched(|| (), |_| build_incremental(items), BatchSize::LargeInput);
        });

        group.bench_with_input(BenchmarkId::new("shuffled", size), &shuffled, |b, items| {
            b.iter_batched(|| (), |_| build_incremental(items), BatchSize::LargeInput);
        });
    }

    group.finish();
}

pub fn bulk_load_build_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_load_u64");

    for size in TREE_SIZES {
        let sorted = sequential_u64_items(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("sorted_callback", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted_callback(items),
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("sorted_unique", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted_unique(items),
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("sorted_presorted", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted(items),
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

pub fn incremental_build_prefixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_load_baseline_incremental_prefixed");

    for size in TREE_SIZES {
        let sorted = prefixed_items(size);
        let shuffled = shuffled(sorted.clone());

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("sorted", size), &sorted, |b, items| {
            b.iter_batched(|| (), |_| build_incremental(items), BatchSize::LargeInput);
        });

        group.bench_with_input(BenchmarkId::new("shuffled", size), &shuffled, |b, items| {
            b.iter_batched(|| (), |_| build_incremental(items), BatchSize::LargeInput);
        });
    }

    group.finish();
}

pub fn bulk_load_build_prefixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_load_prefixed");

    for size in TREE_SIZES {
        let sorted = prefixed_items(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("sorted_callback", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted_callback(items),
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("sorted_unique", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted_unique(items),
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("sorted_presorted", size),
            &sorted,
            |b, items| {
                b.iter_batched(
                    || (),
                    |_| build_bulk_load_sorted(items),
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = incremental_build_u64, incremental_build_prefixed, bulk_load_build_u64, bulk_load_build_prefixed
}
criterion_main!(benches);
