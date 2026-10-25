use super::domain::compilation::coordinate_key;
use super::*;

#[test]
fn streamed_negative_distances_match_ordered_pair_extrema_with_duplicates_and_overflow() {
    use super::domain::{DomainOperand, DomainRelation, DomainValue, RelationKind};
    let sets = [
        vec![
            DomainValue::Int(-9),
            DomainValue::Int(0),
            DomainValue::Int(0),
            DomainValue::Int(23),
        ],
        vec![
            DomainValue::UInt(0),
            DomainValue::UInt(1),
            DomainValue::UInt(u128::MAX),
        ],
        vec![
            DomainValue::Real(-f64::MAX),
            DomainValue::Real(-0.0),
            DomainValue::Real(0.0),
            DomainValue::Real(f64::MAX),
        ],
        vec![DomainValue::Bool(false), DomainValue::Bool(true)],
        vec![DomainValue::Text("a".into()), DomainValue::Text("z".into())],
    ];
    let distance = |a: &DomainValue, b: &DomainValue| match (a, b) {
        (DomainValue::Int(a), DomainValue::Int(b)) => a.abs_diff(*b) as f64,
        (DomainValue::UInt(a), DomainValue::UInt(b)) => a.abs_diff(*b) as f64,
        (DomainValue::Real(a), DomainValue::Real(b)) => (a - b).abs(),
        _ => 0.,
    };
    for values in sets {
        for kind in [
            RelationKind::LessThan,
            RelationKind::LessEqual,
            RelationKind::GreaterThan,
            RelationKind::GreaterEqual,
            RelationKind::Equal,
            RelationKind::NotEqual,
        ] {
            let relation = DomainRelation {
                left: DomainOperand::Value(values[0].clone()),
                right: DomainOperand::Value(values[0].clone()),
                kind,
            };
            let pairs = relation.invalid_pairs(&values, &values);
            let mut reference = Vec::new();
            for a in &values {
                for b in &values {
                    let comparison = DomainRelation {
                        left: DomainOperand::Value(a.clone()),
                        right: DomainOperand::Value(b.clone()),
                        kind,
                    };
                    if comparison.evaluate(&[]) == Some(false)
                        && !reference.contains(&(a.clone(), b.clone()))
                    {
                        reference.push((a.clone(), b.clone()));
                    }
                }
            }
            reference.sort_by(|(a, b), (c, d)| {
                distance(a, b)
                    .total_cmp(&distance(c, d))
                    .then_with(|| (a.key(), b.key()).cmp(&(c.key(), d.key())))
            });
            assert_eq!(format!("{pairs:?}"), format!("{reference:?}"));
            for (a, b) in &pairs {
                let low = distance(&pairs.first().unwrap().0, &pairs.first().unwrap().1);
                let high = distance(&pairs.last().unwrap().0, &pairs.last().unwrap().1);
                let value = distance(a, b);
                let expected = if high <= low || value <= low + (high - low) / 3. {
                    DomainNegativeDistance::Near
                } else if value < low + 2. * ((high - low) / 3.) {
                    DomainNegativeDistance::Mid
                } else {
                    DomainNegativeDistance::Far
                };
                assert_eq!(relation.negative_distance(a, b, &values, &values), expected);
            }
            assert_eq!(
                relation.negative_distance(&values[0], &values[0], &[], &values),
                DomainNegativeDistance::Near
            );
        }
    }
}

