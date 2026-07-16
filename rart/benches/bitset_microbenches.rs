use std::time::Duration;

use micromeasure::{
    BenchContext, BenchmarkMainOptions, BenchmarkRunner, BenchmarkRuntimeOptions, Throughput,
    benchmark_main, black_box,
};

use rart::utils::bitset::{Bitset8, Bitset16, Bitset32, Bitset64, BitsetTrait};

type Bitset64x1 = Bitset64<1>;
type Bitset64x4 = Bitset64<4>;
type Bitset16x3 = Bitset16<3>;
type Bitset8x6 = Bitset8<6>;
type Bitset32x8 = Bitset32<8>;
type Bitset16x16 = Bitset16<16>;

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn runtime_options() -> BenchmarkRuntimeOptions {
    if full_bench_profile() {
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

fn options() -> BenchmarkMainOptions {
    BenchmarkMainOptions {
        suite: Some("rart-bitset".to_string()),
        runtime: runtime_options(),
        ..BenchmarkMainOptions::default()
    }
}

fn microbench_chunk_size() -> usize {
    if full_bench_profile() { 1024 } else { 128 }
}

fn make_unique_positions(capacity: usize, count: usize, seed: usize) -> Vec<usize> {
    let count = count.min(capacity);
    let mut seen = vec![false; capacity];
    let mut positions = Vec::with_capacity(count);
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15usize) | 1;

    while positions.len() < count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let pos = state % capacity;
        if !seen[pos] {
            seen[pos] = true;
            positions.push(pos);
        }
    }

    positions.sort_unstable();
    positions
}

fn build_bitset<Bitset: BitsetTrait + Default>(set_bits: usize, seed: usize) -> Bitset {
    let mut bitset = Bitset::default();
    for pos in make_unique_positions(Bitset::BITSET_WIDTH, set_bits, seed) {
        bitset.set(pos);
    }
    bitset
}

fn first_clear_position<Bitset: BitsetTrait + Default>(set_bits: usize, seed: usize) -> usize {
    let positions = make_unique_positions(Bitset::BITSET_WIDTH, set_bits, seed);
    let mut iter = positions.into_iter();
    let mut next = iter.next();

    for pos in 0..Bitset::BITSET_WIDTH {
        match next {
            Some(set_pos) if set_pos == pos => next = iter.next(),
            _ => return pos,
        }
    }

    0
}

struct ReadProbe<Bitset: BitsetTrait + Default> {
    bitset: Bitset,
    hit_pos: usize,
}

struct RoundTripProbe<Bitset: BitsetTrait + Default> {
    bitset: Bitset,
    toggle_pos: usize,
    starts_set: bool,
}

struct ReadContext<Bitset: BitsetTrait + Default, const SET_BITS: usize> {
    probes: Vec<ReadProbe<Bitset>>,
}

struct RoundTripContext<Bitset: BitsetTrait + Default, const SET_BITS: usize> {
    probes: Vec<RoundTripProbe<Bitset>>,
}

