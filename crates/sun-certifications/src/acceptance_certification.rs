use crate::*;
use std::{any::Any, fmt::Debug, panic::catch_unwind};

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else {
        panic!("final assertion must render its report")
    }
}

fn assert_rejected<T: Debug, V: Debug>(
    name: &str,
    result: CheckResult<T, V>,
    class: CheckClass,
) -> CertificationReport {
    assert_ne!(class, CheckClass::Pass);
    assert_eq!(result.class, class, "{name}: {result:?}");
    assert!(!result.is_pass(), "{name}");
    if class != CheckClass::EvaluationError {
        assert!(!result.violations.is_empty(), "{name}");
    } else {
        assert!(
            result.observed.is_err() || result.analysis_error.is_some(),
            "{name}"
        );
    }
    let mut c = certify(name);
    c.limits(DiagnosticLimits {
        failures: 1,
        characters_per_failure: 512,
    });
    c.section("acceptance", |c| {
        c.check(name, result);
    });
    c.check_fact_expect("independent healthy path", true, true);
    let report = c.certify();
    assert!(!report.is_pass(), "{name}");
    assert_eq!(
        (report.checks, report.failed, report.failures.len()),
        (2, 1, 1),
        "{name}"
    );
    let failure = &report.failures[0];
    assert_eq!(failure.class, class);
    assert_eq!(failure.section, "acceptance/");
    assert_eq!(failure.name, name);
    assert!(failure.diagnostic.contains("sources:"));
    assert!(failure.diagnostic.contains("expected:"));
    assert!(failure.diagnostic.len() <= 512 + '…'.len_utf8());
    let rendered = report.to_string();
    assert!(rendered.contains("CERTIFICATION FAILED"));
    assert!(rendered.contains(&format!("[{class:?}]")));
    assert!(rendered.len() < 2048);
    assert_eq!(
        panic_text(catch_unwind(|| c.assert_certify()).expect_err(name)),
        rendered
    );
    assert_eq!(
        panic_text(catch_unwind(|| report.assert_certify()).expect_err(name)),
        rendered
    );
    report
}

#[test]
fn ordinary_and_custom_expectations_fail_only_at_final_boundaries() {
    let hard = CheckClass::HardViolation;
    assert_rejected(
        "plain equality",
        fact("value", vec![1]).expect(vec![2]),
        hard,
    );
    assert_rejected(
        "string equality",
        fact("value", String::from("actual")).expect("expected"),
        hard,
    );
    assert_rejected("explicit equality", fact("value", 1).expect(Equal(2)), hard);
    assert_rejected("inequality", fact("value", 1).expect(NotEqual(1)), hard);
    for rule in [
        Criterion::Equal(2),
        Criterion::NotEqual(1),
        Criterion::Greater(1),
        Criterion::AtLeast(2),
        Criterion::Less(1),
        Criterion::AtMost(0),
        Criterion::Range { min: 2, max: 3 },
    ] {
        assert_rejected(
            &format!("criterion {rule:?}"),
            fact("value", 1).expect(rule),
            hard,
        );
    }
    for class in [hard, CheckClass::StatisticalViolation] {
        assert_rejected(
            "custom predicate",
            fact("value", 1).expect(criterion("positive", class, |v: &i32| *v > 1)),
            class,
        );
    }
    assert_rejected(
        "proximity",
        fact("value", 2.).expect(close_to(1.).absolute(0.01).relative(0.)),
        hard,
    );
    for rule in [
        CountRule::Exact(1),
        CountRule::Minimum(1),
        CountRule::NonEmpty,
    ] {
        assert_rejected(
            &format!("count {rule:?}"),
            fact("empty", 0usize).expect(rule),
            hard,
        );
    }
    for (value, rule) in [
        (f64::NAN, NumericRule::Finite),
        (2., NumericRule::Range { min: 0., max: 1. }),
        (0., NumericRule::Positive),
        (-1., NumericRule::NonNegative),
    ] {
        assert_rejected(
            &format!("numeric {rule:?}"),
            fact("value", value).expect(rule),
            hard,
        );
    }
}

