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
    let expected = super::tests::reference(&a, &b);
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
