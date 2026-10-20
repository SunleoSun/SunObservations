# sun-observations-analysis

Deterministic, immutable analysis of SunObservations FORMAT v1 evidence. This crate
consumes the runtime; the runtime never depends on analysis. It neither changes v1
nor interprets human rendering as data. Domain acceptance belongs to the consuming
certification, not to an Observation type or anomaly detector.

For a local checkout, add sun-observations-analysis and the runtime as path
dependencies, plus Serde for the optional decoded payload types:

```toml
[dependencies]
sun-observations-analysis = { path = "../SunObservations/crates/sun-observations-analysis" }
sun-observations = { path = "../SunObservations/crates/sun-observations" }
serde = { version = "1", features = ["derive"] }
```

## Snapshot, query, decode

`ObservationDataset::from_memory(&sink)` clones shared handles under the existing
MemorySink lock, not JSON payloads. `from_shared(Vec<Arc<ObservationRecord>>)` takes
ownership of an existing snapshot. Membership/order never change afterward.
Dataset clones share the handle array. Queries are lazy O(n) scans without an index
or allocation. All selectors combine with AND; subtree matching is segment-aware.
Optional scope/phase/span selectors distinguish unrestricted, absent and present.

```rust
use serde::{Deserialize, Serialize};
use sun_observations::{MemorySink, Observation};
use sun_observations_analysis::{EnvelopeQuery, ObservationDataset};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::input")]
struct Input { row: u64, value: f64 }

let memory = MemorySink::new();
let dataset = ObservationDataset::from_memory(&memory);
let selected = dataset.query(EnvelopeQuery {
    namespace_subtree: Some("module"), ..Default::default()
});
assert_eq!(selected.iter().count(), 0);
let inputs = selected.typed::<Input>()?;
assert!(inputs.values().is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

`typed::<T>()` requires `Observation + DeserializeOwned` only at decoding time.
It selects the namespace, decodes canonical `data` by reference once, and retains
values plus original shared records and dataset indices. Span ends are intentionally
excluded: they are lifecycle evidence with null payload, not another T. Points and
span starts, including malformed/null payloads, are decoded strictly. Query ends
through the envelope API. Decode errors retain type and envelope coordinates.

## Numeric and hard integrity contracts

`view.numeric(|v| v.value)` creates a reusable iterator projection, not a numeric
copy. Numeric/key projection functions and cloned iterators must be pure/replayable.
`NumericSeries::new(values.iter().copied())` also works on ordinary numeric slices.

- `summary()` is streaming O(n), O(1) scratch: count, finite count, min/max,
  compensated sum/mean, shifted online population variance/stddev and optional sample variance.
- `finite_count()` counts all and finite inputs without rejecting invalid values.
- `quantile(p)` uses type 7: position `(n-1)*p`, linear interpolation between neighbors.
  Endpoints p=0/1 select min/max. `robust_summary()` adds median, quartiles, IQR,
  and unscaled MAD (median of absolute deviations from the median). Sorting uses
  O(n log n) time and one O(n) f64 scratch vector, reused for MAD.
- Empty numeric summaries/order statistics return `NumericError::Empty`. Non-finite
  inputs fail with index and a JSON-safe tagged value. Unrepresentable intermediate
  arithmetic returns `ArithmeticOverflow`; variance underflow on distinct inputs returns
  `ArithmeticUnderflow`. There is no NaN success or silent filtering.
- `check_count`, `check_unique`, `check_contiguous`, `check_same_keys`, and
  `series.check(NumericRule)` return typed hard integrity evidence. Contiguous ranges
  are inclusive and missing intervals are compressed, not expanded. Numeric range
  endpoints are inclusive; positive is strict and non-negative includes zero.

## Domain-key joins and relations

Use `inputs.join_by(&outputs, |i| i.row, |o| o.row)`, then inspect `alignment()`
and/or `require_complete()`. When several executions coexist and row identity repeats,
filter the dataset by `proc`/`scope` before typed decoding, or carry execution identity in
the typed payload key. Join key closures receive decoded payload values, not envelope
metadata. Insertion order is not a module alignment contract: filtering, concurrency,
fan-out and omissions change it.

The adaptive join uses sorted key/index vectors and a linear merge; it skips
sorting already ordered sides. `join_sorted_by` validates order and rejects a
decrease. Both produce identical key-sorted matches and complete missing/duplicate
evidence. Duplicate keys on either side are entirely excluded from 1:1 matches;
missing-left means present only on the right. Side-local indices map through
`TypedView::sources()` to original dataset records. `join_u64_by` is the
integer-domain entry point: it chooses ordered merge first,
then bounded dense indexing for unordered unique keys spanning at most n+m positions,
otherwise sorted-vector fallback. Duplicates use the canonical sorted diagnostics.
No collection choice is required from callers; there is no hash-based fallback.
Generic unordered complexity is O(n log n + m log m); ordered complexity is
O(n+m). Dense integer joins use O(n+m+span) time with span bounded by n+m.
Join scratch is proportional to both key/index arrays; output stores key and two
indices per match. No values or JSON payloads are cloned.

`join.difference(left_projection, right_projection, top)` reports right-minus-left
errors, signed/absolute summaries, type-7 medians, p95 absolute error, and bounded
largest disagreements (O(n log(top+1)) time, O(top) scratch).
`join.numeric_relation(..., top)` adds Pearson, Spearman,
population/sample covariance, OLS slope/intercept/R² and observed-minus-predicted
residual summaries. These methods fail closed on incomplete alignment; explicitly
iterating `pairs()` is available for deliberate matched-subset analysis.

Pearson/linear moments use O(n), O(1) scratch. Spearman uses deterministic average
ranks for ties (including signed zero), O(n log n) time and O(n) scratch.
`Metric::Undefined` identifies too few pairs or a constant axis. A constant right
axis permits slope=0/intercept=mean but has undefined correlation and R². Empty
standalone linear/correlation calculations return undefined metrics; empty full
relation/difference reports fail because their summaries are undefined. Floating
point results are deterministic for the same ordered inputs/toolchain, not a
promise of cross-platform bit identity or unlimited dynamic range.

`RelationExpectation::positive(0.9)` (threshold in (0,1]) or `negative(-0.9)`
(threshold in [-1,0)) can be extended with
`bound(RelationMeasure, min, max)` for independent correlation, absolute correlation,
slope, intercept, R², difference and residual constraints. Bounds are inclusive.
Undefined metrics fail constraints. Correlation alone does not certify scale.
`evaluate(report)` returns either the report or a structured `RelationFailure`
containing expectations, violations and observed evidence. Hard `RelationError`
prerequisites and statistical failures are separate types. Core reports/errors
implement Serialize/Deserialize. Top-level reports include an `analysis_kind` tag;
human Display is only a projection. Numeric relation errors retain alignment context.

## Suspicion, not domain failure

`series.outliers(config)` uses modified z = 0.6744897501960817 * deviation / MAD.
The strict absolute-score threshold defaults to 3.5. With zero MAD, the default
flags every nonmedian value using `ZeroMadDeviation`, never infinity. For quantized
traces, explicitly choose `ZeroMadPolicy::AbsoluteTolerance(t)` in original units;
only deviations strictly above t are flagged. No hidden alternative estimator is used.

`series.jumps(config)` streams adjacent transitions and records both endpoints,
signed/absolute change and relative change. The relative denominator is
`max(abs(previous), denominator_floor)` with a caller-selected finite positive floor.
Either enabled strict threshold flags a jump. Empty/singleton traces have no
transitions; earliest maxima win ties. Reports retain configuration and source-local
indices; numeric validation/overflow remains a hard error. Jump scratch is O(1)
plus emitted findings; outliers use one O(n) scratch vector plus findings.

## Certification and performance

See the executable [cross-module example](examples/module_relation.rs),
[certification matrix](../../CERTIFICATION.md#analysis-certification), and measured
[analysis benchmark report](../../ANALYSIS_BENCHMARKS.md). Full relation reports
retain alignment matches and serialize O(n) evidence; choose small top limits and
reuse prepared typed views. Difference summaries materialize two f64 arrays.
Spearman peaks at five logical numeric/index arrays (40 bytes/pair on x64);
growing vector capacities can increase this to almost 56 bytes/pair, excluding
caller data, allocator overhead and reports. No eager envelope indexes,
JSON query language, SQL, rolling analysis, drift, lag/cross-correlation, matrices,
change points or analysis persistence are included in this version.