#[test]
fn structural_expectations_and_pair_diagnostics_reach_final_boundaries() {
    let rows = [(0u64, 1.), (1, 2.), (2, 3.)];
    let other = [(0u64, 10.), (1, 20.), (2, 30.)];
    let a = source_slice("producer", &rows);
    let b = source_slice("consumer", &other);
    let pair = a.compare_with(&b).on(|v| v.0, |v| v.0);
    assert_rejected(
        "duplicate keys",
        a.unique(|_| 0).expect_valid(),
        CheckClass::HardViolation,
    );
    assert_rejected(
        "missing range",
        a.contiguous(|v| v.0, 0, 3).expect_valid(),
        CheckClass::HardViolation,
    );
    let report = assert_rejected(
        "pair equality",
        pair.equal(|v| v.1, |v| v.1, 1).expect_valid(),
        CheckClass::HardViolation,
    );
    assert!(report
        .to_string()
        .contains("all compared pairs satisfy predicate"));
    assert_rejected(
        "pair predicate",
        pair.check_pairs(|l, r| l.1 > r.1, 1).expect_valid(),
        CheckClass::HardViolation,
    );
    let b = source_slice("incomplete consumer", &other[..2]);
    let incomplete = a.compare_with(&b).on(|v| v.0, |v| v.0);
    assert_rejected(
        "incomplete comparison",
        incomplete.comparison_fact().expect_valid(),
        CheckClass::HardViolation,
    );
    assert_rejected(
        "incomplete pairs",
        incomplete.equal(|v| v.1, |v| v.1, 1).expect_valid(),
        CheckClass::EvaluationError,
    );
    assert_rejected(
        "incomplete numeric",
        incomplete
            .numeric(|v| v.1, |v| v.1)
            .correlation()
            .expect(CorrelationExpectation::positive(0.9).unwrap()),
        CheckClass::EvaluationError,
    );
}

#[test]
fn each_correlation_measure_independently_fails_final_proof() {
    let values = [(1., 9.), (2., 7.), (3., 5.), (4., 3.)];
    let a = source_slice("left", &values);
    let pair = a.compare_with(&a).on(|v| v.0 as u64, |v| v.0 as u64);
    let numeric = pair.numeric(|v| v.0, |v| v.1);
    for measure in [
        CorrelationMeasure::Pearson,
        CorrelationMeasure::Spearman,
        CorrelationMeasure::AbsolutePearson,
        CorrelationMeasure::AbsoluteSpearman,
    ] {
        let (min, max) = match measure {
            CorrelationMeasure::Pearson | CorrelationMeasure::Spearman => (Some(0.), None),
            CorrelationMeasure::AbsolutePearson | CorrelationMeasure::AbsoluteSpearman => {
                (None, Some(0.))
            }
        };
        assert_rejected(
            &format!("correlation {measure:?}"),
            numeric.correlation().expect(
                CorrelationExpectation::default()
                    .bound(measure, min, max)
                    .unwrap(),
            ),
            CheckClass::StatisticalViolation,
        );
    }
}

#[test]
fn each_fit_and_difference_measure_independently_fails_final_proof() {
    let values = [(1., 5.), (2., 10.), (3., 9.), (4., 16.)];
    let a = source_slice("left", &values);
    let pair = a.compare_with(&a).on(|v| v.0 as u64, |v| v.0 as u64);
    let numeric = pair.numeric(|v| v.0, |v| v.1);
    for measure in [
        LinearFitMeasure::Slope,
        LinearFitMeasure::Intercept,
        LinearFitMeasure::RSquared,
        LinearFitMeasure::ResidualMeanAbsolute,
        LinearFitMeasure::ResidualMaxAbsolute,
    ] {
        assert_rejected(
            &format!("fit {measure:?}"),
            numeric.linear_fit().expect(
                LinearFitExpectation::default()
                    .bound(measure, None, Some(-1.))
                    .unwrap(),
            ),
            CheckClass::StatisticalViolation,
        );
    }
    for measure in [
        DifferenceMeasure::MeanAbsolute,
        DifferenceMeasure::P95Absolute,
        DifferenceMeasure::MaxAbsolute,
    ] {
        assert_rejected(
            &format!("difference {measure:?}"),
            numeric.difference().expect(
                DifferenceExpectation::default()
                    .bound(measure, None, Some(-1.))
                    .unwrap(),
            ),
            CheckClass::StatisticalViolation,
        );
    }
}

