use super::domain::compilation::coordinate_key;
use super::*;
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
    let mut singles = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut count = 0;
    while let Some((_, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        for (i, e) in c.entries.iter().enumerate() {
            singles[i].insert(e.sector.clone());
        }
        occupied.insert(coordinate_key(&c));
        count += 1;
        if count == 7 {
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
