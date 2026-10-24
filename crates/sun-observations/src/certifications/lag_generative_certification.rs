use super::*;
use crate::certifications::test_support::{reference_linear, reference_rank, shuffle, Mismatches};
use crate::certifications::*;

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
            (Metric::Defined(x), Metric::Defined(y)) => m.within("candidate relation", x, y, 1e-9),
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
    let mut c = certification("higher order lag alignment").executor(|s: &AlignmentScenario| {
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
