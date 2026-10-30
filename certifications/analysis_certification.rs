use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sun_observations::certifications::*;
use sun_observations::evidence::*;
use sun_observations::{MemorySink, Observation, ObservationEngine, Selection};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::input")]
struct Input {
    row: u64,
    value: f64,
}
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "pipeline::output")]
struct Output {
    row: u64,
    value: f64,
}
fn engine() -> (ObservationEngine, Arc<MemorySink>) {
    let engine = ObservationEngine::isolated();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("evidence", memory.clone());
    engine
        .register_namespace_channel("pipeline", "evidence")
        .unwrap();
    engine.set_selection(Selection::new().activate_namespace("pipeline").unwrap());
    (engine, memory)
}
fn capture(n: u64, slope: f64) -> CapturedRun<()> {
    capture_observations(
        CaptureConfig::new().activate_namespace("pipeline").unwrap(),
        || {
            for row in 0..n {
                sun_observations::try_observe!(Input {
                    row,
                    value: row as f64
                })
                .unwrap();
            }
            for row in (0..n).rev() {
                sun_observations::try_observe!(Output {
                    row,
                    value: slope * row as f64 + 3.
                })
                .unwrap();
            }
        },
    )
}

#[test]
fn same_observation_types_certify_positive_negative_and_wrong_scale_scenarios() {
    for slope in [2., -2., 100.] {
        let run = capture(100, slope);
        let left = run.to_typed::<Input>().unwrap();
        let right = run.to_typed::<Output>().unwrap();
        let a = left.source("producer");
        let b = right.source("consumer");
        let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
        let numeric = pair.numeric(|v| v.value, |v| v.value);
        let mut proof = run.certification("producer-consumer direction and scale");
        proof.check("comparison", pair.paired_comparison_fact().expect_valid());
        let direction = if slope > 0. {
            CorrelationExpectation::spearman_at_least(0.99)
        } else {
            CorrelationExpectation::spearman_at_most(-0.99)
        }
        .unwrap();
        proof.check("direction", numeric.correlation().expect(direction));
        proof.check(
            "affine slope",
            numeric
                .linear_fit()
                .slope()
                .expect(close_to(slope).unwrap()),
        );
        let wrong = if slope > 0. {
            CorrelationExpectation::spearman_at_most(-0.9)
        } else {
            CorrelationExpectation::spearman_at_least(0.9)
        }
        .unwrap();
        proof.check_fact_expect(
            "wrong direction rejected",
            numeric.correlation().expect(wrong).class(),
            CheckClass::StatisticalViolation,
        );
        if slope == 100. {
            proof.check_fact_expect(
                "scale distinct from direction",
                numeric
                    .linear_fit()
                    .expect(LinearFitExpectation::slope_between(0.9, 1.1).unwrap())
                    .class(),
                CheckClass::StatisticalViolation,
            );
        }
        proof.assert_certify();
    }
}

#[test]
fn runtime_missing_and_duplicate_evidence_is_a_hard_failure_not_suspicion() {
    let dataset = capture_observations(
        CaptureConfig::new().activate_namespace("pipeline").unwrap(),
        || {
            for row in [1, 2, 2, 4] {
                sun_observations::try_observe!(Input {
                    row,
                    value: row as f64
                })
                .unwrap();
            }
            for row in [2, 3, 4] {
                sun_observations::try_observe!(Output {
                    row,
                    value: row as f64
                })
                .unwrap();
            }
        },
    );
    let left = dataset.to_typed::<Input>().unwrap();
    let right = dataset.to_typed::<Output>().unwrap();
    let a = left.source("producer");
    let b = right.source("consumer");
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let a = pair.comparison();
    let mut c = dataset.certification("ambiguous evidence fails closed");
    c.check(
        "classification",
        fact(
            "paired_comparison_fact",
            pair.paired_comparison_fact().expect_valid().class(),
        )
        .expect(CheckClass::HardViolation),
    )
    .check("matched", fact("pairs", a.matched().len()).expect(1))
    .check(
        "missing producer",
        fact("keys", a.right_only()[0].key).expect(3),
    )
    .check(
        "missing consumer",
        fact("keys", a.left_only()[0].key).expect(1),
    )
    .check(
        "ambiguous source coordinates",
        fact("indices", &a.duplicate_left()[0].indices).expect(&vec![1, 2]),
    )
    .check(
        "dataset coordinate",
        fact(
            "provenance",
            left.sources()[a.duplicate_left()[0].indices[1]].0,
        )
        .expect(2),
    )
    .check(
        "dependent blocked",
        fact(
            "relation",
            pair.numeric(|v| v.value, |v| v.value)
                .relation()
                .expect(RelationExpectation::spearman_at_least(0.9).unwrap())
                .class(),
        )
        .expect(CheckClass::EvaluationError),
    )
    .assert_certify();
}

