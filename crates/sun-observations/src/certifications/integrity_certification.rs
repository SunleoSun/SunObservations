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

mod deterministic {
    use super::*;

    #[test]
    fn count_requirements_report_expected_and_actual() {
        for rule in [
            CountRule::Exact(2),
            CountRule::Minimum(2),
            CountRule::NonEmpty,
        ] {
            assert!(check_count(2, rule).is_valid());
            let report = check_count(0, rule);
            assert_eq!(
                report.issues,
                vec![IntegrityIssue::Count {
                    expected: rule,
                    actual: 0
                }]
            );
            assert_eq!(
                serde_json::from_value::<IntegrityReport>(serde_json::to_value(&report).unwrap())
                    .unwrap(),
                report
            );
        }
        assert!(!check_count(3, CountRule::Exact(2)).is_valid());
    }

    #[test]
    fn uniqueness_contiguity_and_same_sets_retain_precise_key_evidence() {
        let keys = [4, 2, 2, 0, 7];
        assert_eq!(
            check_unique(&keys, |v| *v).issues,
            vec![IntegrityIssue::Duplicate(KeyGroup {
                key: 2,
                indices: vec![1, 2]
            })]
        );
        let report = check_contiguous(&keys, |v| *v, 1, 5);
        assert!(report
            .issues
            .contains(&IntegrityIssue::MissingRange { start: 1, end: 1 }));
        assert!(report
            .issues
            .contains(&IntegrityIssue::MissingRange { start: 3, end: 3 }));
        assert!(report
            .issues
            .contains(&IntegrityIssue::MissingRange { start: 5, end: 5 }));
        assert!(report
            .issues
            .contains(&IntegrityIssue::UnexpectedKey { key: 0 }));
        assert!(report
            .issues
            .contains(&IntegrityIssue::UnexpectedKey { key: 7 }));
        assert!(check_contiguous(&[u64::MAX], |v| *v, u64::MAX, u64::MAX).is_valid());
        assert_eq!(
            check_contiguous::<u64>(&[], |v| *v, 0, u64::MAX).issues,
            vec![IntegrityIssue::MissingRange {
                start: 0,
                end: u64::MAX
            }]
        );
        assert!(matches!(
            check_contiguous::<u64>(&[], |v| *v, 2, 1).issues[0],
            IntegrityIssue::InvalidKeyRange { .. }
        ));
        assert!(check_contiguous(&[3, 1, 2], |v| *v, 1, 3).is_valid());
        let mismatch = check_same_keys(&[1, 2], &[2, 3], |v| *v, |v| *v);
        assert_eq!(mismatch.right_only[0].key, 3);
        assert_eq!(mismatch.left_only[0].key, 1);
        assert!(check_same_keys(&[1, 2], &[2, 1], |v| *v, |v| *v).is_complete());
    }

    #[test]
    fn finite_ranges_positive_and_nonnegative_checks_are_structured_and_inclusive() {
        let values = [-1., 0., 1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        let series = NumericSeries::new(values.into_iter());
        assert_eq!(
            series.integrity(NumericRule::Finite).unwrap().issues.len(),
            3
        );
        assert_eq!(
            series
                .integrity(NumericRule::Positive)
                .unwrap()
                .issues
                .len(),
            5
        );
        assert_eq!(
            series
                .integrity(NumericRule::NonNegative)
                .unwrap()
                .issues
                .len(),
            4
        );
        let report = series
            .integrity(NumericRule::between(-1., 1.).unwrap())
            .unwrap();
        assert_eq!(report.issues.len(), 3);
        assert_eq!(
            report.issues[0],
            IntegrityIssue::Numeric {
                index: 3,
                value: NumericValue::Nan,
                expected: NumericRule::between(-1., 1.).unwrap()
            }
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("null"));
        assert_eq!(
            serde_json::from_str::<IntegrityReport>(&json).unwrap(),
            report
        );
        assert!(NumericRule::between(2., 1.).is_err());
        assert!(NumericRule::between(f64::NAN, 1.).is_err());
    }
}
