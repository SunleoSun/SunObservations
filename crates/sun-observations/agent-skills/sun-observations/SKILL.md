---
name: sun-observations
description: Instrument and capture typed SunObservations evidence. Use when working on Observation emission, runtime selection/scopes/policies, context/routing/sinks/spans, capture/query/decode, or evidence provenance.
---

# SunObservations evidence API

Use this skill as domain guidance within the user's requested scope; explicit task instructions take precedence if they conflict with workflow guidance here. Do not use it for generic certification math or acceptance when no runtime/capture API is involved; use the `certifications` skill. When a task spans production evidence and semantic proof, use both.

## Boundary and evidence design

Produce typed production facts for structured logging/diagnostics/profiling, never scenario acceptance thresholds. Prefer canonical returned results/persisted artifacts over duplicate instrumentation. One dependency, sun-observations, provides all capabilities without features. Generic analysis/acceptance belongs to sun_observations::certifications (independent of runtime concepts/state), capture/query/decode/provenance to sun_observations::evidence, emission/runtime to the root runtime modules.

Design **Boundary** evidence for semantic transfers (InputPrepared, PredictionProduced, ArtifactPersisted), then **Decision** for selection/rejection/suppression; add **Diagnostic** metrics/calibration/gradient summaries only for a concrete diagnosis/certification. Before adding evidence, identify **which certification/consumer uses it**, its semantic contract, and why a canonical return/artifact cannot prove it more authoritatively. Do not instrument arbitrary locals or add test-only production getters/counters/callbacks/alternate cfg(test) behavior to observe internal facts.

`Observation` payloads require `Serialize + 'static`; decoding additionally requires `Deserialize`. The derive macro is re-exported by `sun_observations`.

## Declaration, emission and selection

```rust
use serde::{Deserialize, Serialize};
use sun_observations::*;
use sun_observations::evidence::*;
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::prediction", level = INFO, tags("evidence"))]
struct Prediction { row:u64, signal:f64 }
fn execute() -> Result<(), ObservationError> {
    observe!(Prediction { row:1, signal:2. })?;
    Ok(())
}
fn capture() -> Result<(), Box<dyn std::error::Error>> {
    let run = capture_observations(CaptureConfig::new().activate_tags(["evidence"]).unwrap(), execute);
    run.result().as_ref().map_err(|e| e.to_string())?;
    let typed = run.to_typed::<Prediction>()?;
    let _source = typed.source("predictions");
    let _complete = run.status().is_complete();
    Ok(())
}
```

Production functions use the lazy process-global engine via observe!/observe_span!, not injected engines; configure global delivery at application startup. Independent ObservationEngine instances are for runtime/routing/cache/generation isolation proofs, not ordinary helpers.

| API | Contract |
| --- | --- |
| `Observation` derive | Required namespace; level/tags optional. Namespace is semantic schema identity, independent of Rust type name. Segments [a-z][a-z0-9_]* separated by ::; tags single segments. Compile-time metadata validation. |
| Manual `impl Observation` | Same metadata contract: `const NAMESPACE:&'static str`; `LEVEL:ObservationLevel` default INFO; `TAGS:&'static [&'static str]` cross-cutting selection labels, default empty. ObservationLevel::{INFO,WARN,ERROR}. |
| `message() -> Option<&str>`; `render_text() -> Option<String>` | Default None. Semantic message persists beside typed data, never payload schema; render_text is transient human output, never persisted/analyzed. Serde owns payload encoding; serde_json maps NaN/infinity to null—use explicit typed states if the distinction matters. |
| `global() -> &'static ObservationEngine`; `ObservationEngine::isolated()` | Global owns process configuration; isolated creates an explicit independent identity/configuration with disabled Selection, empty routing/sinks and sink_error_count=0. |
| `observe!(expression)` / `observe_if_active(|| value)` | Global lazy point; engine.observe_if_active has same closure syntax. Result<bool,ObservationError>; construct expensive values inside. |
| `Ok(false)` / `Ok(true)` | Rejected: no construction/serialization/message/rendering. Admitted/dispatched, possibly sinkless: constructs/serializes/calls message, **not durable delivery**. |
| `Selection::new()/default()` | Disabled: all=false, empty sets. `activate_all()` is infallible. `activate_namespace(namespace:impl Into<String>)/exclude_namespace(...)` return Result<Self,InvalidNamespace>; `activate_tags(tags:impl IntoIterator<Item=impl Into<String>>)/exclude_tags(...)` return Result<Self,InvalidTag>. Invalid dynamic metadata returns errors without panic. |
| `engine.set_selection(selection:Selection)` | Replace selection/invalidate plans. Global + current-scope positives union; either exclusion vetoes. WARN/ERROR never bypass selection. Namespace roots include descendants, not similarly spelled siblings. |
| `selection.activates(namespace,tags)/excludes(...)/admits(...) -> bool` | Selection alone; tags:&[&str]. Scope policy additionally controls admission. |
| `namespace::{is_normalized_segment(value:&str),matches_subtree(root:&str,namespace:&str)} -> bool` | Metadata helpers; `validate_namespace(value:&str)->Result<(),InvalidNamespace>`; error wraps offending String. |

