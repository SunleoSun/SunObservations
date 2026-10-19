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
    engine
        .channel("logical")
        .add_sink(Arc::new(JsonlSink::append(&path).unwrap()));
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
    engine.flush().unwrap();
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
    assert!(sink.write(&record("jsonl::broken")).is_err());
    assert!(sink.flush().is_err());
}
