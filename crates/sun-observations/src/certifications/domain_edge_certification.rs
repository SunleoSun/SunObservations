use super::domain::compilation::coordinate_key;
use super::*;
use std::{cell::Cell, collections::BTreeSet};
#[test]
fn two_enum_dimensions_finish_pairwise_gaps_after_individual_slots() {
    #[derive(DomainInput)]
    enum Choice {
        A,
        B,
        C,
    }
    #[derive(DomainInput)]
    struct Input {
        a: Choice,
        b: Choice,
    }
    let d = domain!(Input{a.all_enum_values(),b.all_enum_values()})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    while let Some((_, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        occupied.insert(coordinate_key(&c));
    }
    assert_eq!(occupied.len(), 9);
}
#[test]
fn mandatory_real_cases_execute_before_no_generation_capacity_is_reported() {
    let calls = Cell::new(0);
    let mut c = certification("mandatory real inputs").executor(|i: &usize| {
        calls.set(calls.get() + 1);
        if (1..=9).contains(i) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.domain(domain!(usize{in_range(1..=9)}));
    c.data_source([1usize, 2, 3]);
    c.cases_budget(1);
    let r = c.report();
    assert_eq!(calls.get(), 3);
    assert_eq!(r.executed, [3, 0]);
    assert!(r.evaluation_error.unwrap().message.contains("No budget"));
}
#[test]
fn unsigned_128_values_preserve_materialization_and_structural_identity() {
    let start = i128::MAX as u128 + 10;
    let d = domain!(u128{in_range(start..=start+8)}).compile().unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut values = vec![];
    while let Some((input, c)) = d.next(0, &mut cursor, 42, &occupied).unwrap() {
        assert!((start..=start + 8).contains(&input));
        assert_eq!(d.classify(&input).unwrap(), c);
        values.push(input);
        occupied.insert(coordinate_key(&c));
    }
    assert_eq!(values.len(), 9);
    assert_eq!(&values[..3], &[start, start + 8, start + 4]);
}
#[test]
fn certification_identity_has_no_shared_rng_or_test_order_state() {
    fn run() -> Vec<i32> {
        let seen = std::cell::RefCell::new(vec![]);
        let mut c = certification("stable definition").executor(|i: &Scalar| {
            seen.borrow_mut().push(i.value);
            if (1..=1000).contains(&i.value) {
                Ok(())
            } else {
                Err(())
            }
        });
        c.domain(domain!(Scalar{value.in_range(1..=1000)}));
        c.cases_budget(30);
        c.seed(77);
        c.assert_certify();
        seen.into_inner()
    }
    let first = run();
    let mut other = certification("unrelated").executor(|i: &usize| Ok::<_, ()>(*i));
    other.data_source([3usize]);
    other.assert_certify();
    assert_eq!(first, run());
    assert_eq!(first, std::thread::spawn(run).join().unwrap());
}

#[derive(DomainInput, Debug)]
struct Scalar {
    value: i32,
}
thread_local! {static MATERIALIZED:Cell<usize>=const{Cell::new(0)};}
struct Counted {
    value: i32,
}
impl DomainInput for Counted {
    type Fields = ScalarDomainFields;
    fn schema() -> DomainSchema {
        Scalar::schema()
    }
    fn domain_fields(field: DomainField<Self>) -> Self::Fields {
        ScalarDomainFields {
            value: field.child(DomainPathStep::Field(0)),
        }
    }
    fn to_domain_value(&self) -> DomainValue {
        Scalar { value: self.value }.to_domain_value()
    }
    fn from_domain_value(value: DomainValue) -> Result<Self, AnalysisError> {
        MATERIALIZED.with(|n| n.set(n.get() + 1));
        Scalar::from_domain_value(value).map(|v| Self { value: v.value })
    }
}
#[test]
fn hard_stop_prevents_even_materializing_another_case() {
    MATERIALIZED.with(|n| n.set(0));
    let executed = Cell::new(0);
    let mut c = certification("materialization boundary").executor(|_: &Counted| {
        executed.set(executed.get() + 1);
        Err::<(), _>(())
    });
    c.domain(domain!(Counted{value.in_range(1..=1000)}));
    c.failure_budget(1);
    c.cases_budget(100);
    let r = c.report();
    assert!(r.stopped);
    assert_eq!(r.cases_executed(), 1);
    assert_eq!(executed.get(), 1);
    assert_eq!(MATERIALIZED.with(Cell::get), 1);
}
#[derive(DomainInput, Debug, PartialEq)]
enum Mode {
    A,
    B,
}
#[derive(DomainInput, Debug)]
struct Row {
    mode: Mode,
    value: i32,
}
#[derive(DomainInput, Debug)]
struct Batch {
    rows: Vec<Row>,
}
fn batch_domain() -> Domain<Batch> {
    domain!(Batch{rows{length().in_range(2..=3),each{mode.all_enum_values(),when(mode.equal(Mode::A)){value.in_range(1..=3)},when(mode.equal(Mode::B)){value.in_range(5..=7)}}}})
}
#[test]
fn heterogeneous_collection_rows_classify_their_own_conditions_and_slots() {
    let d = batch_domain().compile().unwrap();
    let good = Batch {
        rows: vec![
            Row {
                mode: Mode::A,
                value: 1,
            },
            Row {
                mode: Mode::B,
                value: 7,
            },
        ],
    };
    let c = d.classify(&good).unwrap();
    assert!(c.violated.is_empty());
    assert_eq!(
        c.entries
            .iter()
            .filter(|e| e.name.ends_with("mode"))
            .count(),
        2
    );
    let bad = Batch {
        rows: vec![
            Row {
                mode: Mode::A,
                value: 1,
            },
            Row {
                mode: Mode::B,
                value: 1,
            },
        ],
    };
    assert_eq!(d.classify(&bad).unwrap().violated.len(), 1);
}
#[derive(DomainInput, Debug)]
struct Float {
    value: f32,
}
#[test]
fn f32_materialization_round_trips_coordinates_and_negative_near_is_adjacent() {
    let d = domain!(Float{value.in_range(0.0..=0.001)})
        .compile()
        .unwrap();
    for category in [0, 1] {
        let mut cursor = 0;
        let mut occupied = BTreeSet::new();
        let mut count = 0;
        while let Some((input, c)) = d.next(category, &mut cursor, 42, &occupied).unwrap() {
            assert_eq!(d.classify(&input).unwrap(), c);
            if category == 1
                && input.value < 0.
                && matches!(
                    c.entries[0].slot,
                    CaseDomainCoordinateSlot::Negative(DomainNegativeDistance::Near)
                )
            {
                assert_eq!(input.value, -f32::from_bits(1));
            }
            occupied.insert(coordinate_key(&c));
            count += 1;
            if count == 20 {
                break;
            }
        }
        assert!(count > 2);
    }
}
#[test]
fn a_redundant_relation_can_still_have_a_pairwise_negative() {
    #[derive(DomainInput)]
    struct Pair {
        left: i32,
        right: i32,
    }
    let d = domain!(Pair{left.in_range(0..=1),right.in_range(2..=3),left.less_equal(right)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut found = false;
    while let Some((_, c)) = d.next(2, &mut cursor, 1, &occupied).unwrap() {
        found |= c.violated.contains(&2);
        occupied.insert(coordinate_key(&c));
    }
    assert!(found);
}
#[test]
fn lazy_higher_order_enum_exploration_fills_the_remaining_triple() {
    #[derive(DomainInput, PartialEq)]
    enum Bit {
        Zero,
        One,
    }
    #[derive(DomainInput)]
    struct Bits {
        a: Bit,
        b: Bit,
        c: Bit,
    }
    let d = domain!(Bits{a.all_enum_values(),b.all_enum_values(),c.all_enum_values()})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut triple = false;
    while let Some((i, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        triple |= i.a == Bit::One && i.b == Bit::One && i.c == Bit::One;
        occupied.insert(coordinate_key(&c));
    }
    assert!(triple);
    assert_eq!(occupied.len(), 8);
}
#[test]
fn relation_negative_slots_have_a_separate_severity_axis() {
    #[derive(DomainInput)]
    struct Pair {
        a: i32,
        b: i32,
    }
    let d = domain!(Pair{a.in_range(0..=90),b.in_range(0..=90),a.less_equal(b)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut distances = BTreeSet::new();
    while let Some((_, c)) = d.next(1, &mut cursor, 1, &occupied).unwrap() {
        if c.violated == [2] {
            for e in &c.entries {
                if e.sector.constraint == 2 {
                    distances.insert(e.slot.clone());
                }
            }
        }
        occupied.insert(coordinate_key(&c));
    }
    assert!(distances.contains(&CaseDomainCoordinateSlot::Negative(
        DomainNegativeDistance::Near
    )));
    assert!(distances.contains(&CaseDomainCoordinateSlot::Negative(
        DomainNegativeDistance::Far
    )));
}
#[test]
fn unmaterializable_and_nonfinite_domains_fail_closed() {
    assert!(domain!(Float{value.in_range(f32::NEG_INFINITY..=1.0)})
        .compile()
        .is_err());
    assert!(domain!(i128{in_range(i128::MIN..=i128::MAX)})
        .compile()
        .is_err());
    #[derive(DomainInput)]
    struct Large {
        values: Vec<u8>,
    }
    assert!(
        domain!(Large{values{length().in_range(1..=2_000_000),each(fixed_value(1))}})
            .compile()
            .is_err()
    );
}
#[test]
fn disconnected_large_finite_ranges_preserve_the_global_center_anchor() {
    let d = domain!(f64{one_of_ranges([-1.7e308..=-1.0e307,1.0e307..=1.7e308])})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut values = vec![];
    for _ in 0..3 {
        let (value, coordinate) = d.next(0, &mut cursor, 1, &occupied).unwrap().unwrap();
        occupied.insert(coordinate_key(&coordinate));
        values.push(value);
    }
    assert_eq!(values, vec![-1.7e308, 1.7e308, 1.0e307]);
}
#[test]
fn empty_named_struct_has_one_materializable_positive_case() {
    #[derive(DomainInput)]
    struct Empty {}
    let mut c = certification("empty input").executor(|_: &Empty| Ok::<_, ()>(()));
    c.domain(domain!(Empty {}));
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert_eq!(r.cases_executed(), 1);
}
#[test]
fn coverage_counts_only_feasible_positive_opportunities() {
    #[derive(DomainInput)]
    struct Pair {
        a: i32,
        b: i32,
    }
    let mut c = certification("feasible coverage").executor(|i: &Pair| {
        if (0..=1).contains(&i.a) && (0..=9).contains(&i.b) && i.a == i.b {
            Ok(())
        } else {
            Err(())
        }
    });
    c.domain(domain!(Pair{a.in_range(0..=1),b.in_range(0..=9),a.equal(b)}));
    c.cases_budget(64);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert!(
        r.coverage.positive_anchors.0 <= r.coverage.positive_anchors.1,
        "{:?}",
        r.coverage
    );
}
#[test]
fn real_triple_negative_does_not_replace_exact_pairwise_cases() {
    #[derive(DomainInput)]
    struct Triple {
        a: i32,
        b: i32,
        c: i32,
    }
    let d = domain!(Triple{a.in_range(1..=3),b.in_range(1..=3),c.in_range(1..=3)})
        .compile()
        .unwrap();
    let real = d.classify(&Triple { a: 0, b: 0, c: 0 }).unwrap();
    assert_eq!(real.violated, vec![0, 1, 2]);
    let mut occupied = BTreeSet::from([coordinate_key(&real)]);
    let mut cursor = 0;
    let mut pairs = BTreeSet::new();
    while let Some((_, coordinate)) = d.next(2, &mut cursor, 1, &occupied).unwrap() {
        pairs.insert(coordinate.violated.clone());
        occupied.insert(coordinate_key(&coordinate));
    }
    assert_eq!(pairs, BTreeSet::from([vec![0, 1], vec![0, 2], vec![1, 2]]));
}
#[test]
fn heterogeneous_collection_rows_do_not_fake_enum_pairwise_coverage() {
    #[derive(DomainInput, Clone, Debug, PartialEq)]
    enum Choice {
        A,
        B,
    }
    #[derive(DomainInput, Debug)]
    struct Row {
        left: Choice,
        right: Choice,
    }
    #[derive(DomainInput, Debug)]
    struct Batch {
        rows: Vec<Row>,
    }
    let domain = domain!(Batch {
        rows {
            length().fixed_value(2),
            each {
                left.all_enum_values(),
                right.all_enum_values(),
            },
        },
    })
    .compile()
    .unwrap();
    let real = Batch {
        rows: vec![
            Row {
                left: Choice::A,
                right: Choice::A,
            },
            Row {
                left: Choice::B,
                right: Choice::B,
            },
        ],
    };
    let mut occupied = BTreeSet::from([coordinate_key(&domain.classify(&real).unwrap())]);
    let mut cursor = 0;
    let mut generated = BTreeSet::new();
    while let Some((input, coordinate)) = domain.next(0, &mut cursor, 1, &occupied).unwrap() {
        let row = &input.rows[0];
        generated.insert((
            matches!(row.left, Choice::B),
            matches!(row.right, Choice::B),
        ));
        occupied.insert(coordinate_key(&coordinate));
    }
    assert!(generated.contains(&(false, true)));
    assert!(generated.contains(&(true, false)));
}
