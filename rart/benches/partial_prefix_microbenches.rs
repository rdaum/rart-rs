use std::time::Duration;

use micromeasure::{
    BenchContext, BenchmarkRunner, BenchmarkRuntimeOptions, Throughput, benchmark_main, black_box,
};

use rart::keys::KeyTrait;
use rart::keys::array_key::ArrayKey;
use rart::keys::vector_key::VectorKey;
use rart::partials::Partial;
use rart::partials::array_partial::ArrPartial;
use rart::partials::vector_partial::VectorPartial;

fn prefix_len_scalar(lhs: &[u8], rhs: &[u8]) -> usize {
    let len = lhs.len().min(rhs.len());
    let mut idx = 0;
    while idx < len {
        if lhs[idx] != rhs[idx] {
            break;
        }
        idx += 1;
    }
    idx
}

fn prefix_len_chunked_u64(lhs: &[u8], rhs: &[u8]) -> usize {
    let len = lhs.len().min(rhs.len());
    let mut idx = 0;

    while idx + 8 <= len {
        let lhs_word = u64::from_ne_bytes(lhs[idx..idx + 8].try_into().unwrap());
        let rhs_word = u64::from_ne_bytes(rhs[idx..idx + 8].try_into().unwrap());
        let diff = lhs_word ^ rhs_word;
        if diff != 0 {
            return idx + (diff.trailing_zeros() as usize / 8);
        }
        idx += 8;
    }

    while idx < len {
        if lhs[idx] != rhs[idx] {
            break;
        }
        idx += 1;
    }
    idx
}

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

fn microbench_chunk_size() -> usize {
    if full_bench_profile() { 1024 } else { 128 }
}

struct ArrPrefixContext<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize> {
    probes: Vec<(ArrPartial<SIZE>, ArrPartial<SIZE>, ArrayKey<SIZE>, Vec<u8>)>,
}

struct VectorPrefixContext<const LEN: usize, const MISMATCH_AT: usize> {
    probes: Vec<(VectorPartial, VectorPartial, VectorKey, Vec<u8>)>,
}

impl<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize> BenchContext
    for ArrPrefixContext<SIZE, LEN, MISMATCH_AT>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_arr_prefix_probes::<SIZE, LEN, MISMATCH_AT>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const LEN: usize, const MISMATCH_AT: usize> BenchContext
    for VectorPrefixContext<LEN, MISMATCH_AT>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_vector_prefix_probes::<LEN, MISMATCH_AT>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

fn make_bytes<const LEN: usize>(seed: usize) -> Vec<u8> {
    (0..LEN)
        .map(|idx| seed.wrapping_mul(17).wrapping_add(idx * 29) as u8)
        .collect()
}

fn make_matching_pair<const LEN: usize, const MISMATCH_AT: usize>(
    seed: usize,
) -> (Vec<u8>, Vec<u8>) {
    let lhs = make_bytes::<LEN>(seed);
    let mut rhs = lhs.clone();
    if MISMATCH_AT < LEN {
        rhs[MISMATCH_AT] = rhs[MISMATCH_AT].wrapping_add(1);
    }
    (lhs, rhs)
}

fn make_arr_prefix_probes<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize>(
    num_chunks: usize,
) -> Vec<(ArrPartial<SIZE>, ArrPartial<SIZE>, ArrayKey<SIZE>, Vec<u8>)> {
    debug_assert!(LEN <= SIZE);
    debug_assert!(MISMATCH_AT <= LEN);
    (0..num_chunks)
        .map(|idx| {
            let (lhs, rhs) = make_matching_pair::<LEN, MISMATCH_AT>(idx);
            (
                ArrPartial::from_slice(&lhs),
                ArrPartial::from_slice(&rhs),
                ArrayKey::new_from_slice(&rhs),
                rhs,
            )
        })
        .collect()
}

fn make_vector_prefix_probes<const LEN: usize, const MISMATCH_AT: usize>(
    num_chunks: usize,
) -> Vec<(VectorPartial, VectorPartial, VectorKey, Vec<u8>)> {
    debug_assert!(MISMATCH_AT <= LEN);
    (0..num_chunks)
        .map(|idx| {
            let (lhs, rhs) = make_matching_pair::<LEN, MISMATCH_AT>(idx);
            (
                VectorPartial::from_slice(&lhs),
                VectorPartial::from_slice(&rhs),
                VectorKey::new_from_slice(&rhs),
                rhs,
            )
        })
        .collect()
}

fn bench_arr_prefix_common<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut ArrPrefixContext<SIZE, LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, rhs, _, _) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_common(rhs));
    }
}

fn bench_arr_prefix_slice<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut ArrPrefixContext<SIZE, LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_slice(rhs_slice));
    }
}

fn bench_arr_prefix_key<const SIZE: usize, const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut ArrPrefixContext<SIZE, LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, key, _) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_key(key, 0));
    }
}

fn bench_vector_prefix_common<const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut VectorPrefixContext<LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, rhs, _, _) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_common(rhs));
    }
}

fn bench_vector_prefix_slice<const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut VectorPrefixContext<LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_slice(rhs_slice));
    }
}

fn bench_vector_prefix_key<const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut VectorPrefixContext<LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, key, _) in ctx.probes.iter().take(chunk_size) {
        black_box(lhs.prefix_length_key(key, 0));
    }
}

fn bench_arr_prefix_scalar_reference<
    const SIZE: usize,
    const LEN: usize,
    const MISMATCH_AT: usize,
