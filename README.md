# SunObservations

SunObservations is a Rust framework for **typed structured logging, runtime diagnostics,
profiling, and semantic certification**. Prove that outputs mean what they should,
inspect why a production decision was made, or persist selected evidence without
turning application code into logging infrastructure.

Use canonical returned values and artifacts first. **Certification works without
Observations.** Add typed Observations when a semantic **Boundary** (transfer),
**Decision** (selection/rejection), or **Diagnostic** fact would otherwise disappear.

```text
ordinary inputs / canonical outputs ───────────────────────┐
production code → typed Observations → capture / decode ───┼→ semantic certification
persisted artifacts / independent references ──────────────┘
```

## Five-minute tour

### Certify ordinary Rust values

A Fact records evidence; an Expectation decides acceptance. Checks collect results,
then one final assertion proves the whole named contract. No runtime or Serde is needed:

```rust
use sun_observations::certifications::{certify, fact, try_fact};

let output = [1u64, 2, 3];
let parsed = "3".parse::<usize>();
let mut proof = certify("exported rows");
let rows = proof.collection("canonical output", &output);
proof.check("schema version", fact("version", 1u8).expect(1u8));
proof.check("declared count", try_fact("header", parsed).expect(3usize));
proof.section("rows", |c| {
    c.check("exact count", rows.count().expect(3usize));
    c.check("unique identities", rows.unique(|row| *row).expect_valid());
    c.check("no future rows", rows.matching_count(|row| *row > 3).expect(0usize));
});
proof.assert_certify();
```

Success is silent. A failed proof reports named paths, expected/observed evidence,
failure classes and bounded examples together—not just the first mismatch.

### Observe and certify real production behavior

Here the returned total is canonical; the internal rate decision is not returned.
The production function emits that decision through the normal global `observe!` path.
Capture requires no engine injection, channel, route, file, or global reconfiguration.

```rust
use serde::{Deserialize, Serialize};
use sun_observations::certifications::try_fact;
use sun_observations::{observe, Observation, ObservationError};
use sun_observations::evidence::{capture_observations, CaptureConfig};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "billing::rate_selected", tags("evidence"))]
struct RateSelected { order: u64, rate: u64 }
struct Order { id: u64, net: u64 }
struct Receipt { order: u64, total: u64 }

fn charge(orders: &[Order]) -> Result<Vec<Receipt>, ObservationError> {
    orders.iter().map(|order| {
        let rate = if order.net >= 100 { 90 } else { 100 };
        observe!(RateSelected { order: order.id, rate })?;
        Ok(Receipt { order: order.id, total: order.net * rate / 100 })
    }).collect()
}

let orders = [Order { id: 1, net: 50 }, Order { id: 2, net: 200 }];
let run = capture_observations(
    CaptureConfig::new().activate_tag("evidence"),
    || charge(&orders),
);
let decisions = run.typed::<RateSelected>();
let mut proof = run.certify("billing production contract");
proof.check("call succeeded", try_fact("charge", run.result().as_ref().map(|_| true)).expect(true));
proof.check("decodable evidence", try_fact("rates", decisions.as_ref().map(|_| true)).expect(true));
if let (Ok(receipts), Ok(decisions)) = (run.result(), &decisions) {
    let input = proof.collection("orders", &orders);
    let output = proof.collection("receipts", receipts);
    let transfer = input.compare_with(&output).on(|v| v.id, |v| v.order);
    proof.check("one receipt per order", transfer.comparison_fact().expect_valid());
    proof.check("canonical totals", transfer.check_pairs(
        |a, b| b.total == if a.net >= 100 { a.net * 90 / 100 } else { a.net }, 3,
    ).expect_valid());
    let rates = decisions.source("internal rate decisions");
    let selection = input.compare_with(&rates).on(|v| v.id, |v| v.order);
    proof.check("one decision per order", selection.comparison_fact().expect_valid());
    proof.check("rate policy", selection.check_pairs(
        |a, b| b.rate == if a.net >= 100 { 90 } else { 100 }, 3,
    ).expect_valid());
}
proof.assert_certify();
```

