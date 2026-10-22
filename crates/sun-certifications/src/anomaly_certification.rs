use super::*;
use crate::test_support::{close, median};

#[test]
fn zero_mad_policy_is_explicit_for_quantized_traces() {
    let values = [10., 10., 9., 11., 10., 10., 100.];
    let series = NumericSeries::new(values.into_iter());
    let exact = series.outliers(OutlierConfig::default()).unwrap();
    assert_eq!(
        exact.suspicious.iter().map(|v| v.index).collect::<Vec<_>>(),
        vec![2, 3, 6]
    );
    let report = series
        .outliers(OutlierConfig {
            threshold: 3.5,
            zero_mad: ZeroMadPolicy::AbsoluteTolerance(1.),
        })
        .unwrap();
    assert_eq!(report.suspicious.len(), 1);
    assert_eq!(report.suspicious[0].index, 6);
    assert_eq!(
        report.suspicious[0].score,
        RobustScore::ZeroMadDeviation {
            signed_deviation: 90.
        }
    );
    assert_eq!(
        serde_json::from_str::<OutlierReport>(&serde_json::to_string(&report).unwrap()).unwrap(),
        report
    );
    assert!(NumericSeries::new([10.; 10].into_iter())
        .outliers(OutlierConfig::default())
        .unwrap()
        .suspicious
        .is_empty());
}

#[test]
fn varied_mad_and_scores_match_independent_order_statistic_reference() {
    for seed in 0..50 {
        let mut values: Vec<_> = (0..41).map(|i| ((i * 13 + seed * 7) % 37) as f64).collect();
        values.push(1000.);
        let center = median(&values);
        let mad = median(
            &values
                .iter()
                .map(|v| (v - center).abs())
                .collect::<Vec<_>>(),
        );
        let expected: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(_, v)| (0.6744897501960817 * (**v - center) / mad).abs() > 3.5)
            .map(|(i, _)| i)
            .collect();
        let report = NumericSeries::new(values.iter().copied())
            .outliers(OutlierConfig::default())
            .unwrap();
        assert_eq!(
            report
                .suspicious
                .iter()
                .map(|v| v.index)
                .collect::<Vec<_>>(),
            expected
        );
        for v in report.suspicious {
            let RobustScore::ModifiedZ(score) = v.score else {
                panic!()
            };
            close(score, 0.6744897501960817 * (v.value - center) / mad);
        }
    }
}

#[test]
fn outlier_empty_nonfinite_and_invalid_configurations_fail_closed() {
    assert_eq!(
        NumericSeries::new([].into_iter()).outliers(OutlierConfig::default()),
        Err(NumericError::Empty)
    );
    assert!(NumericSeries::new([1., f64::NAN].into_iter())
        .outliers(OutlierConfig::default())
        .is_err());
    for threshold in [-1., f64::NAN, f64::INFINITY] {
        assert!(NumericSeries::new([1.].into_iter())
            .outliers(OutlierConfig {
                threshold,
                ..Default::default()
            })
            .is_err());
    }
    assert!(NumericSeries::new([1.].into_iter())
        .outliers(OutlierConfig {
            zero_mad: ZeroMadPolicy::AbsoluteTolerance(-1.),
            ..Default::default()
        })
        .is_err());
}

#[test]
fn jumps_identify_exact_transition_both_endpoints_and_stable_traces() {
    let config = JumpConfig {
        absolute_threshold: Some(2.),
        relative_threshold: None,
        denominator_floor: 1e-6,
    };
    let report = NumericSeries::new([1., 1.1, 1.2, 8., 8.1].into_iter())
        .jumps(config)
        .unwrap();
    assert_eq!(report.record_count, 5);
    assert_eq!(report.transition_count, 4);
    assert_eq!(report.suspicious.len(), 1);
    let jump = &report.suspicious[0];
    assert_eq!(
        (
            jump.from_index,
            jump.to_index,
            jump.from_value,
            jump.to_value
        ),
        (2, 3, 1.2, 8.)
    );
    close(jump.absolute_change, 6.8);
    assert_eq!(report.maximum_absolute.as_ref(), Some(jump));
    assert_eq!(
        serde_json::from_value::<JumpReport>(serde_json::to_value(&report).unwrap()).unwrap(),
        report
    );
    assert!(NumericSeries::new([1., 1.1, 1.2, 1.3].into_iter())
        .jumps(config)
        .unwrap()
        .suspicious
        .is_empty());
    assert_eq!(
        NumericSeries::new([].into_iter())
            .jumps(config)
            .unwrap()
            .transition_count,
        0
    );
    assert!(NumericSeries::new([1.].into_iter())
        .jumps(config)
        .unwrap()
        .maximum_absolute
        .is_none());
}

#[test]
fn jump_relative_floor_threshold_boundaries_and_earliest_ties_are_deterministic() {
    let config = JumpConfig {
        absolute_threshold: None,
        relative_threshold: Some(2.),
        denominator_floor: 0.1,
    };
    let report = NumericSeries::new([0., 0.01, 0., 1., 0.].into_iter())
        .jumps(config)
        .unwrap();
    assert_eq!(
        report
            .suspicious
            .iter()
            .map(|j| (j.from_index, j.to_index))
            .collect::<Vec<_>>(),
        vec![(2, 3)]
    );
    assert_eq!(report.suspicious[0].relative_change, 10.);
    assert_eq!(report.maximum_absolute.unwrap().from_index, 2);
    let exact = NumericSeries::new([0., 1., 0.].into_iter())
        .jumps(JumpConfig {
            absolute_threshold: Some(1.),
            relative_threshold: None,
            ..config
        })
        .unwrap();
    assert!(exact.suspicious.is_empty());
    assert_eq!(exact.maximum_absolute.unwrap().from_index, 0);
    for floor in [0., -1., f64::INFINITY, f64::NAN] {
        assert!(NumericSeries::new([0., 1.].into_iter())
            .jumps(JumpConfig {
                denominator_floor: floor,
                ..config
            })
            .is_err());
    }
    assert!(NumericSeries::new([0., f64::INFINITY].into_iter())
        .jumps(config)
        .is_err());
    assert!(NumericSeries::new([0., 1.].into_iter())
        .jumps(JumpConfig {
            absolute_threshold: Some(-1.),
            ..config
        })
        .is_err());
}
