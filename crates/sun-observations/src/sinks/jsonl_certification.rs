use super::*;
use crate::{test_support::record, MemorySink, ObservationEngine, Selection};
use std::{
    fs,
    io::Write,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

#[test]
fn continuous_producers_do_not_starve_periodic_flush_and_share_one_worker() {
    let mut certification = sun_observations::certifications::certification(
        "continuous producers do not starve periodic flush and share one worker",
    );
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Periodic {
        signal: mpsc::Sender<std::thread::ThreadId>,
        producer: std::thread::ThreadId,
        wrote: bool,
    }
    impl Write for Periodic {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            assert_ne!(std::thread::current().id(), self.producer);
            self.wrote = true;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.wrote {
                let _ = self.signal.send(std::thread::current().id());
            }
            Ok(())
        }
    }
    let (signal, receiver) = mpsc::channel();
    let sink = Arc::new(
        JsonlSink::writer_with_config(
            Periodic {
                signal,
                producer: std::thread::current().id(),
                wrote: false,
            },
            JsonlConfig::new().flush_interval_ms(1).unwrap(),
        )
        .unwrap(),
    );
    let stop = AtomicBool::new(false);
    let worker = std::thread::scope(|threads| {
        for _ in 0..4 {
            let sink = sink.clone();
            let stop = &stop;
            threads.spawn(move || {
                let event = Arc::new(record("periodic::load"));
                while !stop.load(Ordering::Acquire) {
                    sink.write_shared(event.clone()).unwrap();
                }
            });
        }
        let worker = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
        certification.check_fact_expect(
            "worker",
            &(worker),
            sun_observations::certifications::NotEqual(&(std::thread::current().id())),
        );
        stop.store(true, Ordering::Release);
        worker
    });
    sink.flush().unwrap();
    certification.check_fact_expect(
        "sink.stats().records_written > 0",
        sink.stats().records_written > 0,
        true,
    );
    while let Ok(next_worker) = receiver.try_recv() {
        certification.check_fact_expect("worker", &(worker), &(next_worker));
    }
    certification.assert_certify();
}
#[test]
fn span_end_async_failure_surfaces_at_engine_barrier_not_as_a_missing_checked_error() {
    let mut certification = sun_observations::certifications::certification(
        "span end async failure surfaces at engine barrier not as a missing checked error",
    );
    use crate::{Observation, Phase};
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "async_span::work")]
    struct Work;
    struct EndFailure;
    impl Write for EndFailure {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            for line in b.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
                let r: ObservationRecord = serde_json::from_slice(line).unwrap();
                if r.phase == Some(Phase::End) {
                    return Err(io::Error::other("async end"));
                }
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("async_span", "all")
        .unwrap();
    engine.add_channel_sink("all", Arc::new(JsonlSink::from_writer(EndFailure)));
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("all", memory.clone());
    let span = engine.span_if_active(|| Work).unwrap();
    engine.flush_channels().unwrap();
    span.finish().unwrap();
    certification.check_fact_expect(
        "engine.flush_channels().is_err()",
        engine.flush_channels().is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(1),
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(2));
    certification.assert_certify();
}

#[test]
fn retention_can_remove_span_start_without_splitting_newest_end() {
    let mut certification = sun_observations::certifications::certification(
        "retention can remove span start without splitting newest end",
    );
    use crate::{Observation, Phase};
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "retained_span::work")]
    struct Work;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("span.jsonl");
    let sink = Arc::new(
        JsonlSink::with_config(&path, JsonlConfig::new().max_file_size_bytes(1024).unwrap())
            .unwrap(),
    );
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("retained_span", "file")
        .unwrap();
    engine.add_channel_sink("file", sink.clone());
    let span = engine.span_if_active(|| Work).unwrap();
    engine.flush_channels().unwrap();
    let mut big = record("retained_span::oversized");
    big.data = serde_json::json!("x".repeat(8192));
    sink.write(&big).unwrap();
    sink.flush().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    certification.check_fact_expect("text.len() > 1024", text.len() > 1024, true);
    certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
    certification.check_fact_expect(
        "serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap()",
        &(serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap()),
        &(big),
    );
    span.finish().unwrap();
    engine.flush_channels().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    certification.check_fact_expect("text.len() <= 1024", text.len() <= 1024, true);
    certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
    let records: Vec<ObservationRecord> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    certification.check_fact_expect("records.len()", &(records.len()), &(1));
    certification.check_fact_expect("records[0].phase", &(records[0].phase), &(Some(Phase::End)));
    certification.check_fact_expect("records[0].data.is_null()", records[0].data.is_null(), true);
    certification.assert_certify();
}

