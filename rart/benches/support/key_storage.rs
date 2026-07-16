use std::time::Duration;

use micromeasure::{BenchContext, BenchmarkRuntimeOptions, black_box};

use rart::keys::KeyTrait;
use rart::keys::overflow_key::OverflowKey;
use rart::partials::Partial;
use rart::partials::vector_partial::VectorPartial;
use rart::tree::AdaptiveRadixTree;

const DEFAULT_DATASET_SIZE: usize = 1 << 12;
const FULL_DATASET_SIZE: usize = 1 << 15;

#[derive(Clone, Copy)]
enum KeyShape {
    Fixed(usize),
    Mixed90Short,
    Mixed50Random,
    CommonPrefix(usize),
}

#[derive(Clone, Copy)]
pub struct Scenario {
    pub name: &'static str,
    shape: KeyShape,
}

impl Scenario {
    const fn fixed(name: &'static str, len: usize) -> Self {
        Self {
            name,
            shape: KeyShape::Fixed(len),
        }
    }

    const fn mixed_90_short(name: &'static str) -> Self {
        Self {
            name,
            shape: KeyShape::Mixed90Short,
        }
    }

    const fn mixed_50_random(name: &'static str) -> Self {
        Self {
            name,
            shape: KeyShape::Mixed50Random,
        }
    }

    const fn common_prefix(name: &'static str, len: usize) -> Self {
        Self {
            name,
            shape: KeyShape::CommonPrefix(len),
        }
    }

    pub fn supports_array_32(self) -> bool {
        matches!(self.shape, KeyShape::Fixed(len) if len <= 32)
    }

    fn make_dataset(self) -> Vec<Vec<u8>> {
        (0..dataset_size())
            .map(|idx| match self.shape {
                KeyShape::Fixed(len) => make_key_bytes(idx, len),
                KeyShape::Mixed90Short => {
                    let len = if idx % 10 == 0 { 96 } else { 12 };
                    make_key_bytes(idx, len)
                }
                KeyShape::Mixed50Random => {
                    let mixed = idx.wrapping_mul(0x9e37_79b1) ^ idx.rotate_left(13);
                    let len = if mixed & 1 == 0 { 12 } else { 96 };
                    make_key_bytes(idx, len)
                }
                KeyShape::CommonPrefix(len) => make_common_prefix_key(idx, len),
            })
            .collect()
    }
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario::fixed("short8", 8),
    Scenario::fixed("at_inline32", 32),
    Scenario::fixed("long96", 96),
    Scenario::mixed_90_short("mixed90_short"),
    Scenario::mixed_50_random("mixed50_random"),
    Scenario::common_prefix("common_prefix48", 48),
];

pub fn dataset_size() -> usize {
    if std::env::var("RART_BENCH_FULL").as_deref() == Ok("1") {
        FULL_DATASET_SIZE
    } else {
        DEFAULT_DATASET_SIZE
    }
}

pub fn runtime_options() -> BenchmarkRuntimeOptions {
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

fn build_keys<K: KeyTrait>(raw: &[Vec<u8>]) -> Vec<K> {
    raw.iter().map(|bytes| K::new_from_slice(bytes)).collect()
}

fn build_tree<K: KeyTrait>(keys: &[K]) -> AdaptiveRadixTree<K, usize> {
    let mut tree = AdaptiveRadixTree::new();
    for (idx, key) in keys.iter().enumerate() {
        tree.insert_k(key, idx);
    }
    tree
}

trait ScenarioBenchContext: Sized {
    fn unsupported_prepare() -> Self {
        panic!("key-storage benchmark contexts require a scenario factory")
    }

    fn chunk_size() -> Option<usize> {
        Some(1)
    }

    fn operations_per_chunk() -> Option<u64> {
        Some(dataset_size() as u64)
    }
}

pub struct ConstructContext<K: KeyTrait> {
    raw: Vec<Vec<u8>>,
    output: Option<Vec<K>>,
}

impl<K: KeyTrait> ConstructContext<K> {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            raw: scenario.make_dataset(),
            output: None,
        }
    }
}

impl<K: KeyTrait> ScenarioBenchContext for ConstructContext<K> {}

impl<K: KeyTrait> BenchContext for ConstructContext<K> {
    fn prepare(_chunk_size: usize) -> Self {
        Self::unsupported_prepare()
    }

    fn chunk_size() -> Option<usize> {
        <Self as ScenarioBenchContext>::chunk_size()
    }

    fn operations_per_chunk() -> Option<u64> {
        <Self as ScenarioBenchContext>::operations_per_chunk()
    }
}

pub struct PrebuiltKeysContext<K: KeyTrait> {
    keys: Vec<K>,
    output: Option<AdaptiveRadixTree<K, usize>>,
}

impl<K: KeyTrait> PrebuiltKeysContext<K> {
    pub fn new(scenario: Scenario) -> Self {
        let raw = scenario.make_dataset();
        Self {
            keys: build_keys(&raw),
            output: None,
        }
    }
}