Manual Observation may override message/render_text using its own fields, e.g. `fn message(&self)->Option<&str>{Some(&self.reason)}`; required Serialize and metadata remain identical to derive.

## Scope, policy and propagation

Hold `let _guard = start_scope(config: ScopeConfig)` via `start_scope(ScopeConfig::new("execution-id").activate_namespace("pipeline").unwrap().override_policy::<Prediction>(Policy::first_n(10).unwrap()))`. `ScopeConfig::new(name:impl Into<String>)` starts with disabled Selection and no policy overrides; builders are `selection(selection:Selection)`, `activate_all()`, `activate_namespace(namespace)`, `exclude_namespace(namespace)`, `activate_tags(tags)`, `exclude_tags(tags)`, and `override_policy::<T:Observation>(policy:Policy)`. A nested scope is a fresh lifecycle/budget, not inherited counters; without scope the engine has an unscoped lifecycle.

| API | Contract |
| --- | --- |
| `Policy::{Always,Never,FirstN(NonZeroU64),EveryN(NonZeroU64)}` | Always default. `first_n(n:u64)/every_n(n:u64) -> Option<Policy>` validate a positive budget/interval. Scope/type counter is shared by propagated workers. EveryN admits n,2n,…; FirstN bounds admitted attempts, not persistence successes. |
| `Context::default()`; `Context::capture()`; `context.enter()->ContextGuard`; `context.run(operation:impl FnOnce()->R)->R`; `scope_name()->Option<&str>` | Default has no scope. capture snapshots installed context; guard Drop restores predecessor on normal return, Result/? or unwind. Thread-local lifecycle, not automatic inheritance by unrelated workers/tasks. |
| `context::thread::spawn(operation:impl FnOnce()->R+Send+'static)->JoinHandle<R>` | Capture caller context; R:Send+'static. Ordinary threads/async tasks do not propagate automatically. Join workers and propagate child failures. |
| `context::rayon::ContextThreadPool::new()` / `with_threads(NonZeroUsize) -> Result<Self,ThreadPoolBuildError>` | Runtime default or explicit count. `install(operation:impl FnOnce()->R+Send)->R`, R:Send, uses construction-bound context including nested work. Drop outside workers joins/releases context. A last owner dropped inside its detached job cannot join itself; capture stays incomplete until escaped work finishes. |

## Routing, sinks and delivery lifecycle

Routing is independent of activation. `engine.add_channel_sink(channel:&str,sink:Arc<dyn Sink>)`; `register_namespace_channel(namespace:&str,channel:&str)->Result<(),namespace::InvalidNamespace>` routes subtree. `clear_channel_sinks(channel:&str)` detaches; active spans keep frozen sinks. Routes deduplicate within a channel; one physical sink on multiple matching channels intentionally receives multiple deliveries. Share one sink per destination.

