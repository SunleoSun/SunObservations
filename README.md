# SunObservations

A domain-independent Rust observation engine for structured runtime logging,
diagnostics and profiling. No logger, recorder or scope arguments in domain calls.

## Workspace

- `crates/sun-observations`: runtime, public macros, sinks and re-exported derive.
- `crates/sun-observations-derive`: compile-time metadata validation/code generation.
- `examples/`: executable downstream-style examples.
- `FORMAT.md`: canonical persisted JSONL contract, independent of Rust API layout.
- Owner-adjacent `*_test.rs`: unit/integration-style tests and derive fixtures.
- `*_certification_test.rs` and `certifications/`: **empty future placeholders**,
  not an implemented or passing certification suite.

Consumers need only the runtime crate plus Serde:

```toml
[dependencies]
sun-observations = { path = "../SunObservations/crates/sun-observations" }
serde = { version = "1", features = ["derive"] }
```

## Quick start

This complete example declares typed metadata, activates a scope, routes a namespace
subtree to a logical channel, and captures the same record in two sinks:

```rust
use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    global, observe, start_scope, JsonlSink, MemorySink,
    Observation, ObservationError, Policy, ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(
    namespace = "compute::batch",
    level = INFO,
    tags("detail", "expensive")
)]
struct Batch { index: usize, norm: f64 }

fn expensive_norm() -> f64 { 0.125 }

// Domain methods do not accept observation infrastructure.
fn work(index: usize) -> Result<(), ObservationError> {
    observe!(Batch { index, norm: expensive_norm() })?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemorySink::new());
    let engine = global();
    engine.add_channel_sink("training", memory.clone());
    engine.add_channel_sink("training", Arc::new(JsonlSink::new(std::io::stdout())));
    engine.register_namespace_channel("compute", "training")?;

    let scope = start_scope(
        ScopeConfig::new("run-a")
            .activate_namespace("compute")
            .activate_tags(["detail"])
            .override_policy::<Batch>(Policy::EveryN(2)),
    );
    work(1)?; // Policy rejects: expensive_norm() is NOT called.
    work(2)?; // Admitted once, delivered to both sinks.
    drop(scope);
    work(3)?; // No selectors outside the scope: still lazy.
    assert_eq!(memory.len(), 1);
    engine.flush_channels()?;
    Ok(())
}
```

Run `cargo run -p sun-observations --example basic`, `--example scoped` or
`--example channels` for further complete examples.

## Metadata, admission and policies

`Observation` requires `Serialize + 'static`; generic serializable owned types
are supported. Deriving it never requires type registration. Namespace is required,
explicit, and **not derived from the Rust type name**. Renaming the struct does not
change its semantic identity. Segments match ASCII `[a-z][a-z0-9_]*`, separated by
`::`, without empty segments. Tags use the same single-segment grammar. Invalid,
missing, duplicate or unknown metadata is rejected at compile time.

Only `INFO` (default), `WARN` and `ERROR` exist. Level is severity, not verbosity;
WARN/ERROR do not bypass selection. Use namespaces/tags/policies for instrumentation
detail, not DEBUG/TRACE. Tags are static type metadata used at runtime, never envelope
fields. An optional `Observation::message()` supplies the envelope message; ordinary
payload fields, including any field named `message`, remain inside `data`.

Admission is evaluated **before construction and serialization**:

1. Positive match: global namespace **OR** global tag **OR** current-scope namespace
   **OR** current-scope tag. Any one matching tag suffices.
2. Any global or scope exclusion (namespace subtree or tag) vetoes that match.
3. Apply the scope override for this type; otherwise use `Policy::Always`.

A namespace selector `compute::train` matches itself and `compute::train::epoch`,
not `compute::trainer`. No wildcard syntax is needed. Empty selection disables
all observations. `Selection::activate_all()` / `ScopeConfig::activate_all()`
explicitly selects everything; exclusions still win. Runtime selector builders panic
on invalid namespaces/tags; route registration instead returns `InvalidNamespace`.

`global().set_selection(Selection::new().activate_namespace("compute"))` replaces
process-wide selection atomically. Routing alone does **not** activate observations.
An admitted observation without any routed sinks is still constructed and serialized;
only its unobservable output envelope is skipped.

| Policy | Selected occurrences admitted |
| --- | --- |
| Always | All (default, no registration required) |
| Never | None |
| FirstN(n) | First n |
| EveryN(n) | n, 2n, 3n, ... (one-based) |
| MaxPerScope(n) | First n; a scope-local cap, equivalent to FirstN in v1 |

