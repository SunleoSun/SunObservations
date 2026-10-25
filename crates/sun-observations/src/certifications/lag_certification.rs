use crate::certifications::test_support::{close, reference_linear, reference_rank, shuffle};
use crate::certifications::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn parallel_pearson_preserves_bits_order_alignment_and_projection_counts() {
    let left: Vec<_> = (0..40000)
        .map(|i| (i as i64 * 3 + i64::MIN, ((i * 17 % 991) as f64).sin()))
        .collect();
    let right: Vec<_> = left
        .iter()
        .copied()
        .filter(|(key, _)| key.rem_euclid(13) != 0)
        .rev()
        .collect();
    let config = config(-32, 32, LagMeasure::Pearson);
    let single = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let reference = single.install(|| run(&left, &right, config));
    for candidate in &reference.candidates {
        let independent = run(
            &left,
            &right,
            LagConfig {
                range: LagRange::new(candidate.lag, candidate.lag).unwrap(),
                ..config
            },
        );
        assert_eq!(
            serde_json::to_vec(candidate).unwrap(),
            serde_json::to_vec(&independent.candidates[0]).unwrap()
        );
    }
    for threads in [1, 2, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let projections = AtomicUsize::new(0);
        for _ in 0..3 {
            projections.store(0, Ordering::Relaxed);
            let actual = pool
                .install(|| {
                    lag_relation_by(
                        &left,
                        &right,
                        |v| {
                            projections.fetch_add(1, Ordering::Relaxed);
                            *v
                        },
                        |v| {
                            projections.fetch_add(1, Ordering::Relaxed);
                            *v
                        },
                        config,
                    )
                })
                .unwrap();
            assert_eq!(
                serde_json::to_vec(&actual).unwrap(),
                serde_json::to_vec(&reference).unwrap()
            );
            assert_eq!(
                projections.load(Ordering::Relaxed),
                left.len() + right.len()
            );
        }
    }
}
#[test]
fn parallel_pearson_returns_the_first_numeric_error_in_lag_order() {
    let values: Vec<_> = (0..40000)
        .map(|i| (i as i64, if i % 2 == 0 { f64::MAX } else { -f64::MAX }))
        .collect();
    let config = config(-32, 32, LagMeasure::Pearson);
    for threads in [1, 2, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let actual = pool
            .install(|| lag_relation_by(&values, &values, |v| *v, |v| *v, config))
            .unwrap_err();
        let independent = lag_relation_by(
            &values,
            &values,
            |v| *v,
            |v| *v,
            LagConfig {
                range: LagRange::new(-32, -32).unwrap(),
                ..config
            },
        )
        .unwrap_err();
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&independent).unwrap()
        );
        assert!(matches!(actual, LagError::Numeric { lag: -32, .. }));
    }
}

