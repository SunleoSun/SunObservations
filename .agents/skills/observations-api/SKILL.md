---
name: observations-api
description: Use the SunObservations client API for typed production evidence. Use when declaring or emitting Observations, configuring selection/scopes/policies, routing/sinks/spans, capture, query/decode, or provenance.
---

# Observations client API

Use this skill for runtime evidence API. If the task is a semantic proof, also read certifications-methodology and the relevant certifications-api or domain-api skills. Acceptance thresholds belong to certifications, not Observation payload metadata.

## Runtime workflow grammar

~~~text
Observation type -> selection/scope -> emission -> routing/sinks
                                      -> optional scoped capture

capture_observations(config, operation)
  -> CapturedRun
  -> result | status | dataset | to_typed | certification

ObservationDataset
  -> query -> RecordView -> to_typed -> TypedView
  -> to_typed -> TypedView

TypedView
  -> values | sources | iter | entry
  -> source(name) -> PreparedSource
~~~

Selection decides admission; routing decides delivery; capture records scoped evidence from the real operation. They are separate stages.

## Minimal typed evidence example

~~~rust
use serde::{Deserialize, Serialize};
use sun_observations::*;
use sun_observations::evidence::*;

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::prediction", level = INFO, tags("evidence"))]
struct Prediction {
    row: u64,
    signal: f64,
}

fn execute() -> Result<(), ObservationError> {
    observe!(Prediction { row: 1, signal: 2.0 })?;
    Ok(())
}

fn capture() -> Result<(), Box<dyn std::error::Error>> {
    let run = capture_observations(
        CaptureConfig::new().activate_tags(["evidence"])?,
        execute,
    );
    run.result().as_ref().map_err(|e| std::io::Error::other(e.to_string()))?;
    let typed = run.to_typed::<Prediction>()?;
    assert_eq!(typed.values().len(), 1);
    Ok(())
}
~~~

Production helpers normally emit through the process-global engine with observe!, observe_span!, or observe_if_active. ObservationEngine::isolated() is for explicit independent runtime/routing proofs, not dependency injection into ordinary production helpers.

## Observation declaration and emission

Observation derive requires Serialize and a namespace. Decoding also requires Deserialize.

Derive attributes:
- namespace = "segment::segment" — required stable schema identity.
- level = INFO, WARN, or ERROR — optional; INFO default.
- tags("tag", ...) — optional selection labels.

Manual impl Observation exposes:
- NAMESPACE
- LEVEL
- TAGS
- message()
- render_text()

message() is persisted semantic text. render_text() is transient human output and is not persisted or analyzed.

Emission:
- observe!(value)
- observe_if_active(|| value)
- engine.observe_if_active(|| value)

Expensive values should be built inside the lazy closure. Ok(false) means rejected before construction or serialization. Ok(true) means admitted and dispatched, not guaranteed durable persistence.

Engine access:
- global()
- ObservationEngine::isolated()
- process_id()
- sink_error_count()
- set_selection(selection)
- flush_channels()

ObservationError contains serialization or sink failures and implements Error.

## Selection

Selection::new() and default() start disabled.

Builders:
- activate_all()
- activate_namespace(namespace)
- exclude_namespace(namespace)
- activate_tags(tags)
- exclude_tags(tags)

Inspection:
- activates(namespace, tags)
- excludes(namespace, tags)
- admits(namespace, tags)

Namespace activation matches the selected subtree by segment boundary. Exclusions veto positives. Dynamic invalid namespace/tag input is returned as an error.

Namespace helpers:
- namespace::is_normalized_segment(value)
- namespace::matches_subtree(root, namespace)
- namespace::validate_namespace(value)

## Scope and policy

Create a scoped runtime lifecycle with:

~~~rust
use sun_observations::*;

fn scoped() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = start_scope(
        ScopeConfig::new("run-1")
            .activate_namespace("pipeline")?
            .override_policy::<Prediction>(Policy::first_n(10).unwrap())
    );
    Ok(())
}

#[derive(serde::Serialize, Observation)]
#[observation(namespace = "pipeline::prediction")]
struct Prediction { row: u64 }
~~~

ScopeConfig:
- new(name)
- selection(selection)
- activate_all()
- activate_namespace(namespace)
- exclude_namespace(namespace)
- activate_tags(tags)
- exclude_tags(tags)
- override_policy::<T: Observation>(policy)

Policy:
- Always
- Never
- FirstN
- EveryN
- first_n(n)
- every_n(n)

first_n/every_n require positive values. Scope/type counters belong to the propagated scope lifecycle.

## Context propagation

Context:
- default()
- capture()
- enter()
- run(operation)
- scope_name()

Context::capture snapshots the installed context. Guard drop restores the predecessor.

For propagated threads:
- context::thread::spawn(operation)

For Rayon:
- context::rayon::ContextThreadPool::new()
- ContextThreadPool::with_threads(count)
- install(operation)