`run.certify(...)` adds an `EvaluationError` prerequisite if capture lost records,
ignored emission errors, or retained unfinished contexts/workers/spans. A retained
subset cannot silently pass. Application and decode errors above also become checks;
independent checks continue even when dependent comparisons cannot be prepared.

### Add one framework dependency

For an edition-2021 application, the complete framework uses one package:

```toml
[dependencies]
sun-observations = "0.1"
# Only needed for your serializable Observation payloads:
serde = { version = "1", features = ["derive"] }
```

For local development before the first release, replace the version with
`{ path = "../SunObservations/crates/sun-observations" }`. The package re-exports
the derive macro; no separate derive dependency is needed. Runtime, capture,
certification and Rayon capabilities are available without feature flags.

## How the framework fits together

| Namespace | Use it to… | Owns |
| --- | --- | --- |
| `sun_observations::certifications` | Prove contracts over ordinary values, collections, artifacts and references | Preparation, comparison, analysis, expectations, collect-all reports |
| `sun_observations` | Expose otherwise invisible production facts and measure lifecycles | Typed evidence, selection, scopes/policies, propagation, routing, sinks, spans |
| `sun_observations::evidence` | Turn production evidence into the same generic sources | Capture/completeness, snapshots, queries, strict decode, provenance |

There is **one certification system**, not a logging-specific parallel one. The
generic certification layer remains independent of runtime concepts and state;
the evidence adapter composes runtime and generic certification.

Choose your next task:

- [Certify values and collections](#certifications): exact invariants, comparison, statistics.
- [Expose production evidence](#observations): select, scope, route, propagate, profile.
- [Capture or load evidence](#capture-production-evidence): typed sources with provenance.
- [Common recipes](#cookbook), [operations](docs/OPERATIONS.md), or the [documentation map](docs/README.md).

## Certifications

### Facts, expectations and one proof boundary

```text
value / PreparedSource → Fact → expect → CheckResult → Certification::check
                                                        ↓
                                           CertificationReport → assert_certify()
```

Facts and analysis are neutral. `fact(name, value)` wraps ordinary evidence;
`try_fact(name, result)` preserves fallible preparation as evidence. `expect(...)`
returns a `CheckResult`, never an assertion or an intermediate panic. Plain values
mean equality; use standard expectations or one domain-owned `Expectation` for
more involved acceptance.

| CheckClass | Meaning |
| --- | --- |
| `Pass` | Accepted |
| `HardViolation` | Evaluated deterministic invariant failed |
| `StatisticalViolation` | Evaluated statistical/anomaly threshold failed |
| `EvaluationError` | Evidence/evaluation invalid, unavailable or incomplete |

**Every non-Pass fails the final proof.** Statistical violations are not warnings.
Undefined required metrics and invalid bounds fail closed, rather than becoming NaN
or silently passing. Scenario thresholds belong to expectations, not Observations.

`proof.section("meaning", |c| { ... })` groups independent checks into semantic paths.
Call `proof.certify()` for a nonpanicking `CertificationReport` (`is_pass()`, counts,
bounded failures), or `proof.assert_certify()` at the test/application proof boundary.
Do not unwrap acceptance midway: that loses collect-all diagnostics. Fallible dependent
preparation should become a failed check, while unrelated sections still run.

### Name sources and prove collection integrity

Use `source(name, &value)` for one value, `source_slice` or `proof.collection` for
rows. A singular `&Vec<T>` is one Vec value, not a row collection. Keep borrowed inputs alive.

```rust
use sun_observations::certifications::*;

let ids = [10u64, 11, 12];
let status = String::from("ready");
let mut proof = certify("published artifact");
let metadata = proof.source("status", &status);
let rows = proof.collection("artifact ids", &ids);
proof.check("ready", metadata.select(|s| s.clone()).expect("ready"));
proof.check("count", rows.count().expect(CountRule::Exact(3)));
proof.check("identity", rows.unique(|id| *id).expect_valid());
proof.check("no gaps", rows.contiguous(|id| *id, 10, 12).expect_valid());
proof.check("no forbidden id", rows.matching_count(|id| *id == 99).expect(0usize));
proof.assert_certify();
```

`expect_valid()` accepts structural reports, not a subjective quality score. For
structural row rules, count invalid rows and expect zero. `project` caches arbitrary
field projections; `numeric` caches an f64 per row for repeated analysis.

### Compare semantic entities, not arrival order

Prepare **domain-key correspondence** once, then reuse it across fields. Tuples or
dedicated key types express identity. Include execution identity when keys repeat
between runs; insertion/arrival order is not a cross-owner identity.

```rust
use sun_observations::certifications::*;

#[derive(Clone)]
struct Row { fold: u8, row: u64, label: u8, value: f64 }
let producer = [
    Row { fold: 0, row: 1, label: 1, value: 2.0 },
    Row { fold: 1, row: 1, label: 2, value: 4.0 },
    Row { fold: 0, row: 2, label: 3, value: 7.0 },
];
let consumer = [producer[2].clone(), producer[0].clone(), producer[1].clone()];
let mut proof = certify("producer to consumer");
let left = proof.collection("producer", &producer);
let right = proof.collection("consumer", &consumer);
let pair = left.compare_with(&right).on(|v| (v.fold, v.row), |v| (v.fold, v.row));
proof.check("exact unique keys", pair.comparison_fact().expect_valid());
proof.check("labels preserved", pair.equal(|v| v.label, |v| v.label, 3).expect_valid());
proof.check("values preserved", pair.check_pairs(|a, b| a.value == b.value, 3).expect_valid());
proof.assert_certify();
```

Missing/duplicate keys are explicit; duplicates never form ambiguous pairs. Dependent
field/numeric checks on incomplete correspondence yield `EvaluationError`, not a
successful proof over the matched subset. The final argument to pair checks bounds
example diagnostics; it does not limit the rows checked.

### Prove numeric agreement, not just correlation

A perfect correlation can still hide an offset, wrong scale, wrong units, or stale
values. It does not prove equality or causality. Combine correspondence with the
metrics that express the actual contract:

```rust
use sun_observations::certifications::*;

let expected = [(1u64, 2.0), (2, 4.0), (3, 7.0)];
let actual = [(3u64, 7.000001), (1, 2.0), (2, 4.0)];
let mut proof = certify("numeric transfer");
let left = proof.collection("reference", &expected);
let right = proof.collection("optimized output", &actual);
let pair = left.compare_with(&right).on(|v| v.0, |v| v.0);
let values = pair.numeric(|v| v.1, |v| v.1);
proof.check("keys", pair.comparison_fact().expect_valid());
proof.check("association", values.correlation().expect(CorrelationExpectation::positive(0.99).unwrap()));
proof.check("scale", values.linear_fit().slope().expect(close_to(1.0).absolute(1e-5)));
proof.check("fit", values.linear_fit().expect(
    LinearFitExpectation::default().slope(0.999, 1.001).unwrap()
        .intercept(-1e-5, 1e-5).unwrap().r_squared(0.99).unwrap(),
));
proof.check("direct error", values.difference().expect(
    DifferenceExpectation::default().max_absolute(1e-5).unwrap(),
));
proof.check("scalar tolerance", fact("total", 13.000001).expect(
    close_to(13.0).absolute(1e-5).relative(0.0),
));
proof.assert_certify();
```

`correlation()` exposes Pearson/Spearman; `linear_fit()` fits right on left and
exposes slope/intercept/R²/residuals; `difference()` measures **right minus left**,
not fitted residuals. Prepared numeric pairs cache projections and compact results
(including errors); requesting these views does not repeat decoding or correspondence.
The `unwrap()` calls above validate fixed scenario configuration, not acceptance.
For external configuration, handle builder errors explicitly. `close_to(x)` accepts
`abs(actual-x) <= absolute + relative*abs(x)`; both defaults are `1e-9`.

### Inspect groups, anomalies and temporal lag

Global scores can hide broken partitions. Group by domain identity, then choose
scenario-owned thresholds. Outliers use robust MAD; jumps inspect **source order**,
so sort into domain time order first when arrival order is not meaningful.

```rust
use sun_observations::certifications::*;

let samples = [(0u8, 10.0), (0, 10.1), (0, 9.9), (1, 10.0), (1, 10.2), (1, 9.8)];
let mut proof = certify("sensor partitions");
let rows = proof.collection("samples in time order", &samples);
let groups = rows.group_by(|v| v.0);
proof.check("each sensor", fact("sensor summaries", groups.numeric(|v| v.1).summaries()).expect(
    GroupExpectation { minimum_count: 3, mean_min: 9.5, mean_max: 10.5 },
));
for group in groups.iter() {
    let values = group.numeric(|v| v.1);
    proof.section(format!("sensor {}", group.key), |c| {
        c.check("robust outliers", try_fact("sensor values", values.outliers(OutlierConfig::default()))
            .expect(AtMostAnomalies(0)));
        c.check("sudden jumps", try_fact("ordered sensor values", values.jumps(JumpConfig {
            absolute_threshold: Some(0.5), relative_threshold: None, denominator_floor: 1e-9,
        })).expect(AtMostAnomalies(0)));
    });
}
proof.assert_certify();
```

For unknown stale/off-by-one relations, search a bounded range of **temporal keys**,
not vector offsets. A positive lag `d` pairs left key `k` with right key `k+d`:

```rust
use sun_observations::certifications::*;

let sent = [(0i64, 2.0), (1, 9.0), (2, 3.0), (3, 7.0), (4, -1.0), (5, 5.0), (6, 11.0), (7, 4.0)];
let received: Vec<_> = sent.iter().map(|(key, value)| (key + 1, *value)).collect();
let mut proof = certify("known one-tick delay");
let left = proof.collection("sent", &sent);
let right = proof.collection("received", &received);
let delayed = left.compare_with(&right).on(|v| v.0 + 1, |v| v.0);
proof.check("exact known tick offset", delayed.comparison_fact().expect_valid());
proof.check("payload preserved", delayed.equal(|v| v.1, |v| v.1, 3).expect_valid());
proof.check("delay", left.lag_relation_by(&right, |v| *v, |v| *v, LagConfig {
    range: LagRange { start: -1, end: 1 }, measure: LagMeasure::Pearson,
}).expect(LagExpectation { best_lag: 1 }));
proof.assert_certify();
```

Use one unique temporal domain per search: partition repeated fold/target/execution
keys first. `LagAlignment` distinguishes expected boundary truncation from interior
gaps; undefined candidates or interior missing keys cannot establish the expected lag.
When offsets are known, prove them exactly too—lag is diagnosis, not a substitute for
causal identity. See the [analysis semantics reference](docs/CERTIFICATIONS_API.md#analysis-semantics).

## Observations

### Define semantic evidence

Choose a stable namespace for meaning, not the Rust module/type spelling. Tags select
cross-cutting facets such as `evidence`, `diagnostic` or `expensive`; they are not
persisted. Namespace segments and individual tags use `[a-z][a-z0-9_]*`.

```rust
use serde::Serialize;
use sun_observations::{observe, start_scope, Observation, ScopeConfig};

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::train::epoch", level = INFO, tags("diagnostic"))]
struct Epoch { epoch: usize, loss: f64 }
let _scope = start_scope(ScopeConfig::new("training-42").activate_namespace("encoder::train"));
observe!(Epoch { epoch: 1, loss: 0.125 })?;
# Ok::<(), sun_observations::ObservationError>(())
```

Payloads require `Serialize + 'static`; add `Deserialize` to decode captured evidence.
Level defaults to INFO; WARN/ERROR are severity, never selection bypasses. Put expensive
construction **inside** `observe!` (or `observe_if_active(|| payload)`) so rejection skips
construction, serialization, message and rendering. Admitted sinkless calls still
serialize and call `message()`: no destination is not disabled instrumentation.

### Select what is active

| Control | Question |
| --- | --- |
| Selection | **Whether** this type is active |
| Scope | **Which execution** owns it |
| Routing | **Where** it goes |
| Policy | **How often** selected occurrences are admitted |

```rust
use sun_observations::{global, Selection};
global().set_selection(Selection::new()
    .activate_namespace("service")
    .activate_tags(["production"])
    .exclude_namespace("service::wire")
    .exclude_tags(["expensive"]));
```

Defaults activate nothing. Global and current-scope positives union (namespace OR
tag); **either exclusion vetoes**. A scope cannot undo a global exclusion. `set_selection`
replaces, not merges. Namespace selectors/routes match segment-aware subtrees:
`service::job` includes its descendants, not `service::jobs`. Routing never activates.
Validate external selector strings before builders; invalid selectors panic, whereas
`register_namespace_channel` returns an error.

### Scope and limit high-rate evidence

```rust
use serde::Serialize;
use sun_observations::{observe, start_scope, Observation, Policy, ScopeConfig};

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::train::batch")]
struct Batch { index: usize, norm: f64 }
fn compute_norm() -> f64 { 0.125 }
let _scope = start_scope(ScopeConfig::new("training-42")
    .activate_namespace("encoder::train")
    .override_policy::<Batch>(Policy::EveryN(100)));
for index in 1..=200 {
    observe!(Batch { index, norm: compute_norm() })?;
}
# Ok::<(), sun_observations::ObservationError>(())
```

Only occurrences 100 and 200 construct the payload here. Default `Always` admits
every selected occurrence; `Never` none, `FirstN(n)` / `MaxPerScope(n)` the first n,
`EveryN(n)` the n-th, 2n-th, …; numeric zero rejects all. Budgets belong to **scope
instance + concrete type**, shared by propagated workers. They count admitted
attempts, not durable writes; failed serialization/delivery still consumes admission.

Keep the scope guard alive. Nested scopes have fresh selectors/policies/counters,
not inherited local configuration; dropping restores the predecessor. Reusing a
scope name does not reuse a budget, but makes persisted execution identity ambiguous.

### Propagate execution context

```rust
use sun_observations::certifications::{certify, fact};
use sun_observations::{context, start_scope, Context, ScopeConfig};

let _scope = start_scope(ScopeConfig::new("request-42"));
let captured = Context::capture();
let worker = context::thread::spawn(|| Context::capture().scope_name().map(str::to_owned));
let child_scope = worker.join().expect("worker panicked");
let installed = captured.run(|| Context::capture().scope_name().map(str::to_owned));
certify("execution identity")
    .check("child", fact("worker scope", child_scope).expect(Some(String::from("request-42"))))
    .check("captured", fact("installed scope", installed).expect(Some(String::from("request-42"))))
    .assert_certify();
```

`Context::capture()` plus `enter()`/`run()` installs execution state at an existing
worker boundary; guard drop restores it. `Context::default()` is explicitly empty.
Plain OS threads and async tasks do **not** inherit context. Join workers and propagate
their failures; do not hold thread-local guards across async suspension/migration.

With the built-in Rayon integration, `context::rayon::ContextThreadPool::new(4)?` binds
the context **at construction**, including nested jobs; `pool.install(|| work())` does
not adopt a later caller's context. Build a pool for each intended execution boundary.
See executable [threads](crates/sun-observations/examples/threads.rs) and [Rayon](crates/sun-observations/examples/rayon.rs) workflows.

### Route, retain and persist

Configure global delivery once at application startup. Share one sink per physical
destination. Channels are logical destinations, not namespaces or persisted fields.

```rust,no_run
use serde::Serialize;
use std::sync::Arc;
use sun_observations::*;

#[derive(Serialize, Observation)]
#[observation(namespace = "service::request", tags("production"))]
struct Request { path: &'static str, status: u16 }

let engine = global();
let memory = Arc::new(MemorySink::new());
let file = Arc::new(JsonlSink::with_config("observations.jsonl",
    JsonlConfig::new().flush_ms(1000).file_size_mb(10).trim_fraction(0.10))?);
engine.add_channel_sink("operations", memory.clone());
engine.add_channel_sink("operations", file.clone());
engine.add_channel_sink("operations", Arc::new(ConsoleSink::stderr()));
engine.register_namespace_channel("service", "operations")?;
engine.set_selection(Selection::new().activate_namespace("service"));
let emission = observe!(Request { path: "/health", status: 200 });
let persistence = engine.flush_channels();
emission?;
persistence?;
let snapshot = memory.shared_records();
# let _ = snapshot;
# Ok::<(), Box<dyn std::error::Error>>(())
```

- **MemorySink** is synchronous bounded history (default 1,000,000 records / 10%
  oldest-batch eviction). Shared snapshots retain records after drain/eviction;
  use scoped capture, not an unchecked memory subset, for proofs.
- **JsonlSink** asynchronously appends structured JSONL. `append(path)?` has no size
  limit; file retention removes whole oldest records toward the configured target.
  `file_size_mb` means MiB; an oversized newest record remains whole. Parents must
  exist; constructor success validates config, not asynchronous file access.
- **ConsoleSink** asynchronously writes human output to stdout/stderr or an owned
  writer. Structured Memory/JSONL never request human rendering.

Overlapping namespace routes fan out. Routes to the same channel deduplicate; the
same sink on different matching channels intentionally receives duplicate deliveries.
All targets are attempted on failure; there is no cross-sink rollback or total order.

`Ok(true)` from `observe!` means admitted/dispatched (possibly sinkless); for async
sinks it means **enqueue**, not persistence. Flush is a queue-prefix error barrier,
**not fsync**. Evaluate the barrier even after emission failure, as above; report
both errors if both fail. JSONL/Console queues are unbounded and failures sticky,
with no retry. Global statics are not dropped at exit: explicit shutdown matters.
See [operations](docs/OPERATIONS.md) for retention, errors, custom sinks and shutdown.

### Render human output without changing evidence

Implement the trait manually when you need a persisted `message()` or transient
`render_text()`. Serde `data` remains canonical:

```rust
use serde::Serialize;
use sun_observations::Observation;
#[derive(Serialize)]
struct Epoch { epoch: u64, loss: f64 }
impl Observation for Epoch {
    const NAMESPACE: &'static str = "encoder::train::epoch";
    fn render_text(&self) -> Option<String> {
        Some(format!("{:>5} | {:8.4}", self.epoch, self.loss))
    }
}
```

Rendering runs once on the producer only when a routed human sink requests it;
multiple human sinks share the text. It is literal/multiline output, not persisted
data. `None` uses Console's envelope/message/data fallback. Tables are independent
header/row/footer point Observations, not spans: see [human.rs](crates/sun-observations/examples/human.rs).

### Measure lifecycles with spans

```rust
use serde::Serialize;
use sun_observations::{observe_span, start_scope, Observation, ScopeConfig};
#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::fit")]
struct Fit { samples: usize }
let _scope = start_scope(ScopeConfig::new("fit-42").activate_namespace("encoder"));
let span = observe_span!(Fit { samples: 128 })?;
// Run the operation while the guard is alive.
span.finish()?;
# Ok::<(), sun_observations::ObservationError>(())
```

One admission produces start with payload and end with null data. Explicit `finish()`
reports end errors; normal Drop also ends, but cannot return errors. Scope/routes/sinks
freeze at start. Rejection builds nothing. Abort, leaks, I/O failure and retention can
leave incomplete pairs. Spans do not invoke point renderers or imply a parent tree.

Pair by `(proc, scope, namespace, span)`, never adjacency; duration is end minus start
`proc_ms`, not wall-clock `ts`. Do not combine elapsed clocks across engines or sum
overlapping intervals as disjoint work. See [FORMAT v1](docs/FORMAT.md#lifecycle-and-timing).

## Capture production evidence

Capture in the tour runs the **real production call** and preserves its result in
`CapturedRun<T>`. Keep/reuse a `TypedView`: each `typed::<T>()` call decodes again.
Its `source(name)` is the same generic `PreparedSource` used for ordinary values,
with optional original-record provenance in bounded diagnostics.

Capture selection replaces global admission/delivery **inside the captured context**
without mutating shared configuration. Scope policy/local exclusions still apply.
Parallel captures isolate storage; nested captures are **exclusive** (inner evidence
is absent from outer). Child scopes inherit attachment, and explicit Context/thread/
Rayon propagation carries it. Finish spans and join/drop propagated owners inside
the closure; escaped owners make completeness unprovable even before late emission.
Panic unwinding restores context, but the panic propagates.

`run.dataset()` is a fixed snapshot. For persisted or existing memory evidence,
construct an `ObservationDataset`, query envelope fields, then strictly decode:

```rust
use serde::{Deserialize, Serialize};
use sun_observations::{MemorySink, Observation};
use sun_observations::evidence::{EnvelopeQuery, ObservationDataset};
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "jobs::completed")]
struct Completed { job: u64 }
let memory = MemorySink::new();
let dataset = ObservationDataset::from_memory(&memory);
let selected = dataset.query(EnvelopeQuery {
    namespace_subtree: Some("jobs"), scope: Some(Some("run-42")),
    ..Default::default()
});
let completed = selected.typed::<Completed>()?;
let source = completed.source("completed jobs");
# let _ = source;
# Ok::<(), Box<dyn std::error::Error>>(())
```

This example shows selection over an existing sink; an empty snapshot proves no
production effect. Snapshot membership is fixed, not proof of complete retention.
For captures, **always start with `run.certify`**. For external files, establish
completeness through the artifact's own contract before making absence/count claims.
Load JSONL fallibly (`BufRead::lines` → deserialize `ObservationRecord` → `Arc` →
`from_shared`); never discard malformed lines. Strict typed decoding includes matching
points/span starts and skips ends, not malformed matching payloads.

Filter `proc`/`scope` before comparing repeated execution keys, or include execution
identity in keys. Source indices are source-local; original coordinates are available
through `typed.sources()[index]`. See the [capture/adapter reference](docs/CAPTURE.md).

## Cookbook

| If you want to prove… | Start here |
| --- | --- |
| Exact count / forbidden absence | `rows.count().expect(n)` / `rows.matching_count(forbidden).expect(0usize)`; capture completeness is prerequisite |
| Row structural validity | Count invalid rows as zero; `unique` / `contiguous` reports with `expect_valid()` |
| Producer/consumer correspondence | `compare_with(...).on(...)` with composite domain keys, then `comparison_fact().expect_valid()` |
| Semantic equality / transformation | Reuse the pair for `equal` / `check_pairs` |
| Approximate equality | `fact(...).expect(close_to(expected).absolute(t).relative(r))` or numeric `difference()` |
| Correlation **and** scale | `correlation()` + `linear_fit()` + direct `difference()`; add temporal checks separately |
| Broken partitions / anomalies | `group_by` + `GroupExpectation`, cached `numeric` + outliers/jumps |
| Stale or shifted evidence | Partition one temporal domain; `lag_relation_by` + `LagExpectation`, plus exact known offsets |
| Fail closed on unavailable evidence | `try_fact(...).expect(...)`, dependent checks only when preparation succeeded; independent sections still run |
| Real production behavior | `capture_observations(..., production_call)` → `run.certify` → ordinary + typed sources → one assertion |
| Negative-path behavior | Check rejection category, forbidden downstream absence, promised effects absent, explanatory evidence and an independent healthy path |

The [mixed evidence workflow](crates/sun-observations/examples/skill_workflows.rs)
combines canonical input/output, internal evidence, an independent formula, grouping
and lag. Its documentation certification also runs the negative-path absence proof
and independently parses the JSONL artifact.
[Module transfer](crates/sun-observations/examples/module_relation.rs)
shows direction/scale controls and bounded diagnostics.

Run the [ordinary keyed/statistical certification](crates/sun-observations/examples/certification.rs)
with `cargo run -p sun-observations --example certification`.
Run linked runtime examples with `cargo run -p sun-observations --example NAME`:
[basic](crates/sun-observations/examples/basic.rs), [scoped](crates/sun-observations/examples/scoped.rs), [channels](crates/sun-observations/examples/channels.rs),
[jsonl](crates/sun-observations/examples/jsonl.rs), [human](crates/sun-observations/examples/human.rs), [spans](crates/sun-observations/examples/spans.rs),
[threads](crates/sun-observations/examples/threads.rs); [rayon](crates/sun-observations/examples/rayon.rs) needs no feature flag.
Adapter examples use `cargo run -p sun-observations --example skill_workflows`
or `--example module_relation`. The jsonl example appends `observations.jsonl`;
the mixed workflow writes a temporary artifact, independently validated by its
documentation certification.

## Production configuration and guarantees

- Select a small semantic subtree; use scope-local policies for high-rate diagnostics.
  Severity does not bypass exclusions. Diagnostic sessions must deliberately replace
  global exclusions if they need those types.
- Keep canonical outputs authoritative. Do not add Observations just to mirror a return
  value; instrument otherwise invisible boundaries/decisions for a concrete consumer.
- Quiesce producers, finish spans, join workers, then flush **every relevant sink**,
  including retained detached ones. Check/report emission and persistence failures.
- Async queues have no backpressure or bounded backlog. Memory/file retention bounds
  history, not async queue RAM; held snapshots also keep evicted records alive.
- No automatic retry, multi-sink transaction, writer coordination on one path,
  cross-sink total order, distributed tracing or automatic async-task propagation.
- JSONL is UTF-8 compact JSON + LF. Optional envelope fields are omitted; tags,
  channels and human text are not persisted. Nonfinite floats follow Serde JSON
  null semantics: use explicit typed states when that distinction matters.

Precise storage/lifecycle guarantees live in [FORMAT.md](docs/FORMAT.md); delivery,
retention, custom sinks and failure handling in [OPERATIONS.md](docs/OPERATIONS.md).

## Performance at a glance

Rejected calls avoid payload work; admitted calls perform real serialization.
Human formatting is dormant on structured routes. Prepare/decode/compare once and
reuse numeric projections/results. One async sink has one writer: **enqueue rate is
not persistence throughput**. Benchmark representative payloads and actual storage.

[Benchmark navigation and charts](docs/BENCHMARK_CHARTS.md) separates runtime,
capture/cache, bounded-writer follow-up and earlier certification measurements.
Reports retain host/toolchain, sample counts, timing boundaries and limitations;
no result is a universal speedup, SLA, or correctness threshold.
[Historical investigations](docs/README.md#historical-evidence) are not current guidance.

## Reference and development

| Need | Reference |
| --- | --- |
| Generic certification semantics | [Focused API reference](docs/CERTIFICATIONS_API.md) |
| Observation capture, query, decode, provenance | [Capture/adapter reference](docs/CAPTURE.md) |
| Delivery/storage | [Operations](docs/OPERATIONS.md), [FORMAT v1](docs/FORMAT.md) |
| Exhaustive agent API inventory | Package-owned [certifications skill](crates/sun-observations/agent-skills/certifications/SKILL.md), [evidence skill](crates/sun-observations/agent-skills/sun-observations/SKILL.md) |
| Agent tooling | [Installation/synchronization](docs/AGENT_SKILLS.md); root `.agents/skills` are byte-synchronized mirrors, not edit targets |
| How SunObservations validates itself | [Framework release validation](docs/RELEASE_VALIDATION.md), **not** an application certification tutorial |

The README is also runtime rustdoc: its Rust snippets are doctests. Documentation
certifications validate links/anchors, skill mirrors, current API examples and retained
benchmark samples. From the workspace root:

```sh
cargo fmt --all --check
cargo check --workspace --all-features --all-targets
cargo test --workspace --all-features
cargo test -p sun-observations-certifications --test documentation_certification
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

For default-feature checks, executable examples, downstream compile contracts and
release procedures, see [release validation](docs/RELEASE_VALIDATION.md#required-release-commands).
Benchmark reproduction and offline chart checking belong to the
[benchmark guide](docs/BENCHMARK_CHARTS.md), not the getting-started path.