impl<Bitset: BitsetTrait + Default, const SET_BITS: usize> BenchContext
    for ReadContext<Bitset, SET_BITS>
{
    fn prepare(num_chunks: usize) -> Self {
        let probes = (0..num_chunks)
            .map(|idx| {
                let bitset = build_bitset::<Bitset>(SET_BITS, idx + 1);
                let hit_pos = if SET_BITS == 0 {
                    0
                } else {
                    make_unique_positions(Bitset::BITSET_WIDTH, SET_BITS, idx + 1)[SET_BITS / 2]
                };
                ReadProbe { bitset, hit_pos }
            })
            .collect();
        Self { probes }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<Bitset: BitsetTrait + Default, const SET_BITS: usize> BenchContext
    for RoundTripContext<Bitset, SET_BITS>
{
    fn prepare(num_chunks: usize) -> Self {
        let probes = (0..num_chunks)
            .map(|idx| {
                let bitset = build_bitset::<Bitset>(SET_BITS, idx + 17);
                let toggle_pos = first_clear_position::<Bitset>(SET_BITS, idx + 17);
                RoundTripProbe {
                    bitset,
                    toggle_pos,
                    starts_set: SET_BITS == Bitset::BITSET_WIDTH,
                }
            })
            .collect();
        Self { probes }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

trait IterBenchSupport: BitsetTrait {
    fn iter_sum(&self) -> usize;
}

macro_rules! impl_iter_bench_support {
    ($($bitset:ty),* $(,)?) => {
        $(
            impl IterBenchSupport for $bitset {
                fn iter_sum(&self) -> usize {
                    self.iter().fold(0usize, |acc, pos| acc.wrapping_add(pos))
                }
            }
        )*
    };
}

impl_iter_bench_support!(
    Bitset64x1,
    Bitset64x4,
    Bitset16x3,
    Bitset8x6,
    Bitset32x8,
    Bitset16x16,
);

fn bench_first_empty<Bitset: BitsetTrait + Default, const SET_BITS: usize>(
    ctx: &mut ReadContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter().take(chunk_size) {
        black_box(probe.bitset.first_empty());
    }
}

fn bench_first_set<Bitset: BitsetTrait + Default, const SET_BITS: usize>(
    ctx: &mut ReadContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter().take(chunk_size) {
        black_box(probe.bitset.first_set());
    }
}

fn bench_size<Bitset: BitsetTrait + Default, const SET_BITS: usize>(
    ctx: &mut ReadContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter().take(chunk_size) {
        black_box(probe.bitset.size());
    }
}

fn bench_check_hit<Bitset: BitsetTrait + Default, const SET_BITS: usize>(
    ctx: &mut ReadContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter().take(chunk_size) {
        black_box(probe.bitset.check(probe.hit_pos));
    }
}

fn bench_iter_sum<Bitset: IterBenchSupport + Default, const SET_BITS: usize>(
    ctx: &mut ReadContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter().take(chunk_size) {
        black_box(probe.bitset.iter_sum());
    }
}

fn bench_set_unset_roundtrip<Bitset: BitsetTrait + Default, const SET_BITS: usize>(
    ctx: &mut RoundTripContext<Bitset, SET_BITS>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for probe in ctx.probes.iter_mut().take(chunk_size) {
        if probe.starts_set {
            probe.bitset.unset(probe.toggle_pos);
            black_box(probe.bitset.check(probe.toggle_pos));
            probe.bitset.set(probe.toggle_pos);
        } else {
            probe.bitset.set(probe.toggle_pos);
            black_box(probe.bitset.check(probe.toggle_pos));
            probe.bitset.unset(probe.toggle_pos);
        }
    }
}

fn register_case<Bitset: IterBenchSupport + Default, const SET_BITS: usize>(
    runner: &BenchmarkRunner,
    type_name: &'static str,
    case_name: &'static str,
) {
    runner.group::<ReadContext<Bitset, SET_BITS>>("bitset_first_empty", |g| {
        g.throughput(Throughput::per_operation(
            Bitset::BITSET_WIDTH as u64,
            "bits",
        ))
        .bench(
            &format!("{type_name}_{case_name}"),
            bench_first_empty::<Bitset, SET_BITS>,
        );
    });
    runner.group::<ReadContext<Bitset, SET_BITS>>("bitset_first_set", |g| {
        g.throughput(Throughput::per_operation(
            Bitset::BITSET_WIDTH as u64,
            "bits",
        ))
        .bench(
            &format!("{type_name}_{case_name}"),
            bench_first_set::<Bitset, SET_BITS>,
        );
    });
    runner.group::<ReadContext<Bitset, SET_BITS>>("bitset_size", |g| {
        g.throughput(Throughput::per_operation(
            Bitset::BITSET_WIDTH as u64,
            "bits",
        ))
        .bench(
            &format!("{type_name}_{case_name}"),
            bench_size::<Bitset, SET_BITS>,
        );
    });
    if SET_BITS > 0 {
        runner.group::<ReadContext<Bitset, SET_BITS>>("bitset_check_hit", |g| {
            g.throughput(Throughput::per_operation(1, "probe")).bench(
                &format!("{type_name}_{case_name}"),
                bench_check_hit::<Bitset, SET_BITS>,
            );
        });
    }
    runner.group::<ReadContext<Bitset, SET_BITS>>("bitset_iter_sum", |g| {
        g.throughput(Throughput::per_operation(
            SET_BITS.max(1) as u64,
            "set-bits",
        ))
        .bench(
            &format!("{type_name}_{case_name}"),
            bench_iter_sum::<Bitset, SET_BITS>,
        );
    });
    runner.group::<RoundTripContext<Bitset, SET_BITS>>("bitset_set_unset_roundtrip", |g| {
        g.throughput(Throughput::per_operation(1, "toggle")).bench(
            &format!("{type_name}_{case_name}"),
            bench_set_unset_roundtrip::<Bitset, SET_BITS>,
        );
    });
}

fn register_bitset64x1_benches(runner: &BenchmarkRunner) {
    register_case::<Bitset64x1, 0>(runner, "u64x1", "empty");
    register_case::<Bitset64x1, 1>(runner, "u64x1", "one");
    register_case::<Bitset64x1, 8>(runner, "u64x1", "eight");
    register_case::<Bitset64x1, 32>(runner, "u64x1", "half");
    register_case::<Bitset64x1, 63>(runner, "u64x1", "dense");
    register_case::<Bitset64x1, 64>(runner, "u64x1", "full");
}

fn register_bitset48_alternate_width_benches(runner: &BenchmarkRunner) {
    register_case::<Bitset16x3, 0>(runner, "u16x3", "empty");
    register_case::<Bitset16x3, 1>(runner, "u16x3", "one");
    register_case::<Bitset16x3, 8>(runner, "u16x3", "eight");
    register_case::<Bitset16x3, 24>(runner, "u16x3", "half");
    register_case::<Bitset16x3, 47>(runner, "u16x3", "dense");
    register_case::<Bitset16x3, 48>(runner, "u16x3", "full");

    register_case::<Bitset8x6, 0>(runner, "u8x6", "empty");
    register_case::<Bitset8x6, 1>(runner, "u8x6", "one");
    register_case::<Bitset8x6, 8>(runner, "u8x6", "eight");
    register_case::<Bitset8x6, 24>(runner, "u8x6", "half");
    register_case::<Bitset8x6, 47>(runner, "u8x6", "dense");
    register_case::<Bitset8x6, 48>(runner, "u8x6", "full");
}

fn register_bitset64x4_benches(runner: &BenchmarkRunner) {
    register_case::<Bitset64x4, 0>(runner, "u64x4", "empty");
    register_case::<Bitset64x4, 1>(runner, "u64x4", "one");
    register_case::<Bitset64x4, 48>(runner, "u64x4", "forty_eight");
    register_case::<Bitset64x4, 64>(runner, "u64x4", "sixty_four");
    register_case::<Bitset64x4, 128>(runner, "u64x4", "half");
    register_case::<Bitset64x4, 192>(runner, "u64x4", "dense");
    register_case::<Bitset64x4, 255>(runner, "u64x4", "near_full");
    register_case::<Bitset64x4, 256>(runner, "u64x4", "full");
}

fn register_bitset256_alternate_width_benches(runner: &BenchmarkRunner) {
    register_case::<Bitset32x8, 0>(runner, "u32x8", "empty");
    register_case::<Bitset32x8, 1>(runner, "u32x8", "one");
    register_case::<Bitset32x8, 48>(runner, "u32x8", "forty_eight");
    register_case::<Bitset32x8, 64>(runner, "u32x8", "sixty_four");
    register_case::<Bitset32x8, 128>(runner, "u32x8", "half");
    register_case::<Bitset32x8, 192>(runner, "u32x8", "dense");
    register_case::<Bitset32x8, 255>(runner, "u32x8", "near_full");
    register_case::<Bitset32x8, 256>(runner, "u32x8", "full");

    register_case::<Bitset16x16, 0>(runner, "u16x16", "empty");
    register_case::<Bitset16x16, 1>(runner, "u16x16", "one");
    register_case::<Bitset16x16, 48>(runner, "u16x16", "forty_eight");
    register_case::<Bitset16x16, 64>(runner, "u16x16", "sixty_four");
    register_case::<Bitset16x16, 128>(runner, "u16x16", "half");
    register_case::<Bitset16x16, 192>(runner, "u16x16", "dense");
    register_case::<Bitset16x16, 255>(runner, "u16x16", "near_full");
    register_case::<Bitset16x16, 256>(runner, "u16x16", "full");
}

benchmark_main!(options(), |runner| {
    register_bitset64x1_benches(runner);
    register_bitset48_alternate_width_benches(runner);
    register_bitset64x4_benches(runner);
    register_bitset256_alternate_width_benches(runner);
});