fn config(start: i64, end: i64, measure: LagMeasure) -> LagConfig {
    LagConfig {
        range: LagRange::new(start, end).unwrap(),
        measure,
    }
}
fn signal(k: i64) -> f64 {
    let x = (k as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((x ^ (x >> 29)) % 1009) as f64
}
fn run(a: &[(i64, f64)], b: &[(i64, f64)], c: LagConfig) -> LagReport {
    lag_relation_by(a, b, |v| *v, |v| *v, c).unwrap()
}
#[test]
fn explicit_positive_zero_negative_shifts_and_negative_relations_ignore_insertion_order() {
    for d in [-3, 0, 4] {
        for scale in [2., -2.] {
            for measure in [LagMeasure::Pearson, LagMeasure::Spearman] {
                let mut a: Vec<_> = (100..228).map(|k| (k, signal(k))).collect();
                let mut b: Vec<_> = (100..228)
                    .map(|k| (k + d, scale * signal(k) + 3.))
                    .collect();
                shuffle(&mut a, 42);
                shuffle(&mut b, 137);
                let report = run(&a, &b, config(-5, 5, measure));
                let best = report.best.as_ref().unwrap();
                assert_eq!(best.lag, d);
                close(best.relation, scale.signum());
                assert_eq!(
                    report.candidates[(d + 5) as usize].alignment.pair_count,
                    128
                );
                assert_eq!(report.zero_lag().unwrap().lag, 0);
                assert!(fact("lag", report)
                    .expect(LagExpectation { best_lag: d })
                    .is_pass());
            }
        }
    }
}
#[test]
fn truncated_boundaries_are_reported_but_do_not_fail_known_alignment() {
    let a: Vec<_> = (0..100).map(|k| (k, signal(k))).collect();
    let b: Vec<_> = (5..95).map(|k| (k + 2, signal(k))).collect();
    let report = run(&a, &b, config(-3, 3, LagMeasure::Pearson));
    let best = &report.candidates[5];
    assert_eq!(best.lag, 2);
    assert_eq!(
        best.alignment,
        LagAlignment {
            pair_count: 90,
            boundary_left: 10,
            ..Default::default()
        }
    );
    assert!(fact("lag", report)
        .expect(LagExpectation { best_lag: 2 })
        .is_pass());
}
#[test]
fn interior_gaps_are_visible_and_expectations_fail_closed() {
    let a: Vec<_> = (0..100).map(|k| (k, signal(k))).collect();
    let b: Vec<_> = a.iter().copied().filter(|(k, _)| *k != 42).collect();
    let report = run(&a, &b, config(0, 0, LagMeasure::Pearson));
    assert_eq!(report.candidates[0].alignment.left_only, 1);
    assert_eq!(report.candidates[0].alignment.pair_count, 99);
    let failure = fact("lag", report).expect(LagExpectation { best_lag: 0 });
    assert_eq!(failure.violations, [LagViolation::InteriorMissingKeys]);
}
#[test]
fn duplicate_keys_fail_with_original_side_indices_even_outside_candidate_overlap() {
    let a = [(5, 1.), (1, 3.), (5, 2.)];
    let b = [(99, 2.), (99, 3.)];
    let error =
        lag_relation_by(&a, &b, |v| *v, |v| *v, config(-1, 1, LagMeasure::Pearson)).unwrap_err();
    assert!(!error.to_string().is_empty());
    let LagError::DuplicateKeys { left, right } = &error else {
        panic!()
    };
    assert_eq!(
        left[0],
        KeyGroup {
            key: 5,
            indices: vec![0, 2]
        }
    );
    assert_eq!(
        right[0],
        KeyGroup {
            key: 99,
            indices: vec![0, 1]
        }
    );
    assert_eq!(
        serde_json::from_str::<LagError>(&serde_json::to_string(&error).unwrap()).unwrap(),
        error
    );
}
#[test]
fn empty_singleton_disjoint_and_constant_inputs_remain_explicitly_undefined() {
    for (a, b, reason) in [
        (vec![], vec![], UndefinedReason::InsufficientPairs),
        (
            vec![(1, 2.)],
            vec![(1, 2.)],
            UndefinedReason::InsufficientPairs,
        ),
        (
            vec![(1, 2.)],
            vec![(9, 2.)],
            UndefinedReason::InsufficientPairs,
        ),
        (
            vec![(0, 1.), (1, 1.)],
            vec![(0, 1.), (1, 2.)],
            UndefinedReason::ConstantLeft,
        ),
        (
            vec![(0, 1.), (1, 2.)],
            vec![(0, 1.), (1, 1.)],
            UndefinedReason::ConstantRight,
        ),
    ] {
        for measure in [LagMeasure::Pearson, LagMeasure::Spearman] {
            let report = run(&a, &b, config(0, 0, measure));
            assert_eq!(report.candidates[0].relation, Metric::Undefined(reason));
            assert!(report.best.is_none());
            assert_eq!(
                fact("lag", report)
                    .expect(LagExpectation { best_lag: 0 })
                    .violations,
                [LagViolation::NoDefinedCandidate]
            );
        }
    }
}
#[test]
fn exact_ties_prefer_smallest_absolute_then_smallest_signed_lag() {
    let a = [(0, 0.), (2, 1.)];
    let b: Vec<_> = (-2..=4).map(|k| (k, k as f64)).collect();
    let report = run(&a, &b, config(-2, 2, LagMeasure::Pearson));
    assert!(report
        .candidates
        .iter()
        .all(|c| c.relation == report.candidates[0].relation));
    assert_eq!(report.best.unwrap().lag, 0);
    assert_eq!(
        run(&a, &b, config(1, 2, LagMeasure::Pearson))
            .best
            .unwrap()
            .lag,
        1
    );
    let a = [(0, 0.), (2, 1.), (4, 2.)];
    let b = [(1, 0.), (3, 1.), (5, 2.)];
    assert_eq!(
        run(&a, &b, config(-1, 1, LagMeasure::Pearson))
            .best
            .unwrap()
            .lag,
        -1
    );
}
#[test]
fn bounded_ranges_nonfinite_values_and_numeric_overflow_are_hard_errors() {
    for (start, end) in [(2, 1), (i64::MIN, i64::MAX), (0, MAX_LAG_CANDIDATES as i64)] {
        assert!(matches!(
            LagRange::new(start, end),
            Err(LagError::InvalidRange { .. })
        ));
    }
    let a = [(0, 1.), (1, f64::INFINITY)];
    assert!(matches!(
        lag_relation_by(
            &a,
            &a[..1],
            |v| *v,
            |v| *v,
            config(0, 0, LagMeasure::Pearson)
        ),
        Err(LagError::Projection {
            side: Side::Left,
            error: NumericError::NonFinite { index: 1, .. }
        })
    ));
    let a = [(0, f64::MAX), (1, -f64::MAX)];
    assert!(matches!(
        lag_relation_by(&a, &a, |v| *v, |v| *v, config(0, 0, LagMeasure::Pearson)),
        Err(LagError::Numeric { lag: 0, .. })
    ));
}
#[test]
fn extreme_i64_keys_shift_without_wrap_and_project_only_once() {
    let a = [(i64::MAX - 2, 1.), (i64::MAX - 1, 5.), (i64::MAX, 2.)];
    let count = AtomicUsize::new(0);
    let report = lag_relation_by(
        &a,
        &a,
        |v| {
            count.fetch_add(1, Ordering::Relaxed);
            *v
        },
        |v| {
            count.fetch_add(1, Ordering::Relaxed);
            *v
        },
        config(-2, 2, LagMeasure::Pearson),
    )
    .unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 6);
    assert_eq!(report.best.unwrap().lag, 0);
    assert_eq!(report.candidates[4].alignment.boundary_left, 2);
    let a = [(i64::MIN, 1.), (i64::MIN + 1, 5.), (i64::MIN + 2, 2.)];
    assert_eq!(
        run(&a, &a, config(-2, 2, LagMeasure::Pearson)).candidates[0]
            .alignment
            .boundary_left,
        2
    );
}
#[test]
fn failed_lag_expectation_retains_observed_report_and_roundtrip() {
    let a: Vec<_> = (0..30).map(|k| (k, signal(k))).collect();
    let b: Vec<_> = a.iter().map(|(k, v)| (k + 2, *v)).collect();
    let result = fact("lag", run(&a, &b, config(-3, 3, LagMeasure::Pearson)))
        .expect(LagExpectation { best_lag: 0 });
    assert_eq!(result.violations, [LagViolation::UnexpectedBestLag]);
    let report = result.observed.unwrap();
    assert_eq!(report.best.as_ref().unwrap().lag, 2);
    assert!(report.zero_lag().is_some());
    assert_eq!(
        serde_json::from_value::<LagReport>(serde_json::to_value(&report).unwrap()).unwrap(),
        report
    );
}
#[test]
fn sparse_unordered_optimized_search_matches_independent_exhaustive_domain_reference() {
    let mut a: Vec<_> = (0..180)
        .filter(|k| k % 7 != 0)
        .map(|k| (3 * k, signal(k)))
        .collect();
    let mut b: Vec<_> = (0..180)
        .filter(|k| k % 11 != 0)
        .map(|k| (3 * k + 2, -signal(k)))
        .collect();
    shuffle(&mut a, 567);
    shuffle(&mut b, 765);
    for measure in [LagMeasure::Pearson, LagMeasure::Spearman] {
        let report = run(&a, &b, config(-8, 8, measure));
        for candidate in &report.candidates {
            let mut ordered = a.clone();
            ordered.sort_by_key(|p| p.0);
            let mut pairs: Vec<_> = ordered
                .iter()
                .filter_map(|(k, x)| {
                    b.iter()
                        .find(|(j, _)| i128::from(*j) == i128::from(*k) + i128::from(candidate.lag))
                        .map(|(_, y)| (*x, *y))
                })
                .collect();
            assert_eq!(candidate.alignment.pair_count, pairs.len());
            let boundary_a = a
                .iter()
                .filter(|(k, _)| {
                    let shifted = *k + candidate.lag;
                    shifted < b.iter().map(|p| p.0).min().unwrap()
                        || shifted > b.iter().map(|p| p.0).max().unwrap()
                })
                .count();
            let boundary_b = b
                .iter()
                .filter(|(k, _)| {
                    *k < a.iter().map(|p| p.0).min().unwrap() + candidate.lag
                        || *k > a.iter().map(|p| p.0).max().unwrap() + candidate.lag
                })
                .count();
            assert_eq!(candidate.alignment.boundary_left, boundary_a);
            assert_eq!(candidate.alignment.boundary_right, boundary_b);
            assert_eq!(
                candidate.alignment.left_only,
                a.len() - pairs.len() - boundary_a
            );
            assert_eq!(
                candidate.alignment.right_only,
                b.len() - pairs.len() - boundary_b
            );
            if pairs.len() < 2 {
                assert_eq!(
                    candidate.relation,
                    Metric::Undefined(UndefinedReason::InsufficientPairs)
                );
                continue;
            }
            if measure == LagMeasure::Spearman {
                let x = reference_rank(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
                let y = reference_rank(&pairs.iter().map(|p| p.1).collect::<Vec<_>>());
                pairs = x.into_iter().zip(y).collect();
            }
            close(
                candidate.relation.value().unwrap(),
                reference_linear(&pairs).0,
            );
        }
    }
}

mod generated {
    use super::*;
    use crate::certifications::test_support::{
        reference_linear, reference_rank, shuffle, Mismatches,
    };
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Measure {
        Pearson,
        Spearman,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Shift {
        NegativeLarge,
        Negative,
        Zero,
        Positive,
        PositiveLarge,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Sign {
        Positive,
        Negative,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Shape {
        Signal,
        ConstantLeft,
        ConstantRight,
        Singleton,
        Empty,
        LinearTies,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Missing {
        None,
        Boundary,
        Interior,
        Both,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Magnitude {
        Ordinary,
        Offset,
        Tiny,
        Large,
    }
    #[derive(DomainInput)]
    struct Scenario {
        measure: Measure,
        shift: Shift,
        sign: Sign,
        shape: Shape,
        missing: Missing,
        magnitude: Magnitude,
        order: crate::certifications::test_support::RowOrder,
    }

    fn oracle_metric(mut pairs: Vec<(f64, f64)>, measure: LagMeasure) -> Metric {
        let reason = if pairs.len() < 2 {
            Some(UndefinedReason::InsufficientPairs)
        } else if pairs.iter().all(|p| p.0 == pairs[0].0) {
            Some(UndefinedReason::ConstantLeft)
        } else if pairs.iter().all(|p| p.1 == pairs[0].1) {
            Some(UndefinedReason::ConstantRight)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Metric::Undefined(reason);
        }
        if measure == LagMeasure::Spearman {
            let a = reference_rank(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
            let b = reference_rank(&pairs.iter().map(|p| p.1).collect::<Vec<_>>());
            pairs = a.into_iter().zip(b).collect();
        }
        Metric::Defined(reference_linear(&pairs).0)
    }

    fn execute(s: &Scenario) -> Result<Vec<String>, LagError> {
        use crate::certifications::test_support::RowOrder;
        let shift = match s.shift {
            Shift::NegativeLarge => -4,
            Shift::Negative => -1,
            Shift::Zero => 0,
            Shift::Positive => 1,
            Shift::PositiveLarge => 4,
        };
        let sign = if s.sign == Sign::Negative { -1. } else { 1. };
        let (scale, offset) = match s.magnitude {
            Magnitude::Ordinary => (1., 0.),
            Magnitude::Offset => (1., 2f64.powi(36)),
            Magnitude::Tiny => (2f64.powi(-80), 0.),
            Magnitude::Large => (2f64.powi(80), 0.),
        };
        let n = match s.shape {
            Shape::Empty => 0,
            Shape::Singleton => 1,
            _ => 27,
        };
        let signal = |k: i64| {
            if s.shape == Shape::LinearTies {
                k as f64
            } else {
                ((k * k * 13 + k * 17 + 11) % 101) as f64
            }
        };
        let mut a: Vec<_> = (0..n)
            .filter(|k| !matches!(s.missing, Missing::Interior | Missing::Both) || k % 7 != 3)
            .map(|k| {
                (
                    k,
                    offset
                        + scale
                            * if s.shape == Shape::ConstantLeft {
                                3.
                            } else {
                                signal(k)
                            },
                )
            })
            .collect();
        let mut b: Vec<_> = (0..n)
            .filter(|k| {
                !matches!(s.missing, Missing::Boundary | Missing::Both) || (*k >= 2 && *k < n - 3)
            })
            .filter(|k| !matches!(s.missing, Missing::Interior | Missing::Both) || k % 5 != 2)
            .map(|k| {
                (
                    k + shift,
                    offset
                        + sign
                            * scale
                            * if s.shape == Shape::ConstantRight {
                                5.
                            } else {
                                signal(k)
                            },
                )
            })
            .collect();
        match s.order {
            RowOrder::Sorted => (),
            RowOrder::Reversed => {
                a.reverse();
                b.reverse();
            }
            RowOrder::Shuffled => {
                shuffle(&mut a, 71);
                shuffle(&mut b, 19);
            }
        }
        let measure = if s.measure == Measure::Pearson {
            LagMeasure::Pearson
        } else {
            LagMeasure::Spearman
        };
        let report = lag_relation_by(
            &a,
            &b,
            |r| *r,
            |r| *r,
            LagConfig {
                range: LagRange::new(-5, 5).unwrap(),
                measure,
            },
        )?;
        let mut m = Mismatches::default();
        m.equal(
            "all candidate lags",
            &report.candidates.iter().map(|c| c.lag).collect::<Vec<_>>(),
            &(-5..=5).collect::<Vec<i64>>(),
        );
        m.equal("left record count", &report.left_count, &a.len());
        m.equal("right record count", &report.right_count, &b.len());
        let mut reference = Vec::new();
        for candidate in &report.candidates {
            let d = i128::from(candidate.lag);
            let pairs: Vec<_> = a
                .iter()
                .filter_map(|(k, x)| {
                    b.iter()
                        .find(|(j, _)| i128::from(*j) == i128::from(*k) + d)
                        .map(|(_, y)| (*x, *y))
                })
                .collect();
            let outside = |key: i128, values: &[(i64, f64)], shift: i128| {
                values
                    .iter()
                    .map(|r| i128::from(r.0) + shift)
                    .min()
                    .zip(values.iter().map(|r| i128::from(r.0) + shift).max())
                    .is_none_or(|(lo, hi)| key < lo || key > hi)
            };
            let boundary_left = a
                .iter()
                .filter(|r| outside(i128::from(r.0) + d, &b, 0))
                .count();
            let boundary_right = b.iter().filter(|r| outside(i128::from(r.0), &a, d)).count();
            let alignment = LagAlignment {
                pair_count: pairs.len(),
                boundary_left,
                boundary_right,
                left_only: a.len() - pairs.len() - boundary_left,
                right_only: b.len() - pairs.len() - boundary_right,
            };
            m.equal("candidate alignment", &candidate.alignment, &alignment);
            let expected = oracle_metric(pairs, measure);
            match (candidate.relation, expected) {
                (Metric::Defined(x), Metric::Defined(y)) => {
                    m.within("candidate relation", x, y, 1e-9)
                }
                (x, y) => m.equal("undefined reason", &x, &y),
            }
            if let Metric::Defined(v) = expected {
                reference.push((candidate.lag, v.abs()));
            }
        }
        reference.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| (a.0.unsigned_abs(), a.0).cmp(&(b.0.unsigned_abs(), b.0)))
        });
        if let Some(best) = &report.best {
            let selected = report
                .candidates
                .iter()
                .find(|c| c.lag == best.lag)
                .and_then(|c| c.relation.value());
            m.equal("selected best relation", &Some(best.relation), &selected);
            m.equal(
                "absolute best strength",
                &best.strength,
                &best.relation.abs(),
            );
            m.within("best strength", best.strength, reference[0].1, 1e-9);
            let unique = reference
                .iter()
                .skip(1)
                .all(|r| reference[0].1 - r.1 > 1e-9);
            if unique {
                m.equal("independent best lag", &best.lag, &reference[0].0);
            }
            let mut ranked: Vec<_> = report
                .candidates
                .iter()
                .filter_map(|c| c.relation.value().map(|v| (c.lag, v.abs())))
                .collect();
            ranked.sort_by(|a, b| {
                b.1.total_cmp(&a.1)
                    .then_with(|| (a.0.unsigned_abs(), a.0).cmp(&(b.0.unsigned_abs(), b.0)))
            });
            m.equal("exact tie ordering", &best.lag, &ranked[0].0);
        } else {
            m.require("all candidates undefined", reference.is_empty());
        }
        Ok(m.finish())
    }

    #[test]
    fn generated_lag_interactions_match_exhaustive_alignment_and_metric_oracle() {
        let mut c = certification("lag semantic interactions").executor(execute);
        c.domain(domain!(Scenario{measure.all_enum_values(),shift.all_enum_values(),sign.all_enum_values(),shape.all_enum_values(),missing.all_enum_values(),magnitude.all_enum_values(),order.all_enum_values()}));
        c.seed(719).cases_budget(384);
        let case = c.case();
        c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
        c.require_pairwise_coverage();
        c.assert_certify();
    }

    #[derive(DomainInput)]
    struct AlignmentScenario {
        shape: Shape,
        missing: Missing,
        order: crate::certifications::test_support::RowOrder,
    }

    #[test]
    fn higher_order_alignment_explores_every_shape_gap_order_triple_within_small_budget() {
        let mut seen = Vec::new();
        let mut c =
            certification("higher order lag alignment").executor(|s: &AlignmentScenario| {
                let triple = (s.shape.clone(), s.missing.clone(), s.order.clone());
                if !seen.contains(&triple) {
                    seen.push(triple);
                }
                execute(&Scenario {
                    measure: Measure::Spearman,
                    shift: Shift::Negative,
                    sign: Sign::Negative,
                    shape: s.shape.clone(),
                    missing: s.missing.clone(),
                    magnitude: Magnitude::Ordinary,
                    order: s.order.clone(),
                })
            });
        c.domain(domain!(AlignmentScenario{shape.all_enum_values(),missing.all_enum_values(),order.all_enum_values()}));
        c.seed(197).cases_budget(128);
        let case = c.case();
        c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
        c.require_pairwise_coverage();
        let report = c.report();
        report.assert_certify();
        assert_eq!(seen.len(), 72);
    }
}
