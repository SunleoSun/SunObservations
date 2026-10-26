# SunObservations persisted format — v1

This document specifies the JSONL envelope produced by the runtime, independently of
Rust struct names and API module layout. It is the canonical input contract for future
parsers. No domain-specific fields or parser/query/stats implementation are defined here.

For application integration and operations, start with the [framework human guide](../README.md).
This document remains the canonical persisted-format contract.

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

Human sink rendering (`Observation::render_text`) is transient presentation, not part
of the v1 envelope and never persisted by JsonlSink. It is distinct from `message`.
Downstream presentation fields inside `data` remain ordinary typed payload fields.

## Envelope

| Member | JSON type | Meaning |
| --- | --- | --- |
| ts | string | UTC wall-clock RFC 3339 timestamp, exactly millisecond precision with Z suffix, e.g. 2026-10-20T00:00:00.000Z. For human/cross-process correlation, not elapsed timing. |
| proc | string | Opaque process-instance identity, fresh on singleton engine initialization. Consumers must not parse its internal spelling. Isolated engine instances have their own identity. |
| proc_ns | unsigned integer | Monotonic elapsed nanoseconds since engine initialization (not OS process launch), never derived from wall-clock differences. Units preserve available precision; platform clock resolution may be coarser. |
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
{"ts":"2026-10-20T00:00:00.000Z","proc":"1234_a1b2","proc_ns":1842000000,"scope":"run-c3d4","namespace":"compute::train::epoch","level":"INFO","data":{"epoch":7,"loss":0.0132}}
```

Example point outside any scope:

```json
{"ts":"2026-10-20T00:00:00.001Z","proc":"1234_a1b2","proc_ns":1843000000,"namespace":"service::ready","level":"INFO","data":{}}
```

## Lifecycle and timing

A successfully admitted span emits start followed on normal guard drop by end. The
pair has identical proc, scope (or absence), namespace, level and span. Start carries
the typed data and optional message; end has data: null and no message. Span ids are
opaque; no distributed trace identity, parent id or nesting relation is implied.

```json
{"ts":"2026-10-20T00:00:00.000Z","proc":"1234_a1b2","proc_ns":1842000000,"scope":"run-c3d4","namespace":"compute::fit","level":"INFO","data":{"samples":128},"phase":"start","span":"opaque-span-a"}
{"ts":"2026-10-20T00:00:00.125Z","proc":"1234_a1b2","proc_ns":1967000000,"scope":"run-c3d4","namespace":"compute::fit","level":"INFO","data":null,"phase":"end","span":"opaque-span-a"}
```

Pair by (proc, scope-or-absence, namespace, span), not by adjacency. Concurrent spans
can interleave, and records can be duplicated through fan-out. For one successful pair,
duration_ns = end.proc_ns - start.proc_ns gives elapsed duration in nanoseconds; zero is
valid. Do not subtract wall-clock ts (which can jump) and do not compare proc_ns across
process/engine identities. proc_ns is the only persisted elapsed clock; milliseconds and microseconds are derived views. Nanosecond units do not promise nanosecond hardware resolution or remove instrumentation/clock noise. No duration or total/self-time field is persisted.

Nested/overlapping spans measure overlapping intervals. Summing their durations as
though they were disjoint incorrectly overcounts time; a future aggregator must choose
explicit inclusive, exclusive or interval-union semantics. Point records are not durations.

Admission/selection/policy is evaluated once at start. End uses the captured scope and
sink snapshot even if those change later. Rejection emits neither record and constructs
no payload. Policy budgets count one span admission, not two lifecycle emissions.
Normal panic unwinding runs Drop; abort, leaked guards, termination, serialization/I/O
failure or incomplete transport can leave unmatched pairs. Readers must handle them
explicitly, not invent an end timestamp or duration. Evidence preparation requires exactly one start/end per identity with matching level and end.proc_ns >= start.proc_ns. Missing records, duplicate lifecycle deliveries (even identical fan-out copies), or invalid pairs fail closed; no partial profiling sample is produced. Record delivery order need not match timestamp order. Applications must prevent fan-out duplicates in a profiling input rather than silently deduplicating them.

## Ordering, fan-out and asynchronous persistence

JsonlSink enqueues immutable shared records into an unbounded queue. One lazy worker
per sink performs JSONL encoding, buffered writes, flush and optional file compaction.
Construction/registration alone creates neither a file nor a worker. Queue order defines
per-sink delivery order; concurrent timestamps can be equal or appear out of order. There
is no sequence number or cross-sink total ordering guarantee. ts/proc_ns are captured at
observation emission, **not** at background persistence, and do not include queue latency.

One observation may route to several logical channels and each channel to several sinks.
Repeated/overlapping routes to the same channel are deduplicated. The same sink attached
to different matching channels receives repeated writes; channels intentionally remain
absent from records. Consumers should decide whether their wiring requires avoiding such
duplicates; v1 provides no point-event id for automatic deduplication. A span id identifies
the span, not a globally unique lifecycle-record delivery.

Without a size limit, file mode appends without truncation. With retention configured,
after exceeding maximum bytes the worker trims oldest **whole records** toward
max_bytes * (1 - trim_fraction). It aligns the approximate cut forward to a newline,
copies the complete suffix to a temporary sibling, then replaces the original file.
The newest record is always retained whole: an oversized single record can exceed the
maximum. A normal cut can undershoot the target by a line; later appends grow within
the hysteresis window. The configured maximum is approximate during batches. This
bounded historical retention is deliberate, not queue-overflow loss. Trimming can remove
span starts/ends independently; retained files can legitimately contain unmatched spans.

Share one sink instance for a physical destination. Separate instances/processes writing
or compacting the same file are not coordinated. Periodic flush defaults to 1000 ms
(configurable); it is best-effort at batch boundaries, not a real-time deadline. Underlying
I/O or a large backlog can delay it. The queue is unbounded: slow disks consume memory,
not overflow drops. A successful enqueue does not establish persistence.

After successful engine.flush_channels(), each currently attached sink has processed its
queue prefix up to its barrier and its underlying writer has been flushed. Writes concurrent
with the barrier may or may not be included. Flush is not fsync or a disk durability/crash
recovery guarantee. Use an explicit barrier before reading the file or deterministic global
shutdown; process-global Rust statics are not dropped automatically. Last-owner sink Drop
drains the queue, flushes and joins the writer. A detached sink needs its own barrier if it
is still used by a captured span/old plan.

Background failure is sticky and observable through last_error(), subsequent write/flush
errors and a stderr report at last-owner Drop. There is no automatic retry, multi-sink
rollback or repair of preexisting corruption. Failed/partial I/O or process termination can
leave a torn final line. Parsers must report malformed lines and choose an explicit recovery
policy. Retention replacement only operates on complete records from a valid JSONL file;
it is not a repair mechanism for a preexisting incomplete tail.

## Namespace interpretation for future tools

Namespace is a segment-aware tree: compute::train includes itself and its descendants,
not compute::trainer. Display tools may collapse unary chains and aggregate descendants
at branching nodes while preserving exact-node/exact-child counts. Such views must state
whether span start/end records count as lifecycle records or completed spans. Aggregation
must not implicitly double-count overlapping durations. These are interpretation rules,
not an implemented stats/MCP engine in this library.
