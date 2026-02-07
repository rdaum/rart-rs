//! Prefix-operation benchmarks for AdaptiveRadixTree.
//! Compares ART against BTreeMap and HashMap for:
//! - longest-prefix lookup
//! - prefix-subtree iteration

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rart::keys::KeyTrait;
use rart::keys::vector_key::VectorKey;
use rart::tree::AdaptiveRadixTree;

const SIZES: [usize; 3] = [1 << 10, 1 << 12, 1 << 15];
const MAX_PREFIX_QUERIES: usize = 1024;

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

fn make_keys(size: usize) -> Vec<Vec<u8>> {
    (0..size)
        .map(|i| {
            let g1 = (i & 0x1f) as u8; // 32 groups
            let g2 = ((i >> 5) & 0x1f) as u8;
            let mut k = Vec::with_capacity(11);
            k.push(g1);
            k.push(g2);
            k.push(b'/');
            k.extend_from_slice(&(i as u64).to_be_bytes());
            k
        })
        .collect()
}

fn make_prefix_queries(keys: &[Vec<u8>]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let query_count = keys.len().min(MAX_PREFIX_QUERIES);
    let step = (keys.len() / query_count.max(1)).max(1);

    let mut medium = Vec::with_capacity(query_count);
    let mut narrow = Vec::with_capacity(query_count);
    for key in keys.iter().step_by(step).take(query_count) {
        medium.push(key[..1].to_vec()); // ~1/32 of keyspace
        narrow.push(key[..2].to_vec()); // ~1/1024 of keyspace
    }
    (medium, narrow)
}

fn make_longest_prefix_dataset(size: usize) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut inserted = Vec::with_capacity(size);
    let mut queries = Vec::with_capacity(size);
    for i in 0..size {
        let g = (i & 0x1f) as u8;
        let stem = (i as u64).to_be_bytes();

        let mut key = Vec::with_capacity(9);
        key.push(g);
        key.extend_from_slice(&stem);
        inserted.push(key.clone());

        let mut q = key;
        q.push(0xfe);
        q.push(0xff);
        queries.push(q);
    }
    (inserted, queries)
}

fn next_prefix_bound(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut next = prefix.to_vec();
    for i in (0..next.len()).rev() {
        if next[i] != u8::MAX {
            next[i] += 1;
            next.truncate(i + 1);
            return Some(next);
        }
    }
    None
}

fn longest_prefix_hash<'a>(map: &'a HashMap<Vec<u8>, usize>, q: &[u8]) -> Option<&'a usize> {
    for len in (0..=q.len()).rev() {
        if let Some(v) = map.get(&q[..len]) {
            return Some(v);
        }
    }
    None
}

fn longest_prefix_btree<'a>(map: &'a BTreeMap<Vec<u8>, usize>, q: &[u8]) -> Option<&'a usize> {
    for len in (0..=q.len()).rev() {
        if let Some(v) = map.get(&q[..len]) {
            return Some(v);
        }
    }
    None
}

pub fn longest_prefix_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("longest_prefix_match");

    for size in SIZES {
        let (keys, queries) = make_longest_prefix_dataset(size);
        group.throughput(Throughput::Elements(queries.len() as u64));

        group.bench_with_input(BenchmarkId::new("art", size), &size, |b, _| {
            let mut tree = AdaptiveRadixTree::<VectorKey, usize>::new();
            for (i, key) in keys.iter().enumerate() {
                tree.insert_k(&VectorKey::new_from_slice(key), i);
            }

            b.iter(|| {
                let mut acc = 0usize;
                for q in &queries {
                    if let Some((_, v)) = tree.longest_prefix_match_k(&VectorKey::new_from_slice(q))
                    {
                        acc = acc.wrapping_add(*v);
                    }
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("btree", size), &size, |b, _| {
            let mut map = BTreeMap::new();
            for (i, key) in keys.iter().enumerate() {
                map.insert(key.clone(), i);
            }

            b.iter(|| {
                let mut acc = 0usize;
                for q in &queries {
                    if let Some(v) = longest_prefix_btree(&map, q) {
                        acc = acc.wrapping_add(*v);
                    }
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("hashmap", size), &size, |b, _| {
            let mut map = HashMap::new();
            for (i, key) in keys.iter().enumerate() {
                map.insert(key.clone(), i);
            }

            b.iter(|| {
                let mut acc = 0usize;
                for q in &queries {
                    if let Some(v) = longest_prefix_hash(&map, q) {
                        acc = acc.wrapping_add(*v);
                    }
                }
                std::hint::black_box(acc);
            })
        });
    }

    group.finish();
}

pub fn prefix_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("prefix_iteration");

    for size in SIZES {
        let keys = make_keys(size);
        let (medium_prefixes, narrow_prefixes) = make_prefix_queries(&keys);

        for (label, prefixes) in [("medium", &medium_prefixes), ("narrow", &narrow_prefixes)] {
            group.throughput(Throughput::Elements(prefixes.len() as u64));

            group.bench_with_input(
                BenchmarkId::new(format!("art_{label}"), size),
                &size,
                |b, _| {
                    let mut tree = AdaptiveRadixTree::<VectorKey, usize>::new();
                    for (i, key) in keys.iter().enumerate() {
                        tree.insert_k(&VectorKey::new_from_slice(key), i);
                    }

                    b.iter(|| {
                        let mut acc = 0usize;
                        for p in prefixes {
                            for (_, v) in tree.prefix_iter_k(&VectorKey::new_from_slice(p)) {
                                acc = acc.wrapping_add(*v);
                            }
                        }
                        std::hint::black_box(acc);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new(format!("btree_{label}"), size),
                &size,
                |b, _| {
                    let mut map = BTreeMap::new();
                    for (i, key) in keys.iter().enumerate() {
                        map.insert(key.clone(), i);
                    }

                    b.iter(|| {
                        let mut acc = 0usize;
                        for p in prefixes {
                            let start = Bound::Included(p.clone());
                            let end = match next_prefix_bound(p) {
                                Some(next) => Bound::Excluded(next),
                                None => Bound::Unbounded,
                            };
                            for (_, v) in map.range((start, end)) {
                                acc = acc.wrapping_add(*v);
                            }
                        }
                        std::hint::black_box(acc);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new(format!("hashmap_{label}"), size),
                &size,
                |b, _| {
                    let mut map = HashMap::new();
                    for (i, key) in keys.iter().enumerate() {
                        map.insert(key.clone(), i);
                    }

                    b.iter(|| {
                        let mut acc = 0usize;
                        for p in prefixes {
                            for (k, v) in &map {
                                if k.starts_with(p) {
                                    acc = acc.wrapping_add(*v);
                                }
                            }
                        }
                        std::hint::black_box(acc);
                    })
                },
            );
        }
    }

    group.finish();
}

criterion_group! {
    name = prefix_benches;
    config = criterion_config();
    targets = longest_prefix_match, prefix_iteration
}
criterion_main!(prefix_benches);
