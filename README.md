# SunObservations

Typed, structured runtime observations for Rust applications: production events,
selective diagnostics, in-process capture, and coarse lifecycle profiling without
passing a logger or recorder through domain methods.

## Overview

Use SunObservations when the identity and payload of an event should be a typed
application contract, while activation, execution context, and output destinations
remain runtime concerns. An Observation describes **what happened**, not where to
write it. It can be selected by semantic namespace or cross-cutting tags, limited
by a scope-local Policy, and delivered to several Sinks.

Principal properties:

- Explicit, stable namespaces independent of Rust type names; Serde payloads.
- Lazy admission: rejected observations do not construct or serialize their payload.
- Execution-local Scopes and explicitly propagated Context, including optional Rayon.
- Independent routing to structured Memory/JSONL, asynchronous human Console, or custom Sinks.
- Opt-in transient `render_text()`, evaluated once only for human-capable routes.
- RAII Spans and a documented persisted JSONL contract.
- Executable production-contract certifications and measured performance evidence.

This is not a distributed tracing system or an automatic async-runtime integration.
If bounded backlog, fsync durability, or transactional delivery across destinations
is required, the built-in Sinks do not provide those guarantees.

```text
typed Observation expression
          |
          v
 admission: Selection + exclusions + Policy
          | rejected: no payload construction
          v
 payload serialization -> ObservationRecord
          |
          v
 namespace routing
          +---- Channel A ---- Sink(s)
          +---- Channel B ---- Sink(s)
```

### Guide map

