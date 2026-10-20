mod support;

use serde::Serialize;
use std::{
    sync::{Arc, Barrier},
    time::Instant,
};
use sun_observations::{JsonlSink, Observation, ObservationEngine, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "batch::event")]
struct Event {
    producer: usize,
    sequence: usize,
    value: f64,
}
#[derive(Serialize, Observation)]
#[observation(namespace = "batch::rich")]
struct Rich {
    event: Event,
    text: String,
    values: [f64; 16],
}

fn run(producers: usize, rich: bool, sample: usize) {
    let directory = support::tempdir().unwrap();
    let path = directory.path().join("batch.jsonl");
    let engine = ObservationEngine::new();
    let sink = Arc::new(JsonlSink::append(&path).unwrap());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("batch", "file").unwrap();
    engine.add_channel_sink("file", sink.clone());
    engine
        .observe_if_active(|| Event {
            producer: 0,
            sequence: 0,
            value: 0.5,
        })
        .unwrap();
    engine.flush_channels().unwrap();
    let n = 262144;
    let barrier = Barrier::new(producers + 1);
    let start = Instant::now();
    let enqueue = std::thread::scope(|threads| {
        let mut handles = Vec::new();
        for producer in 0..producers {
            let engine = &engine;
            let barrier = &barrier;
            handles.push(threads.spawn(move || {
                barrier.wait();
                for sequence in 0..n / producers {
                    let event = || Event {
                        producer,
                        sequence,
                        value: 0.5,
                    };
                    if rich {
                        engine
                            .observe_if_active(|| Rich {
                                event: event(),
                                text: "Unicode λ 日本語 \n \" \\".repeat(3),
                                values: [0.125; 16],
                            })
                            .unwrap();
                    } else {
                        engine.observe_if_active(event).unwrap();
                    }
                }
            }));
        }
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        start.elapsed()
    });
    let pending = n + 1 - sink.stats().records_written as usize;
    engine.flush_channels().unwrap();
    let total = start.elapsed();
    println!(
        "{sample},{producers},{rich},{n},{:.2},{:.2},{:.0},{pending},{}",
        enqueue.as_nanos() as f64 / n as f64,
        total.as_nanos() as f64 / n as f64,
        n as f64 / total.as_secs_f64(),
        sink.stats().batches
    );
    assert_eq!(sink.stats().records_written as usize, n + 1);
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.ends_with('\n'));
    assert_eq!(text.lines().count(), n + 1);
    for line in text.lines() {
        let _: serde_json::Value = serde_json::from_str(line).unwrap();
    }
}
fn main() {
    support::run(benchmark);
}

fn benchmark() {
    println!(
        "sample,producers,rich,records,producer_ns,e2e_ns,records_per_s,not_yet_written,batches"
    );
    for sample in 0..5 {
        for producers in [1, 4, 16] {
            for rich in [false, true] {
                run(producers, rich, sample);
            }
        }
    }
}