#[test]
fn every_composite_relation_measure_fails_for_metrics_and_owned_reports() {
    let values = [(1., 19.), (2., 16.), (3., 11.), (4., 4.)];
    let a = source_slice("left", &values);
    let pair = a.compare_with(&a).on(|v| v.0 as u64, |v| v.0 as u64);
    let numeric = pair.numeric(|v| v.0, |v| v.1);
    let joined = join_by(&values, &values, |v| v.0 as u64, |v| v.0 as u64);
    for measure in [
        RelationMeasure::Pearson,
        RelationMeasure::Spearman,
        RelationMeasure::AbsolutePearson,
        RelationMeasure::AbsoluteSpearman,
        RelationMeasure::Slope,
        RelationMeasure::Intercept,
        RelationMeasure::RSquared,
        RelationMeasure::MeanAbsoluteDifference,
        RelationMeasure::P95AbsoluteDifference,
        RelationMeasure::MaxAbsoluteDifference,
        RelationMeasure::ResidualMeanAbsolute,
        RelationMeasure::ResidualMaxAbsolute,
    ] {
        let (min, max) = match measure {
            RelationMeasure::Pearson | RelationMeasure::Spearman | RelationMeasure::Slope => {
                (Some(0.), None)
            }
            RelationMeasure::AbsolutePearson | RelationMeasure::AbsoluteSpearman => {
                (None, Some(0.))
            }
            _ => (None, Some(-1.)),
        };
        let expected = RelationExpectation::default()
            .bound(measure, min, max)
            .unwrap();
        assert_rejected(
            &format!("relation metrics {measure:?}"),
            numeric.relation().expect(expected.clone()),
            CheckClass::StatisticalViolation,
        );
        assert_rejected(
            &format!("relation report {measure:?}"),
            try_fact("owned report", joined.numeric_relation(|v| v.0, |v| v.1, 1)).expect(expected),
            CheckClass::StatisticalViolation,
        );
    }
}

#[test]
fn anomaly_jump_and_group_failures_cannot_silently_pass() {
    let values = [0., 0., 0., 10.];
    let a = source_slice("anomalies", &values);
    assert_rejected(
        "outliers",
        a.numeric(|v| *v)
            .outliers(OutlierConfig::default())
            .expect(AtMostAnomalies(0)),
        CheckClass::StatisticalViolation,
    );
    assert_rejected(
        "jumps",
        a.numeric(|v| *v)
            .jumps(JumpConfig {
                absolute_threshold: Some(1.),
                relative_threshold: None,
                denominator_floor: 1.,
            })
            .expect(AtMostAnomalies(0)),
        CheckClass::StatisticalViolation,
    );
    let groups = a.group_by(|_| "group");
    for expectation in [
        GroupExpectation {
            minimum_count: 5,
            mean_min: 0.,
            mean_max: 10.,
        },
        GroupExpectation {
            minimum_count: 1,
            mean_min: 0.,
            mean_max: 1.,
        },
    ] {
        assert_rejected(
            "group violation",
            fact("groups", groups.numeric(|v| *v).summaries_with_robust()).expect(expectation),
            CheckClass::StatisticalViolation,
        );
    }
}

#[test]
fn every_lag_failure_reason_reaches_final_boundaries() {
    let values = [(0, 0.), (1, 1.), (2, 4.), (3, 9.)];
    let config = LagConfig {
        range: LagRange { start: 0, end: 0 },
        measure: LagMeasure::Pearson,
    };
    let full = lag_relation_by(&values, &values, |v| *v, |v| *v, config).unwrap();
    let unexpected = fact("lag", full.clone()).expect(LagExpectation { best_lag: 1 });
    assert_eq!(unexpected.violations, [LagViolation::UnexpectedBestLag]);
    assert_rejected(
        "unexpected lag",
        unexpected,
        CheckClass::StatisticalViolation,
    );
    let constant = [(0, 1.), (1, 1.), (2, 1.)];
    let undefined = try_fact(
        "lag",
        lag_relation_by(&constant, &constant, |v| *v, |v| *v, config),
    )
    .expect(LagExpectation { best_lag: 0 });
    assert_eq!(undefined.violations, [LagViolation::NoDefinedCandidate]);
    assert_rejected("undefined lag", undefined, CheckClass::StatisticalViolation);
    let gaps = [values[0], values[2], values[3]];
    let interior = try_fact(
        "lag",
        lag_relation_by(&values, &gaps, |v| *v, |v| *v, config),
    )
    .expect(LagExpectation { best_lag: 0 });
    assert_eq!(interior.violations, [LagViolation::InteriorMissingKeys]);
    assert_rejected(
        "interior lag gaps",
        interior,
        CheckClass::StatisticalViolation,
    );
    assert_rejected(
        "direct lag evaluation",
        try_fact("lag", LagExpectation { best_lag: 1 }.evaluate(full)).expect(criterion(
            "available",
            CheckClass::HardViolation,
            |_: &LagReport| true,
        )),
        CheckClass::EvaluationError,
    );
}

