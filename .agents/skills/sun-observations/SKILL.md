---
name: sun-observations
description: Produce typed production evidence with SunObservations structured logging, diagnostics, profiling, lazy emission, selection, policies, propagation, routing, sinks and spans; query and decode Observation evidence into generic certification sources.
---

# SunObservations evidence API

## Boundary and evidence design

SunObservations produces typed production semantic evidence for structured logging, runtime diagnostics and profiling. Runtime observations describe facts, never scenario acceptance thresholds. Generic statistics and acceptance belong to `sun-certifications`. Prefer canonical returned values or persisted artifacts when they already expose the fact; do not emit a duplicate Observation just for a test.

Start with **Boundary** evidence (semantic transfer: InputPrepared, PredictionProduced, ArtifactPersisted). Add **Decision** evidence for meaningful selection/rejection/suppression. Add **Diagnostic** evidence (metrics, calibration, gradient summaries) only for a concrete diagnosis/certification. Do not instrument arbitrary locals. Before adding evidence, identify which certification/consumer uses it, which semantic contract it proves, and why a canonical return/artifact cannot provide the same fact more authoritatively; without those answers the Observation is probably unnecessary. When semantic internal evidence disappears on the production path, prefer a production Observation over test-only getters/counters/callbacks or alternate `#[cfg(test)]` branches.

## Dependencies

Rust edition 2021. Use the dependency source already configured by the consuming workspace (workspace, registry, Git revision, or local path); keep sun-observations, sun-observations-data-source, and sun-certifications on the same compatible release/revision. Do not invent a local checkout path when the consumer only has crate dependencies.

Runtime emission needs sun-observations plus Serde derive support. Observation-backed certification additionally needs sun-observations-data-source and sun-certifications. The Observation derive macro is re-exported by the runtime. Payloads require Serialize + 'static; add Deserialize only when decoding captured evidence. The generic certification framework has no runtime dependency.

## Canonical production capture and typed evidence

```rust
use serde::{Deserialize, Serialize};
use sun_observations::*;
use sun_observations_data_source::*;

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::prediction", level = INFO, tags("evidence"))]
struct Prediction { row: u64, signal: f64 }
fn execute(count: u64) -> Result<Vec<u64>, ObservationError> {
    let mut output = Vec::new();
    for row in 0..count {
        observe!(Prediction { row, signal: row as f64 })?;
        output.push(row);
    }
    Ok(output)
}
fn capture() -> Result<(), Box<dyn std::error::Error>> {
    let run = capture_observations(
        CaptureConfig::new().activate_tags(["evidence"])
            .exclude_namespace("pipeline::verbose"),
        || execute(8),
    );
    let output = run.result().as_ref().unwrap();
    let typed = run.typed::<Prediction>()?;
    let _output_rows = output.len();
    let _evidence = typed.source("encoder predictions");
    let _status = run.status();
    let _complete = run.status().is_complete();
    Ok(())
}
```

Production functions use the process-global lazy `global()` engine through typed `observe!` / `observe_span!`, not injected engine parameters. Configure global delivery at application startup. Explicit `ObservationEngine::new()` is appropriate for independent runtime/routing/generation/cache isolation certifications, not ordinary production helpers.

## Scoped capture and completeness

