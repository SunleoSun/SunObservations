---
name: observations-api
description: Use the SunObservations client API for typed production evidence. Use when declaring or emitting Observations, configuring selection/scopes/policies, routing/sinks/spans, capture, query/decode, or provenance.
---

# Observation evidence language

Use sun_observations::* for runtime and sun_observations::evidence::* for capture/query/decode. Read certifications-methodology to decide which facts need observing; certifications-api owns acceptance and proof aggregation. Domain generation belongs to domain-api.

## DSL

Authoring structure, not literal Rust. Selection controls admission; routing controls delivery; capture observes the real operation without replacing its behavior.

~~~text
DECLARATION :=
    #[derive(Serialize, Deserialize, Observation)]
    #[observation(namespace = NAMESPACE, level = LEVEL, tags(TAG, ...))]
    struct PAYLOAD { FIELDS }
EMISSION := observe!(VALUE) | observe_if_active(|| VALUE) | engine.observe_if_active(|| VALUE)
SELECTION := Selection::new()
    [ .activate_all() | .activate_namespace(NS)? | .activate_tags(TAGS)? ]*
    [ .exclude_namespace(NS)? | .exclude_tags(TAGS)? ]*
SCOPE := guard = start_scope(ScopeConfig::new(NAME)
    [ .selection(SELECTION) ] [ .override_policy::<PAYLOAD>(POLICY) ])
    [ Context::capture().run(OPERATION) | context::thread::spawn(OPERATION) ]
ROUTING := engine.add_channel_sink(CHANNEL, SINK)
           engine.register_namespace_channel(NAMESPACE, CHANNEL)?
SINK := Arc::new(MemorySink::new() | MemorySink::with_config(RETENTION)
              | JsonlSink::open_append(PATH)? | JsonlSink::with_config(PATH, CONFIG)?
              | ConsoleSink::stdout() | ConsoleSink::stderr() | CUSTOM_SINK)
SPAN := span = observe_span!(PAYLOAD) | span_if_active(|| PAYLOAD) | engine.span_if_active(|| PAYLOAD)
        OPERATION; span.finish()?
CAPTURE := run = capture_observations(CaptureConfig::new()
    [ .selection(SELECTION) | .activate_tags(TAGS)? | .activate_namespace(NS)? ]
    [ .retention(RETENTION) ], OPERATION)
QUERY := dataset.query(ObservationsQuery::new()
    [ .namespace(NS) | .namespace_subtree(NS) ]
    [ .scope(NAME) | .without_scope() ] [ .level(LEVEL) ]
    [ .points() | .span_starts() | .span_ends() ] [ .span(ID) ] [ .process(ID) ])
TYPED_EVIDENCE := typed = (run | dataset | QUERY).to_typed::<PAYLOAD>()?
                  typed.source(NAME)
~~~

## DECLARATION

Derive Observation on Serialize payloads; add Deserialize for typed decoding. namespace is required stable schema identity; level is INFO (default), WARN, ERROR; tags are optional selection labels. Manual implementations provide NAMESPACE, LEVEL, TAGS and optionally message() (persisted semantic text) and render_text() (transient human output).

FORMAT v1 persists ts, proc, proc_ms, namespace, level, data plus optional scope, message, phase, span. Tags, channels, render_text are not persisted. Point records have no phase/span; span starts carry data/message and ends have null data without message. proc_ms is monotonic only within one process.

~~~rust
use sun_observations::*;
#[derive(serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "orders::accepted", level = INFO, tags("evidence"))]
struct Accepted { order_id: u64, quantity: u32 }
#[test]
fn stable_identity() {
    assert_eq!(Accepted::NAMESPACE, "orders::accepted");
}
~~~

## EMISSION

Emit through the global production engine with observe!(value) or observe_if_active(|| value). Put expensive construction inside the lazy closure. Ok(false) means rejected before construction/serialization; Ok(true) means admitted/dispatched, not durable persistence. ObservationError exposes serialization or sink failures.

Use global() to configure the process engine; process_id(), sink_error_count(), flush_channels() inspect effects. ObservationEngine::isolated() and engine.observe_if_active are for independent runtime/routing proofs, not dependency injection into ordinary production functions.

~~~rust
use sun_observations::*;
#[derive(serde::Serialize, Observation)]
#[observation(namespace = "orders::validated")]
struct Validated { order_id: u64 }
fn validate(order_id: u64) -> Result<bool, ObservationError> {
    if order_id == 0 { return Ok(false); }
    observe!(Validated { order_id })?;
    Ok(true)
}
#[test]
fn production_validation() { assert!(validate(42).unwrap()); }
~~~

