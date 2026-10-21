# Semantic certification benchmarks

The final release harness measures ordinary and Observation-backed sources, reusable alignment, generic analysis, criteria and bounded aggregation at **1k, 10k, 100k and 1M records**. Every cell is the median of three raw samples, in milliseconds. These are measurements, not acceptance thresholds.

These samples predate scoped capture and numeric-result caching. See [current capture and semantic-view measurements](CAPTURE_BENCHMARKS.md) for the final API; this historical dataset is preserved, not relabeled as a new run.

## Environment and reproduction

Reference host: AMD Ryzen 9 5950X, 64 GiB RAM, Windows 11/NTFS, MSVC target, Rust/Cargo 1.96.0, optimized release profile. The host was neither pinned nor isolated. Source fixtures are deterministic; timed boundaries are explicit in the harness. Decode is measured separately from subsequent typed-view operations; numeric-pair analyses reuse cached alignment. Fixture construction is outside the timed boundary unless it is the named preparation operation.

```text
cargo test -p sun-observations-data-source --release --bench certifications -- --measure
```

Harness: [certifications.rs](../crates/sun-observations-data-source/benches/certifications.rs). Raw evidence: [348 CSV samples](../scripts/certification_metrics.csv) and [complete final run](../scripts/certification_benchmark_run.txt), including command status and output. Without `--measure` the harness exits without timing work, so normal all-target validation does not run expensive benchmarks.

## Final measurements

| Operation (ms median) | 1k | 10k | 100k | 1M |
| --- | ---: | ---: | ---: | ---: |
| ordinary_source | 0.000200 | 0.000100 | 0.000200 | 0.000100 |
| observation_source | 0.000100 | 0.000100 | 0.000100 | 0.000100 |
| ordinary_numeric_projection | 0.000700 | 0.004100 | 0.124700 | 3.755800 |
| observation_numeric_projection | 0.000700 | 0.003900 | 0.061100 | 3.945000 |
| observation_decode | 0.078200 | 0.556000 | 16.933400 | 181.686100 |
| scalar_criteria_1000 | 0.210200 | 0.173600 | 0.181600 | 0.185600 |
| key_projection | 0.000900 | 0.005800 | 0.462700 | 7.580600 |
| prepared_keys | 0.033900 | 0.307400 | 4.121100 | 59.134600 |
| simple_alignment | 0.008600 | 0.065100 | 1.527300 | 29.288200 |
| u64_alignment | 0.006500 | 0.046900 | 1.335300 | 32.911700 |
| composite_alignment | 0.102500 | 0.868800 | 9.609800 | 147.591200 |
| same_keys | 0.001900 | 0.001500 | 0.001800 | 0.003100 |
| equality | 0.002400 | 0.020600 | 0.205400 | 10.961200 |
| difference | 0.021300 | 0.159400 | 1.735800 | 37.366000 |
| pearson | 0.019800 | 0.160600 | 1.614100 | 23.389400 |
| spearman | 0.078900 | 1.285600 | 13.972500 | 201.513000 |
| relation | 0.116500 | 1.776600 | 18.954700 | 367.598900 |
| grouped_summary | 0.019500 | 0.220200 | 2.732000 | 42.544600 |
| robust_mad | 0.027200 | 0.198900 | 1.734500 | 24.981600 |
| outliers | 0.020400 | 0.206400 | 1.819100 | 26.238500 |
| jumps | 0.002700 | 0.022100 | 0.213900 | 3.144800 |
| lag_pm2 | 0.161900 | 1.613500 | 16.810700 | 217.045300 |
| lag_pm8 | 0.519800 | 5.165900 | 52.364200 | 679.318100 |
| lag_pm32 | 1.917500 | 19.462800 | 195.400000 | 2513.201500 |
| prepare_once_5_checks | 0.106000 | 1.308600 | 17.274300 | 265.356700 |
| reprepare_5_checks | 0.224000 | 4.875200 | 54.665400 | 831.671700 |
| success_aggregation_100 | 0.012200 | 0.012600 | 0.013600 | 0.030900 |
| multiple_failures_100 | 0.056400 | 0.060400 | 0.063200 | 0.115600 |
| bounded_formatting | 0.001700 | 0.002300 | 0.002100 | 0.002400 |

## Interpretation

- **Prepare once + five checks vs prepare/join five times** includes preparation in both measurements and five different checks over composite keys: signal equality, strength equality, fold/target predicate, signal linear metrics and strength differences. At 1M: **265.3567 vs 831.6717 ms, 3.13× faster**. At 1k/10k/100k: 2.11×/3.73×/3.16×. Separate equality/difference/Pearson/Spearman/relation rows isolate analyses on one cached alignment and numeric field pair. This is not a claim that every possible statistical workload has this speedup.
- **Ordinary Vec vs decoded Observation TypedView**: source handles are O(1), near clock resolution; do not interpret their tiny timing differences. The meaningful cached numeric projection costs **3.7558 vs 3.9450 ms** at 1M (**1.050×**, about 5% more on this run; not a universal overhead bound). Decode costs **181.6861 ms once**, not per measurement. Smaller-scale order/cache effects explain ratios below one; these are not intrinsic adapter speedups.
- Composite alignment costs more than ascending simple row identity; it sorts fold/target/row keys. u64 specialization was **slower** than generic simple alignment at 1M (32.9117 vs 29.2882 ms) here; both remain available, no blanket fastest-path claim or speculative hash fallback.
- Full relation includes Spearman ranks, difference quantiles and residuals. Pearson alone is linear/constant numeric scratch; Spearman and quantiles allocate/sort O(n) scratch. Lag Pearson ±2/±8/±32 scales with 5/17/65 candidates, retaining only candidate summaries, not one pair trace per lag.
- Success aggregation stores counters and no measured trace; failures retain at most the configured diagnostics. The 100-check and ten-failure formatting rows are nearly source-size independent (timer/cache noise applies). Bounded reporting is structurally certified on a 1M-row source, not inferred from timing.

## Storage/scratch model (approximate, representation-derived, not allocator measurements)

On 64-bit targets: a numeric source projection owns n f64 (~8n bytes); a numeric comparison owns n f64 pairs (~16n). Simple u64 key/index vectors are ~16n bytes per side and matched mappings ~24n; three-u64 composite key/index vectors are ~32n per side and matched mappings ~40n (Rust layout is not a persisted contract). PreparedKeys retains its sorted vector; a prepared comparison retains one mapping. Group membership stores key/index plus one cached numeric vector; robust statistics use sortable f64 scratch. Alignment retains full missing/duplicate indices for direct callers; rendered examples are bounded independently. Original Observation records stay shared by Arc, while decoded payloads are stored once separately. No allocation counter was installed, so these are estimates, not measured peak RSS or allocation counts.

## Runtime and historical measurements

Runtime evidence and charts remain in [BENCHMARKS.md](BENCHMARKS.md),
[BATCH_BENCHMARKS.md](BATCH_BENCHMARKS.md), and the README benchmark section.
[ANALYSIS_BENCHMARKS.md](ANALYSIS_BENCHMARKS.md) preserves earlier algorithm/extension measurements;
its old grouping projection representation is historical. Current framework measurements above
include the final prepared-source/group caches and named collect-all result path.
