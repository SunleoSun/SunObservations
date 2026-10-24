use super::*;
use crate::certifications::test_support::{KeyScenario, Mismatches};
use crate::certifications::*;
use std::collections::BTreeMap;

fn execute(s: &KeyScenario) -> Result<Vec<String>, ()> {
    let (keys, start, end) = s.materialize();
    let mut groups = BTreeMap::<u64, Vec<usize>>::new();
    for (i, k) in keys.iter().enumerate() {
        groups.entry(*k).or_default().push(i);
    }
    let duplicates: Vec<_> = groups
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, v)| {
            IntegrityIssue::Duplicate(KeyGroup {
                key: *k,
                indices: v.clone(),
            })
        })
        .collect();
    let unique = check_unique(&keys, |k| *k);
    let contiguous = check_contiguous(&keys, |k| *k, start, end);
    let mut missing = Vec::new();
    for k in start..=end {
        if !keys.contains(&k) {
            match missing.last_mut() {
                Some(IntegrityIssue::MissingRange { end: previous, .. })
                    if previous.checked_add(1) == Some(k) =>
                {
                    *previous = k
                }
                _ => missing.push(IntegrityIssue::MissingRange { start: k, end: k }),
            }
        }
    }
    let unexpected: Vec<_> = groups
        .iter()
        .filter(|(k, _)| **k < start || **k > end)
        .flat_map(|(k, v)| {
            v.iter()
                .map(move |_| IntegrityIssue::UnexpectedKey { key: *k })
        })
        .collect();
    let select = |predicate: fn(&IntegrityIssue<u64>) -> bool| {
        contiguous
            .issues
            .iter()
            .filter(|i| predicate(i))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut m = Mismatches::default();
    m.equal("unique exact payload", &unique.issues, &duplicates);
    m.equal(
        "contiguous duplicates",
        &select(|i| matches!(i, IntegrityIssue::Duplicate(_))),
        &duplicates,
    );
    m.equal(
        "missing ranges",
        &select(|i| matches!(i, IntegrityIssue::MissingRange { .. })),
        &missing,
    );
    m.equal(
        "unexpected keys",
        &select(|i| matches!(i, IntegrityIssue::UnexpectedKey { .. })),
        &unexpected,
    );
    m.equal(
        "total exact issues",
        &contiguous.issues.len(),
        &(duplicates.len() + missing.len() + unexpected.len()),
    );
    m.equal("record count", &contiguous.record_count, &keys.len());
    let right: Vec<_> = (start..=end).collect();
    let same = check_same_keys(&keys, &right, |k| *k, |k| *k);
    let joined = join_by(&keys, &right, |k| *k, |k| *k);
    m.equal("same keys canonical parity", &same, joined.comparison());
    let absent_left: Vec<_> = groups
        .iter()
        .filter(|(k, _)| !right.contains(k))
        .map(|(k, v)| KeyGroup {
            key: *k,
            indices: v.clone(),
        })
        .collect();
    let absent_right: Vec<_> = right
        .iter()
        .enumerate()
        .filter(|(_, k)| !groups.contains_key(k))
        .map(|(i, k)| KeyGroup {
            key: *k,
            indices: vec![i],
        })
        .collect();
    m.equal("left-only exact evidence", &same.left_only, &absent_left);
    m.equal("right-only exact evidence", &same.right_only, &absent_right);
    Ok(m.finish())
}
#[test]
fn generated_key_integrity_retains_exact_indices_ranges_and_extreme_boundaries() {
    let mut c = certification("integrity semantic structure").executor(execute);
    c.domain(KeyScenario::domain());
    c.seed(463).cases_budget(128);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    c.require_pairwise_coverage();
    c.assert_certify();
}

#[derive(Clone, Debug, PartialEq, DomainInput)]
enum NumericShape {
    Empty,
    FiniteExtremes,
    NonFinite,
    SignedZeros,
    Bounds,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum Rule {
    Finite,
    Positive,
    NonNegative,
    Range,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
struct NumericScenario {
    shape: NumericShape,
    rule: Rule,
    order: crate::certifications::test_support::RowOrder,
}
fn numeric(s: &NumericScenario) -> Result<Vec<String>, ()> {
    use crate::certifications::test_support::{shuffle, RowOrder};
    let mut values = match s.shape {
        NumericShape::Empty => vec![],
        NumericShape::FiniteExtremes => vec![
            -f64::MAX,
            -f64::MIN_POSITIVE,
            0.,
            f64::MIN_POSITIVE,
            f64::MAX,
        ],
        NumericShape::NonFinite => vec![1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.],
        NumericShape::SignedZeros => vec![-0., 0., -1., 1.],
        NumericShape::Bounds => vec![-2., -1., 0., 1., 2.],
    };
    match s.order {
        RowOrder::Sorted => (),
        RowOrder::Reversed => values.reverse(),
        RowOrder::Shuffled => shuffle(&mut values, 753),
    }
    let rule = match s.rule {
        Rule::Finite => NumericRule::Finite,
        Rule::Positive => NumericRule::Positive,
        Rule::NonNegative => NumericRule::NonNegative,
        Rule::Range => NumericRule::between(-1., 1.).unwrap(),
    };
    let expected: Vec<IntegrityIssue<()>> = values
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            !(v.is_finite()
                && match s.rule {
                    Rule::Finite => true,
                    Rule::Positive => **v > 0.,
                    Rule::NonNegative => **v >= 0.,
                    Rule::Range => (-1. ..=1.).contains(*v),
                })
        })
        .map(|(i, v)| IntegrityIssue::Numeric {
            index: i,
            value: (*v).into(),
            expected: rule,
        })
        .collect();
    let actual = NumericSeries::new(values.iter().copied())
        .integrity(rule)
        .unwrap();
    let mut m = Mismatches::default();
    m.equal("numeric issues", &actual.issues, &expected);
    m.equal("numeric count", &actual.record_count, &values.len());
    Ok(m.finish())
}
#[test]
fn generated_numeric_integrity_checks_exact_nonfinite_sign_and_boundary_evidence() {
    let mut c = certification("numeric integrity semantics").executor(numeric);
    c.domain(domain!(NumericScenario { shape.all_enum_values(), rule.all_enum_values(), order.all_enum_values() }));
    c.seed(753).cases_budget(128);
    let case = c.case();
    c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
    c.require_pairwise_coverage();
    c.assert_certify();
}
