//! Research only: public API measurements and isolated controls, no production instrumentation.
//! SUN_OBS_INV_REPEATS=7 SUN_OBS_INV_GROUP=micro|serde|writer|api|confirm.
//! Empty group selects main four groups; confirm is a separate order-reversed follow-up.
//! Times include destruction unless stated otherwise.
// Deliberately mirror engine's to_value(&payload), including its borrow.
#![allow(clippy::needless_borrows_for_generic_args)]
mod support;
use chrono::{DateTime, SecondsFormat, Utc};
use crossbeam_channel::{unbounded, Sender};
use serde::Serialize;
use serde_json::{value::to_raw_value, Value};
use std::{
    cell::RefCell,
    fs::File,
    hint::black_box,
    io::{self, BufRead, BufReader, BufWriter, Write},
    sync::{Arc, Barrier},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sun_observations::{
    global, observe, start_scope, Context, JsonlSink, MemorySink, Observation, ObservationLevel,
    ObservationRecord, Phase, ScopeConfig, Selection, Sink,
};

const MICRO: usize = 500_000;
const RECORDS: usize = 300_000;
const PROCESS: &str = "01234567-89ab-4cde-8123-456789abcdef";
const NAMESPACE: &str = "bench::event";
const SCOPE: &str = "benchmark";
const TS: &str = "2026-10-20T12:34:56.789Z";

#[derive(Serialize, Observation)]
#[observation(namespace = "bench::event")]
struct Event {
    index: u64,
    value: f64,
}
#[derive(Serialize, Observation)]
#[observation(namespace = "bench::event")]
struct Unit;
#[derive(Serialize)]
struct MessageEvent {
    index: u64,
    value: f64,
}
impl Observation for MessageEvent {
    const NAMESPACE: &'static str = NAMESPACE;
    #[inline(never)]
    fn message(&self) -> Option<&str> {
        black_box(Some("message"))
    }
}
#[derive(Serialize)]
struct Nested {
    enabled: bool,
    phase: u32,
}
#[derive(Serialize)]
struct Rich {
    index: u64,
    label: &'static str,
    nested: Nested,
    samples: [f64; 16],
    value: f64,
}
fn event(i: usize) -> Event {
    Event {
        index: black_box(i as u64),
        value: 0.5,
    }
}
fn rich(i: usize) -> Rich {
    Rich {
        index: black_box(i as u64),
        label: "a representative string with \"quotes\", \\slashes,\nnewline and UTF-8: наблюдение",
        nested: Nested {
            enabled: true,
            phase: 3,
        },
        samples: [0.125; 16],
        value: 0.5,
    }
}
fn row(name: &str, sample: usize, n: usize, elapsed: Duration, bytes: usize, aux: u64) {
    println!(
        "{name},{sample},{n},{:.3},{bytes},{aux}",
        elapsed.as_nanos() as f64 / n as f64
    );
}
fn timed(mut f: impl FnMut(), n: usize) -> Duration {
    // Warm the exact closure outside the measured interval.
    for _ in 0..1024 {
        f();
    }
    let t = Instant::now();
    for _ in 0..n {
        f();
    }
    t.elapsed()
}
fn bench(name: &str, sample: usize, mut f: impl FnMut()) {
    let elapsed = timed(&mut f, MICRO);
    row(name, sample, MICRO, elapsed, 0, 0);
}
fn fixture() -> ObservationRecord {
    ObservationRecord {
        ts: TS.into(),
        proc: PROCESS.into(),
        proc_ms: 1234,
        scope: Some(SCOPE.into()),
        namespace: NAMESPACE.into(),
        level: ObservationLevel::INFO,
        data: serde_json::to_value(event(123456)).unwrap(),
        message: None,
        phase: None,
        span: None,
    }
}
fn envelope(data: Value, started: Instant) -> ObservationRecord {
    ObservationRecord {
        ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        proc: PROCESS.to_owned(),
        proc_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        scope: Some(SCOPE.to_owned()),
        namespace: NAMESPACE.to_owned(),
        level: ObservationLevel::INFO,
        data,
        message: None,
        phase: None,
        span: None,
    }
}
// Same field order, optional-field omission, escaping and numeric representation as v1.
#[derive(Serialize)]
struct Envelope<'a, D: ?Sized> {
    ts: &'a str,
    proc: &'a str,
    proc_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
    namespace: &'a str,
    level: ObservationLevel,
    data: &'a D,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<&'a str>,
}
fn borrowed<'a, D: ?Sized>(base: &'a ObservationRecord, data: &'a D) -> Envelope<'a, D> {
    Envelope {
        ts: &base.ts,
        proc: &base.proc,
        proc_ms: base.proc_ms,
        scope: base.scope.as_deref(),
        namespace: &base.namespace,
        level: base.level,
        data,
        message: base.message.as_deref(),
        phase: base.phase,
        span: base.span.as_deref(),
    }
}
struct Discard;
impl Sink for Discard {
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn write(&self, r: &ObservationRecord) -> io::Result<()> {
        black_box(r);
        Ok(())
    }
    fn write_shared(&self, r: Arc<ObservationRecord>) -> io::Result<()> {
        black_box(&r);
        Ok(())
    }
}
enum QueueDiscardMessage {
    Record(Arc<ObservationRecord>),
    Flush(Sender<()>),
}
struct QueueDiscard(Sender<QueueDiscardMessage>);
impl Sink for QueueDiscard {
    fn flush(&self) -> std::io::Result<()> {
        let (tx, rx) = unbounded();
        self.0
            .send(QueueDiscardMessage::Flush(tx))
            .map_err(|e| io::Error::other(e.to_string()))?;
        rx.recv().map_err(|e| io::Error::other(e.to_string()))
    }
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        unreachable!()
    }
    fn write_shared(&self, r: Arc<ObservationRecord>) -> io::Result<()> {
        self.0
            .send(QueueDiscardMessage::Record(r))
            .map_err(|e| io::Error::other(e.to_string()))
    }
}
thread_local! {
    // Hypothesis only: reuse formatted UTC text within the same wall-clock millisecond.
    static TS_CACHE: RefCell<Option<(i64, String)>> = const { RefCell::new(None) };
}
fn cached_timestamp(now: DateTime<Utc>) -> String {
    TS_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let millis = now.timestamp_millis();
        if c.as_ref().is_none_or(|(key, _)| *key != millis) {
            *c = Some((millis, now.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        c.as_ref().unwrap().1.clone()
    })
}
fn micro(sample: usize) {
    let now = Utc::now();
    let started = Instant::now();
    let arc = Arc::new(fixture());
    bench("micro.empty_black_box", sample, || {
        black_box(0_u64);
    });
    bench("micro.payload_build", sample, || {
        black_box(event(123456));
    });
    bench("micro.payload_value", sample, || {
        black_box(serde_json::to_value(&event(123456)).unwrap());
    });
    bench("micro.message_default", sample, || {
        black_box(black_box(event(123456)).message());
    });
    bench("micro.message_callback", sample, || {
        black_box(
            black_box(MessageEvent {
                index: 123456,
                value: 0.5,
            })
            .message(),
        );
    });
    bench("micro.payload_value_message", sample, || {
        let p = MessageEvent {
            index: black_box(123456),
            value: 0.5,
        };
        black_box((serde_json::to_value(&p).unwrap(), p.message()));
    });
    bench("micro.utc_now", sample, || {
        black_box(Utc::now());
    });
    bench("micro.utc_numeric_millis", sample, || {
        black_box(Utc::now().timestamp_millis());
    });
    bench("micro.systemtime_numeric", sample, || {
        black_box(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        );
    });
    bench("micro.rfc3339_fixed", sample, || {
        black_box(black_box(now).to_rfc3339_opts(SecondsFormat::Millis, true));
    });
    bench("micro.utc_plus_rfc3339", sample, || {
        black_box(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    });
    bench("micro.utc_plus_cached_rfc3339", sample, || {
        black_box(cached_timestamp(Utc::now()));
    });
    bench("micro.proc_ms", sample, || {
        black_box(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
    });
    bench("micro.envelope_strings", sample, || {
        black_box((
            black_box(PROCESS).to_owned(),
            Some(black_box(SCOPE).to_owned()),
            black_box(NAMESPACE).to_owned(),
        ));
    });
    bench("micro.envelope_null", sample, || {
        black_box(envelope(Value::Null, started));
    });
    bench("micro.envelope_null_arc", sample, || {
        black_box(Arc::new(envelope(Value::Null, started)));
    });
    bench("micro.full_record", sample, || {
        black_box(envelope(
            serde_json::to_value(&event(123456)).unwrap(),
            started,
        ));
    });
    bench("micro.full_record_arc", sample, || {
        black_box(Arc::new(envelope(
            serde_json::to_value(&event(123456)).unwrap(),
            started,
        )));
    });
    bench("micro.arc_clone_drop", sample, || {
        black_box(arc.clone());
    });
    bench("micro.arc_alloc_record_move", sample, || {
        // Stack record has no heap fields in this control: isolates Arc allocation/free.
        black_box(Arc::new(ObservationRecord {
            ts: String::new(),
            proc: String::new(),
            proc_ms: 0,
            scope: None,
            namespace: String::new(),
            level: ObservationLevel::INFO,
            data: Value::Null,
            message: None,
            phase: None,
            span: None,
        }));
    });
    for concurrent in [false, true] {
        let (tx, rx) = unbounded::<u64>();
        let worker_rx = rx.clone();
        let consumer = concurrent.then(|| {
            std::thread::spawn(move || {
                for item in worker_rx {
                    black_box(item);
                }
            })
        });
        let t = Instant::now();
        for i in 0..MICRO {
            tx.send(black_box(i as u64)).unwrap();
        }
        row(
            if concurrent {
                "queue.scalar_send_active"
            } else {
                "queue.scalar_send_backlogged"
            },
            sample,
            MICRO,
            t.elapsed(),
            0,
            0,
        );
        drop(tx);
        if let Some(consumer) = consumer {
            consumer.join().unwrap();
        }
    }
    // Exact equality at millisecond/second boundaries, including negative UNIX time.
    for millis in [-1001, -1000, -1, 0, 999, 1000, 123456789, 1792499696789] {
        let dt = DateTime::from_timestamp_millis(millis).unwrap();
        assert_eq!(
            cached_timestamp(dt),
            dt.to_rfc3339_opts(SecondsFormat::Millis, true)
        );
        assert_eq!(
            cached_timestamp(dt),
            dt.to_rfc3339_opts(SecondsFormat::Millis, true)
        );
    }
}
fn serde_cases<P: Serialize>(name: &str, payload: &P, sample: usize) {
    let base = fixture();
    let value = serde_json::to_value(payload).unwrap();
    let raw = to_raw_value(payload).unwrap();
    let typed = borrowed(&base, payload);
    let value_env = borrowed(&base, &value);
    let raw_env = borrowed(&base, &*raw);
    let expected = serde_json::to_vec(&value_env).unwrap();
    assert_eq!(
        expected,
        serde_json::to_vec(&typed).unwrap(),
        "typed schema"
    );
    assert_eq!(
        expected,
        serde_json::to_vec(&raw_env).unwrap(),
        "raw schema"
    );
    let mut output = Vec::with_capacity(expected.len() * 2);
    eprintln!(
        "schema {name}: payload={} envelope={} bytes",
        raw.get().len(),
        expected.len()
    );
    bench(&format!("serde.{name}.to_value"), sample, || {
        black_box(serde_json::to_value(black_box(payload)).unwrap());
    });
    bench(&format!("serde.{name}.to_raw_alloc"), sample, || {
        black_box(to_raw_value(black_box(payload)).unwrap());
    });
    bench(
        &format!("serde.{name}.value_payload_encode"),
        sample,
        || {
            output.clear();
            serde_json::to_writer(&mut output, black_box(&value)).unwrap();
            black_box(&output);
        },
    );
    bench(
        &format!("serde.{name}.typed_payload_encode_reuse"),
        sample,
        || {
            output.clear();
            serde_json::to_writer(&mut output, black_box(payload)).unwrap();
            black_box(&output);
        },
    );
    bench(
        &format!("serde.{name}.value_envelope_encode"),
        sample,
        || {
            output.clear();
            serde_json::to_writer(&mut output, black_box(&value_env)).unwrap();
            black_box(&output);
        },
    );
    bench(&format!("serde.{name}.raw_envelope_encode"), sample, || {
        output.clear();
        serde_json::to_writer(&mut output, black_box(&raw_env)).unwrap();
        black_box(&output);
    });
    bench(
        &format!("serde.{name}.typed_envelope_encode"),
        sample,
        || {
            output.clear();
            serde_json::to_writer(&mut output, black_box(&typed)).unwrap();
            black_box(&output);
        },
    );
    bench(&format!("serde.{name}.two_stage_full"), sample, || {
        let v = serde_json::to_value(black_box(payload)).unwrap();
        output.clear();
        serde_json::to_writer(&mut output, &borrowed(&base, &v)).unwrap();
        black_box(&output);
    });
    bench(&format!("serde.{name}.raw_full"), sample, || {
        let r = to_raw_value(black_box(payload)).unwrap();
        output.clear();
        serde_json::to_writer(&mut output, &borrowed(&base, &*r)).unwrap();
        black_box(&output);
    });
}
fn verify_file(path: &std::path::Path, expected: usize) {
    let reader = BufReader::new(File::open(path).unwrap());
    let mut count = 0;
    let mut last = Vec::new();
    for line in reader.split(b'\n') {
        let line = line.unwrap();
        assert!(!line.is_empty());
        if count == 0 {
            let _: ObservationRecord = serde_json::from_slice(&line).unwrap();
        }
        last = line;
        count += 1;
    }
    let _: ObservationRecord = serde_json::from_slice(&last).unwrap();
    assert_eq!(count, expected);
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(path).unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    let mut last_byte = [0];
    file.read_exact(&mut last_byte).unwrap();
    assert_eq!(last_byte, [b'\n']);
}
fn api(sample: usize) {
    global().clear_channel_sinks("investigation");
    global().set_selection(Selection::new());
    let scope = start_scope(ScopeConfig::new(SCOPE));
    bench("api.rejected", sample, || {
        black_box(observe!(event(123456)).unwrap());
    });
    global().set_selection(Selection::new().activate_namespace("bench").unwrap());
    bench("api.unit_no_sink", sample, || {
        black_box(observe!(Unit).unwrap());
    });
    bench("api.event_no_sink", sample, || {
        black_box(observe!(event(123456)).unwrap());
    });
    bench("api.message_no_sink", sample, || {
        black_box(
            observe!(MessageEvent {
                index: black_box(123456),
                value: 0.5
            })
            .unwrap(),
        );
    });
    global().add_channel_sink("investigation", Arc::new(Discard));
    bench("api.discard", sample, || {
        black_box(observe!(event(123456)).unwrap());
    });
    global().clear_channel_sinks("investigation");
    let memory = Arc::new(MemorySink::new());
    global().add_channel_sink("investigation", memory.clone());
    bench("api.memory", sample, || {
        black_box(observe!(event(123456)).unwrap());
    });
    assert_eq!(memory.len(), MICRO + 1024);
    global().clear_channel_sinks("investigation");
    drop(memory);
    let (tx, rx) = unbounded();
    let consumer = std::thread::spawn(move || {
        for message in rx {
            match message {
                QueueDiscardMessage::Record(record) => {
                    black_box(record);
                }
                QueueDiscardMessage::Flush(reply) => reply.send(()).unwrap(),
            }
        }
    });
    global().add_channel_sink("investigation", Arc::new(QueueDiscard(tx)));
    bench("api.queue_discard", sample, || {
        black_box(observe!(event(123456)).unwrap());
    });
    global().flush_channels().unwrap();
    // Scope/plan caches own sink Arcs: release scope and bump generation before joining.
    global().clear_channel_sinks("investigation");
    drop(scope);
    {
        let _s = start_scope(ScopeConfig::new("release"));
        black_box(observe!(event(0)).unwrap());
    }
    consumer.join().unwrap();
    for threads in [1, 4] {
        let dir = support::tempdir().unwrap();
        let path = dir.path().join("api.jsonl");
        let sink = Arc::new(JsonlSink::open_append(&path).unwrap());
        global().add_channel_sink("investigation", sink.clone());
        let scope = start_scope(ScopeConfig::new(SCOPE));
        black_box(observe!(event(123456)).unwrap());
        sink.flush().unwrap();
        let ctx = Context::capture();
        let barrier = Arc::new(Barrier::new(threads + 1));
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for _ in 0..threads {
                let ctx = ctx.clone();
                let barrier = barrier.clone();
                handles.push(s.spawn(move || {
                    ctx.run(|| {
                        // Warm this thread's TLS plan before starting the timer.
                        black_box(observe!(event(123456)).unwrap());
                        barrier.wait();
                        for i in 0..MICRO / threads {
                            black_box(observe!(event(i)).unwrap());
                        }
                    })
                }));
            }
            let start = Instant::now();
            barrier.wait();
            for h in handles {
                h.join().unwrap();
            }
            let prod = start.elapsed();
            let pending_upper = MICRO + 1 + threads - sink.stats().records_written as usize;
            sink.flush().unwrap();
            row(
                &format!("api.jsonl_{threads}p_producer"),
                sample,
                MICRO,
                prod,
                0,
                pending_upper as u64,
            );
            row(
                &format!("api.jsonl_{threads}p_e2e"),
                sample,
                MICRO,
                start.elapsed(),
                0,
                sink.stats().batches,
            );
        });
        assert_eq!(sink.stats().records_written as usize, MICRO + 1 + threads);
        assert_eq!(sink.stats().compactions, 0);
        verify_file(&path, MICRO + 1 + threads);
        drop(ctx);
        drop(scope);
        global().clear_channel_sinks("investigation");
    }
}
fn service(file: bool, unique: bool, engine_refs: bool, sample: usize) {
    let base = fixture();
    let bytes = serde_json::to_vec(&base).unwrap().len() + 1;
    let records: Vec<_> = if unique {
        (0..RECORDS).map(|_| Arc::new(base.clone())).collect()
    } else {
        let shared = Arc::new(base);
        (0..RECORDS).map(|_| shared.clone()).collect()
    };
    let dir = support::tempdir().unwrap();
    let path = dir.path().join("service.jsonl");
    let sink = if file {
        JsonlSink::open_append(&path).unwrap()
    } else {
        JsonlSink::from_writer(io::sink())
    };
    sink.write_shared(Arc::new(fixture())).unwrap();
    sink.flush().unwrap();
    let t = Instant::now();
    for record in records {
        if engine_refs {
            sink.write_shared(record.clone()).unwrap();
            drop(record);
        } else {
            sink.write_shared(record).unwrap();
        }
    }
    let producer = t.elapsed();
    let pending_upper = RECORDS + 1 - sink.stats().records_written as usize;
    sink.flush().unwrap();
    let total = t.elapsed();
    let name = format!(
        "service.{}_{}{}",
        if file { "file" } else { "null" },
        if unique { "unique" } else { "shared" },
        if engine_refs { "_engine_refs" } else { "" }
    );
    row(
        &format!("{name}_enqueue"),
        sample,
        RECORDS,
        producer,
        bytes,
        pending_upper as u64,
    );
    row(
        &format!("{name}_e2e"),
        sample,
        RECORDS,
        total,
        bytes,
        sink.stats().batches,
    );
    assert_eq!(sink.stats().records_written as usize, RECORDS + 1);
    assert_eq!(sink.stats().compactions, 0);
    eprintln!(
        "{name} sample={sample} stats={:?} not_yet_committed_at_enqueue_end={pending_upper}",
        sink.stats()
    );
    if file {
        verify_file(&path, RECORDS + 1);
    }
}
fn batch_cpu(batch: usize, sample: usize, append: bool) {
    let record = fixture();
    let mut line = serde_json::to_vec(&record).unwrap();
    line.push(b'\n');
    let n = RECORDS / batch * batch;
    let mut bytes = Vec::with_capacity(line.len() * batch);
    let t = Instant::now();
    for _ in 0..n / batch {
        bytes.clear();
        for _ in 0..batch {
            if append {
                bytes.extend_from_slice(black_box(&line));
            } else {
                serde_json::to_writer(&mut bytes, black_box(&record)).unwrap();
                bytes.push(b'\n');
            }
        }
        black_box(&bytes);
    }
    row(
        &format!(
            "writer.{}_{batch}",
            if append {
                "append_cached_line"
            } else {
                "encode_batch"
            }
        ),
        sample,
        n,
        t.elapsed(),
        line.len(),
        0,
    );
}
fn file_bytes(batch: usize, flush_each: bool, sample: usize) {
    let mut line = serde_json::to_vec(&fixture()).unwrap();
    line.push(b'\n');
    let bytes = line.repeat(batch);
    let n = RECORDS / batch * batch;
    let dir = support::tempdir().unwrap();
    let path = dir.path().join("bytes.jsonl");
    let mut writer = BufWriter::with_capacity(64 * 1024, File::create(&path).unwrap());
    let t = Instant::now();
    for _ in 0..n / batch {
        writer.write_all(black_box(&bytes)).unwrap();
        if flush_each {
            writer.flush().unwrap();
        }
    }
    writer.flush().unwrap();
    row(
        &format!("writer.file_cached_bytes_{batch}_flush_{flush_each}"),
        sample,
        n,
        t.elapsed(),
        line.len(),
        0,
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().len() as usize,
        n * line.len()
    );
}
fn prefilled_record<R: Serialize + Clone + Send + Sync + 'static>(
    base: R,
    kind: &str,
    unique: bool,
    encode: bool,
    file: bool,
    cap: usize,
    sample: usize,
) {
    let line_size = serde_json::to_vec(&base).unwrap().len() + 1;
    let (tx, rx) = unbounded();
    let shared = Arc::new(base);
    for _ in 0..RECORDS {
        tx.send(if unique {
            Arc::new((*shared).clone())
        } else {
            shared.clone()
        })
        .unwrap();
    }
    drop(tx);
    let dir = support::tempdir().unwrap();
    let path = dir.path().join("prefilled.jsonl");
    let output = file.then(|| BufWriter::with_capacity(64 * 1024, File::create(&path).unwrap()));
    let elapsed = std::thread::spawn(move || {
        let mut output = output;
        let mut messages = Vec::new();
        let mut bytes = Vec::with_capacity(64 * 1024);
        let t = Instant::now();
        let mut consumed = 0;
        while consumed < RECORDS {
            let first = rx.recv().unwrap();
            let pending = rx.len().min(cap - 1);
            messages.push(first);
            messages.extend((0..pending).filter_map(|_| rx.try_recv().ok()));
            consumed += messages.len();
            for r in messages.drain(..) {
                if encode {
                    serde_json::to_writer(&mut bytes, &*r).unwrap();
                    bytes.push(b'\n');
                } else {
                    black_box(&r);
                }
            }
            if let Some(output) = output.as_mut() {
                output.write_all(&bytes).unwrap();
            }
            black_box(&bytes);
            bytes.clear();
        }
        if let Some(output) = output.as_mut() {
            output.flush().unwrap();
        }
        t.elapsed()
    })
    .join()
    .unwrap();
    row(
        &format!(
            "writer.prefilled_{kind}{}_encode_{encode}_file_{file}_cap_{cap}",
            if unique { "unique" } else { "shared" }
        ),
        sample,
        RECORDS,
        elapsed,
        line_size,
        RECORDS.div_ceil(cap) as u64,
    );
    if file {
        verify_file(&path, RECORDS);
    }
}
fn prefilled(unique: bool, encode: bool, file: bool, cap: usize, sample: usize) {
    prefilled_record(fixture(), "", unique, encode, file, cap, sample);
}

