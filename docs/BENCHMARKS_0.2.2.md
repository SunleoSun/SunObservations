# Version 0.2.2 benchmark refresh

These are measurements of the 0.2.2 release candidate, not performance gates or a
controlled comparison with 0.2.1. Compilation before/after evidence is recorded
separately in [COMPILATION_PERFORMANCE.md](COMPILATION_PERFORMANCE.md).

## Environment and protocol

- Windows 11 build 26200, AMD Ryzen 9 5950X, 32 logical CPUs, 64 GiB RAM.
- rustc 1.96.0 MSVC, LLVM 22.1.2; ordinary release profile, no native-CPU override.
- No CPU pinning, host isolation, or inherited Cargo/Rayon profile overrides.
- Each capture, certification, and runtime configuration retains samples 0, 1, 2.
  Charts use their medians. Timings are elapsed time, not CPU cycles.
- Generative measurements retain the existing sample-0 selection protocol: each
  row is the mean of five operations, not the median of all three samples.
  The measurement script verifies all 999 raw rows, 22 operation families and
  333 configurations before selecting the 70 tracked rows.
- Buffered file delivery includes the engine flush barrier where labelled, not
  fsync or a durable-storage guarantee. Async queues remain unbounded.

## Commands

Run from the repository root:

~~~sh
python scripts/measure_generative_certifications.py --summary
cargo test --locked --release -p sun-observations --bench capture -- --measure
cargo test --locked --release -p sun-observations --bench certifications -- --measure
cargo test --locked --release -p sun-observations --bench runtime -- --measure
python scripts/render_benchmark_charts.py
python scripts/render_benchmark_charts.py --check
~~~

The release run used a separate target directory on D:; it did not clean or
relocate the configured workspace cache. Full native benchmark reports and
command outputs were retained in the external release evidence directory.

## Release runtime medians

`scripts/runtime_metrics.csv` retains all 159 runtime rows.
`scripts/benchmark_metrics.csv` derives 30 refreshed chart rows from that dataset:

| Chart | Selection | Metric |
| --- | --- | --- |
| hot-path | hot, five scenarios | producer ns/call |
| selector-scale | selectors_warm, 0/10/100/1000/10000 irrelevant rules | producer ns/call |
| jsonl-scaling | threads/jsonl, 1/2/4/8/16 producers | producer and through-flush ns/call |
| sink-comparison | sinks, five topologies | through-flush ns/call |

Rejected hot-path medians are 8.328 ns inactive, 8.372 ns excluded, and
8.453 ns Never. These are not paired regression measurements against the
historical chart values.

## Capture, prepared evidence and typed analysis

`scripts/capture_metrics.csv` retains 156 rows, including emission, full capture
execution, storage controls and prepared correspondence reuse at 1k/10k/100k/1M
records. At 1M rows, six requests take 113.5945 ms with one preparation and
347.7923 ms with repeated preparation. The capture chart times emission inside
the captured operation; full capture execution is a separate measurement.

`scripts/certification_metrics.csv` retains 336 rows from the current public API
benchmark. The old dataset had 348 rows: legacy alignment operations have been
replaced by canonical comparison operations; the separate u64 alignment helper
is no longer an API. The current simple comparison already uses a u64 key.
No production certification cases or budgets were reduced.

The benchmark fixture now supplies a valid millisecond-precision FORMAT v1
timestamp before ingestion. An empty timestamp failed current validation before
any timing rows; production validation was not relaxed.

## Historical comparisons retained

Writer-batch, human-structured and console rows in `benchmark_metrics.csv` are
unchanged historical evidence, not new 0.2.2 measurements. In particular, the
before/after comparisons were not reconstructed by mixing historical baselines
with fresh final measurements. Original source labels are retained as historical
provenance; those old report paths need not exist in the current checkout.

All 14 SVGs are rendered by the existing deterministic renderer. Ten charts
consume refreshed data; four historical charts keep their measurements. The
prepared-reuse annotation now derives its numbers from the CSV medians instead
of keeping an independent hardcoded copy.

The client API and portable skills did not change. Package README rebasing and
canonical skill/mirror equality remain release validation requirements.
