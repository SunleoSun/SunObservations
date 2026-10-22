# Analysis benchmark report

Historical algorithm/extension samples retained across the ownership migration. Generic
algorithms now belong to sun-certifications; the executable adapter harness delegates to
that owner. Grouped numeric projection now caches values once, so these timings describe
the earlier representation, not a claim of byte-identical current measurements. See
[final framework/adapter benchmarks](CERTIFICATION_BENCHMARKS.md) for current prepared-source,
reusable comparison and aggregation results.

## Environment and reproduction

Measured on AMD Ryzen 9 5950X (16 cores / 32 logical CPUs), Windows 11 build
10.0.26200, x86_64 Windows MSVC, rustc 1.96.0 (ac68faa20 2026-05-25), cargo
1.96.0 (30a34c682 2026-05-25). Release optimization, ordinary allocator, no
task-supplied RUSTFLAGS, CPU affinity, LTO or native-CPU tuning. The machine was
not isolated. These are measurements, not universal latency guarantees.

```sh
cargo bench -p sun-observations-data-source --bench analysis
```

The identical executable can be run through the repository Cargo test wrapper:

```sh
cargo test -p sun-observations-data-source --release --bench analysis -- --measure
```

The harness is crates/sun-observations-data-source/benches/analysis.rs. It reuses the
existing benchmark child-process report support: unique run directory beside the
active Cargo profiles, metrics.csv and stderr.txt, replay to console, and child
failure propagation. The original baseline measured run was analysis-cjX2jh under
C:/Temp/sun-observations-build/benchmarks. Tracked complete samples are
[scripts/analysis_metrics.csv](../scripts/analysis_metrics.csv) (240 rows) and
[scripts/analysis_stderr.txt](../scripts/analysis_stderr.txt) (verified report sizes).
No benchmark writes to production evidence or modifies FORMAT v1.

## Workload and timing boundaries

Each size uses n shared structured records with a u64 row and f64 value, one
namespace, and values repeating every 997 rows. Capture/JSON construction is
outside all timers. Snapshot construction includes shared-handle copying and
the immutable handle-array conversion. Namespace query counts all n matches.
Typed decode includes the namespace scan, one decode of each data payload and
source-handle retention. Prepared views are reused for later analyses.

Join sizes mean n records on each side, not n/2. Sorted keys cover [0,n).
Unordered sides use a fixed-seed permutation; the two-unordered case reverses
the permutation on the other side. Sparse keys multiply each integer by
1,000,003. Numeric relations use y=2x+3; repeating values exercise rank ties.
Outlier/jump workloads have no findings at configured thresholds, so their
output-allocation cost for many findings is not represented. JSON serialization
uses a complete aligned report with n match entries and five disagreements.
Display formats a precomputed expectation failure without cloning its evidence.

There are three nonadaptive samples per operation/size, fixed operation order.
Timers include construction/allocation and computation, exclude result destruction,
source setup, report preconstruction and artifact I/O. black_box retains outputs.
Assertions outside timers verify comparison/candidate equivalence, fit and report
counts. The public certification separately captures and checks exact 100k and
1M-record traces; benchmarks are not substitutes for correctness tests.

## Original baseline results

**Median total milliseconds**, three samples; full per-record nanoseconds and
min/max observations remain in the tracked CSV. At 1M, milliseconds total also
numerically equal nanoseconds per record.

| Operation | 1k | 10k | 100k | 1M |
| --- | ---: | ---: | ---: | ---: |
| snapshot | 0.0034 | 0.0601 | 0.8889 | 9.2728 |
| namespace_query | 0.0102 | 0.3233 | 3.8378 | 41.6119 |
| typed_decode | 0.0499 | 1.0223 | 11.6693 | 117.1535 |
| summary | 0.0062 | 0.0609 | 0.6152 | 6.6705 |
| median_mad | 0.0251 | 0.2727 | 2.1571 | 21.9834 |
| sorted_join | 0.0069 | 0.0696 | 1.4790 | 15.3494 |
| generic_ordered_join | 0.0067 | 0.0664 | 1.4293 | 15.8818 |
| generic_unordered_join | 0.0194 | 0.2445 | 3.2919 | 42.4048 |
| integer_unordered_join | 0.0059 | 0.0552 | 1.3222 | 27.2081 |
| generic_both_unordered_join | 0.0302 | 0.4044 | 5.2226 | 62.7077 |
| integer_both_unordered_join | 0.0056 | 0.0560 | 1.4614 | 21.9647 |
| integer_sparse_join | 0.0171 | 0.2330 | 3.4081 | 38.5453 |
| dense_candidate_unique | 0.0019 | 0.0167 | 0.5637 | 8.4340 |
| pearson | 0.0179 | 0.1824 | 1.7894 | 18.0897 |
| spearman | 0.0482 | 0.8807 | 10.0512 | 123.8567 |
| outliers | 0.0289 | 0.3240 | 2.5269 | 26.1427 |
| jumps | 0.0049 | 0.0483 | 0.4809 | 4.9454 |
| relation | 0.1273 | 1.6126 | 16.7744 | 193.2408 |
| report_json | 0.0430 | 0.3919 | 5.9566 | 59.1052 |
| report_display | 0.0008 | 0.0011 | 0.0031 | 0.0043 |

