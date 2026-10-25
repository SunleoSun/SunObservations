use super::*;
use crate::certifications::test_support::Mismatches;
use crate::certifications::*;

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Thresholds {
    Absolute,
    Relative,
    Both,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Boundary {
    Below,
    Exact,
    Above,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Floor {
    Below,
    Equal,
    Above,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Trace {
    Empty,
    Singleton,
    Stable,
    Rising,
    Falling,
    RepeatedMaxima,
    CrossingZero,
    NonFinite,
}
#[derive(DomainInput)]
struct Scenario {
    thresholds: Thresholds,
    absolute: Boundary,
    relative: Boundary,
    floor: Floor,
    trace: Trace,
}
fn execute(s: &Scenario) -> Result<Vec<String>, ()> {
    let values: Vec<f64> = match s.trace {
        Trace::Empty => vec![],
        Trace::Singleton => vec![2.],
        Trace::Stable => vec![2.; 9],
        Trace::Rising => vec![2., 4., 6., 10.],
        Trace::Falling => vec![-2., -4., -6., -10.],
        Trace::RepeatedMaxima => vec![2., 4., 2., 4., 2.],
        Trace::CrossingZero => vec![0., 2., 0., -2., 0.],
        Trace::NonFinite => vec![2., f64::NAN, 4.],
    };
    let floor = match s.floor {
        Floor::Below => 1.,
        Floor::Equal => 2.,
        Floor::Above => 8.,
    };
    let absolute = match s.absolute {
        Boundary::Below => 1.,
        Boundary::Exact => 2.,
        Boundary::Above => 3.,
    };
    let relative = match s.relative {
        Boundary::Below => 0.5,
        Boundary::Exact => 1.,
        Boundary::Above => 1.5,
    };
    let config = JumpConfig::new(
        if s.thresholds != Thresholds::Relative {
            Some(absolute)
        } else {
            None
        },
        if s.thresholds != Thresholds::Absolute {
            Some(relative)
        } else {
            None
        },
        floor,
    )
    .unwrap();
    let actual = NumericSeries::new(values.iter().copied()).jumps(config);
    let mut m = Mismatches::default();
    if let Some(index) = values.iter().position(|v| !v.is_finite()) {
        m.equal(
            "nonfinite exact error",
            &actual,
            &Err(NumericError::NonFinite {
                index,
                value: NumericValue::Nan,
            }),
        );
        return Ok(m.finish());
    }
    let actual = actual.unwrap();
    let adjacent: Vec<_> = values
        .windows(2)
        .enumerate()
        .map(|(i, p)| Jump {
            from_index: i,
            to_index: i + 1,
            from_value: p[0],
            to_value: p[1],
            signed_change: p[1] - p[0],
            absolute_change: (p[1] - p[0]).abs(),
            relative_change: (p[1] - p[0]).abs() / p[0].abs().max(floor),
        })
        .collect();
    let suspicious: Vec<_> = adjacent
        .iter()
        .filter(|j| {
            (s.thresholds != Thresholds::Relative && j.absolute_change > absolute)
                || (s.thresholds != Thresholds::Absolute && j.relative_change > relative)
        })
        .cloned()
        .collect();
    let mut by_absolute = adjacent.clone();
    by_absolute.sort_by(|a, b| {
        b.absolute_change
            .total_cmp(&a.absolute_change)
            .then(a.from_index.cmp(&b.from_index))
    });
    let mut by_relative = adjacent.clone();
    by_relative.sort_by(|a, b| {
        b.relative_change
            .total_cmp(&a.relative_change)
            .then(a.from_index.cmp(&b.from_index))
    });
    m.equal(
        "adjacent threshold evidence",
        &actual.suspicious,
        &suspicious,
    );
    m.equal(
        "earliest absolute maximum",
        &actual.maximum_absolute,
        &by_absolute.first().cloned(),
    );
    m.equal(
        "earliest relative maximum",
        &actual.maximum_relative,
        &by_relative.first().cloned(),
    );
    m.equal("record count", &actual.record_count, &values.len());
    m.equal(
        "transition count",
        &actual.transition_count,
        &values.len().saturating_sub(1),
    );
    if !values.is_empty() {
        let transformed: Vec<_> = values.iter().map(|v| v * 4. + 16.).collect();
        for tolerance in [0., 1.] {
            let a = NumericSeries::new(values.iter().copied())
                .outliers(
                    OutlierConfig::new(3.5, ZeroMadPolicy::absolute_tolerance(tolerance).unwrap())
                        .unwrap(),
                )
                .unwrap();
            let b = NumericSeries::new(transformed.iter().copied())
                .outliers(
                    OutlierConfig::new(
                        3.5,
                        ZeroMadPolicy::absolute_tolerance(tolerance * 4.).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            m.equal(
                "outlier positive affine indices",
                &a.suspicious.iter().map(|v| v.index).collect::<Vec<_>>(),
                &b.suspicious.iter().map(|v| v.index).collect::<Vec<_>>(),
            );
        }
    }
    Ok(m.finish())
}
#[test]
fn generated_jump_threshold_floor_sign_and_tie_interactions_match_adjacent_reference() {
    let mut c = certification("jump semantic interactions").executor(execute);
    c.domain(domain!(Scenario{thresholds.all_enum_values(),absolute.all_enum_values(),relative.all_enum_values(),floor.all_enum_values(),trace.all_enum_values()}));
    c.seed(521).cases_budget(256);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    c.require_pairwise_coverage();
    c.assert_certify();
}

mod deterministic {
    use super::*;
    use crate::certifications::test_support::{close, median};

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
            .outliers(
                OutlierConfig::new(3.5, ZeroMadPolicy::absolute_tolerance(1.).unwrap()).unwrap(),
            )
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
            serde_json::from_str::<OutlierReport>(&serde_json::to_string(&report).unwrap())
                .unwrap(),
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
            assert!(OutlierConfig::new(threshold, ZeroMadPolicy::ExactMedian).is_err());
        }
        assert!(ZeroMadPolicy::absolute_tolerance(-1.).is_err());
    }

    #[test]
    fn jumps_identify_exact_transition_both_endpoints_and_stable_traces() {
        let config = JumpConfig::new(Some(2.), None, 1e-6).unwrap();
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
        let config = JumpConfig::new(None, Some(2.), 0.1).unwrap();
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
            .jumps(JumpConfig::new(Some(1.), None, config.denominator_floor()).unwrap())
            .unwrap();
        assert!(exact.suspicious.is_empty());
        assert_eq!(exact.maximum_absolute.unwrap().from_index, 0);
        for floor in [0., -1., f64::INFINITY, f64::NAN] {
            assert!(JumpConfig::new(None, Some(2.), floor).is_err());
        }
        assert!(NumericSeries::new([0., f64::INFINITY].into_iter())
            .jumps(config)
            .is_err());
        assert!(JumpConfig::new(Some(-1.), None, 0.1).is_err());
    }
}
