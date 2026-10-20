use obs::*;
use serde::Serialize;
use std::sync::{Arc, Barrier};
#[derive(Serialize, Observation)]
#[observation(namespace = "fifo::event")]
struct Event {
    producer: usize,
    sequence: usize,
}
#[test]
fn real_file_multi_producer_integrity_preserves_each_producer_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fifo.jsonl");
    let engine = ObservationEngine::new();
    let sink = Arc::new(JsonlSink::append(&path).unwrap());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("fifo", "file").unwrap();
    engine.add_channel_sink("file", sink.clone());
    let barrier = Barrier::new(16);
    std::thread::scope(|threads| {
        for producer in 0..16 {
            let engine = &engine;
            let barrier = &barrier;
            threads.spawn(move || {
                barrier.wait();
                for sequence in 0..3000 {
                    engine
                        .observe_if_active(|| Event { producer, sequence })
                        .unwrap();
                }
            });
        }
    });
    engine.flush_channels().unwrap();
    let records = sun_observations_certifications::read(&path);
    assert_eq!(records.len(), 48000);
    for producer in 0..16 {
        let sequences: Vec<_> = records
            .iter()
            .filter(|r| r["data"]["producer"] == producer)
            .map(|r| r["data"]["sequence"].as_u64().unwrap())
            .collect();
        assert_eq!(sequences, (0..3000).collect::<Vec<_>>());
    }
    assert_eq!(sink.stats().records_written, 48000);
}
