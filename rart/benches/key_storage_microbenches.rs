use std::time::Duration;

use micromeasure::{BenchContext, BenchmarkRuntimeOptions, Throughput, benchmark_main, black_box};

use rart::keys::KeyTrait;
use rart::keys::array_key::ArrayKey;
use rart::keys::overflow_key::OverflowKey;
use rart::keys::vector_key::VectorKey;
use rart::partials::Partial;
use rart::partials::vector_partial::VectorPartial;
use rart::tree::AdaptiveRadixTree;

const INLINE: usize = 32;
const PARTIAL_INLINE_SMALL: usize = 8;
const PARTIAL_INLINE_MEDIUM: usize = 16;
const DATASET_SIZE: usize = 4096;

fn runtime_options() -> BenchmarkRuntimeOptions {
    if std::env::var("RART_BENCH_FULL").as_deref() == Ok("1") {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_secs(1),
            benchmark_duration: Duration::from_secs(10),
            min_samples: 20,
            max_samples: 100,
        }
    } else {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_millis(250),
            benchmark_duration: Duration::from_secs(2),
            min_samples: 10,
            max_samples: 40,
        }
    }
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
    (0..DATASET_SIZE)
        .map(|idx| {
            if MODE == 5 {
                return make_common_prefix_key(idx, 48);
            }

            let len = match MODE {
                0 => 8,
                1 => 32,
                2 => 96,
                3 => {
                    if idx % 10 == 0 {
                        96
                    } else {
                        12
                    }
                }
                4 => {
                    let mixed = idx.wrapping_mul(0x9e37_79b1) ^ idx.rotate_left(13);
                    if mixed & 1 == 0 { 12 } else { 96 }
                }
                _ => unreachable!("unknown key storage dataset mode"),
            };
            make_key_bytes(idx, len)
        })
        .collect()
}