// Benchmark-only owned envelope; JSON bytes are asserted equal to ObservationRecord.
#[derive(Clone, Serialize)]
struct RawRecord {
    ts: String,
    proc: String,
    proc_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    namespace: String,
    level: ObservationLevel,
    data: Box<serde_json::value::RawValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<String>,
}
fn raw_fixture() -> RawRecord {
    let b = fixture();
    let r = RawRecord {
        ts: b.ts.clone(),
        proc: b.proc.clone(),
        proc_ms: b.proc_ms,
        scope: b.scope.clone(),
        namespace: b.namespace.clone(),
        level: b.level,
        data: to_raw_value(&event(123456)).unwrap(),
        message: b.message.clone(),
        phase: b.phase,
        span: b.span.clone(),
    };
    assert_eq!(
        serde_json::to_vec(&r).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
    r
}
// Same construction and fan-out Arc clone in every row. Retention vs immediate recycling
// is controlled separately from enqueue; all cleanup is outside producer timing.
fn producer_controls(sample: usize) {
    let started = Instant::now();
    for mode in ["drop", "retain", "queue_paused", "queue_active"] {
        let (tx, rx) = unbounded();
        let worker_rx = rx.clone();
        let consumer = (mode == "queue_active").then(|| {
            std::thread::spawn(move || {
                for r in worker_rx {
                    black_box(r);
                }
            })
        });
        let mut retained = Vec::with_capacity(RECORDS);
        let t = Instant::now();
        for i in 0..RECORDS {
            let r = Arc::new(envelope(serde_json::to_value(&event(i)).unwrap(), started));
            match mode {
                "drop" => {
                    black_box(r.clone());
                }
                "retain" => retained.push(r.clone()),
                _ => tx.send(r.clone()).unwrap(),
            }
            drop(r);
        }
        row(
            &format!("producer.full_record_{mode}"),
            sample,
            RECORDS,
            t.elapsed(),
            0,
            0,
        );
        drop(tx);
        if let Some(consumer) = consumer {
            consumer.join().unwrap();
        }
    }
}
fn retained_payload<P: Serialize>(name: &str, payload: &P, n: usize, sample: usize) {
    let value = serde_json::to_value(payload).unwrap();
    let raw = to_raw_value(payload).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), raw.get().as_bytes());
    eprintln!("retained {name} payload={} bytes n={n}", raw.get().len());
    let mut values = Vec::with_capacity(n);
    let t = Instant::now();
    for _ in 0..n {
        values.push(serde_json::to_value(black_box(payload)).unwrap());
    }
    row(
        &format!("producer.{name}.value_retained"),
        sample,
        n,
        t.elapsed(),
        raw.get().len(),
        0,
    );
    drop(values);
    let mut raws = Vec::with_capacity(n);
    let t = Instant::now();
    for _ in 0..n {
        raws.push(to_raw_value(black_box(payload)).unwrap());
    }
    row(
        &format!("producer.{name}.raw_retained"),
        sample,
        n,
        t.elapsed(),
        raw.get().len(),
        0,
    );
    drop(raws);
    if name == "text" {
        let mut bytes = Vec::with_capacity(raw.get().len() * 2);
        let t = timed(
            || {
                let v = serde_json::to_value(black_box(payload)).unwrap();
                bytes.clear();
                serde_json::to_writer(&mut bytes, &v).unwrap();
                black_box(&bytes);
            },
            n,
        );
        row(
            "serde.text.two_stage_payload",
            sample,
            n,
            t,
            raw.get().len(),
            0,
        );
        let t = timed(
            || {
                let r = to_raw_value(black_box(payload)).unwrap();
                bytes.clear();
                serde_json::to_writer(&mut bytes, &*r).unwrap();
                black_box(&bytes);
            },
            n,
        );
        row("serde.text.raw_payload", sample, n, t, raw.get().len(), 0);
    }
}
fn confirm(sample: usize) {
    // Reverse order on alternating samples to expose order/cache/thermal effects.
    let caps = if sample.is_multiple_of(2) {
        [RECORDS, 256, 8192]
    } else {
        [8192, 256, RECORDS]
    };
    for cap in caps {
        prefilled(true, true, true, cap, sample);
    }
    producer_controls(sample);
    service(true, true, false, sample);
    service(true, true, true, sample);
    retained_payload("small", &event(123456), 100_000, sample);
    retained_payload("rich", &rich(123456), 100_000, sample);
    #[derive(Serialize)]
    struct Text {
        text: String,
    }
    let text = Text {
        text: "\"quoted\" \\path\n\r\t representative UTF-8 наблюдение ".repeat(64),
    };
    retained_payload("text", &text, 10_000, sample);
    for cap in [RECORDS, 8192] {
        prefilled_record(raw_fixture(), "raw_", true, true, false, cap, sample);
        prefilled_record(raw_fixture(), "raw_", true, true, true, cap, sample);
    }
}
// Allocation occurs on the caller; only destruction of an already populated Vec is timed.
// Cross-thread mode excludes thread creation/join. No encode, queue or filesystem work.
fn drop_objects<T: Send + 'static>(name: &str, sample: usize, mut make: impl FnMut() -> T) {
    for remote in [false, true] {
        let objects: Vec<_> = (0..RECORDS).map(|_| make()).collect();
        let work = move || {
            let t = Instant::now();
            for object in objects {
                black_box(object);
            }
            t.elapsed()
        };
        let elapsed = if remote {
            std::thread::spawn(work).join().unwrap()
        } else {
            work()
        };
        row(
            &format!("drop.{name}.remote_{remote}"),
            sample,
            RECORDS,
            elapsed,
            0,
            0,
        );
    }
}
fn writer(sample: usize) {
    let record = fixture();
    let mut bytes = Vec::with_capacity(1024);
    bench("writer.encode_record_reuse", sample, || {
        bytes.clear();
        serde_json::to_writer(&mut bytes, black_box(&record)).unwrap();
        bytes.push(b'\n');
        black_box(&bytes);
    });
    let numeric = DateTime::parse_from_rfc3339(TS).unwrap().timestamp_millis();
    bench("writer.encode_plus_deferred_timestamp", sample, || {
        let ts = DateTime::<Utc>::from_timestamp_millis(black_box(numeric))
            .unwrap()
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut env = borrowed(&record, &record.data);
        env.ts = &ts;
        bytes.clear();
        serde_json::to_writer(&mut bytes, &env).unwrap();
        bytes.push(b'\n');
        black_box(&bytes);
    });
    for batch in [1, 256, 8192] {
        batch_cpu(batch, sample, false);
        batch_cpu(batch, sample, true);
        file_bytes(batch, false, sample);
        // Per-record flush is intentionally excluded: would be a different persistence contract.
        if batch > 1 {
            file_bytes(batch, true, sample);
        }
    }
    let dir = support::tempdir().unwrap();
    let mut f =
        BufWriter::with_capacity(64 * 1024, File::create(dir.path().join("empty")).unwrap());
    bench("writer.flush_empty", sample, || {
        black_box(&mut f).flush().unwrap();
    });
    file_bytes(RECORDS, false, sample);
    drop_objects("payload_value", sample, || {
        serde_json::to_value(event(123456)).unwrap()
    });
    let base = fixture();
    drop_objects("envelope_strings", sample, || {
        (
            base.ts.clone(),
            base.proc.clone(),
            base.scope.clone(),
            base.namespace.clone(),
        )
    });
    drop_objects("arc_envelope_null", sample, || {
        let mut r = base.clone();
        r.data = Value::Null;
        Arc::new(r)
    });
    drop_objects("arc_record", sample, || Arc::new(base.clone()));
    prefilled(false, false, false, RECORDS, sample);
    prefilled(true, false, false, RECORDS, sample);
    prefilled(true, true, false, RECORDS, sample);
    prefilled(true, true, true, RECORDS, sample);
    for cap in [256, 8192] {
        prefilled(true, true, false, cap, sample);
        prefilled(true, true, true, cap, sample);
    }
    service(false, false, false, sample);
    service(false, true, false, sample);
    service(true, true, false, sample);
}
fn main() {
    support::run(benchmark);
}

fn benchmark() {
    let repeats: usize = std::env::var("SUN_OBS_INV_REPEATS")
        .ok()
        .map(|x| x.parse().unwrap())
        .unwrap_or(7);
    assert!(repeats > 0);
    let group = std::env::var("SUN_OBS_INV_GROUP").unwrap_or_default();
    let enabled = |g: &str| group.is_empty() || group == g;
    global()
        .register_namespace_channel("bench", "investigation")
        .unwrap();
    println!("case,sample,iterations,ns_per_record,bytes_per_record,aux");
    for sample in 0..repeats {
        if enabled("micro") {
            micro(sample);
        }
        if enabled("serde") {
            serde_cases("small", &event(123456), sample);
            serde_cases("rich", &rich(123456), sample);
        }
        if enabled("writer") {
            writer(sample);
        }
        if enabled("api") {
            api(sample);
        }
        if group == "confirm" {
            confirm(sample);
        }
    }
    global().clear_channel_sinks("investigation");
}