- `CaptureConfig::new()/default()` starts disabled. `selection(Selection)` supplies canonical runtime semantics; `activate_all()`, `activate_namespace(x)/activate_namespaces(iter)`, `activate_tag(x)/activate_tags(iter)`, `exclude_namespace(x)/exclude_namespaces(iter)` and `exclude_tag(x)/exclude_tags(iter)` build the same Selection. Exclusions veto activation; namespace roots include descendants, not spelling prefixes.
- `capture_observations(config, FnOnce() -> T) -> CapturedRun<T>` attaches private synchronous storage to the existing context. It never changes global routes/selection. Capture selection replaces global admission/delivery for the global engine in this context; scope policy and local exclusions still apply. Independent local engines retain their ordinary runtime behavior.
- Parallel captures never share storage. Nested captures are **exclusive**: inner evidence is absent from the outer run. Child scopes inherit attachment; Context/thread/Rayon propagation carries it. Ordinary threads and async tasks do not inherit automatically. Each capture is a fresh scope/policy lifecycle. Join workers and finish spans inside the closure.
- Context guards restore the predecessor on normal return, Result/?, or panic unwind, independently of certification/assertion. Panics propagate. Escaped Contexts, workers or spans make completeness unprovable even if no late emission has occurred.
- `CapturedRun::result() -> &T` preserves any application Result as data; `dataset() -> &ObservationDataset` exposes the finalized fixed snapshot; `typed::<O>() -> Result<TypedView<O>,DecodeError>` decodes matching evidence. Keep the TypedView alive and reuse its prepared source; calling typed again decodes again.
- `status() -> CaptureStatus` exposes `retention:MemoryStats`, `emission_errors:u64` and `unfinished_contexts:bool`; `status().is_complete()` requires no eviction, emission failures or escaped context owners. Ignored serialization failures are counted against the frozen admission context, even if serialization changes the installed context.
- `CapturedRun::certify` (`run.certify(name)`) is the data-source completeness-aware bridge to the generic proof owner. See the certifications skill for its proof workflow; capture integration itself lives only in the data-source crate.
- `retention(MemorySinkConfig)` configures capture capacity. Default finite history holds up to 1,000,000 records; size for points **and both span halves**. Eviction is legal for runtime history, never silent successful certification. No channel, sink, route or flush is required for ordinary capture.

## Declaration and meaning

Derive accepts `#[observation(namespace = "owner::fact", level = INFO, tags("evidence", "diagnostic"))]`; level/tags optional. Namespace is independent of the Rust name. Namespace segments match `[a-z][a-z0-9_]*` separated by `::`; tags are single segments. Derive validates metadata at compile time. Manual metadata must obey the same contract.

Manual implementation (instead of derive) can customize semantic message and human output:

```rust
use serde::Serialize;
use sun_observations::{Observation, ObservationLevel};
#[derive(Serialize)]
struct Decision { row: u64, reason: String }
impl Observation for Decision {
    const NAMESPACE: &'static str = "pipeline::decision";
    const LEVEL: ObservationLevel = ObservationLevel::INFO;
    const TAGS: &'static [&'static str] = &["evidence"];
    fn message(&self) -> Option<&str> { Some(&self.reason) }
    fn render_text(&self) -> Option<String> { Some(format!("{} | {}", self.row, self.reason)) }
}
```

- `NAMESPACE` identifies the semantic schema, `LEVEL` is INFO/WARN/ERROR (default INFO), `TAGS` are cross-cutting selection labels (default empty).
- `message() -> Option<&str>` defaults None: semantic context persisted beside typed `data`; never use it as the payload schema.
- `render_text() -> Option<String>` defaults None: optional transient human formatting, not persisted or analyzed.
- Serde determines payload data. JSON cannot represent NaN/infinity; serde_json serializes floating nonfinites as null. Use explicit typed state when that distinction matters.

## Emission, selection and admission

| API | Parameters and result/semantics |
| --- | --- |
| `global() -> &'static ObservationEngine` | Global configuration owner; local `ObservationEngine::new()/default()` isolates configuration/process identity. |
| `observe!(expression)`; `observe_if_active(|| value)` | Global lazy point emission; returns `Result<bool, ObservationError>`. Engine method has identical closure syntax. Construct expensive values inside. |
| `Ok(false)` | Inactive/excluded/policy-rejected: no payload construction, serialization, message or rendering. |
| `Ok(true)` | Admitted/dispatched, possibly sinkless; not durable delivery. Even sinkless admission constructs/serializes and calls message. |
| `Selection::new()/default()` | Initially disabled. Builders: `activate_all()`, `activate_namespace(Into<String>)`, `exclude_namespace(Into<String>)`, `activate_tags(iterable of Into<String>)`, `exclude_tags(iterable)`. |
| `engine.set_selection(Selection)` | Replaces engine selection and invalidates cached plans. Global/current-scope positives union; either exclusion vetoes. **WARN/ERROR do not bypass selection**. Namespace activation includes descendants, not similarly spelled siblings. |
| `selection.activates(namespace, tags)/excludes(...)/admits(...) -> bool` | Inspect a selection alone; tags argument `&[&str]`. Admission additionally applies scope policy. |
| `namespace::{is_normalized_segment(&str), matches_subtree(root, namespace)} -> bool` | Metadata helpers. `validate_namespace(&str) -> Result<(), InvalidNamespace>`; error wraps offending String. |

