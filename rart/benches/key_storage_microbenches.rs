mod support;

use micromeasure::{BenchmarkMainOptions, BenchmarkRunner, Throughput, benchmark_main};

use rart::keys::KeyTrait;
use rart::{ArrayKey, OverflowKey, VectorKey};

use support::key_storage::{
    ConstructContext, EncodedBuildContext, OverflowVectorPartialKey, PrebuiltKeysContext,
    SCENARIOS, Scenario, TreeContext, bench_at_scan, bench_construct, bench_iter_owned,
    bench_iter_view, bench_lookup_all, bench_prefix_key, bench_tree_build_encoded,
    bench_tree_build_prebuilt, runtime_options,
};

const INLINE: usize = 32;

fn options() -> BenchmarkMainOptions {
    BenchmarkMainOptions {
        suite: Some("rart-key-storage".to_string()),
        filter_help: Some(
            "scenario (short8, at_inline32, long96, mixed90_short, mixed50_random, \
             common_prefix48), representation, or operation"
                .to_string(),
        ),
        runtime: runtime_options(),
        ..BenchmarkMainOptions::default()
    }
}

fn register_representation<K: KeyTrait>(
    runner: &BenchmarkRunner,
    scenario: Scenario,
    representation: &'static str,
) {
    let construct_factory = || ConstructContext::<K>::new(scenario);
    let construct_name = format!("{}/{representation}/construct", scenario.name);
    runner.group::<ConstructContext<K>>(scenario.name, |g| {
        g.throughput(Throughput::per_operation(1, "keys"))
            .factory(&construct_factory)
            .bench(&construct_name, bench_construct::<K>);
    });

    let prebuilt_factory = || PrebuiltKeysContext::<K>::new(scenario);
    let prebuilt_name = format!("{}/{representation}/tree_build_prebuilt", scenario.name);
    runner.group::<PrebuiltKeysContext<K>>(scenario.name, |g| {
        g.throughput(Throughput::per_operation(1, "keys"))
            .factory(&prebuilt_factory)
            .bench(&prebuilt_name, bench_tree_build_prebuilt::<K>);
    });

    let encoded_factory = || EncodedBuildContext::<K>::new(scenario);
    let encoded_name = format!("{}/{representation}/tree_build_encoded", scenario.name);
    runner.group::<EncodedBuildContext<K>>(scenario.name, |g| {
        g.throughput(Throughput::per_operation(1, "keys"))
            .factory(&encoded_factory)
            .bench(&encoded_name, bench_tree_build_encoded::<K>);
    });

    let tree_factory = || TreeContext::<K>::new(scenario);
    runner.group::<TreeContext<K>>(scenario.name, |g| {
        let g = g
            .throughput(Throughput::per_operation(1, "keys"))
            .factory(&tree_factory);
        g.bench(
            &format!("{}/{representation}/lookup_all", scenario.name),
            bench_lookup_all::<K>,
        );
        g.bench(
            &format!("{}/{representation}/iter_owned", scenario.name),
            bench_iter_owned::<K>,
        );
        g.bench(
            &format!("{}/{representation}/iter_view", scenario.name),
            bench_iter_view::<K>,
        );
        g.bench(
            &format!("{}/{representation}/at_scan", scenario.name),
            bench_at_scan::<K>,
        );
        g.bench(
            &format!("{}/{representation}/prefix_key", scenario.name),
            bench_prefix_key::<K>,
        );
    });
}

fn register_scenario(runner: &BenchmarkRunner, scenario: Scenario) {
    if scenario.supports_array_32() {
        register_representation::<ArrayKey<INLINE>>(runner, scenario, "array32");
    }
    register_representation::<VectorKey>(runner, scenario, "vector");
    register_representation::<OverflowKey<INLINE>>(runner, scenario, "overflow32");
    register_representation::<OverflowKey<INLINE, 8>>(runner, scenario, "overflow32p8");
    register_representation::<OverflowKey<INLINE, 16>>(runner, scenario, "overflow32p16");
    register_representation::<OverflowVectorPartialKey<INLINE>>(
        runner,
        scenario,
        "overflow32vpartial",
    );
}

benchmark_main!(options(), |runner| {
    for &scenario in SCENARIOS {
        register_scenario(runner, scenario);
    }
});