All numeric policies with zero reject everything. Counters are keyed by concrete Rust
type (not namespace) in each scope instance. Rejected selection does not advance them;
an admission attempt consumes its occurrence even if later serialization/I/O fails.
Children inheriting a context share that scope's counters safely. Two starts, even with
the same scope name, have independent counters. Independent types do not share a budget.
Outside a scope, selected observations use `Policy::Always`; policy overrides are
scope-local. A scope override never changes an unscoped or another scope's policy.

`observe!(expression)` returns `Result<bool, ObservationError>`: false means rejected,
true means admitted and successfully dispatched (possibly to zero sinks). For closures,
use `observe_if_active::<Type, _>(|| payload)`. A payload already constructed before
the macro cannot be made lazy retroactively.

## Resolved plans and hot-path model

A scope is the canonical owner of shared resolved plans. Child threads/Rayon workers
inherit the same scope state and can reuse the parent's resolution. Each plan freezes
activation/exclusion results, effective policy, counter handle and immutable sink snapshot.
Changing engine selection, routes or attachments increments one atomic configuration
generation; the next call re-resolves a stale plan without resetting scope policy counters.

TLS holds only **four fixed entries**, keyed by engine identity, scope identity, concrete
TypeId and generation. A warm hit performs no general-purpose HashMap lookup, Mutex or
RwLock acquisition, selector/route scan, or plan Arc refcount traffic: a local Rc reference
keeps the shared Arc plan alive across payload callbacks. Rejected warm calls allocate
nothing and never construct/serialize the payload (covered by an allocation-counting test).
Context boundaries clear fast references, not shared scope plans. A new thread allocates
no large TLS table; a first type miss creates one small local reference.

Cache misses/evictions are deliberately cold: they use a shared scope map and configuration
read lock. Indexed namespace prefix sets and tag membership sets avoid scanning all rules;
routing likewise looks up namespace ancestors, then only matching channels. More than four
interleaved concrete types can cause tiny-cache misses, but still reuse shared resolution.
Always/Never need no counter. Counting policies use a relaxed atomic, saturating increment
per admitted selection; shared counters can contend at high thread counts because strict
scope-wide occurrence ordering is intentional. No per-occurrence scope map lock remains.

For admitted observations with no destinations, payload serialization and message callbacks
still execute (including errors/side effects), but an unobservable envelope/timestamp is not
allocated. Delivered records keep the same public ObservationRecord and FORMAT.md schema.

[ BENCHMARKS.md ](BENCHMARKS.md) records actual 1M-call macro workloads, selector scale,
cold/warm paths, enqueue versus end-to-end file throughput, thread scaling and compaction.
Run the complete suite with:

```sh
cargo bench -p sun-observations --bench runtime
```

## Execution-local scopes and propagation

`global()` is one lazily initialized process-wide `ObservationEngine`. Scope
installations are thread-local, **not a mutable global current scope**. Guards restore
the previous context, including during panic unwinding, and cannot move between threads.
Nested scopes are independent executions; they do not inherit the outer scope's selectors,
overrides or counters. The engine's global selectors still apply. Outside any scope,
records omit `scope` rather than inheriting another thread's identity.

Capture/install at execution boundaries, not throughout the domain call graph:

```rust
use sun_observations::{context, start_scope, Context, ScopeConfig};
let scope = start_scope(ScopeConfig::new("parent-a"));
let child = context::thread::spawn(|| {
    // Any observe! calls here use parent-a, including its shared policy counters.
    assert_eq!(Context::capture().scope_name(), Some("parent-a"));
});
child.join().unwrap();
let captured = Context::capture();
drop(scope);
captured.run(|| assert_eq!(Context::capture().scope_name(), Some("parent-a")));
assert_eq!(Context::capture().scope_name(), None);
```

`Context::enter()` is the RAII alternative to `run`. `Context::default()` is
explicitly empty and can mask an unrelated worker scope. Captures keep their scope alive
past the original guard. Arbitrary `std::thread::spawn`, async tasks and external
executors are not automatically intercepted. Do not hold a TLS scope guard across async
suspension/thread migration; task-local async integration is outside v1.

### Optional Rayon integration

Enable `sun-observations = { ..., features = ["rayon"] }` and add `rayon = "1"`
to the consumer if using its parallel iterators. Construct a **dedicated context-bound
pool** once per execution boundary; all its workers inherit the captured context for the
pool lifetime. No changes to nested `par_iter()` callsites are required:

```rust
# fn main() -> Result<(), Box<dyn std::error::Error>> {
#[cfg(feature = "rayon")]
{
    use rayon::prelude::*;
    use sun_observations::{context::rayon::ContextThreadPool, start_scope, Context, ScopeConfig};
    let scope = start_scope(ScopeConfig::new("parallel-run"));
    let pool = ContextThreadPool::new(4)?;
    drop(scope);
    pool.install(|| {
        (0..100).into_par_iter().for_each(|_| {
            assert_eq!(Context::capture().scope_name(), Some("parallel-run"));
        });
    });
    assert_eq!(Context::capture().scope_name(), None);
}
# Ok(())
# }
```

A pool remains bound to the context at **construction**, even when reused by another
caller. It does not mutate the global Rayon pool or install transient global worker state.
Use a new context-bound pool at a new scope boundary. Scopes opened *inside* a pool job
are local to that thread; to propagate those further, create another explicit boundary.
Unrelated parents can execute concurrently on separate pools without context contamination.
Rayon is not a default dependency.

## Channels, sinks and asynchronous persistence

Namespace → Channel → Sink remains independent from selection. All graph mutation goes
through the engine:

- `add_channel_sink(channel, Arc<dyn Sink>)` attaches a consumer;
- `register_namespace_channel(namespace, channel)` only registers a subtree route;
- `clear_channel_sinks(channel)` detaches that channel's consumers.

Each mutation publishes a new configuration generation. There is no external mutable
`channel().add_sink()` handle. Multiple matching channels fan out; each channel can have
many sinks. Overlapping/repeated routes to the same channel are deduplicated. The same
sink attached to two matching channels still receives **two deliveries**, with the same
immutable record allocation. Channel names and metadata tags never enter the envelope.

The engine dispatches `Arc<ObservationRecord>` through `Sink::write_shared()`. Its default
implementation calls the borrowed `write()` method for simple custom sinks. MemorySink and
JsonlSink override it to retain/enqueue the shared allocation without deep cloning data.
MemorySink offers cheap `shared_records()` / `drain_shared()`; `records()` / `drain()`
remain owned compatibility snapshots. A borrowed direct `sink.write(&record)` must clone
when ownership is needed; engine fan-out never uses that convenience path.

### JsonlSink configuration and worker

`JsonlSink` is now asynchronous for **all** destinations; there is no parallel synchronous
file implementation. `append(path)` uses defaults: flush interval 1000 ms, no size limit,
trim fraction 0.10. Configure file behavior when constructing/attaching the sink, not in
namespace route registration:

```rust
use std::sync::Arc;
use sun_observations::{global, JsonlConfig, JsonlSink};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let config = JsonlConfig::new()
    .flush_ms(1000)
    .file_size_mb(10) // MiB: 10 * 1024 * 1024 bytes
    .trim_fraction(0.10);
let file = Arc::new(JsonlSink::with_config("observations.jsonl", config)?);
global().add_channel_sink("training", file);
global().register_namespace_channel("compute::train", "training")?;
global().flush_channels()?;
# Ok(())
# }
```

Registration, construction and flushing an unused sink do **not** open/create a file or
start a thread. The first actual write starts a named background worker. File open errors
therefore surface through the async error model, not necessarily the constructor.
`new(writer)` and `writer_with_config(writer, config)` support owned `Write + Send + 'static`
writers on the same background path; file-size settings are rejected for non-file writers.
`max_file_size_bytes(bytes)` is available for exact byte limits. Zero flush interval/size,
unrepresentable intervals, and non-finite or non-(0,1) trim fractions fail construction.

The producer constructs/serializes the typed payload to Value, captures timestamps, then
enqueues a shared record. It performs **no file open, JSONL encoding, write, flush or
compaction**. The worker blocks for a message, snapshots and drains the queued prefix,
then encodes/writes batches. New arrivals cannot indefinitely starve writes or periodic
flush. Periodic flush runs while idle and under continuous traffic, at batch boundaries;
it is best-effort and can be delayed by a slow I/O operation or large batch.

The queue is deliberately **unbounded**: disk slowdown means backlog grows in memory,
not overflow drops. There is no bounded queue, overflow policy or backpressure guarantee.
A successful observe/enqueue does **not** mean persisted data; failures must be checked
at a flush barrier. File mode buffers writes and appends without truncation unless explicit
retention is enabled. Share one Arc<JsonlSink> per physical destination; independent
instances/processes writing or compacting the same path are not coordinated.

