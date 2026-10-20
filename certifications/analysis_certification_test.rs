extern crate analysis as sun_observations_analysis;
extern crate obs as sun_observations;

use analysis::*;
use obs::{MemorySink, Observation, ObservationEngine, Selection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-9 * (1. + expected.abs()),
        "{actual} != {expected}"
    );
}

#[test]
fn same_observation_types_certify_positive_negative_and_wrong_scale_scenarios() {
    for slope in [2., -2., 100.] {
        let dataset = capture(100, slope);
        let left = dataset.typed::<Input>().unwrap();
        let right = dataset.typed::<Output>().unwrap();
        let joined = left.join_by(&right, |v| v.row, |v| v.row);
        let report = joined
            .numeric_relation(|v| v.value, |v| v.value, 4)
            .unwrap();
        assert_eq!(
            report.alignment.left_namespace.as_deref(),
            Some(Input::NAMESPACE)
        );
        assert_eq!(
            report.alignment.right_namespace.as_deref(),
            Some(Output::NAMESPACE)
        );
        close(report.observed.linear.slope.value().unwrap(), slope);
        let expected = if slope > 0. {
            RelationExpectation::positive(0.99)
        } else {
            RelationExpectation::negative(-0.99)
        }
        .unwrap();
        assert!(expected.evaluate(report.clone()).is_ok());
        let wrong = if slope > 0. {
            RelationExpectation::negative(-0.9)
        } else {
            RelationExpectation::positive(0.9)
        }
        .unwrap();
        let failure = wrong.evaluate(report.clone()).unwrap_err();
        assert_eq!(failure.observed.alignment.matched.len(), 100);
        assert_eq!(failure.violated.len(), 1);
        let json = serde_json::to_value(&failure).unwrap();
        assert_eq!(json["analysis_kind"], "relation_expectation_failure");
        assert_eq!(json["observed"]["analysis_kind"], "relation");
        assert!(json["observed"]["observed"]["spearman"]
            .get("Defined")
            .is_some());
        assert_eq!(
            serde_json::from_value::<RelationFailure<u64>>(json).unwrap(),
            *failure
        );
        if slope == 100. {
            let wrong_scale = expected
                .bound(RelationMeasure::Slope, Some(0.9), Some(1.1))
                .unwrap()
                .evaluate(report)
                .unwrap_err();
            assert_eq!(wrong_scale.violated[0].measure, RelationMeasure::Slope);
        }
    }
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
    let joined = left.join_by(&right, |v| v.row, |v| v.row);
    let Err(RelationError::Alignment(report)) =
        joined.numeric_relation(|v| v.value, |v| v.value, 5)
    else {
        panic!()
    };
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.missing_left[0].key, 3);
    assert_eq!(report.missing_right[0].key, 1);
    assert_eq!(report.duplicate_left[0].indices, vec![1, 2]);
    assert_eq!(left.sources()[report.duplicate_left[0].indices[1]].0, 2);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["left_namespace"], Input::NAMESPACE);
    assert_eq!(json["duplicate_left"][0]["key"], 2);
}

#[test]
fn captured_100k_and_one_million_records_remain_complete_and_mathematically_correct() {
    for n in [50_000u64, 500_000] {
        let dataset = capture(n, 2.);
        assert_eq!(dataset.records().len(), 2 * n as usize);
        let left = dataset.typed::<Input>().unwrap();
        let right = dataset.typed::<Output>().unwrap();
        assert!(check_contiguous(left.values(), |v| v.row, 0, n - 1).is_valid());
        let generic = left.join_by(&right, |v| v.row, |v| v.row);
        let joined = left.join_u64_by(&right, |v| v.row, |v| v.row);
        assert_eq!(joined.alignment(), generic.alignment());
        assert_eq!(joined.alignment().matched.len(), n as usize);
        assert!(joined.alignment().is_complete());
        let report = joined
            .numeric_relation(|v| v.value, |v| v.value, 3)
            .unwrap();
        close(report.observed.linear.pearson.value().unwrap(), 1.);
        close(report.observed.spearman.value().unwrap(), 1.);
        close(report.observed.linear.slope.value().unwrap(), 2.);
        close(report.observed.linear.intercept.value().unwrap(), 3.);
        close(report.observed.residual.as_ref().unwrap().absolute.max, 0.);
        close(
            left.numeric(|v| v.value).summary().unwrap().mean,
            (n - 1) as f64 / 2.,
        );
        assert!(left
            .numeric(|v| v.value)
            .outliers(OutlierConfig::default())
            .unwrap()
            .suspicious
            .is_empty());
        let jumps = left
            .numeric(|v| v.value)
            .jumps(JumpConfig {
                absolute_threshold: Some(1.),
                relative_threshold: None,
                denominator_floor: 1.,
            })
            .unwrap();
        assert_eq!(jumps.transition_count, n as usize - 1);
        assert!(jumps.suspicious.is_empty());
        assert_eq!(
            serde_json::to_value(&report.observed).unwrap()["linear"]["count"],
            n
        );
    }
}

#[path = "../crates/sun-observations-analysis/examples/module_relation.rs"]
mod example;
#[test]
fn public_cross_module_example_executes() {
    example::main().unwrap();
}
