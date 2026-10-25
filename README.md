# SunObservations

SunObservations is a Rust framework for typed structured logging, runtime diagnostics, profiling, generated input exploration, and semantic certification.
## Table of contents

- [Overview](#overview)
- [Five-minute tour](#five-minute-tour)
- [Performance](#performance)
- [How the framework fits together](#how-the-framework-fits-together)
- [Certifications](#certifications)
- [Domain generation](#domain-generation)
- [Observations](#observations)
- [Capture production evidence](#capture-production-evidence)
- [Certification methodology](#certification-methodology)
- [How Domain generation works internally](#how-domain-generation-works-internally)
- [Complete supported client API](#complete-supported-client-api)
- [Cookbook](#cookbook)
- [Production configuration and guarantees](#production-configuration-and-guarantees)
- [Agent integration](#agent-integration)
- [Development and validation](#development-and-validation)

## Overview

SunObservations combines typed production evidence, semantic certification, generated input domains, and reusable analysis in one Rust framework.

It provides one aggregated proof boundary, typed failure classes, bounded Domain generation, semantic negative-space exploration, concrete regressions plus generated cases, production Observations instead of test-only hooks, identity-first correspondence, reusable numerical analysis, deterministic replay, and a low-cost rejected Observation path.

Use the most authoritative evidence available: canonical return value or artifact first, then promised external state or effect, then a typed Observation for internal facts that would otherwise disappear.

## Five-minute tour

### Certify ordinary Rust values

A Fact records evidence; an Expectation decides acceptance. Checks collect results,
then one final assertion proves the whole named contract. No runtime or Serde is needed:

```rust
use sun_observations::certifications::{certification, fact, try_fact};

let output = [1u64, 2, 3];
let parsed = "3".parse::<usize>();
let mut proof = certification("exported rows");
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

### Add generated Domain coverage

When one contract should hold for a family of inputs, keep meaningful concrete regressions and add a typed Domain instead of hand-writing every boundary combination.

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
struct Request { count: usize }

fn execute(input: &Request) -> Result<usize, &'static str> {
    if !(1..=9).contains(&input.count) { return Err("invalid count"); }
    Ok(input.count * 2)
}

let mut proof = certification("accepted count doubles").executor(execute);
proof.data_source([Request { count: 3 }]);
proof.domain(domain!(Request { count.in_range(1..=9) }));
proof.cases_budget(32);

let case = proof.case();
proof.check(
    fact!(*case.result())
        .expect(fact!(case.input().count * 2))
        .description("production result follows accepted input"),
);
proof.assert_certify();
~~~

Real inputs execute first and prefill coverage. Generated positive inputs must succeed and satisfy checks; generated invalid representatives must be rejected by the boundary that owns validation. Domain generation is bounded and deterministic rather than an eager Cartesian product.

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
    CaptureConfig::new().activate_tag("evidence").unwrap(),
    || charge(&orders),
);
let decisions = run.to_typed::<RateSelected>();
let mut proof = run.certification("billing production contract");
proof.check("call succeeded", try_fact("charge", run.result().as_ref().map(|_| true)).expect(true));
proof.check("decodable evidence", try_fact("rates", decisions.as_ref().map(|_| true)).expect(true));
if let (Ok(receipts), Ok(decisions)) = (run.result(), &decisions) {
    let input = proof.collection("orders", &orders);
    let output = proof.collection("receipts", receipts);
    let transfer = input.compare_with(&output).on(|v| v.id, |v| v.order);
    proof.check("one receipt per order", transfer.paired_comparison_fact().expect_valid());
    proof.check("canonical totals", transfer.check_pairs(
        |a, b| b.total == if a.net >= 100 { a.net * 90 / 100 } else { a.net }, 3,
    ).expect_valid());
    let rates = decisions.source("internal rate decisions");
    let selection = input.compare_with(&rates).on(|v| v.id, |v| v.order);
    proof.check("one decision per order", selection.paired_comparison_fact().expect_valid());
    proof.check("rate policy", selection.check_pairs(
        |a, b| b.rate == if a.net >= 100 { 90 } else { 100 }, 3,
    ).expect_valid());
}
proof.assert_certify();
```

`run.certification(...)` adds an `EvaluationError` prerequisite if capture lost records,
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

## Performance

Performance is a framework feature, but these charts are **recorded benchmark evidence, not SLAs**. The principal runs were collected on an AMD Ryzen 9 5950X with 64 GiB RAM on Windows 11 using MSVC Rust 1.96.0, without CPU pinning or host isolation. File benchmarks use ordinary buffered OS writes; flush is not fsync. Different payloads, storage, allocators, and machines can change the results.

Tracked metrics live under `scripts/*.csv`. `scripts/render_benchmark_charts.py` validates the retained evidence and deterministically renders the SVG assets.

### Disabled Observation cost

![Warm observation hot path](img/benchmark-hot-path.svg)

The recorded warm rejected path is about 7.3 ns/call. `EveryN(100)` mostly exercises rejection. An admitted sinkless Observation still serializes its payload, so “no sink” is not the same as disabled instrumentation.

### Scoped capture follows normal emission cost

![Scoped capture emission](img/benchmark-capture-emission.svg)

For the small two-field payload in the current capture dataset, scoped capture emission tracks an independently configured in-memory engine closely from 1k to 1M records. Capture finalization is measured separately; this is not a universal payload-size bound.

### Prepared evidence avoids repeated work

![Prepared evidence reuse](img/benchmark-prepared-reuse.svg)

At 1M rows, six requests over one prepared correspondence took a 121.2639 ms median versus 357.8696 ms when keys, correspondence, and projections were rebuilt for every request. Prepared semantic views avoid repeating decode, alignment, and projection work.
### Typed decode performance

![Typed decode scaling](img/benchmark-observation-decode.svg)

The retained certification benchmark measures strict typed decoding from 1k through 1M records, including filtering, Serde conversion, and source-coordinate construction.

### Lag search performance

![Lag search scaling](img/benchmark-lag-window.svg)

The keyed Pearson benchmark shows that temporal analysis cost grows with both evidence size and the requested candidate window.
### Domain planning is frontier-bounded

![Domain frontier](img/benchmark-domain-frontier.svg)

For four active enum dimensions at budget 64, growing positive potential from 256 to 1,048,576 did not trigger eager Cartesian construction. Compile and bounded traversal remained in the same small-millisecond range for these fixtures.

![Domain budget](img/benchmark-domain-budget.svg)

With eight 32-choice enums, the positive Cartesian potential is about 1.1 trillion combinations, yet full planned execution follows the requested case budget. This demonstrates bounded frontier exploration; it does **not** claim that arbitrary constraint solving is constant time. Constraint-heavy negative combinations can be substantially more expensive.

### Producer throughput is not persistence throughput

![JSONL producer and end-to-end scaling](img/benchmark-jsonl-scaling.svg)

Multiple producers share one JSONL writer. Producer enqueue rate can improve while through-flush service remains bounded by that writer. Async sink queues are currently unbounded, so enqueue throughput is not bounded-memory or durable-storage throughput.

### Selector scale

![Warm selector scale](img/benchmark-selector-scale.svg)

The warm selector path is measured through 10,000 irrelevant rules per category while namespace depth and Observation tags remain fixed. Setup and cold resolution are outside this chart.

### Sink topology

![Sink comparison](img/benchmark-sink-comparison.svg)

The no-sink control still serializes admitted payloads. Memory, JSONL, combined delivery, and fan-out measure different promised effects; the fan-out fixture uses four MemorySinks across two channels.

### Writer batching

![Writer batch tradeoff](img/benchmark-writer-batch.svg)

The bounded writer chunk helps saturated tracked cases but is not a universal speedup, queue backpressure, or a payload-size limit.

### Human rendering
![Structured control](img/benchmark-human-structured.svg)

![Console scaling](img/benchmark-console-scaling.svg)

![Console rendering](img/benchmark-console-rendering.svg)
Human formatting is requested only by human sinks. Structured-only routes do not need human rendering. Console measurements use file-backed output rather than terminal painting, so a real terminal can be slower.

### Reproduce or verify the charts

~~~sh
python scripts/render_benchmark_charts.py
python scripts/render_benchmark_charts.py --check
~~~

Fresh measurement commands include:

~~~sh
cargo bench -p sun-observations --bench runtime
cargo bench -p sun-observations --bench batch
cargo bench -p sun-observations --bench human
cargo test -p sun-observations --release --bench capture -- --measure
cargo test --release -p sun-observations --bench generative_certifications -- --measure
~~~

The retained benchmark CSV files under `scripts/` are the measurement evidence; README charts are deterministic views of those datasets.

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
- [Common recipes](#cookbook), [production guarantees](#production-configuration-and-guarantees), or the [complete client API](#complete-supported-client-api).

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
Normal certification tests finish with `proof.assert_certify()`, the aggregated proof boundary.
`proof.report()` is reserved for programmatic/tooling inspection where a nonpanicking
`CertificationReport` is itself needed; do not replace the normal test assertion with manual
report checks. Do not unwrap acceptance midway: that loses collect-all diagnostics. Fallible
dependent preparation should become a failed check, while unrelated sections still run.

### Name sources and prove collection integrity

Use `source(name, &value)` for one value, `collection` or `proof.collection` for
rows. A singular `&Vec<T>` is one Vec value, not a row collection. Keep borrowed inputs alive.

```rust
use sun_observations::certifications::*;

let ids = [10u64, 11, 12];
let status = String::from("ready");
let mut proof = certification("published artifact");
let metadata = proof.source("status", &status);
let rows = proof.collection("artifact ids", &ids);
proof.check("ready", metadata.single_map(|s| s.clone()).expect("ready"));
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
let mut proof = certification("producer to consumer");
let left = proof.collection("producer", &producer);
let right = proof.collection("consumer", &consumer);
let pair = left.compare_with(&right).on(|v| (v.fold, v.row), |v| (v.fold, v.row));
proof.check("exact unique keys", pair.paired_comparison_fact().expect_valid());
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
let mut proof = certification("numeric transfer");
let left = proof.collection("reference", &expected);
let right = proof.collection("optimized output", &actual);
let pair = left.compare_with(&right).on(|v| v.0, |v| v.0);
let values = pair.numeric(|v| v.1, |v| v.1);
proof.check("keys", pair.paired_comparison_fact().expect_valid());
proof.check("association", values.correlation().expect(CorrelationExpectation::spearman_at_least(0.99).unwrap()));
proof.check("scale", values.linear_fit().slope().expect(close_to(1.0).unwrap().absolute_tolerance(1e-5).unwrap()));
proof.check("fit", values.linear_fit().expect(
    LinearFitExpectation::slope_between(0.999, 1.001).unwrap()
        .and_intercept_between(-1e-5, 1e-5).unwrap().and_r_squared_at_least(0.99).unwrap(),
));
proof.check("direct error", values.difference().expect(
    DifferenceExpectation::max_absolute_at_most(1e-5).unwrap(),
));
proof.check("scalar tolerance", fact("total", 13.000001).expect(
    close_to(13.0).unwrap().absolute_tolerance(1e-5).unwrap().relative_tolerance(0.0).unwrap(),
));
proof.assert_certify();
```

`correlation()` exposes Pearson/Spearman; `linear_fit()` fits right on left and
exposes slope/intercept/R²/residuals; `difference()` measures **right minus left**,
not fitted residuals. Prepared numeric pairs cache projections and compact results
(including errors); requesting these views does not repeat decoding or correspondence.
The `unwrap()` calls above validate fixed scenario configuration, not acceptance.
For external configuration, handle builder errors explicitly. `close_to(x).unwrap()` accepts
`abs(actual-x) <= absolute + relative*abs(x)`; both defaults are `1e-9`.

### Inspect groups, anomalies and temporal lag

Global scores can hide broken partitions. Group by domain identity, then choose
scenario-owned thresholds. Outliers use robust MAD; jumps inspect **source order**,
so sort into domain time order first when arrival order is not meaningful.

```rust
use sun_observations::certifications::*;

let samples = [(0u8, 10.0), (0, 10.1), (0, 9.9), (1, 10.0), (1, 10.2), (1, 9.8)];
let mut proof = certification("sensor partitions");
let rows = proof.collection("samples in time order", &samples);
let groups = rows.group_by(|v| v.0);
proof.check("each sensor", fact("sensor summaries", groups.numeric(|v| v.1).summaries()).expect(
    GroupExpectation::new(3, 9.5, 10.5 ).unwrap(),
));
for group in groups.iter() {
    let values = group.numeric(|v| v.1);
    proof.section(format!("sensor {}", group.key), |c| {
        c.check("robust outliers", try_fact("sensor values", values.outliers(OutlierConfig::default()))
            .expect(AtMostAnomalies(0)));
        c.check("sudden jumps", try_fact("ordered sensor values", values.jumps(JumpConfig::new(Some(0.5), None, 1e-9).unwrap())).expect(AtMostAnomalies(0)));
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
let mut proof = certification("known one-tick delay");
let left = proof.collection("sent", &sent);
let right = proof.collection("received", &received);
let delayed = left.compare_with(&right).on(|v| v.0 + 1, |v| v.0);
proof.check("exact known tick offset", delayed.paired_comparison_fact().expect_valid());
proof.check("payload preserved", delayed.equal(|v| v.1, |v| v.1, 3).expect_valid());
proof.check("delay", left.lag_with(&right, |v| *v, |v| *v, LagConfig {
    range: LagRange::new(-1, 1 ).unwrap(), measure: LagMeasure::Pearson,
}).expect(LagExpectation { best_lag: 1 }));
proof.assert_certify();
```

Use one unique temporal domain per search: partition repeated fold/target/execution
keys first. `LagAlignment` distinguishes expected boundary truncation from interior
gaps; undefined candidates or interior missing keys cannot establish the expected lag.
When offsets are known, prove them exactly too—lag is diagnosis, not a substitute for
causal identity; see the complete client API section below for correspondence, relation, and lag.

## Domain generation

A Domain describes **where a semantic contract must hold**. Checks describe **what must hold** after execution. Use a Domain when one semantic owner promises behavior over a family of inputs that the typed DSL can faithfully express.

Generate invalid states only against the owner that owns validation or rejection. A downstream owner that receives a validated type should be certified over the valid states that type can represent, not impossible raw states.

### Define a generated input

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
enum Mode { Fast, Accurate }

#[derive(Clone, DomainInput)]
struct Request {
    mode: Mode,
    window: usize,
    values: Vec<f64>,
}
~~~

The derive supports concrete named structs/enums plus integer primitives, `f32`, `f64`, `bool`, `String`, and `Vec<T: DomainInput>`. Generic input types are not supported by the derive.

### Constructive value spaces

Every active field needs one constructive specification.

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
struct Input {
    exact: u8,
    choice: i32,
    bounded: i64,
    split: i64,
}

let domain = domain!(Input {
    exact.fixed_value(7),
    choice.selected_values([-1, 0, 3]),
    bounded.in_range(-10..=10),
    split.one_of_ranges([-100..=-90, 90..=100]),
});
~~~

`fixed_value` gives one constructive value. `selected_values` gives an explicit finite set. `in_range` describes one ordered interval. `one_of_ranges` preserves disconnected branches rather than flattening their gaps.

`String` currently uses fixed/selected values; there is no general regex or arbitrary-string generator.

### Enums and payloads

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
enum Strategy {
    Direct,
    Window { size: usize },
}

#[derive(Clone, DomainInput)]
struct Input { strategy: Strategy }

let domain = domain!(Input {
    strategy {
        all_enum_values(),
        Strategy::Window {
            size.in_range(1..=8)
        }
    }
});
~~~

`all_enum_values()` or `selected_enum_values(...)` chooses variants. Payload fields still need their own constructive rules inside the variant block.

### Conditions

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput, PartialEq)]
enum Mode { Fast, Accurate }

#[derive(Clone, DomainInput)]
struct Input { mode: Mode, window: usize }

let domain = domain!(Input {
    mode.all_enum_values(),
    when(mode.equal(Mode::Fast)) {
        window.in_range(1..=3)
    },
    when(mode.equal(Mode::Accurate)) {
        window.in_range(4..=8)
    },
});
~~~

Conditional rules are typed. Missing, overlapping, contradictory, or unmaterializable active construction fails closed before generated execution.

### Collections

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
struct Input { values: Vec<i32> }

let domain = domain!(Input {
    values {
        length().in_range(1..=8),
        each(in_range(-10..=10))
    }
});
~~~

For structured elements, use `each { ... }` and ordinary nested rules. Collection length is itself a typed `usize` field and can participate in relations.

### Relations

The DSL supports `equal`, `not_equal`, `less_than`, `less_equal`, `greater_than`, and `greater_equal`.

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
struct Window { start: i64, end: i64 }

let domain = domain!(Window {
    start.in_range(0..=100),
    end.in_range(0..=100),
    start.less_equal(end),
});

~~~

At a validation boundary the runner can explore both satisfying and constraint-breaking representatives. At a downstream validated boundary, describe only reachable valid states.

### Concrete cases plus Domain

A strong owner certification often combines both:

~~~rust
use sun_observations::certifications::*;

#[derive(Clone, DomainInput)]
struct Input { value: i32 }

let mut proof = certification("accepted value roundtrips").executor(
    |input: &Input| if input.value == 0 { Err(()) } else { Ok(input.value) }
);

proof.data_source([Input { value: 17 }]);
proof.domain(domain!(Input { value.in_range(1..=100) }));

let case = proof.case();
proof.check(fact!(*case.result()).expect(fact!(case.input().value)));
proof.assert_certify();
~~~

Keep a concrete case when it has independent regression, fixture, boundary, or diagnostic meaning. Do not manually enumerate ordinary boundaries that the Domain already expresses.

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
let _scope = start_scope(ScopeConfig::new("training-42").activate_namespace("encoder::train").unwrap());
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
    .activate_namespace("service").unwrap()
    .activate_tags(["production"]).unwrap()
    .exclude_namespace("service::wire").unwrap()
    .exclude_tags(["expensive"]).unwrap());
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
    .activate_namespace("encoder::train").unwrap()
    .override_policy::<Batch>(Policy::every_n(100).unwrap()));
for index in 1..=200 {
    observe!(Batch { index, norm: compute_norm() })?;
}
# Ok::<(), sun_observations::ObservationError>(())
```

Only occurrences 100 and 200 construct the payload here. Default `Always` admits
every selected occurrence; `Never` none, `FirstN(n)` the first n,
`EveryN(n)` the n-th, 2n-th, …; both variants require NonZeroU64. Budgets belong to **scope
instance + concrete type**, shared by propagated workers. They count admitted
attempts, not durable writes; failed serialization/delivery still consumes admission.

Keep the scope guard alive. Nested scopes have fresh selectors/policies/counters,
not inherited local configuration; dropping restores the predecessor. Reusing a
scope name does not reuse a budget, but makes persisted execution identity ambiguous.

### Propagate execution context

```rust
use sun_observations::certifications::{certification, fact};
use sun_observations::{context, start_scope, Context, ScopeConfig};

let _scope = start_scope(ScopeConfig::new("request-42"));
let captured = Context::capture();
let worker = context::thread::spawn(|| Context::capture().scope_name().map(str::to_owned));
let child_scope = worker.join().expect("worker panicked");
let installed = captured.run(|| Context::capture().scope_name().map(str::to_owned));
certification("execution identity")
    .check("child", fact("worker scope", child_scope).expect(Some(String::from("request-42"))))
    .check("captured", fact("installed scope", installed).expect(Some(String::from("request-42"))))
    .assert_certify();
```

`Context::capture()` plus `enter()`/`run()` installs execution state at an existing
worker boundary; guard drop restores it. `Context::default()` is explicitly empty.
Plain OS threads and async tasks do **not** inherit context. Join workers and propagate
their failures; do not hold thread-local guards across async suspension/migration.

With the built-in Rayon integration, `context::rayon::ContextThreadPool::with_threads(std::num::NonZeroUsize::new(4).unwrap())?` binds
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
    JsonlConfig::new().flush_interval_ms(1000).unwrap().max_file_size_mb(10).unwrap().trim_fraction(0.10).unwrap())?);
engine.add_channel_sink("operations", memory.clone());
engine.add_channel_sink("operations", file.clone());
engine.add_channel_sink("operations", Arc::new(ConsoleSink::stderr()));
engine.register_namespace_channel("service", "operations")?;
engine.set_selection(Selection::new().activate_namespace("service").unwrap());
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
- **JsonlSink** asynchronously appends structured JSONL. `open_append(path)?` has no size
  limit; file retention removes whole oldest records toward the configured target.
  `max_file_size_mb` means MiB; an oversized newest record remains whole. Parents must
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
See [production configuration and guarantees](#production-configuration-and-guarantees) for retention, errors, custom sinks, and shutdown.

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
let _scope = start_scope(ScopeConfig::new("fit-42").activate_namespace("encoder").unwrap());
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
`CapturedRun<T>`. Keep/reuse a `TypedView`: each `to_typed::<T>()` call decodes again.
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
use sun_observations::evidence::{ObservationsQuery, ObservationDataset};
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "jobs::completed")]
struct Completed { job: u64 }
let memory = MemorySink::new();
let dataset = ObservationDataset::try_from_memory(&memory).unwrap();
let selected = dataset.query(ObservationsQuery::new().namespace_subtree("jobs").scope("run-42"));
let completed = selected.to_typed::<Completed>()?;
let source = completed.source("completed jobs");
# let _ = source;
# Ok::<(), Box<dyn std::error::Error>>(())
```

This example shows selection over an existing sink; an empty snapshot proves no
production effect. Snapshot membership is fixed, not proof of complete retention.
For captures, **always start with `run.certification`**. For external files, establish
completeness through the artifact's own contract before making absence/count claims.
Load JSONL fallibly (`BufRead::lines` → deserialize `ObservationRecord` → `Arc` →
`try_from_records`); never discard malformed lines. Strict typed decoding includes matching
points/span starts and skips ends, not malformed matching payloads.

Filter `proc`/`scope` before comparing repeated execution keys, or include execution
identity in keys. Source indices are source-local; original coordinates are available
through `typed.sources()[index]`. See the [capture tutorial](#capture-production-evidence).

## Certification methodology

### Certification only vs certification + Observations

Use certification alone when canonical inputs, returns, artifacts, or promised external state expose enough evidence. Add Observations when a proof-critical semantic fact disappears inside the real production pipeline.

- **Boundary Observation** records a meaningful transfer between owners or stages.
- **Decision Observation** records a domain-significant accept/reject/select/suppress branch.
- **Diagnostic Observation** records a metric or state needed by a concrete certification or diagnostic workflow.

Observations record facts. Certifications own expectations and pass/fail policy. Do not emit an Observation that merely duplicates an already canonical result.

### Must happen and must never happen

A strong failure-path proof is usually stronger than checking only `Result::is_err()`. When semantically promised, prove the rejection owner/category, absence of prohibited downstream effects, promised cleanup/state, and an independent healthy path.

### Identity, causality, oracle, and controls

Join by explicit domain identity, not vector index, sink order, or formatted field names unless that ordering is contractual. Value agreement does not prove causality: certify row identity, timestamp, horizon, or decision-entry offsets explicitly when they are known.

For optimized or mathematically complex behavior, prefer an independent oracle using a meaningfully different implementation path. When exact expected output is awkward, use a known metamorphic relation such as positive scaling, sign reversal, translation invariance, partition invariance, or a null/control scenario.

### Migrating a single-case test

Choose among four outcomes:

- **Concrete case only** for a historical regression, exact fixture, meaningful boundary, or external scenario.
- **Concrete case + Domain** when the example stays valuable but the same rule is family-wide.
- **Domain only** when the old example was arbitrary and has no independent meaning.
- **Remove** when another certification already proves the same semantic contract without losing diagnostic value.

If one arbitrary example stands in for a family-wide contract, Domain must be considered. Invalid generation belongs to the owner that validates or rejects those states; downstream validated owners receive only reachable valid inputs.

## How Domain generation works internally

This section explains implementation strategy so users can reason about coverage and cost. These planner concepts are **not** the normal client API; application code should use `DomainInput`, `domain!(...)`, runner controls, and checks.

### Compile into a typed constraint model

Before generated execution, the framework compiles the Domain into typed variables, constructive value spaces, conditional contexts, relation constraints, enum coverage catalogs, collection-composition targets, and positive/negative target frontiers. Invalid or unmaterializable specifications fail before generated execution instead of silently dropping rules.

### Representative range anchors

An ordered range is not represented by one random value. The compiler derives deterministic representatives including outer endpoints, a center across disconnected ranges, branch endpoints, boundaries around low/middle/high thirds where representable, and interior representatives. Real-valued ranges use analogous interpolation.

`one_of_ranges(...)` preserves each disconnected branch. Gaps become distinct invalid regions rather than one generic “outside” bucket.

### Negative sectors

Constructive rules define valid space and also allow representative invalid sectors when the underlying type can represent them.

Ranges can produce near/middle/far values below or above the valid range and inside disconnected gaps. Explicit selections can produce nearby/outside alternatives. Enum selections can produce excluded variants. Relations carry their own violation identity.

The planner explores single-constraint negatives before selected simultaneous negative pairs. Therefore `failure_budget` tracks representative semantic failures, not simply the first N rejected calls.

### Enum coverage without eager Cartesian expansion

Enum choices have stable coverage slots. The planner covers useful individual anchors, then feasible pairwise combinations, then higher-order combinations when budget remains. Conditional branches contribute only combinations feasible in that active context.

`require_pairwise_coverage()` turns feasible enum-value and pair coverage into a required proof result. It does not allocate the full product and does not increase the case budget.

### Collection compositions

Collections are not represented only by one repeated element. Depending on the declared element space and available budget, generation can cover representative lengths, uniform anchors, mixed pair compositions, spread compositions, and nested structured elements. Collection length is a typed scalar and can participate in relations.

### Real data executes first

`data_source(...)` inputs run before generated cases. With a Domain, those real inputs are classified into the same semantic coordinate system and prefill already-covered slots.

This keeps historical regressions explicit while avoiding generated work that merely repeats their coverage.

### Breadth before depth

After deterministic anchors, remaining ordered-range exploration is stratified. Range branches are divided into low/middle/high regions and smaller strata. The framework seed deterministically permutes candidate strata and derives local seeds from semantic coordinate identity, so exploration spreads across the space before repeatedly deepening one small region.

### Case-local seeds

`executor_with_seed` receives a deterministic seed derived from certification identity plus case identity. Generated seeds are tied to canonical Domain coordinates; concrete seeds are tied to registered `data_source` ordinals. Adding unrelated generated executions does not intentionally redefine an existing case seed.

### Coverage categories and budgets

Generated execution conceptually progresses through:

1. positive anchors, enum combinations, collection compositions, and bounded free exploration;
2. single-constraint negative representatives;
3. selected multi-negative combinations.

Unused capacity can be redistributed when a category exhausts. `cases_budget` bounds generated exploration; mandatory concrete inputs may execute in addition. `failure_budget` may stop further materialization/execution once enough distinct failure representatives have been retained.

### Why huge Domains stay bounded

A Domain can describe a huge theoretical product without materializing all combinations. The retained benchmark with eight 32-choice enums describes about 1.1 trillion positive combinations, while a budget of 64 executes only a bounded frontier.

This is a bounded/lazy exploration guarantee, **not** a claim that all constraint solving is constant-time. Highly coupled negative constraints can require substantially more search than independent ranges.

### Classification verifies generation

Every materialized input is projected back into Domain coordinates. Generated coverage is counted from the resulting semantic classification, not merely from what the planner intended to create. A target that cannot be faithfully materialized is infeasible or fails closed rather than becoming false coverage.

### Current generation limits

The current DSL intentionally does not provide shrinking, arbitrary regex/string generation, maps/sets, general collection uniqueness constraints, arbitrary collection-order constraints, or unrestricted predicate constraints. Keep such cases as concrete fixtures or validate them at the semantic owner until they have a typed first-class representation.

## Complete supported client API

This section inventories the supported client-facing surface. Framework planner coordinates, solver representation types, and execution adapters are intentionally omitted. Each API item has a minimal usage example; complete workflows appear in the tutorial above.

### Certification construction, facts, and proof boundary

| API | What it does | Minimal example |
| --- | --- | --- |
| `certification(name)` | Starts one named eager proof. | `let mut c = certification("export contract");` |
| `check(name, check)` | Registers one semantic check. | `c.check("count", fact("n", 3).expect(3));` |
| `check_fact_expect(name, observed, expected)` | Exact eager shorthand. | `c.check_fact_expect("version", 2u8, 2u8);` |
| `section(name, f)` | Groups checks under a semantic path. | `c.section("header", |s| { s.check_fact_expect("v", 2, 2); });` |
| `source(name, &value)` | Prepares one borrowed value. | `let src = c.source("status", &status);` |
| `collection(name, &values)` | Prepares a borrowed collection. | `let rows = c.collection("rows", &rows);` |
| `fact(source, value)` | Wraps available typed evidence. | `fact("version", 3u8)` |
| `try_fact(source, result)` | Preserves preparation failure as `EvaluationError`. | `try_fact("parsed", "3".parse::<u64>())` |
| `Fact::details(text)` | Adds diagnostic context. | `fact("state", state).details("after reload")` |
| `Fact::expect(expectation)` | Evaluates an expectation. | `fact("count", 3).expect(3)` |
| `fact!(expression)` | Builds a named eager/deferred fact from source text. | `fact!(case.input().count)` |
| `CheckPlan::description(text)` | Adds per-case diagnostic meaning. | `fact!(*case.result()).expect(4).description("double count")` |
| `assert_certify()` | Final normal-test proof boundary. | `c.assert_certify();` |

Normal certification tests end with `assert_certify()`. Programmatic report inspection exists for framework/tooling scenarios but is not the ordinary test terminator.

### General expectations

| API | What it expresses | Minimal example |
| --- | --- | --- |
| exact equality | Exact value contract. | `fact("id", id).expect(7u64)` |
| `NotEqual(value)` | Exact inequality. | `fact("id", id).expect(NotEqual(0))` |
| `Criterion::greater_than(v)` | Strict lower bound. | `Criterion::greater_than(0).unwrap()` |
| `Criterion::at_least(v)` | Inclusive lower bound. | `Criterion::at_least(3).unwrap()` |
| `Criterion::less_than(v)` | Strict upper bound. | `Criterion::less_than(10).unwrap()` |
| `Criterion::at_most(v)` | Inclusive upper bound. | `Criterion::at_most(10).unwrap()` |
| `Criterion::between(min, max)` | Inclusive interval. | `Criterion::between(0, 10).unwrap()` |
| `criterion(description, class, predicate)` | Custom typed predicate. | `criterion("even", ViolationClass::HardViolation, |v: &u64| *v % 2 == 0)` |
| `close_to(expected)` | Approximate scalar expectation. | `close_to(1.0).unwrap().absolute_tolerance(1e-6).unwrap()` |
| `absolute_tolerance(v)` | Adds absolute tolerance. | `.absolute_tolerance(1e-6).unwrap()` |
| `relative_tolerance(v)` | Adds relative tolerance. | `.relative_tolerance(1e-4).unwrap()` |
| `CountRule::Exact(n)` | Exact collection count. | `rows.count().expect(CountRule::Exact(3))` |
| `CountRule::Minimum(n)` | Minimum count. | `rows.count().expect(CountRule::Minimum(1))` |
| `CountRule::NonEmpty` | At least one item. | `rows.count().expect(CountRule::NonEmpty)` |
| `NumericRule::Finite` | Finite numerical evidence. | `numeric.expect(NumericRule::Finite)` |
| `NumericRule::Positive` | Strictly positive values. | `numeric.expect(NumericRule::Positive)` |
| `NumericRule::NonNegative` | Zero-or-positive values. | `numeric.expect(NumericRule::NonNegative)` |
| `NumericRule::between(min, max)` | Inclusive numeric interval. | `NumericRule::between(-1.0, 1.0).unwrap()` |
| `expect_valid()` / `AllValid` | Structural report must be valid. | `rows.unique(|r| r.id).expect_valid()` |
| `AtMostAnomalies(n)` | Bounds anomaly/jump count. | `fact("outliers", report).expect(AtMostAnomalies(2))` |
| `GroupExpectation::new(...)` | Bounds each group count/mean. | `GroupExpectation::new(3, 0.0, 1.0).unwrap()` |
| `LagExpectation { best_lag }` | Requires one best lag. | `LagExpectation { best_lag: 1 }` |
| `Expectation<T>` | Extension point for reusable domain expectations. | `impl Expectation<MyType> for MyExpectation { ... }` |

### Planned production execution

| API | What it does | Minimal example |
| --- | --- | --- |
| `executor(f)` | Runs the real production function for every case. | `let mut c = certification("x").executor(production);` |
| `executor_with_seed(f)` | Also supplies deterministic case-local seed. | `.executor_with_seed(|input, seed| production(input, seed))` |
| `data_source(inputs)` | Registers meaningful concrete cases. | `c.data_source([Input { n: 3 }]);` |
| `domain(domain)` | Registers generated input contract. | `c.domain(domain!(Input { n.in_range(1..=9) }));` |
| `cases_budget(n)` | Bounds generated exploration. | `c.cases_budget(128);` |
| `failure_budget(n)` | Bounds representative failures. | `c.failure_budget(5);` |
| `seed(n)` | Sets deterministic framework/Domain seed. | `c.seed(184729361);` |
| `report_mode(bool)` | Requests compact coverage/check diagnostics. | `c.report_mode(true);` |
| `require_pairwise_coverage()` | Requires feasible enum values/pairs without increasing budget. | `c.require_pairwise_coverage();` |
| `case()` | Binds symbolic deferred case access. | `let case = c.case();` |
| `case.input()` | Reads materialized input. | `fact!(case.input().n)` |
| `case.result()` | Reads successful positive output. | `fact!(*case.result())` |
| `case.observations::<T>()` | Reads typed captured evidence for that case. | `fact!(case.observations::<Decision>().len())` |

### Domain client API

| API | What it does | Minimal example |
| --- | --- | --- |
| `#[derive(DomainInput)]` | Generates typed schema/value conversion. | `#[derive(Clone, DomainInput)] struct Input { n: i32 }` |
| `domain!(Type { ... })` | Builds a typed Domain. | `domain!(Input { n.in_range(1..=10) })` |
| `fixed_value(value)` | One constructive value. | `field.fixed_value(7)` |
| `selected_values(values)` | Explicit scalar choices. | `field.selected_values([-1, 0, 1])` |
| `in_range(range)` | One ordered range. | `field.in_range(0..=100)` |
| `one_of_ranges(ranges)` | Disconnected ordered ranges. | `field.one_of_ranges([-10..=-1, 1..=10])` |
| `all_enum_values()` | Activates every enum variant. | `mode.all_enum_values()` |
| `selected_enum_values(values)` | Activates listed variants only. | `mode.selected_enum_values([Mode::Fast])` |
| nested `field { ... }` | Descends into structured rules. | `config { window.in_range(1..=8) }` |
| `Enum::Variant { ... }` | Defines variant payload rules. | `Strategy::Window { size.in_range(1..=8) }` |
| `length()` | Addresses collection length as `usize`. | `length().in_range(1..=32)` |
| `each(rule)` | Constructs scalar collection elements. | `each(in_range(-1.0..=1.0))` |
| `each { ... }` | Constructs structured collection elements. | `each { id.in_range(1..=10) }` |
| `when(relation) { ... }` | Activates rules under a typed condition. | `when(mode.equal(Mode::Fast)) { window.in_range(1..=3) }` |
| `equal` | Equality relation. | `left.equal(right)` |
| `not_equal` | Inequality relation. | `left.not_equal(right)` |
| `less_than` | Strict ordering. | `start.less_than(end)` |
| `less_equal` | Inclusive ordering. | `start.less_equal(end)` |
| `greater_than` | Strict reverse ordering. | `end.greater_than(start)` |
| `greater_equal` | Inclusive reverse ordering. | `end.greater_equal(start)` |

### PreparedSource and cached projections

Assume `let rows = collection("rows", &values);`.

| API | What it does | Minimal example |
| --- | --- | --- |
| `name()` | Returns semantic source name. | `rows.name()` |
| `values()` | Borrows source values. | `rows.values()` |
| `entry(index)` | Accesses one value with source-local index/provenance. | `rows.entry(0)` |
| `iter()` | Iterates source entries. | `for entry in rows.iter() { ... }` |
| `with_provenance(&p)` | Attaches `SourceProvenance`. | `rows.with_provenance(&provenance)` |
| `single_map(project)` | Requires one value and projects it. | `rows.single_map(|r| r.id)` |
| `project(project)` | Caches an arbitrary typed projection. | `rows.project(|r| r.label.clone())` |
| `numeric(project)` | Caches an `f64` projection. | `rows.numeric(|r| r.value)` |
| `count()` | Produces row-count fact. | `rows.count().expect(3usize)` |
| `matching_count(predicate)` | Counts matching rows. | `rows.matching_count(|r| r.invalid).expect(0usize)` |
| `unique(key)` | Checks typed key uniqueness. | `rows.unique(|r| r.id).expect_valid()` |
| `contiguous(key, start, end)` | Checks inclusive integer-key continuity. | `rows.contiguous(|r| r.id, 10, 20).expect_valid()` |
| `keys(key)` | Prepares/caches typed keys. | `let keys = rows.keys(|r| r.id);` |
| `compare_with(&other)` | Starts typed correspondence. | `rows.compare_with(&other)` |
| `group_by(key)` | Groups by semantic key. | `rows.group_by(|r| r.fold)` |
| `lag_self(project, config)` | Searches keyed lag within one source. | `rows.lag_self(|r| (r.tick, r.value), config)` |
| `lag_with(&other, left, right, config)` | Searches keyed lag between sources. | `rows.lag_with(&other, |r| (r.tick, r.value), |r| (r.tick, r.value), config)` |

### Prepared keys and projections

| API | What it does | Minimal example |
| --- | --- | --- |
| `PreparedKeys::indices()` | Borrows cached key/index pairs. | `keys.indices()` |
| `PreparedKeys::compare_with(&other)` | Compares compatible prepared key views. | `left_keys.compare_with(&right_keys)` |
| `PreparedValues::values()` | Borrows cached typed projections. | `labels.values()` |
| `PreparedValues::source()` | Re-exposes the source behind a projection. | `labels.source()` |
| `PreparedNumeric::values()` | Borrows cached numeric values. | `numeric.values()` |
| `series()` | Builds a reusable numeric series. | `numeric.series()` |
| `summary()` | Computes ordinary summary. | `numeric.summary()` |
| `robust_summary()` | Computes robust summary. | `numeric.robust_summary()` |
| `quantile(q)` | Computes a quantile. | `numeric.quantile(0.95)` |
| `expect(rule)` | Applies a numeric integrity rule. | `numeric.expect(NumericRule::Finite)` |
| `outliers(config)` | Runs robust outlier analysis. | `numeric.outliers(OutlierConfig::default())` |
| `jumps(config)` | Runs adjacent-jump analysis. | `numeric.jumps(JumpConfig::new(Some(1.0), None, 1e-9).unwrap())` |

### Grouped evidence

| API | What it does | Minimal example |
| --- | --- | --- |
| `GroupedView::iter()` | Iterates semantic groups. | `for group in groups.iter() { ... }` |
| `GroupedView::numeric(project)` | Prepares one numeric field for every group. | `groups.numeric(|r| r.value)` |
| `Group::len()` | Group size. | `group.len()` |
| `Group::is_empty()` | Tests group emptiness. | `group.is_empty()` |
| `Group::values()` | Borrows group members. | `group.values()` |
| `Group::iter()` | Iterates group entries. | `group.iter()` |
| `Group::numeric(project)` | Numeric view of one group. | `group.numeric(|r| r.value)` |
| `GroupedNumeric::summaries()` | Per-group ordinary summaries. | `groups.numeric(|r| r.value).summaries()` |
| `summaries_with_robust()` | Ordinary plus robust summaries. | `groups.numeric(|r| r.value).summaries_with_robust()` |

### Typed correspondence and pair analysis

Assume `let builder = left.compare_with(&right);`.

| API | What it does | Minimal example |
| --- | --- | --- |
| `on(left_key, right_key)` | Builds canonical typed correspondence. | `builder.on(|a| a.id, |b| b.id)` |
| `on_presorted(left_key, right_key)` | Uses and validates promised sorted keys. | `builder.on_presorted(|a| a.id, |b| b.id)` |
| `PairedComparison::name()` | Returns diagnostic pair name. | `paired.name()` |
| `comparison()` | Returns structural correspondence report. | `paired.comparison()` |
| `matched_pairs()` | Iterates matched key/entries. | `for (_, a, b) in paired.matched_pairs() { ... }` |
| `paired_comparison_fact()` | Converts structural correspondence to a fact. | `paired.paired_comparison_fact().expect_valid()` |
| `equal(left_value, right_value, top)` | Checks exact equality across matched pairs. | `paired.equal(|a| a.value, |b| b.value, 5)` |
| `check_pairs(predicate, top)` | Applies one custom predicate to all matched pairs. | `paired.check_pairs(|a, b| a.value <= b.limit, 5)` |
| `numeric(left_value, right_value)` | Caches aligned `f64` projections. | `paired.numeric(|a| a.value, |b| b.value)` |

### PreparedNumericComparison

Assume `let n = paired.numeric(...);`.

| API | What it does | Minimal example |
| --- | --- | --- |
| `values()` | Borrows aligned numeric pairs. | `n.values()` |
| `correlation()` | Cached Pearson/Spearman metrics fact. | `n.correlation()` |
| `linear_fit()` | Cached right-on-left fit metrics. | `n.linear_fit()` |
| `relation()` | Composite relation metrics. | `n.relation()` |
| `difference()` | Direct right-minus-left error summary. | `n.difference()` |
| `linear()` | Direct linear metrics view. | `n.linear()` |
| `pearson()` | Scalar Pearson fact. | `n.pearson()` |
| `spearman()` | Scalar Spearman fact. | `n.spearman()` |
| `largest(top)` | Largest direct pair differences for diagnostics. | `n.largest(5)` |

### Statistical expectation builders

CorrelationExpectation examples: `pearson_at_least(0.95)`, `spearman_at_least(0.95)`, `spearman_at_most(0.2)`, `with_bound(CorrelationMeasure::Spearman, Some(0.8), None)`, `and_pearson_bounds(0.8, 1.0)`, `and_spearman_bounds(0.8, 1.0)`, and `bound(CorrelationMeasure::AbsolutePearson, Some(0.9), None)`. CorrelationMeasure selects Pearson, Spearman, AbsolutePearson, or AbsoluteSpearman.

LinearFitExpectation examples: `slope_between(0.99, 1.01)`, `intercept_between(-0.01, 0.01)`, `r_squared_at_least(0.99)`, `residual_mean_absolute_at_most(0.1)`, `residual_max_absolute_at_most(0.5)`, `with_bound(LinearFitMeasure::Slope, Some(0.9), Some(1.1))`, `and_slope_between(0.9, 1.1)`, `and_intercept_between(-0.1, 0.1)`, `and_r_squared_at_least(0.9)`, `and_residual_mean_absolute_at_most(0.1)`, `and_residual_max_absolute_at_most(0.5)`, and `bound(LinearFitMeasure::RSquared, Some(0.99), None)`. LinearFitMeasure selects Slope, Intercept, RSquared, ResidualMeanAbsolute, or ResidualMaxAbsolute. Scalar fit Facts expose `slope()`, `intercept()`, and `r_squared()`, for example `n.linear_fit().slope()`.

DifferenceExpectation examples: `mean_absolute_at_most(0.1)`, `p95_absolute_at_most(0.2)`, `max_absolute_at_most(0.5)`, `with_bound(DifferenceMeasure::MeanAbsolute, None, Some(0.1))`, `and_mean_absolute_at_most(0.1)`, `and_p95_absolute_at_most(0.2)`, `and_max_absolute_at_most(0.5)`, and `bound(DifferenceMeasure::P95Absolute, None, Some(0.2))`. DifferenceMeasure selects MeanAbsolute, P95Absolute, or MaxAbsolute.

RelationExpectation examples: `spearman_at_least(0.95)`, `spearman_at_most(0.2)`, `with_bound(RelationMeasure::Slope, Some(0.9), Some(1.1))`, and `bound(RelationMeasure::RSquared, Some(0.99), None)`. RelationMeasure selects correlation, fit, direct-difference, and residual metrics; for example `RelationMeasure::MeanAbsoluteDifference`.

### Anomaly, jump, and lag configuration

| API | What it does | Minimal example |
| --- | --- | --- |
| `OutlierConfig::default()` | Robust default threshold and zero-MAD policy. | `OutlierConfig::default()` |
| `OutlierConfig::new(threshold, policy)` | Explicit validated outlier config. | `OutlierConfig::new(3.5, ZeroMadPolicy::ExactMedian).unwrap()` |
| `ZeroMadPolicy::absolute_tolerance(v)` | Original-unit tolerance when MAD is zero. | `ZeroMadPolicy::absolute_tolerance(1e-6).unwrap()` |
| `JumpConfig::new(abs, rel, floor)` | Strict absolute/relative jump thresholds. | `JumpConfig::new(Some(2.0), Some(0.5), 1e-9).unwrap()` |
| `LagRange::new(start, end)` | Validates inclusive candidate lag range. | `LagRange::new(-8, 8).unwrap()` |
| `LagConfig { range, measure }` | Chooses range and Pearson/Spearman strength. | `LagConfig { range: LagRange::new(-2, 2).unwrap(), measure: LagMeasure::Pearson }` |
| `LagReport::zero_lag()` | Returns zero-lag candidate when present. | `report.zero_lag()` |
| `LagAlignment::has_interior_gaps()` | Detects interior missing-key gaps. | `candidate.alignment.has_interior_gaps()` |

### Direct analysis API

Prepared views are preferred when evidence is reused. Direct helpers remain useful for one-off independent analysis and return typed data/errors rather than assertions.

| API | What it does | Minimal example |
| --- | --- | --- |
| `NumericSeries::new(iterator)` | Wraps a cloneable numeric iterator. | `let s = NumericSeries::new(values.iter().copied());` |
| `iter()` | Replays the series iterator. | `for value in s.iter() { ... }` |
| `finite_count()` | Counts finite values. | `s.finite_count()` |
| `summary()` | Computes count/mean/variance/min/max summary. | `s.summary()` |
| `quantile(q)` | Computes a validated quantile. | `s.quantile(0.95)` |
| `robust_summary()` | Computes median/MAD-style robust summary. | `s.robust_summary()` |
| `integrity(rule)` | Applies a `NumericRule` to the series. | `s.integrity(NumericRule::Finite)` |
| `outliers(config)` | Finds robust outliers. | `s.outliers(OutlierConfig::default())` |
| `jumps(config)` | Finds adjacent jumps in source order. | `s.jumps(JumpConfig::new(Some(1.0), None, 1e-9).unwrap())` |
| `check_count(actual, rule)` | Checks a count directly. | `check_count(rows.len(), CountRule::Minimum(1))` |
| `check_unique(values, key)` | Checks key uniqueness. | `check_unique(&rows, |r| r.id)` |
| `check_contiguous(values, key, start, end)` | Checks an inclusive integer-key interval. | `check_contiguous(&rows, |r| r.id, 1, 10)` |
| `check_same_keys(left, right, ...)` | Compares key sets. | `check_same_keys(&a, &b, |x| x.id, |x| x.id)` |
| `join_by(...)` | Builds typed correspondence without a sorted-input promise. | `let j = join_by(&a, &b, |x| x.id, |x| x.id);` |
| `join_presorted_by(...)` | Uses and validates sorted-key inputs. | `let j = join_presorted_by(&a, &b, |x| x.id, |x| x.id)?;` |
| `Join::comparison()` | Reads structural correspondence. | `j.comparison()` |
| `Join::matched_pairs()` | Iterates matched rows. | `j.matched_pairs()` |
| `Join::require_complete()` | Fails if correspondence is incomplete. | `j.require_complete()?` |
| `linear_metrics(pairs)` | Computes direct linear metrics. | `linear_metrics(pairs.iter().copied())` |
| `pearson(pairs)` | Computes Pearson correlation. | `pearson(pairs.iter().copied())` |
| `spearman(pairs)` | Computes Spearman correlation. | `spearman(pairs.iter().copied())` |
| `correlation(pairs)` | Computes combined correlation metrics. | `correlation(pairs.iter().copied())` |
| `linear_fit(pairs)` | Computes right-on-left fit metrics. | `linear_fit(pairs.iter().copied())` |
| `numeric_relation(pairs)` | Computes the composite relation report. | `numeric_relation(pairs.iter().copied())` |
| `difference_summary(pairs)` | Computes direct right-minus-left error. | `difference_summary(pairs.iter().copied())` |
| `lag_relation_by(...)` | Searches keyed lag directly. | `lag_relation_by(&a, &b, |x| (x.tick, x.value), |x| (x.tick, x.value), config)` |

Wrap a direct result with `fact`/`try_fact`, apply an expectation, and still finish the proof with `assert_certify()`.

### Observation declaration and emission API

| API | What it does | Minimal example |
| --- | --- | --- |
| `#[derive(Observation)]` | Generates the typed Observation implementation for a serializable payload. | `#[derive(serde::Serialize, Observation)] struct Event { id: u64 }` |
| `namespace = "..."` | Declares stable persisted semantic identity. | `#[observation(namespace = "service::event")]` |
| `level = INFO/WARN/ERROR` | Declares severity; INFO is default. | `#[observation(namespace = "service::event", level = WARN)]` |
| `tags("...")` | Declares non-persisted selection labels. | `#[observation(namespace = "service::event", tags("evidence"))]` |
| `Observation::NAMESPACE` | Manual implementation namespace constant. | `const NAMESPACE: &'static str = "service::event";` |
| `Observation::LEVEL` | Manual implementation severity constant. | `const LEVEL: Level = Level::Warn;` |
| `Observation::TAGS` | Manual implementation static tags. | `const TAGS: &'static [&'static str] = &["evidence"];` |
| `message()` | Supplies persisted semantic text. | `fn message(&self) -> Option<String> { Some("accepted".into()) }` |
| `render_text()` | Supplies transient human rendering. | `fn render_text(&self) -> Option<String> { Some(format!("{}", self.id)) }` |
| `observe!(value)` | Emits through the global production engine. | `observe!(Event { id: 7 })?;` |
| `observe_if_active(|| value)` | Lazily constructs only after admission. | `observe_if_active(|| Event { id: expensive_id() })?;` |
| `engine.observe_if_active(|| value)` | Lazy emission through an explicit engine. | `engine.observe_if_active(|| Event { id: 7 })?;` |
| `global()` | Gets the process-global engine. | `let engine = global();` |
| `ObservationEngine::isolated()` | Creates an independent engine for explicit runtime/routing work. | `let engine = ObservationEngine::isolated();` |
| `process_id()` | Reads the engine/process identity used in records. | `let proc = global().process_id();` |
| `sink_error_count()` | Reads checked sink-error count. | `let failures = global().sink_error_count();` |
| `set_selection(selection)` | Replaces engine selection. | `global().set_selection(Selection::new().activate_all());` |
| `flush_channels()` | Waits for configured sink barriers and reports errors. | `global().flush_channels()?;` |
| `ObservationError` | Typed serialization/delivery failure. | `fn emit() -> Result<(), ObservationError> { observe!(Event { id: 1 })?; Ok(()) }` |

### Selection API

| API | What it does | Minimal example |
| --- | --- | --- |
| `Selection::new()` / `default()` | Starts with nothing active. | `let s = Selection::new();` |
| `activate_all()` | Activates every type unless excluded. | `s.activate_all()` |
| `activate_namespace(ns)` | Activates one namespace subtree. | `s.activate_namespace("service")?` |
| `activate_tags(tags)` | Activates when any selected tag matches. | `s.activate_tags(["evidence", "diagnostic"])?` |
| `exclude_namespace(ns)` | Vetoes one namespace subtree. | `s.exclude_namespace("service::wire")?` |
| `exclude_tags(tags)` | Vetoes matching tags. | `s.exclude_tags(["expensive"])?` |
| `activates(namespace, tags)` | Inspects positive selection. | `s.activates("service::job", &["evidence"])` |
| `excludes(namespace, tags)` | Inspects exclusion. | `s.excludes("service::wire", &[])` |
| `admits(namespace, tags)` | Evaluates activation minus exclusions. | `s.admits("service::job", &["evidence"])` |
| `namespace::is_normalized_segment(v)` | Checks one namespace segment. | `namespace::is_normalized_segment("train_1")` |
| `namespace::matches_subtree(root, ns)` | Tests segment-aware subtree membership. | `namespace::matches_subtree("service", "service::job")` |
| `namespace::validate_namespace(v)` | Validates a dynamic namespace. | `namespace::validate_namespace("service::job")?` |

### Scope, policy, and context API

| API | What it does | Minimal example |
| --- | --- | --- |
| `start_scope(config)` | Installs a scoped execution context until guard drop. | `let _scope = start_scope(ScopeConfig::new("run-1"));` |
| `ScopeConfig::new(name)` | Creates named scope configuration. | `ScopeConfig::new("run-1")` |
| `selection(selection)` | Replaces the scope-local selection. | `ScopeConfig::new("run").selection(selection)` |
| `activate_all()` | Activates all Observations in this scope. | `ScopeConfig::new("run").activate_all()` |
| `activate_namespace(ns)` | Adds scope-local namespace activation. | `ScopeConfig::new("run").activate_namespace("pipeline")?` |
| `activate_tags(tags)` | Adds scope-local tag activation. | `ScopeConfig::new("run").activate_tags(["evidence"])?` |
| `exclude_namespace(ns)` | Adds scope-local namespace veto. | `ScopeConfig::new("run").exclude_namespace("pipeline::debug")?` |
| `exclude_tags(tags)` | Adds scope-local tag veto. | `ScopeConfig::new("run").exclude_tags(["expensive"])?` |
| `override_policy::<T>(policy)` | Overrides frequency for one Observation type. | `ScopeConfig::new("run").override_policy::<Event>(Policy::first_n(10).unwrap())` |
| `Policy::Always` | Admits every selected occurrence. | `Policy::Always` |
| `Policy::Never` | Admits none. | `Policy::Never` |
| `Policy::FirstN` | Stores a validated first-N policy value. | `Policy::first_n(10).unwrap()` |
| `Policy::EveryN` | Stores a validated every-N policy value. | `Policy::every_n(100).unwrap()` |
| `Policy::first_n(n)` | Constructs positive FirstN policy. | `Policy::first_n(10).unwrap()` |
| `Policy::every_n(n)` | Constructs positive EveryN policy. | `Policy::every_n(100).unwrap()` |
| `Context::default()` | Creates explicitly empty context. | `let empty = Context::default();` |
| `Context::capture()` | Snapshots installed execution state. | `let captured = Context::capture();` |
| `enter()` | Installs a captured context until guard drop. | `let _guard = captured.enter();` |
| `run(operation)` | Installs context for one closure. | `captured.run(|| work())` |
| `scope_name()` | Reads current captured scope name. | `Context::capture().scope_name()` |
| `context::thread::spawn(operation)` | Spawns a thread with propagated context. | `let h = context::thread::spawn(|| work());` |
| `ContextThreadPool::new()` | Builds a context-bound Rayon pool. | `let pool = context::rayon::ContextThreadPool::new()?;` |
| `ContextThreadPool::with_threads(n)` | Builds a context-bound pool with explicit size. | `ContextThreadPool::with_threads(NonZeroUsize::new(4).unwrap())?` |
| `install(operation)` | Runs work in that context-bound Rayon pool. | `pool.install(|| work())` |

### Routing and sink API

| API | What it does | Minimal example |
| --- | --- | --- |
| `add_channel_sink(channel, sink)` | Attaches one sink to a logical channel. | `engine.add_channel_sink("ops", memory.clone());` |
| `register_namespace_channel(ns, channel)` | Routes a namespace subtree to a channel. | `engine.register_namespace_channel("service", "ops")?;` |
| `clear_channel_sinks(channel)` | Detaches sinks from one channel. | `engine.clear_channel_sinks("ops");` |
| `MemorySink::new()` / `default()` | Creates bounded in-memory history. | `let memory = MemorySink::new();` |
| `MemorySink::with_config(config)` | Uses explicit retention. | `MemorySink::with_config(MemorySinkConfig::new(1000, 0.1)?)` |
| `len()` / `is_empty()` | Inspects retained count. | `memory.len()` |
| `config()` | Reads memory retention config. | `memory.config()` |
| `stats()` | Reads retention accounting. | `memory.stats().evicted` |
| `shared_records()` | Snapshots shared records. | `let records = memory.shared_records();` |
| `drain_shared()` | Drains retained shared records. | `let records = memory.drain_shared();` |
| `records()` | Clones retained records. | `let records = memory.records();` |
| `drain()` | Drains cloned records. | `let records = memory.drain();` |
| `MemorySinkConfig::new(max, fraction)` | Validates retention limits. | `MemorySinkConfig::new(1000, 0.1)?` |
| `MemorySinkConfig::default()` | Uses framework defaults. | `MemorySinkConfig::default()` |
| `max_items()` | Reads configured record bound. | `memory.config().max_items()` |
| `compact_fraction()` | Reads eviction batch fraction. | `memory.config().compact_fraction()` |
| `MemoryStats` | Exposes retained/evicted/compaction counts. | `let evicted = memory.stats().evicted;` |
| `JsonlSink::open_append(path)` | Opens append-only JSONL with no size limit. | `JsonlSink::open_append("observations.jsonl")?` |
| `JsonlSink::with_config(path, config)` | Opens file sink with explicit config. | `JsonlSink::with_config("observations.jsonl", JsonlConfig::new())?` |
| `JsonlSink::from_writer(writer)` | Uses an owned writer. | `JsonlSink::from_writer(writer)` |
| `JsonlSink::writer_with_config(writer, config)` | Uses writer plus explicit config. | `JsonlSink::writer_with_config(writer, JsonlConfig::new())` |
| `JsonlConfig::new()` / `default()` | Creates default JSONL config. | `JsonlConfig::new()` |
| `flush_interval_ms(ms)` | Sets background flush interval. | `JsonlConfig::new().flush_interval_ms(250)?` |
| `max_file_size_mb(mib)` | Sets MiB retention limit. | `JsonlConfig::new().max_file_size_mb(10)?` |
| `max_file_size_bytes(bytes)` | Sets byte retention limit. | `JsonlConfig::new().max_file_size_bytes(1_000_000)?` |
| `trim_fraction(f)` | Sets whole-record compaction target. | `JsonlConfig::new().trim_fraction(0.1)?` |
| `stats()` | Reads JSONL runtime stats. | `file.stats()` |
| `is_started()` | Reports whether worker/file startup occurred. | `file.is_started()` |
| `last_error()` | Reads sticky async failure. | `file.last_error()` |
| `ConsoleSink::stdout()` | Builds stdout human sink. | `ConsoleSink::stdout()` |
| `ConsoleSink::stderr()` | Builds stderr human sink. | `ConsoleSink::stderr()` |
| `ConsoleSink::new(writer)` | Uses an owned human writer. | `ConsoleSink::new(writer)` |
| `Sink::write(record)` | Required custom structured delivery hook. | `fn write(&self, record: &ObservationRecord) -> Result<(), SinkError> { ... }` |
| `Sink::flush()` | Required completion barrier. | `fn flush(&self) -> Result<(), SinkError> { ... }` |
| `write_shared(record)` | Optional shared-record hook. | `fn write_shared(&self, record: Arc<ObservationRecord>) -> Result<(), SinkError> { ... }` |
| `wants_render_text()` | Requests producer-side human rendering. | `fn wants_render_text(&self) -> bool { true }` |
| `write_delivery(delivery)` | Receives structured record plus optional human text. | `fn write_delivery(&self, d: ObservationDelivery<'_>) -> Result<(), SinkError> { ... }` |
| `ObservationDelivery::record()` | Borrows structured record. | `delivery.record()` |
| `shared_record()` | Gets shared record ownership when available. | `delivery.shared_record()` |
| `render_text()` | Borrows rendered human text. | `delivery.render_text()` |
| `shared_render_text()` | Gets shared rendered text when available. | `delivery.shared_render_text()` |

### Span API

| API | What it does | Minimal example |
| --- | --- | --- |
| `observe_span!(payload)` | Starts a typed span through the global engine. | `let span = observe_span!(Fit { samples: 128 })?;` |
| `span_if_active(|| payload)` | Lazily starts a global span. | `let span = span_if_active(|| Fit { samples: expensive_count() })?;` |
| `engine.span_if_active(|| payload)` | Lazily starts a span on an explicit engine. | `let span = engine.span_if_active(|| Fit { samples: 128 })?;` |
| `SpanGuard::is_active()` | Reports whether start admission succeeded. | `span.is_active()` |
| `finish()` | Explicitly emits end and returns delivery failure. | `span.finish()?;` |

### Scoped capture API

| API | What it does | Minimal example |
| --- | --- | --- |
| `CaptureConfig::new()` / `default()` | Starts disabled selection with default memory retention. | `CaptureConfig::new()` |
| `selection(selection)` | Replaces capture selection. | `CaptureConfig::new().selection(selection)` |
| `retention(config)` | Sets capture memory retention. | `CaptureConfig::new().retention(MemorySinkConfig::new(10_000, 0.1)?)` |
| `activate_all()` | Captures every non-excluded Observation. | `CaptureConfig::new().activate_all()` |
| `activate_namespace(ns)` | Captures one namespace subtree. | `CaptureConfig::new().activate_namespace("pipeline")?` |
| `activate_namespaces(iter)` | Captures several namespace subtrees. | `CaptureConfig::new().activate_namespaces(["a", "b"])?` |
| `activate_tag(tag)` | Captures one tag. | `CaptureConfig::new().activate_tag("evidence")?` |
| `activate_tags(iter)` | Captures several tags. | `CaptureConfig::new().activate_tags(["evidence", "diagnostic"])?` |
| `exclude_namespace(ns)` | Vetoes one capture namespace. | `CaptureConfig::new().exclude_namespace("pipeline::debug")?` |
| `exclude_namespaces(iter)` | Vetoes several namespaces. | `CaptureConfig::new().exclude_namespaces(["a::debug", "b::debug"])?` |
| `exclude_tag(tag)` | Vetoes one tag. | `CaptureConfig::new().exclude_tag("expensive")?` |
| `exclude_tags(iter)` | Vetoes several tags. | `CaptureConfig::new().exclude_tags(["expensive", "verbose"])?` |
| `capture_observations(config, operation)` | Runs the real production closure under scoped capture. | `let run = capture_observations(config, || production());` |
| `CapturedRun::result()` | Borrows production result. | `run.result()` |
| `dataset()` | Reads the fixed capture snapshot. | `run.dataset()` |
| `status()` | Reads completeness status. | `run.status().is_complete()` |
| `to_typed::<T>()` | Strictly decodes matching captured evidence. | `let decisions = run.to_typed::<Decision>()?;` |
| `certification(name)` | Starts proof with capture completeness prerequisite. | `let mut proof = run.certification("production contract");` |

### Dataset, query, decode, and provenance API

| API | What it does | Minimal example |
| --- | --- | --- |
| `ObservationDataset::try_from_records(records)` | Builds a validated fixed dataset. | `ObservationDataset::try_from_records(records)?` |
| `try_from_memory(&sink)` | Snapshots a MemorySink into a dataset. | `ObservationDataset::try_from_memory(&memory)?` |
| `records()` | Borrows dataset records. | `dataset.records()` |
| `query(query)` | Creates a filtered RecordView. | `dataset.query(ObservationsQuery::new().scope("run-1"))` |
| `to_typed::<T>()` | Strictly decodes matching dataset records. | `dataset.to_typed::<Decision>()?` |
| `ObservationsQuery::new()` / `default()` | Starts unrestricted query. | `ObservationsQuery::new()` |
| `namespace(value)` | Requires exact namespace. | `ObservationsQuery::new().namespace("jobs::completed")` |
| `namespace_subtree(value)` | Selects one namespace subtree. | `ObservationsQuery::new().namespace_subtree("jobs")` |
| `scope(value)` | Requires one scope. | `ObservationsQuery::new().scope("run-1")` |
| `without_scope()` | Requires records with no scope. | `ObservationsQuery::new().without_scope()` |
| `level(level)` | Filters by severity. | `ObservationsQuery::new().level(Level::Info)` |
| `points()` | Selects point records. | `ObservationsQuery::new().points()` |
| `span_starts()` | Selects span start records. | `ObservationsQuery::new().span_starts()` |
| `span_ends()` | Selects span end records. | `ObservationsQuery::new().span_ends()` |
| `span(value)` | Filters one span identity. | `ObservationsQuery::new().span(span_id)` |
| `process(value)` | Filters one process identity. | `ObservationsQuery::new().process(proc_id)` |
| `RecordView::iter()` | Iterates selected records. | `for record in view.iter() { ... }` |
| `RecordView::to_typed::<T>()` | Strictly decodes selected records. | `view.to_typed::<Decision>()?` |
| `TypedView::values()` | Borrows decoded values. | `typed.values()` |
| `sources()` | Borrows original source coordinates. | `typed.sources()` |
| `iter()` | Iterates typed entries. | `for entry in typed.iter() { ... }` |
| `entry(index)` | Reads one TypedEntry. | `typed.entry(0)` |
| `source(name)` | Produces certification-ready PreparedSource with provenance. | `typed.source("decisions")` |
| `TypedEntry` | Carries decoded value, dataset index, and source record. | `let index = typed.entry(0).dataset_index;` |
| `SourceProvenance` | Lets prepared diagnostics resolve original evidence coordinates. | `let prepared = typed.source("decisions");` |
| `DecodeError / SourceContext` | Reports original dataset/namespace/process/scope/span/time coordinates. | `let source = decode_error.source();` |

Persisted wire/schema details stay in [FORMAT v1](docs/FORMAT.md); they are a protocol contract rather than another tutorial.

## Cookbook

| If you want to prove… | Start here |
| --- | --- |
| Exact count / forbidden absence | `rows.count().expect(n)` / `rows.matching_count(forbidden).expect(0usize)`; capture completeness is prerequisite |
| Row structural validity | Count invalid rows as zero; `unique` / `contiguous` reports with `expect_valid()` |
| Producer/consumer correspondence | `compare_with(...).on(...)` with composite domain keys, then `paired_comparison_fact().expect_valid()` |
| Semantic equality / transformation | Reuse the pair for `equal` / `check_pairs` |
| Approximate equality | `fact(...).expect(close_to(expected).unwrap().absolute_tolerance(t).unwrap().relative_tolerance(r).unwrap())` or numeric `difference()` |
| Correlation **and** scale | `correlation()` + `linear_fit()` + direct `difference()`; add temporal checks separately |
| Broken partitions / anomalies | `group_by` + `GroupExpectation`, cached `numeric` + outliers/jumps |
| Stale or shifted evidence | Partition one temporal domain; `lag_relation_by` + `LagExpectation`, plus exact known offsets |
| Fail closed on unavailable evidence | `try_fact(...).expect(...)`, dependent checks only when preparation succeeded; independent sections still run |
| Real production behavior | `capture_observations(..., production_call)` → `run.certification` → ordinary + typed sources → one assertion |
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

Precise persisted schema and lifecycle guarantees remain in [FORMAT v1](docs/FORMAT.md).
## Agent integration

SunObservations ships four package-owned atomic skills:

- `certifications-methodology` for proof design, migration, Domain/Observation decisions, and negative-path methodology;
- `certifications-api` for the client certification DSL;
- `domain-api` for the typed generation DSL;
- `observations-api` for production evidence, capture, routing, sinks, and provenance.

Skills are deliberately compact machine-oriented operational contracts. This README is the human handbook and may explain internals, performance, rationale, and API behavior in much greater detail.

Install and synchronize the skills with:

~~~text
cargo install sun-observations --bin cargo-sun-observations-skills
cargo sun-observations-skills sync
cargo sun-observations-skills sync --skills-dir <PATH>
~~~

See [agent skill installation and synchronization](docs/AGENT_SKILLS.md).

## Development and validation

The README is the canonical user tutorial and supported-client reference. Documentation certification validates its structure, API coverage, links, examples, retained benchmark evidence, synchronized skills, and generated chart freshness.

From the workspace root:

~~~sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p sun-observations
cargo test -p sun-observations-certifications --test documentation_certification
python scripts/render_benchmark_charts.py --check
~~~

Persisted record schema/lifecycle details remain in docs/FORMAT.md. Release-only evidence and procedures remain in docs/RELEASE_VALIDATION.md.
See README.