impl<K: KeyTrait> ScenarioBenchContext for PrebuiltKeysContext<K> {}

impl<K: KeyTrait> BenchContext for PrebuiltKeysContext<K> {
    fn prepare(_chunk_size: usize) -> Self {
        Self::unsupported_prepare()
    }

    fn chunk_size() -> Option<usize> {
        <Self as ScenarioBenchContext>::chunk_size()
    }

    fn operations_per_chunk() -> Option<u64> {
        <Self as ScenarioBenchContext>::operations_per_chunk()
    }
}

pub struct EncodedBuildContext<K: KeyTrait> {
    raw: Vec<Vec<u8>>,
    output: Option<AdaptiveRadixTree<K, usize>>,
}

impl<K: KeyTrait> EncodedBuildContext<K> {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            raw: scenario.make_dataset(),
            output: None,
        }
    }
}

impl<K: KeyTrait> ScenarioBenchContext for EncodedBuildContext<K> {}

impl<K: KeyTrait> BenchContext for EncodedBuildContext<K> {
    fn prepare(_chunk_size: usize) -> Self {
        Self::unsupported_prepare()
    }

    fn chunk_size() -> Option<usize> {
        <Self as ScenarioBenchContext>::chunk_size()
    }

    fn operations_per_chunk() -> Option<u64> {
        <Self as ScenarioBenchContext>::operations_per_chunk()
    }
}

pub struct TreeContext<K: KeyTrait> {
    keys: Vec<K>,
    partials: Vec<K::PartialType>,
    tree: AdaptiveRadixTree<K, usize>,
}

impl<K: KeyTrait> TreeContext<K> {
    pub fn new(scenario: Scenario) -> Self {
        let raw = scenario.make_dataset();
        let keys = build_keys::<K>(&raw);
        let partials = keys.iter().map(|key| key.to_partial(0)).collect();
        let tree = build_tree(&keys);
        Self {
            keys,
            partials,
            tree,
        }
    }
}

impl<K: KeyTrait> ScenarioBenchContext for TreeContext<K> {}

impl<K: KeyTrait> BenchContext for TreeContext<K> {
    fn prepare(_chunk_size: usize) -> Self {
        Self::unsupported_prepare()
    }

    fn chunk_size() -> Option<usize> {
        <Self as ScenarioBenchContext>::chunk_size()
    }

    fn operations_per_chunk() -> Option<u64> {
        <Self as ScenarioBenchContext>::operations_per_chunk()
    }
}

pub fn bench_construct<K: KeyTrait>(
    ctx: &mut ConstructContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let keys = build_keys::<K>(&ctx.raw);
    black_box(&keys);
    debug_assert!(ctx.output.is_none());
    ctx.output = Some(keys);
}

pub fn bench_tree_build_prebuilt<K: KeyTrait>(
    ctx: &mut PrebuiltKeysContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let tree = build_tree(&ctx.keys);
    black_box(&tree);
    debug_assert!(ctx.output.is_none());
    ctx.output = Some(tree);
}

pub fn bench_tree_build_encoded<K: KeyTrait>(
    ctx: &mut EncodedBuildContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut tree = AdaptiveRadixTree::new();
    for (idx, bytes) in ctx.raw.iter().enumerate() {
        let key = K::new_from_slice(bytes);
        tree.insert_k(&key, idx);
    }
    black_box(&tree);
    debug_assert!(ctx.output.is_none());
    ctx.output = Some(tree);
}

pub fn bench_lookup_all<K: KeyTrait>(
    ctx: &mut TreeContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for key in &ctx.keys {
        sum = sum.wrapping_add(*ctx.tree.get_k(key).unwrap());
    }
    black_box(sum);
}

pub fn bench_iter_owned<K: KeyTrait>(
    ctx: &mut TreeContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for (key, value) in ctx.tree.iter() {
        sum = sum.wrapping_add(key.as_ref().len()).wrapping_add(*value);
    }
    black_box(sum);
}

pub fn bench_iter_view<K: KeyTrait>(
    ctx: &mut TreeContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    ctx.tree.for_each_view(|key, value| {
        sum = sum.wrapping_add(key.len()).wrapping_add(*value);
    });
    black_box(sum);
}

pub fn bench_at_scan<K: KeyTrait>(ctx: &mut TreeContext<K>, _chunk_size: usize, _chunk_num: usize) {
    let mut sum = 0usize;
    for key in &ctx.keys {
        for pos in 0..key.length_at(0) {
            sum = sum.wrapping_add(key.at(pos) as usize);
        }
    }
    black_box(sum);
}

pub fn bench_prefix_key<K: KeyTrait>(
    ctx: &mut TreeContext<K>,
    _chunk_size: usize,
    _chunk_num: usize,
) {
    let mut sum = 0usize;
    for (partial, key) in ctx.partials.iter().zip(&ctx.keys) {
        sum = sum.wrapping_add(partial.prefix_length_key(key, 0));
    }
    black_box(sum);
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct OverflowVectorPartialKey<const N: usize> {
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
