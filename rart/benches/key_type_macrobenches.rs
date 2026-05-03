use std::time::Duration;

use micromeasure::{BenchContext, BenchmarkRuntimeOptions, Throughput, benchmark_main, black_box};

use rart::keys::KeyTrait;
use rart::{AdaptiveRadixTree, ArrayKey, OverflowKey, VectorKey};

const SHORT_SIZE: usize = 1 << 15;
const LARGE_SIZE: usize = 1 << 15;

fn runtime_options() -> BenchmarkRuntimeOptions {
    if std::env::var("RART_BENCH_QUICK").as_deref() == Ok("1") {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_millis(100),
            benchmark_duration: Duration::from_millis(500),
            min_samples: 5,
            max_samples: 10,
        }
    } else if std::env::var("RART_BENCH_FULL").as_deref() == Ok("1") {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_secs(1),
            benchmark_duration: Duration::from_secs(10),
            min_samples: 20,
            max_samples: 100,
        }
    } else {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_millis(500),
            benchmark_duration: Duration::from_secs(3),
            min_samples: 10,
            max_samples: 40,
        }
    }
}

fn bench_filter_matches(name: &str) -> bool {
    std::env::var("RART_BENCH_FILTER")
        .map(|filter| name.contains(&filter))
        .unwrap_or(true)
}

fn make_key_bytes(idx: usize, len: usize) -> Vec<u8> {
    let mut data = vec![0; len];
    let id = (idx as u64).to_be_bytes();
    let copy_len = len.min(id.len());
    data[..copy_len].copy_from_slice(&id[id.len() - copy_len..]);
    for (offset, byte) in data[copy_len..].iter_mut().enumerate() {
        *byte = idx.wrapping_mul(31).wrapping_add(offset * 17) as u8;
    }
    data
}

fn make_common_prefix_key(idx: usize, len: usize) -> Vec<u8> {
    let mut data = vec![b'p'; len];
    let id = (idx as u64).to_be_bytes();
    let start = len - id.len();
    data[start..].copy_from_slice(&id);
    data
}

fn make_dataset<const MODE: usize>() -> Vec<Vec<u8>> {
    (0..dataset_len::<MODE>())
        .map(|idx| match MODE {
            0 => make_key_bytes(idx, 8),
            1 => make_key_bytes(idx, 32),
            2 => {
                if idx % 10 == 0 {
                    make_key_bytes(idx, 96)
                } else {
                    make_key_bytes(idx, 12)
                }
            }
            3 => {
                let mixed = idx.wrapping_mul(0x9e37_79b1) ^ idx.rotate_left(13);
                if mixed & 1 == 0 {
                    make_key_bytes(idx, 12)
                } else {
                    make_key_bytes(idx, 96)
                }
            }
            4 => make_common_prefix_key(idx, 48),
            _ => unreachable!("unknown key type macrobench dataset mode"),
        })
        .collect()
}

fn dataset_len<const MODE: usize>() -> usize {
    if MODE <= 1 { SHORT_SIZE } else { LARGE_SIZE }
}

fn build_keys<K: KeyTrait>(raw: &[Vec<u8>]) -> Vec<K> {
    raw.iter().map(|bytes| K::new_from_slice(bytes)).collect()
}

fn build_tree<K: KeyTrait>(keys: &[K]) -> AdaptiveRadixTree<K, usize> {
    let mut tree = AdaptiveRadixTree::<K, usize>::new();
    for (idx, key) in keys.iter().enumerate() {
        tree.insert_k(key, idx);
    }
    tree
}

struct RawContext<const MODE: usize> {
    raw: Vec<Vec<u8>>,
}

