use super::*;
use crate::{
    join_by,
    test_support::{close, reference_linear, reference_rank, shuffle},
};

fn defined(metric: Metric, expected: f64) {
    close(metric.value().unwrap(), expected);
}

#[test]
fn positive_negative_nonlinear_and_known_affine_transforms_are_certified() {
    let x = [1., 2., 3., 4., 5.];
    for (y, slope, intercept, direction) in [
        ([2., 4., 6., 8., 10.], 2., 0., 1.),
        ([10., 8., 6., 4., 2.], -2., 12., -1.),
        ([5., 7., 9., 11., 13.], 2., 3., 1.),
    ] {
        let report = numeric_relation(x.into_iter().zip(y)).unwrap();
        defined(report.linear.pearson, direction);
        defined(report.spearman, direction);
        defined(report.linear.slope, slope);
        defined(report.linear.intercept, intercept);
        defined(report.linear.r_squared, 1.);
        defined(report.linear.covariance, 2. * slope);
        defined(report.linear.sample_covariance, 2.5 * slope);
        close(report.residual.unwrap().absolute.max, 0.);
    }
    defined(
        spearman(x.into_iter().zip([1., 4., 9., 16., 25.])).unwrap(),
        1.,
    );
}

#[test]
fn shifted_bivariate_moments_preserve_offset_relationship_and_detect_variance_underflow() {
    let mut values: Vec<_> = (0..128)
        .map(|i| {
            let x = 1e12 + i as f64 / 4.;
            (x, 2. * x + 3.)
        })
        .collect();
    shuffle(&mut values, 31);
    let actual = linear_metrics(values).unwrap();
    defined(actual.pearson, 1.);
    defined(actual.slope, 2.);
    defined(actual.intercept, 3.);
    assert!(matches!(
        pearson([(0., 0.), (1e-200, 1.)]),
        Err(NumericError::ArithmeticUnderflow { .. })
    ));
    assert!(matches!(
        pearson([(0., 0.), (1., 1e-200)]),
        Err(NumericError::ArithmeticUnderflow { .. })
    ));
}

#[test]
fn ties_use_average_ranks_including_signed_zero() {
    defined(
        spearman([1., 2., 2., 3.].into_iter().zip([4., 1., 1., 0.])).unwrap(),
        -1.,
    );
    let x = [-0., 0., 1., 2., 2.];
    let y = [1., 2., 2., 4., 3.];
    let rx = reference_rank(&x);
    let ry = reference_rank(&y);
    let expected = reference_linear(&rx.into_iter().zip(ry).collect::<Vec<_>>()).0;
    defined(spearman(x.into_iter().zip(y)).unwrap(), expected);
}

#[test]
fn empty_singleton_constant_and_nonfinite_metrics_never_succeed_as_nan() {
    assert_eq!(
        pearson([]).unwrap(),
        Metric::Undefined(UndefinedReason::InsufficientPairs)
    );
    assert_eq!(
        spearman([(1., 2.)]).unwrap(),
        Metric::Undefined(UndefinedReason::InsufficientPairs)
    );
    assert_eq!(
        pearson([(1., 2.), (1., 3.)]).unwrap(),
        Metric::Undefined(UndefinedReason::ConstantLeft)
    );
    assert_eq!(
        spearman([(1., 2.), (2., 2.)]).unwrap(),
        Metric::Undefined(UndefinedReason::ConstantRight)
    );
    let constant = linear_metrics([(1., 2.), (2., 2.)]).unwrap();
    assert_eq!(constant.slope, Metric::Defined(0.));
    assert_eq!(constant.intercept, Metric::Defined(2.));
    assert_eq!(
        constant.r_squared,
        Metric::Undefined(UndefinedReason::ConstantRight)
    );
    assert_eq!(numeric_relation([].into_iter()), Err(NumericError::Empty));
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(pearson([(1., 2.), (v, 3.)]).is_err());
        assert!(spearman([(1., v)]).is_err());
        assert!(difference_summary([(v, 1.)]).is_err());
    }
    assert!(linear_metrics([(f64::MAX, 0.), (-f64::MAX, 1.)]).is_err());
}

