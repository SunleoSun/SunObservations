use super::*;
use crate::{support_test::record, Phase};
use std::{fs, sync::Arc};

#[test]
fn append_preserves_existing_lines_and_schema_roundtrips() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observations.jsonl");
    let point = record("jsonl::point");
    let mut span = point.clone();
    span.scope = Some("run-a".into());
    span.phase = Some(Phase::Start);
    span.span = Some("span-a".into());
    span.message = Some("line one\nline two".into());
    JsonlSink::append(&path).unwrap().write(&point).unwrap();
    let sink = JsonlSink::append(&path).unwrap();
    sink.write(&span).unwrap();
    sink.flush().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.ends_with('\n'));
    let records: Vec<ObservationRecord> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records, [point, span]);
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        for key in ["ts", "proc", "proc_ms", "namespace", "level", "data"] {
            assert!(value.get(key).is_some());
        }
        for key in ["tags", "channel", "channels"] {
            assert!(value.get(key).is_none());
        }
    }
}

#[test]
fn concurrent_lines_are_complete_json_objects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("parallel.jsonl");
    let sink = Arc::new(JsonlSink::append(&path).unwrap());
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
    assert_eq!(text.lines().count(), 600);
    assert_eq!(pairs.len(), 600);
}

#[test]
fn engine_output_matches_envelope_without_routing_metadata() {
    use crate::{start_scope, Observation, ObservationEngine, ScopeConfig, Selection};
    #[derive(serde::Serialize, Observation)]
    #[observation(namespace = "jsonl::engine::event", level = ERROR, tags("expensive"))]
    struct Event {
        value: usize,
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("engine.jsonl");
    let engine = ObservationEngine::new();
    engine.add_channel_sink("logical", Arc::new(JsonlSink::append(&path).unwrap()));
    engine
        .register_namespace_channel("jsonl::engine", "logical")
        .unwrap();
    engine.set_selection(Selection::new().activate_namespace("jsonl::engine"));
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
    assert_eq!(records.len(), 4);
    assert_eq!(records[0].scope.as_deref(), Some("persisted-run"));
    assert_eq!(records[0].data, serde_json::json!({"value": 7}));
    assert_eq!(records[1].span, records[2].span);
    assert_eq!(records[1].phase, Some(Phase::Start));
    assert_eq!(records[2].phase, Some(Phase::End));
    assert!(records[2].data.is_null());
    assert_eq!(records[3].scope, None);
    for record in &records {
        assert_eq!(record.proc, engine.process_id());
        assert_eq!(record.namespace, Event::NAMESPACE);
        assert_eq!(record.level, crate::ObservationLevel::ERROR);
        assert!(record.ts.ends_with('Z'));
        assert_eq!(record.ts.len(), 24);
        assert!(chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok());
    }
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        for absent in ["tags", "channel", "channels", "message"] {
            assert!(value.get(absent).is_none());
        }
    }
}

#[test]
fn lazy_worker_and_periodic_flush_run_off_producer_without_sleeps() {
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
        JsonlConfig::new().flush_ms(10),
    )
    .unwrap();
    assert!(!sink.is_started());
    sink.flush().unwrap();
    assert!(!sink.is_started());
    sink.write(&record("jsonl::periodic")).unwrap();
    assert!(sink.is_started());
    assert!(receiver.recv_timeout(Duration::from_secs(3)).unwrap() > 0);
    assert_eq!(sink.stats().records_written, 1);
}

#[test]
fn file_open_is_lazy_and_background_failure_is_sticky() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-yet.jsonl");
    let sink = JsonlSink::append(&path).unwrap();
    assert!(!path.exists());
    sink.write(&record("jsonl::lazy_file")).unwrap();
    sink.flush().unwrap();
    assert!(path.exists());
    let invalid = JsonlSink::append(directory.path().join("missing").join("file.jsonl")).unwrap();
    let _ = invalid.write(&record("jsonl::bad_path"));
    assert!(invalid.flush().is_err());
    assert!(invalid.last_error().is_some());
    assert!(invalid.write(&record("jsonl::bad_path")).is_err());
}

#[test]
fn normal_drop_drains_all_accepted_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drop.jsonl");
    {
        let sink = Arc::new(JsonlSink::append(&path).unwrap());
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
    assert_eq!(fs::read_to_string(path).unwrap().lines().count(), 1000);
}