- [Quick Start](#quick-start)
- [Core Model](#core-model), [Observations](#observations), [Namespaces and Tags](#namespaces-and-tags)
- [Runtime Selection](#runtime-selection), [Scopes and Policies](#scopes-and-policies)
- [Threads and Rayon](#threads-and-rayon), [Channels and Routing](#channels-and-routing)
- [Human Rendering](#human-rendering), [Sinks](#sinks), [Spans](#spans), [Error Handling and Shutdown](#error-handling-and-shutdown)
- [Persisted Format](#persisted-format), [Benchmarks](#benchmarks), [Examples](#examples)
- [Production Usage Patterns](#production-usage-patterns), [Performance Guidelines](#performance-guidelines)
- [Production Certification](#production-certification), [Guarantees and Non-Guarantees](#guarantees-and-non-guarantees)
- [Public API Surface](#public-api-surface), [Development and Release Checks](#development-and-release-checks)

| Document | Role |
| --- | --- |
| This README | Primary production integration and operations guide |
| [FORMAT.md](FORMAT.md) | **Canonical** persisted JSONL compatibility contract |
| [CERTIFICATION.md](CERTIFICATION.md) | **Canonical** certification matrix, executable evidence, and release gate |
| [BENCHMARKS.md](BENCHMARKS.md) | Principal performance methodology and measured results |
| [BATCH_BENCHMARKS.md](BATCH_BENCHMARKS.md) | Bounded JSONL writer experiment and production measurements |
| [PERFORMANCE_INVESTIGATION.md](PERFORMANCE_INVESTIGATION.md) | Historical engineering investigation, not current usage guidance |

## Quick Start

Use the runtime crate and Serde. For a local checkout, adjust this path to your
application's location; these instructions do not assume a published crates.io release.
The workspace uses Rust **edition 2021**. The derive macro is re-exported by the runtime;
no separate derive dependency is needed.

```toml
[dependencies]
sun-observations = { path = "../SunObservations/crates/sun-observations" }
serde = { version = "1", features = ["derive"] }
```

The complete program below appends a typed request Observation to a retained JSONL
file. It configures activation **and** routing; neither substitutes for the other.
It is the [executable jsonl example](examples/jsonl.rs).

```rust,no_run
use serde::Serialize;
use std::sync::Arc;
use sun_observations::{global, observe, JsonlConfig, JsonlSink, Observation, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "service::request", tags("production"))]
struct Request {
    path: &'static str,
    status: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = global();
    let config = JsonlConfig::new()
        .flush_ms(1000)
        .file_size_mb(10)
        .trim_fraction(0.10);
    let file = Arc::new(JsonlSink::with_config("observations.jsonl", config)?);
    engine.add_channel_sink("operations", file.clone());
    engine.register_namespace_channel("service", "operations")?;
    engine.set_selection(Selection::new().activate_namespace("service"));

    let emission = observe!(Request {
        path: "/health",
        status: 200,
    });
    let persistence = engine.flush_channels();
    emission?;
    persistence?;
    assert!(file.last_error().is_none());
    assert_eq!(file.stats().records_written, 1);
    Ok(())
}
```

Run from this repository:

```sh
cargo run -p sun-observations --example jsonl
```

The output is one compact JSON object per physical line in `observations.jsonl`.
The example retains up to approximately 10 MiB of history, trimming toward 9 MiB.
For unlimited append, use `JsonlSink::append(path)?` instead. Ensure the parent directory
exists and is writable; file opening happens asynchronously on the first write.

`observe!` returns `Result<bool, ObservationError>`. `Ok(false)` means rejected;
`Ok(true)` means admitted and dispatched successfully to the current targets, possibly
zero Sinks. With JSONL, dispatch success means **enqueue**, not completed file I/O.
The explicit `flush_channels()` is the application-level persistence/error barrier,
not fsync. The example evaluates that barrier even if emission failed; a real
application should report both errors if both operations fail.

## Core Model

| Concept | Meaning |
| --- | --- |
| Observation | A serializable Rust value with static semantic metadata |
| namespace | The Observation's stable semantic hierarchy, persisted in each record |
| tags | Static cross-cutting selection facets; not persisted envelope metadata |
| level | Severity: `INFO`, `WARN`, or `ERROR`; not verbosity |
| Selection | Positive namespace/tag rules and exclusion vetoes |
| Scope | An execution identity, such as a request, training run, or job |
| Policy | Occurrence control for one concrete Observation type in a Scope |
| Context | A capturable handle to the current Scope, or explicit absence of one |
| Channel | A logical routing destination with attached Sinks |
| Sink | A structured or human consumer; human consumers opt into transient rendering |
| ObservationDelivery | Shared canonical record plus optional shared human text; not a persisted envelope |
| Span | One admitted RAII lifecycle producing paired start/end records |

Selection answers **whether** to emit. Scope answers **which execution** owns it.
Routing answers **where** it goes. An Observation type knows no Channel or Sink.
A Scope is not a logger category; a Channel name is not a namespace or persisted field.

`global()` returns one lazily initialized process-wide `ObservationEngine`. Public
macros always use it. Independent `ObservationEngine::new()` instances have their own
Selection, routing, process identity, and clocks. Use their closure methods for
isolated embedding or tests:

```rust
use serde::Serialize;
use sun_observations::{Observation, ObservationEngine, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "service::ready")]
struct Ready;

let engine = ObservationEngine::new();
engine.set_selection(Selection::new().activate_all());
assert!(engine.observe_if_active(|| Ready)?);
# Ok::<(), sun_observations::ObservationError>(())
```

Even without routed Sinks, an admitted point Observation runs payload serialization
and its message callback. No destination is not the same as disabled instrumentation.

## Observations

Derive `Serialize` and `Observation` on an owned serializable type. No type registration
is required. The trait requires `Serialize + 'static`; generic types are supported when
their concrete instantiations satisfy those bounds. Borrowed non-static payload types
do not satisfy this contract.

```rust
use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize, Observation)]
#[observation(
    namespace = "encoder::train::epoch",
    level = INFO,
    tags("training", "performance")
)]
struct EpochObservation {
    epoch: usize,
    loss: f32,
}
```

`namespace` is mandatory. Level defaults to `INFO` and tags default to empty.
The derive rejects missing/invalid namespaces, invalid tags/levels, duplicate metadata
attributes, and unknown attributes at compile time. `DEBUG` and `TRACE` do not exist.

An optional envelope `message` comes from `Observation::message() -> Option<&str>`.
To customize it, implement `Observation` manually instead of deriving it, as in
[spans.rs](examples/spans.rs). The derive has no message attribute. A payload field
named `message` remains an ordinary field inside `data`.

Put expensive work **inside** the macro expression or the closure passed to
`observe_if_active::<Type, _>(|| payload)`. Work done before the call cannot become
lazy retroactively. Rejection skips payload construction, serialization, and the
message callback; admission performs real Serde-to-JSON-value work on the producer.

## Human Rendering

Structured data remains canonical. These are three distinct contracts:

| Source | Meaning |
| --- | --- |
| Serde payload / record `data` | Canonical machine-readable data |
| `message() -> Option<&str>` | Optional persisted semantic context |
| `render_text() -> Option<String>` | Optional transient human projection; never added to JSONL or MemorySink |

Implement the trait manually to customize rendering; existing derives/types work unchanged:

~~~rust
use serde::Serialize;
use sun_observations::Observation;

#[derive(Serialize)]
struct Epoch { epoch: u64, loss: f64, accuracy: f64 }
impl Observation for Epoch {
    const NAMESPACE: &'static str = "encoder::train::epoch";
    fn render_text(&self) -> Option<String> {
        Some(format!("{:>5} | {:8.4} | {:8.4}", self.epoch, self.loss, self.accuracy))
    }
}
~~~

The default renderer returns `None`. Rejected, excluded, and Never observations do
not build, serialize, call message, or render. Admitted sinkless observations retain
serialization/message callbacks but never render. Memory-only, JSONL-only, and their
combination never call the renderer or allocate human text, even on a render-capable
type. A cached routing decision requests rendering only when at least one routed Sink
opts in. Capability is sampled once at attachment, without engine locks; plans aggregate
the published routing flags. Attaching/detaching sinks or adding routes invalidates
that decision without resetting counting Policies. Capabilities are not rescanned on warm calls.

With human consumers, rendering runs synchronously on the producer after successful
serialization, at most once per admitted payload. Multiple human sinks share the same
text and immutable record; no `Arc<ObservationDelivery>` is allocated. Keep expensive
formatting inside `render_text()` rather than doing it before admission. A renderer
panic propagates like a payload/message callback panic; it is not an I/O error.

### Tables are observation streams

Use independent ordinary types: `TableStart { columns }`, `Epoch { epoch, loss, accuracy }`,
and `TableEnd { epochs }`. Each implements its own `render_text()` for header/row/footer.
Emit `TableStart → Epoch → Epoch → TableEnd` and explicitly flush. The human sink sees:

~~~text
Epoch | Loss     | Accuracy
------+----------+---------
    1 |   0.3800 |   0.6300
    2 |   0.1900 |   0.6500
------+----------+---------
~~~

[human.rs](examples/human.rs) implements and emits all three types, plus a default-
formatted status to stderr. Add a JsonlSink to the table channel to independently
persist four structured records; header/footer text does not enter those records.
No Span is needed. Spans remain lifecycle/profiling observations and do not invoke
`render_text()`; human sinks use their default structured formatter for span lifecycle records.

Presentation fields chosen by downstream code remain ordinary structured `data`.
The core defines no tone, kind, table phase, styling, terminal, or UI event model.

## Namespaces and Tags

### Design a semantic hierarchy

Namespace segments match lowercase ASCII `[a-z][a-z0-9_]*` and are separated by `::`.
There are no leading/trailing separators or empty segments. Choose a stable identity
for the meaning of the event, independent of Rust modules or type names. Renaming
`EpochObservation` does not change `encoder::train::epoch`.

A namespace selector or route matches a **segment-aware subtree**:

| Selector `encoder::train` | Matches? |
| --- | --- |
| `encoder::train` | Yes: the node itself |
| `encoder::train::epoch` | Yes: a descendant |
| `encoder::train::batch` | Yes: a descendant |
| `encoder::trainer` | No: different segment |

This is not arbitrary string-prefix matching. No wildcard syntax is needed.

### Use tags for cross-cutting facets

Tags use the same **single-segment** grammar. Examples: `diagnostic`, `profiling`,
`performance`, `expensive`, `certification`, `low_level`. Matching any selected tag
activates a type. Use tags to select related instrumentation across semantic owners,
not as a replacement for namespace ownership. Tags are static type metadata and
are **not** persisted in the JSONL envelope. Namespace is persisted.

## Runtime Selection

An engine starts with empty Selection. A new Scope also starts with empty local
Selection. Empty positive rules activate nothing; a Scope can nevertheless receive
activation from the engine, and vice versa.

```text
active   = global namespace/tag match OR scope namespace/tag match
excluded = global namespace/tag exclusion OR scope namespace/tag exclusion
emit     = active AND NOT excluded AND Policy allows
```

`activate_all()` counts as a positive match. Any exclusion vetoes activation,
including `activate_all()`. Namespace and tag positives are **OR**, not AND; exclusion
by either kind wins. A Scope cannot undo a global exclusion, nor can global activation
undo a scope exclusion. `WARN` and `ERROR` must pass the same rules as `INFO`.

```rust
use sun_observations::{global, Selection};

global().set_selection(
    Selection::new()
        .activate_namespace("service")
        .activate_tags(["production"])
        .exclude_namespace("service::wire")
        .exclude_tags(["expensive", "low_level"]),
);
```

`set_selection()` replaces the engine's Selection; it does not incrementally merge it.
Use `Selection::new().activate_all()` when explicitly selecting every type.
Routing alone never activates anything. Conversely, activation does not create routes.

Runtime namespace/tag selector builders panic on invalid input. Validate externally
supplied configuration before building it; `namespace::validate_namespace()` is public
for namespaces. `register_namespace_channel()` instead returns `InvalidNamespace` on
an invalid route without partially installing it.

Selection/routing/attachment changes invalidate future observation decisions without
resetting existing scope counters. Already admitted Spans retain their start-time
routing and Scope. Reconfiguration is not retroactive delivery or a global stop-the-world
cutover for concurrent calls; quiesce producers when a precise application boundary matters.

## Scopes and Policies

### Scope lifetime and identity

Enter a Scope at an execution boundary and keep its guard alive:

```rust
use serde::Serialize;
use sun_observations::{observe, start_scope, Observation, Policy, ScopeConfig};

#[derive(Serialize, Observation)]
#[observation(namespace = "encoder::train::batch", tags("expensive"))]
struct Batch {
    index: usize,
    norm: f64,
}

fn compute_norm() -> f64 { 0.125 }

let scope = start_scope(
    ScopeConfig::new("training-run-42")
        .activate_namespace("encoder::train")
        .override_policy::<Batch>(Policy::EveryN(100)),
);
for index in 1..=200 {
    observe!(Batch { index, norm: compute_norm() })?;
}
drop(scope);
# Ok::<(), sun_observations::ObservationError>(())
```

With no exclusion veto, the 100th and 200th selected calls construct their payload.
Choose unique Scope names when persisted runs must be distinguishable. Starting two
Scopes with the same name creates independent executions/counters, but the persisted
name alone will not distinguish them.

Scope installation is thread-local, not a global mutable current Scope. A nested
`start_scope()` creates a **fresh configuration**: it does not inherit parent selectors,
policy overrides, or counters. Engine Selection still applies. Dropping the inner guard
restores the parent Context, including during normal panic unwinding. Guards cannot
move between threads. Outside a Scope, the envelope omits `scope`.

`ScopeConfig::selection(selection)` sets a complete local Selection. Its convenience
`activate_namespace`, `activate_tags`, `exclude_namespace`, `exclude_tags`, and
`activate_all` builders use the same semantics as engine Selection.

### Occurrence policies

The default is universally **`Policy::Always`**. There is no per-type registration or
global policy override. Overrides belong to `ScopeConfig::override_policy::<Type>()`.
Outside a Scope, selected observations use `Always`.

| Policy | Selected occurrences admitted |
| --- | --- |
| `Always` | Every occurrence |
| `Never` | None; payload remains lazy |
| `FirstN(n)` | The first `n` |
| `EveryN(n)` | Occurrences `n, 2n, 3n, ...` (one-based) |
| `MaxPerScope(n)` | A cap of the first `n`; equivalent to `FirstN` in the current model |

Numeric policies with zero reject everything. Counters belong to **Scope instance +
concrete Rust Observation type**, not namespace, thread, or Channel. Two types sharing
a namespace still have separate budgets. Propagated children share the parent's
counter, even across independent engines using that captured Scope. Separate Scopes
have separate counters. Changing engine configuration does not restart a budget.

Selection/exclusion rejection does not advance the counter. A selected occurrence
advances a counting Policy even if the Policy rejects it; an admitted attempt still
consumes its occurrence if subsequent serialization or Sink delivery fails. Spans
consume one admission, not two. Counters saturate rather than wrap. Under concurrency,
the exact admitted budget is enforced, but which inputs win is scheduling-dependent.

### Captured Context

`Context::capture()` clones a handle to the current execution. `enter()` installs it with
an RAII guard; `run(|| ...)` installs it only for that operation. Captures can outlive
the original scope guard and keep its configuration and counters alive.

```rust
use sun_observations::{start_scope, Context, ScopeConfig};

let scope = start_scope(ScopeConfig::new("job-42"));
let captured = Context::capture();
drop(scope);
captured.run(|| assert_eq!(Context::capture().scope_name(), Some("job-42")));
assert_eq!(Context::capture().scope_name(), None);
```

`Context::default()` is explicitly **empty**, not "inherit whatever is current".
Running it masks an unrelated worker's Scope and restores that Scope afterward;
it does not disable engine-wide Selection. Context carries Scope state, not a
replacement engine or a Span parent relationship.

## Threads and Rayon

### OS threads: propagation is explicit

| Boundary | Context inside the child |
| --- | --- |
| `std::thread::spawn(...)` | Empty; no inherited Scope, local Selection, or Policy |
| `context::thread::spawn(...)` | Captures the caller's Context at spawn and installs it in the child |
| `captured.run(...)` / `captured.enter()` | Explicitly installs that captured Context |

```rust
use sun_observations::{context, start_scope, Context, ScopeConfig};

let _scope = start_scope(ScopeConfig::new("request-42"));
std::thread::spawn(|| {
    assert_eq!(Context::capture().scope_name(), None);
}).join().unwrap();
context::thread::spawn(|| {
    assert_eq!(Context::capture().scope_name(), Some("request-42"));
    context::thread::spawn(|| {
        assert_eq!(Context::capture().scope_name(), Some("request-42"));
    }).join().unwrap();
}).join().unwrap();
```

The wrapper supports transitive propagation when children use it again. Plain
threads can still emit **unscoped** observations enabled by engine Selection; empty
Context does not mean globally inactive. For an existing executor or scoped OS
threads, capture a Context and install it explicitly at the execution boundary.
[threads.rs](examples/threads.rs) checks propagation, isolation, captured lifetime,
and one shared parent/child Policy budget with actual records.

### Optional Rayon integration

Enable the runtime's `rayon` feature. Consumers using Rayon parallel iterators also
need their own Rayon dependency:

```toml
[dependencies]
sun-observations = { path = "../SunObservations/crates/sun-observations", features = ["rayon"] }
serde = { version = "1", features = ["derive"] }
rayon = "1.10"
```

`context::rayon::ContextThreadPool` captures Context **at pool construction**, not
at each later `install()`. The dedicated pool stays bound to that execution for its
lifetime, including nested Rayon work. Reusing it from a different caller does not
adopt the new caller's Scope:

```rust
# #[cfg(feature = "rayon")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use rayon::prelude::*;
use sun_observations::{context::rayon::ContextThreadPool, start_scope, Context, ScopeConfig};

let origin = start_scope(ScopeConfig::new("pool-owner"));
let pool = ContextThreadPool::new(4)?;
drop(origin);
let _caller = start_scope(ScopeConfig::new("another-caller"));
pool.install(|| {
    (0..10).into_par_iter().for_each(|_| {
        assert_eq!(Context::capture().scope_name(), Some("pool-owner"));
    });
});
assert_eq!(Context::capture().scope_name(), Some("another-caller"));
# Ok(())
# }
# #[cfg(not(feature = "rayon"))]
# fn main() {}
```

`new(0)` uses Rayon's default worker count. Build a new context-bound pool for a new
execution boundary. This does not alter the global Rayon pool. A Scope opened inside
a pool job is local to that thread; it does not automatically replace Context on
other workers. The [Rayon example](examples/rayon.rs) exercises nested parallel work
and a shared scope cap.

Arbitrary async runtimes and external thread pools are **not** automatically integrated.
Do not hold a thread-local scope/context guard across async suspension or migration;
task-local propagation is outside this library's current contract.

## Channels and Routing

Configure the graph on the engine:

| Method | Effect |
| --- | --- |
| `register_namespace_channel(namespace, channel)` | Route that namespace subtree to a Channel |
| `add_channel_sink(channel, Arc<dyn Sink>)` | Attach a consumer to that Channel |
| `clear_channel_sinks(channel)` | Detach that Channel's Sinks |

For `service::jobs::completed`, overlapping routes fan out:

```text
service --------------------> Channel "operations" ---> JsonlSink
   |
   +-- service::jobs --------> Channel "jobs" ----------> MemorySink
                                              +-------> another Sink
```

```rust
use std::sync::Arc;
use sun_observations::{global, JsonlSink, MemorySink};

let memory = Arc::new(MemorySink::new());
global().add_channel_sink("operations", Arc::new(JsonlSink::new(std::io::stdout())));
global().add_channel_sink("jobs", memory);
global().register_namespace_channel("service", "operations")?;
global().register_namespace_channel("service::jobs", "jobs")?;
# Ok::<(), sun_observations::namespace::InvalidNamespace>(())
```

This is fan-out, not longest-prefix-wins routing. Repeating an identical route is
deduplicated, and multiple matching routes to the **same Channel** produce one Channel
visit. The same physical Sink attached through **different matching Channels** can
intentionally receive the same record twice. Repeated Sink attachments are not a
record-deduplication feature; wire only the multiplicity you want.

The engine materializes one immutable shared `ObservationRecord` and shares it across
fan-out; it does not serialize the typed payload once per Sink. Channel names and
tags are not added to the envelope. FORMAT v1 has no point-event delivery ID for
automatically removing duplicates. Cross-Sink total ordering and atomic rollback are
not promised. All targets are attempted if one returns an error.

The [Channels example](examples/channels.rs) demonstrates overlapping subtrees and
multiple Sinks on one Channel. Configuration mutation goes through the engine;
there is no mutable `channel().add_sink()` handle.

## Sinks

### MemorySink

Use `MemorySink` for tests, certifications, diagnostics, and in-process consumers.
It synchronously captures shared records and is safe to share between producers.
It is not a bounded ring buffer: records accumulate until drained or dropped.

| Method | Result |
| --- | --- |
| `shared_records()` | Snapshot of shared record handles without deep-copying payloads |
| `drain_shared()` | Remove all stored handles and return them |
| `records()` | Owned snapshot, cloning records/payloads |
| `drain()` | Remove records and return owned values |
| `len()` / `is_empty()` | Current capture size |

Capture is synchronized and can contend at high concurrent production rates.
Snapshot/drain at appropriate consumer boundaries; prefer shared handles for large
captures. It is not persistent storage, and flush has no file effect.

### JsonlSink

Use `JsonlSink` for UTF-8 JSON Lines to one file or an owned
`Write + Send + 'static` destination such as stdout.

```text
many producer threads
          |
          v
 one JsonlSink: unbounded queue
          |
          v
 one lazy background writer thread
          |
          v
 one destination file / writer
```

**The writer belongs to the Sink instance, not a producer thread.** Share one
`Arc<JsonlSink>` per physical destination. Independent instances/processes writing or
compacting the same file are not coordinated.

#### Construction and configuration

| Constructor | Destination / behavior |
| --- | --- |
| `JsonlSink::append(path)?` | File append, default configuration |
| `JsonlSink::with_config(path, config)?` | File append with explicit settings |
| `JsonlSink::new(writer)` | Owned writer, default configuration |
| `JsonlSink::writer_with_config(writer, config)?` | Owned writer with settings; file-size retention is rejected |

Defaults: 1000 ms periodic flush, no file-size limit, trim fraction 0.10 if retention
is enabled. `file_size_mb(n)` means **MiB** (`n * 1024 * 1024` bytes).
`max_file_size_bytes(n)` provides an exact byte setting. `trim_fraction` must be
finite and strictly between 0 and 1. Zero flush interval/size and an unrepresentable
flush interval fail construction with `InvalidInput`.

Construction, registration, and flushing an unused Sink create **neither a file nor
a worker**. First write starts the lazy worker; it opens the destination. Constructor
success validates configuration, not file access. Missing parents, permissions, and
other file-open errors can surface only asynchronously.

#### Producer/worker boundary and queue risk

Producers perform admission, payload construction, Serde-to-value materialization,
record/timestamp construction, and enqueue. They do **no file open, JSONL encoding,
file write, flush, or compaction**. Custom writer destinations use the same background
path; they are not a separate synchronous mode.

The queue is intentionally **unbounded**. Sustained producer throughput above writer
capacity grows backlog and RAM usage, potentially exhausting memory. There is no
bounded-queue/backpressure/overflow-drop guarantee.

The worker processes FIFO chunks of currently at most **8192 messages** (including
barriers, so at most 8192 records per iteration). This internal working-set bound is
not a public tuning setting, not a bound on producer backlog, not a byte limit for
arbitrary payloads, and not an enqueue/persistence latency guarantee.

#### Flush and diagnostics

Periodic flush is best-effort while idle and under traffic, checked at batch boundaries.
It is not a real-time 1000 ms deadline; slow I/O or processing can delay it.

`engine.flush_channels()?` flushes each unique currently attached physical Sink once,
including attached but unrouted Sinks. For JSONL it places a FIFO barrier and waits
for that queue prefix to be processed and the underlying writer flushed. Concurrent
writes may or may not be included. No engine configuration lock is held while waiting.

Import the `Sink` trait to call `file.flush()?` directly on a retained Sink handle.
Use that when a Sink is detached: the current engine graph no longer includes it,
although an admitted Span or cached plan can still hold it alive. Finish Spans and
stop/join producers before the final barrier. **Flush is not fsync** and does not
promise power-loss durability or crash recovery.

`is_started()` reports worker initialization. `last_error()` reports the sticky first
failure without clearing it. `stats()` exposes `records_written`, `batches`,
`flushes`, and `compactions`. These are diagnostics, not a queue-depth API or fsync
receipt: writes may still be buffered, counters can be observed between operations,
and retention can remove records included in `records_written`.

#### Line-safe retention

With a maximum configured, after exceeding it the worker trims oldest **complete
records** toward `max_bytes * (1 - trim_fraction)`. It aligns the approximate byte cut
to a line boundary, copies the complete newest suffix to a temporary sibling, and
replaces the destination. Directory permissions must allow that replacement work.

- The newest record is always retained whole, even when it alone exceeds the maximum.
- Size is approximate during batches; a boundary cut can undershoot the target by a
  line, and later appends refill the hysteresis window.
- Retention intentionally loses old history, including either half of a Span pair.
- Preexisting retained history is validated as complete JSON object lines before append.
  Malformed history fails closed with a sticky error, without silently repairing it.
  This is framing/JSON validation, not certification of arbitrary hand-written envelopes.
- Without retention, append does not retrospectively validate or repair existing history.
- Compaction/replacement errors are sticky too. Small limits and frequent replacement
  can be expensive; see the [retention measurements](BENCHMARKS.md#writer-batches-and-line-safe-compaction).

#### Local Drop and global shutdown

Last-owner `JsonlSink` Drop closes the queue, drains queued messages, flushes, and
joins the worker, subject to I/O failure. It can block and reports errors to stderr,
not to a caller. Dropping one `Arc` is not necessarily last-owner Drop: engines,
Contexts with cached plans, and Spans can extend Sink lifetime. An independent engine's
Drop also attempts a channel flush.

**Process-global statics are not automatically dropped on normal process exit.**
Applications using `global()` must explicitly flush before returning from `main`
when deterministic persistence is required. Do not rely on sleep, periodic flush,
or destructor ordering as the shutdown protocol.

### ConsoleSink

`JsonlSink` and `MemorySink` are structured-only. `ConsoleSink` is human output:

~~~rust,no_run
use std::sync::Arc;
use sun_observations::{ConsoleSink, JsonlSink, ObservationEngine, Selection};
let engine = ObservationEngine::new();
engine.set_selection(Selection::new().activate_namespace("encoder"));
engine.register_namespace_channel("encoder", "output")?;
engine.add_channel_sink("output", Arc::new(ConsoleSink::stdout()));
engine.add_channel_sink("output", Arc::new(JsonlSink::append("training.jsonl")?));
// ConsoleSink::stderr() selects stderr instead. Both may be used on distinct channels.
engine.flush_channels()?;
# Ok::<(), Box<dyn std::error::Error>>(())
~~~

`Some(text)` is the complete human representation: literal UTF-8, no prefix, no
trimming, with one final LF added only if absent (empty text produces a blank line).
This preserves multiline tables. It is not escaped or sanitized. `None` uses:

~~~text
2026-10-20T00:00:00.000Z INFO [training] encoder::train::epoch data={"epoch":1,"loss":0.38}
~~~

The exact order is RFC3339 record timestamp, level, optional ` [scope]`, namespace,
optional ` phase=start|end`, optional ` span=id`, optional ` message="JSON-escaped context"`,
then ` data=<compact JSON>` and LF. Scope/span identifiers are literal. Custom text
replaces this entire format; it never repurposes persisted `message`.

One lazy worker per instance owns output I/O and default formatting. Producer calls
only render when needed, materialize the canonical record, and enqueue shared ownership;
no console I/O runs on producers. Construction and unused flush start no thread. The
unbounded FIFO queue is processed in chunks of at most 1024 messages, not a byte limit
or backpressure. Each stdout/stderr chunk holds its stream lock, so a multiline record
is not interleaved with another ConsoleSink record. Concurrent producers have per-
producer order, not a fictitious global chronology or cross-sink ordering guarantee.

`flush()`/`engine.flush_channels()` waits for the queued prefix and output flush; racing
later writes need not be included. Last-owner local Drop disconnects, drains, flushes,
and joins, subject to failure; it may block and cannot return errors. Engine plans,
captured contexts and spans can retain ownership. Global statics require explicit flush
before exit. Write/flush failures and caught worker panics are sticky, stop the worker,
release pending barriers, and surface through `last_error()` and subsequent writes/flush.
There is no retry, fsync promise, or rollback of partial output. Retain the handle to
check errors, especially after detaching. Slow destinations can grow queue memory.

`ConsoleSink::new(writer)` accepts an owned `Write + Send + 'static` output using exactly
the same worker/barrier path, useful for embedding, deterministic tests, and benchmarks.
It is a human writer, not an alternate JSONL serializer.

### Custom Sinks

Implement `Sink: Send + Sync + 'static` with `write(&ObservationRecord) -> io::Result<()>`.
The default `write_shared(Arc<ObservationRecord>)` calls `write`; override it when
retaining/enqueuing ownership to avoid deep cloning. Implement `flush()` to honor any
buffering/barrier contract your consumer needs (the default is a no-op). Callbacks
run without engine/configuration locks, can be concurrent, and must synchronize their
own state. The engine cannot give a custom Sink stronger persistence guarantees than
its implementation provides.

Existing structured implementations need no changes. To consume human rendering,
implement `wants_render_text() -> bool` returning true and override `write_delivery()`.
The default delivery method calls `write_shared()`, preserving legacy structured sinks.
Capability must be immutable while attached; reconfigure through engine attachment
boundaries rather than silently toggling it. Human callbacks can also receive `None`
(e.g. the renderer default or a Span end); the sink chooses its own fallback.

~~~rust
use std::{io, sync::{Arc, Mutex}};
use sun_observations::{ObservationDelivery, ObservationRecord, Sink};

#[derive(Default)]
struct HumanQueue(Mutex<Vec<ObservationDelivery>>);
impl Sink for HumanQueue {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.write_delivery(&ObservationDelivery::new(Arc::new(record.clone()), None))
    }
    fn wants_render_text(&self) -> bool { true }
    fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()> {
        self.0.lock().unwrap().push(delivery.clone());
        Ok(())
    }
}
let sink = HumanQueue::default();
# let _ = sink;
~~~

A real async downstream sink enqueues the cloned stack value to its worker rather than
retaining an unbounded Vec. `record()` and `render_text()` borrow the values;
`shared_record()` exposes `&Arc<ObservationRecord>` and `shared_render_text()` exposes
`Option<&Arc<String>>` for independent async ownership. Cloning a delivery shares both;
it neither re-renders nor deep-clones the record. `Arc<String>` preserves the rendered
String buffer without copying bytes into `Arc<str>`. Only `Some(text)` allocates its
shared header. No delivery wrapper heap allocation is needed, including on structured
paths. A downstream human sink can interpret its application's ordinary `data` fields
without any framework-specific UI dependencies.

## Spans

`observe_span!` returns `Result<SpanGuard, ObservationError>`. Keep the guard alive
for the operation's interval:

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

An admitted Span emits `phase=start` with typed `data` and optional message. Normal
Drop or explicit consuming `finish()` emits `phase=end` with `data: null` and no
message. `finish()` never emits twice and can return an end dispatch error.
A rejected Span returns an inactive guard (`is_active() == false`), builds no
payload, and emits neither record.

Start and end share process identity, Scope (or its absence), namespace, level, and
opaque span ID. Admission/Policy runs **once**. Scope and destination snapshot are
frozen at admission: dropping the scope or changing Selection/routes does not redirect
or suppress the end. Normal panic **unwinding** runs Drop; abort, leaked guards,
termination, failed start/end delivery, and retention can leave unmatched records.
A failed start dispatch can already have reached some Sinks; no rollback is attempted.

Pair by `(proc, scope-or-absence, namespace, span)`, not adjacency. Fan-out can duplicate
records and concurrent Spans interleave. For a successful pair, subtract `proc_ms`
for millisecond-resolution elapsed time; zero is valid. Do not subtract wall-clock
`ts` or compare elapsed clocks across engines. No parent tree, distributed trace ID,
automatic duration field, or exclusive/self-time aggregation is provided. Overlapping
intervals cannot simply be summed as disjoint work. See [FORMAT.md](FORMAT.md#lifecycle-and-timing).

## Error Handling and Shutdown

| Failure | When observed / effect |
| --- | --- |
| Payload serialization | `ObservationError::Serialize` before dispatch; no target receives that attempt |
| Synchronous Sink error | `ObservationError::Sink` from emission/finish; all fan-out targets are attempted, first error returned |
| JSONL worker open/encoding/write/flush error or caught writer panic | Sticky first failure; writer stops, no automatic retry; `last_error()` and subsequent write/flush calls report it |
| Console worker write/flush error or caught panic | Sticky first failure, no retry; pending barriers fail, healthy fan-out still attempted |
| JSONL/Console enqueue racing with worker failure | Enqueue may have returned success; explicit barrier detects failure |
| Scope/Span/Sink Drop | Cannot return errors; use explicit finish/flush where errors must be handled |

`sink_error_count()` counts checked failed engine dispatch/flush **operations**, not
individual failing Sinks or all background failures at the moment they happen.
Serialization failures do not increment it. A Span end failure during Drop increments
the counter through dispatch, but Drop cannot return it. For asynchronous end records,
even successful `finish()` is not completed persistence: flush the destination afterward.

Recommended application shutdown order:

1. Stop accepting new work; finish active Spans and join producers.
2. Flush every relevant destination, including retained detached Sinks.
3. Check/report emission **and** barrier errors, then exit according to application policy.

A helper for the global engine after producers are quiescent:

```rust
use sun_observations::{global, ObservationError};

fn shutdown(work_result: Result<(), ObservationError>) -> Result<(), ObservationError> {
    let persistence_result = global().flush_channels();
    if let Err(error) = &work_result {
        eprintln!("Observation emission failed: {error}");
    }
    if let Err(error) = &persistence_result {
        eprintln!("Observation persistence failed: {error}");
    }
    work_result?;
    persistence_result
}
```

The barrier must execute even if work returned an error; blindly propagating that
error before cleanup skips the barrier. There is no transactional multi-Sink rollback,
automatic retry, or corruption repair. Partial/failed I/O or termination can leave a
torn final line; readers must report malformed history and choose an explicit recovery
policy. Do not silently invent missing Span ends or retry into a second destination
without an application-defined duplication policy.

## Persisted Format

[FORMAT.md](FORMAT.md) is the **canonical v1 compatibility contract**. JSONL is UTF-8
without BOM: one compact JSON object followed by LF, no headers, wrapper array, or
blank lines. The following point record is pretty-printed only for readability;
the file stores it on one physical line:

```json
{
  "ts": "2026-10-20T12:34:56.123Z",
  "proc": "opaque-engine-instance-42",
  "proc_ms": 1482,
  "scope": "training-run-42",
  "namespace": "encoder::train::epoch",
  "level": "INFO",
  "data": {
    "epoch": 17,
    "loss": 0.031
  }
}
```

| Field | Presence | Meaning |
| --- | --- | --- |
| `ts` | Required | UTC RFC 3339 wall time, exactly milliseconds and `Z`; correlation, not elapsed timing |
| `proc` | Required | Opaque engine/process-instance identity; not simply an OS PID |
| `proc_ms` | Required | Monotonic elapsed milliseconds since engine initialization, truncated; not OS process launch |
| `namespace` | Required | Explicit stable semantic identity |
| `level` | Required | Exactly `INFO`, `WARN`, or `ERROR` |
| `data` | Required | Any Serde JSON value, usually an object; null for a Span end |
| `scope` | Optional | Execution identity captured at admission; omitted outside a Scope |
| `message` | Optional | Human-readable supplement from the Observation trait callback |
| `phase`, `span` | Optional pair | `start`/`end` and opaque lifecycle identity |

Optional fields are omitted, not null. Payload null is allowed; non-finite floats follow
`serde_json` behavior (null). **No tags, channel, channels, Rust type name, sequence
number, or implicit version field is added to the envelope.** User payloads may contain
similarly named fields inside `data`. Readers should tolerate unknown additional
envelope members; object member order is not semantic.

Timestamps are captured during emission, not persistence, so they do not measure
queue delay. Queue order defines per-Sink delivery order, with per-producer order
preserved; there is no global thread order or cross-Sink total ordering. Concurrent
timestamps can tie or appear out of order. Retention, duplication, failures, and
unmatched lifecycle records are part of the reader contract, not automatically
corrected by the runtime.

## Benchmarks

These are **measured reference-machine results, not universal performance guarantees**.
[BENCHMARKS.md](BENCHMARKS.md#environment-and-reproduction) records the AMD Ryzen 9
5950X (16 cores/32 logical CPUs), 64 GiB RAM, Windows 11/NTFS, MSVC target, Rust/Cargo
1.96.0, default optimized profile, and timing boundaries. The host was not isolated or
pinned. Filesystem measurements use buffered writes/OS caching, **not fsync**.

Charts 1–4 use the principal report's three-sample medians and small Event payload.
Those runs precede the bounded-writer follow-up; chart 5 shows that paired follow-up
separately. Do not treat results from different harnesses/runs as one normalized sample.
Full reports include min–max ranges, cold resolution, short-lived threads, and retention
costs not captured in these headline views.

### Warm hot-path latency

![Warm hot-path latency in nanoseconds per macro call](img/benchmark-hot-path.svg)

Inactive, excluded, and Never observations measured **7.28–7.37 ns/call**.
EveryN(100) measured 9.55 ns averaged over all calls, including the 99% rejected;
it is not the latency of each accepted record. Always/no-Sink measured 154.45 ns
and still serializes the payload. Payload complexity matters.
[Method and results](BENCHMARKS.md#warm-hot-path).

### Selector scale

![Warm rejected and admitted costs across selector counts with separate panel scales](img/benchmark-selector-scale.svg)

Each N means N irrelevant positive namespaces, tags, excluded namespaces, and excluded
tags: **4N rules**. With fixed Observation depth/tag count, warm rejected cost was
7.30 ns at N=10 and 7.35 ns at N=10,000; warm admitted/no-Sink cost stayed around
153–156 ns. This is evidence against linear warm degradation in this workload,
not a promise about unlimited interleaved types, cold resolution, or arbitrary taxonomy.
The two panels have separate scales.
[Method and cold costs](BENCHMARKS.md#selector-scale-no-linear-warm-degradation).

### JSONL producer versus end-to-end scaling

![Aggregate JSONL producer and end-to-end throughput across one to sixteen producers](img/benchmark-jsonl-scaling.svg)

Producer throughput includes admission, payload/envelope materialization, and enqueue;
end-to-end adds `flush_channels()`. Throughput is derived as `1000 / aggregate ns/call`
in million calls/s, not an additional measurement. With 16 producers, the measured
136.78 ns/call producer cost versus 888.42 ns/call end-to-end shows substantial
backlog: more producers do not create more writers for one Sink. **Enqueue throughput
is not file persistence throughput.**
[Timing boundaries and thread results](BENCHMARKS.md#thread-scaling-1m-total-shared-parent-scope).

### Sink comparison

![End-to-end accepted-observation costs by sink topology](img/benchmark-sink-comparison.svg)

For one producer, no Sink cost 157.95 ns/call, MemorySink 630.22, JSONL 678.93,
and Memory+JSONL 783.08. The fan-out case is four MemorySinks across two Channels,
not four file writers. Destination choice is a major part of accepted-record cost;
JSONL's near-equal producer/end-to-end timings here mean the worker largely kept up
in **this** single-producer workload. They do not make JSONL synchronous.
[Topology and results](BENCHMARKS.md#sink-comparison-one-producer).

### Bounded writer batch tradeoff

![Before and 8192-message writer chunk end-to-end costs for Event and Rich payloads](img/benchmark-writer-batch.svg)

The production follow-up compares the previous unbounded **writer chunk** with the
current 8192-message bound; the producer queue is unbounded in both. Medians cover
ten samples for each before/after case with 262,144 records. Saturated 4/16-producer
end-to-end costs improved **6.4–12.4%**, while single-producer Rich **regressed 9.3%**
(1022.26 to 1116.84 ns/record). Single-producer Event increased 1.1%, within observed
run variation. This is **not a universal speedup** or a payload-byte/latency bound.
[Paired measurements, alternative caps, and limitations](BATCH_BENCHMARKS.md).

### Human rendering and structured-path control

![Same-machine structured before and after medians](img/benchmark-human-structured.svg)

![Console default producer and through-flush service by producer count](img/benchmark-console-scaling.svg)

![Default versus custom row formatting Console costs](img/benchmark-console-rendering.svg)

The five-sample follow-up keeps the historical datasets above separate. Structured
end-to-end medians changed by −2.8% no-sink, −3.3% Memory, +0.8% JSONL, and −14.9%
Memory+JSONL, with overlapping ranges: no material regression is demonstrated. Improvements
are not isolated causal speedups. Console's file-backed default output measured 782 ns/call
with one producer; custom five-column rows measured 1606 ns/call including rendering.
At 16 producers default enqueue was 157 ns/call versus 816 ns/call through flush,
exposing backlog rather than promising terminal throughput. Real terminal rendering can be much slower.
See [human benchmark methodology, raw data and limits](BENCHMARKS.md#human-rendering-follow-up).

### Reproduce the graphics, not a new benchmark dataset

```sh
python scripts/render_benchmark_charts.py
python scripts/render_benchmark_charts.py --check
```

The renderer needs Python 3.9+ and only the standard library, runs offline, and emits
deterministic SVGs. [benchmark_metrics.csv](scripts/benchmark_metrics.csv) traces every
value to its report, section, row, and column. The script validates those values
against the tracked Markdown evidence before rendering; it does not estimate missing
points or rerun benchmarks. `--check` also byte-compares generated output with the
checked-in assets. See [Development](#development-and-release-checks) to intentionally
collect a new machine-specific dataset instead.

## Examples

All examples live in [examples/](examples/) and use the public API.

| Example | What it teaches |
| --- | --- |
| [human.rs](examples/human.rs) | Console stdout/stderr, default output and independent table header/row/footer types |
| [basic.rs](examples/basic.rs) | Derived point Observation, global activation, stdout JSONL, explicit flush |
| [scoped.rs](examples/scoped.rs) | Scope-local FirstN shared with a child thread and an RAII Span captured in memory |
| [channels.rs](examples/channels.rs) | Overlapping routes, multi-Sink Channel, Memory/stdout fan-out |
| [jsonl.rs](examples/jsonl.rs) | Quick Start, retained file configuration, explicit error barrier and writer stats |
| [spans.rs](examples/spans.rs) | Custom message, one Policy admission, start/end pairing and explicit finish after scope exit |
| [threads.rs](examples/threads.rs) | Plain-thread isolation, transitive explicit propagation, captured lifetime and shared budget |
| [rayon.rs](examples/rayon.rs) | Construction-bound pool reused by another caller, nested Rayon work and shared cap |

```sh
cargo run -p sun-observations --example human
cargo run -p sun-observations --example basic
cargo run -p sun-observations --example scoped
cargo run -p sun-observations --example channels
cargo run -p sun-observations --example jsonl
cargo run -p sun-observations --example spans
cargo run -p sun-observations --example threads
cargo run -p sun-observations --features rayon --example rayon
```

`jsonl` appends to `observations.jsonl` in the working directory. The memory examples
assert their captures; `basic` and `channels` emit JSONL to stdout. Cargo excludes the
Rayon example without its required feature.

## Production Usage Patterns

These are illustrative configurations, **not framework defaults**. Engine defaults
activate nothing. Configure routing separately for every pattern.

### Normal production

Adopt a small high-level subtree and explicitly veto costly detail:

```rust
use sun_observations::{global, Selection};
global().set_selection(
    Selection::new()
        .activate_namespace("service")
        .exclude_namespace("service::wire")
        .exclude_tags(["diagnostic", "profiling", "expensive"]),
);
```

If a WARN/ERROR type has an excluded tag, it is excluded too. Design the taxonomy so
required production signals are selected; severity is not a bypass switch.

### Deep diagnostics

Replace Selection deliberately for a diagnostic session; a scope-only activation
cannot override a global exclusion left in place:

```rust
use sun_observations::{global, start_scope, ScopeConfig, Selection};
global().set_selection(Selection::new().activate_namespace("service"));
let _diagnostic = start_scope(
    ScopeConfig::new("diagnostic-request-42")
        .activate_namespace("encoder::train")
        .activate_tags(["diagnostic", "profiling"])
        .exclude_tags(["low_level"]),
);
```

The tag positives apply across namespaces. Add per-type EveryN/FirstN overrides to
that Scope for high-rate expensive observations. Remember nested Scopes are fresh
configurations rather than inherited diagnostic sessions.

### Test/certification capture

Use an isolated engine, targeted Selection, and `MemorySink` to make capture ownership
explicit without changing the global engine:

```rust
use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    start_scope, MemorySink, Observation, ObservationEngine, ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "jobs::completed", tags("certification"))]
struct Completed { count: usize }

let engine = ObservationEngine::new();
let memory = Arc::new(MemorySink::new());
engine.add_channel_sink("capture", memory.clone());
engine.register_namespace_channel("jobs", "capture")?;
let _scope = start_scope(ScopeConfig::new("test-job").activate_tags(["certification"]));
engine.observe_if_active(|| Completed { count: 10 })?;
assert_eq!(memory.shared_records()[0].data["count"], 10);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use a dedicated JSONL file plus a checked barrier when the persisted artifact itself
is under test. A local engine still uses the installed Context; use
`Context::default().run(...)` when isolation from an existing caller Scope is required.

## Performance Guidelines

- Keep expensive work inside lazy payload expressions. Disabled calls cannot undo work
  already performed by the application.
- Human rendering is dormant on structured-only routes. When requested, custom rendering
  executes once on the producer; Console default formatting/output runs on its worker.
- Accepted payload serialization is real CPU/allocation work even without Sinks.
  Prefer focused payloads and scope-local occurrence Policies for high-rate diagnostics.
- Activate deep profiling selectively. Broad tag activation is an OR across the taxonomy.
- `MemorySink` is convenient capture, not unlimited high-concurrency storage; drain it
  and account for contention and retained payload memory.
- One `JsonlSink` has one writer service stream. Multiple independent Sink files can
  provide independent writer capacity, but sharing one physical path is unsupported.
- If producers sustainably outrun the writer, the unbounded queue grows. Reduce volume,
  use Policies, or provision storage; the 8192-message chunk is not backpressure.
- Frequent tiny-file retention can dominate persistence time. Choose size and trim
  hysteresis for the workload; benchmark representative payloads and storage.
- Reuse execution boundaries/pools sensibly. OS-thread creation and strict shared Policy
  counters have costs distinct from warm inactive instrumentation.

### Architecture notes for advanced readers

The runtime reuses resolved scope/type plans and a small thread-local fast cache;
configuration generations invalidate future decisions. This explains why recurring
warm calls need not rescan all configured rules, without promising fixed latency for
arbitrary interleaved types or cold calls. Delivered records are immutable shared
allocations. JSONL keeps encoding/file work on its single background worker and bounds
the worker's chunk working set, not queue memory. No cache knowledge is needed for
correct application usage.

## Production Certification

[CERTIFICATION.md](CERTIFICATION.md) is the canonical executable matrix and release
gate. It covers namespace/derive contracts, admission/laziness/exclusions, exact
scope Policies, configuration invalidation, Scope isolation, OS-thread and optional
Rayon propagation, Channel fan-out, Span lifecycle, JSONL framing/FIFO, multi-producer
integrity, barriers, sticky failures, retention, explicit process shutdown, FORMAT v1,
public end-to-end scenarios, and downstream renamed-dependency compilation. Human
rendering adds laziness/isolation, shared delivery, table streams, routing invalidation,
and Console worker FIFO/barriers/failure/drain/stdout/stderr process checks.

Owner-adjacent certifications run in the runtime test harness. The
`sun-observations-certifications` workspace package adds public cross-module and
isolated-process/downstream checks. Global-engine scenarios run in fresh child
processes rather than relying on singleton resets. The Rayon release gate requires
all features. Benchmark measurements are separate evidence, not performance-threshold
certification or a guarantee for every host/workload.

## Guarantees and Non-Guarantees

**Provided contracts**, subject to successful serialization/I/O and correct wiring:

- Lazy rejected payloads; union activation with exclusion vetoes at every level.
- Scope/type-owned occurrence budgets shared by explicitly propagated children.
- Segment-aware namespace routes and documented fan-out multiplicity.
- Span admission once with frozen Scope/destinations and normal-unwind RAII end.
- JSONL and Console per-Sink FIFO and per-producer integrity; explicit queue-prefix flush barriers.
- No human rendering for structured-only routes; at most one render for multi-human fan-out.
- Complete-record newest-suffix retention, preserving the newest record whole.
- A stable documented v1 envelope independent of Rust type names and Channel names.

**Not guaranteed**:

- fsync/power-loss durability, crash recovery, or persistence of unflushed global statics;
- bounded producer backlog, backpressure, arbitrary payload-byte or real-time latency bounds;
- coordination of independent writers/processes on one file;
- transactional multi-Sink rollback/retry, cross-Sink total order, or automatic deduplication;
- complete Span pairs after abort, leaks, I/O failure, termination, or retention;
- distributed tracing, parent-span graphs, or automatic async task-local propagation;
- automatic Context inheritance in arbitrary OS threads or external executors;
- universal benchmark speedups or a guaranteed throughput on your hardware.

See [certification limits](CERTIFICATION.md#findings-and-limits) and
[FORMAT.md](FORMAT.md) for the precise boundaries.

## Public API Surface

This is a navigation map, not a replacement for source-generated Rust API docs.

| Item | Role |
| --- | --- |
| `Observation` trait / derive | Static metadata, optional message/render_text, and canonical Serde payload |
| `ObservationEngine` | Owns Selection, routing, identity/clocks, emission methods and flush barrier |
| `Selection` | Builds positive/excluded namespace/tag rules |
| `ScopeConfig` | Names an execution and configures its local Selection/Policy overrides |
| `start_scope` | Installs a fresh Scope and returns a restoring guard |
| `Context` / `ContextGuard` | Capture/install execution state and restore it on guard Drop |
| `Policy` | Always/Never/FirstN/EveryN/MaxPerScope occurrence control |
| `ObservationLevel` | INFO/WARN/ERROR severity |
| `Channel` | Named routing destination; graph mutation is through the engine |
| `Sink` | Structured interface plus optional human capability/delivery and flush |
| `ObservationDelivery` | Stack wrapper sharing canonical record and optional transient text |
| `ConsoleSink` | Lazy asynchronous stdout/stderr or owned human writer |
| `MemorySink` | Synchronized in-process capture and drain |
| `JsonlSink` / `JsonlConfig` / `JsonlStats` | Background JSONL consumer, configuration, and diagnostics |
| `ObservationRecord` / `Phase` | Persisted envelope and start/end lifecycle phase |
| `observe!` / `observe_if_active` | Lazy point emission via the global engine |
| `observe_span!` / `span_if_active` / `SpanGuard` | Lazy RAII lifecycle via the global engine |
| `ObservationError` | Serialization versus checked Sink errors |
| `global` | Access the process-wide engine |
| `context::thread::spawn` | Explicit Context-capturing OS-thread boundary |
| `context::rayon::ContextThreadPool` | Optional construction-bound Rayon pool (`rayon` feature) |
| `namespace::validate_namespace` / `namespace::InvalidNamespace` | Runtime namespace validation and route error |

## Development and Release Checks

Repository layout: runtime in `crates/sun-observations`, derive in
`crates/sun-observations-derive`, public/process checks in `certifications`, and
executable consumers in `examples`. Tests stay adjacent to their semantic owners.
The README is included in runtime rustdoc, so its Rust code blocks are doctests;
feature-dependent code must also be checked with all features.

From the workspace root:

```sh
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-features --all-targets
cargo test --workspace
cargo test --workspace --all-features
cargo test -p sun-observations --all-features --lib certification
cargo test -p sun-observations-certifications
cargo test -p sun-observations-certifications --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p sun-observations --examples
cargo build -p sun-observations --all-features --examples
python scripts/render_benchmark_charts.py
python scripts/render_benchmark_charts.py --check
```

Run the example binaries as listed [above](#examples), not only their build checks.
See [CERTIFICATION.md](CERTIFICATION.md#required-release-commands) for the release
matrix and compile-pass/fail/downstream requirements.

To intentionally collect new benchmark evidence:

```sh
cargo bench -p sun-observations --bench runtime
cargo bench -p sun-observations --bench batch
cargo bench -p sun-observations --bench human
```

All benchmark targets save `metrics.csv` and `stderr.txt` in unique run directories
beside the active Cargo profile directory, replay output, and remove temporary
scenario JSONL files. Failed runs keep diagnostic reports and propagate failure.
See [artifact layout and timing controls](BENCHMARKS.md#generated-artifacts) and the
[batch method](BATCH_BENCHMARKS.md#method). Record host/toolchain/workload context and
update measured report tables and chart data together when intentionally refreshing
the dataset; chart regeneration alone must not replace reference results with a new
machine's measurements.
