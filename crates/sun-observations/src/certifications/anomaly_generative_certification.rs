use super::*;
use crate::certifications::test_support::{assert_pairwise_certified, Mismatches};
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
    assert_pairwise_certified(&c.report());
}
