# sun-observations-data-source

Observation-specific evidence access for the logging + diagnostics + profiling +
semantic certification workspace. This adapter depends on sun-observations and
sun-certifications. The generic framework never depends on the runtime or adapter.
Persisted FORMAT v1 is unchanged; human rendering is never analysis data.

## Scoped real-production capture

Use capture_observations(CaptureConfig::new().activate_tags(["evidence"]), || production_call()).
Production emits typed observe!/observe_span! through the process-global lazy engine, with no
injected engine. Config wraps canonical Selection (activate_all, singular/plural namespace/tag
activation and exclusions), plus retention(MemorySinkConfig). No channel/sink/route is required.
The private attachment replaces global selection/delivery in the existing context; it does not
mutate shared configuration. Parallel captures isolate records, nested captures are exclusive,
child scopes and explicit Context/thread/Rayon propagation retain attachment. Guards restore
prior context on return/Result/?/unwind. Join workers and finish spans inside the closure.

CapturedRun<T>::result() -> &T preserves application Result as ordinary data; dataset() returns
a fixed ObservationDataset; typed::<O>() strictly decodes a TypedView; status() returns
CaptureStatus{retention,emission_errors,unfinished_contexts}. run.certify(name) constructs a
generic Certification and automatically records EvaluationError if completeness is unprovable
(eviction, ignored serialization errors, escaped contexts/spans/workers). Healthy capture is
silent. Always start Observation-backed proof from run.certify, not a standalone root over
possibly incomplete retained records. TypedView decoding is explicit: prepare once and reuse.

Runtime MemorySink defaults to 1,000,000 records with 10% oldest-batch eviction in VecDeque.
MemorySinkConfig::new(capacity,fraction) validates positive capacity and finite (0,1] fraction.
MemoryStats exposes retained/evicted/compactions; lifetime loss counters survive drain. Snapshots
never prove retention completeness themselves. Capture freezes stats after context cleanup and
fails closed on escaped owners; FORMAT v1 is unchanged.

## Snapshot, query, decode

`ObservationDataset::from_memory(&sink)` takes fixed membership/order over shared
canonical records, not cloned JSON payloads. `from_shared(Vec<Arc<ObservationRecord>>)`
accepts already loaded records. `records()` borrows the canonical shared slice;
dataset clones share that array. Append/drain cannot mutate an existing snapshot.
Load persisted JSONL fallibly using BufRead lines plus serde_json::from_str::<ObservationRecord>,
wrap each in Arc and pass to from_shared; do not silently ignore malformed lines.

`query(EnvelopeQuery { ..Default::default() })` gives a lazy RecordView over a fixed
snapshot, with AND selectors namespace, namespace_subtree, proc, level, scope, phase,
span. Optional scope/phase/span selectors distinguish unrestricted None, absent
Some(None), and present Some(Some(value)); namespace subtrees respect segment boundaries.
RecordView::iter yields (original dataset index, &Arc<ObservationRecord>).

`dataset.typed::<T>()` or `selected.typed::<T>()` requires Observation+DeserializeOwned.
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
copies or record clones are needed to create another source handle or measurement.

If row identities repeat across runs, first query proc/scope or include execution identity
in typed domain keys. Never join cross-owner events by insertion order.

```rust
use serde::{Deserialize, Serialize};
use sun_observations::{observe, Observation};
use sun_observations_data_source::{capture_observations, CaptureConfig, EnvelopeQuery};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::boundary")]
struct Boundary { row:u64 }
let run=capture_observations(CaptureConfig::new().activate_namespace("module"), || observe!(Boundary { row: 1 }));
let snapshot=run.dataset();
let selected=snapshot.query(EnvelopeQuery { namespace_subtree:Some("module"), ..Default::default() });
let typed=selected.typed::<Boundary>()?;
run.certify("production boundary")
    .check("count",typed.source("boundary").count().expect(1usize))
    .assert_certify();
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Generic analysis ownership

Count/integrity, uniqueness/contiguity, joins/alignment, numeric/robust summaries,
relations/differences/residuals, grouping, outliers/jumps, lag, expectations and proof
aggregation all live in **sun-certifications**. Import those types/functions there.
Adapter convenience methods numeric/group_by/join_by/join_sorted_by/join_u64_by/
lag_relation_by delegate to that owner; they are not parallel algorithms. Lazy numeric
conveniences can replay selectors; prefer source().numeric() for cached expensive projection.
Group iteration now yields generic SourceEntry rather than TypedEntry; preserve the typed
view when resolving original provenance.

Use a PreparedComparison for one typed composite-key alignment reused across fields,
and a cached numeric field pair for several metrics. Direct generic analysis remains
available without asserting; scenario acceptance belongs in a CompareResult/Certification.

See [generic workflows](CERTIFICATIONS_API.md), [certification matrix](CERTIFICATION.md),
[current performance](CERTIFICATION_BENCHMARKS.md), and executable
[module transfer](../crates/sun-observations-data-source/examples/module_relation.rs) /
[mixed evidence workflow](../crates/sun-observations-data-source/examples/skill_workflows.rs).
The self-contained [evidence skill](../.agents/skills/sun-observations/SKILL.md) documents
the full usable runtime/adapter API; the
[certification skill](../.agents/skills/certifications/SKILL.md) documents the complete generic API.