impl<const MODE: usize> BenchContext for RawContext<MODE> {
    fn prepare(_num_chunks: usize) -> Self {
        Self {
            raw: make_dataset::<MODE>(),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(1)
    }

    fn operations_per_chunk() -> Option<u64> {
        Some(dataset_len::<MODE>() as u64)
    }
}

struct TreeContext<K: KeyTrait, const MODE: usize> {
    keys: Vec<K>,
    tree: AdaptiveRadixTree<K, usize>,
}

impl<K: KeyTrait, const MODE: usize> BenchContext for TreeContext<K, MODE> {
    fn prepare(_num_chunks: usize) -> Self {
        let raw = make_dataset::<MODE>();
        let keys = build_keys::<K>(&raw);
        let tree = build_tree(&keys);
        Self { keys, tree }
    }

    fn chunk_size() -> Option<usize> {
        Some(1)
    }

    fn operations_per_chunk() -> Option<u64> {
        Some(dataset_len::<MODE>() as u64)
    }
}

fn bench_build<K: KeyTrait, const MODE: usize>(
    ctx: &mut RawContext<MODE>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let keys = build_keys::<K>(&ctx.raw);
    black_box(build_tree(&keys));
}

fn bench_lookup_all<K: KeyTrait, const MODE: usize>(
    ctx: &mut TreeContext<K, MODE>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for key in &ctx.keys {
        sum = sum.wrapping_add(*ctx.tree.get_k(key).unwrap());
    }
    black_box(sum);
}

fn bench_iter_owned<K: KeyTrait, const MODE: usize>(
    ctx: &mut TreeContext<K, MODE>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for (key, value) in ctx.tree.iter() {
        sum = sum.wrapping_add(key.as_ref().len()).wrapping_add(*value);
    }
    black_box(sum);
}

fn bench_iter_view<K: KeyTrait, const MODE: usize>(
    ctx: &mut TreeContext<K, MODE>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    ctx.tree.for_each_view(|key, value| {
        sum = sum.wrapping_add(key.len()).wrapping_add(*value);
    });
    black_box(sum);
}

macro_rules! register_dynamic_dataset {
    ($runner:ident, $mode:literal, $name:literal) => {
        $runner.group::<RawContext<$mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_build"),
                bench_build::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32p8_build"),
                bench_build::<OverflowKey<32, 8>, $mode>,
            );
        });

        $runner.group::<TreeContext<VectorKey, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_lookup_all"),
                bench_lookup_all::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_iter_owned"),
                bench_iter_owned::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_iter_view"),
                bench_iter_view::<VectorKey, $mode>,
            );
        });

        $runner.group::<TreeContext<OverflowKey<32, 8>, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32p8_lookup_all"),
                bench_lookup_all::<OverflowKey<32, 8>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32p8_iter_owned"),
                bench_iter_owned::<OverflowKey<32, 8>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32p8_iter_view"),
                bench_iter_view::<OverflowKey<32, 8>, $mode>,
            );
        });
    };
}

macro_rules! register_bounded_dataset {
    ($runner:ident, $mode:literal, $name:literal) => {
        $runner.group::<RawContext<$mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_build"),
                bench_build::<ArrayKey<32>, $mode>,
            );
        });

        $runner.group::<TreeContext<ArrayKey<32>, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_lookup_all"),
                bench_lookup_all::<ArrayKey<32>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_iter_owned"),
                bench_iter_owned::<ArrayKey<32>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_iter_view"),
                bench_iter_view::<ArrayKey<32>, $mode>,
            );
        });
    };
}

benchmark_main!(|runner| {
    runner.set_runtime(runtime_options());

    if bench_filter_matches("short8") {
        register_bounded_dataset!(runner, 0, "short8");
        register_dynamic_dataset!(runner, 0, "short8");
    }

    if bench_filter_matches("at_inline32") {
        register_bounded_dataset!(runner, 1, "at_inline32");
        register_dynamic_dataset!(runner, 1, "at_inline32");
    }

    if bench_filter_matches("mixed90_short") {
        register_dynamic_dataset!(runner, 2, "mixed90_short");
    }
    if bench_filter_matches("mixed50_random") {
        register_dynamic_dataset!(runner, 3, "mixed50_random");
    }
    if bench_filter_matches("common_prefix48") {
        register_dynamic_dataset!(runner, 4, "common_prefix48");
    }
});
