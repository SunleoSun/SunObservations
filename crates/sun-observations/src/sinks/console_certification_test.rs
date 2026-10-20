use super::*;
use crate::support_test::record;
use std::{
    sync::mpsc,
    thread::{self, ThreadId},
    time::Duration,
};

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    threads: Vec<ThreadId>,
    flush_lengths: Vec<usize>,
}
struct Writer {
    capture: Arc<Mutex<Capture>>,
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some((entered, release)) = self.gate.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let mut capture = self.capture.lock();
        capture.bytes.extend_from_slice(bytes);
        capture.threads.push(thread::current().id());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut capture = self.capture.lock();
        let len = capture.bytes.len();
        capture.flush_lengths.push(len);
        Ok(())
    }
}
fn delivery(text: &str) -> ObservationDelivery {
    ObservationDelivery::new(
        Arc::new(record("console::test")),
        Some(Arc::new(text.into())),
    )
}

#[test]
fn lazy_targets_default_format_and_literal_multiline_text() {
    for sink in [ConsoleSink::stdout(), ConsoleSink::stderr()] {
        assert!(!sink.is_started());
        sink.flush().unwrap();
        assert!(!sink.is_started());
        assert!(sink.wants_render_text());
    }
    let capture = Arc::new(Mutex::new(Capture::default()));
    let sink = ConsoleSink::new(Writer {
        capture: capture.clone(),
        gate: None,
    });
    let mut value = record("console::test");
    value.scope = Some("training".into());
    value.message = Some("semantic\ncontext".into());
    value.phase = Some(Phase::Start);
    value.span = Some("id".into());
    sink.write(&value).unwrap();
    sink.write_delivery(&delivery("header\nrow\nfooter"))
        .unwrap();
    sink.write_delivery(&delivery("already terminated\n"))
        .unwrap();
    sink.write_delivery(&delivery("")).unwrap();
    sink.flush().unwrap();
    assert_eq!(
        String::from_utf8(capture.lock().bytes.clone()).unwrap(),
        concat!(
            "2026-10-20T00:00:00.000Z INFO [training] console::test phase=start span=id ",
            "message=\"semantic\\ncontext\" data={\"value\":7}\n",
            "header\nrow\nfooter\nalready terminated\n\n"
        )
    );
}

#[test]
fn gated_backlog_fifo_flush_boundary_and_drop_drain_off_producer() {
    let capture = Arc::new(Mutex::new(Capture::default()));
    let (entered, observed) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let sink = ConsoleSink::new(Writer {
        capture: capture.clone(),
        gate: Some((entered, wait)),
    });
    sink.write_delivery(&delivery("first")).unwrap();
    observed.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut expected = "first\n".to_owned();
    for index in 0..MAX_BATCH_MESSAGES * 4 {
        let text = format!("{index}\ncontinued");
        sink.write_delivery(&delivery(&text)).unwrap();
        expected.push_str(&text);
        expected.push('\n');
    }
    let (reply, completed) = unbounded();
    sink.start()
        .unwrap()
        .send(Message::Flush(reply), &sink.status)
        .unwrap();
    let boundary = expected.len();
    sink.write_delivery(&delivery("after barrier")).unwrap();
    assert!(completed.try_recv().is_err());
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(capture.lock().flush_lengths[0], boundary);
    expected.push_str("after barrier\n");
    drop(sink);
    let capture = capture.lock();
    assert_eq!(capture.bytes, expected.as_bytes());
    assert_eq!(capture.flush_lengths.last(), Some(&expected.len()));
    assert!(!capture.threads.is_empty());
    let worker = capture.threads[0];
    assert_ne!(worker, thread::current().id());
    assert!(capture.threads.iter().all(|id| *id == worker));
}

