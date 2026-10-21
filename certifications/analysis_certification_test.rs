extern crate analysis as sun_observations_data_source;
extern crate obs as sun_observations;

use analysis::*;
use obs::{MemorySink, Observation, ObservationEngine, Selection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sun_certifications::*;

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
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("evidence", memory.clone());
    engine
        .register_namespace_channel("pipeline", "evidence")
        .unwrap();
    engine.set_selection(Selection::new().activate_namespace("pipeline"));
    (engine, memory)
}
fn capture(n: u64, slope: f64) -> ObservationDataset {
    let (engine, memory) = engine();
    for row in 0..n {
        engine
            .observe_if_active(|| Input {
                row,
                value: row as f64,
            })
            .unwrap();
    }
    for row in (0..n).rev() {
        engine
            .observe_if_active(|| Output {
                row,
                value: slope * row as f64 + 3.,
            })
            .unwrap();
    }
    engine.flush_channels().unwrap();
    ObservationDataset::from_memory(&memory)
}
fn close(c: &mut Certification, name: &str, actual: f64, expected: f64) {
    c.check(
        name,
        measured(name, actual).compare(Criterion::Range {
            min: expected - 1e-9 * (1. + expected.abs()),
            max: expected + 1e-9 * (1. + expected.abs()),
        }),
    );
}

#[test]
fn same_observation_types_certify_positive_negative_and_wrong_scale_scenarios() {
    let mut proof = certify("producer-consumer direction and scale");
    for slope in [2., -2., 100.] {
        let dataset = capture(100, slope);
        let left = dataset.typed::<Input>().unwrap();
        let right = dataset.typed::<Output>().unwrap();
        let a = left.source("producer");
        let b = right.source("consumer");
        let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
        let numeric = pair.numeric(|v| v.value, |v| v.value);
        proof.check("coverage", pair.coverage().compare(AllValid));
        let expectation = if slope > 0. {
            RelationExpectation::positive(0.99)
        } else {
            RelationExpectation::negative(-0.99)
        }
        .unwrap();
        proof.check("direction", numeric.relation().compare(expectation.clone()));
        let metrics = numeric.relation().observed.unwrap();
        close(
            &mut proof,
            "affine slope",
            metrics.linear.slope.value().unwrap(),
            slope,
        );
        let wrong = if slope > 0. {
            RelationExpectation::negative(-0.9)
        } else {
            RelationExpectation::positive(0.9)
        }
        .unwrap();
        let rejected = numeric.relation().compare(wrong);
        proof.check(
            "wrong direction rejected",
            measured("control", rejected.class).compare(Equal(CheckClass::Statistical)),
        );
        if slope == 100. {
            let rejected = numeric.relation().compare(
                expectation
                    .bound(RelationMeasure::Slope, Some(0.9), Some(1.1))
                    .unwrap(),
            );
            proof.check(
                "scale distinct from direction",
                measured("control", rejected.violations[0].measure)
                    .compare(Equal(RelationMeasure::Slope)),
            );
        }
    }
    proof.assert_certify();
}

#[test]
fn runtime_missing_and_duplicate_evidence_is_a_hard_failure_not_suspicion() {
    let (engine, memory) = engine();
    for row in [1, 2, 2, 4] {
        engine
            .observe_if_active(|| Input {
                row,
                value: row as f64,
            })
            .unwrap();
    }
    for row in [2, 3, 4] {
        engine
            .observe_if_active(|| Output {
                row,
                value: row as f64,
            })
            .unwrap();
    }
    let dataset = ObservationDataset::from_memory(&memory);
    let left = dataset.typed::<Input>().unwrap();
    let right = dataset.typed::<Output>().unwrap();
    let a = left.source("producer");
    let b = right.source("consumer");
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let a = pair.alignment();
    let mut c = certify("ambiguous evidence fails closed");
    c.check(
        "classification",
        measured("coverage", pair.coverage().compare(AllValid).class)
            .compare(Equal(CheckClass::Hard)),
    )
    .check(
        "matched",
        measured("pairs", a.matched.len()).compare(Equal(1)),
    )
    .check(
        "missing producer",
        measured("keys", a.missing_left[0].key).compare(Equal(3)),
    )
    .check(
        "missing consumer",
        measured("keys", a.missing_right[0].key).compare(Equal(1)),
    )
    .check(
        "ambiguous source coordinates",
        measured("indices", &a.duplicate_left[0].indices).compare(Equal(&vec![1, 2])),
    )
    .check(
        "dataset coordinate",
        measured(
            "provenance",
            left.sources()[a.duplicate_left[0].indices[1]].0,
        )
        .compare(Equal(2)),
    )
    .check(
        "dependent blocked",
        measured(
            "relation",
            pair.numeric(|v| v.value, |v| v.value)
                .relation()
                .compare(RelationExpectation::positive(0.9).unwrap())
                .class,
        )
        .compare(Equal(CheckClass::Error)),
    )
    .assert_certify();
}

