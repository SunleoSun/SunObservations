# Benchmark evidence and charts

Measurement navigation, not a tutorial or performance guarantee. For learning use
[the framework guide](../README.md); for configuration use [operations](OPERATIONS.md).

## Choose a dataset

| Question | Evidence |
| --- | --- |
| Runtime admission, selectors, sinks, threads, retention | [Runtime report](BENCHMARKS.md) |
| Scoped capture, bounded memory, reused cached analysis | [Capture/cache report](CAPTURE_BENCHMARKS.md) |
| Production writer chunk tradeoff | [Bounded-writer follow-up](BATCH_BENCHMARKS.md) |
| Generic preparation, ordinary vs adapted values, analysis through 1M | [Pre-cache certification dataset](history/CERTIFICATION_BENCHMARKS.md), explicitly historical |
| Earlier algorithms and experiments | [Historical evidence map](README.md#historical-evidence) |

Runtime charts below use Ryzen 9 5950X, 64 GiB, Windows 11/NTFS, MSVC, Rust/Cargo
1.96.0, optimized profile, unpinned/unisolated host. File results use buffered OS
writes, **not fsync**. Principal charts use three-sample medians and a small Event
payload; bounded-writer and human follow-ups are separate runs. See each linked
report for full ranges, boundaries and controls. No chart is an SLA or correctness test.

## Runtime admission and selectors

![Warm hot-path latency](../img/benchmark-hot-path.svg)

Rejected calls measured 7.28–7.37 ns in the principal run; EveryN(100) averages mostly
rejected calls, not accepted-record latency. Sinkless admission still serializes.
[Method and ranges](BENCHMARKS.md#warm-hot-path).

![Selector scale with separate panel scales](../img/benchmark-selector-scale.svg)

N is N rules in each of four categories (4N total). Fixed namespace depth/type tags;
warm measurements do not promise unlimited interleaved-type or cold-path constant cost.
[Selector method](BENCHMARKS.md#selector-scale-no-linear-warm-degradation).

## Producer versus persistence

![JSONL aggregate producer and end-to-end throughput](../img/benchmark-jsonl-scaling.svg)

End-to-end adds the engine flush barrier. Enqueue rate is not persistence throughput;
more producers still share one sink writer and may grow an unbounded backlog.
[Thread results](BENCHMARKS.md#thread-scaling-1m-total-shared-parent-scope).

![Accepted-observation costs by sink topology](../img/benchmark-sink-comparison.svg)

Fan-out is four MemorySinks on two channels, not four file writers. Near-equal JSONL
producer/end-to-end cost in the one-producer workload does not make it synchronous.
[Topology and ranges](BENCHMARKS.md#sink-comparison-one-producer).

## Bounded writer follow-up

![Before and 8192-message writer chunk](../img/benchmark-writer-batch.svg)

Ten samples per before/after case, 262,144 records. Saturated cases improved 6.4–12.4%,
but one-producer Rich regressed 9.3%. The chunk cap is not queue backpressure,
a payload-byte limit or a universal speedup. [Method and alternatives](BATCH_BENCHMARKS.md).

## Human output follow-up

![Structured before and after medians](../img/benchmark-human-structured.svg)
![Console enqueue versus through-flush service](../img/benchmark-console-scaling.svg)
![Default versus custom row formatting](../img/benchmark-console-rendering.svg)

Five-sample separate follow-up. Structured ranges overlap; changes are not isolated
causal speedups. Console measurements use file-backed output, not terminal painting;
custom rows include synchronous producer formatting. Real terminals can be slower.
[Raw samples, timing boundaries and limits](BENCHMARKS.md#human-rendering-follow-up).

## Reproduce artifacts

From the workspace root, render/check tracked measurements **without new timing runs**:

~~~sh
python scripts/render_benchmark_charts.py
python scripts/render_benchmark_charts.py --check
~~~

Python 3.9+, standard library only, offline deterministic SVG output.
[benchmark_metrics.csv](../scripts/benchmark_metrics.csv) traces every value to its
report/section/row/column; the renderer validates that evidence before drawing.
Chart checking never substitutes a new machine's timings for recorded evidence.

To intentionally collect new machine-specific samples:

~~~sh
cargo bench -p sun-observations --bench runtime
cargo bench -p sun-observations --bench batch
cargo bench -p sun-observations --bench human
cargo test -p sun-observations-data-source --release --bench capture -- --measure
~~~

See [artifact layout](BENCHMARKS.md#generated-artifacts),
[batch method](BATCH_BENCHMARKS.md#method) and each historical report's reproduction
limits. Preserve host/toolchain/workload, raw samples, status and boundaries. Update
report tables and chart data together only when deliberately refreshing the dataset.
