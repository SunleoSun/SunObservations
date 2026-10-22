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
