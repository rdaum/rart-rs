use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rart::KeyTrait;
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
            let g1 = (i & 0x1f) as u8;
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
        medium.push(key[..1].to_vec());
        narrow.push(key[..2].to_vec());
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

fn make_range_keys(size: usize) -> Vec<Vec<u8>> {
    (0..size)
        .map(|i| (i as u64).to_be_bytes().to_vec())
        .collect()
}

fn generate_overlapping_keys(size: usize, overlap_ratio: f64) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let overlap = ((size as f64) * overlap_ratio) as usize;
    let unique = size - overlap;

    let mut left = Vec::with_capacity(size);
    let mut right = Vec::with_capacity(size);

    for i in 0..overlap {
        let mut key = b"shared/".to_vec();
        key.extend_from_slice(&(i as u64).to_be_bytes());
        left.push(key.clone());
        right.push(key);
    }

    for i in 0..unique {
        let mut left_key = b"left/".to_vec();
        left_key.extend_from_slice(&(i as u64).to_be_bytes());
        left.push(left_key);

        let mut right_key = b"right/".to_vec();
        right_key.extend_from_slice(&(i as u64).to_be_bytes());
        right.push(right_key);
    }

    (left, right)
}

fn build_tree(keys: &[Vec<u8>]) -> AdaptiveRadixTree<VectorKey, usize> {
    let mut tree = AdaptiveRadixTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert_k(&VectorKey::new_from_slice(key), i);
    }
    tree
}

fn cmp_segments_to_slice(segments: &[&[u8]], len: usize, slice: &[u8]) -> std::cmp::Ordering {
    let mut offset = 0usize;
    for segment in segments {
        let remaining = &slice[offset..];
        let common = segment.len().min(remaining.len());
        match segment[..common].cmp(&remaining[..common]) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        if segment.len() != common {
            return std::cmp::Ordering::Greater;
        }
        if remaining.len() != common {
            return std::cmp::Ordering::Less;
        }
        offset += common;
    }

    len.cmp(&slice.len())
}

type SegmentedCompareCase = (&'static str, Vec<Vec<u8>>, Vec<u8>);

fn make_segmented_compare_cases() -> Vec<SegmentedCompareCase> {
    vec![
        (
            "one_seg_lt",
            vec![b"00000042".to_vec()],
            b"00000080".to_vec(),
        ),
        (
            "two_seg_lt",
            vec![b"00".to_vec(), b"000042".to_vec()],
            b"00000080".to_vec(),
        ),
        (
            "four_seg_lt",
            vec![
                b"0".to_vec(),
                b"0".to_vec(),
                b"0000".to_vec(),
                b"42".to_vec(),
            ],
            b"00000080".to_vec(),
        ),
        (
            "two_seg_eq",
            vec![b"00".to_vec(), b"000080".to_vec()],
            b"00000080".to_vec(),
        ),
        (
            "two_seg_gt",
            vec![b"00".to_vec(), b"000081".to_vec()],
            b"00000080".to_vec(),
        ),
    ]
}

fn bench_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_iteration");

    for size in SIZES {
        let keys = make_keys(size);
        let tree = build_tree(&keys);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("iter_owned", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                for (key, value) in tree.iter() {
                    acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("iter_lending", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                tree.for_each_view(|key, value| {
                    acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                });
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("values_only", size), &size, |b, _| {
            b.iter(|| {
                let acc: usize = tree.values_iter().copied().sum();
                std::hint::black_box(acc);
            })
        });
    }

    group.finish();
}

fn bench_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_range");

    for size in SIZES {
        let keys = make_range_keys(size);
        let tree = build_tree(&keys);
        let start = VectorKey::new_from_slice(&keys[size / 4]);
        let end = VectorKey::new_from_slice(&keys[(size * 3) / 4]);
        group.throughput(Throughput::Elements((size / 2) as u64));

        group.bench_with_input(BenchmarkId::new("range_owned", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                for (key, value) in tree.range(start.clone()..end.clone()) {
                    acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("range_lending", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                tree.for_each_range_view(start.clone()..end.clone(), |key, value| {
                    acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                });
                std::hint::black_box(acc);
            })
        });
    }

    group.finish();
}

fn bench_range_start_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_range_start_only");

    for size in SIZES {
        let keys = make_range_keys(size);
        let tree = build_tree(&keys);
        let start = VectorKey::new_from_slice(&keys[size / 4]);
        group.throughput(Throughput::Elements((size - (size / 4)) as u64));

        group.bench_with_input(BenchmarkId::new("range_owned", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                for (key, value) in tree.range(start.clone()..) {
                    acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("range_lending", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                tree.for_each_range_view(start.clone().., |key, value| {
                    acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                });
                std::hint::black_box(acc);
            })
        });
    }

    group.finish();
}

fn bench_range_end_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_range_end_only");

    for size in SIZES {
        let keys = make_range_keys(size);
        let tree = build_tree(&keys);
        let end = VectorKey::new_from_slice(&keys[(size * 3) / 4]);
        group.throughput(Throughput::Elements(((size * 3) / 4) as u64));

        group.bench_with_input(BenchmarkId::new("range_owned", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                for (key, value) in tree.range(..end.clone()) {
                    acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                }
                std::hint::black_box(acc);
            })
        });

        group.bench_with_input(BenchmarkId::new("range_lending", size), &size, |b, _| {
            b.iter(|| {
                let mut acc = 0usize;
                tree.for_each_range_view(..end.clone(), |key, value| {
                    acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                });
                std::hint::black_box(acc);
            })
        });
    }

    group.finish();
}

