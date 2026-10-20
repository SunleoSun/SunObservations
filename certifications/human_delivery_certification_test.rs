use obs::{
    ConsoleSink, JsonlSink, MemorySink, Observation, ObservationDelivery, ObservationEngine,
    ObservationRecord, Selection, Sink,
};
use serde::Serialize;
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Default)]
struct Legacy(Mutex<Vec<ObservationRecord>>);
impl Sink for Legacy {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.0.lock().unwrap().push(record.clone());
        Ok(())
    }
}
#[derive(Default)]
struct Human(Mutex<Vec<ObservationDelivery>>);
impl Sink for Human {
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
    let engine = ObservationEngine::new();
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("table", "all").unwrap();
    let legacy = Arc::new(Legacy::default());
    let memory = Arc::new(MemorySink::new());
    let humans = [Arc::new(Human::default()), Arc::new(Human::default())];
    engine.add_channel_sink("all", legacy.clone());
    engine.add_channel_sink("all", memory.clone());
    engine.add_channel_sink("all", Arc::new(JsonlSink::append(&jsonl).unwrap()));
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
    assert_eq!(renders.load(Ordering::Relaxed), 2);
    assert_eq!(
        std::fs::read_to_string(&text).unwrap(),
        "Epoch | Loss\n------+-------\n    1 | 0.2500\n    2 | 0.1250\n------+-------\n"
    );
    let raw = std::fs::read_to_string(&jsonl).unwrap();
    assert!(!raw.contains("Epoch | Loss"));
    assert!(!raw.contains("------+-------"));
    let records = sun_observations_certifications::read(&jsonl);
    assert_eq!(records.len(), 4);
    assert_eq!(legacy.0.lock().unwrap().as_slice(), memory.records());
    let a = humans[0].0.lock().unwrap();
    let b = humans[1].0.lock().unwrap();
    for index in 0..4 {
        assert!(Arc::ptr_eq(
            a[index].shared_record(),
            b[index].shared_record()
        ));
        assert!(Arc::ptr_eq(
            a[index].shared_render_text().unwrap(),
            b[index].shared_render_text().unwrap()
        ));
        assert!(Arc::ptr_eq(
            a[index].shared_record(),
            &memory.shared_records()[index]
        ));
        assert_eq!(
            records[index],
            serde_json::to_value(a[index].record()).unwrap()
        );
        assert!(a[index].record().phase.is_none());
        assert!(a[index].record().span.is_none());
    }
}

#[test]
fn custom_human_sink_receives_absent_renderer_and_span_lifecycle_without_invented_text() {
    #[derive(Serialize, Observation)]
    #[observation(namespace = "table::plain")]
    struct Plain {
        value: u64,
    }
    let engine = ObservationEngine::new();
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
    assert_eq!(records.len(), 3);
    assert!(records
        .iter()
        .all(|delivery| delivery.render_text().is_none()));
    assert_eq!(records[2].record().data, serde_json::Value::Null);
}
