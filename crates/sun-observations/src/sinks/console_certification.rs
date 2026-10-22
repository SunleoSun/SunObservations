use super::*;
use crate::test_support::record;
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
    let mut certification =
        sun_certifications::certify("lazy targets default format and literal multiline text");
    for sink in [ConsoleSink::stdout(), ConsoleSink::stderr()] {
        certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
        sink.flush().unwrap();
        certification.check_fact_expect("sink.is_started()", sink.is_started(), false);
        certification.check_fact_expect("sink.wants_render_text()", sink.wants_render_text(), true);
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
    certification.check_fact_expect(
        "String::from_utf8(capture.lock().bytes.clone()).unwrap()",
        String::from_utf8(capture.lock().bytes.clone())
            .unwrap()
            .as_str(),
        concat!(
            "2026-10-20T00:00:00.000Z INFO [training] console::test phase=start span=id ",
            "message=\"semantic\\ncontext\" data={\"value\":7}\n",
            "header\nrow\nfooter\nalready terminated\n\n"
        ),
    );
    certification.assert_certify();
}

#[test]
fn gated_backlog_fifo_flush_boundary_and_drop_drain_off_producer() {
    let mut certification = sun_certifications::certify(
        "gated backlog fifo flush boundary and drop drain off producer",
    );
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
    certification.check_fact_expect(
        "completed.try_recv().is_err()",
        completed.try_recv().is_err(),
        true,
    );
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(10)).unwrap();
    certification.check_fact_expect(
        "capture.lock().flush_lengths[0]",
        &(capture.lock().flush_lengths[0]),
        &(boundary),
    );
    expected.push_str("after barrier\n");
    drop(sink);
    let capture = capture.lock();
    certification.check_fact_expect(
        "capture.bytes",
        capture.bytes.as_slice(),
        expected.as_bytes(),
    );
    certification.check_fact_expect(
        "capture.flush_lengths.last()",
        &(capture.flush_lengths.last()),
        &(Some(&expected.len())),
    );
    certification.check_fact_expect(
        "capture.threads.is_empty()",
        capture.threads.is_empty(),
        false,
    );
    let worker = capture.threads[0];
    certification.check_fact_expect(
        "worker",
        &(worker),
        sun_certifications::NotEqual(&(thread::current().id())),
    );
    certification.check_fact_expect(
        "capture.threads.iter().all(|id| *id == worker)",
        capture.threads.iter().all(|id| *id == worker),
        true,
    );
    certification.assert_certify();
}

#[test]
fn concurrent_producers_preserve_each_producer_order_and_multiline_record_integrity() {
    let mut certification = sun_certifications::certify(
        "concurrent producers preserve each producer order and multiline record integrity",
    );
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
    certification.check_fact_expect("lines.len()", &(lines.len()), &(32_000));
    let mut next = [0; 16];
    for pair in lines.chunks_exact(2) {
        let (producer, index) = pair[0].split_once(',').unwrap();
        let producer: usize = producer.parse().unwrap();
        let index: usize = index.parse().unwrap();
        certification.check_fact_expect("next[producer]", &(next[producer]), &(index));
        next[producer] += 1;
        certification.check_fact_expect(
            "pair[1]",
            pair[1],
            format!("continued {}", pair[0]).as_str(),
        );
    }
    certification.check_fact_expect("next", &(next), &([1000; 16]));
    certification.assert_certify();
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
    let mut certification = sun_certifications::certify(
        "write flush and panic failures are sticky and release all barriers",
    );
    for fault in [Fault::Write, Fault::Flush, Fault::Panic] {
        let sink = Arc::new(ConsoleSink::new(Broken(fault)));
        sink.write_delivery(&delivery("failure")).unwrap();
        let first = sink.flush().unwrap_err();
        certification.check_fact_expect(
            "sink.last_error().unwrap().to_string()",
            &(sink.last_error().unwrap().to_string()),
            &(first.to_string()),
        );
        let outcomes = thread::scope(|scope| {
            let mut workers = Vec::new();
            for _ in 0..16 {
                workers.push(scope.spawn(|| {
                    let mut certification =
                        sun_certifications::certify("concurrent sticky failure");
                    certification.check_fact_expect(
                        "sink.flush().unwrap_err().to_string()",
                        &(sink.flush().unwrap_err().to_string()),
                        &(first.to_string()),
                    );
                    certification.check_fact_expect(
                        "sink.write_delivery(&delivery(\"no retry\")) .unwrap_err() .to_string()",
                        &(sink
                            .write_delivery(&delivery("no retry"))
                            .unwrap_err()
                            .to_string()),
                        &(first.to_string()),
                    );
                    certification.certify()
                }));
            }
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        for report in outcomes {
            certification.check(
                "concurrent sticky failure",
                sun_certifications::fact("worker", report.is_pass())
                    .details(report.to_string())
                    .expect(true),
            );
        }
        drop(sink);
    }
    certification.assert_certify();
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
    let mut certification = sun_certifications::certify(
        "failure releases concurrent already queued public flush barriers",
    );
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
                certification.check_fact_expect(
                    "error.to_string()",
                    &(error.to_string()),
                    &(sink.last_error().unwrap().to_string()),
                );
            }
        });
    }
    certification.assert_certify();
}

#[test]
fn failed_console_does_not_prevent_healthy_structured_fanout() {
    let mut certification =
        sun_certifications::certify("failed console does not prevent healthy structured fanout");
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
    certification.check_fact_expect(
        "engine.flush_channels().is_err()",
        engine.flush_channels().is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).is_err()",
        engine.observe_if_active(|| Event).is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(2),
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(2));
    certification.assert_certify();
}
