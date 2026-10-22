# Scoped capture and bounded retention benchmarks

Current capture/cache benchmark evidence: recorded samples of the current
representation, not fresh measurements of every checkout. See the
[benchmark map](BENCHMARK_CHARTS.md) for other runs and historical datasets.

## Reproduction and boundaries

Host verified for this run: AMD Ryzen 9 5950X, Windows 11 10.0.26200, x86_64-pc-windows-msvc, rustc 1.96.0 (ac68faa20), Cargo 1.96.0. Default optimized release profile; no pinning/isolation or allocator/RSS profiling. Three raw samples at every scale, no samples discarded. Timings are evidence, never correctness assertions.

Run: cargo test -p sun-observations-data-source --release --bench capture -- --measure

Harness: [capture.rs](../crates/sun-observations-data-source/benches/capture.rs). [Raw samples](../scripts/capture_metrics.csv) contain all 156 timings (13 operations × 4 scales × 3 samples). The child-process report directory was C:/Temp/sun-observations-build/benchmarks/capture-aUxlHo; the child exited successfully. Report I/O and record destruction are outside emission timings.

- Direct configured emission uses an intentionally independent engine with one MemorySink and a scope, the low-level baseline. Scoped capture calls the real global observe! macro with identical typed payload/retention and canonical selection. Both include first plan resolution, serialization, envelope construction and synchronous delivery; configuration/context entry and finalization are outside these rows. Capture does not alter global routes/selection.
- Finalization is the canonical ScopedCapture::finish + ObservationDataset::from_shared path after emission: context cleanup, health snapshot, draining shared handles and exposing a fixed dataset. It does not decode or deep-clone payloads. Completeness and exact dataset size are checked outside timing.
- Memory rows clone a single prebuilt Arc record, isolating synchronization/container costs from payload destruction/serialization. Below-limit capacity=n; repeated-compaction capacity=n/10, batch=ceil(capacity/10). Exact retained+evicted=n and capacity are verified outside timing. Real heterogeneous payload destructor costs may differ.
- Vec and VecDeque controls use identical Arc handles, capacities and oldest-batch retention, without mutex overhead. Vec drains must move the retained tail; VecDeque drains oldest records without shifting that tail. No single-record remove(0) control is needed for the selected batch contract.
- Numeric rows use one typed-key comparison and one projected numeric pair prepared outside timing. Correlation is cold (moments+ranks). Linear fit follows correlation, reusing moments and computing residuals. Difference is cold. Cached views repeat all three after preparation; sub-microsecond results are near timer resolution. Full preparation + six requests is compared against rebuilding keys/comparison/projection for each request; identical analyses are requested twice each. These preparation comparison rows include temporary preparation destruction.

## Release medians (milliseconds)

| Operation | 1k | 10k | 100k | 1M |
| --- | ---: | ---: | ---: | ---: |
| direct_configured_emission | 0.713100 | 6.783300 | 69.134800 | 726.299700 |
| scoped_capture_emission | 0.609600 | 6.408300 | 67.660300 | 709.843200 |
| capture_finalize_dataset | 0.004800 | 0.033500 | 0.361400 | 3.584000 |
| memory_below_limit | 0.009200 | 0.153800 | 0.994900 | 8.026600 |
| memory_repeated_compaction | 0.009700 | 0.073800 | 0.771500 | 7.216500 |
| vec_batch_control | 0.004100 | 0.042300 | 0.547200 | 5.897700 |
| deque_batch_control | 0.004400 | 0.032100 | 0.417900 | 3.960300 |
| correlation_cold | 0.059500 | 0.675700 | 5.655500 | 58.042500 |
| linear_fit_after_correlation | 0.018000 | 0.250300 | 2.248000 | 20.302100 |
| difference_cold | 0.017500 | 0.246800 | 2.005000 | 20.065700 |
| numeric_views_cached | 0.000300 | 0.000400 | 0.000500 | 0.001300 |
| prepare_once_six_checks | 0.115600 | 1.392500 | 11.851000 | 121.263900 |
| reprepare_six_checks | 0.328800 | 4.207500 | 34.312400 | 357.869600 |

## Interpretation and limits

At 1M, scoped emission median is 709.8432 ms versus 726.2997 ms direct configured emission (0.977×); finalization adds 3.5840 ms, approximately 0.5% of emission. At 10k/100k capture emission is about 0.945×/0.979× direct. The 1M sample ranges overlap: no material regression is demonstrated. Fixed operation order, three samples and an unisolated host do not establish an intrinsic speedup. The 1k comparison is dominated by warmup/allocator effects. This is a small two-field payload, not a universal overhead bound.

At 1M, VecDeque batch control is 3.9603 ms versus Vec batch 5.8977 ms (~33% lower); VecDeque is also lower at 10k and 100k. Choose VecDeque for the canonical bounded MemorySink. The default stays finite at 1,000,000 records / 10% batches: bounded handle count, amortized batch removal, no million-handle tail move on overflow. Actual byte use depends on payloads and snapshots held by callers; this is not a byte cap or RSS guarantee.

Prepare-once six requests take 121.2639 ms versus 357.8696 ms repeated preparation (2.95× faster at 1M). Cached semantic views retain compact results, not rank/quantile scratch, and do not repeat projection or correspondence. [Deterministic certifications](RELEASE_VALIDATION.md) protect decoding/key/numeric reuse, bounded diagnostics, dropped success reports and retention independently of timing.

The [earlier certification dataset](history/CERTIFICATION_BENCHMARKS.md) is historical and unchanged. Its uncached analysis timings must not be interpreted as fresh samples from the now-cached harness. Runtime JSONL/Console queue benchmarks remain separate from scoped in-memory capture; bounded history does not bound asynchronous sink backlog.