## Scope and propagation

Hold `let _guard = start_scope(ScopeConfig::new("execution-id").activate_namespace("pipeline").override_policy::<Prediction>(Policy::FirstN(10)));`. ScopeConfig also supports `selection(Selection)` and all the Selection activation/exclusion builders. Current scope supplies local selection/policy; a nested scope is a new lifecycle, not an inherited budget. Without an explicit scope the engine has an unscoped lifecycle.

| API | Contract |
| --- | --- |
| `Policy::{Always,Never,FirstN(u64),EveryN(u64),MaxPerScope(u64)}` | Per-concrete-type/current-scope admission counter shared by propagated workers. EveryN admits n,2n,…; zero rejects. Always is default. FirstN/MaxPerScope bound admitted attempts, not successful persistence. |
| `Context::capture()` | Captures current scope/context. `context.run(|| work()) -> R` or hold `context.enter() -> ContextGuard`; guard restores prior context on Drop. `scope_name() -> Option<&str>`. |
| `context::thread::spawn(move || work()) -> JoinHandle<R>` | Captures caller context; closure/result Send+'static. Ordinary threads do not propagate. Join and handle child failures. No automatic async-task propagation. |
| `context::rayon::ContextThreadPool::new(threads:usize) -> Result<Self,ThreadPoolBuildError>` | Enable runtime `rayon` feature. Construct in desired scope; 0 uses Rayon default. `install(FnOnce + Send) -> R`, R:Send, uses construction-bound context including nested work. Dropping the pool outside its workers terminates/joins workers and releases context before returning; a last owner dropped inside its own detached job cannot join itself, so capture remains incomplete until that escaped work finishes. |

Guards are execution/thread-local lifecycle objects; do not drop a parent while assuming ordinary unrelated worker threads inherit it. Keep/join propagated work intentionally.

## Routing and sinks

**Namespace → Channel → Sink** is independent of activation. `engine.add_channel_sink(channel:&str, Arc<dyn Sink>)` attaches; `register_namespace_channel(namespace:&str, channel:&str) -> Result<(), namespace::InvalidNamespace>` routes a subtree. `clear_channel_sinks(channel)` detaches; active spans keep their frozen sinks. Route matches within one channel deduplicate. One physical sink on multiple matching channels intentionally gets multiple deliveries. Share one sink per physical destination. `Channel::name() -> &str` exposes channel identity; use engine APIs for configuration.

| Sink/API | Complete construction/access surface and behavior |
| --- | --- |
| `MemorySink::new()/default()/with_config(MemorySinkConfig)` | Synchronous bounded history in delivery order using VecDeque; default max_items=1,000,000 and compact_fraction=0.10. `len()/is_empty()`; `shared_records()/drain_shared() -> Vec<Arc<ObservationRecord>>` preserve identity; `records()/drain() -> Vec<ObservationRecord>` provide owned records. Drain removes membership, not snapshots or lifetime statistics. |
| `MemorySinkConfig::new(max_items:usize,compact_fraction:f64) -> io::Result<Self>` | Positive capacity; finite fraction in (0,1]. `default()`, `max_items()`, `compact_fraction()`; `sink.config()` returns the validated config. At capacity, evict ceil(capacity*fraction) oldest records before appending. Never exceeds capacity after a write. |
| `MemorySink::stats() -> MemoryStats` | Exact retained:usize, evicted:u64 and compactions:u64; all writes/compactions synchronized. Eviction counts survive drain. Runtime history may evict; CapturedRun proof automatically fails if capture evicts. |
| `JsonlSink::append(path:AsRef<Path>) -> io::Result<Self>` | Append file; parent must exist. `with_config(path, JsonlConfig)` validates configuration. File opening is lazy at writer start; flush exposes asynchronous opening/writing failures. |
| `JsonlConfig::new()/default()` | `flush_ms(u64)` default 1000; `file_size_mb(u64)` uses MiB, or `max_file_size_bytes(u64)` (default unlimited); `trim_fraction(f64)` default 0.10. Intervals/limits positive; trim finite strictly (0,1). Invalid construction returns InvalidInput. Whole-line retention can remove one span half. |
| `JsonlSink::new(writer:Write+Send+'static)` | Owned writer; no file retention. `writer_with_config(writer,config) -> io::Result<Self>` validates settings; any configured file-retention limit is rejected with InvalidInput for an arbitrary writer. |
| `JsonlSink::stats() -> JsonlStats` | Snapshot fields `records_written, batches, flushes, compactions: u64`; flush before interpreting completed output. |
| `ConsoleSink::{stdout(),stderr(),new(writer:Write+Send+'static)}` | Asynchronous human sink. Custom render text is literal/multiline; missing trailing newline is added. Otherwise envelope/message/data fallback. |
| Both async sinks | `is_started() -> bool`; `last_error() -> Option<io::Error>`. Lazy worker, unbounded queue, sticky first failure, no retry. Last-owner Drop drains/joins; global statics are not automatically dropped at process exit. |

