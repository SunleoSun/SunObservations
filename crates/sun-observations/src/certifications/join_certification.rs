use super::*;

#[test]
fn cardinality_mismatch_cannot_report_complete_correspondence() {
    let left = [1u64, 2];
    let right = [1u64, 2];
    let mut comparison = join_by(&left, &right, |v| *v, |v| *v).comparison().clone();
    assert!(comparison.is_complete());
    comparison.matched.pop();
    assert!(!comparison.is_complete());
    comparison.matched.clear();
    assert!(!comparison.is_complete());
}
use crate::certifications::test_support::shuffle;
use std::collections::BTreeMap;

pub(super) fn reference(left: &[u64], right: &[u64]) -> Comparison<u64> {
    let groups = |values: &[u64]| {
        let mut groups = BTreeMap::<u64, Vec<usize>>::new();
        for (i, &k) in values.iter().enumerate() {
            groups.entry(k).or_default().push(i);
        }
        groups
    };
    let a = groups(left);
    let b = groups(right);
    let mut report = Comparison {
        left_source: None,
        right_source: None,
        left_count: left.len(),
        right_count: right.len(),
        matched: Vec::new(),
        right_only: Vec::new(),
        left_only: Vec::new(),
        duplicate_left: Vec::new(),
        duplicate_right: Vec::new(),
    };
    for (&key, indices) in &a {
        if indices.len() > 1 {
            report.duplicate_left.push(KeyGroup {
                key,
                indices: indices.clone(),
            });
        }
        match b.get(&key) {
            Some(other) if indices.len() == 1 && other.len() == 1 => {
                report.matched.push(MatchedKey {
                    key,
                    left_index: indices[0],
                    right_index: other[0],
                })
            }
            None => report.left_only.push(KeyGroup {
                key,
                indices: indices.clone(),
            }),
            _ => (),
        }
    }
    for (&key, indices) in &b {
        if indices.len() > 1 {
            report.duplicate_right.push(KeyGroup {
                key,
                indices: indices.clone(),
            });
        }
        if !a.contains_key(&key) {
            report.right_only.push(KeyGroup {
                key,
                indices: indices.clone(),
            });
        }
    }
    report
}

#[test]
fn missing_and_duplicate_keys_never_form_false_pairs() {
    let left = [1, 2, 2, 4];
    let right = [2, 3, 4, 4];
    let joined = join_by(&left, &right, |v| *v, |v| *v);
    let a = joined.comparison();
    assert!(a.matched.is_empty());
    assert_eq!(a.right_only[0].key, 3);
    assert_eq!(a.left_only[0].key, 1);
    assert_eq!(
        a.duplicate_left[0],
        KeyGroup {
            key: 2,
            indices: vec![1, 2]
        }
    );
    assert_eq!(
        a.duplicate_right[0],
        KeyGroup {
            key: 4,
            indices: vec![2, 3]
        }
    );
    assert!(joined.require_complete().is_err());
    let json = serde_json::to_string(a).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["left_count"],
        left.len()
    );
}

#[test]
fn sorted_order_contract_checks_both_sides_and_empty_inputs() {
    let sorted = [1, 2, 3];
    let unordered = [2, 1, 3];
    assert_eq!(
        join_presorted_by(&unordered, &sorted, |v| *v, |v| *v).err(),
        Some(JoinError {
            side: Side::Left,
            index: 1
        })
    );
    assert_eq!(
        join_presorted_by(&sorted, &unordered, |v| *v, |v| *v).err(),
        Some(JoinError {
            side: Side::Right,
            index: 1
        })
    );
    assert!(join_presorted_by::<u64, u64, u64>(&[], &[], |v| *v, |v| *v)
        .unwrap()
        .comparison()
        .is_complete());
    assert_eq!(
        join_by(&sorted, &[], |v| *v, |v: &u64| *v)
            .comparison()
            .left_only
            .len(),
        3
    );
    assert_eq!(
        join_by(&[], &sorted, |v: &u64| *v, |v| *v)
            .comparison()
            .right_only
            .len(),
        3
    );
}

#[test]
fn generic_and_sorted_merges_match_independent_map_reference_with_omissions_and_duplicates() {
    for seed in 0..80u64 {
        let mut left: Vec<_> = (0..100).filter(|k| (k + seed) % 7 != 0).collect();
        let mut right: Vec<_> = (0..105).filter(|k| (k + seed) % 11 != 0).collect();
        left.extend([seed, seed]);
        right.extend([seed + 2, seed + 2]);
        shuffle(&mut left, seed + 1);
        shuffle(&mut right, seed + 2);
        let joined = join_by(&left, &right, |v| *v, |v| *v);
        assert_eq!(*joined.comparison(), reference(&left, &right));
        left.sort_unstable();
        right.sort_unstable();
        let sorted = join_presorted_by(&left, &right, |v| *v, |v| *v).unwrap();
        assert_eq!(*sorted.comparison(), reference(&left, &right));
        assert_eq!(
            *sorted.comparison(),
            *join_by(&left, &right, |v| *v, |v| *v).comparison()
        );
    }
}

