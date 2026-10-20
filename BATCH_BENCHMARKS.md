# Production JsonlSink bounded-batch measurements

See the [production user guide](README.md#bounded-writer-batch-tradeoff) for the generated
comparison chart and operational context.

## Method

The production writer caps one processing iteration at **8192 FIFO messages** (therefore
at most 8192 records). The producer queue is still unbounded. This is not a public setting
or a hard byte limit: one user record can itself be arbitrarily large.

Harness: [batch.rs](crates/sun-observations/benches/batch.rs). Reproduce with:

~~~sh
cargo bench -p sun-observations --bench batch
# Equivalent command used via the Cargo tool wrapper:
cargo test -p sun-observations --release --bench batch
~~~

Each invocation automatically writes `metrics.csv` and `stderr.txt` to a new
`batch-<id>` directory under the benchmark output directory beside the active Cargo
profile directory. Temporary JSONL files are cleaned up after each case.
No shell redirection is needed. See [BENCHMARKS.md](BENCHMARKS.md#generated-artifacts).

262144 admitted engine observations per case, real temporary JSONL file, retention off,
default 1000 ms periodic flush. Event contains producer/sequence/f64; Rich adds nested Event,
16 floats and repeated Unicode/escaped text. Same executable/harness, default release profile,
same host/toolchain/storage as BENCHMARKS.md; no affinity or isolated machine. Each sample
uses a fresh engine/file; one worker warm-up plus engine barrier precedes timing. Producer
timing includes producer thread creation, first thread-local resolution and enqueue/join;
end-to-end adds **engine.flush_channels()**, not fsync. Parsing every physical line, exact
count, terminal LF and records_written are checked outside timing.

Execution order: unbounded (5 samples), 8192 (5), 4096 (5), 16384 (5), unbounded confirmation
(5), 8192 confirmation (5). Only the pending-prefix bound changed for comparisons; the
confirmation also contains the retention-history fix, which is not exercised (retention off).
The original measurement runs used local snapshots named
`performance/batch_baseline.txt`, `performance/batch_baseline_confirm.txt`,
`performance/batch_after.txt`, `performance/batch_after_confirm.txt`,
`performance/batch_after4096.txt`, and `performance/batch_after16384.txt`.
Those historical files are not tracked or regenerated; the tables below preserve
the measured results and no samples were discarded.

The temporary unbounded confirmation produces an unused-cap warning; the final bounded
source has no warning. Baseline original prefix logic was restored only for that control,
then bounded production code was restored before certification acceptance.

## Actual sink results

Medians over all 10 samples for before/8192; aggregate reciprocal throughput, not individual
producer-call latency. Positive change is a regression; negative is faster. No outliers removed.

| Producers | Payload | Before producer ns | After producer ns | Before e2e ns | After e2e ns | e2e change | After records/s |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | Event | 699.88 | 707.42 | 700.02 | 707.68 | 1.1% | 1413068 |
| 1 | Rich | 974.23 | 989.48 | 1022.26 | 1116.84 | 9.3% | 895387 |
| 4 | Event | 196.81 | 188.81 | 699.01 | 654.09 | -6.4% | 1528842 |
| 4 | Rich | 389.53 | 390.67 | 1645.20 | 1501.97 | -8.7% | 665790 |
| 16 | Event | 145.87 | 144.00 | 786.07 | 724.79 | -7.8% | 1379710 |
| 16 | Rich | 257.15 | 256.94 | 1827.66 | 1600.99 | -12.4% | 624614 |


At enqueue completion, 4/16 producers leave roughly 186k–257k records not yet reflected in
output stats. This includes the in-flight writer batch, not just queue.len(). With the bounded
writer, at most 8192 of these can be in that batch, so a substantial queue backlog exists.
Single-producer Event normally has no outstanding backlog; Rich sometimes does. Raw aux
values and batch counts are in the linked output. File count validation passed for every case.

## Cap selection and interpretation

Median end-to-end ns/record (5 samples for alternative caps):

| Cap | 1p Event | 1p Rich | 4p Event | 4p Rich | 16p Event | 16p Rich |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| unbounded | 700.02 | 1022.26 | 699.01 | 1645.20 | 786.07 | 1827.66 |
| 4096 | 668.37 | 1185.11 | 665.06 | 1525.13 | 746.28 | 1689.81 |
| 8192 | 707.68 | 1116.84 | 654.09 | 1501.97 | 724.79 | 1600.99 |
| 16384 | 721.40 | 1223.25 | 681.93 | 1527.34 | 734.83 | 1660.50 |


Retain **8192**: strongest saturated 4/16-producer results among these bounded candidates.
Actual saturated persistence improved 6.4–12.4%, unlike merely inferring production speedup
from the earlier prefilled control. Producer costs are largely unchanged. The earlier ~17.5%
prefilled gain is **not** claimed for actual engine+sink workloads.

There is **no universal speedup**: one-producer Rich end-to-end regressed 9.3% in these
runs; Event +1.1% is within observed run variation. Both adjacent caps made the Rich case
worse. Concurrent allocation/destruction and a partially saturated writer differ from the
prefilled, producer-free control; bounding the working set trades batch amortization against
allocator/cache reuse. These measurements do not isolate CPU causes or promise a latency
bound. The finite working-set requirement and measured multi-producer improvement justify
8192, without a public tuning API, producer throttling, payload redesign or timing assertions.