>(
    ctx: &mut ArrPrefixContext<SIZE, LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(prefix_len_scalar(lhs.to_slice(), rhs_slice));
    }
}

fn bench_arr_prefix_chunked_reference<
    const SIZE: usize,
    const LEN: usize,
    const MISMATCH_AT: usize,
>(
    ctx: &mut ArrPrefixContext<SIZE, LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(prefix_len_chunked_u64(lhs.to_slice(), rhs_slice));
    }
}

fn bench_vector_prefix_scalar_reference<const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut VectorPrefixContext<LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(prefix_len_scalar(lhs.to_slice(), rhs_slice));
    }
}

fn bench_vector_prefix_chunked_reference<const LEN: usize, const MISMATCH_AT: usize>(
    ctx: &mut VectorPrefixContext<LEN, MISMATCH_AT>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (lhs, _, _, rhs_slice) in ctx.probes.iter().take(chunk_size) {
        black_box(prefix_len_chunked_u64(lhs.to_slice(), rhs_slice));
    }
}

fn mismatch_case_name<const LEN: usize, const MISMATCH_AT: usize>() -> &'static str {
    match (LEN, MISMATCH_AT) {
        (16, 0) => "len16_mismatch0",
        (16, 8) => "len16_mismatch8",
        (16, 15) => "len16_mismatch15",
        (16, 16) => "len16_full",
        (64, 0) => "len64_mismatch0",
        (64, 32) => "len64_mismatch32",
        (64, 63) => "len64_mismatch63",
        (64, 64) => "len64_full",
        _ => "unsupported",
    }
}

fn register_arr_case<const LEN: usize, const MISMATCH_AT: usize>(runner: &BenchmarkRunner) {
    let name = mismatch_case_name::<LEN, MISMATCH_AT>();
    runner.group::<ArrPrefixContext<64, LEN, MISMATCH_AT>>("arr_partial_prefix_common", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_arr_prefix_common::<64, LEN, MISMATCH_AT>);
    });
    runner.group::<ArrPrefixContext<64, LEN, MISMATCH_AT>>("arr_partial_prefix_slice", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_arr_prefix_slice::<64, LEN, MISMATCH_AT>);
    });
    runner.group::<ArrPrefixContext<64, LEN, MISMATCH_AT>>("arr_partial_prefix_key", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_arr_prefix_key::<64, LEN, MISMATCH_AT>);
    });
    runner.group::<ArrPrefixContext<64, LEN, MISMATCH_AT>>(
        "arr_partial_prefix_scalar_reference",
        |g| {
            g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
                .bench(
                    name,
                    bench_arr_prefix_scalar_reference::<64, LEN, MISMATCH_AT>,
                );
        },
    );
    runner.group::<ArrPrefixContext<64, LEN, MISMATCH_AT>>(
        "arr_partial_prefix_chunked_reference",
        |g| {
            g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
                .bench(
                    name,
                    bench_arr_prefix_chunked_reference::<64, LEN, MISMATCH_AT>,
                );
        },
    );
}

fn register_vector_case<const LEN: usize, const MISMATCH_AT: usize>(runner: &BenchmarkRunner) {
    let name = mismatch_case_name::<LEN, MISMATCH_AT>();
    runner.group::<VectorPrefixContext<LEN, MISMATCH_AT>>("vector_partial_prefix_common", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_vector_prefix_common::<LEN, MISMATCH_AT>);
    });
    runner.group::<VectorPrefixContext<LEN, MISMATCH_AT>>("vector_partial_prefix_slice", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_vector_prefix_slice::<LEN, MISMATCH_AT>);
    });
    runner.group::<VectorPrefixContext<LEN, MISMATCH_AT>>("vector_partial_prefix_key", |g| {
        g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
            .bench(name, bench_vector_prefix_key::<LEN, MISMATCH_AT>);
    });
    runner.group::<VectorPrefixContext<LEN, MISMATCH_AT>>(
        "vector_partial_prefix_scalar_reference",
        |g| {
            g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
                .bench(
                    name,
                    bench_vector_prefix_scalar_reference::<LEN, MISMATCH_AT>,
                );
        },
    );
    runner.group::<VectorPrefixContext<LEN, MISMATCH_AT>>(
        "vector_partial_prefix_chunked_reference",
        |g| {
            g.throughput(Throughput::per_operation(LEN as u64, "bytes"))
                .bench(
                    name,
                    bench_vector_prefix_chunked_reference::<LEN, MISMATCH_AT>,
                );
        },
    );
}

fn register_arr_prefix_benches(runner: &BenchmarkRunner) {
    register_arr_case::<16, 0>(runner);
    register_arr_case::<16, 8>(runner);
    register_arr_case::<16, 15>(runner);
    register_arr_case::<16, 16>(runner);
    register_arr_case::<64, 0>(runner);
    register_arr_case::<64, 32>(runner);
    register_arr_case::<64, 63>(runner);
    register_arr_case::<64, 64>(runner);
}

fn register_vector_prefix_benches(runner: &BenchmarkRunner) {
    register_vector_case::<16, 0>(runner);
    register_vector_case::<16, 8>(runner);
    register_vector_case::<16, 15>(runner);
    register_vector_case::<16, 16>(runner);
    register_vector_case::<64, 0>(runner);
    register_vector_case::<64, 32>(runner);
    register_vector_case::<64, 63>(runner);
    register_vector_case::<64, 64>(runner);
}

benchmark_main!(|runner| {
    runner.set_runtime(runtime_options());

    register_arr_prefix_benches(runner);
    register_vector_prefix_benches(runner);
});