#[derive(Clone, Debug, DomainInput)]
struct SolverNumbers {
    a: i64,
    b: i64,
    c: i64,
    d: i64,
}
#[test]
fn relation_pair_witnesses_preserve_coordinates_and_exhaustive_feasibility() {
    for (topology, relations, end) in [
        (0, 0, 1000),
        (0, 1, 1000),
        (0, 3, 1000),
        (0, 6, 8),
        (1, 3, 1000),
    ] {
        let f = DomainField::<SolverNumbers>::root().fields();
        let mut domain = domain!(SolverNumbers { a.in_range(0..=end), b.in_range(0..=end), c.in_range(0..=end), d.in_range(0..=end) });
        let rules = if topology == 1 {
            vec![
                f.a.less_equal(&f.b),
                f.a.less_equal(&f.c),
                f.a.less_equal(&f.d),
            ]
        } else {
            vec![
                f.a.less_equal(&f.b),
                f.b.less_equal(&f.c),
                f.c.less_equal(&f.d),
                f.a.less_equal(&f.c),
                f.a.less_equal(&f.d),
                f.b.less_equal(&f.d),
            ]
        };
        for rule in rules.into_iter().take(relations) {
            domain.add(rule, &[]);
        }
        let compiled = domain.compile().unwrap();
        let mut signatures = Vec::new();
        for seed in [1, 719, 184729361] {
            let mut occupied = BTreeSet::new();
            let mut cursor = 0;
            let mut pairs = BTreeSet::new();
            let mut witnesses = Vec::new();
            while let Some((input, coordinate)) =
                compiled.next(2, &mut cursor, seed, &occupied).unwrap()
            {
                let actual = compiled.classify(&input).unwrap();
                assert_eq!(actual.violated, coordinate.violated);
                assert_eq!(coordinate.violated.len(), 2);
                pairs.insert(coordinate.violated.clone());
                occupied.insert(coordinate_key(&coordinate));
                witnesses.push((
                    [input.a, input.b, input.c, input.d],
                    coordinate_key(&coordinate),
                ));
            }
            if end == 8 {
                let mut oracle = BTreeSet::new();
                for a in -1..=9 {
                    for b in -1..=9 {
                        for c in -1..=9 {
                            for d in -1..=9 {
                                let flags = [
                                    !(0..=end).contains(&a),
                                    !(0..=end).contains(&b),
                                    !(0..=end).contains(&c),
                                    !(0..=end).contains(&d),
                                    a > b,
                                    b > c,
                                    c > d,
                                    a > c,
                                    a > d,
                                    b > d,
                                ];
                                let violated: Vec<_> = flags
                                    .into_iter()
                                    .enumerate()
                                    .filter_map(|(id, bad)| bad.then_some(id))
                                    .collect();
                                if violated.len() == 2 {
                                    oracle.insert(violated);
                                }
                            }
                        }
                    }
                }
                assert_eq!(pairs, oracle);
            }
            let portable = format!("{witnesses:?}")
                .bytes()
                .fold(0xcbf29ce484222325u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                });
            signatures.push(portable);
        }
        let expected = match (topology, relations, end) {
            (0, 0, 1000) => 3947585941243040917,
            (0, 1, 1000) => 15794665842306307549,
            (0, 3, 1000) => 17098276074675806099,
            (0, 6, 8) => 10467569229425296998,
            (1, 3, 1000) => 9536504339960619074,
            _ => unreachable!(),
        };
        assert_eq!(signatures, [expected; 3]);
    }
}
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};
#[derive(DomainInput, Debug, Clone)]
struct Pair {
    left: i32,
    right: i32,
}
#[test]
fn all_scalar_relations_use_their_own_exact_negative_semantics() {
    let domains = [
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.less_than(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.less_equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.greater_than(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.greater_equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.equal(right)}),
        domain!(Pair{left.in_range(0..=9),right.in_range(0..=9),left.not_equal(right)}),
    ];
    for (kind, domain) in domains.into_iter().enumerate() {
        let d = domain.compile().unwrap();
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut relation_negative = false;
        while let Some((i, c)) = d.next(1, &mut cursor, 1, &occupied).unwrap() {
            if c.violated == [2] {
                let valid = match kind {
                    0 => i.left < i.right,
                    1 => i.left <= i.right,
                    2 => i.left > i.right,
                    3 => i.left >= i.right,
                    4 => i.left == i.right,
                    _ => i.left != i.right,
                };
                assert!(!valid);
                assert!((0..=9).contains(&i.left));
                assert!((0..=9).contains(&i.right));
                relation_negative = true;
            }
            occupied.insert(coordinate_key(&c));
        }
        assert!(relation_negative);
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum Choice {
    A,
    B,
    C,
}
#[derive(DomainInput, Debug, Clone)]
struct Choices {
    a: Choice,
    b: Choice,
    c: Choice,
}
#[test]
fn enums_cover_individual_then_pairwise_then_higher_order_without_cartesian_eager_cases() {
    let d = domain!(Choices{a.all_enum_values(),b.all_enum_values(),c.all_enum_values()})
        .compile()
        .unwrap();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut sectors = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut singles = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut count = 0;
    while let Some((_, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        for (i, e) in c.entries.iter().enumerate() {
            sectors[i].insert(e.sector.clone());
            singles[i].insert(e.slot.clone());
        }
        occupied.insert(coordinate_key(&c));
        count += 1;
        if count == 7 {
            assert!(sectors.iter().all(|s| s.len() == 1));
            assert!(singles.iter().all(|s| s.len() == 3));
        }
    }
    assert!(count >= 19);
    assert!(count <= 27);
}
#[derive(DomainInput, Debug)]
struct Discrete {
    choice: Choice,
    retries: u8,
    text: String,
}
#[test]
fn excluded_enum_and_selected_scalar_cases_use_the_real_rejection_path() {
    let seen = RefCell::new(vec![]);
    let mut c = certification("discrete rejection").executor(|i: &Discrete| {
        seen.borrow_mut().push((i.choice.clone(), i.retries));
        if matches!(i.choice, Choice::A | Choice::C)
            && [0, 3, 5].contains(&i.retries)
            && i.text == "input"
        {
            Ok(())
        } else {
            Err(())
        }
    });
    c.domain(domain!(Discrete{choice.selected_enum_values([Choice::A,Choice::C]),retries.selected_values([0,3,5]),text.fixed_value("input".into())}));
    c.cases_budget(30);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert!(seen.borrow().iter().any(|(choice, _)| *choice == Choice::B));
}
#[derive(DomainInput, Debug)]
struct Single {
    value: i32,
}
#[test]
fn disconnected_ranges_keep_gap_and_branch_sectors_separate() {
    let d = domain!(Single{value.one_of_ranges([1..=3,10..=12])})
        .compile()
        .unwrap();
    let gap = d.classify(&Single { value: 7 }).unwrap();
    assert_eq!(gap.violated, [0]);
    assert_eq!(gap.entries[0].sector.region, DomainRegion::Gap(0));
    assert_ne!(
        d.classify(&Single { value: 1 }).unwrap().entries[0].sector,
        d.classify(&Single { value: 10 }).unwrap().entries[0].sector
    );
}
#[test]
fn data_source_prefill_skips_an_anchor_even_when_other_dimensions_differ() {
    let seen = RefCell::new(vec![]);
    let mut c = certification("semantic prefill").executor(|i: &Pair| {
        seen.borrow_mut().push((i.left, i.right));
        if (1..=9).contains(&i.left) && (1..=9).contains(&i.right) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.data_source([Pair { left: 1, right: 9 }]);
    c.domain(domain!(Pair{left.in_range(1..=9),right.in_range(1..=9)}));
    c.cases_budget(2);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert_eq!(seen.borrow()[1], (1, 1));
    let d = domain!(Pair{left.in_range(1..=9),right.in_range(1..=9)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    occupied.insert(coordinate_key(
        &d.classify(&Pair { left: 1, right: 9 }).unwrap(),
    ));
    let mut cursor = 0;
    d.next(0, &mut cursor, 1, &occupied).unwrap().unwrap();
    assert!(cursor > 1);
}
#[test]
fn independent_checks_continue_after_a_failure_and_eager_checks_register() {
    let checked = Cell::new(0);
    let mut c = certification("independent checks").executor(|_: &Single| Ok::<_, ()>(1));
    c.data_source([Single { value: 1 }]);
    let case = c.case();
    c.check(fact!(*case.result()).expect(2));
    c.check(
        fact!({
            checked.set(checked.get() + 1);
            *case.result()
        })
        .expect(1),
    );
    c.check(fact!(4).expect(4));
    let r = c.report();
    assert!(!r.is_pass());
    assert_eq!(checked.get(), 1);
}
#[test]
fn lower_categories_are_explored_and_unused_failure_capacity_is_redistributed() {
    let mut c = certification("only positive fails").executor(|i: &Pair| {
        if (0..=9).contains(&i.left) && (0..=9).contains(&i.right) {
            Ok(0)
        } else {
            Err(())
        }
    });
    c.domain(domain!(Pair{left.in_range(0..=9),right.in_range(0..=9)}));
    c.cases_budget(200);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1));
    let r = c.report();
    assert!(r.stopped, "{r}");
    assert_eq!(r.failures.len(), 5);
    assert!(r.categories[1] > 0 && r.categories[2] > 0);
    assert!(r
        .failures
        .iter()
        .all(|f| f.category == CaseCategory::Positive));
}
#[test]
fn multi_negative_failures_are_sought_when_single_negatives_reject_correctly() {
    let mut c = certification("double failures").executor(|i: &Pair| {
        let invalid =
            usize::from(!(0..=9).contains(&i.left)) + usize::from(!(0..=9).contains(&i.right));
        if invalid == 1 {
            Err(())
        } else {
            Ok(0)
        }
    });
    c.domain(domain!(Pair{left.in_range(0..=9),right.in_range(0..=9)}));
    c.cases_budget(100);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1));
    let r = c.report();
    assert!(
        r.failures
            .iter()
            .any(|f| f.category == CaseCategory::MultiNegative),
        "{r}"
    );
    assert!(r
        .failures
        .iter()
        .all(|f| f.category != CaseCategory::SingleNegative));
}
#[derive(DomainInput, Debug)]
struct Conditional {
    mode: Choice,
    count: usize,
}
#[test]
fn missing_and_overlapping_conditional_branches_are_preflight_errors() {
    assert!(domain!(Conditional{mode.all_enum_values(),when(mode.equal(Choice::A)){count.fixed_value(1)}}).compile().is_err());
    assert!(domain!(Conditional{mode.all_enum_values(),count.fixed_value(1),when(mode.equal(Choice::A)){count.fixed_value(2)}}).compile().is_err());
}
#[test]
fn real_inputs_outside_domain_are_classified_and_rejected_by_the_same_executor() {
    let calls = Cell::new(0);
    let mut c = certification("real negative").executor(|i: &Single| {
        calls.set(calls.get() + 1);
        if (1..=9).contains(&i.value) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.data_source([Single { value: 0 }]);
    c.domain(domain!(Single{value.in_range(1..=9)}));
    c.cases_budget(5);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert!(r.categories[1] > 0);
    assert_eq!(r.executed[0], 1);
    assert_eq!(calls.get(), r.cases_executed());
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum Octet {
    V0,
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
}
#[derive(DomainInput, Debug, Clone)]
struct WideDomain {
    e0: Octet,
    e1: Octet,
    e2: Octet,
    e3: Octet,
    e4: Octet,
    e5: Octet,
    e6: Octet,
    e7: Octet,
    n0: i32,
    n1: i32,
    n2: i32,
    n3: i32,
    n4: i32,
    n5: i32,
}
#[test]
fn high_cardinality_domain_compiles_lazily_and_bounds_solver_work_by_cases_budget() {
    let spec = domain!(WideDomain {
        e0.all_enum_values(),
        e1.all_enum_values(),
        e2.all_enum_values(),
        e3.all_enum_values(),
        e4.all_enum_values(),
        e5.all_enum_values(),
        e6.all_enum_values(),
        e7.all_enum_values(),
        n0.in_range(1..=100),
        n1.in_range(1..=100),
        n2.in_range(1..=100),
        n3.in_range(1..=100),
        n4.in_range(1..=100),
        n5.in_range(1..=100)
    });
    compilation::reset_solve_calls();
    compilation::reset_enum_pair_context_checks();
    let compiled = spec.clone().compile().unwrap();
    let compile_solves = compilation::solve_calls();
    let pair_context_checks = compilation::enum_pair_context_checks();
    assert!(compile_solves <= 2, "{compile_solves}");
    assert!(pair_context_checks <= 32, "{pair_context_checks}");
    assert_eq!(compiled.coverage_catalog().enum_values.len(), 64);
    assert_eq!(compiled.coverage_catalog().pairwise_total, 28 * 64);

    let mut c = certification("lazy wide domain").executor(|i: &WideDomain| {
        if (1..=100).contains(&i.n0)
            && (1..=100).contains(&i.n1)
            && (1..=100).contains(&i.n2)
            && (1..=100).contains(&i.n3)
            && (1..=100).contains(&i.n4)
            && (1..=100).contains(&i.n5)
        {
            Ok(1usize)
        } else {
            Err(())
        }
    });
    c.domain(spec);
    c.cases_budget(5);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    compilation::reset_solve_calls();
    let r = c.report();
    let run_solves = compilation::solve_calls();
    assert!(r.is_pass(), "{r}");
    assert_eq!(r.cases_executed(), 5);
    assert!(run_solves <= 25, "{run_solves}");
}
#[derive(DomainInput, Debug, Clone)]
struct FloatSingle {
    value: f64,
}
#[test]
fn stratified_range_exploration_is_breadth_first_across_sectors_and_seeded_random_in_depth() {
    let d = domain!(Single {
        value.in_range(1..=10_000)
    })
    .compile()
    .unwrap();
    let sample_sequence = |seed: u64, count: usize| {
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut out = vec![];
        while out.len() < count {
            let (input, coord) = d.next(0, &mut cursor, seed, &occupied).unwrap().unwrap();
            occupied.insert(coordinate_key(&coord));
            out.push((input.value, coord.entries[0].clone()));
        }
        out
    };
    let anchor_count = d.coverage_catalog().positive_anchors.len();
    let seq_a = sample_sequence(7, anchor_count + 30);
    let seq_a_repeat = sample_sequence(7, anchor_count + 30);
    let seq_b = sample_sequence(99, anchor_count + 30);
    assert_eq!(
        seq_a.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
        seq_a_repeat.iter().map(|(v, _)| *v).collect::<Vec<_>>()
    );
    for (idx, (_, entry)) in seq_a[..anchor_count].iter().enumerate() {
        assert_eq!(entry.slot, CaseDomainCoordinateSlot::Anchor(idx));
    }
    assert_eq!(
        seq_a[..anchor_count]
            .iter()
            .map(|(v, _)| *v)
            .collect::<Vec<_>>(),
        seq_b[..anchor_count]
            .iter()
            .map(|(v, _)| *v)
            .collect::<Vec<_>>()
    );
    for chunk in seq_a[anchor_count..].chunks_exact(3) {
        let terciles: Vec<_> = chunk
            .iter()
            .map(|(_, e)| match e.sector.region {
                DomainRegion::Range { tercile, .. } => tercile,
                ref other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            terciles,
            vec![DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
        );
    }
    let low_strata_a: Vec<u64> = seq_a[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    let low_strata_b: Vec<u64> = seq_b[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        low_strata_a.iter().copied().collect::<BTreeSet<_>>().len(),
        low_strata_a.len()
    );
    assert_ne!(
        low_strata_a,
        (0..low_strata_a.len() as u64).collect::<Vec<_>>()
    );
    assert_ne!(low_strata_a, low_strata_b);

    let full_seq = sample_sequence(7, anchor_count + 64 * 3);
    let all_low: BTreeSet<u64> = full_seq[anchor_count..]
        .iter()
        .step_by(3)
        .map(|(_, e)| match e.slot {
            CaseDomainCoordinateSlot::Stratum(bin) => bin % 64,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(all_low.len(), 64);

    let disconnected = domain!(Single {
        value.one_of_ranges([0..=300, 1000..=1300])
    })
    .compile()
    .unwrap();
    let disc_anchors = disconnected.coverage_catalog().positive_anchors.len();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut multi = vec![];
    while multi.len() < disc_anchors + 6 {
        let (_, coord) = disconnected
            .next(0, &mut cursor, 11, &occupied)
            .unwrap()
            .unwrap();
        occupied.insert(coordinate_key(&coord));
        multi.push(coord.entries[0].clone());
    }
    let branch_regions: Vec<_> = multi[disc_anchors..]
        .iter()
        .map(|e| e.sector.region.clone())
        .collect();
    assert_eq!(
        branch_regions,
        vec![
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::Low
            },
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::Mid
            },
            DomainRegion::Range {
                branch: 0,
                tercile: DomainTercile::High
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::Low
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::Mid
            },
            DomainRegion::Range {
                branch: 1,
                tercile: DomainTercile::High
            },
        ]
    );

    let df = domain!(FloatSingle {
        value.in_range(0.0..=300.0)
    })
    .compile()
    .unwrap();
    let mut f_cursor = 0;
    let mut f_occupied = BTreeSet::new();
    let mut f_entries = vec![];
    while f_entries.len() < 7 + 12 {
        let (_, coord) = df.next(0, &mut f_cursor, 23, &f_occupied).unwrap().unwrap();
        f_occupied.insert(coordinate_key(&coord));
        f_entries.push(coord.entries[0].clone());
    }
    for chunk in f_entries[7..].chunks_exact(3) {
        let terciles: Vec<_> = chunk
            .iter()
            .map(|e| match e.sector.region {
                DomainRegion::Range { tercile, .. } => tercile,
                ref other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            terciles,
            vec![DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
        );
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ChoiceVec {
    items: Vec<Choice>,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct IntVec {
    items: Vec<i32>,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ConditionalItem {
    mode: Choice,
    value: i32,
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
struct ConditionalBatch {
    items: Vec<ConditionalItem>,
}
#[test]
fn generated_collections_produce_heterogeneous_compositions_and_preserve_negative_invariants() {
    let d_enum = domain!(ChoiceVec {
        items {
            length().in_range(2..=4),
            each(all_enum_values())
        }
    })
    .compile()
    .unwrap();
    let sample_enum_vec = |seed: u64| {
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut cases = vec![];
        while let Some((input, coord)) = d_enum.next(0, &mut cursor, seed, &occupied).unwrap() {
            assert_eq!(
                coordinate_key(&d_enum.classify(&input).unwrap()),
                coordinate_key(&coord)
            );
            occupied.insert(coordinate_key(&coord));
            cases.push((input.items, coord));
            if cases.len() >= 10 {
                break;
            }
        }
        cases
    };
    let enum_cases = sample_enum_vec(42);
    let enum_cases_repeat = sample_enum_vec(42);
    assert_eq!(
        enum_cases
            .iter()
            .map(|(v, _)| v.clone())
            .collect::<Vec<_>>(),
        enum_cases_repeat
            .iter()
            .map(|(v, _)| v.clone())
            .collect::<Vec<_>>()
    );
    assert!(enum_cases
        .iter()
        .any(|(items, _)| items.len() >= 2 && items.iter().all(|x| x == &items[0])));
    assert!(enum_cases
        .iter()
        .any(|(items, _)| items.windows(2).any(|w| w[0] != w[1])));
    assert!(enum_cases.iter().any(|(_, coord)| {
        let variant_slots: BTreeSet<_> = coord
            .entries
            .iter()
            .filter_map(|e| match e.slot {
                CaseDomainCoordinateSlot::EnumVariant(tag) => Some(tag),
                _ => None,
            })
            .collect();
        variant_slots.len() == 3
    }));

    let prefilled_coord = d_enum
        .classify(&ChoiceVec {
            items: vec![Choice::A, Choice::B, Choice::A],
        })
        .unwrap();
    let mut occupied_prefill = BTreeSet::from([coordinate_key(&prefilled_coord)]);
    let mut cursor_prefill = 0;
    let mut saw_ab_only_mix = false;
    while let Some((input, coord)) = d_enum
        .next(0, &mut cursor_prefill, 42, &occupied_prefill)
        .unwrap()
    {
        occupied_prefill.insert(coordinate_key(&coord));
        let distinct: BTreeSet<_> = input
            .items
            .iter()
            .map(|c| match c {
                Choice::A => 0usize,
                Choice::B => 1usize,
                Choice::C => 2usize,
            })
            .collect();
        if distinct == BTreeSet::from([0usize, 1usize]) {
            saw_ab_only_mix = true;
        }
    }
    assert!(!saw_ab_only_mix);

    let d_int = domain!(IntVec {
        items {
            length().in_range(3..=5),
            each(in_range(1..=30))
        }
    })
    .compile()
    .unwrap();
    let mut cursor_int = 0;
    let mut occupied_int = BTreeSet::new();
    let mut int_cases = vec![];
    while let Some((input, coord)) = d_int.next(0, &mut cursor_int, 17, &occupied_int).unwrap() {
        occupied_int.insert(coordinate_key(&coord));
        int_cases.push((input.items, coord));
        if int_cases.len() >= 20 {
            break;
        }
    }
    assert!(int_cases
        .iter()
        .any(|(items, _)| items.windows(2).any(|w| w[0] != w[1])));
    assert!(int_cases.iter().any(|(_, coord)| {
        let terciles: BTreeSet<_> = coord
            .entries
            .iter()
            .filter_map(|e| match e.sector.region {
                DomainRegion::Range { tercile, .. } if e.sector.constraint == 1 => Some(tercile),
                _ => None,
            })
            .collect();
        terciles.len() == 3
    }));
    let mut neg_cursor = 0;
    let mut neg_occupied = BTreeSet::new();
    while let Some((_, coord)) = d_int.next(1, &mut neg_cursor, 17, &neg_occupied).unwrap() {
        assert_eq!(coord.violated.len(), 1);
        neg_occupied.insert(coordinate_key(&coord));
    }
    assert!(!neg_occupied.is_empty());

    let d_cond = domain!(ConditionalBatch {
        items {
            length().in_range(2..=4),
            each {
                mode.all_enum_values(),
                when(mode.equal(Choice::A)) { value.in_range(1..=3) },
                when(mode.equal(Choice::B)) { value.in_range(10..=12) },
                when(mode.equal(Choice::C)) { value.in_range(20..=22) }
            }
        }
    })
    .compile()
    .unwrap();
    let mut cond_cursor = 0;
    let mut cond_occupied = BTreeSet::new();
    let mut saw_mixed_modes = false;
    while let Some((batch, coord)) = d_cond.next(0, &mut cond_cursor, 5, &cond_occupied).unwrap() {
        cond_occupied.insert(coordinate_key(&coord));
        for item in &batch.items {
            match item.mode {
                Choice::A => assert!((1..=3).contains(&item.value)),
                Choice::B => assert!((10..=12).contains(&item.value)),
                Choice::C => assert!((20..=22).contains(&item.value)),
            }
        }
        if batch.items.windows(2).any(|w| w[0].mode != w[1].mode) {
            saw_mixed_modes = true;
        }
        if cond_occupied.len() >= 18 {
            break;
        }
    }
    assert!(saw_mixed_modes);

    for max_len in [0i32, 1i32] {
        let d_short = domain!(ChoiceVec {
            items {
                length().in_range(0..=max_len as usize),
                each(all_enum_values())
            }
        })
        .compile()
        .unwrap();
        let mut short_cursor = 0;
        let short_occupied = BTreeSet::new();
        let (first, coord) = d_short
            .next(0, &mut short_cursor, 1, &short_occupied)
            .unwrap()
            .unwrap();
        assert!(first.items.len() <= max_len as usize);
        assert!(coord.violated.is_empty());
    }
}
#[derive(DomainInput, Debug, Clone, PartialEq)]
enum QuadChoice {
    A,
    B,
    C,
    D,
}
#[derive(DomainInput, Debug, Clone)]
struct QuadHolder {
    choice: QuadChoice,
}
#[test]
fn enum_coordinates_use_dimension_sector_and_variant_slot_for_positive_and_excluded_variants() {
    let d = domain!(Choices {
        a.all_enum_values(),
        b.all_enum_values(),
        c.all_enum_values()
    })
    .compile()
    .unwrap();
    let c0 = d
        .classify(&Choices {
            a: Choice::A,
            b: Choice::B,
            c: Choice::C,
        })
        .unwrap();
    let c1 = d
        .classify(&Choices {
            a: Choice::B,
            b: Choice::C,
            c: Choice::A,
        })
        .unwrap();
    let c2 = d
        .classify(&Choices {
            a: Choice::C,
            b: Choice::A,
            c: Choice::B,
        })
        .unwrap();
    assert_eq!(c0.entries[0].sector.region, DomainRegion::Enum);
    assert_eq!(c0.entries[0].sector, c1.entries[0].sector);
    assert_eq!(c1.entries[0].sector, c2.entries[0].sector);
    assert_eq!(c0.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(0));
    assert_eq!(c1.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(1));
    assert_eq!(c2.entries[0].slot, CaseDomainCoordinateSlot::EnumVariant(2));

    let mut cert = certification("enum prefill").executor(|_: &Choices| Ok::<usize, ()>(1usize));
    cert.data_source([Choices {
        a: Choice::A,
        b: Choice::A,
        c: Choice::A,
    }]);
    cert.domain(domain!(Choices {
        a.all_enum_values(),
        b.all_enum_values(),
        c.all_enum_values()
    }));
    cert.cases_budget(32);
    let case = cert.case();
    cert.check(fact!(*case.result()).expect(1usize));
    let report = cert.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (9, 9));
    assert_eq!(report.coverage.pairwise_combinations, (27, 27));

    let dq = domain!(QuadHolder {
        choice.selected_enum_values([QuadChoice::A, QuadChoice::C])
    })
    .compile()
    .unwrap();
    let ex_b = dq
        .classify(&QuadHolder {
            choice: QuadChoice::B,
        })
        .unwrap();
    let ex_d = dq
        .classify(&QuadHolder {
            choice: QuadChoice::D,
        })
        .unwrap();
    assert_eq!(ex_b.violated, vec![0]);
    assert_eq!(ex_d.violated, vec![0]);
    assert_eq!(ex_b.entries[0].sector.region, DomainRegion::ExcludedVariant);
    assert_eq!(ex_b.entries[0].sector, ex_d.entries[0].sector);
    assert_eq!(
        ex_b.entries[0].slot,
        CaseDomainCoordinateSlot::ExcludedVariant(1)
    );
    assert_eq!(
        ex_d.entries[0].slot,
        CaseDomainCoordinateSlot::ExcludedVariant(3)
    );
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum NestedChoice {
    A,
    B,
    C,
    D { inner: QuadChoice },
}
#[derive(DomainInput, Debug, Clone)]
struct NestedChoiceHolder {
    choice: NestedChoice,
}
#[test]
fn nested_enum_pairwise_coverage_counts_only_feasible_combinations() {
    let mut c = certification("nested enum pair coverage")
        .executor(|_: &NestedChoiceHolder| Ok::<usize, ()>(1));
    c.domain(domain!(NestedChoiceHolder {
        choice {
            all_enum_values(),
            NestedChoice::D { inner.all_enum_values() }
        }
    }));
    c.cases_budget(32).require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (8, 8));
    assert_eq!(report.coverage.pairwise_combinations, (4, 4));
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum ConditionalBackend {
    X,
    Y,
    Z,
}
#[derive(DomainInput, Debug, Clone)]
struct ConditionalEnumPair {
    mode: BinaryMode,
    backend: ConditionalBackend,
}
#[test]
fn conditional_enum_pairwise_coverage_counts_only_active_combinations() {
    let mut c =
        certification("conditional enum pair coverage").executor(|input: &ConditionalEnumPair| {
            match (&input.mode, &input.backend) {
                (BinaryMode::A, ConditionalBackend::X)
                | (BinaryMode::B, ConditionalBackend::Y | ConditionalBackend::Z) => Ok(1usize),
                _ => Err(()),
            }
        });
    c.domain(domain!(ConditionalEnumPair {
        mode.all_enum_values(),
        when(mode.equal(BinaryMode::A)) {
            backend.selected_enum_values([ConditionalBackend::X])
        },
        when(mode.equal(BinaryMode::B)) {
            backend.selected_enum_values([ConditionalBackend::Y, ConditionalBackend::Z])
        }
    }));
    c.cases_budget(32).require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (5, 5));
    assert_eq!(report.coverage.pairwise_combinations, (3, 3));
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum BinaryMode {
    A,
    B,
}
#[derive(DomainInput, Debug, Clone)]
struct OverlapItem {
    mode: BinaryMode,
    value: i32,
}
#[derive(DomainInput, Debug, Clone)]
struct OverlapBatch {
    items: Vec<OverlapItem>,
}
#[test]
fn conditional_collection_compositions_are_generated_in_each_active_branch() {
    let d = domain!(OverlapBatch {
        items {
            length().fixed_value(4),
            each {
                mode.all_enum_values(),
                when(mode.equal(BinaryMode::A)) { value.in_range(1..=30) },
                when(mode.equal(BinaryMode::B)) { value.in_range(1..=30) }
            }
        }
    })
    .compile()
    .unwrap();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut saw_a = false;
    let mut saw_b = false;
    while let Some((batch, coordinate)) = d.next(0, &mut cursor, 41, &occupied).unwrap() {
        occupied.insert(coordinate_key(&coordinate));
        let heterogeneous = batch.items.windows(2).any(|w| w[0].value != w[1].value);
        if !heterogeneous {
            continue;
        }
        if batch.items.iter().all(|item| item.mode == BinaryMode::A) {
            saw_a = true;
        }
        if batch.items.iter().all(|item| item.mode == BinaryMode::B) {
            saw_b = true;
        }
        if saw_a && saw_b {
            break;
        }
    }
    assert!(saw_a && saw_b, "saw_a={saw_a}, saw_b={saw_b}");
}

#[derive(DomainInput, Debug, Clone, PartialEq)]
enum RepeatedChoice {
    One,
    Two,
}
#[derive(DomainInput, Debug, Clone)]
struct RepeatedConditionalEnums {
    mode: BinaryMode,
    left: RepeatedChoice,
    right: RepeatedChoice,
}
#[test]
fn pairwise_enum_coverage_distinguishes_same_slots_in_different_conditional_sectors() {
    let mut c = certification("conditional enum sector pairs")
        .executor(|_: &RepeatedConditionalEnums| Ok::<usize, ()>(1));
    c.domain(domain!(RepeatedConditionalEnums {
        mode.all_enum_values(),
        when(mode.equal(BinaryMode::A)) {
            left.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two]),
            right.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two])
        },
        when(mode.equal(BinaryMode::B)) {
            left.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two]),
            right.selected_enum_values([RepeatedChoice::One, RepeatedChoice::Two])
        }
    }));
    c.cases_budget(22);
    let case = c.case();
    c.check(fact!(*case.result()).expect(1usize));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.enum_values, (10, 10));
    assert_eq!(report.coverage.pairwise_combinations, (16, 16));
}