| Sink/API | Public surface and behavior |
| --- | --- |
| `MemorySink::new()/default()/with_config(config:MemorySinkConfig)` | `new()` and `default()` use `MemorySinkConfig::default()` = 1,000,000/0.10. Synchronous bounded VecDeque in delivery order. `len()/is_empty()/config()/stats()`; `shared_records()/drain_shared()->Vec<Arc<ObservationRecord>>` preserve identity; `records()/drain()->Vec<ObservationRecord>` owned copies. Drain changes membership, not existing snapshots/lifetime statistics. |
| `MemorySinkConfig::new(max_items:usize,compact_fraction:f64)->io::Result<Self>` | Positive capacity, finite fraction (0,1]; `default()` = 1,000,000/0.10; `max_items()/compact_fraction()` accessors. At capacity evict ceil(capacity*fraction) oldest before append; never exceeds capacity after write. |
| `MemoryStats{retained:usize,evicted:u64,compactions:u64}` (Default: all counters 0) | Exact synchronized counters; eviction survives drain. Legal runtime history eviction cannot silently prove capture completeness. |
| `JsonlSink::open_append(path:impl AsRef<Path>)->io::Result<Self>`; `with_config(path,config:JsonlConfig)` | Append file using default/explicit config; parent must exist. Opening is lazy; async opening/writing failures surface on flush. |
| `JsonlConfig::new()/default()` | Defaults: flush_interval_ms=1000, max_file_size_bytes=None, trim_fraction=0.10. `flush_interval_ms(ms:u64)/max_file_size_mb(mib:u64)/max_file_size_bytes(bytes:u64)/trim_fraction(fraction:f64) -> io::Result<Self>` validate immediately. Whole-line retention may remove one span half. |
| `JsonlSink::from_writer(writer:impl Write+Send+'static)`; `writer_with_config(writer,config:JsonlConfig)->io::Result<Self>` | Default/explicit config; writer mode rejects file retention. |
| `JsonlSink::stats()->JsonlStats` | JsonlStats fields records_written, batches, flushes, compactions are u64; Default sets all four counters to 0. Flush before interpreting completed output. |
| `ConsoleSink::{stdout(),stderr(),new(writer:impl Write+Send+'static)}` | Async human output. Render text literal/multiline, missing trailing newline added; otherwise envelope/message/data fallback. |
| Both async sinks | `is_started()->bool`; `last_error()->Option<io::Error>`. Lazy worker, unbounded queue, sticky first error, no retry; last-owner Drop drains/joins. Global statics are not automatically dropped on process exit. |

### Custom Sink and human delivery

Implement `Sink:Send+Sync+'static` with explicit `write(&self,record:&ObservationRecord)->io::Result<()>` and `flush(&self)->io::Result<()>`. Synchronous sinks implement trivial flush; asynchronous sinks must await a real barrier. Optional `write_shared(&self,record:Arc<ObservationRecord>)` delegates to write; `wants_render_text(&self)->bool` defaults false; `write_delivery(&self,delivery:&ObservationDelivery)` delegates to shared-record delivery. Render capability stays fixed while attached.

Delivered `ObservationDelivery` exposes `record()/shared_record()/render_text()/shared_render_text()`. Point render_text runs **once** only when attached sinks need it. Human sinks share transient text; Memory/JSONL ignore it. Human tables are ordinary typed points, not a persistence contract.

`engine.flush_channels()->Result<(),ObservationError>` flushes unique currently attached sinks; detached sinks need `sink.flush()?` through Sink. Flush even after emission failure before returning. Covers accepted queue prefix, **not fsync**, bounded backlog or cross-sink transaction; fan-out continues on failure, no rollback. `ObservationError::{Serialize(serde_json::Error),Sink(io::Error)}` implements Error; `engine.sink_error_count()->u64` counts surfaced sink errors, not exact loss. `process_id()->&str` is execution-envelope identity. Drop cannot report errors: explicitly flush/finish promised effects.

## Spans and FORMAT v1

`engine.span_if_active(|| payload)?`, `span_if_active(|| payload)?`, `observe_span!(payload)?` → SpanGuard. `is_active()->bool`; consuming `finish()->Result<(),ObservationError>` reports end errors, otherwise Drop emits end. Rejection returns an inactive guard without building payload. One admission freezes scope/routing/sinks/policy through end despite reconfiguration. Serialization/start delivery can fail before guard return. Spans never invoke point render_text and are not a human table API.

FORMAT v1: compact UTF-8 JSON object + LF. ObservationRecord exposes these fields (optional as Option); Phase::{Start,End} serializes lowercase.

| Required field | Meaning |
| --- | --- |
| ts:String | UTC wall clock for human/cross-process context, not durations. |
| proc:String; proc_ms:u64 | Engine/process identity and monotonic elapsed milliseconds; never combine proc_ms across processes. |
| namespace:String; level:INFO/WARN/ERROR | Stable schema and severity. |
| data:serde_json::Value | Typed payload; span ends null. |

Optional scope:String/message:String/phase:"start" or "end"/span:String omitted when absent. Points have no phase/span; start carries data/message, end null/no message. Pair by **(proc,scope,namespace,span)**, never adjacency; duration=end.proc_ms−start.proc_ms. Incomplete halves possible; overlapping durations not additive. Tags/channels/render_text are **not persisted**.

## Scoped capture and completeness

