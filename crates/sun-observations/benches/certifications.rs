use serde::{Deserialize, Serialize};
use std::{hint::black_box, sync::Arc, time::Instant};
use sun_observations::certifications::*;
use sun_observations::evidence::*;
use sun_observations::{Observation, ObservationLevel, ObservationRecord};
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "bench::row")]
struct Row {
    fold: u64,
    target: u64,
    row: u64,
    signal: f64,
    strength: f64,
}
fn key(v: &Row) -> (u64, u64, u64) {
    (v.fold, v.target, v.row)
}
fn sample(name: &str, n: usize, mut op: impl FnMut()) {
    for sample in 0..3 {
        let start = Instant::now();
        op();
        println!(
            "{name},{n},{sample},{:.6}",
            start.elapsed().as_secs_f64() * 1000.
        );
    }
}
fn field_check(pair: &PairedComparison<'_, Row, Row, (u64, u64, u64)>, field: usize) {
    match field {
        0 => {
            black_box(pair.equal(|v| v.signal, |v| v.signal, 5).expect_valid());
        }
        1 => {
            black_box(pair.equal(|v| v.strength, |v| v.strength, 5).expect_valid());
        }
        2 => {
            black_box(
                pair.check_pairs(|l, r| l.fold == r.fold && l.target == r.target, 5)
                    .expect_valid(),
            );
        }
        3 => {
            black_box(pair.numeric(|v| v.signal, |v| v.signal).linear_fit());
        }
        _ => {
            black_box(pair.numeric(|v| v.strength, |v| v.strength).difference());
        }
    }
}
fn main() {
    if !std::env::args().any(|v| v == "--measure") {
        return;
    }
    println!("operation,records,sample,milliseconds");
    for n in [1000usize, 10000, 100000, 1000000] {
        let left: Vec<_> = (0..n)
            .map(|i| Row {
                fold: (i % 8) as u64,
                target: (i % 3) as u64,
                row: i as u64,
                signal: ((i * 17 + i * i * 13) % 1009) as f64,
                strength: i as f64,
            })
            .collect();
        let right: Vec<_> = left
            .iter()
            .rev()
            .map(|v| Row {
                fold: v.fold,
                target: v.target,
                row: v.row,
                signal: v.signal,
                strength: v.strength,
            })
            .collect();
        let records = left
            .iter()
            .map(|v| {
                Arc::new(ObservationRecord {
                    ts: String::new(),
                    proc: "bench".into(),
                    proc_ms: 0,
                    scope: None,
                    namespace: Row::NAMESPACE.into(),
                    level: ObservationLevel::INFO,
                    data: serde_json::to_value(v).unwrap(),
                    message: None,
                    phase: None,
                    span: None,
                })
            })
            .collect();
        let dataset = ObservationDataset::try_from_records(records).unwrap();
        let typed = dataset.to_typed::<Row>().unwrap();
        let a = collection("ordinary", &left);
        let b = collection("reference", &right);
        sample("ordinary_source", n, || {
            black_box(collection("ordinary", black_box(&left)));
        });
        sample("observation_source", n, || {
            black_box(typed.source("observed"));
        });
        sample("ordinary_numeric_projection", n, || {
            black_box(a.numeric(|v| v.signal));
        });
        sample("observation_numeric_projection", n, || {
            black_box(typed.source("observed").numeric(|v| v.signal));
        });
        sample("observation_decode", n, || {
            black_box(dataset.to_typed::<Row>().unwrap());
        });
        sample("scalar_criteria_1000", n, || {
            for i in 0..1000 {
                black_box(
                    source("returned row", black_box(&left[i % n]))
                        .single_map(|v| v.signal)
                        .expect(Criterion::at_least(0.).unwrap()),
                );
            }
        });
        sample("key_projection", n, || {
            black_box(a.project(key));
        });
        sample("prepared_keys", n, || {
            black_box(a.keys(key));
        });
        sample("simple_comparison", n, || {
            black_box(a.compare_with(&b).on(|v| v.row, |v| v.row));
        });
        sample("composite_comparison", n, || {
            black_box(a.compare_with(&b).on(key, key));
        });
        let pair = a.compare_with(&b).on(key, key);
        let numeric = pair.numeric(|v| v.signal, |v| v.signal);
        sample("same_keys", n, || {
            black_box(pair.paired_comparison_fact().expect_valid());
        });
        sample("equality", n, || {
            black_box(pair.equal(|v| v.signal, |v| v.signal, 5).expect_valid());
        });
        sample("difference", n, || {
            black_box(numeric.difference());
        });
        sample("pearson", n, || {
            black_box(numeric.pearson());
        });
        sample("spearman", n, || {
            black_box(numeric.spearman());
        });
        sample("relation", n, || {
            black_box(numeric.relation());
        });
        sample("grouped_summary", n, || {
            black_box(a.group_by(|v| v.fold).numeric(|v| v.signal).summaries());
        });
        let signal = a.numeric(|v| v.signal);
        sample("robust_mad", n, || {
            black_box(signal.robust_summary());
        });
        sample("outliers", n, || {
            black_box(signal.outliers(OutlierConfig::default()));
        });
        sample("jumps", n, || {
            black_box(signal.jumps(JumpConfig::new(Some(1000.), None, 1.).unwrap()));
        });
        for window in [2, 8, 32] {
            sample(&format!("lag_pm{window}"), n, || {
                black_box(a.lag_with(
                    &b,
                    |v| (v.row as i64, v.signal),
                    |v| (v.row as i64, v.signal),
                    LagConfig {
                        range: LagRange::new(-window, window).unwrap(),
                        measure: LagMeasure::Pearson,
                    },
                ));
            });
        }
        sample("prepare_once_5_checks", n, || {
            let p = a.compare_with(&b).on(key, key);
            for field in 0..5 {
                field_check(&p, field);
            }
        });
        sample("reprepare_5_checks", n, || {
            for field in 0..5 {
                let p = a.compare_with(&b).on(key, key);
                field_check(&p, field);
            }
        });
        sample("success_aggregation_100", n, || {
            let mut c = certification("success");
            for _ in 0..100 {
                c.check("count", a.count().expect(n));
            }
            black_box(c.report());
        });
        sample("multiple_failures_100", n, || {
            let mut c = certification("failed");
            for _ in 0..100 {
                c.check("count", a.count().expect(0));
            }
            black_box(c.report());
        });
        let mut c = certification("million-record failure");
        for _ in 0..10 {
            c.check("count", a.count().expect(0));
        }
        let report = c.report();
        sample("bounded_formatting", n, || {
            black_box(report.to_string());
        });
    }
}
