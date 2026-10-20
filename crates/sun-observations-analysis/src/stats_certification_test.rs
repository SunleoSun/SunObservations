use super::*;
use crate::test_support::{close, median, shuffle};

#[test]
fn known_population_sample_quantile_and_mad_results() {
    let values = [1., 2., 3., 4., 5.];
    let s = NumericSeries::new(values.into_iter());
    let summary = s.summary().unwrap();
    assert_eq!(summary.count, 5);
    assert_eq!(summary.finite_count, 5);
    assert_eq!(
        (
            summary.min,
            summary.max,
            summary.sum,
            summary.mean,
            summary.variance,
            summary.sample_variance
        ),
        (1., 5., 15., 3., 2., Some(2.5))
    );
    close(summary.standard_deviation, 2f64.sqrt());
    assert_eq!(s.quantile(0.).unwrap(), 1.);
    assert_eq!(s.quantile(1.).unwrap(), 5.);
    assert_eq!(s.quantile(0.125).unwrap(), 1.5);
    assert_eq!(
        s.robust_summary().unwrap(),
        RobustSummary {
            count: 5,
            median: 3.,
            q1: 2.,
            q3: 4.,
            iqr: 2.,
            mad: 1.
        }
    );
    let even = NumericSeries::new([4., 1., 3., 2.].into_iter());
    assert_eq!(even.quantile(0.5).unwrap(), 2.5);
    assert_eq!(even.robust_summary().unwrap().mad, 1.);
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(serde_json::from_value::<Summary>(json).unwrap(), summary);
}

#[test]
fn empty_constant_singleton_and_invalid_parameters_are_explicit() {
    let empty = NumericSeries::new([].into_iter());
    assert_eq!(empty.summary(), Err(NumericError::Empty));
    assert_eq!(empty.quantile(0.5), Err(NumericError::Empty));
    assert_eq!(empty.robust_summary(), Err(NumericError::Empty));
    for values in [vec![7.], vec![7.; 4]] {
        let series = NumericSeries::new(values.iter().copied());
        assert_eq!(series.summary().unwrap().variance, 0.);
        assert_eq!(
            series.summary().unwrap().sample_variance,
            (values.len() > 1).then_some(0.)
        );
        assert_eq!(series.robust_summary().unwrap().mad, 0.);
        for p in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                series.quantile(p),
                Err(NumericError::InvalidParameter { .. })
            ));
        }
    }
}

#[test]
fn non_finite_inputs_and_finite_arithmetic_overflow_fail_without_nan_reports() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let series = NumericSeries::new([1., value].into_iter());
        assert_eq!(
            series.finite_count(),
            FiniteCount {
                count: 2,
                finite_count: 1
            }
        );
        for result in [
            series.summary().map(|_| ()),
            series.robust_summary().map(|_| ()),
            series.quantile(0.5).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(
                error,
                NumericError::NonFinite {
                    index: 1,
                    value: value.into()
                }
            );
            assert_eq!(
                serde_json::from_str::<NumericError>(&serde_json::to_string(&error).unwrap())
                    .unwrap(),
                error
            );
        }
    }
    assert!(matches!(
        NumericSeries::new([f64::MAX, f64::MAX].into_iter()).summary(),
        Err(NumericError::ArithmeticOverflow { .. })
    ));
    assert!(matches!(
        NumericSeries::new([-f64::MAX, -f64::MAX, f64::MAX].into_iter()).robust_summary(),
        Err(NumericError::ArithmeticOverflow { .. })
    ));
}

#[test]
fn generated_streaming_and_robust_statistics_match_independent_two_pass_reference() {
    for size in 1..128 {
        let mut values: Vec<_> = (0..size)
            .map(|i| ((i * 37 + size * 11) % 101) as f64 / 7.0 - 5.)
            .collect();
        shuffle(&mut values, size as u64);
        let series = NumericSeries::new(values.iter().copied());
        let summary = series.summary().unwrap();
        let mean = values.iter().sum::<f64>() / size as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / size as f64;
        close(summary.mean, mean);
        close(summary.variance, variance);
        close(summary.sum, values.iter().sum());
        let robust = series.robust_summary().unwrap();
        close(robust.median, median(&values));
        close(
            robust.mad,
            median(
                &values
                    .iter()
                    .map(|v| (v - median(&values)).abs())
                    .collect::<Vec<_>>(),
            ),
        );
    }
}

#[test]
fn shifted_moments_preserve_small_variation_over_large_offsets_and_reject_underflow() {
    let mut values: Vec<_> = (0..128).map(|i| 1e12 + i as f64 / 4.).collect();
    shuffle(&mut values, 57);
    let actual = NumericSeries::new(values.iter().copied())
        .summary()
        .unwrap();
    close(actual.mean - 1e12, 15.875);
    close(actual.variance, (128f64.powi(2) - 1.) / 192.);
    assert!(matches!(
        NumericSeries::new([0., 1e-200].into_iter()).summary(),
        Err(NumericError::ArithmeticUnderflow { .. })
    ));
}

#[test]
fn compensated_sum_retains_cancellation_terms() {
    close(
        NumericSeries::new([1e16, 1., -1e16].into_iter())
            .summary()
            .unwrap()
            .mean,
        1. / 3.,
    );
    assert_eq!(
        NumericSeries::new([1e16, 1., -1e16].into_iter())
            .summary()
            .unwrap()
            .sum,
        1.
    );
}