#[test]
fn concurrent_producers_preserve_each_producer_order_and_multiline_record_integrity() {
    let capture = Arc::new(Mutex::new(Capture::default()));
    let sink = ConsoleSink::new(Writer {
        capture: capture.clone(),
        gate: None,
    });
    let barrier = std::sync::Barrier::new(16);
    thread::scope(|scope| {
        for producer in 0..16 {
            let sink = &sink;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for index in 0..1000 {
                    sink.write_delivery(&delivery(&format!(
                        "{producer},{index}\ncontinued {producer},{index}"
                    )))
                    .unwrap();
                }
            });
        }
    });
    sink.flush().unwrap();
    let text = String::from_utf8(capture.lock().bytes.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 32_000);
    let mut next = [0; 16];
    for pair in lines.chunks_exact(2) {
        let (producer, index) = pair[0].split_once(',').unwrap();
        let producer: usize = producer.parse().unwrap();
        let index: usize = index.parse().unwrap();
        assert_eq!(next[producer], index);
        next[producer] += 1;
        assert_eq!(pair[1], format!("continued {}", pair[0]));
    }
    assert_eq!(next, [1000; 16]);
}

#[derive(Clone, Copy)]
enum Fault {
    Write,
    Flush,
    Panic,
}
struct Broken(Fault);
impl Write for Broken {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.0 {
            Fault::Write => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "write failed",
            )),
            Fault::Panic => panic!("writer panic"),
            Fault::Flush => Ok(bytes.len()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}
#[test]
fn write_flush_and_panic_failures_are_sticky_and_release_all_barriers() {
    for fault in [Fault::Write, Fault::Flush, Fault::Panic] {
        let sink = Arc::new(ConsoleSink::new(Broken(fault)));
        sink.write_delivery(&delivery("failure")).unwrap();
        let first = sink.flush().unwrap_err();
        assert_eq!(sink.last_error().unwrap().to_string(), first.to_string());
        thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    assert_eq!(sink.flush().unwrap_err().to_string(), first.to_string());
                    assert_eq!(
                        sink.write_delivery(&delivery("no retry"))
                            .unwrap_err()
                            .to_string(),
                        first.to_string()
                    );
                });
            }
        });
        drop(sink);
    }
}

struct GatedBroken {
    fault: Fault,
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Write for GatedBroken {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        Broken(self.fault).write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Broken(self.fault).flush()
    }
}
#[test]
fn failure_releases_concurrent_already_queued_public_flush_barriers() {
    for fault in [Fault::Write, Fault::Flush, Fault::Panic] {
        let (entered, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let sink = ConsoleSink::new(GatedBroken {
            fault,
            entered,
            release: wait,
        });
        sink.write_delivery(&delivery("fail with pending barriers"))
            .unwrap();
        observed.recv_timeout(Duration::from_secs(10)).unwrap();
        thread::scope(|scope| {
            let mut flushers = Vec::new();
            for _ in 0..16 {
                flushers.push(scope.spawn(|| sink.flush()));
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let worker = sink.start().unwrap();
            while worker.sender.as_ref().unwrap().len() < 16 {
                assert!(std::time::Instant::now() < deadline);
                thread::yield_now();
            }
            release.send(()).unwrap();
            for flusher in flushers {
                let error = flusher.join().unwrap().unwrap_err();
                assert_eq!(error.to_string(), sink.last_error().unwrap().to_string());
            }
        });
    }
}

#[test]
fn failed_console_does_not_prevent_healthy_structured_fanout() {
    let engine = crate::ObservationEngine::new();
    engine.set_selection(crate::Selection::new().activate_all());
    let sink = Arc::new(ConsoleSink::new(Broken(Fault::Write)));
    let memory = Arc::new(crate::MemorySink::new());
    engine.add_channel_sink("all", sink);
    engine.add_channel_sink("all", memory.clone());
    engine.register_namespace_channel("console", "all").unwrap();
    #[derive(serde::Serialize, crate::Observation)]
    #[observation(namespace = "console::event")]
    struct Event;
    engine.observe_if_active(|| Event).unwrap();
    assert!(engine.flush_channels().is_err());
    assert!(engine.observe_if_active(|| Event).is_err());
    assert_eq!(engine.sink_error_count(), 2);
    assert_eq!(memory.len(), 2);
}