fn build_keys<K: KeyTrait>(raw: &[Vec<u8>]) -> Vec<K> {
    raw.iter().map(|bytes| K::new_from_slice(bytes)).collect()
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct OverflowVectorPartialKey<const N: usize> {
    inner: OverflowKey<N>,
}

impl<const N: usize> AsRef<[u8]> for OverflowVectorPartialKey<N> {
    fn as_ref(&self) -> &[u8] {
        self.inner.as_ref()
    }
}

impl<const N: usize> KeyTrait for OverflowVectorPartialKey<N> {
    type PartialType = VectorPartial;
    const MAXIMUM_SIZE: Option<usize> = None;

    fn new_from_slice(slice: &[u8]) -> Self {
        Self {
            inner: OverflowKey::new_from_slice(slice),
        }
    }

    fn new_from_partial(partial: &Self::PartialType) -> Self {
        Self::new_from_slice(partial.to_slice())
    }

    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self {
        let mut data = Vec::with_capacity(self.inner.length_at(0) + partial.len());
        data.extend_from_slice(self.inner.as_ref());
        data.extend_from_slice(partial.to_slice());
        Self::new_from_slice(&data)
    }

    fn truncate(&self, at_depth: usize) -> Self {
        Self::new_from_slice(&self.inner.as_ref()[..at_depth])
    }

    #[inline(always)]
    fn at(&self, pos: usize) -> u8 {
        self.inner.at(pos)
    }

    #[inline(always)]
    fn length_at(&self, at_depth: usize) -> usize {
        self.inner.length_at(at_depth)
    }

    fn to_partial(&self, at_depth: usize) -> Self::PartialType {
        VectorPartial::from_slice(&self.inner.as_ref()[at_depth..])
    }

    fn matches_slice(&self, slice: &[u8]) -> bool {
        self.inner.matches_slice(slice)
    }
}

impl<const N: usize> From<OverflowVectorPartialKey<N>> for VectorPartial {
    fn from(value: OverflowVectorPartialKey<N>) -> Self {
        value.to_partial(0)
    }
}

struct RawKeyContext<const MODE: usize> {
    raw: Vec<Vec<u8>>,
}

impl<const MODE: usize> BenchContext for RawKeyContext<MODE> {
    fn prepare(_num_chunks: usize) -> Self {
        Self {
            raw: make_dataset::<MODE>(),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(DATASET_SIZE)
    }

    fn operations_per_chunk() -> Option<u64> {
        Some(DATASET_SIZE as u64)
    }
}

struct KeyStorageContext<K: KeyTrait, const MODE: usize> {
    keys: Vec<K>,
    partials: Vec<K::PartialType>,
    tree: AdaptiveRadixTree<K, usize>,
}

impl<K: KeyTrait, const MODE: usize> BenchContext for KeyStorageContext<K, MODE> {
    fn prepare(_num_chunks: usize) -> Self {
        let raw = make_dataset::<MODE>();
        let keys = build_keys::<K>(&raw);
        let partials = keys.iter().map(|key| key.to_partial(0)).collect();
        let mut tree = AdaptiveRadixTree::<K, usize>::new();
        for (idx, key) in keys.iter().enumerate() {
            tree.insert_k(key, idx);
        }
        Self {
            keys,
            partials,
            tree,
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(DATASET_SIZE)
    }

    fn operations_per_chunk() -> Option<u64> {
        Some(DATASET_SIZE as u64)
    }
}

fn bench_construct<K: KeyTrait, const MODE: usize>(
    ctx: &mut RawKeyContext<MODE>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    let mut keys = Vec::with_capacity(chunk_size);
    for bytes in ctx.raw.iter().take(chunk_size) {
        keys.push(K::new_from_slice(bytes));
    }
    black_box(keys);
}

fn bench_insert<K: KeyTrait, const MODE: usize>(
    ctx: &mut KeyStorageContext<K, MODE>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    let mut tree = AdaptiveRadixTree::<K, usize>::new();
    for (idx, key) in ctx.keys.iter().take(chunk_size).enumerate() {
        tree.insert_k(key, idx);
    }
    black_box(tree);
}

fn bench_get<K: KeyTrait, const MODE: usize>(
    ctx: &mut KeyStorageContext<K, MODE>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for key in ctx.keys.iter().take(chunk_size) {
        sum = sum.wrapping_add(*ctx.tree.get_k(key).unwrap());
    }
    black_box(sum);
}

fn bench_iter<K: KeyTrait, const MODE: usize>(
    ctx: &mut KeyStorageContext<K, MODE>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for (key, value) in ctx.tree.iter() {
        sum = sum.wrapping_add(key.as_ref().len()).wrapping_add(*value);
    }
    black_box(sum);
}

fn bench_at_scan<K: KeyTrait, const MODE: usize>(
    ctx: &mut KeyStorageContext<K, MODE>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for key in ctx.keys.iter().take(chunk_size) {
        for pos in 0..key.length_at(0) {
            sum = sum.wrapping_add(key.at(pos) as usize);
        }
    }
    black_box(sum);
}

fn bench_prefix_key<K: KeyTrait, const MODE: usize>(
    ctx: &mut KeyStorageContext<K, MODE>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for (partial, key) in ctx.partials.iter().zip(ctx.keys.iter()).take(chunk_size) {
        sum = sum.wrapping_add(partial.prefix_length_key(key, 0));
    }
    black_box(sum);
}

macro_rules! register_dataset {
    ($runner:ident, $mode:literal, $name:literal) => {
        $runner.group::<RawKeyContext<$mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_construct"),
                bench_construct::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_construct"),
                bench_construct::<OverflowKey<INLINE>, $mode>,
            );
        });

        $runner.group::<KeyStorageContext<VectorKey, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_insert"),
                bench_insert::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys"))
                .bench(concat!($name, "/vector_get"), bench_get::<VectorKey, $mode>);
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_iter"),
                bench_iter::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_at_scan"),
                bench_at_scan::<VectorKey, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/vector_prefix_key"),
                bench_prefix_key::<VectorKey, $mode>,
            );
        });

        $runner.group::<KeyStorageContext<OverflowKey<INLINE>, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_insert"),
                bench_insert::<OverflowKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_get"),
                bench_get::<OverflowKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_iter"),
                bench_iter::<OverflowKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_at_scan"),
                bench_at_scan::<OverflowKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32_prefix_key"),
                bench_prefix_key::<OverflowKey<INLINE>, $mode>,
            );
        });

        $runner.group::<KeyStorageContext<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>>(
            $name,
            |g| {
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p8_insert"),
                    bench_insert::<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p8_get"),
                    bench_get::<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p8_iter"),
                    bench_iter::<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p8_at_scan"),
                    bench_at_scan::<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p8_prefix_key"),
                    bench_prefix_key::<OverflowKey<INLINE, PARTIAL_INLINE_SMALL>, $mode>,
                );
            },
        );

        $runner.group::<KeyStorageContext<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>>(
            $name,
            |g| {
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p16_insert"),
                    bench_insert::<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p16_get"),
                    bench_get::<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p16_iter"),
                    bench_iter::<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p16_at_scan"),
                    bench_at_scan::<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>,
                );
                g.throughput(Throughput::per_operation(1, "keys")).bench(
                    concat!($name, "/overflow32p16_prefix_key"),
                    bench_prefix_key::<OverflowKey<INLINE, PARTIAL_INLINE_MEDIUM>, $mode>,
                );
            },
        );

        $runner.group::<KeyStorageContext<OverflowVectorPartialKey<INLINE>, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32vpartial_insert"),
                bench_insert::<OverflowVectorPartialKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32vpartial_get"),
                bench_get::<OverflowVectorPartialKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32vpartial_iter"),
                bench_iter::<OverflowVectorPartialKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32vpartial_at_scan"),
                bench_at_scan::<OverflowVectorPartialKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/overflow32vpartial_prefix_key"),
                bench_prefix_key::<OverflowVectorPartialKey<INLINE>, $mode>,
            );
        });
    };
}

macro_rules! register_array_dataset {
    ($runner:ident, $mode:literal, $name:literal) => {
        $runner.group::<RawKeyContext<$mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_construct"),
                bench_construct::<ArrayKey<INLINE>, $mode>,
            );
        });

        $runner.group::<KeyStorageContext<ArrayKey<INLINE>, $mode>>($name, |g| {
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_insert"),
                bench_insert::<ArrayKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_get"),
                bench_get::<ArrayKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_iter"),
                bench_iter::<ArrayKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_at_scan"),
                bench_at_scan::<ArrayKey<INLINE>, $mode>,
            );
            g.throughput(Throughput::per_operation(1, "keys")).bench(
                concat!($name, "/array32_prefix_key"),
                bench_prefix_key::<ArrayKey<INLINE>, $mode>,
            );
        });
    };
}

benchmark_main!(|runner| {
    runner.set_runtime(runtime_options());

    register_array_dataset!(runner, 0, "short8");
    register_dataset!(runner, 0, "short8");
    register_array_dataset!(runner, 1, "at_inline32");
    register_dataset!(runner, 1, "at_inline32");
    register_dataset!(runner, 2, "long96");
    register_dataset!(runner, 3, "mixed90_short");
    register_dataset!(runner, 4, "mixed50_random");
    register_dataset!(runner, 5, "common_prefix48");
});