#[test]
fn unavailable_invalid_and_undefined_evidence_fails_closed_at_final_boundaries() {
    let error = CheckClass::EvaluationError;
    assert_rejected(
        "empty scalar selection",
        source_slice::<usize>("empty", &[])
            .select(|v| *v)
            .expect(1usize),
        error,
    );
    assert_rejected(
        "unavailable",
        try_fact("artifact", Err::<usize, _>("missing artifact")).expect(1usize),
        error,
    );
    assert_rejected(
        "invalid criterion",
        fact("value", 1).expect(Criterion::Range { min: 2, max: 1 }),
        error,
    );
    assert_rejected(
        "incomparable",
        fact("value", f64::NAN).expect(Criterion::Greater(0.)),
        error,
    );
    assert_rejected(
        "invalid numeric bounds",
        fact("value", 1.).expect(NumericRule::Range { min: 2., max: 1. }),
        error,
    );
    assert_rejected(
        "nonfinite proximity",
        fact("value", f64::INFINITY).expect(close_to(1.)),
        error,
    );
    assert_rejected(
        "invalid predicate class",
        fact("value", 1).expect(criterion("invalid", CheckClass::Pass, |_: &i32| true)),
        error,
    );
    let a = source_slice("invalid data", &[f64::NAN]);
    assert_rejected(
        "invalid summary",
        a.numeric(|v| *v).summary().expect(criterion(
            "available",
            CheckClass::HardViolation,
            |_: &Summary| true,
        )),
        error,
    );
    assert_rejected(
        "invalid group",
        fact("groups", a.group_by(|_| 0).numeric(|v| *v).summaries()).expect(GroupExpectation {
            minimum_count: 1,
            mean_min: 0.,
            mean_max: 1.,
        }),
        error,
    );
    let pairs = [(1., 2.), (1., 2.)];
    assert_rejected(
        "undefined correlation",
        try_fact("constant", correlation(pairs.into_iter()))
            .expect(CorrelationExpectation::positive(0.9).unwrap()),
        error,
    );
    assert_rejected(
        "undefined fit",
        try_fact("constant", linear_fit(pairs.into_iter()))
            .expect(LinearFitExpectation::default().slope(0., 1.).unwrap()),
        error,
    );
    assert_rejected(
        "undefined scalar fit",
        try_fact("constant", linear_fit(pairs.into_iter()))
            .slope()
            .expect(close_to(1.)),
        error,
    );
    assert_rejected(
        "undefined relation",
        try_fact("constant", numeric_relation(pairs.into_iter()))
            .expect(RelationExpectation::positive(0.9).unwrap()),
        error,
    );
    let rows = [(0, 1.), (1, 2.), (2, 3.)];
    let joined = join_by(&rows, &rows, |v| v.0, |v| v.0);
    let report = joined.numeric_relation(|v| v.1, |v| v.1, 1).unwrap();
    let expected = RelationExpectation::negative(-0.9).unwrap();
    assert_rejected(
        "direct relation evaluation",
        try_fact("relation", expected.evaluate(report)).expect(criterion(
            "available",
            CheckClass::HardViolation,
            |_: &RelationReport<i32>| true,
        )),
        error,
    );
    let mut incomplete = joined.numeric_relation(|v| v.1, |v| v.1, 1).unwrap();
    incomplete.comparison = join_by(&rows, &rows[..2], |v| v.0, |v| v.0)
        .comparison()
        .clone();
    assert_rejected(
        "incomplete relation report",
        fact("relation", incomplete).expect(RelationExpectation::positive(0.9).unwrap()),
        error,
    );
}

#[test]
fn compact_failure_entrypoint_reaches_both_final_boundaries() {
    let mut c = certify("compact acceptance");
    c.check_fact_expect("wrong value", 1, 2);
    let report = c.certify();
    assert_eq!(
        (report.checks, report.failed, report.failures.len()),
        (1, 1, 1)
    );
    assert_eq!(report.failures[0].class, CheckClass::HardViolation);
    assert!(!report.to_string().is_empty());
    assert!(catch_unwind(|| c.assert_certify()).is_err());
    assert!(catch_unwind(|| report.assert_certify()).is_err());
}

#[test]
fn positive_final_assertions_are_silent_for_both_boundaries() {
    let mut c = certify("healthy proof");
    c.check_fact_expect("ordinary", 1, 1);
    c.check(
        "statistical",
        try_fact(
            "relation",
            correlation([(1., 2.), (2., 4.), (3., 6.)].into_iter()),
        )
        .expect(CorrelationExpectation::positive(0.9).unwrap()),
    );
    let report = c.certify();
    assert!(report.is_pass());
    assert_eq!(
        (report.checks, report.failed, report.failures.len()),
        (2, 0, 0)
    );
    assert_eq!(report.to_string(), "");
    c.assert_certify();
    report.assert_certify();
}