#[test]
fn generic_keys_and_key_order_not_insertion_order_define_comparison() {
    let left = [("b", 2.), ("a", 1.)];
    let right = [("a", 10.), ("b", 20.)];
    let joined = join_by(&left, &right, |v| v.0, |v| v.0);
    assert_eq!(
        joined
            .matched_pairs()
            .map(|(k, l, r)| (*k, l.1, r.1))
            .collect::<Vec<_>>(),
        vec![("a", 1., 10.), ("b", 2., 20.)]
    );
}

#[test]
fn large_duplicate_run_and_disjoint_tail_preserve_linear_merge_evidence() {
    let left: Vec<_> = (0..100_000).collect();
    let right = vec![100_001; 100_000];
    let joined = join_presorted_by(&left, &right, |v| *v, |v| *v).unwrap();
    assert_eq!(joined.comparison().left_only.len(), 100_000);
    assert_eq!(joined.comparison().right_only[0].indices.len(), 100_000);
    assert_eq!(
        joined.comparison().duplicate_right[0].indices.len(),
        100_000
    );
}

mod generated {
    use super::*;
    use crate::certifications::test_support::{shuffle, Mismatches};
    use crate::certifications::*;

    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Overlap {
        None,
        Partial,
        Full,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Cardinality {
        Empty,
        Singleton,
        Unequal,
        Equal,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Duplicates {
        None,
        Left,
        Right,
        Both,
    }
    #[derive(Clone, Debug, PartialEq, DomainInput)]
    enum Order {
        Sorted,
        Reversed,
        Shuffled,
    }
    #[derive(DomainInput)]
    struct Scenario {
        overlap: Overlap,
        cardinality: Cardinality,
        duplicates: Duplicates,
        order: Order,
    }

    fn execute(s: &Scenario) -> Result<Vec<String>, ()> {
        let (n, m) = match s.cardinality {
            Cardinality::Empty => (0, 0),
            Cardinality::Singleton => (1, 1),
            Cardinality::Unequal => (7, 13),
            Cardinality::Equal => (12, 12),
        };
        let mut a: Vec<u64> = (0..n).collect();
        let start = match s.overlap {
            Overlap::None => 100,
            Overlap::Partial => n / 2,
            Overlap::Full => 0,
        };
        let mut b: Vec<u64> = (start..start + m).collect();
        if matches!(s.duplicates, Duplicates::Left | Duplicates::Both) {
            a.extend(a.clone().into_iter().step_by(2));
        }
        if matches!(s.duplicates, Duplicates::Right | Duplicates::Both) {
            b.extend(b.clone().into_iter().step_by(3));
        }
        a.sort_unstable();
        b.sort_unstable();
        match s.order {
            Order::Sorted => (),
            Order::Reversed => {
                a.reverse();
                b.reverse();
            }
            Order::Shuffled => {
                shuffle(&mut a, 179);
                shuffle(&mut b, 281);
            }
        }
        let actual = join_by(&a, &b, |v| *v, |v| *v);
        let expected = super::reference(&a, &b);
        let mut checks = Mismatches::default();
        checks.equal("map reference", actual.comparison(), &expected);
        let composite = join_by(&a, &b, |v| (v % 3, *v), |v| (v % 3, *v));
        checks.equal(
            "generic complete",
            &composite.comparison().is_complete(),
            &expected.is_complete(),
        );
        let sorted = |v: &[u64]| v.windows(2).all(|w| w[0] <= w[1]);
        if sorted(&a) && sorted(&b) {
            checks.equal(
                "presorted parity",
                join_presorted_by(&a, &b, |v| *v, |v| *v)
                    .unwrap()
                    .comparison(),
                &expected,
            );
        } else {
            let (side, values) = if !sorted(&a) {
                (Side::Left, &a)
            } else {
                (Side::Right, &b)
            };
            let index = values.windows(2).position(|w| w[0] > w[1]).unwrap() + 1;
            let error = join_presorted_by(&a, &b, |v| *v, |v| *v).err().unwrap();
            checks.equal("unsorted side", &error.side, &side);
            checks.equal("unsorted index", &error.index, &index);
        }
        Ok(checks.finish())
    }

    #[test]
    fn generated_join_structure_matches_map_reference_and_presorted_contract() {
        let mut c = certification("join semantic interactions").executor(execute);
        c.domain(domain!(Scenario { overlap.all_enum_values(), cardinality.all_enum_values(), duplicates.all_enum_values(), order.all_enum_values() }));
        c.seed(179).cases_budget(160);
        let case = c.case();
        c.check(fact!(case.result().clone()).expect(Vec::<String>::new()));
        c.require_pairwise_coverage();
        c.assert_certify();
    }
}
