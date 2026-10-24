use crate::certifications::test_support::{assert_pairwise_certified, median, shuffle, Mismatches};
use crate::certifications::*;
use std::{cell::Cell, collections::BTreeMap};

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Groups {
    Empty,
    One,
    Several,
    Many,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Sizes {
    Singletons,
    Equal,
    Unequal,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Order {
    Sorted,
    Reversed,
    Shuffled,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Values {
    Distinct,
    Tied,
    Constant,
    OneBroken,
}
#[derive(DomainInput)]
struct Scenario {
    groups: Groups,
    sizes: Sizes,
    order: Order,
    values: Values,
}

fn execute(s: &Scenario) -> Result<Vec<String>, ()> {
    let n = match s.groups {
        Groups::Empty => 0,
        Groups::One => 1,
        Groups::Several => 3,
        Groups::Many => 11,
    };
    let mut rows = Vec::new();
    for key in 0..n {
        let count = match s.sizes {
            Sizes::Singletons => 1,
            Sizes::Equal => 6,
            Sizes::Unequal => key + 1,
        };
        for i in 0..count {
            let value = match s.values {
                Values::Distinct => (3 * i + key) as f64,
                Values::Tied => (i / 2) as f64,
                Values::Constant => 7.,
                Values::OneBroken if key == 0 => f64::NAN,
                Values::OneBroken => i as f64,
            };
            rows.push((key, value));
        }
    }
    match s.order {
        Order::Sorted => (),
        Order::Reversed => rows.reverse(),
        Order::Shuffled => shuffle(&mut rows, 3281),
    }
    let mut expected = BTreeMap::<usize, Vec<usize>>::new();
    for (index, (key, _)) in rows.iter().enumerate() {
        expected.entry(*key).or_default().push(index);
    }
    let source = collection("groups", &rows);
    let grouped = source.group_by(|r| r.0);
    let calls = Cell::new(0);
    let numeric = grouped.numeric(|r| {
        calls.set(calls.get() + 1);
        r.1
    });
    let ordinary = numeric.summaries();
    let robust = numeric.summaries_with_robust();
    let repeated = numeric.summaries();
    let mut m = Mismatches::default();
    m.equal(
        "ordinary group count",
        &ordinary.groups.len(),
        &expected.len(),
    );
    m.equal("robust group count", &robust.groups.len(), &expected.len());
    m.equal("projection cached", &calls.get(), &rows.len());
    m.equal("repeat report", &repeated, &ordinary);
    m.equal(
        "group keys",
        &grouped.iter().map(|g| *g.key).collect::<Vec<_>>(),
        &expected.keys().copied().collect::<Vec<_>>(),
    );
    for ((group, (key, indices)), (a, b)) in grouped
        .iter()
        .zip(&expected)
        .zip(ordinary.groups.iter().zip(&robust.groups))
    {
        m.require("nonempty", !group.is_empty());
        m.equal(
            "source indices",
            &group.iter().map(|r| r.index).collect::<Vec<_>>(),
            indices,
        );
        m.equal(
            "values in source order",
            &group.values().map(|r| r.1.to_bits()).collect::<Vec<_>>(),
            &indices
                .iter()
                .map(|i| rows[*i].1.to_bits())
                .collect::<Vec<_>>(),
        );
        m.equal("identity", &a.key, key);
        m.equal("robust identity", &b.key, key);
        m.equal("member count", &a.member_count, &indices.len());
        m.equal("robust member count", &b.member_count, &indices.len());
        m.equal("robust ordinary parity", &a.summary, &b.summary);
        m.require("ordinary opts out of robust", a.robust.is_none());
        let values: Vec<f64> = indices.iter().map(|i| rows[*i].1).collect();
        if values.iter().any(|v| !v.is_finite()) {
            m.require("isolated ordinary error", a.summary.is_err());
            m.require("isolated robust error", b.robust.as_ref().unwrap().is_err());
        } else {
            let summary = a.summary.as_ref().unwrap();
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance =
                values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
            m.within("sum", summary.sum, values.iter().sum(), 1e-10);
            m.within(
                "min",
                summary.min,
                values.iter().copied().reduce(f64::min).unwrap(),
                1e-10,
            );
            m.within(
                "max",
                summary.max,
                values.iter().copied().reduce(f64::max).unwrap(),
                1e-10,
            );
            m.within("stddev", summary.standard_deviation, variance.sqrt(), 1e-10);
            m.equal("finite count", &summary.finite_count, &values.len());
            if values.len() > 1 {
                m.within(
                    "sample variance",
                    summary.sample_variance.unwrap(),
                    variance * values.len() as f64 / (values.len() - 1) as f64,
                    1e-10,
                );
            } else {
                m.require(
                    "singleton sample variance",
                    summary.sample_variance.is_none(),
                );
            }
            m.within("mean", summary.mean, mean, 1e-10);
            m.within("variance", summary.variance, variance, 1e-10);
            m.equal("summary count", &summary.count, &values.len());
            let r = b.robust.as_ref().unwrap().as_ref().unwrap();
            let center = median(&values);
            m.within("median", r.median, center, 1e-10);
            m.within(
                "mad",
                r.mad,
                median(
                    &values
                        .iter()
                        .map(|v| (v - center).abs())
                        .collect::<Vec<_>>(),
                ),
                1e-10,
            );
        }
    }
    Ok(m.finish())
}

#[test]
fn generated_groups_preserve_membership_summary_identity_and_error_isolation() {
    let mut c = certification("grouping semantic interactions").executor(execute);
    c.domain(domain!(Scenario { groups.all_enum_values(), sizes.all_enum_values(), order.all_enum_values(), values.all_enum_values() }));
    c.seed(3281).cases_budget(160);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    assert_pairwise_certified(&c.report());
}
