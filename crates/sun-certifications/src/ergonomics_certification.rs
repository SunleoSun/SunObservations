use crate::*;
#[derive(Debug, PartialEq)]
enum State {
    Ready,
}
#[test]
fn ordinary_equality_specialized_acceptance_and_fallible_evidence_share_one_boundary() {
    assert!(fact("integer", 64usize).expect(64usize).is_pass());
    assert!(fact("boolean", true).expect(true).is_pass());
    assert!(fact("enum", State::Ready).expect(State::Ready).is_pass());
    assert!(fact("str", "reason").expect("reason").is_pass());
    assert!(fact("String", String::from("reason"))
        .expect("reason")
        .is_pass());
    assert!(fact("optional", Some(7)).expect(Some(7)).is_pass());
    assert!(fact("specialized", 64usize)
        .expect(CountRule::NonEmpty)
        .is_pass());
    let failed = fact("integer", 3usize).expect(4usize);
    assert_eq!(failed.class, CheckClass::HardViolation);
    assert_eq!(failed.observed.unwrap(), 3);
    assert_eq!(failed.violations.len(), 1);
    assert_eq!(
        try_fact::<usize, _>("unavailable", Err("decode"))
            .expect(4usize)
            .class,
        CheckClass::EvaluationError
    );
}
#[test]
fn compact_checks_and_source_factories_preserve_explicit_semantics() {
    let mut explicit = certify("same");
    let mut compact = certify("same");
    explicit.check("count", fact("count", 3usize).expect(4usize));
    compact.check_fact_expect("count", 3usize, 4usize);
    assert_eq!(
        explicit.certify().to_string(),
        compact.certify().to_string()
    );
    let rows = [1usize, 2, 3];
    assert_eq!(
        compact.collection("rows", &rows).count().observed.unwrap(),
        source_slice("rows", &rows).count().observed.unwrap()
    );
    assert_eq!(
        compact
            .source("value", &rows[0])
            .select(|v| *v)
            .observed
            .unwrap(),
        source("value", &rows[0]).select(|v| *v).observed.unwrap()
    );
}
#[test]
fn approximate_equality_has_absolute_relative_and_finite_contracts() {
    for (observed, expected) in [(2., 2.), (1e-10, 0.), (-2. - 1e-10, -2.), (1e9 + 0.5, 1e9)] {
        assert!(fact("score", observed).expect(close_to(expected)).is_pass());
    }
    assert!(fact("absolute", 0.1)
        .expect(close_to(0.).absolute(0.2).relative(0.))
        .is_pass());
    assert!(fact("relative", 101.)
        .expect(close_to(100.).relative(0.02).absolute(0.))
        .is_pass());
    assert_eq!(
        fact("outside", 1.01).expect(close_to(1.)).class,
        CheckClass::HardViolation
    );
    for expectation in [
        close_to(f64::NAN),
        close_to(f64::INFINITY),
        close_to(1.).absolute(-1.),
        close_to(1.).relative(f64::NAN),
        close_to(1.).absolute(f64::INFINITY),
        close_to(1.).relative(-1.),
    ] {
        assert_eq!(
            fact("invalid", 1.).expect(expectation).class,
            CheckClass::EvaluationError
        );
    }
    for observed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            fact("invalid evidence", observed)
                .expect(close_to(1.))
                .class,
            CheckClass::EvaluationError
        );
    }
}
#[test]
fn all_failure_classes_collect_and_only_pass_is_success() {
    let mut c = certify("classification");
    c.check_fact_expect("pass", 1usize, 1usize);
    c.check_fact_expect("hard", 1usize, 2usize);
    c.check(
        "statistical",
        fact("score", 0.).expect(criterion(
            "positive",
            CheckClass::StatisticalViolation,
            |v: &f64| *v > 0.,
        )),
    );
    c.check(
        "error",
        try_fact::<usize, _>("decode", Err("unavailable")).expect(1usize),
    );
    let report = c.certify();
    assert_eq!((report.checks, report.failed), (4, 3));
    assert_eq!(
        report.failures.iter().map(|v| v.class).collect::<Vec<_>>(),
        [
            CheckClass::HardViolation,
            CheckClass::StatisticalViolation,
            CheckClass::EvaluationError
        ]
    );
    assert!(std::panic::catch_unwind(|| report.assert_certify()).is_err());
    assert_eq!(certify("silent").certify().to_string(), "");
}
#[test]
fn correlation_fit_difference_and_composite_reuse_prepared_correspondence() {
    use std::cell::Cell;
    let values: Vec<_> = (0..10_000).map(|row| (row, row as f64)).collect();
    let outputs: Vec<_> = values
        .iter()
        .map(|(row, v)| (*row, 2. * v + 3.))
        .rev()
        .collect();
    let keys = Cell::new(0);
    let projections = Cell::new(0);
    let left = source_slice("input", &values);
    let right = source_slice("output", &outputs);
    let pair = left.compare_with(&right).on(
        |v| {
            keys.set(keys.get() + 1);
            v.0
        },
        |v| {
            keys.set(keys.get() + 1);
            v.0
        },
    );
    assert!(pair.comparison_fact().expect_valid().is_pass());
    assert_eq!(pair.alignment().matched.len(), 10_000);
    let numeric = pair.numeric(
        |v| {
            projections.set(projections.get() + 1);
            v.1
        },
        |v| {
            projections.set(projections.get() + 1);
            v.1
        },
    );
    for _ in 0..4 {
        assert!(numeric
            .correlation()
            .expect(CorrelationExpectation::positive(0.99).unwrap())
            .is_pass());
        assert!(numeric
            .linear_fit()
            .expect(
                LinearFitExpectation::default()
                    .slope(1.99, 2.01)
                    .unwrap()
                    .intercept(2.99, 3.01)
                    .unwrap()
                    .r_squared(0.99)
                    .unwrap()
                    .residual_mean_absolute(1e-9)
                    .unwrap()
                    .residual_max_absolute(1e-9)
                    .unwrap()
            )
            .is_pass());
        assert!(numeric.linear_fit().slope().expect(close_to(2.)).is_pass());
        assert!(numeric
            .difference()
            .expect(
                DifferenceExpectation::default()
                    .max_absolute(10002.)
                    .unwrap()
            )
            .is_pass());
        assert_eq!(
            numeric.relation().observed.unwrap(),
            numeric_relation(numeric.values().iter().copied()).unwrap()
        );
    }
    assert_eq!(keys.get(), 20_000);
    assert_eq!(projections.get(), 20_000);
    let missing = source_slice("missing", &outputs[1..]);
    let incomplete = left.compare_with(&missing).on(|v| v.0, |v| v.0);
    assert_eq!(
        incomplete.comparison_fact().expect_valid().class,
        CheckClass::HardViolation
    );
    assert_eq!(
        incomplete
            .numeric(|v| v.1, |v| v.1)
            .correlation()
            .expect(CorrelationExpectation::positive(0.9).unwrap())
            .class,
        CheckClass::EvaluationError
    );
    let duplicates = [(0, 1.), (0, 2.)];
    let duplicate_source = source_slice("duplicate", &duplicates);
    let duplicate_pair = left.compare_with(&duplicate_source).on(|v| v.0, |v| v.0);
    assert_eq!(duplicate_pair.alignment().duplicate_right.len(), 1);
    assert!(!duplicate_pair.comparison_fact().expect_valid().is_pass());
}
#[test]
fn composite_relation_invalid_preconditions_match_direct_and_collect_all_acceptance() {
    let values = [1., 2., 3.];
    let report = join_by(&values, &values, |v| *v as i64, |v| *v as i64)
        .numeric_relation(|v| *v, |v| *v, 0)
        .unwrap();
    let invalid = RelationExpectation {
        constraints: vec![RelationConstraint {
            measure: RelationMeasure::Slope,
            min: Some(2.),
            max: Some(1.),
        }],
    };
    let comparison = fact("relation", report.clone()).expect(invalid.clone());
    assert_eq!(comparison.class, CheckClass::EvaluationError);
    let failure = invalid.evaluate(report).unwrap_err();
    assert_eq!(failure.analysis_error, comparison.analysis_error);
    assert!(failure.violated.is_empty());
    let restored: RelationFailure<i64> =
        serde_json::from_value(serde_json::to_value(&failure).unwrap()).unwrap();
    assert_eq!(restored, *failure);
}
#[test]
fn correlation_known_positive_negative_zero_and_undefined_fixtures() {
    for (pairs, expectation) in [
        (
            [(1., 2.), (2., 4.), (3., 6.)],
            CorrelationExpectation::positive(0.99).unwrap(),
        ),
        (
            [(1., -2.), (2., -4.), (3., -6.)],
            CorrelationExpectation::negative(-0.99).unwrap(),
        ),
    ] {
        assert!(try_fact("fixture", correlation(pairs.into_iter()))
            .expect(expectation)
            .is_pass());
    }
    let zero = [(-1., 1.), (0., -2.), (1., 1.)];
    assert!(try_fact("zero", correlation(zero.into_iter()))
        .expect(
            CorrelationExpectation::default()
                .pearson(Some(-1e-9), Some(1e-9))
                .unwrap()
                .spearman(Some(-1e-9), Some(1e-9))
                .unwrap()
        )
        .is_pass());
    let undefined = correlation([(1., 1.), (1., 1.)].into_iter());
    assert_eq!(
        try_fact("undefined", undefined)
            .expect(CorrelationExpectation::positive(0.9).unwrap())
            .class,
        CheckClass::EvaluationError
    );
}