### Flush, shutdown and errors

`global().flush_channels()?` synchronously visits every unique currently attached sink,
including unrouted sinks. Each JSONL queue receives a barrier: successful return means
all records enqueued before that barrier were processed and the underlying writer's
`flush()` completed. Writes concurrent with the call may or may not be included. Sink
locks/configuration locks are not held while waiting. Flush is **not fsync** or a crash
recovery guarantee. Use it before reading JSONL and before deterministic shutdown, without
sleeps/retries. Flush before detaching sinks, or flush a retained sink directly afterward;
a detached sink is no longer included in the engine's current graph barrier.

A last-owner JsonlSink Drop closes its queue, drains accepted messages, flushes and joins
the worker. Independent engine Drop also attempts a channel flush. Cached plans and spans
can extend a sink's lifetime beyond detachment. **Rust process-global statics are not
dropped on main return**: applications using global() must explicitly flush_channels()
before exit. Abort, process termination, leaked owners and I/O failure cannot guarantee
persistence. Drop errors are reported to stderr; explicit barriers return errors to callers.

All fan-out targets are attempted if one fails, with the first synchronous error returned.
The JSONL worker latches its first I/O/encoding error or caught writer panic, stops writing,
and exposes a sticky `last_error()`; subsequent writes/flushes fail. An enqueue that raced
with that failure can have returned Ok, so the final barrier is mandatory for error checking.
No errors are silently cleared and there are no retries or transactional multi-sink writes.
`is_started()` and `stats()` expose lifecycle/batch/flush/compaction diagnostics without
test-only production branches. `engine.sink_error_count()` counts failed dispatch/flush
operations, including when the barrier surfaces a background error, not individual failed
sinks or background failures that have not yet been checked.

### Line-safe file retention

When size exceeds the configured maximum, the worker targets approximately
`max_bytes * (1 - trim_fraction)`. For 10 MiB and 0.10 this is roughly 9 MiB. It seeks to
an approximate oldest-byte cut, scans forward to a newline, copies the complete suffix
to a temporary sibling and replaces the file. No retained first/last record is split;
the newest record is always kept whole, even if it alone exceeds the maximum. A normal
boundary cut can undershoot the target by one line; later appends grow within the hysteresis
window. Size is approximate during a batch. Retention intentionally removes oldest records,
possibly span starts whose ends remain; parsers must handle unmatched lifecycle records.
Replacement/file I/O failure is sticky and reported, not ignored. Small thresholds can be
expensive on filesystems with costly replacement/antivirus hooks; see BENCHMARKS.md.

See [FORMAT.md](FORMAT.md) for unchanged persisted fields, framing and lifecycle meanings.

Independent `ObservationEngine::new()` instances support isolated embedding/testing through
their closure methods, with their own configuration, identities and clocks. Public macros
always use global(), never a hidden per-test engine override.

## Typed RAII spans

```rust
use serde::Serialize;
use sun_observations::{observe_span, start_scope, Observation, ScopeConfig};
#[derive(Serialize, Observation)]
#[observation(namespace = "compute::fit")]
struct Fit { samples: usize }
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let _scope = start_scope(ScopeConfig::new("fit-a").activate_namespace("compute"));
let span = observe_span!(Fit { samples: 128 })?;
// Work runs here. Normal Drop (including unwinding) emits the end record.
span.finish()?; // Explicit finish also reports end I/O errors; never emits twice.
# Ok(())
# }
```

The macro returns `Result<SpanGuard, ObservationError>`. A rejected span returns an
inert guard, never constructs its payload, and writes neither lifecycle record. Active
spans freeze scope and routing at admission, write start with the typed payload, and end
with `data: null`. Both have identical namespace, level, process, scope and span id;
each separate span gets a fresh id. Configuration changes or dropping the scope do not
suppress end. Policy counts **spans**, not their two records. No distributed tracing,
parent-span tree or automatic duration field is modeled. Normal Drop cannot return I/O
errors; synchronous enqueue failures increment the engine counter. Async end-write failures
surface at flush_channels(), not necessarily at finish()/Drop. Aborts, leaks, process
termination, retention and I/O failures can leave unmatched spans. See FORMAT.md before aggregating overlapping timings.

## Validation

```sh
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-features --all-targets
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Compile-pass/fail tests run through `trybuild`; the owner harness is
`observation_test.rs`, with checked-in diagnostic snapshots in adjacent `fixtures/`.
Certification placeholders contain no assertions and are not part of test discovery.
