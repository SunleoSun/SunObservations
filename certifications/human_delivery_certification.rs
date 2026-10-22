use serde::Serialize;
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use sun_observations::certifications::*;
use sun_observations::{
    ConsoleSink, JsonlSink, MemorySink, Observation, ObservationDelivery, ObservationEngine,
    ObservationRecord, Selection, Sink,
};

#[derive(Default)]
struct Legacy(Mutex<Vec<ObservationRecord>>);
impl Sink for Legacy {
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.0.lock().unwrap().push(record.clone());
        Ok(())
    }
}
#[derive(Default)]
struct Human(Mutex<Vec<ObservationDelivery>>);
impl Sink for Human {
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        panic!("expected delivery")
    }
    fn wants_render_text(&self) -> bool {
        true
    }
    fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()> {
        self.0.lock().unwrap().push(delivery.clone());
        Ok(())
    }
}
#[derive(Serialize)]
struct TableStart {
    columns: [&'static str; 2],
}
impl Observation for TableStart {
    const NAMESPACE: &'static str = "table::start";
    fn render_text(&self) -> Option<String> {
        Some("Epoch | Loss\n------+-------".into())
    }
}
#[derive(Serialize)]
struct TableRow {
    epoch: u64,
    loss: f64,
    #[serde(skip)]
    renders: Arc<AtomicUsize>,
}
impl Observation for TableRow {
    const NAMESPACE: &'static str = "table::row";
    fn render_text(&self) -> Option<String> {
        self.renders.fetch_add(1, Ordering::Relaxed);
        Some(format!("{:>5} | {:.4}", self.epoch, self.loss))
    }
}
#[derive(Serialize)]
struct TableEnd {
    epochs: u64,
}
impl Observation for TableEnd {
    const NAMESPACE: &'static str = "table::end";
    fn render_text(&self) -> Option<String> {
        Some("------+-------".into())
    }
}

#[test]
fn public_table_stream_shares_render_and_record_without_changing_v1_or_legacy_sinks() {
    let directory = tempfile::tempdir().unwrap();
    let jsonl = directory.path().join("machine.jsonl");
    let text = directory.path().join("human.txt");
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("table", "all").unwrap();
    let legacy = Arc::new(Legacy::default());
    let memory = Arc::new(MemorySink::new());
    let humans = [Arc::new(Human::default()), Arc::new(Human::default())];
    engine.add_channel_sink("all", legacy.clone());
    engine.add_channel_sink("all", memory.clone());
    engine.add_channel_sink("all", Arc::new(JsonlSink::open_append(&jsonl).unwrap()));
    engine.add_channel_sink(
        "all",
        Arc::new(ConsoleSink::new(std::fs::File::create(&text).unwrap())),
    );
    for sink in &humans {
        engine.add_channel_sink("all", sink.clone());
    }
    let renders = Arc::new(AtomicUsize::new(0));
    engine
        .observe_if_active(|| TableStart {
            columns: ["epoch", "loss"],
        })
        .unwrap();
    for epoch in 1..=2 {
        engine
            .observe_if_active(|| TableRow {
                epoch,
                loss: 0.25 / epoch as f64,
                renders: renders.clone(),
            })
            .unwrap();
    }
    engine.observe_if_active(|| TableEnd { epochs: 2 }).unwrap();
    engine.flush_channels().unwrap();
    let rendered = renders.load(Ordering::Relaxed);
    let human_text = std::fs::read_to_string(&text).unwrap();
    let raw = std::fs::read_to_string(&jsonl).unwrap();
    let records = sun_observations_certifications::read(&jsonl);
    let legacy_matches_memory = legacy.0.lock().unwrap().as_slice() == memory.records();
    let a = humans[0].0.lock().unwrap();
    let b = humans[1].0.lock().unwrap();
    let shared_record_mismatches = (0..4)
        .filter(|&index| !Arc::ptr_eq(a[index].shared_record(), b[index].shared_record()))
        .count();
    let shared_render_mismatches = (0..4)
        .filter(
            |&index| match (a[index].shared_render_text(), b[index].shared_render_text()) {
                (Some(left), Some(right)) => !Arc::ptr_eq(left, right),
                _ => true,
            },
        )
        .count();
    let memory_records = memory.shared_records();
    let memory_identity_mismatches = (0..4)
        .filter(|&index| !Arc::ptr_eq(a[index].shared_record(), &memory_records[index]))
        .count();
    let persisted_mismatches = (0..4)
        .filter(|&index| records[index] != serde_json::to_value(a[index].record()).unwrap())
        .count();
    let lifecycle_fields_present = a
        .iter()
        .filter(|delivery| delivery.record().phase.is_some() || delivery.record().span.is_some())
        .count();

    let newline = char::from(10);
    let expected_human = format!(
        "Epoch | Loss{newline}------+-------{newline}{:>5} | {:.4}{newline}{:>5} | {:.4}{newline}------+-------{newline}",
        1, 0.25, 2, 0.125
    );
    certification("human rendering preserves structured evidence and shared delivery identity")
        .check("render once per row", fact("renderer", rendered).expect(2))
        .check(
            "human text",
            fact("console output", human_text).expect(expected_human),
        )
        .check(
            "render text absent from JSONL",
            fact(
                "jsonl",
                raw.contains("Epoch | Loss") || raw.contains("------+-------"),
            )
            .expect(false),
        )
        .check("record count", fact("jsonl", records.len()).expect(4))
        .check(
            "legacy and memory payloads",
            fact("structured sinks", legacy_matches_memory).expect(true),
        )
        .check(
            "human sinks share records",
            fact("human sinks", shared_record_mismatches).expect(0),
        )
        .check(
            "human sinks share rendered text",
            fact("human sinks", shared_render_mismatches).expect(0),
        )
        .check(
            "memory and human share records",
            fact("memory delivery", memory_identity_mismatches).expect(0),
        )
        .check(
            "persisted records equal delivered records",
            fact("persisted delivery", persisted_mismatches).expect(0),
        )
        .check(
            "point lifecycle fields absent",
            fact("point records", lifecycle_fields_present).expect(0),
        )
        .assert_certify();
}

#[test]
fn custom_human_sink_receives_absent_renderer_and_span_lifecycle_without_invented_text() {
    #[derive(Serialize, Observation)]
    #[observation(namespace = "table::plain")]
    struct Plain {
        value: u64,
    }
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("table", "all").unwrap();
    let human = Arc::new(Human::default());
    engine.add_channel_sink("all", human.clone());
    engine.observe_if_active(|| Plain { value: 7 }).unwrap();
    engine
        .span_if_active(|| Plain { value: 8 })
        .unwrap()
        .finish()
        .unwrap();
    let records = human.0.lock().unwrap();
    let evidence = collection("human deliveries", records.as_slice());
    certification("human sink does not invent render text for points or span lifecycle")
        .check("delivery count", evidence.count().expect(3))
        .check(
            "render text absent",
            evidence
                .matching_count(|delivery| delivery.render_text().is_some())
                .expect(0),
        )
        .check(
            "span end data is null",
            fact("span end", records[2].record().data.is_null()).expect(true),
        )
        .assert_certify();
}