#[test]
fn captured_100k_and_one_million_records_remain_complete_and_mathematically_correct() {
    for n in [50_000u64, 500_000] {
        let dataset = capture(n, 2.);
        let left = dataset.typed::<Input>().unwrap();
        let right = dataset.typed::<Output>().unwrap();
        let a = left.source("input");
        let b = right.source("output");
        let pair = a.compare_with(&b).on_u64(|v| v.row, |v| v.row);
        let numeric = pair.numeric(|v| v.value, |v| v.value);
        let mut c = certify("million-record production trace");
        c.check(
            "snapshot count",
            measured("snapshot", dataset.records().len()).compare(Equal(2 * n as usize)),
        )
        .check(
            "contiguous",
            a.contiguous(|v| v.row, 0, n - 1).compare(AllValid),
        )
        .check("coverage", pair.coverage().compare(AllValid))
        .check(
            "affine semantics",
            numeric.relation().compare(
                RelationExpectation::positive(0.99)
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
        close(
            &mut c,
            "mean",
            signal.summary().observed.unwrap().mean,
            (n - 1) as f64 / 2.,
        );
        c.check(
            "outliers",
            signal
                .outliers(OutlierConfig::default())
                .compare(AtMostAnomalies(0)),
        )
        .check(
            "jumps",
            signal
                .jumps(JumpConfig {
                    absolute_threshold: Some(1.),
                    relative_threshold: None,
                    denominator_floor: 1.,
                })
                .compare(AtMostAnomalies(0)),
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
        let dataset = ObservationDataset::from_memory(&memory);
        let inputs = dataset.typed::<Input>().unwrap();
        let outputs = dataset.typed::<Output>().unwrap();
        memory.drain_shared();
        let a = inputs.source("inputs");
        let b = outputs.source("shifted outputs");
        let groups = a.group_by(|v| v.row % 16);
        let summaries = groups.numeric(|v| v.value).summaries();
        let mut c = certify("snapshot provenance, groups and causal lag");
        c.check(
            "source identity",
            measured(
                "snapshot",
                inputs.iter().all(|e| {
                    e.dataset_index == e.value.row as usize
                        && Arc::ptr_eq(e.source, &dataset.records()[e.dataset_index])
                        && e.source.proc == engine.process_id()
                }),
            )
            .compare(Equal(true)),
        )
        .check(
            "group membership",
            measured(
                "groups",
                groups.iter().all(|g| {
                    g.values().all(|v| v.row % 16 == *g.key) && g.len() == g.iter().count()
                }),
            )
            .compare(Equal(true)),
        )
        .check(
            "group count",
            measured("groups", summaries.groups.len()).compare(Equal(16)),
        )
        .check(
            "member count",
            measured(
                "groups",
                summaries
                    .groups
                    .iter()
                    .map(|g| g.member_count)
                    .sum::<usize>(),
            )
            .compare(Equal(n as usize)),
        );
        close(
            &mut c,
            "group sum",
            summaries
                .groups
                .iter()
                .map(|g| g.summary.as_ref().unwrap().sum)
                .sum(),
            a.numeric(|v| v.value).summary().observed.unwrap().sum,
        );
        c.check(
            "causal lag",
            a.lag_relation_by(
                &b,
                |v| (v.row as i64, v.value),
                |v| (v.row as i64, v.value),
                LagConfig {
                    range: LagRange { start: -2, end: 2 },
                    measure: LagMeasure::Pearson,
                },
            )
            .compare(LagExpectation { best_lag: 2 }),
        )
        .assert_certify();
    }
}

#[path = "../crates/sun-observations-data-source/examples/module_relation.rs"]
mod example;
#[test]
fn public_cross_module_example_executes() {
    example::main().unwrap();
}
