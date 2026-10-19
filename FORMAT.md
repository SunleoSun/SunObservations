# SunObservations persisted format — v1

This document specifies the JSONL envelope produced by the runtime, independently of
Rust struct names and API module layout. It is the canonical input contract for future
parsers. No domain-specific fields or parser/query/stats implementation are defined here.

## Framing and compatibility

- UTF-8 JSON Lines, without a BOM. Each record is one complete JSON object followed by
  a single LF (\n). Strings, including messages, escape embedded newlines normally.
- No pretty printing, headers, array wrapper or blank lines. Object member order is not
  semantic; readers must address members by name, not position.
- v1 is this documented schema; there is no implicit version, sequence, component, type
  or channel column. Required fields below are always present in engine-generated output.
- Optional fields are **omitted**, not serialized as null. The payload itself may be null.
- Readers should tolerate unknown additional envelope members for additive extensions.
  Changes to required meanings/types need a new documented schema version.
- The sink serializes supplied ObservationRecord values as-is. Applications constructing
  records directly are responsible for satisfying the same envelope/lifecycle invariants.

## Envelope

| Member | JSON type | Meaning |
| --- | --- | --- |
| ts | string | UTC wall-clock RFC 3339 timestamp, exactly millisecond precision with Z suffix, e.g. 2026-10-20T00:00:00.000Z. For human/cross-process correlation, not elapsed timing. |
| proc | string | Opaque process-instance identity, fresh on singleton engine initialization. Consumers must not parse its internal spelling. Isolated engine instances have their own identity. |
| proc_ms | unsigned integer | Monotonic elapsed milliseconds since engine initialization (not OS process launch); truncated to millisecond resolution, never derived from wall-clock differences. |
| scope | string, optional | Execution/run identity captured at admission. Omitted outside an execution scope. Scope is not a verbosity category; use unique names when persisted runs must be distinguishable. |
| namespace | string | Stable, explicit semantic identity. ASCII segments matching [a-z][a-z0-9_]* separated by ::, without leading/trailing/empty segments. Independent of Rust type name. |
| level | string | Exactly INFO, WARN or ERROR. Severity, not detail/verbosity. DEBUG/TRACE do not exist in v1. |
| data | any JSON value | Structured typed payload, usually an object. Any shape Serde can represent as JSON is supported. End-span records use null. Non-finite floating-point values follow serde_json semantics (null). |
| message | string, optional | Human-readable supplement from Observation::message(), not a replacement for namespace or data. |
| phase | string, optional | start or end for a span. Omitted for ordinary point observations. |
| span | string, optional | Opaque unique span id. Present together with phase for engine-generated lifecycle records. |

Tags are selection metadata only. **No envelope tags, channel or channels fields are
emitted.** Fields with these names may legitimately occur *inside data*, like any user
payload; consumers must not mistake payload content for envelope metadata.

Example point record (one physical line):

```json
{"ts":"2026-10-20T00:00:00.000Z","proc":"1234_a1b2","proc_ms":1842,"scope":"run-c3d4","namespace":"compute::train::epoch","level":"INFO","data":{"epoch":7,"loss":0.0132}}
```

Example point outside any scope:

```json
{"ts":"2026-10-20T00:00:00.001Z","proc":"1234_a1b2","proc_ms":1843,"namespace":"service::ready","level":"INFO","data":{}}
```

## Lifecycle and timing

A successfully admitted span emits start followed on normal guard drop by end. The
pair has identical proc, scope (or absence), namespace, level and span. Start carries
the typed data and optional message; end has data: null and no message. Span ids are
opaque; no distributed trace identity, parent id or nesting relation is implied.

```json
{"ts":"2026-10-20T00:00:00.000Z","proc":"1234_a1b2","proc_ms":1842,"scope":"run-c3d4","namespace":"compute::fit","level":"INFO","data":{"samples":128},"phase":"start","span":"opaque-span-a"}
{"ts":"2026-10-20T00:00:00.125Z","proc":"1234_a1b2","proc_ms":1967,"scope":"run-c3d4","namespace":"compute::fit","level":"INFO","data":null,"phase":"end","span":"opaque-span-a"}
```

Pair by (proc, scope-or-absence, namespace, span), not by adjacency. Concurrent spans
can interleave, and records can be duplicated through fan-out. For one successful pair,
end.proc_ms - start.proc_ms gives elapsed duration at millisecond resolution; zero is
valid. Do not subtract wall-clock ts (which can jump) and do not compare proc_ms across
process/engine identities. No duration or total/self-time field is persisted.

Nested/overlapping spans measure overlapping intervals. Summing their durations as
though they were disjoint incorrectly overcounts time; a future aggregator must choose
explicit inclusive, exclusive or interval-union semantics. Point records are not durations.

Admission/selection/policy is evaluated once at start. End uses the captured scope and
sink snapshot even if those change later. Rejection emits neither record and constructs
no payload. Policy budgets count one span admission, not two lifecycle emissions.
Normal panic unwinding runs Drop; abort, leaked guards, termination, serialization/I/O
failure or incomplete transport can leave unmatched pairs. Readers must handle them
explicitly, not invent an end timestamp or duration.

## Ordering, fan-out and durability

Each shared JsonlSink serializes concurrent calls under a lock for whole-line writes.
Ordering is sink lock-acquisition order; concurrent timestamps can be equal or appear
out of order. There is no sequence number or cross-sink total ordering guarantee.

One observation may route to several logical channels and each channel to several sinks.
Repeated/overlapping routes to the same channel are deduplicated. The same sink attached
to different matching channels receives repeated writes; channels intentionally remain
absent from records. Consumers should decide whether their wiring requires avoiding such
duplicates; v1 provides no point-event id for automatic deduplication. A span id identifies
the span, not a globally unique lifecycle-record delivery.

File mode appends and never truncates existing contents. Share one sink instance for
concurrent writers to a destination; separate instances/processes are not coordinated.
Flush drains the writer, not an fsync guarantee. Failed/partial I/O can leave a torn final
line; sinks do not retry, roll back other sinks, repair preexisting files or rotate them.
Parsers should report malformed lines and explicitly choose a recovery policy.

## Namespace interpretation for future tools

Namespace is a segment-aware tree: compute::train includes itself and its descendants,
not compute::trainer. Display tools may collapse unary chains and aggregate descendants
at branching nodes while preserving exact-node/exact-child counts. Such views must state
whether span start/end records count as lifecycle records or completed spans. Aggregation
must not implicitly double-count overlapping durations. These are interpretation rules,
not an implemented stats/MCP engine in this library.