| API | Contract |
| --- | --- |
| `CaptureConfig::new()/default()` | Defaults to disabled `Selection::default()` plus `MemorySinkConfig::default()` retention. Builders: `selection(selection:Selection)`, `retention(config:MemorySinkConfig)`, `activate_all()`, `activate_namespace(namespace)`, `activate_namespaces(namespaces)`, `activate_tag(tag)`, `activate_tags(tags)`, `exclude_namespace(namespace)`, `exclude_namespaces(namespaces)`, `exclude_tag(tag)`, `exclude_tags(tags)`. Exclusions veto; namespace segment boundaries apply. |
| `capture_observations(config:CaptureConfig,operation:impl FnOnce()->T)->CapturedRun<T>` | Private synchronous storage on existing context; never mutates global routes/selection. For global engine in this context, capture selection replaces global admission/delivery; scope policy/local exclusions still apply. Independent local engines unchanged. No channel/route/sink/flush required. |
| Capture lifecycle | Parallel captures isolate storage; nested captures **exclusive** (inner absent from outer). Child scopes inherit attachment, Context/thread/Rayon propagation carries it. Fresh capture scope/policy lifecycle; join workers/finish spans inside closure. Guards restore predecessor even on panic; panics propagate. Escaped Contexts/workers/spans make completeness unprovable even before late emission. |
| `CapturedRun::result()->&T`; `dataset()->&ObservationDataset`; `to_typed::<O:Observation+DeserializeOwned>()->Result<TypedView<O>,DecodeError>` | Application Result remains data; finalized fixed snapshot. Decode once and reuse TypedView. |
| `status()->CaptureStatus`; `status().is_complete()` | CaptureStatus{retention:MemoryStats,emission_errors:u64,unfinished_contexts:bool}; complete iff no eviction/emission failure/escaped owners. Ignored serialization failures count against frozen admission context even if serialization changes installed context. |
| `retention(MemorySinkConfig)` | Default finite 1,000,000 records; size for points **and both span halves**. Eviction may retain a subset, never silently complete evidence. |
| `CapturedRun::certification(name:impl Into<String>)` / `run.certification(name)` | Completeness-aware bridge to the generic proof owner; use the certifications skill for proof methodology/API. Integration belongs to the evidence adapter only. |
## Observation dataset/query/decode/provenance adapters

Import capture/query/decode/provenance from sun_observations::evidence; generic operations/types from sun_observations::certifications. No algorithms/acceptance are reimplemented by adapters.

| API | Result/semantics |
| --- | --- |
| `ObservationDataset::try_from_records(Vec<Arc<ObservationRecord>>)`; `try_from_memory(&MemorySink) -> Result<Self,IngestionError>` | Validate FORMAT v1 timestamps, namespaces and per-record lifecycle before accepting external evidence. Unmatched span halves are allowed. Fixed shared-record membership; `records()->&[Arc<ObservationRecord>]`. JSONL: fallible deserialize every physical line, collect Arc records, then try_from_records; never discard malformed lines. |
| `ObservationDataset::query(ObservationsQuery<'a>)->RecordView<'a>` | `ObservationsQuery::new()` / `ObservationsQuery::default()` is unrestricted. Builders `namespace(&str)/namespace_subtree(&str)`, `scope(&str)/without_scope()`, `level(ObservationLevel)`, `points()/span_starts()/span_ends()`, `span(&str)`, `process(&str)` combine independent dimensions with AND. Later namespace/lifecycle intent replaces incompatible earlier intent. |
| `RecordView::iter()` | Cloneable (dataset_index:usize,&Arc<ObservationRecord>) iterator in fixed order. |
| `ObservationDataset::to_typed::<T:Observation+DeserializeOwned>()` / `RecordView::to_typed::<T>() -> Result<TypedView<T>,DecodeError>` | Strict materialization of matching namespace points/span starts, never ends. Malformed matching payload is an error. |
| `TypedView::values()->&[T]`; `sources()->&[(usize,Arc<ObservationRecord>)]` | Equal-length corresponding values/original coordinates/shared records. Owns decoded values, may outlive dataset/sink. |
| `TypedView::iter()` | Allocation-free/no decode; cloneable/double-ended/exact iterator; TypedEntry{value:&T,dataset_index:usize,source:&Arc<ObservationRecord>}. |
| `TypedView::source(name:impl Into<Arc<str>>)->PreparedSource<'_,T>` | Named generic borrowed adapter, no record clone/redecode. SourceEntry index is source-local, not dataset index; resolve via typed.sources()[index]. |
| SourceContext; DecodeError | SourceContext{dataset_index,namespace,proc,scope,span,phase,ts,proc_ms}; DecodeError{source:Box<SourceContext>,target_type:String,message:String} implements Display/Error. |
| `SourceProvenance::describe(index:usize,out:&mut dyn fmt::Write)->fmt::Result` | TypedView implements generic provenance; original dataset/proc/scope/span/time in bounded prepared pair failures. `source.entry(index)` yields value/index/optional provenance. |

Filter proc/scope first when row keys repeat across executions, or include typed execution identity in keys. After `.source(name)`, all analysis belongs to the generic certifications API.