#[test]
fn captured_100k_and_one_million_records_remain_complete_and_mathematically_correct() {
    for n in [50_000u64, 500_000] {
        let dataset = capture(n, 2.);
        let left = dataset.to_typed::<Input>().unwrap();
        let right = dataset.to_typed::<Output>().unwrap();
        let a = left.source("input");
        let b = right.source("output");
        let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
        let numeric = pair.numeric(|v| v.value, |v| v.value);
        let mut c = dataset.certification("million-record production trace");
        c.check(
            "snapshot count",
            fact("snapshot", dataset.dataset().records().len()).expect(2 * n as usize),
        )
        .check(
            "contiguous",
            a.contiguous(|v| v.row, 0, n - 1).expect_valid(),
        )
        .check(
            "paired_comparison_fact",
            pair.paired_comparison_fact().expect_valid(),
        )
        .check(
            "affine semantics",
            numeric.relation().expect(
                RelationExpectation::spearman_at_least(0.99)
                    .unwrap()
                    .bound(RelationMeasure::Slope, Some(2.), Some(2.))
                    .unwrap()
                    .bound(RelationMeasure::Intercept, Some(3.), Some(3.))
                    .unwrap()
                    .bound(RelationMeasure::ResidualMaxAbsolute, None, Some(1e-9))
                    .unwrap(),
            ),
        );
        let signal = a.numeric(|v| v.value);
        c.check_fact_expect(
            "mean",
            signal.summary().observed.unwrap().mean,
            close_to((n - 1) as f64 / 2.).unwrap(),
        );
        c.check(
            "outliers",
            signal
                .outliers(OutlierConfig::default())
                .expect(AtMostAnomalies(0)),
        )
        .check(
            "jumps",
            signal
                .jumps(JumpConfig::new(Some(1.), None, 1.).unwrap())
                .expect(AtMostAnomalies(0)),
        )
        .assert_certify();
    }
}

#[test]
fn captured_large_grouped_source_and_shifted_evidence_remains_exact_after_drain() {
    for n in [50_000u64, 500_000] {
        let (engine, memory) = engine();
        let value = |row: u64| ((row.wrapping_mul(6364136223846793005) ^ (row >> 3)) % 1009) as f64;
        for row in 0..n {
            engine
                .observe_if_active(|| Input {
                    row,
                    value: value(row),
                })
                .unwrap();
        }
        for row in (0..n).rev() {
            engine
                .observe_if_active(|| Output {
                    row: row + 2,
                    value: -2. * value(row) + 3.,
                })
                .unwrap();
        }
        engine.flush_channels().unwrap();
        let dataset = ObservationDataset::try_from_memory(&memory).unwrap();
        let inputs = dataset.to_typed::<Input>().unwrap();
        let outputs = dataset.to_typed::<Output>().unwrap();
        memory.drain_shared();
        let a = inputs.source("inputs");
        let b = outputs.source("shifted outputs");
        let groups = a.group_by(|v| v.row % 16);
        let summaries = groups.numeric(|v| v.value).summaries();
        let mut c = certification("snapshot provenance, groups and causal lag");
        c.check(
            "source identity",
            fact(
                "snapshot",
                inputs.iter().all(|e| {
                    e.dataset_index == e.value.row as usize
                        && Arc::ptr_eq(e.source, &dataset.records()[e.dataset_index])
                        && e.source.proc == engine.process_id()
                }),
            )
            .expect(true),
        )
        .check(
            "group membership",
            fact(
                "groups",
                groups.iter().all(|g| {
                    g.values().all(|v| v.row % 16 == *g.key) && g.len() == g.iter().count()
                }),
            )
            .expect(true),
        )
        .check(
            "group count",
            fact("groups", summaries.groups.len()).expect(16),
        )
        .check(
            "member count",
            fact(
                "groups",
                summaries
                    .groups
                    .iter()
                    .map(|g| g.member_count)
                    .sum::<usize>(),
            )
            .expect(n as usize),
        );
        c.check_fact_expect(
            "group sum",
            summaries
                .groups
                .iter()
                .map(|g| g.summary.as_ref().unwrap().sum)
                .sum::<f64>(),
            close_to(a.numeric(|v| v.value).summary().observed.unwrap().sum).unwrap(),
        );
        c.check(
            "causal lag",
            a.lag_with(
                &b,
                |v| (v.row as i64, v.value),
                |v| (v.row as i64, v.value),
                LagConfig {
                    range: LagRange::new(-2, 2).unwrap(),
                    measure: LagMeasure::Pearson,
                },
            )
            .expect(LagExpectation { best_lag: 2 }),
        )
        .assert_certify();
    }
}

#[path = "../crates/sun-observations/examples/module_relation.rs"]
mod example;
#[test]
fn public_cross_module_example_executes() {
    example::main().unwrap();
}
