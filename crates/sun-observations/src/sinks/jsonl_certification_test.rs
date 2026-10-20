use super::*;
use crate::{support_test::record, MemorySink, ObservationEngine, Selection};
use std::{
    fs,
    io::Write,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

#[test]
fn continuous_producers_do_not_starve_periodic_flush_and_share_one_worker() {
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
            JsonlConfig::new().flush_ms(1),
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
        assert_ne!(worker, std::thread::current().id());
        stop.store(true, Ordering::Release);
        worker
    });
    sink.flush().unwrap();
    assert!(sink.stats().records_written > 0);
    while let Ok(next_worker) = receiver.try_recv() {
        assert_eq!(worker, next_worker);
    }
}
#[test]
fn span_end_async_failure_surfaces_at_engine_barrier_not_as_a_missing_checked_error() {
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
    let engine = ObservationEngine::new();
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("async_span", "all")
        .unwrap();
    engine.add_channel_sink("all", Arc::new(JsonlSink::new(EndFailure)));
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("all", memory.clone());
    let span = engine.span_if_active(|| Work).unwrap();
    engine.flush_channels().unwrap();
    span.finish().unwrap();
    assert!(engine.flush_channels().is_err());
    assert_eq!(engine.sink_error_count(), 1);
    assert_eq!(memory.len(), 2);
}

#[test]
fn retention_can_remove_span_start_without_splitting_newest_end() {
    use crate::{Observation, Phase};
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "retained_span::work")]
    struct Work;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("span.jsonl");
    let sink = Arc::new(
        JsonlSink::with_config(&path, JsonlConfig::new().max_file_size_bytes(1024)).unwrap(),
    );
    let engine = ObservationEngine::new();
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
    assert!(text.len() > 1024);
    assert!(text.ends_with('\n'));
    assert_eq!(
        serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap(),
        big
    );
    span.finish().unwrap();
    engine.flush_channels().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.len() <= 1024);
    assert!(text.ends_with('\n'));
    let records: Vec<ObservationRecord> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].phase, Some(Phase::End));
    assert!(records[0].data.is_null());
}

#[test]
fn retention_accepts_valid_deep_payload_without_an_implicit_format_depth_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep.jsonl");
    let config = JsonlConfig::new().max_file_size_bytes(4096);
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
    assert!(fs::read_to_string(&path).unwrap().starts_with(&original));
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
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
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backlog.jsonl");
    let (entered, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let workers = Arc::new(Mutex::new(Vec::new()));
    let sink = JsonlSink::new(GateFile {
        file: fs::File::create(&path).unwrap(),
        entered: Some(entered),
        release: resume,
        producer: std::thread::current().id(),
        workers: workers.clone(),
        chunks: chunks.clone(),
    });
    assert!(!sink.is_started());
    sink.flush().unwrap();
    assert!(!sink.is_started());
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
    assert!(bytes.ends_with('\n'));
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
    assert_eq!(actual, (0..n).collect::<Vec<_>>());
    assert!(sink.stats().batches > (n as u64 - 1).div_ceil(8192));
    assert!(chunks.lock().unwrap().iter().all(|&n| n <= 8192));
    let ids = workers.lock().unwrap();
    assert!(ids.iter().all(|id| *id == ids[0]));
    drop(ids);
    for i in n..n + 20000 {
        event.data = serde_json::json!(i);
        sink.write(&event).unwrap();
    }
    drop(sink);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), n + 20000);
    assert_eq!(
        serde_json::from_str::<ObservationRecord>(text.lines().last().unwrap())
            .unwrap()
            .data,
        n + 19999
    );
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
    for fault in [Fault::Write, Fault::Flush, Fault::Panic] {
        let calls = Arc::new(Mutex::new((0, 0)));
        let sink = Arc::new(JsonlSink::new(FaultWriter {
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
                assert!(h.join().unwrap().is_err());
            }
        });
        let first = sink.last_error().unwrap().to_string();
        let counts = *calls.lock().unwrap();
        assert!(sink.write(&record("fault::later")).is_err());
        assert!(sink.flush().is_err());
        assert_eq!(sink.last_error().unwrap().to_string(), first);
        assert_eq!(*calls.lock().unwrap(), counts);
        assert_eq!(counts.0, 1);
        assert_eq!(counts.1, usize::from(matches!(fault, Fault::Flush)));
        drop(sink);
    }
}
#[test]
fn engine_error_barrier_counts_checked_operations_and_healthy_fanout_survives() {
    use crate::Observation;
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "fault::engine")]
    struct Event;
    let engine = ObservationEngine::new();
    let directory = tempfile::tempdir().unwrap();
    let broken = Arc::new(JsonlSink::append(directory.path().join("missing/file")).unwrap());
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("fault", "all").unwrap();
    engine.add_channel_sink("all", broken.clone());
    engine.add_channel_sink("all", memory.clone());
    let initial = engine.observe_if_active(|| Event);
    assert_eq!(memory.len(), 1);
    assert_eq!(engine.sink_error_count(), u64::from(initial.is_err()));
    assert!(engine.flush_channels().is_err());
    let errors = engine.sink_error_count();
    assert!(broken.last_error().is_some());
    assert!(engine.observe_if_active(|| Event).is_err());
    assert_eq!(memory.len(), 2);
    assert_eq!(engine.sink_error_count(), errors + 1);
    assert!(engine.flush_channels().is_err());
    assert_eq!(engine.sink_error_count(), errors + 2);
}
#[test]
fn retention_is_a_newest_complete_suffix_across_cycles_and_rejects_corrupt_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("suffix.jsonl");
    let config = JsonlConfig::new()
        .max_file_size_bytes(4096)
        .trim_fraction(0.25);
    let sink = JsonlSink::with_config(&path, config.clone()).unwrap();
    let mut history = Vec::new();
    for i in 0..300 {
        let mut event = record("retention::event");
        event.data = serde_json::json!({"index": i, "text": "日本語\n\\\"".repeat(8)});
        history.push(event.clone());
        sink.write(&event).unwrap();
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
        let values: Vec<ObservationRecord> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(values, history[history.len() - values.len()..]);
        assert_eq!(values.last(), history.last());
        assert!(text.len() <= 4096);
    }
    assert!(sink.stats().compactions > 10);
    drop(sink);
    for corrupt in ["{\"valid\":true}\n{", "not json\n", "{\"valid\":true}\r\n"] {
        fs::write(&path, corrupt).unwrap();
        let sink = JsonlSink::with_config(&path, config.clone()).unwrap();
        let mut event = record("retention::new");
        event.data = serde_json::json!("x".repeat(8192));
        let _ = sink.write(&event);
        assert!(sink.flush().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
    }
}
