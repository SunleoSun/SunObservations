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
    engine.channel("training").add_sink(memory.clone());
    engine.channel("training").add_sink(Arc::new(JsonlSink::new(std::io::stdout())));
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
    engine.flush()?;
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
An admitted observation without any routed sinks is still constructed and serialized.

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

## Channels and sinks

Namespace → Channel → Sink is routing, independent from selection. Multiple matching
subtree routes fan out to multiple channels, each containing any number of `Arc<dyn Sink>`.
Routes to the same channel are deduplicated, including overlapping roots. A shared sink
attached to two distinct matching channels receives two writes by design. A channel is
never restricted to one file, and neither channel names nor metadata tags enter records.

`MemorySink` offers synchronized `records()` snapshots, `drain()`, `len()` and
`is_empty()`. `JsonlSink::append(path)` creates/appends without truncation;
`JsonlSink::new(writer)` accepts any owned `Write + Send + 'static` (including a
buffered writer). Share an `Arc<JsonlSink>` to synchronize a destination. A single lock
covers each full line; independent sink instances/processes targeting the same file are
not coordinated. `flush()` drains writers but is not a disk durability/fsync guarantee.
See [FORMAT.md](FORMAT.md) for framing, fields and lifecycle interpretation.

Writes are synchronous and can block. Engine/channel configuration locks are released
before payload construction or sink callbacks. All fan-out targets are attempted even
if one fails; the first error is returned. `ObservationError` distinguishes serialization
and sink errors. `engine.sink_error_count()` counts failed dispatch/flush operations
(including span Drop); it is not a count of individual failed sinks. No transactional
multi-sink guarantee, retries, background buffering, file rotation or error callback exists.
A failed write can leave partial external output; applications choose their error policy.
Flush explicitly before consuming buffered output or shutdown.

Independent `ObservationEngine::new()` instances support isolated embedding/testing
through their closure methods, with their own configuration, identities and clocks. Public
macros intentionally always use `global()`, never a hidden per-test engine override.

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
errors; they increment the engine counter. Aborts, leaks, process termination and I/O
failures can leave unmatched spans. See FORMAT.md before aggregating overlapping timings.

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