#[test]
fn concurrent_flush_barriers_do_not_deadlock_or_lose_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("barriers.jsonl");
    let sink = Arc::new(JsonlSink::append(&path).unwrap());
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
    assert_eq!(fs::read_to_string(path).unwrap().lines().count(), 160);
}

#[test]
fn repeated_compaction_retains_complete_newest_lines_with_hysteresis() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("trim.jsonl");
    let sink = JsonlSink::with_config(
        &path,
        JsonlConfig::new()
            .max_file_size_bytes(4096)
            .trim_fraction(0.25),
    )
    .unwrap();
    let mut observed = 0;
    for cycle in 0..8 {
        for item in 0..30 {
            let mut event = record("jsonl::trim");
            event.data = serde_json::json!({"item": cycle * 30 + item, "padding": "x".repeat(64)});
            sink.write(&event).unwrap();
        }
        sink.flush().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
        let values: Vec<ObservationRecord> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(values.last().unwrap().data["item"], cycle * 30 + 29);
        assert!(values[0].data["item"].as_u64().unwrap() > 0);
        assert!(text.len() <= 4096);
        let compactions = sink.stats().compactions;
        if compactions > observed {
            assert!(text.len() >= 3072 - 512);
            observed = compactions;
        }
    }
    assert!(observed >= 8);
}

#[test]
fn threshold_crossing_trims_to_target_not_just_to_maximum() {
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
    assert!(original.len() > 4096);
    fs::write(&path, original).unwrap();
    let sink = JsonlSink::with_config(
        &path,
        JsonlConfig::new()
            .max_file_size_bytes(4096)
            .trim_fraction(0.25),
    )
    .unwrap();
    let mut newest = record("jsonl::hysteresis");
    newest.data = serde_json::json!({"item": 100});
    sink.write(&newest).unwrap();
    sink.flush().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.len() <= 3072 && text.len() >= 3072 - longest - 1);
    let lines: Vec<ObservationRecord> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines[0].data["item"].as_u64().unwrap() > 0);
    assert_eq!(lines.last().unwrap().data["item"], 100);
    assert_eq!(sink.stats().compactions, 1);
    sink.write(&newest).unwrap();
    sink.flush().unwrap();
    assert_eq!(sink.stats().compactions, 1);
}

#[test]
fn oversized_newest_record_is_never_split_or_deleted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("oversize.jsonl");
    let sink = JsonlSink::with_config(&path, JsonlConfig::new().max_file_size_bytes(512)).unwrap();
    sink.write(&record("jsonl::small")).unwrap();
    let mut big = record("jsonl::big");
    big.data = serde_json::json!({"padding": "x".repeat(4096)});
    sink.write(&big).unwrap();
    sink.flush().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 1);
    assert!(text.ends_with('\n'));
    assert_eq!(
        serde_json::from_str::<ObservationRecord>(text.trim_end()).unwrap(),
        big
    );
    sink.write(&record("jsonl::newest")).unwrap();
    sink.flush().unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.len() <= 512);
    assert_eq!(
        serde_json::from_str::<ObservationRecord>(text.trim_end())
            .unwrap()
            .namespace,
        "jsonl::newest"
    );
}

#[test]
fn writer_panics_are_observable_and_do_not_deadlock_flush_or_drop() {
    struct Panics;
    impl Write for Panics {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            panic!("writer panic");
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let sink = JsonlSink::new(Panics);
    let _ = sink.write(&record("jsonl::panic"));
    assert!(sink.flush().is_err());
    assert!(sink.last_error().unwrap().to_string().contains("panicked"));
}

#[test]
fn invalid_file_settings_and_writer_retention_are_rejected() {
    for config in [
        JsonlConfig::new().flush_ms(0),
        JsonlConfig::new().max_file_size_bytes(0),
        JsonlConfig::new().trim_fraction(1.0),
        JsonlConfig::new().trim_fraction(f64::NAN),
    ] {
        assert!(JsonlSink::with_config("unused.jsonl", config).is_err());
    }
    assert!(
        JsonlSink::writer_with_config(Vec::<u8>::new(), JsonlConfig::new().file_size_mb(1))
            .is_err()
    );
}

#[test]
fn writer_failures_are_reported() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("broken"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("broken"))
        }
    }
    let sink = JsonlSink::new(Broken);
    let _ = sink.write(&record("jsonl::broken"));
    assert!(sink.flush().is_err());
    assert!(sink.last_error().is_some());
    assert!(sink.write(&record("jsonl::broken")).is_err());
}