fn bench_borrowed_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_compare");

    for (label, owned_segments, bound) in make_segmented_compare_cases() {
        let segment_refs: Vec<&[u8]> = owned_segments.iter().map(Vec::as_slice).collect();
        let flattened: Vec<u8> = owned_segments.concat();
        let len = flattened.len();

        group.throughput(Throughput::Elements(len as u64));

        group.bench_with_input(
            BenchmarkId::new("segments_vs_slice", label),
            &label,
            |b, _| {
                b.iter(|| {
                    std::hint::black_box(cmp_segments_to_slice(
                        &segment_refs,
                        len,
                        std::hint::black_box(bound.as_slice()),
                    ))
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("contiguous_vs_slice", label),
            &label,
            |b, _| {
                b.iter(|| {
                    std::hint::black_box(
                        std::hint::black_box(flattened.as_slice())
                            .cmp(std::hint::black_box(bound.as_slice())),
                    )
                })
            },
        );
    }

    group.finish();
}

fn bench_prefix(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_prefix_iteration");

    for size in SIZES {
        let keys = make_keys(size);
        let tree = build_tree(&keys);
        let (medium_prefixes, narrow_prefixes) = make_prefix_queries(&keys);

        for (label, prefixes) in [("medium", &medium_prefixes), ("narrow", &narrow_prefixes)] {
            group.throughput(Throughput::Elements(prefixes.len() as u64));

            group.bench_with_input(
                BenchmarkId::new(format!("prefix_owned_{label}"), size),
                &size,
                |b, _| {
                    b.iter(|| {
                        let mut acc = 0usize;
                        for p in prefixes {
                            for (key, value) in tree.prefix_iter_k(&VectorKey::new_from_slice(p)) {
                                acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                            }
                        }
                        std::hint::black_box(acc);
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new(format!("prefix_lending_{label}"), size),
                &size,
                |b, _| {
                    b.iter(|| {
                        let mut acc = 0usize;
                        for p in prefixes {
                            tree.prefix_for_each_view_k(
                                &VectorKey::new_from_slice(p),
                                |key, value| {
                                    acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                                },
                            );
                        }
                        std::hint::black_box(acc);
                    })
                },
            );
        }
    }

    group.finish();
}

fn bench_longest_prefix(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_longest_prefix_match");

    for size in SIZES {
        let (keys, queries) = make_longest_prefix_dataset(size);
        let tree = build_tree(&keys);
        group.throughput(Throughput::Elements(queries.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("longest_prefix_owned", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for q in &queries {
                        if let Some((key, value)) =
                            tree.longest_prefix_match_k(&VectorKey::new_from_slice(q))
                        {
                            acc = acc.wrapping_add(key.as_ref().len()).wrapping_add(*value);
                        }
                    }
                    std::hint::black_box(acc);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("longest_prefix_lending", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for q in &queries {
                        tree.with_longest_prefix_match_view_k(
                            &VectorKey::new_from_slice(q),
                            |key, value| {
                                acc = acc.wrapping_add(key.len()).wrapping_add(*value);
                            },
                        );
                    }
                    std::hint::black_box(acc);
                })
            },
        );
    }

    group.finish();
}

fn bench_intersection(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrowed_intersection");

    for size in [10_000usize, 100_000usize] {
        for overlap in [0.1f64, 0.5f64, 0.9f64] {
            let (left_keys, right_keys) = generate_overlapping_keys(size, overlap);
            let left = build_tree(&left_keys);
            let right = build_tree(&right_keys);
            let case = format!("n{}_o{}", size, (overlap * 100.0) as u32);

            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(BenchmarkId::new("intersect_owned", &case), &case, |b, _| {
                b.iter(|| {
                    let mut acc = 0usize;
                    left.intersect_with(&right, |key, left_value, right_value| {
                        acc = acc
                            .wrapping_add(key.as_ref().len())
                            .wrapping_add(*left_value)
                            .wrapping_add(*right_value);
                    });
                    std::hint::black_box(acc);
                });
            });

            group.bench_with_input(
                BenchmarkId::new("intersect_lending", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let mut acc = 0usize;
                        left.intersect_lending_with(&right, |key, left_value, right_value| {
                            acc = acc
                                .wrapping_add(key.len())
                                .wrapping_add(*left_value)
                                .wrapping_add(*right_value);
                        });
                        std::hint::black_box(acc);
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("intersect_values", &case),
                &case,
                |b, _| {
                    b.iter(|| {
                        let mut acc = 0usize;
                        left.intersect_values_with(&right, |left_value, right_value| {
                            acc = acc.wrapping_add(*left_value).wrapping_add(*right_value);
                        });
                        std::hint::black_box(acc);
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(
    name = borrowed_view_benches;
    config = criterion_config();
    targets = bench_iteration,
        bench_range,
        bench_range_start_only,
        bench_range_end_only,
        bench_borrowed_compare,
        bench_prefix,
        bench_longest_prefix,
        bench_intersection
);
criterion_main!(borrowed_view_benches);