#[test]
fn pair_differences_percentiles_and_top_keys_have_reference_values() {
    let left = [(3, 30.), (1, 10.), (2, 20.)];
    let right = [(2, 18.), (3, 35.), (1, 11.)];
    let joined = join_by(&left, &right, |v| v.0, |v| v.0);
    let report = joined.difference(|v| v.1, |v| v.1, 2).unwrap();
    close(report.summary.signed.mean, 4. / 3.);
    close(report.summary.absolute.mean, 8. / 3.);
    assert_eq!(report.summary.median_signed, 1.);
    assert_eq!(report.summary.median_absolute, 2.);
    close(report.summary.p95_absolute, 4.7);
    assert_eq!(report.summary.absolute.max, 5.);
    assert_eq!(
        report.largest.iter().map(|v| v.key).collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(report.largest[0].signed_difference, 5.);
    assert!(joined
        .difference(|v| v.1, |v| v.1, 0)
        .unwrap()
        .largest
        .is_empty());
    assert_eq!(
        serde_json::from_value::<DifferenceReport<i32>>(serde_json::to_value(&report).unwrap())
            .unwrap(),
        report
    );
}

#[test]
fn bounded_top_disagreements_match_full_sort_for_all_limits_and_ties() {
    let left: Vec<_> = (0..100).map(|key| (key, 0.)).collect();
    let right: Vec<_> = (0..100)
        .map(|key| (key, ((key * 37) % 11) as f64 - 5.))
        .collect();
    let joined = join_by(&left, &right, |v| v.0, |v| v.0);
    let mut expected = right.clone();
    expected.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()).then(a.0.cmp(&b.0)));
    for top in [0, 1, 2, 5, 99, 100, 200] {
        let report = joined.difference(|v| v.1, |v| v.1, top).unwrap();
        assert_eq!(
            report
                .largest
                .iter()
                .map(|v| (v.key, v.right))
                .collect::<Vec<_>>(),
            expected.iter().take(top).copied().collect::<Vec<_>>()
        );
    }
}

#[test]
fn numeric_join_failures_retain_alignment_and_pair_to_source_coordinates() {
    let left = [(7, 1.), (8, 2.)];
    let right = [(8, f64::NAN), (7, 3.)];
    let joined = join_by(&left, &right, |v| v.0, |v| v.0);
    let Err(RelationError::Numeric { alignment, error }) =
        joined.numeric_relation(|v| v.1, |v| v.1, 1)
    else {
        panic!()
    };
    assert_eq!(
        error,
        NumericError::NonFinite {
            index: 1,
            value: crate::NumericValue::Nan
        }
    );
    assert_eq!(alignment.matched[1].key, 8);
    assert_eq!(alignment.matched[1].right_index, 0);
    assert!(serde_json::to_string(&RelationError::Numeric { alignment, error }).is_ok());
}

#[test]
fn scenario_expectations_distinguish_direction_scale_and_residuals() {
    let x = [1., 2., 3., 4., 5.];
    for sign in [1., -1.] {
        let y = x.map(|v| sign * 2. * v + 3.);
        let joined = join_by(&x, &y, |v| *v as i64, |v| ((*v - 3.) / (sign * 2.)) as i64);
        let report = joined.numeric_relation(|v| *v, |v| *v, 2).unwrap();
        let expectation = if sign > 0. {
            RelationExpectation::positive(0.9)
        } else {
            RelationExpectation::negative(-0.9)
        }
        .unwrap()
        .bound(
            RelationMeasure::Slope,
            Some(sign * 2. - 0.01),
            Some(sign * 2. + 0.01),
        )
        .unwrap()
        .bound(RelationMeasure::Intercept, Some(2.99), Some(3.01))
        .unwrap()
        .bound(RelationMeasure::RSquared, Some(0.99), None)
        .unwrap()
        .bound(RelationMeasure::ResidualMaxAbsolute, None, Some(1e-10))
        .unwrap();
        assert!(expectation.evaluate(report.clone()).is_ok());
        let failure = if sign > 0. {
            RelationExpectation::negative(-0.8)
        } else {
            RelationExpectation::positive(0.8)
        }
        .unwrap()
        .evaluate(report)
        .unwrap_err();
        assert_eq!(failure.violated.len(), 1);
        assert_eq!(failure.observed.observed.linear.count, 5);
        let json = serde_json::to_value(&failure).unwrap();
        assert_eq!(
            serde_json::from_value::<RelationFailure<i64>>(json).unwrap(),
            *failure
        );
    }
    let y = x.map(|v| 100. * v);
    let joined = join_by(&x, &y, |v| *v as i64, |v| (*v / 100.) as i64);
    let report = joined.numeric_relation(|v| *v, |v| *v, 1).unwrap();
    assert!(RelationExpectation::positive(0.99)
        .unwrap()
        .evaluate(report.clone())
        .is_ok());
    let failure = RelationExpectation::positive(0.99)
        .unwrap()
        .bound(RelationMeasure::Slope, Some(0.9), Some(1.1))
        .unwrap()
        .evaluate(report)
        .unwrap_err();
    assert_eq!(failure.violated[0].measure, RelationMeasure::Slope);
    defined(failure.observed.observed.linear.slope, 100.);
}

