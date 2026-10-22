# Observation capture and evidence reference

Technical reference for `sun_observations::evidence`. Start with the
[production workflow](../README.md#observe-and-certify-real-production-behavior);
this page specifies capture, access and provenance boundaries.

Observation-specific evidence access for the logging + diagnostics + profiling +
semantic certification workspace. The evidence adapter composes the runtime and
generic certification layers. Generic certification remains independent of runtime
concepts and state.
Persisted FORMAT v1 is unchanged; human rendering is never analysis data.

## Scoped real-production capture

Use capture_observations(CaptureConfig::new().activate_tags(["evidence"]).unwrap(), || production_call()).
Production emits typed observe!/observe_span! through the process-global lazy engine, with no
injected engine. Config wraps canonical Selection (activate_all, singular/plural namespace/tag
activation and exclusions), plus retention(MemorySinkConfig). No channel/sink/route is required.
The private attachment replaces global selection/delivery in the existing context; it does not
mutate shared configuration. Parallel captures isolate records, nested captures are exclusive,
child scopes and explicit Context/thread/Rayon propagation retain attachment. Guards restore
prior context on return/Result/?/unwind. Join workers and finish spans inside the closure.

CapturedRun<T>::result() -> &T preserves application Result as ordinary data; dataset() returns
a fixed ObservationDataset; to_typed::<O>() strictly decodes a TypedView; status() returns
CaptureStatus{retention,emission_errors,unfinished_contexts}. run.certification(name) constructs a
generic Certification and always records an explicit capture-completeness prerequisite: healthy
capture contributes a passing check, while eviction, ignored serialization errors, or escaped
contexts/spans/workers contribute EvaluationError. Always start Observation-backed proof from
run.certification, not a standalone root over possibly incomplete retained records. TypedView decoding is explicit: prepare once and reuse.

Runtime MemorySink defaults to 1,000,000 records with 10% oldest-batch eviction in VecDeque.
MemorySinkConfig::new(capacity,fraction) validates positive capacity and finite (0,1] fraction.
MemoryStats exposes retained/evicted/compactions; lifetime loss counters survive drain. Snapshots
never prove retention completeness themselves. Capture freezes stats after context cleanup and
fails closed on escaped owners; FORMAT v1 is unchanged.

## Snapshot, query, decode

`ObservationDataset::try_from_memory(&sink)` or `try_from_records(Vec<Arc<ObservationRecord>>)` validates external FORMAT v1 records before taking fixed shared-record membership. Both return Result<Self,IngestionError>; errors retain the source index. `records()` borrows the shared slice; clones share membership and later append/drain cannot change it. JSONL loading must deserialize every physical line fallibly and pass the collected Arc records to try_from_records. Unmatched span halves remain valid retained evidence.

`query(ObservationsQuery::new())` is unrestricted. Independent dimensions compose with AND: `namespace(...)` or `namespace_subtree(...)`, `scope(...)` or `without_scope()`, `level(...)`, `points()/span_starts()/span_ends()`, `span(...)/without_span()`, `process(...)`. Later namespace/lifecycle intent replaces incompatible earlier intent. RecordView::iter yields (original dataset index, &Arc<ObservationRecord>).

`dataset.to_typed::<T>()` or `selected.to_typed::<T>()` requires Observation+DeserializeOwned.
Matching namespace points and span starts decode strictly once; lifecycle ends have
null payload and are excluded. DecodeError retains target type, message and SourceContext
(dataset_index/namespace/proc/scope/span/phase/ts/proc_ms). Do not treat a malformed
matching payload as an absent event.

## Typed values, provenance and generic sources

TypedView owns decoded values and shared original records; it outlives sink drains or
the dataset. `values()` and `sources()` expose aligned slices. TypedView::iter gives
TypedEntry { value:&T, dataset_index, source:&Arc<ObservationRecord> } without allocation
or further deserialization.

`typed.source("semantic name")` borrows those decoded values and implements the generic
PreparedSource path with SourceProvenance. Generic SourceEntry uses a **typed-source-local**
index; original dataset coordinates are `typed.sources()[index]`. Its optional provenance
formatter supplies proc/scope/namespace/span/time in bounded pair diagnostics. No payload
copies or record clones are needed to create another source handle or fact.

If row identities repeat across runs, first query proc/scope or include execution identity
in typed domain keys. Never join cross-owner events by insertion order.

```rust
use sun_observations::certifications::try_fact;
use serde::{Deserialize, Serialize};
use sun_observations::{observe, Observation};
use sun_observations::evidence::{capture_observations, CaptureConfig, ObservationsQuery};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::boundary")]
struct Boundary { row:u64 }
let run=capture_observations(CaptureConfig::new().activate_namespace("module").unwrap(), || observe!(Boundary { row: 1 }));
let snapshot=run.dataset();
let selected=snapshot.query(ObservationsQuery::new().namespace_subtree("module"));
let typed=selected.to_typed::<Boundary>()?;
run.certification("production boundary")
    .check("production call", try_fact("emission", run.result().as_ref().map(|_| true)).expect(true))
    .check("count",typed.source("boundary").count().expect(1usize))
    .assert_certify();
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Generic analysis ownership

Count/integrity, uniqueness/contiguity, joins/comparison, numeric/robust summaries,
relations/differences/residuals, grouping, outliers/jumps, lag, expectations and proof
aggregation all live in **sun_observations::certifications**. Import those types/functions there.
TypedView owns only materialization and provenance. Its `.source(name)` bridge provides the generic PreparedSource operations, including cached numeric projections, grouping, paired comparison and lag. SourceEntry uses source-local indices; retain TypedView to resolve original provenance.

Use a PairedComparison for one typed composite-key comparison reused across fields,
and a cached numeric field pair for several metrics. Direct generic analysis remains
available without asserting; scenario acceptance belongs in a CheckResult/Certification.

See the [generic API reference](CERTIFICATIONS_API.md),
[current capture/cache measurements](CAPTURE_BENCHMARKS.md), and executable
[module transfer](../crates/sun-observations/examples/module_relation.rs) /
[mixed evidence workflow](../crates/sun-observations/examples/skill_workflows.rs).
The self-contained [evidence skill](../crates/sun-observations/agent-skills/sun-observations/SKILL.md) documents
the full usable runtime/adapter API; the
[certification skill](../crates/sun-observations/agent-skills/certifications/SKILL.md) documents the complete generic API.