### Join choice

The generic API uses adaptive sorted key/index vectors, never a HashMap. An
ordered side skips sorting; sorted merge is O(n+m). Unordered generic sides
sort by (key, original index), then share the same duplicate-aware merge.

A unique-dense prototype consistently beat sorting, so the production integer-
domain API join_u64_by automatically selects ordered merge first, bounded dense
indexing for unordered unique keys spanning at most n+m positions, and generic
sorted fallback otherwise. Duplicate detection falls back to the canonical merge
diagnostics: no ambiguous key is paired. Extent overflow and sparse u64 keys never
request an enormous dense array. Callers select the integer-key contract, not a
collection, capacity or density threshold. There is **no hash-based fallback**.

At 1M the production one-unordered integer join median is 27.21 ms versus
42.40 ms generic; two-unordered is 21.96 versus 62.71 ms. Explicit sorted is
15.35 ms and adaptive generic already-ordered is 15.88 ms. The 8.43 ms prototype
is only a constrained lower bound: it assumes exact unique [0,n) keys and omits
production extraction, validation and mismatch diagnostics. It is not offered
as a production API and its time must not be attributed to join_u64_by.

### Memory/materialization (x64, not process RSS)

The CSV byte column describes logical scratch or retained output for the measured
scenario, not allocator accounting. Capacity rounding, headers, existing captures,
keys, input pairs and prebuilt reports are excluded. No allocation profiler/RSS
measurement was run. In particular:

- Snapshot: 8n bytes retained for handles (8 MB at 1M); conversion can temporarily
  retain two handle arrays. Canonical payloads are never deep-cloned.
- Prepared Sample view: 32n logical bytes for decoded values and source coordinates/
  handles; growing Vec capacities can be larger (about 33.6 MB here at 1M).
- Streaming summary and Pearson: constant scratch. Shifted centered moments avoid
  large common-offset accumulation; compensated sum supplies the published mean.
- Robust summary/outliers: one 8n-byte vector, reused for median/MAD, plus findings.
  Jumps use constant scratch plus findings and retain both maximum transitions.
- Complete generic/sorted u64 joins: 32n bytes of key/index scratch plus 24n match
  output, approximately 56 MB at 1M. Production dense span=n peaks near 48n bytes
  while building indexes; keys are dropped before building matches. At the density
  bound span=2n this can peak near 64n. Failure evidence can allocate per missing
  or duplicated key; these complete-join timings do not model a failure-heavy trace.
- Spearman: five logical f64/index arrays at peak, approximately 40n bytes; growing
  capacities can approach 56n. Difference/residual summaries use two logical f64
  arrays, sequentially, not in addition to all ranking arrays. Full joined reports
  retain their own comparison copy. Bounded largest-disagreement selection costs
  O(n log(top+1)) time and O(top) scratch.
- JSON output is 48,476 / 498,529 / 5,268,593 / 55,668,652 bytes at the four sizes.
  to_vec capacity can exceed length. At 1M serialization costs 59.11 ms median;
  human Display is a 47-byte summary, not a replacement for structured evidence.

### Noise and limitations

The final 1M sorted samples ranged 14.30–41.84 ms; integer one-unordered ranged
18.76–42.14 ms; generic one-unordered ranged 34.96–44.21 ms. This substantial
scheduler/cache/allocator noise is retained, not discarded. Medians make scaling
visible but three samples are not a statistical confidence interval. Small-case
Display results approach timer resolution. No universal ordering is claimed between
the explicit and adaptive ordered entry points.

These payloads are deliberately small, schema decoding is by-reference, and a single
namespace/all-match scan is measured. Deep/nested payloads, sparse query selectivity,
string/composite key costs, non-finite failures and failure-heavy serialization can
behave differently. Envelope scans remain allocation-free O(n), without speculative
indexes. Materializing ranking/order statistics and full machine-readable comparison
necessarily costs linear memory. No production capture hot-path claims are inferred
from these offline-consumer measurements.

## Typed/source, grouping and lag extension measurements

The extended **same harness** was run in release mode with the command above on
Windows 11 10.0.26200, AMD64 Family 25 Model 33 (the reference host), rustc/cargo
1.96.0, ordinary release profile/allocator, no added tuning or affinity. Run directory:
`C:/Temp/sun-observations-build/benchmarks/analysis-ed0SRm`. Complete raw samples:
[analysis_extension_metrics.csv](../scripts/analysis_extension_metrics.csv) (**336 rows**,
28 operations × four scales × three samples), with
[diagnostics](../scripts/analysis_extension_stderr.txt). Original 240 baseline samples
remain separately tracked; do not silently compare runs as a controlled speedup claim.