### Custom Sink and human delivery

Implement `Sink: Send + Sync + 'static`:

- Required `fn write(&self, record:&ObservationRecord) -> io::Result<()>`.
- Optional `write_shared(&self, Arc<ObservationRecord>) -> io::Result<()>` avoids deep cloning; default delegates to write.
- Optional `flush(&self) -> io::Result<()>` (default success); implement a real queue-prefix barrier for asynchronous delivery.
- Optional `wants_render_text(&self) -> bool` (default false), `write_delivery(&self, &ObservationDelivery) -> io::Result<()>` (default shared-record delivery). Capability stays fixed while attached; synchronize concurrent writes.
- `ObservationDelivery::new(Arc<ObservationRecord>, Option<Arc<String>>)`; `record() -> &ObservationRecord`, `shared_record() -> &Arc<ObservationRecord>`, `render_text() -> Option<&str>`, `shared_render_text() -> Option<&Arc<String>>`.

Point emission calls render_text **once**, only if a matched attached sink requests it; all human sinks share that rendering. Structured Memory/JSONL ignore transient rendering. A table can be ordinary typed point header/row/footer observations with human rendering; do not invent a separate table persistence contract.

### Flush/errors/shutdown

`engine.flush_channels() -> Result<(), ObservationError>` flushes each unique currently attached sink; detached sinks require `sink.flush()?` via the Sink trait. Flush even after emission failure before returning. A barrier covers accepted queue prefix, **not fsync**, bounded backlog or transactional cross-sink delivery. Fan-out continues on failure, with no rollback. `ObservationError::{Serialize(serde_json::Error), Sink(io::Error)}` implement Error; `engine.sink_error_count() -> u64` tracks surfaced sink failures, not exact lost-record count. `process_id() -> &str` is execution-envelope identity. Drop cannot return errors; explicitly flush/finish promised effects.

## Spans: lifecycle/profiling evidence

`let span = engine.span_if_active(|| payload)?;`; global equivalents `observe_span!(payload)?` / `span_if_active(|| payload)?` return `SpanGuard`. `is_active() -> bool`; consume `finish() -> Result<(), ObservationError>` to report end errors, otherwise Drop emits the end. Rejection creates inactive guard without building. One admission freezes scope, routing/sinks and policy for the end, even after reconfiguration. Serialization/start delivery can fail before a guard is returned. Spans never invoke point-observation render_text. They are not a human table API.

## Persisted structured contract: FORMAT v1

Compact UTF-8 JSON object + LF per record. Required canonical fields:

| Field | Meaning |
| --- | --- |
| `ts: String` | UTC wall-clock timestamp, human/cross-process context; not duration clock. |
| `proc: String` | Engine/process identity; do not combine proc_ms across processes. |
| `proc_ms: u64` | Monotonic elapsed milliseconds within the engine/process. |
| `namespace: String`, `level: INFO|WARN|ERROR` | Stable schema identity and severity. |
| `data: serde_json::Value` | Canonical typed payload (span ends use null). |

