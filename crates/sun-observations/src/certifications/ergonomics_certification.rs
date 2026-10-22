use crate::certifications::*;

#[test]
fn static_bounds_and_nonempty_constraints_are_validated_at_construction() {
    assert!(MetricConstraint::new(CorrelationMeasure::Pearson, None, None).is_err());
    assert!(GroupExpectation::new(1, 2., 1.).is_err());
    assert!(GroupExpectation::new(1, f64::NAN, 1.).is_err());
    assert!(JumpConfig::new(None, None, 1.).is_err());
    assert!(serde_json::from_str::<NumericRule>(r#"{"Range":[2.0,1.0]}"#).is_err());
    assert!(
        serde_json::from_str::<MetricConstraint<CorrelationMeasure>>(
            r#"{"measure":"Pearson","min":null,"max":null}"#
        )
        .is_err()
    );
}
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
    let mut explicit = certification("same");
    let mut compact = certification("same");
    explicit.check("count", fact("count", 3usize).expect(4usize));
    compact.check_fact_expect("count", 3usize, 4usize);
    assert_eq!(explicit.report().to_string(), compact.report().to_string());
    let rows = [1usize, 2, 3];
    assert_eq!(
        compact.collection("rows", &rows).count().observed.unwrap(),
        collection("rows", &rows).count().observed.unwrap()
    );
    assert_eq!(
        compact
            .source("value", &rows[0])
            .single_map(|v| *v)
            .observed
            .unwrap(),
        source("value", &rows[0])
            .single_map(|v| *v)
            .observed
            .unwrap()
    );
}
#[test]
fn approximate_equality_has_absolute_relative_and_finite_contracts() {
    for (observed, expected) in [(2., 2.), (1e-10, 0.), (-2. - 1e-10, -2.), (1e9 + 0.5, 1e9)] {
        assert!(fact("score", observed)
            .expect(close_to(expected).unwrap())
            .is_pass());
    }
    assert!(fact("absolute", 0.1)
        .expect(
            close_to(0.)
                .unwrap()
                .absolute_tolerance(0.2)
                .unwrap()
                .relative_tolerance(0.)
                .unwrap()
        )
        .is_pass());
    assert!(fact("relative", 101.)
        .expect(
            close_to(100.)
                .unwrap()
                .relative_tolerance(0.02)
                .unwrap()
                .absolute_tolerance(0.)
                .unwrap()
        )
        .is_pass());
    assert_eq!(
        fact("outside", 1.01).expect(close_to(1.).unwrap()).class,
        CheckClass::HardViolation
    );
    for expectation in [
        close_to(f64::NAN),
        close_to(f64::INFINITY),
        close_to(1.).unwrap().absolute_tolerance(-1.),
        close_to(1.).unwrap().relative_tolerance(f64::NAN),
        close_to(1.).unwrap().absolute_tolerance(f64::INFINITY),
        close_to(1.).unwrap().relative_tolerance(-1.),
    ] {
        assert!(expectation.is_err());
    }
    for observed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            fact("invalid evidence", observed)
                .expect(close_to(1.).unwrap())
                .class,
            CheckClass::EvaluationError
        );
    }
}
#[test]
fn all_failure_classes_collect_and_only_pass_is_success() {
    let mut c = certification("classification");
    c.check_fact_expect("pass", 1usize, 1usize);
    c.check_fact_expect("hard", 1usize, 2usize);
    c.check(
        "statistical",
        fact("score", 0.).expect(criterion(
            "positive",
            ViolationClass::Statistical,
            |v: &f64| *v > 0.,
        )),
    );
    c.check(
        "error",
        try_fact::<usize, _>("decode", Err("unavailable")).expect(1usize),
    );
    let report = c.report();
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
    let mut silent = certification("silent");
    silent.check("meaningful", fact("value", 1).expect(1));
    assert_eq!(silent.report().to_string(), "");
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
    let left = collection("input", &values);
    let right = collection("output", &outputs);
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
    assert!(pair.paired_comparison_fact().expect_valid().is_pass());
    assert_eq!(pair.comparison().matched.len(), 10_000);
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
            .expect(CorrelationExpectation::spearman_at_least(0.99).unwrap())
            .is_pass());
        assert!(numeric
            .linear_fit()
            .expect(
                LinearFitExpectation::slope_between(1.99, 2.01)
                    .unwrap()
                    .and_intercept_between(2.99, 3.01)
                    .unwrap()
                    .and_r_squared_at_least(0.99)
                    .unwrap()
                    .and_residual_mean_absolute_at_most(1e-9)
                    .unwrap()
                    .and_residual_max_absolute_at_most(1e-9)
                    .unwrap()
            )
            .is_pass());
        assert!(numeric
            .linear_fit()
            .slope()
            .expect(close_to(2.).unwrap())
            .is_pass());
        assert!(numeric
            .difference()
            .expect(DifferenceExpectation::max_absolute_at_most(10002.).unwrap())
            .is_pass());
        assert_eq!(
            numeric.relation().observed.unwrap(),
            numeric_relation(numeric.values().iter().copied()).unwrap()
        );
    }
    assert_eq!(keys.get(), 20_000);
    assert_eq!(projections.get(), 20_000);
    let missing = collection("missing", &outputs[1..]);
    let incomplete = left.compare_with(&missing).on(|v| v.0, |v| v.0);
    assert_eq!(
        incomplete.paired_comparison_fact().expect_valid().class,
        CheckClass::HardViolation
    );
    assert_eq!(
        incomplete
            .numeric(|v| v.1, |v| v.1)
            .correlation()
            .expect(CorrelationExpectation::spearman_at_least(0.9).unwrap())
            .class,
        CheckClass::EvaluationError
    );
    let duplicates = [(0, 1.), (0, 2.)];
    let duplicate_source = collection("duplicate", &duplicates);
    let duplicate_pair = left.compare_with(&duplicate_source).on(|v| v.0, |v| v.0);
    assert_eq!(duplicate_pair.comparison().duplicate_right.len(), 1);
    assert!(!duplicate_pair
        .paired_comparison_fact()
        .expect_valid()
        .is_pass());
}
#[test]
fn composite_relation_invalid_bounds_and_empty_constraints_fail_at_construction() {
    assert!(RelationExpectation::with_bound(RelationMeasure::Slope, Some(2.), Some(1.)).is_err());
    assert!(RelationExpectation::with_bound(RelationMeasure::Slope, None, None).is_err());
    assert!(serde_json::from_str::<RelationExpectation>(r#"{"constraints":[]}"#).is_err());
}
#[test]
fn correlation_known_positive_negative_zero_and_undefined_fixtures() {
    for (pairs, expectation) in [
        (
            [(1., 2.), (2., 4.), (3., 6.)],
            CorrelationExpectation::spearman_at_least(0.99).unwrap(),
        ),
        (
            [(1., -2.), (2., -4.), (3., -6.)],
            CorrelationExpectation::spearman_at_most(-0.99).unwrap(),
        ),
    ] {
        assert!(try_fact("fixture", correlation(pairs.into_iter()))
            .expect(expectation)
            .is_pass());
    }
    let zero = [(-1., 1.), (0., -2.), (1., 1.)];
    assert!(try_fact("zero", correlation(zero.into_iter()))
        .expect(
            CorrelationExpectation::with_bound(
                CorrelationMeasure::Pearson,
                Some(-1e-9),
                Some(1e-9)
            )
            .unwrap()
            .and_spearman_bounds(Some(-1e-9), Some(1e-9))
            .unwrap()
        )
        .is_pass());
    let undefined = correlation([(1., 1.), (1., 1.)].into_iter());
    assert_eq!(
        try_fact("undefined", undefined)
            .expect(CorrelationExpectation::spearman_at_least(0.9).unwrap())
            .class,
        CheckClass::EvaluationError
    );
}

#[test]
fn typed_facts_checks_and_comparison_reports_share_the_public_contract() {
    let observed: Fact<usize> = fact("returned count", 3);
    let checked: CheckResult<usize> = observed.expect(3);
    let unavailable: Fact<usize> = try_fact("unavailable", Err("missing artifact"));
    assert_eq!(unavailable.expect(3).class, CheckClass::EvaluationError);
    let left = [(1u64, 2.), (2, 4.), (3, 8.)];
    let right = [(3u64, 8.), (1, 2.), (2, 4.)];
    let a = collection("returned", &left);
    let b = collection("persisted", &right);
    let prepared = a.compare_with(&b).on(|r| r.0, |r| r.0);
    let comparison: &Comparison<u64> = prepared.comparison();
    let structural_fact: Fact<&Comparison<u64>> = prepared.paired_comparison_fact();
    let structural_check: CheckResult<&Comparison<u64>> = structural_fact.expect_valid();
    assert!(std::ptr::eq(
        *structural_check.observed.as_ref().unwrap(),
        comparison
    ));
    let mut c = certification("typed public flow");
    c.check("count", checked)
        .check("correspondence", structural_check);
    assert!(c.report().is_pass());
    c.assert_certify();
    let incomplete = a
        .compare_with(&collection("partial", &right[..2]))
        .on(|r| r.0, |r| r.0);
    assert!(!incomplete.comparison().is_complete());
    assert_eq!(
        incomplete
            .numeric(|r| r.1, |r| r.1)
            .relation()
            .expect(RelationExpectation::spearman_at_least(0.9).unwrap())
            .class(),
        CheckClass::EvaluationError
    );
    assert!(serde_json::to_value(comparison).is_ok());
}
