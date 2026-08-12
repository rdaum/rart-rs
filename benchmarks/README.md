# Microbenchmark workflow

The focused CPU microbenchmarks use Micromeasure 0.15:

- `bitset_microbenches`
- `node_mapping_microbenches`
- `partial_prefix_microbenches`
- `key_storage_microbenches`
- `versioned_tree_microbenches`

Run one suite, optionally with a substring filter, from the workspace root:

```sh
cargo bench -p rart --bench node_mapping_microbenches -- sorted_key_search
```

`RART_BENCH_FULL=1` selects the longer rart workload profile. The key-storage
suite also accepts `RART_BENCH_QUICK=1` for smoke runs.

## Measurement configuration

Every suite uses the same Micromeasure backend configuration. Defaults retain
the historical full CPU PMU profile, opt out of RAPL energy, and allow the
framework's best-effort automatic IMC bandwidth probe.

| Variable | Values | Default | Effect |
|---|---|---|---|
| `RART_BENCH_PMU` | `full`, `compact`, `none` | `full` | Select all CPU counters, the non-multiplexing-friendly four-counter profile, or timing only. |
| `RART_BENCH_RAPL` | `off`, `package`, `package-core` | `off` | Add gross system-wide package energy, optionally including per-core domains. |
| `RART_BENCH_MEMORY_BANDWIDTH` | `auto`, `requested`, `off` | `auto` | Probe quietly, explicitly request IMC bandwidth diagnostics, or skip the probe. |

For example, a compact-counter run that avoids global IMC measurements is:

```sh
RART_BENCH_PMU=compact \
RART_BENCH_MEMORY_BANDWIDTH=off \
cargo bench -p rart --bench bitset_microbenches -- bitset_check_hit
```

RAPL and IMC values are system-wide rather than process-attributed. Use a
quiet machine and sufficiently long samples before treating them as evidence.
Micromeasure persists the selected measurement scopes in each result, so runs
with incompatible PMU, energy, or bandwidth settings are not compared.

## Reproducible reports and comparisons

Micromeasure 0.15's launcher accepts an explicit context, baseline, and output
path without benchmark-specific code:

```sh
MICROMEASURE_CONTEXT_FILE=benchmark-context.json \
MICROMEASURE_BASELINE=artifacts/baseline/node-mapping.json \
MICROMEASURE_OUTPUT=artifacts/current/node-mapping.json \
cargo bench -p rart --bench node_mapping_microbenches
```

The context captures comparison dimensions such as commit, compiler, or host
configuration. An explicit incompatible or malformed baseline fails rather
than silently falling back to an unrelated local result.

Micromeasure 0.15 requires Rust 1.95 or newer to build these development-only
bench targets. The `rart` library's published Rust version remains independent
of that benchmark-tooling requirement.
