use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rart::keys::array_key::ArrayKey;
use rart::tree::AdaptiveRadixTree;

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn criterion_config() -> Criterion {
    if full_bench_profile() {
        Criterion::default()
    } else {
        Criterion::default()
            .sample_size(20)
            .warm_up_time(Duration::from_millis(700))
            .measurement_time(Duration::from_secs(2))
    }
}

fn generate_overlapping_keys(size: usize, overlap_ratio: f64) -> (Vec<u64>, Vec<u64>) {
    let overlap = ((size as f64) * overlap_ratio) as usize;
    let unique = size - overlap;

    let mut left = Vec::with_capacity(size);
    let mut right = Vec::with_capacity(size);

    // Shared prefix of keyspace, appears in both sets.
    for i in 0..overlap as u64 {
        left.push(i);
        right.push(i);
    }

    // Non-overlapping tails.
    for i in 0..unique as u64 {
        left.push(1_000_000_000 + i);
        right.push(2_000_000_000 + i);
    }

    (left, right)
}

fn build_art(keys: &[u64]) -> AdaptiveRadixTree<ArrayKey<16>, usize> {
    let mut tree = AdaptiveRadixTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert(*key, i);
    }
    tree
}

fn build_btree(keys: &[u64]) -> BTreeMap<u64, usize> {
    let mut map = BTreeMap::new();
    for (i, key) in keys.iter().enumerate() {
        map.insert(*key, i);
    }
    map
}

fn art_intersect_count(
    left: &AdaptiveRadixTree<ArrayKey<16>, usize>,
    right: &AdaptiveRadixTree<ArrayKey<16>, usize>,
) -> usize {
    let mut count = 0usize;
    left.intersect_with(right, |_k, _lv, _rv| {
        count += 1;
    });
    count
}

fn art_intersect_count_keyless(
    left: &AdaptiveRadixTree<ArrayKey<16>, usize>,
    right: &AdaptiveRadixTree<ArrayKey<16>, usize>,
) -> usize {
    left.intersect_count(right)
}

fn btree_merge_join_count(left: &BTreeMap<u64, usize>, right: &BTreeMap<u64, usize>) -> usize {
    let mut left_it = left.iter().peekable();
    let mut right_it = right.iter().peekable();
    let mut count = 0usize;

    loop {
        let (Some((left_key, _)), Some((right_key, _))) = (left_it.peek(), right_it.peek()) else {
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

fn bench_intersection(c: &mut Criterion) {
    let mut group = c.benchmark_group("intersection_vs_btree_merge_join");

    for size in [10_000usize, 100_000usize] {
        for overlap in [0.1f64, 0.5f64, 0.9f64] {
            let (left_keys, right_keys) = generate_overlapping_keys(size, overlap);
            let left_art = build_art(&left_keys);
            let right_art = build_art(&right_keys);
            let left_btree = build_btree(&left_keys);
            let right_btree = build_btree(&right_keys);
            let case = format!("n{}_o{}", size, (overlap * 100.0) as u32);

            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(
                BenchmarkId::new("art_intersect_with", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count =
                            art_intersect_count(black_box(&left_art), black_box(&right_art));
                        black_box(count);
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("art_intersect_count", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count = art_intersect_count_keyless(
                            black_box(&left_art),
                            black_box(&right_art),
                        );
                        black_box(count);
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("btree_merge_join", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let count =
                            btree_merge_join_count(black_box(&left_btree), black_box(&right_btree));
                        black_box(count);
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = bench_intersection
}
criterion_main!(benches);
