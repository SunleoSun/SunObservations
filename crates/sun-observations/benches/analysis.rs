#[path = "support/mod.rs"]
mod support;
use serde::{Deserialize, Serialize};
use std::{hint::black_box, sync::Arc, time::Instant};
use sun_observations::certifications::*;
use sun_observations::evidence::*;
use sun_observations::{MemorySink, Observation, ObservationLevel, ObservationRecord, Sink};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "bench::sample")]
struct Sample {
    row: u64,
    value: f64,
}
fn shuffle(values: &mut [u64]) {
    let mut state = 42u64;
    for i in (1..values.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.swap(i, state as usize % (i + 1));
    }
}
fn measure<T>(name: &str, n: usize, sample: usize, scratch_bytes: usize, f: impl FnOnce() -> T) {
    let start = Instant::now();
    let output = f();
    let elapsed = start.elapsed();
    black_box(&output);
    println!(
        "{name},{n},{sample},{:.6},{:.3},{scratch_bytes}",
        elapsed.as_secs_f64() * 1000.,
        elapsed.as_nanos() as f64 / n as f64
    );
}
fn dense_candidate(left: &[u64], right: &[u64]) -> Vec<MatchedKey<u64>> {
    let mut a = vec![usize::MAX; left.len()];
    let mut b = vec![usize::MAX; right.len()];
    for (i, &k) in left.iter().enumerate() {
        a[k as usize] = i;
    }
    for (i, &k) in right.iter().enumerate() {
        b[k as usize] = i;
    }
    a.into_iter()
        .zip(b)
        .enumerate()
        .map(|(key, (left_index, right_index))| MatchedKey {
            key: key as u64,
            left_index,
            right_index,
        })
        .collect()
}
fn benchmark() {
    println!("operation,records,sample,total_ms,ns_per_record,scratch_or_output_bytes_estimate");
    for n in [1_000, 10_000, 100_000, 1_000_000] {
        let memory = MemorySink::new();
        for row in 0..n {
            let value = (row % 997) as f64 + 0.25;
            memory
                .write_shared(Arc::new(ObservationRecord {
                    ts: "2026-10-20T00:00:00.000Z".into(),
                    proc: "benchmark".into(),
                    proc_ms: row as u64,
                    scope: None,
                    namespace: Sample::NAMESPACE.into(),
                    level: ObservationLevel::INFO,
                    data: serde_json::to_value(Sample {
                        row: row as u64,
                        value,
                    })
                    .unwrap(),
                    message: None,
                    phase: None,
                    span: None,
                }))
                .unwrap();
        }
        let dataset = ObservationDataset::try_from_memory(&memory).unwrap();
        let typed = dataset.to_typed::<Sample>().unwrap();
        let source = typed.source("samples");
        let keys: Vec<_> = (0..n as u64).collect();
        let mut unordered = keys.clone();
        shuffle(&mut unordered);
        let mut unordered_right = unordered.clone();
        unordered_right.reverse();
        let prepared_numeric = source.numeric(|v| v.value);
        let numeric = prepared_numeric.series();
        let values: Vec<_> = typed
            .values()
            .iter()
            .map(|s| (s.value, 2. * s.value + 3.))
            .collect();
        let few_groups = source.group_by(|v| v.row % 8);
        let many_groups = source.group_by(|v| v.row % (n as u64 / 8));
        assert_eq!(few_groups.iter().map(|g| g.len()).sum::<usize>(), n);
        assert_eq!(many_groups.iter().map(|g| g.len()).sum::<usize>(), n);
        for window in [2, 8, 32] {
            let report = source
                .lag_with(
                    &source,
                    |v| (v.row as i64, v.value),
                    |v| (v.row as i64, v.value),
                    LagConfig {
                        range: LagRange::new(-window, window).unwrap(),
                        measure: LagMeasure::Pearson,
                    },
                )
                .observed
                .unwrap();
            assert_eq!(report.best.unwrap().lag, 0);
            for candidate in report.candidates {
                assert_eq!(
                    candidate.alignment.pair_count,
                    n - candidate.lag.unsigned_abs() as usize
                );
                assert!(!candidate.alignment.has_interior_gaps());
            }
        }
        let jumps = JumpConfig::new(Some(2000.), None, 1.).unwrap();
        let keys_source = collection("keys", &keys);
        let paired = keys_source
            .compare_with(&keys_source)
            .on_presorted(|v| *v, |v| *v)
            .unwrap();
        let numeric_comparison = paired.numeric(|v| *v as f64, |v| 2. * *v as f64 + 3.);
        let report = numeric_comparison.relation().observed.unwrap();
        let failure = fact("relation", report.clone())
            .expect(RelationExpectation::spearman_at_most(-0.9).unwrap());
        assert_eq!(
            dense_candidate(&keys, &unordered),
            join_by(&keys, &unordered, |v| *v, |v| *v)
                .comparison()
                .matched()
        );
        assert_eq!(paired.comparison().matched().len(), n);
        assert!((report.linear.slope.value().unwrap() - 2.).abs() < 1e-9);
        let report_bytes = serde_json::to_vec(&report).unwrap().len();
        let display_bytes = format!("{failure:?}").len();
        for sample in 0..3 {
            measure("snapshot", n, sample, 8 * n, || {
                ObservationDataset::try_from_memory(black_box(&memory)).unwrap()
            });
            measure("namespace_query", n, sample, 0, || {
                dataset
                    .query(ObservationsQuery::new().namespace(black_box(Sample::NAMESPACE)))
                    .iter()
                    .count()
            });
            measure("typed_decode", n, sample, 32 * n, || {
                dataset.to_typed::<Sample>().unwrap()
            });
            measure("typed_source_iteration", n, sample, 0, || {
                typed.iter().fold(0u64, |sum, entry| {
                    black_box(entry.source);
                    black_box(entry.value.value);
                    sum.wrapping_add(entry.value.row)
                        .wrapping_add(entry.dataset_index as u64)
                })
            });
            measure("group_few", n, sample, 16 * n, || {
                source.group_by(|v| v.row % 8)
            });
            measure("group_many", n, sample, 16 * n, || {
                source.group_by(|v| v.row % (n as u64 / 8))
            });
            measure(
                "group_summary_few",
                n,
                sample,
                8 * std::mem::size_of::<GroupSummary<u64>>(),
                || few_groups.numeric(|v| v.value).summaries(),
            );
            measure(
                "group_summary_many",
                n,
                sample,
                (n / 8) * std::mem::size_of::<GroupSummary<u64>>(),
                || many_groups.numeric(|v| v.value).summaries(),
            );
            for window in [2i64, 8, 32] {
                measure(
                    &format!("lag_pearson_pm{window}"),
                    n,
                    sample,
                    48 * n + (2 * window + 1) as usize * std::mem::size_of::<LagCandidate>(),
                    || {
                        source
                            .lag_with(
                                &source,
                                |v| (v.row as i64, v.value),
                                |v| (v.row as i64, v.value),
                                LagConfig {
                                    range: LagRange::new(-window, window).unwrap(),
                                    measure: LagMeasure::Pearson,
                                },
                            )
                            .observed
                            .unwrap()
                    },
                );
            }
            measure("summary", n, sample, 0, || numeric.summary().unwrap());
            measure("median_mad", n, sample, 8 * n, || {
                numeric.robust_summary().unwrap()
            });
            measure("sorted_join", n, sample, 56 * n, || {
                join_presorted_by(&keys, &keys, |v| *v, |v| *v).unwrap()
            });
            measure("generic_ordered_join", n, sample, 56 * n, || {
                join_by(&keys, &keys, |v| *v, |v| *v)
            });
            measure("generic_unordered_join", n, sample, 56 * n, || {
                join_by(&unordered, &keys, |v| *v, |v| *v)
            });
            measure("generic_both_unordered_join", n, sample, 56 * n, || {
                join_by(&unordered, &unordered_right, |v| *v, |v| *v)
            });
            measure("dense_candidate_unique", n, sample, 40 * n, || {
                dense_candidate(&unordered, &keys)
            });
            measure("pearson", n, sample, 0, || {
                pearson(values.iter().copied()).unwrap()
            });
            measure("spearman", n, sample, 40 * n, || {
                spearman(values.iter().copied()).unwrap()
            });
            measure("outliers", n, sample, 8 * n, || {
                numeric.outliers(OutlierConfig::default()).unwrap()
            });
            measure("jumps", n, sample, 0, || numeric.jumps(jumps).unwrap());
            measure("relation", n, sample, 40 * n, || {
                numeric_relation(values.iter().copied()).unwrap()
            });
            measure("report_json", n, sample, report_bytes, || {
                serde_json::to_vec(&report).unwrap()
            });
            measure("report_display", n, sample, display_bytes, || {
                format!("{failure:?}")
            });
        }
        eprintln!(
            "records={n}, serialized_report_bytes={}",
            serde_json::to_vec(&report).unwrap().len()
        );
    }
}
fn main() {
    if std::env::args().any(|a| a == "--measure") || !std::env::args().any(|a| a == "--test") {
        support::run(benchmark);
    }
}