#[test]
fn retention_accepts_valid_deep_payload_without_an_implicit_format_depth_limit() {
    let mut certification = sun_observations::certifications::certification(
        "retention accepts valid deep payload without an implicit format depth limit",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep.jsonl");
    let config = JsonlConfig::new().max_file_size_bytes(4096).unwrap();
    let mut event = record("retention::deep");
    for _ in 0..256 {
        event.data = serde_json::json!([event.data]);
    }
    let original = serde_json::to_string(&event).unwrap() + "\n";
    {
        let sink = JsonlSink::with_config(&path, config.clone()).unwrap();
        sink.write(&event).unwrap();
        sink.flush().unwrap();
    }
    let sink = JsonlSink::with_config(&path, config).unwrap();
    let _ = sink.write(&record("retention::next"));
    sink.flush().unwrap();
    certification.check_fact_expect(
        "fs::read_to_string(&path).unwrap().starts_with(&original)",
        fs::read_to_string(&path).unwrap().starts_with(&original),
        true,
    );
    certification.check_fact_expect(
        "fs::read_to_string(&path).unwrap().lines().count()",
        &(fs::read_to_string(&path).unwrap().lines().count()),
        &(2),
    );
    certification.assert_certify();
}

struct GateFile {
    file: fs::File,
    entered: Option<mpsc::Sender<()>>,
    release: mpsc::Receiver<()>,
    producer: std::thread::ThreadId,
    workers: Arc<Mutex<Vec<std::thread::ThreadId>>>,
    chunks: Arc<Mutex<Vec<usize>>>,
}
impl Write for GateFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let id = std::thread::current().id();
        assert_ne!(id, self.producer);
        self.workers.lock().unwrap().push(id);
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        self.chunks
            .lock()
            .unwrap()
            .push(bytes.iter().filter(|&&b| b == b'\n').count());
        self.file.write_all(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        assert_ne!(std::thread::current().id(), self.producer);
        self.file.flush()
    }
}
#[test]
fn bounded_backlog_fifo_barrier_and_disconnect_drain_real_file() {
    let mut certification = sun_observations::certifications::certification(
        "bounded backlog fifo barrier and disconnect drain real file",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backlog.jsonl");
    let (entered, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let workers = Arc::new(Mutex::new(Vec::new()));
    let sink = JsonlSink::from_writer(GateFile {
        file: fs::File::create(&path).unwrap(),
        entered: Some(entered),
        release: resume,
        producer: std::thread::current().id(),
        workers: workers.clone(),
        chunks: chunks.clone(),
    });
    certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
    sink.flush().unwrap();
    certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
    let mut event = record("bounded::event");
    event.data = serde_json::json!(0);
    sink.write(&event).unwrap();
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    let n = 65537;
    for i in 1..n {
        event.data = serde_json::json!(i);
        sink.write(&event).unwrap();
    }
    release.send(()).unwrap();
    sink.flush().unwrap();
    let bytes = fs::read_to_string(&path).unwrap();
    certification.check_fact_expect("bytes.ends_with('\\n')", bytes.ends_with('\n'), true);
    let actual: Vec<usize> = bytes
        .lines()
        .map(|l| {
            serde_json::from_str::<ObservationRecord>(l)
                .unwrap()
                .data
                .as_u64()
                .unwrap() as usize
        })
        .collect();
    certification.check_fact_expect("actual", &(actual), &((0..n).collect::<Vec<_>>()));
    certification.check_fact_expect(
        "sink.stats().batches > (n as u64 - 1).div_ceil(8192)",
        sink.stats().batches > (n as u64 - 1).div_ceil(8192),
        true,
    );
    certification.check_fact_expect(
        "chunks.lock().unwrap().iter().all(|&n| n <= 8192)",
        chunks.lock().unwrap().iter().all(|&n| n <= 8192),
        true,
    );
    let ids = workers.lock().unwrap();
    certification.check_fact_expect(
        "ids.iter().all(|id| *id == ids[0])",
        ids.iter().all(|id| *id == ids[0]),
        true,
    );
    drop(ids);
    for i in n..n + 20000 {
        event.data = serde_json::json!(i);
        sink.write(&event).unwrap();
    }
    drop(sink);
    let text = fs::read_to_string(&path).unwrap();
    certification.check_fact_expect(
        "text.lines().count()",
        &(text.lines().count()),
        &(n + 20000),
    );
    certification.check_fact_expect(
        "serde_json::from_str::<ObservationRecord>(text.lines().last().unwrap()) .unwrap() .data",
        &(serde_json::from_str::<ObservationRecord>(text.lines().last().unwrap())
            .unwrap()
            .data),
        &(serde_json::json!(n + 19999)),
    );
    certification.assert_certify();
}
#[derive(Clone, Copy, Debug)]
enum Fault {
    Write,
    Flush,
    Panic,
}
struct FaultWriter {
    fault: Fault,
    calls: Arc<Mutex<(usize, usize)>>,
}
impl Write for FaultWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls.lock().unwrap().0 += 1;
        match self.fault {
            Fault::Write => Err(io::Error::other("first write")),
            Fault::Panic => panic!("writer panic"),
            Fault::Flush => Ok(bytes.len()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().1 += 1;
        Err(io::Error::other("first flush"))
    }
}
#[test]
fn independent_background_failures_are_sticky_do_not_retry_and_release_concurrent_barriers() {
    let mut certification = sun_observations::certifications::certification(
        "independent background failures are sticky do not retry and release concurrent barriers",
    );
    for fault in [Fault::Write, Fault::Flush, Fault::Panic] {
        let calls = Arc::new(Mutex::new((0, 0)));
        let sink = Arc::new(JsonlSink::from_writer(FaultWriter {
            fault,
            calls: calls.clone(),
        }));
        let _ = sink.write(&record("fault::event"));
        std::thread::scope(|threads| {
            let mut handles = Vec::new();
            for _ in 0..16 {
                let sink = sink.clone();
                handles.push(threads.spawn(move || sink.flush()));
            }
            for h in handles {
                certification.check_fact_expect(
                    "h.join().unwrap().is_err()",
                    h.join().unwrap().is_err(),
                    true,
                );
            }
        });
        let first = sink.last_error().unwrap().to_string();
        let counts = *calls.lock().unwrap();
        certification.check_fact_expect(
            "sink.write(&record(\"fault::later\")).is_err()",
            sink.write(&record("fault::later")).is_err(),
            true,
        );
        certification.check_fact_expect("sink.flush().is_err()", sink.flush().is_err(), true);
        certification.check_fact_expect(
            "sink.last_error().unwrap().to_string()",
            &(sink.last_error().unwrap().to_string()),
            &(first),
        );
        certification.check_fact_expect(
            "*calls.lock().unwrap()",
            &(*calls.lock().unwrap()),
            &(counts),
        );
        certification.check_fact_expect("counts.0", &(counts.0), &(1));
        certification.check_fact_expect(
            "counts.1",
            &(counts.1),
            &(usize::from(matches!(fault, Fault::Flush))),
        );
        drop(sink);
    }
    certification.assert_certify();
}
#[test]
fn engine_error_barrier_counts_checked_operations_and_healthy_fanout_survives() {
    let mut certification = sun_observations::certifications::certification(
        "engine error barrier counts checked operations and healthy fanout survives",
    );
    use crate::Observation;
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "fault::engine")]
    struct Event;
    let engine = ObservationEngine::isolated();
    let directory = tempfile::tempdir().unwrap();
    let broken = Arc::new(JsonlSink::open_append(directory.path().join("missing/file")).unwrap());
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("fault", "all").unwrap();
    engine.add_channel_sink("all", broken.clone());
    engine.add_channel_sink("all", memory.clone());
    let initial = engine.observe_if_active(|| Event);
    certification.check_fact_expect("memory.len()", &(memory.len()), &(1));
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(u64::from(initial.is_err())),
    );
    certification.check_fact_expect(
        "engine.flush_channels().is_err()",
        engine.flush_channels().is_err(),
        true,
    );
    let errors = engine.sink_error_count();
    certification.check_fact_expect(
        "broken.last_error().is_some()",
        broken.last_error().is_some(),
        true,
    );
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).is_err()",
        engine.observe_if_active(|| Event).is_err(),
        true,
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(2));
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(errors + 1),
    );
    certification.check_fact_expect(
        "engine.flush_channels().is_err()",
        engine.flush_channels().is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(errors + 2),
    );
    certification.assert_certify();
}
#[test]
fn retention_is_a_newest_complete_suffix_across_cycles_and_rejects_corrupt_history() {
    let mut certification = sun_observations::certifications::certification(
        "retention is a newest complete suffix across cycles and rejects corrupt history",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("suffix.jsonl");
    let config = JsonlConfig::new()
        .max_file_size_bytes(4096)
        .unwrap()
        .trim_fraction(0.25)
        .unwrap();
    let sink = JsonlSink::with_config(&path, config.clone()).unwrap();
    let mut history = Vec::new();
    for i in 0..300 {
        let mut event = record("retention::event");
        event.data = serde_json::json!({"index": i, "text": "日本語\n\\\"".repeat(8)});
        history.push(event.clone());
        sink.write(&event).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
        let values: Vec<ObservationRecord> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        certification.check_fact_expect(
            "values",
            values.as_slice(),
            &history[history.len() - values.len()..],
        );
        certification.check_fact_expect("values.last()", &(values.last()), &(history.last()));
        certification.check_fact_expect("text.len() <= 4096", text.len() <= 4096, true);
    }
    certification.check_fact_expect(
        "sink.stats().compactions > 10",
        sink.stats().compactions > 10,
        true,
    );
    drop(sink);
    for corrupt in ["{\"valid\":true}\n{", "not json\n", "{\"valid\":true}\r\n"] {
        fs::write(&path, corrupt).unwrap();
        let sink = JsonlSink::with_config(&path, config.clone()).unwrap();
        let mut event = record("retention::new");
        event.data = serde_json::json!("x".repeat(8192));
        let _ = sink.write(&event);
        certification.check_fact_expect("sink.flush().is_err()", sink.flush().is_err(), true);
        certification.check_fact_expect(
            "fs::read_to_string(&path).unwrap()",
            fs::read_to_string(&path).unwrap().as_str(),
            corrupt,
        );
    }
    certification.assert_certify();
}

mod persistence {
    use super::super::*;
    use crate::{test_support::record, Phase};
    use std::{fs, sync::Arc};

    #[test]
    fn append_preserves_existing_lines_and_schema_roundtrips() {
        let mut certification = sun_observations::certifications::certification(
            "append preserves existing lines and schema roundtrips",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("observations.jsonl");
        let point = record("jsonl::point");
        let mut span = point.clone();
        span.scope = Some("run-a".into());
        span.phase = Some(Phase::Start);
        span.span = Some("span-a".into());
        span.message = Some("line one\nline two".into());
        JsonlSink::open_append(&path)
            .unwrap()
            .write(&point)
            .unwrap();
        let sink = JsonlSink::open_append(&path).unwrap();
        sink.write(&span).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
        let records: Vec<ObservationRecord> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        certification.check_fact_expect("records", records.as_slice(), [point, span].as_slice());
        for line in text.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            for key in ["ts", "proc", "proc_ms", "namespace", "level", "data"] {
                certification.check_fact_expect(
                    "value.get(key).is_some()",
                    value.get(key).is_some(),
                    true,
                );
            }
            for key in ["tags", "channel", "channels"] {
                certification.check_fact_expect(
                    "value.get(key).is_none()",
                    value.get(key).is_none(),
                    true,
                );
            }
        }
        certification.assert_certify();
    }

    #[test]
    fn concurrent_lines_are_complete_json_objects() {
        let mut certification = sun_observations::certifications::certification(
            "concurrent lines are complete json objects",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parallel.jsonl");
        let sink = Arc::new(JsonlSink::open_append(&path).unwrap());
        std::thread::scope(|scope| {
            for writer in 0..8 {
                let sink = sink.clone();
                scope.spawn(move || {
                    for item in 0..75 {
                        let mut record = record("jsonl::parallel");
                        record.data = serde_json::json!({"writer": writer, "item": item});
                        sink.write(&record).unwrap();
                    }
                });
            }
        });
        sink.flush().unwrap();
        let text = fs::read_to_string(path).unwrap();
        let pairs: std::collections::HashSet<_> = text
            .lines()
            .map(|line| {
                let record: ObservationRecord = serde_json::from_str(line).unwrap();
                (
                    record.data["writer"].as_u64().unwrap(),
                    record.data["item"].as_u64().unwrap(),
                )
            })
            .collect();
        certification.check_fact_expect("text.lines().count()", &(text.lines().count()), &(600));
        certification.check_fact_expect("pairs.len()", &(pairs.len()), &(600));
        certification.assert_certify();
    }

    #[test]
    fn engine_output_matches_envelope_without_routing_metadata() {
        let mut certification = sun_observations::certifications::certification(
            "engine output matches envelope without routing metadata",
        );
        use crate::{start_scope, Observation, ObservationEngine, ScopeConfig, Selection};
        #[derive(serde::Serialize, Observation)]
        #[observation(namespace = "jsonl::engine::event", level = ERROR, tags("expensive"))]
        struct Event {
            value: usize,
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("engine.jsonl");
        let engine = ObservationEngine::isolated();
        engine.add_channel_sink("logical", Arc::new(JsonlSink::open_append(&path).unwrap()));
        engine
            .register_namespace_channel("jsonl::engine", "logical")
            .unwrap();
        engine.set_selection(
            Selection::new()
                .activate_namespace("jsonl::engine")
                .unwrap(),
        );
        {
            let _scope = start_scope(ScopeConfig::new("persisted-run"));
            engine.observe_if_active(|| Event { value: 7 }).unwrap();
            let _span = engine.span_if_active(|| Event { value: 8 }).unwrap();
        }
        engine.observe_if_active(|| Event { value: 9 }).unwrap();
        engine.flush_channels().unwrap();
        let text = fs::read_to_string(path).unwrap();
        let records: Vec<ObservationRecord> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        certification.check_fact_expect("records.len()", &(records.len()), &(4));
        certification.check_fact_expect(
            "records[0].scope.as_deref()",
            &(records[0].scope.as_deref()),
            &(Some("persisted-run")),
        );
        certification.check_fact_expect(
            "records[0].data",
            &(records[0].data),
            &(serde_json::json!({"value": 7})),
        );
        certification.check_fact_expect("records[1].span", &(records[1].span), &(records[2].span));
        certification.check_fact_expect(
            "records[1].phase",
            &(records[1].phase),
            &(Some(Phase::Start)),
        );
        certification.check_fact_expect(
            "records[2].phase",
            &(records[2].phase),
            &(Some(Phase::End)),
        );
        certification.check_fact_expect(
            "records[2].data.is_null()",
            records[2].data.is_null(),
            true,
        );
        certification.check_fact_expect("records[3].scope", &(records[3].scope), &(None));
        for record in &records {
            certification.check_fact_expect(
                "record.proc",
                record.proc.as_str(),
                engine.process_id(),
            );
            certification.check_fact_expect(
                "record.namespace",
                record.namespace.as_str(),
                Event::NAMESPACE,
            );
            certification.check_fact_expect(
                "record.level",
                &(record.level),
                &(crate::ObservationLevel::ERROR),
            );
            certification.check_fact_expect(
                "record.ts.ends_with('Z')",
                record.ts.ends_with('Z'),
                true,
            );
            certification.check_fact_expect("record.ts.len()", &(record.ts.len()), &(24));
            certification.check_fact_expect(
                "chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok()",
                chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok(),
                true,
            );
        }
        for line in text.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            for absent in ["tags", "channel", "channels", "message"] {
                certification.check_fact_expect(
                    "value.get(absent).is_none()",
                    value.get(absent).is_none(),
                    true,
                );
            }
        }
        certification.assert_certify();
    }

    #[test]
    fn lazy_worker_and_periodic_flush_run_off_producer_without_sleeps() {
        let mut certification = sun_observations::certifications::certification(
            "lazy worker and periodic flush run off producer without sleeps",
        );
        use std::sync::mpsc;
        use std::time::Duration;
        struct Probe {
            producer: std::thread::ThreadId,
            flushed: mpsc::Sender<usize>,
            bytes: Vec<u8>,
        }
        impl Write for Probe {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                assert_ne!(self.producer, std::thread::current().id());
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                assert_ne!(self.producer, std::thread::current().id());
                if !self.bytes.is_empty() {
                    let _ = self.flushed.send(self.bytes.len());
                }
                Ok(())
            }
        }
        let (sender, receiver) = mpsc::channel();
        let sink = JsonlSink::writer_with_config(
            Probe {
                producer: std::thread::current().id(),
                flushed: sender,
                bytes: Vec::new(),
            },
            JsonlConfig::new().flush_interval_ms(10).unwrap(),
        )
        .unwrap();
        certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
        sink.flush().unwrap();
        certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
        sink.write(&record("jsonl::periodic")).unwrap();
        certification.check_fact_expect("sink.is_started()", sink.is_started(), true);
        certification.check_fact_expect(
            "receiver.recv_timeout(Duration::from_secs(3)).unwrap() > 0",
            receiver.recv_timeout(Duration::from_secs(3)).unwrap() > 0,
            true,
        );
        certification.check_fact_expect(
            "sink.stats().records_written",
            &(sink.stats().records_written),
            &(1),
        );
        certification.assert_certify();
    }

    #[test]
    fn file_open_is_lazy_and_background_failure_is_sticky() {
        let mut certification = sun_observations::certifications::certification(
            "file open is lazy and background failure is sticky",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-yet.jsonl");
        let sink = JsonlSink::open_append(&path).unwrap();
        certification.check_fact_expect("path.exists()", path.exists(), false);
        sink.write(&record("jsonl::lazy_file")).unwrap();
        sink.flush().unwrap();
        certification.check_fact_expect("path.exists()", path.exists(), true);
        let invalid =
            JsonlSink::open_append(directory.path().join("missing").join("file.jsonl")).unwrap();
        let _ = invalid.write(&record("jsonl::bad_path"));
        certification.check_fact_expect("invalid.flush().is_err()", invalid.flush().is_err(), true);
        certification.check_fact_expect(
            "invalid.last_error().is_some()",
            invalid.last_error().is_some(),
            true,
        );
        certification.check_fact_expect(
            "invalid.write(&record(\"jsonl::bad_path\")).is_err()",
            invalid.write(&record("jsonl::bad_path")).is_err(),
            true,
        );
        certification.assert_certify();
    }

    #[test]
    fn normal_drop_drains_all_accepted_records() {
        let mut certification = sun_observations::certifications::certification(
            "normal drop drains all accepted records",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drop.jsonl");
        {
            let sink = Arc::new(JsonlSink::open_append(&path).unwrap());
            std::thread::scope(|threads| {
                for _ in 0..4 {
                    let sink = sink.clone();
                    threads.spawn(move || {
                        for _ in 0..250 {
                            sink.write(&record("jsonl::drop")).unwrap();
                        }
                    });
                }
            });
        }
        certification.check_fact_expect(
            "fs::read_to_string(path).unwrap().lines().count()",
            &(fs::read_to_string(path).unwrap().lines().count()),
            &(1000),
        );
        certification.assert_certify();
    }

    #[test]
    fn concurrent_flush_barriers_do_not_deadlock_or_lose_records() {
        let mut certification = sun_observations::certifications::certification(
            "concurrent flush barriers do not deadlock or lose records",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("barriers.jsonl");
        let sink = Arc::new(JsonlSink::open_append(&path).unwrap());
        std::thread::scope(|threads| {
            for _ in 0..8 {
                let sink = sink.clone();
                threads.spawn(move || {
                    for _ in 0..20 {
                        sink.write(&record("jsonl::barrier")).unwrap();
                        sink.flush().unwrap();
                    }
                });
            }
        });
        sink.flush().unwrap();
        certification.check_fact_expect(
            "fs::read_to_string(path).unwrap().lines().count()",
            &(fs::read_to_string(path).unwrap().lines().count()),
            &(160),
        );
        certification.assert_certify();
    }

    #[test]
    fn repeated_compaction_retains_complete_newest_lines_with_hysteresis() {
        let mut certification = sun_observations::certifications::certification(
            "repeated compaction retains complete newest lines with hysteresis",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("trim.jsonl");
        let sink = JsonlSink::with_config(
            &path,
            JsonlConfig::new()
                .max_file_size_bytes(4096)
                .unwrap()
                .trim_fraction(0.25)
                .unwrap(),
        )
        .unwrap();
        let mut observed = 0;
        for cycle in 0..8 {
            for item in 0..30 {
                let mut event = record("jsonl::trim");
                event.data =
                    serde_json::json!({"item": cycle * 30 + item, "padding": "x".repeat(64)});
                sink.write(&event).unwrap();
            }
            sink.flush().unwrap();
            let text = fs::read_to_string(&path).unwrap();
            certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
            let values: Vec<ObservationRecord> = text
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            certification.check_fact_expect(
                "values.last().unwrap().data[\"item\"]",
                &(values.last().unwrap().data["item"]),
                &(serde_json::json!(cycle * 30 + 29)),
            );
            certification.check_fact_expect(
                "values[0].data[\"item\"].as_u64().unwrap() > 0",
                values[0].data["item"].as_u64().unwrap() > 0,
                true,
            );
            certification.check_fact_expect("text.len() <= 4096", text.len() <= 4096, true);
            let compactions = sink.stats().compactions;
            if compactions > observed {
                certification.check_fact_expect(
                    "text.len() >= 3072 - 512",
                    text.len() >= 3072 - 512,
                    true,
                );
                observed = compactions;
            }
        }
        certification.check_fact_expect("observed >= 8", observed >= 8, true);
        certification.assert_certify();
    }

    #[test]
    fn threshold_crossing_trims_to_target_not_just_to_maximum() {
        let mut certification = sun_observations::certifications::certification(
            "threshold crossing trims to target not just to maximum",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hysteresis.jsonl");
        let mut original = String::new();
        let mut longest = 0;
        for item in 0..100 {
            let mut event = record("jsonl::hysteresis");
            event.data = serde_json::json!({"item": item});
            let line = serde_json::to_string(&event).unwrap();
            longest = longest.max(line.len() + 1);
            original.push_str(&line);
            original.push('\n');
        }
        certification.check_fact_expect("original.len() > 4096", original.len() > 4096, true);
        fs::write(&path, original).unwrap();
        let sink = JsonlSink::with_config(
            &path,
            JsonlConfig::new()
                .max_file_size_bytes(4096)
                .unwrap()
                .trim_fraction(0.25)
                .unwrap(),
        )
        .unwrap();
        let mut newest = record("jsonl::hysteresis");
        newest.data = serde_json::json!({"item": 100});
        sink.write(&newest).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        certification.check_fact_expect(
            "text.len() <= 3072 && text.len() >= 3072 - longest - 1",
            text.len() <= 3072 && text.len() >= 3072 - longest - 1,
            true,
        );
        let lines: Vec<ObservationRecord> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        certification.check_fact_expect(
            "lines[0].data[\"item\"].as_u64().unwrap() > 0",
            lines[0].data["item"].as_u64().unwrap() > 0,
            true,
        );
        certification.check_fact_expect(
            "lines.last().unwrap().data[\"item\"]",
            &(lines.last().unwrap().data["item"]),
            &(serde_json::json!(100)),
        );
        certification.check_fact_expect(
            "sink.stats().compactions",
            &(sink.stats().compactions),
            &(1),
        );
        sink.write(&newest).unwrap();
        sink.flush().unwrap();
        certification.check_fact_expect(
            "sink.stats().compactions",
            &(sink.stats().compactions),
            &(1),
        );
        certification.assert_certify();
    }

    #[test]
    fn oversized_newest_record_is_never_split_or_deleted() {
        let mut certification = sun_observations::certifications::certification(
            "oversized newest record is never split or deleted",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("oversize.jsonl");
        let sink =
            JsonlSink::with_config(&path, JsonlConfig::new().max_file_size_bytes(512).unwrap())
                .unwrap();
        sink.write(&record("jsonl::small")).unwrap();
        let mut big = record("jsonl::big");
        big.data = serde_json::json!({"padding": "x".repeat(4096)});
        sink.write(&big).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        certification.check_fact_expect("text.lines().count()", &(text.lines().count()), &(1));
        certification.check_fact_expect("text.ends_with('\\n')", text.ends_with('\n'), true);
        certification.check_fact_expect(
            "serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap()",
            &(serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap()),
            &(big),
        );
        sink.write(&record("jsonl::newest")).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(path).unwrap();
        certification.check_fact_expect("text.len() <= 512", text.len() <= 512, true);
        certification.check_fact_expect(
            "serde_json::from_str::<ObservationRecord>(text.trim_end()) .unwrap() .namespace",
            &(serde_json::from_str::<ObservationRecord>(text.trim_end())
                .unwrap()
                .namespace),
            &(String::from("jsonl::newest")),
        );
        certification.assert_certify();
    }

    #[test]
    fn writer_panics_are_observable_and_do_not_deadlock_flush_or_drop() {
        let mut certification = sun_observations::certifications::certification(
            "writer panics are observable and do not deadlock flush or drop",
        );
        struct Panics;
        impl Write for Panics {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                panic!("writer panic");
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let sink = JsonlSink::from_writer(Panics);
        let _ = sink.write(&record("jsonl::panic"));
        certification.check_fact_expect("sink.flush().is_err()", sink.flush().is_err(), true);
        certification.check_fact_expect(
            "sink.last_error().unwrap().to_string().contains(\"panicked\")",
            sink.last_error().unwrap().to_string().contains("panicked"),
            true,
        );
        certification.assert_certify();
    }

    #[test]
    fn invalid_file_settings_and_writer_retention_are_rejected() {
        let mut certification = sun_observations::certifications::certification(
            "invalid file settings and writer retention are rejected",
        );
        for config in [
            JsonlConfig::new().flush_interval_ms(0),
            JsonlConfig::new().max_file_size_bytes(0),
            JsonlConfig::new().trim_fraction(1.0),
            JsonlConfig::new().trim_fraction(f64::NAN),
        ] {
            certification.check_fact_expect(
                "JsonlSink::with_config(\"unused.jsonl\", config).is_err()",
                config.is_err(),
                true,
            );
        }
        certification.check_fact_expect(
            "arbitrary writer rejects file retention",
            JsonlSink::writer_with_config(
                Vec::<u8>::new(),
                JsonlConfig::new().max_file_size_mb(1).unwrap(),
            )
            .is_err(),
            true,
        );
        certification.assert_certify();
    }

    #[test]
    fn writer_failures_are_reported() {
        let mut certification =
            sun_observations::certifications::certification("writer failures are reported");
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("broken"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("broken"))
            }
        }
        let sink = JsonlSink::from_writer(Broken);
        let _ = sink.write(&record("jsonl::broken"));
        certification.check_fact_expect("sink.flush().is_err()", sink.flush().is_err(), true);
        certification.check_fact_expect(
            "sink.last_error().is_some()",
            sink.last_error().is_some(),
            true,
        );
        certification.check_fact_expect(
            "sink.write(&record(\"jsonl::broken\")).is_err()",
            sink.write(&record("jsonl::broken")).is_err(),
            true,
        );
        certification.assert_certify();
    }
}
