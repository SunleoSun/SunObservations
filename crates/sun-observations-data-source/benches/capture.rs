#[path = "../../sun-observations/benches/support/mod.rs"]
mod support;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    hint::black_box,
    sync::Arc,
    time::{Duration, Instant},
};
use sun_certifications::*;
use sun_observations::{
    capture::ScopedCapture, observe, start_scope, MemorySink, MemorySinkConfig, Observation,
    ObservationEngine, ObservationRecord, ScopeConfig, Selection, Sink,
};
use sun_observations_data_source::*;
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "bench::capture", tags("evidence"))]
struct Event {
    row: usize,
    value: f64,
}
fn row(name: &str, n: usize, sample: usize, elapsed: Duration) {
    println!("{name},{n},{sample},{:.6}", elapsed.as_secs_f64() * 1000.);
}
fn measure<T>(name: &str, n: usize, sample: usize, op: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let output = op();
    let elapsed = start.elapsed();
    black_box(&output);
    row(name, n, sample, elapsed);
    output
}
fn emit(n: usize) {
    for row in 0..n {
        observe!(Event {
            row,
            value: row as f64
        })
        .unwrap();
    }
}
fn record() -> Arc<ObservationRecord> {
    Arc::new(ObservationRecord {
        ts: String::new(),
        proc: "bench".into(),
        proc_ms: 0,
        scope: None,
        namespace: Event::NAMESPACE.into(),
        level: sun_observations::ObservationLevel::INFO,
        data: serde_json::Value::Null,
        message: None,
        phase: None,
        span: None,
    })
}
fn benchmark() {
    println!("operation,records,sample,milliseconds");
    for n in [1000usize, 10_000, 100_000, 1_000_000] {
        for sample in 0..3 {
            let engine = ObservationEngine::new();
            let memory = Arc::new(MemorySink::new());
            engine.add_channel_sink("bench", memory.clone());
            engine.register_namespace_channel("bench", "bench").unwrap();
            engine.set_selection(Selection::new().activate_all());
            {
                let _scope = start_scope(ScopeConfig::new("capture"));
                measure("direct_configured_emission", n, sample, || {
                    for row in 0..n {
                        engine
                            .observe_if_active(|| Event {
                                row,
                                value: row as f64,
                            })
                            .unwrap();
                    }
                });
            }
            assert_eq!(memory.len(), n);
            drop(memory);
            drop(engine);
            let run =
                capture_observations(CaptureConfig::new().activate_tags(["evidence"]), || {
                    let start = Instant::now();
                    emit(n);
                    start.elapsed()
                });
            row("scoped_capture_emission", n, sample, *run.result());
            assert_eq!(run.dataset().records().len(), n);
            assert!(run.status().is_complete());
            drop(run);
            let guard =
                ScopedCapture::new(Selection::new().activate_all(), MemorySinkConfig::default());
            emit(n);
            let (dataset, status) = measure("capture_finalize_dataset", n, sample, || {
                let (records, status) = guard.finish();
                (ObservationDataset::from_shared(records), status)
            });
            assert!(status.is_complete());
            assert_eq!(dataset.records().len(), n);
            drop(dataset);
            let input = record();
            for (name, capacity) in [
                ("memory_below_limit", n),
                ("memory_repeated_compaction", (n / 10).max(1)),
            ] {
                let memory = MemorySink::with_config(MemorySinkConfig::new(capacity, 0.1).unwrap());
                measure(name, n, sample, || {
                    for _ in 0..n {
                        memory.write_shared(input.clone()).unwrap();
                    }
                });
                let stats = memory.stats();
                assert!(stats.retained <= capacity);
                assert_eq!(stats.retained as u64 + stats.evicted, n as u64);
            }
            let capacity = (n / 10).max(1);
            let batch = capacity.div_ceil(10);
            let mut vector = Vec::new();
            measure("vec_batch_control", n, sample, || {
                for _ in 0..n {
                    if vector.len() == capacity {
                        vector.drain(..batch);
                    }
                    vector.push(input.clone());
                }
            });
            let mut deque = VecDeque::new();
            measure("deque_batch_control", n, sample, || {
                for _ in 0..n {
                    if deque.len() == capacity {
                        deque.drain(..batch);
                    }
                    deque.push_back(input.clone());
                }
            });
            assert_eq!(vector.len(), deque.len());
        }
        let left: Vec<_> = (0..n).map(|row| (row, row as f64)).collect();
        let right: Vec<_> = left
            .iter()
            .map(|(row, v)| (*row, 2. * v + 3.))
            .rev()
            .collect();
        let a = source_slice("input", &left);
        let b = source_slice("output", &right);
        let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
        for sample in 0..3 {
            let numeric = pair.numeric(|v| v.1, |v| v.1);
            measure("correlation_cold", n, sample, || numeric.correlation());
            measure("linear_fit_after_correlation", n, sample, || {
                numeric.linear_fit()
            });
            measure("difference_cold", n, sample, || numeric.difference());
            measure("numeric_views_cached", n, sample, || {
                (
                    numeric.correlation(),
                    numeric.linear_fit(),
                    numeric.difference(),
                )
            });
            measure("prepare_once_six_checks", n, sample, || {
                let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
                let numeric = pair.numeric(|v| v.1, |v| v.1);
                for _ in 0..2 {
                    black_box(numeric.correlation());
                    black_box(numeric.linear_fit());
                    black_box(numeric.difference());
                }
            });
            measure("reprepare_six_checks", n, sample, || {
                for _ in 0..2 {
                    for kind in 0..3 {
                        let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
                        let numeric = pair.numeric(|v| v.1, |v| v.1);
                        match kind {
                            0 => {
                                black_box(numeric.correlation());
                            }
                            1 => {
                                black_box(numeric.linear_fit());
                            }
                            _ => {
                                black_box(numeric.difference());
                            }
                        }
                    }
                }
            });
        }
    }
}
fn main() {
    if std::env::args().any(|v| v == "--measure") {
        support::run(benchmark);
    }
}