## SELECTION

Selection::new()/default() starts disabled. Combine activate_all, activate_namespace(ns), activate_tags(tags); exclude_namespace/exclude_tags veto positives. Namespace matching respects segment boundaries. Dynamic invalid namespaces/tags return errors. Inspect activates(ns,tags), excludes(ns,tags), admits(ns,tags). namespace::{is_normalized_segment, validate_tag, matches_subtree, validate_namespace} support dynamic inputs.

~~~rust
use sun_observations::*;
#[test]
fn select_boundary_not_debug() {
    let selection = Selection::new().activate_namespace("orders").unwrap()
        .exclude_namespace("orders::debug").unwrap();
    assert!(selection.admits("orders::accepted", &[]));
    assert!(!selection.admits("orders::debug::row", &[]));
}
~~~

## SCOPE

Keep the start_scope guard alive through the operation. ScopeConfig::new(name) supports selection(selection), activate_all, activate_namespace/exclude_namespace, activate_tags/exclude_tags, override_policy::<T>(policy). Policy::Always/Never or first_n(n)/every_n(n) use validated positive counts; counters belong to scope/type lifecycle.

Propagate with Context::capture(), then .enter() (guard), .run(operation), or context::thread::spawn(operation). Context::default() is empty; scope_name() inspects identity. For Rayon use context::rayon::ContextThreadPool::new() or with_threads(NonZeroUsize), then install(operation). Ordinary threads/unrelated async tasks do not inherit context. Join workers before interpreting capture completeness.

~~~rust
use sun_observations::*;
#[test]
fn propagate_batch_identity() {
    let _guard = start_scope(ScopeConfig::new("batch-42").activate_all());
    let worker = context::thread::spawn(|| Context::capture().scope_name().map(str::to_owned));
    assert_eq!(worker.join().unwrap().as_deref(), Some("batch-42"));
}
~~~

## ROUTING

Attach Arc<dyn Sink> with add_channel_sink(channel,sink), then register_namespace_channel(namespace,channel). Routing does not activate anything. Matching routes deduplicate a sink within one channel; the same sink in two matching channels receives twice. clear_channel_sinks(channel) detaches; flush_channels() waits for delivery barriers.

~~~rust
use std::sync::Arc;
use sun_observations::*;
#[derive(serde::Serialize, Observation)]
#[observation(namespace = "routing::accepted")]
struct Accepted { id: u64 }
#[test]
fn route_to_audit() {
    let engine = ObservationEngine::isolated();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("audit", memory.clone());
    engine.register_namespace_channel("routing", "audit").unwrap();
    engine.set_selection(Selection::new().activate_all());
    engine.observe_if_active(|| Accepted { id: 42 }).unwrap();
    engine.flush_channels().unwrap();
    assert_eq!(memory.len(), 1);
}
~~~

## SINK

- MemorySink::new/default or with_config(MemorySinkConfig::new(max_items,compact_fraction)?). Default: 1,000,000 records, 0.10 compaction. Inspect len/is_empty/config/stats (MemoryStats: retained, evicted, compactions); shared_records/drain_shared avoid record cloning; records/drain clone. Retention eviction makes capture incomplete.
- JsonlSink::open_append(path) / with_config(path,config), or from_writer(writer) / writer_with_config(writer,config). JsonlConfig::new/default uses 1000ms flush, no size bound, trim fraction 0.10. Builders flush_interval_ms, max_file_size_mb, max_file_size_bytes, trim_fraction validate. Inspect stats/is_started/last_error; flush before reading async output.
- ConsoleSink::stdout/stderr/new(writer) provides human output.
- Custom Sink implements write(record), flush(); optional write_shared, wants_render_text, write_delivery. ObservationDelivery exposes record/shared_record/render_text/shared_render_text. A custom async sink must make flush a real delivery barrier.

~~~rust
use sun_observations::*;
#[test]
fn bounded_audit_retention() {
    let retention = MemorySinkConfig::new(1000, 0.1).unwrap();
    let memory = MemorySink::with_config(retention);
    assert_eq!(memory.config().max_items(), 1000);
    assert_eq!(memory.stats().evicted, 0);
    let _jsonl = JsonlSink::writer_with_config(Vec::<u8>::new(),
        JsonlConfig::new().flush_interval_ms(250).unwrap()).unwrap();
}
~~~