Optional fields `scope: String, message: String, phase: "start"|"end", span: String` are omitted when absent. Points have no phase/span. Start carries typed data/message; end carries null data/no message. Pair by **(proc,scope,namespace,span)**, never adjacency; duration=end.proc_ms−start.proc_ms. Incomplete halves are possible; overlapping durations are not additive. Tags/channels/render_text are **not persisted**. `ObservationRecord` exposes these same fields (optional ones as Option); `Phase::{Start,End}` uses lowercase Serde names.

## Observation data-source API

Import dataset types from `sun_observations_data_source`; generic operations/types from `sun_certifications`. Dataset/query/decode/provenance have one Observation-specific owner; numeric/join/group/lag algorithms have one generic owner.

| API | Parameters, result, failure semantics |
| --- | --- |
| `ObservationDataset::from_memory(&MemorySink)` | Fixed snapshot over canonical shared records; later append/drain cannot change it. |
| `from_shared(Vec<Arc<ObservationRecord>>)`; `records() -> &[Arc<ObservationRecord>]` | Use for already-loaded artifacts; Arc sharing avoids payload copies. Clone of dataset shares membership. To load JSONL: BufRead::lines, deserialize each line into ObservationRecord, wrap in Arc, collect fallibly, then from_shared; never ignore malformed lines. |
| `query(EnvelopeQuery<'a>) -> RecordView<'a>` | AND selection; defaults unrestricted. Fields `namespace,namespace_subtree,proc:Option<&str>`; `level:Option<ObservationLevel>`; `scope,span:Option<Option<&str>>`; `phase:Option<Option<Phase>>`. Outer None=unrestricted, Some(None)=absent. Subtree respects segment boundaries. Query borrows dataset/selectors. |
| `RecordView::iter()` | Cloneable iterator of `(dataset_index:usize,&Arc<ObservationRecord>)` in fixed dataset order. |
| `dataset.typed::<T>()` / `record_view.typed::<T>() -> Result<TypedView<T>,DecodeError>` | T:Observation+DeserializeOwned. Strict decode of matching namespace points/span starts; skips only span ends. Fails on malformed matching payload, not silently dropping it. Decodes once. |
| `TypedView::values() -> &[T]`; `sources() -> &[(usize,Arc<ObservationRecord>)]` | Equal-length aligned arrays of decoded values/original dataset coordinates and shared records. TypedView owns decoded values; can outlive snapshot/sink. |
| `TypedView::iter()` | Cloneable, double-ended exact iterator; `TypedEntry { value:&T, dataset_index:usize, source:&Arc<ObservationRecord> }`, allocation-free/no decode. |
| `TypedView::source(name:Into<Arc<str>>) -> PreparedSource<'_,T>` | Canonical adapter to generic framework, borrowing values/provenance without cloning records or decoding again. Same operations as an ordinary source_slice. SourceEntry has source-local index (not dataset index); resolve original coordinates through typed.sources()[index]. |
| `SourceContext` | Decode coordinates: dataset_index, namespace, proc, scope, span, phase, ts, proc_ms. `DecodeError { source:Box<SourceContext>, target_type:String, message:String }` implements Display/Error. |
| `SourceProvenance::describe(index,&mut dyn fmt::Write) -> fmt::Result` | Generic adapter context implemented by TypedView; emits original dataset/proc/scope/span/time. PreparedSource pair failures include bounded coordinates. `source.entry(index)` returns value/index/optional provenance. |

If row keys repeat across executions, first filter **proc/scope**, or include explicit execution identity in typed domain keys. Insertion order is never cross-owner identity.

Existing adapter conveniences delegate to the generic owner: `typed.numeric(project) -> NumericSeries` (lazy projection: prefer source().numeric for reuse), `group_by(key) -> GroupedView`; `join_by(&other,lk,rk)` / `join_u64_by(...) -> Join`, `join_sorted_by(...) -> Result<Join,JoinError>`; joins require Ord+Clone keys and the typed join entry points require Observation types. `lag_relation_by(&other, left:(i64,f64) selector, right selector, LagConfig) -> Result<LagReport,LagError>` attaches namespaces. These do not own algorithms or acceptance; prefer named PreparedSource/PreparedComparison for certifications.
