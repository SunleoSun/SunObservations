# Deliver and retain production evidence

Focused user guide for operational boundaries. Begin with the
[runtime examples](../README.md#observations) for selection, scoping and propagation.
Use [capture](CAPTURE.md) for in-process semantic proofs; configure sinks when
actual delivery or persisted artifacts are the intended effect.

## Configure the right boundary

Selection controls whether, Scope which execution, routing where, Policy how often.
Configure the global engine at application startup and emit with typed macros in
ordinary production functions. Independent engines are useful for delivery/isolation
certifications, not a dependency to inject into every helper.

A normal production selection might activate `service` and exclude expensive
facets. A diagnostic scope can add positives but cannot undo global exclusions:
replace global selection deliberately when changing diagnostic mode. Tags are OR
activation facets, not an AND filter. WARN/ERROR still pass selection.

Reconfiguration affects future calls, invalidates cached plans and preserves scope
counters. It is not retroactive delivery or a global stop-the-world cutover. Quiesce
producers for an exact application boundary. Already admitted spans retain their
start-time scope and sink snapshot even after routes/sinks are changed.

## Memory history and snapshots

MemorySink synchronously retains shared records in delivery order. Defaults are
1,000,000 records and 10% oldest-batch eviction. Configure with
a positive capacity and a finite fraction in (0,1]:

~~~rust
use sun_observations::{MemorySink, MemorySinkConfig};
let sink = MemorySink::with_config(MemorySinkConfig::new(10_000, 0.10)?);
let retained = sink.shared_records();
let removed = sink.drain_shared();
let stats = sink.stats();
# let _ = (retained, removed, stats);
# Ok::<(), std::io::Error>(())
~~~

At capacity, evict ceil(capacity*fraction) oldest before append. The capacity bounds
handles in the sink, not bytes/RSS: payload size and held snapshots matter.
shared_records/drain_shared preserve Arc identity; records/drain make owned copies.
Draining changes membership, not old snapshots or lifetime eviction counters.
Append is synchronized and can contend at high concurrency. A snapshot alone never
establishes complete evidence; scoped proofs must start with run.certify.

## Asynchronous delivery and barriers

One JsonlSink or ConsoleSink instance owns one lazy worker and an **unbounded**
FIFO queue. Share an Arc per destination. First write starts the worker; unused
construction/registration/flush starts no worker, and JSONL file opening is lazy.
Constructor success validates settings, not permissions or file existence.

Producers construct/serialize payloads and materialize records. JSONL encoding/file
I/O runs on the writer. Custom human rendering, if requested, runs once on the
producer; Console fallback formatting and output run on its worker. Structured
sinks never request human rendering. Rendering/payload/message panics propagate;
they are not I/O errors. Spans never invoke point render_text.

A successful observe! means admitted/dispatched, not completed async output.
engine.flush_channels() flushes unique currently attached sinks, even unrouted ones.
Each async barrier waits for its queued prefix and underlying writer flush; racing
later writes may be excluded. It is **not fsync**, a cross-sink transaction, or a
real-time deadline. Periodic flush is best-effort at batch boundaries, not a substitute
for shutdown barriers. Slow output grows RAM backlog; internal chunk limits do not
bound queue memory, arbitrary payload bytes or latency.

Import Sink to call retained_handle.flush() after detachment. Active spans/old plans
can still retain a detached sink. Last-owner Drop drains/flushes/joins and may block,
but cannot return errors; dropping one Arc need not be last-owner Drop. Global
statics are not automatically dropped on process exit.

## File retention and framing

JsonlSink::append(path) has unlimited append; with_config(path, config) adds settings.
JsonlSink::new(writer) / writer_with_config accept owned Write + Send + 'static
writers on the same asynchronous path, but reject file-retention limits.

Default periodic flush is 1000 ms; retention is disabled by default. file_size_mb
means MiB; max_file_size_bytes specifies exact bytes. Positive intervals/limits and
finite trim_fraction strictly between 0 and 1 are required. Ensure parent directories
exist and permissions allow replacement as well as append.

When a size limit is exceeded, trim oldest complete lines toward
max_bytes*(1-trim_fraction), keeping the newest whole record even if oversized.
The limit is approximate during batches; the retained suffix can undershoot target
by a line. Either span half may be removed. Before appending with retention, existing
history is validated as complete JSON object lines; malformed history fails closed
without silent repair. Unlimited append does not retrospectively validate old data.

Independent sink instances/processes writing or compacting the same path are not
coordinated. No multiprocess locking, crash recovery or corruption repair is promised.
Failed I/O/termination may leave a torn final line: readers must report it and choose
an explicit recovery policy. Precise envelope, ordering and lifecycle semantics live
in [FORMAT v1](FORMAT.md).

## Observe errors and shut down

| Failure | Required handling |
| --- | --- |
| Serialize failure | observe!/span start returns ObservationError::Serialize before dispatch |
| Synchronous sink failure | Engine attempts all targets, returns first sink error; no rollback |
| Async open/encode/write/flush or caught worker panic | Sticky first failure, no retry; inspect last_error and explicit barrier |
| Async enqueue racing with worker failure | Enqueue may succeed; barrier must still detect failure |
| Span/Sink Drop failure | Cannot return errors; explicitly finish/flush promised effects |

sink_error_count counts surfaced checked sink-error operations, not exact lost
records or every background failure at its occurrence. stats/records_written are
service diagnostics, not fsync receipts: retention may remove counted records.

Shutdown protocol:

1. Stop accepting new work; finish spans and join producers.
2. Flush every relevant attached or retained detached sink.
3. Report emission **and** persistence errors; apply application exit policy.

~~~rust
use sun_observations::{global, ObservationError};
fn shutdown(work: Result<(), ObservationError>) -> Result<(), ObservationError> {
    let persistence = global().flush_channels();
    if let Err(error) = &work { eprintln!("emission: {error}"); }
    if let Err(error) = &persistence { eprintln!("persistence: {error}"); }
    work?;
    persistence
}
~~~

Do not propagate the work error before evaluating cleanup. Do not invent missing
span ends, silently retry partial output into another destination, or use sleep as
a persistence proof. Application-defined recovery must account for duplication.

## Add a custom destination

Implement Sink: Send + Sync + 'static; callbacks can be concurrent and run without
engine configuration locks. Own synchronization and durability semantics in the sink.
For example, count deliveries without cloning records:

~~~rust
use std::{io, sync::atomic::{AtomicU64, Ordering}};
use sun_observations::{ObservationRecord, Sink};
#[derive(Default)]
struct DeliveryCounter(AtomicU64);
impl Sink for DeliveryCounter {
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
~~~

For retained/asynchronous ownership, override write_shared(Arc<ObservationRecord>)
to avoid deep cloning. Implement flush as your queue-prefix barrier (default: no-op).
Human consumers opt into wants_render_text and write_delivery; capability must remain
fixed while attached. ObservationDelivery clones share record and optional text;
None requires the consumer's fallback. Custom human text is literal, not sanitized,
and never changes structured data/message. See [human.rs](../examples/human.rs) for
table streams and the [evidence API manual](../crates/sun-observations/agent-skills/sun-observations/SKILL.md)
for exact extension signatures.