#[test]
fn independent_constraint_measures_and_undefined_values_fail_closed() {
    let left = [1., 2., 3.];
    let right = [3., 2., 1.];
    let joined = join_by(&left, &right, |v| *v as i32, |v| (4. - *v) as i32);
    let report = joined.numeric_relation(|v| *v, |v| *v, 3).unwrap();
    for measure in [
        RelationMeasure::Pearson,
        RelationMeasure::Spearman,
        RelationMeasure::AbsolutePearson,
        RelationMeasure::AbsoluteSpearman,
        RelationMeasure::MeanAbsoluteDifference,
        RelationMeasure::P95AbsoluteDifference,
        RelationMeasure::MaxAbsoluteDifference,
        RelationMeasure::ResidualMeanAbsolute,
    ] {
        let value = super::measure(&report.observed, measure).unwrap();
        assert!(RelationExpectation::default()
            .bound(measure, Some(value - 1e-9), Some(value + 1e-9))
            .unwrap()
            .evaluate(report.clone())
            .is_ok());
        assert!(RelationExpectation::default()
            .bound(measure, Some(value + 1.), None)
            .unwrap()
            .evaluate(report.clone())
            .is_err());
    }
    assert!(RelationExpectation::default()
        .bound(RelationMeasure::Slope, Some(2.), Some(1.))
        .is_err());
    for invalid in [0., -0.5, 1.1, f64::NAN] {
        assert!(RelationExpectation::positive(invalid).is_err());
    }
    for invalid in [0., 0.5, -1.1, f64::NAN] {
        assert!(RelationExpectation::negative(invalid).is_err());
    }
    let constant = join_by(&left, &left, |v| *v as i32, |v| *v as i32)
        .numeric_relation(|_| 1., |v| *v, 1)
        .unwrap();
    assert!(RelationExpectation::positive(0.1)
        .unwrap()
        .evaluate(constant)
        .is_err());
}

#[test]
fn generated_centered_moments_ranking_and_fit_match_independent_references() {
    for seed in 1..80 {
        let mut values: Vec<_> = (0..60)
            .map(|i| {
                (
                    ((i * 7 + seed) % 19) as f64,
                    ((i * 11 + seed * 3) % 23) as f64,
                )
            })
            .collect();
        shuffle(&mut values, seed);
        let expected = reference_linear(&values);
        let actual = linear_metrics(values.iter().copied()).unwrap();
        for (metric, value) in [
            (actual.pearson, expected.0),
            (actual.covariance, expected.1),
            (actual.slope, expected.2),
            (actual.intercept, expected.3),
            (actual.r_squared, expected.4),
        ] {
            defined(metric, value);
        }
        let rx = reference_rank(&values.iter().map(|v| v.0).collect::<Vec<_>>());
        let ry = reference_rank(&values.iter().map(|v| v.1).collect::<Vec<_>>());
        defined(
            spearman(values.iter().copied()).unwrap(),
            reference_linear(&rx.into_iter().zip(ry).collect::<Vec<_>>()).0,
        );
    }
}