## SPAN

Use observe_span!(payload) or lazy span_if_active (global/engine); inspect is_active(), call finish() when end-delivery failure matters. Drop attempts end delivery. Start admission freezes scope/routing/sinks/policy through end; do not re-check current selection at finish. Match persisted spans by proc, scope, namespace, span, never adjacency.

~~~rust
use sun_observations::*;
#[derive(serde::Serialize, Observation)]
#[observation(namespace = "batch::sort")]
struct Sort { rows: usize }
#[test]
fn measure_sort() {
    let mut rows = [3, 1, 2];
    let span = observe_span!(Sort { rows: rows.len() }).unwrap();
    rows.sort_unstable();
    span.finish().unwrap();
    assert_eq!(rows, [1, 2, 3]);
}
~~~

## CAPTURE

capture_observations(config, operation) wraps real production. CaptureConfig::new/default starts disabled with default memory retention. selection and retention replace defaults; activate_all, activate_namespace(s), activate_tag(s), exclude_namespace(s), exclude_tag(s) select evidence.

Read run.result(), dataset(), status().is_complete(), to_typed::<T>(). CapturedRun completeness requires no eviction, emission error, or escaped context/worker/span. Nested captures are exclusive; parallel captures have separate storage. run.certification(name) starts a proof with completeness already registered. A planned executor can return captured Result; case.observations::<T>() then accesses typed evidence and capture completeness is mandatory.

~~~rust
use sun_observations::*;
use sun_observations::evidence::*;
#[derive(serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "capture::normalized", tags("evidence"))]
struct Normalized { original_bytes: usize, output_bytes: usize }
fn normalize(raw: &str) -> Result<String, ObservationError> {
    let output = raw.trim().to_lowercase();
    observe!(Normalized { original_bytes: raw.len(), output_bytes: output.len() })?;
    Ok(output)
}
#[test]
fn capture_normalization() {
    let run = capture_observations(CaptureConfig::new().activate_tags(["evidence"]).unwrap(),
        || normalize(" ALPHA "));
    assert_eq!(run.result().as_ref().unwrap(), "alpha");
    assert!(run.status().is_complete());
    assert_eq!(run.to_typed::<Normalized>().unwrap().values().len(), 1);
}
~~~

## QUERY

Build fixed evidence with ObservationDataset::try_from_records(Vec<Arc<ObservationRecord>>) or try_from_memory(&sink); records() borrows the snapshot. dataset.query(ObservationsQuery::new()...) filters without decoding. Independent selector dimensions combine with AND; later namespace/subtree, scope/without_scope, or point/span builders replace conflicting intent. Variants: namespace(ns) / namespace_subtree(ns), scope(name) / without_scope(), level(ObservationLevel::INFO), points() / span_starts() / span_ends(), span(id), process(id). Queries are unrestricted by default; iter() inspects selected records.

~~~rust
use sun_observations::evidence::*;
#[test]
fn select_one_execution() {
    let dataset = ObservationDataset::try_from_records(vec![]).unwrap();
    let view = dataset.query(ObservationsQuery::new()
        .namespace_subtree("orders").scope("batch-42").points());
    assert_eq!(view.iter().count(), 0);
}
~~~

## TYPED_EVIDENCE

Use run/dataset/query.to_typed::<Payload>() for strict decode. Read typed.values(), sources(), iter() (or iter().nth(index)); TypedEntry exposes value, dataset_index, source record. typed.source(name) prepares generic certification evidence without re-decoding and preserves SourceProvenance. DecodeError carries SourceContext (dataset_index, namespace, proc, scope, span, phase, ts, proc_ms). Source-local and dataset indexes differ.

Filter by process/scope first when domain keys repeat across executions, or use typed execution identity in the comparison key. Acceptance and correspondence rules belong to certifications-api.

~~~rust
use sun_observations::*;
use sun_observations::evidence::*;
#[derive(serde::Serialize, serde::Deserialize, Observation)]
#[observation(namespace = "evidence::accepted")]
struct Accepted { id: u64 }
#[test]
fn retain_provenance() {
    let run = capture_observations(CaptureConfig::new().activate_all(),
        || observe!(Accepted { id: 42 }));
    let typed = run.to_typed::<Accepted>().unwrap();
    let prepared = typed.source("accepted orders");
    assert_eq!(prepared.values()[0].id, 42);
    assert_eq!(typed.iter().next().unwrap().dataset_index, 0);
}
~~~