### Extension workload and timing boundaries

The existing prepared Sample view is reused. Typed/source iteration reads the numeric
value, row, dataset index and shared record reference under black_box; no allocation
or decoding. Grouping uses row modulo 8 (few large groups) or row modulo n/8 (many
small groups, exactly 8 members each). Both are key-unordered at these scales; timing
includes key/index extraction and sorting. Group summary timings reuse those prepared
groups and include typed summary output allocation, not group construction. Summary
semantics are the existing NumericSeries owner. Robust summaries remain explicit and
use the already-measured median/MAD owner; these grouped timings do not include them.

Lag compares the prepared view to itself, numeric values repeating every 997 keys.
It includes both projections, validation, index allocation, ascending-key fast path
and every candidate Pearson calculation. Dense [0,n) keys produce n−abs(d) pairs at
candidate d; missing boundary counts are expected, interior gaps are zero. Both
projections run even for the same view, since each side is an independent contract.
Outside timers the harness verifies group membership totals, best lag zero and all
candidate counts/completeness. Independent owner/public certifications, not benchmark
assertions or timings, establish algorithm correctness and large-trace semantics.

### Extension results

Median **total milliseconds** (all three nonadaptive samples retained):

| Operation | 1k | 10k | 100k | 1M |
| --- | ---: | ---: | ---: | ---: |
| typed_source_iteration | 0.0025 | 0.0146 | 0.2924 | 1.4202 |
| group_few | 0.0149 | 0.1865 | 2.1304 | 30.3779 |
| group_many | 0.0154 | 0.2001 | 2.2288 | 30.8997 |
| group_summary_few | 0.0116 | 0.0788 | 0.7525 | 11.1735 |
| group_summary_many | 0.0120 | 0.1124 | 1.4830 | 20.2871 |
| lag_pearson_pm2 | 0.1658 | 1.7263 | 17.1355 | 239.9123 |
| lag_pearson_pm8 | 0.5435 | 5.4562 | 54.4074 | 712.8121 |
| lag_pearson_pm32 | 2.0284 | 20.4789 | 206.7467 | 2799.6154 |

Effective **nanoseconds per aligned candidate pair**, including shared preparation:
`median_total_ms × 1e6 / ((2w+1)n − w(w+1))`. There are 5/17/65 candidates.

| Half-window w | 1k | 10k | 100k | 1M |
| --- | ---: | ---: | ---: | ---: |
| 2 | 33.200 | 34.530 | 34.271 | 47.983 |
| 8 | 32.107 | 32.109 | 32.006 | 41.930 |
| 32 | 31.722 | 31.557 | 31.812 | 43.072 |

At 1M, effective ms/candidate are 47.9825 / 41.9301 / 43.0710 respectively.
Increasing window size is linear candidate work, not FFT signal processing; the
preparation component is amortized over more candidates. The larger-trace increase
in effective cost is measured, not extrapolated from small inputs.

### Strategy and memory

Grouping retains a single sorted (key, original-index) vector, **16n bytes** for u64
keys on x64 (16 MB at 1M), skips sorting if ordered, and discovers group slices without
another index or per-group allocation. Key extraction is once; no values/Arc payloads
are cloned. Summary processing is O(n+g), constant numeric scratch plus g output
entries: sizeof(GroupSummary<u64>)=152 bytes here, 1216 bytes for 8 groups or 19 MB
logical output at 125k groups. Allocator capacity/header rounding is excluded.
String/composite keys have their own concrete size/allocation costs. There is no
hash-table default or speculative grouping cache.

Lag uses the canonical grouped merge walk shared with joins, two key/index arrays
and two numeric arrays: **24(n+m) bytes**, 48 MB for two 1M sides, plus 64w_candidate
bytes for LagCandidate output (320/1088/4160 bytes here). Original observations,
prepared views and allocator overhead are excluded. Pearson has no candidate pair
materialization; each candidate streams existing numeric values. Preparation is
O(n+m) if ordered, otherwise O(n log n + m log m); search O(candidates×(n+m)).
Spearman uses the existing ranking owner per candidate and adds its O(p) scratch;
these new window timings measure Pearson, not Spearman or full relation reports.

### Extension noise and limitations

The machine was not isolated. At 1M typed/source samples range 1.2842–1.5798 ms;
grouping few 26.0591–42.6813 ms, many 29.5973–47.9038 ms; lag ±2
229.3988–356.8104 ms, ±8 708.2139–964.2688 ms, ±32 2632.5862–2917.8857 ms.
Three samples expose substantial scheduler/cache/allocator noise, not confidence
intervals. Input/setup and destruction remain outside timers. No allocation profiler
or process RSS measurement was used. Sparse/failure-heavy lag searches and arbitrary
key payload costs are certified functionally but not represented by this benchmark.
No timing is a correctness threshold, and no runtime capture performance is inferred.