Ordinary threads or unrelated async tasks do not inherit context automatically. Join propagated workers before considering capture/effects complete.

## Routing and sinks

Engine routing:
- add_channel_sink(channel, sink)
- register_namespace_channel(namespace, channel)
- clear_channel_sinks(channel)
- flush_channels()

Routing is independent from selection. Matching routes deduplicate a sink within one channel; attaching the same physical sink to multiple matching channels intentionally creates multiple deliveries.

### MemorySink

Construction:
- MemorySink::new()
- MemorySink::default()
- MemorySink::with_config(config)

Access:
- len()
- is_empty()
- config()
- stats()
- shared_records()
- drain_shared()
- records()
- drain()

MemorySinkConfig:
- new(max_items, compact_fraction)
- default()
- max_items()
- compact_fraction()

Default retention is 1,000,000 records with compact_fraction 0.10. Eviction is observable in MemoryStats and makes capture completeness fail.

MemoryStats contains retained, evicted, compactions.

### JsonlSink

Construction:
- JsonlSink::open_append(path)
- JsonlSink::with_config(path, config)
- JsonlSink::from_writer(writer)
- JsonlSink::writer_with_config(writer, config)

JsonlConfig:
- new()
- default()
- flush_interval_ms(ms)
- max_file_size_mb(mib)
- max_file_size_bytes(bytes)
- trim_fraction(fraction)

Default config: flush_interval_ms=1000, no file-size limit, trim_fraction=0.10.

Runtime inspection:
- stats()
- is_started()
- last_error()

Flush before interpreting completed async output.

### ConsoleSink and custom Sink

ConsoleSink:
- stdout()
- stderr()
- new(writer)

Custom sinks implement Sink with write(record) and flush(). Optional client hooks:
- write_shared(record)
- wants_render_text()
- write_delivery(delivery)

ObservationDelivery exposes:
- record()
- shared_record()
- render_text()
- shared_render_text()

A custom async sink must make flush() a real delivery barrier.

## Spans

Create spans with:
- observe_span!(payload)
- span_if_active(|| payload)
- engine.span_if_active(|| payload)

SpanGuard:
- is_active()
- finish()

One successful span admission freezes its scope/routing/sinks/policy through the matching end. Drop attempts end emission; call finish() when end-delivery failure must be observable.

Persisted span identity is matched by proc, scope, namespace, and span, never by adjacency.

## Scoped capture

CaptureConfig::new() and default() use disabled selection plus default memory retention.

Builders:
- selection(selection)
- retention(config)
- activate_all()
- activate_namespace(namespace)
- activate_namespaces(namespaces)
- activate_tag(tag)
- activate_tags(tags)
- exclude_namespace(namespace)
- exclude_namespaces(namespaces)
- exclude_tag(tag)
- exclude_tags(tags)

Run capture_observations(config, operation) around the real production operation.

CapturedRun client access:
- result()
- dataset()
- status()
- to_typed::<T>()
- certification(name)

status().is_complete() is true only when capture has no retention eviction, emission error, or escaped context/worker/span ownership.

run.certification(name) starts an eager certification with capture completeness already registered. When a planned executor returns CapturedRun<Result<Output, Error>>, certifications-api allows case.observations::<T>() and enforces completeness as an execution prerequisite.

Nested captures are exclusive: inner capture records do not enter the outer capture. Parallel captures keep separate storage.

## Dataset, query, decode, provenance

ObservationDataset:
- try_from_records(records)
- try_from_memory(&MemorySink)
- records()
- query(query)
- to_typed::<T>()

ObservationsQuery::new() and default() are unrestricted. Builders combine independent dimensions with AND:
- namespace(value)
- namespace_subtree(value)
- scope(value)
- without_scope()
- level(level)
- points()
- span_starts()
- span_ends()
- span(value)
- process(value)

RecordView:
- iter()
- to_typed::<T>()

TypedView:
- values()
- sources()
- iter()
- entry(index)
- source(name)

TypedEntry exposes the decoded value, dataset index, and source record.

TypedView::source(name) creates a generic PreparedSource for certifications without re-decoding. Source-local index and dataset index are different; resolve original evidence through sources() or TypedEntry.

DecodeError includes SourceContext with dataset_index, namespace, proc, scope, span, phase, ts, and proc_ms.

TypedView implements SourceProvenance, so prepared comparison failures can retain bounded original Observation coordinates.

Filter by process/scope first when domain keys repeat across executions, or include typed execution identity in the comparison key.

## Persisted FORMAT v1

Each record is one compact UTF-8 JSON object plus LF.

Required persisted fields:
- ts
- proc
- proc_ms
- namespace
- level
- data

Optional:
- scope
- message
- phase
- span

Tags, channels, and render_text are not persisted.

Point records have no phase/span. Span start contains data/message; span end has null data and no message. proc_ms is process-local monotonic elapsed time; do not compare it across process IDs.