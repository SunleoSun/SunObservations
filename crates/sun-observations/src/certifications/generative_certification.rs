use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

#[derive(Debug, Clone, PartialEq, DomainInput)]
struct Request {
    count: usize,
    lower: i32,
    upper: i32,
}
fn request_domain() -> Domain<Request> {
    domain!(Request{count.in_range(1..=9),lower.in_range(0..=9),upper.in_range(0..=9),lower.less_equal(upper)})
}
fn execute(input: &Request) -> Result<Vec<i32>, &'static str> {
    if !(1..=9).contains(&input.count)
        || !(0..=9).contains(&input.lower)
        || !(0..=9).contains(&input.upper)
        || input.lower > input.upper
    {
        Err("invalid")
    } else {
        Ok(vec![input.lower; input.count])
    }
}
#[test]
fn eager_macro_and_existing_eager_aggregation_remain_canonical() {
    let mut c = certification("eager");
    c.check("value", fact!(1 + 2).expect(3));
    assert!(c.report().is_pass());
    assert_eq!(c.report().to_string(), "");
}
#[test]
fn eager_fact_named_case_remains_eager() {
    let case = 2usize;
    let mut c = certification("eager local named case");
    c.check("value", fact!(case + 1).expect(3));
    assert!(c.report().is_pass());
}

#[test]
fn data_source_only_planned_execution_does_not_require_domain_schema() {
    struct PlainInput {
        value: i32,
    }

    let mut c = certification("plain real inputs")
        .executor(|input: &PlainInput| Ok::<_, ()>(input.value + 1));
    c.data_source([PlainInput { value: 1 }, PlainInput { value: 4 }]);
    let case = c.case();
    c.check(fact!(*case.result()).expect(fact!(case.input().value + 1)));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.cases_executed(), 2);
}

#[test]
fn planned_executor_repeats_and_resolves_heterogeneous_checks() {
    let calls = Cell::new(0);
    let mut c = certification("generated production request").executor(|input: &Request| {
        calls.set(calls.get() + 1);
        execute(input)
    });
    c.domain(request_domain());
    c.cases_budget(40);
    let case = c.case();
    c.check(
        fact!(case.result().len())
            .expect(fact!(case.input().count))
            .description("output count"),
    );
    let case = c.case();
    c.check(fact!(case.result()[0]).expect(fact!(case.input().lower)));
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(calls.get(), report.cases_executed());
    assert!(calls.get() > 10);
    assert!(report.categories.iter().all(|n| *n > 0));
    assert_eq!(report.to_string(), "");
}
#[test]
fn positive_error_negative_acceptance_and_panic_are_failures() {
    for behavior in 0..3 {
        let mut c = certification("failures").executor(move |_: &Request| -> Result<(), &str> {
            match behavior {
                0 => Err("bad"),
                1 => Ok(()),
                _ => panic!("production panic"),
            }
        });
        c.domain(request_domain());
        c.cases_budget(32);
        let r = c.report();
        assert!(!r.is_pass());
        let text = r.to_string();
        assert!(
            text.contains(match behavior {
                0 => "valid input rejected",
                1 => "invalid input accepted",
                _ => "production panic",
            }),
            "{text}"
        );
    }
}
#[test]
fn expected_negative_rejection_does_not_evaluate_checks() {
    let checks = Cell::new(0);
    let mut c = certification("negative rejection").executor(execute);
    c.domain(request_domain());
    c.cases_budget(20);
    let case = c.case();
    c.check(
        fact!({
            checks.set(checks.get() + 1);
            case.result().len()
        })
        .expect(fact!(case.input().count)),
    );
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert_eq!(checks.get(), r.categories[0]);
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
struct Single {
    value: i32,
}
#[test]
fn range_anchors_precede_seeded_points_and_small_ranges_deduplicate() {
    let d = domain!(Single{value.in_range(1..=9)}).compile().unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut values = vec![];
    while let Some((input, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        values.push(input.value);
        occupied.insert(super::domain::compilation::coordinate_key(&c));
    }
    assert_eq!(values, [1, 9, 5, 3, 4, 6, 7, 2, 8]);
    let d = domain!(Single{value.in_range(1..=2)}).compile().unwrap();
    let mut cursor = 0;
    let mut occupied = BTreeSet::new();
    let mut values = vec![];
    while let Some((i, c)) = d.next(0, &mut cursor, 1, &occupied).unwrap() {
        values.push(i.value);
        occupied.insert(super::domain::compilation::coordinate_key(&c));
    }
    assert_eq!(values, [1, 2]);
}
#[test]
fn incomplete_duplicate_and_unsatisfiable_domains_fail_before_execution() {
    let calls = Cell::new(0);
    for domain in [
        domain!(Request{count.in_range(1..=9)}),
        domain!(Request{count.in_range(1..=9),count.in_range(1..=3),lower.fixed_value(0),upper.fixed_value(9)}),
        domain!(Request{count.fixed_value(1),lower.in_range(5..=9),upper.in_range(0..=3),lower.less_equal(upper)}),
    ] {
        let mut c = certification("invalid definition").executor(|i: &Request| {
            calls.set(calls.get() + 1);
            execute(i)
        });
        c.domain(domain);
        let r = c.report();
        assert!(r.evaluation_error.is_some(), "{r}");
    }
    assert_eq!(calls.get(), 0);
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
enum Mode {
    Fast,
    Accurate,
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
enum Tuning {
    Fixed {
        strength: f64,
    },
    Adaptive {
        min_strength: f64,
        max_strength: f64,
    },
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
enum Payload {
    Fast { batch: usize },
    Accurate { iterations: usize, tuning: Tuning },
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
struct Config {
    threshold: f64,
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
struct Recursive {
    mode: Mode,
    window: usize,
    values: Vec<f64>,
    config: Config,
    payload: Payload,
}
fn recursive_domain() -> Domain<Recursive> {
    domain!(Recursive{
        mode.all_enum_values(),
        when(mode.equal(Mode::Fast)){window.in_range(1..=3)},
        when(mode.equal(Mode::Accurate)){window.in_range(4..=9)},
        values{length().in_range(3..=12),each(in_range(-1.0..=1.0))},
        config{threshold.one_of_ranges([-4.0..=-1.0,1.0..=4.0])},
        payload{all_enum_values(),Payload::Fast{batch.in_range(1..=4)},Payload::Accurate{iterations.in_range(1..=4),tuning{all_enum_values(),Tuning::Fixed{strength.in_range(0.0..=1.0)},Tuning::Adaptive{min_strength.in_range(0.0..=0.5),max_strength.in_range(0.5..=1.0),min_strength.less_equal(max_strength)}}}},
        window.less_equal(values.length()),
    })
}
#[test]
fn recursive_domains_materialize_active_variants_collections_conditions_and_relations() {
    let d = recursive_domain().compile().unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut count = 0;
    while let Some((input, c)) = d.next(0, &mut cursor, 3, &occupied).unwrap() {
        assert!(c.violated.is_empty());
        assert!(input.window <= input.values.len());
        assert!(match input.mode {
            Mode::Fast => (1..=3).contains(&input.window),
            Mode::Accurate => (4..=9).contains(&input.window),
        });
        if matches!(input.payload, Payload::Fast { .. }) {
            assert!(!c.entries.iter().any(|e| e.name.contains("tuning")));
        }
        occupied.insert(super::domain::compilation::coordinate_key(&c));
        count += 1;
        if count == 80 {
            break;
        }
    }
    assert!(count > 20);
}
#[derive(Debug, Clone, PartialEq, DomainInput)]
struct NestedRelationConfig {
    window: usize,
}

#[derive(Debug, Clone, PartialEq, DomainInput)]
struct NestedRelationInput {
    config: NestedRelationConfig,
    values: Vec<i32>,
}

#[test]
fn nested_struct_leaf_can_participate_in_cross_structure_relation() {
    let domain = domain!(NestedRelationInput {
        config { window.in_range(1..=6) },
        values { length().in_range(1..=6), each(in_range(0..=1)) },
        config.window.less_equal(values.length()),
    });
    let compiled = domain.compile().unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut saw_positive = false;
    while let Some((input, coordinate)) = compiled.next(0, &mut cursor, 11, &occupied).unwrap() {
        assert!(input.config.window <= input.values.len());
        saw_positive = true;
        occupied.insert(super::domain::compilation::coordinate_key(&coordinate));
    }
    assert!(saw_positive);
}

#[test]
fn single_and_double_negatives_violate_exactly_the_selected_constraints() {
    let d = request_domain().compile().unwrap();
    for category in [1, 2] {
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut count = 0;
        while let Some((i, c)) = d.next(category, &mut cursor, 3, &occupied).unwrap() {
            let violations = usize::from(!(1..=9).contains(&i.count))
                + usize::from(!(0..=9).contains(&i.lower))
                + usize::from(!(0..=9).contains(&i.upper))
                + usize::from(i.lower > i.upper);
            assert_eq!(violations, category);
            assert_eq!(c.violated.len(), category);
            occupied.insert(super::domain::compilation::coordinate_key(&c));
            count += 1;
        }
        assert!(count > 0);
    }
}
#[test]
fn real_inputs_prefill_generated_coordinates_and_zero_capacity_fails_closed() {
    let seen = RefCell::new(vec![]);
    let mut c = certification("real prefill").executor(|i: &Single| {
        seen.borrow_mut().push(i.value);
        if (1..=9).contains(&i.value) {
            Ok(())
        } else {
            Err(())
        }
    });
    c.data_source([Single { value: 1 }]);
    c.domain(domain!(Single{value.in_range(1..=9)}));
    c.cases_budget(8);
    let r = c.report();
    assert!(r.is_pass(), "{r}");
    assert_eq!(seen.borrow().iter().filter(|v| **v == 1).count(), 1);
    assert_eq!(seen.borrow()[1], 9);
    let mut c = certification("zero generation").executor(|_: &Single| Ok::<_, ()>(()));
    c.data_source([Single { value: 1 }]);
    c.domain(domain!(Single{value.in_range(1..=9)}));
    c.cases_budget(1);
    let r = c.report();
    assert!(r.evaluation_error.unwrap().message.contains("No budget"));
}
#[test]
fn failure_budget_distributes_representatives_and_stops_executor_immediately() {
    let calls = Cell::new(0);
    let mut c = certification("distributed failures").executor(|_: &Request| {
        calls.set(calls.get() + 1);
        Ok::<_, ()>(0)
    });
    c.domain(request_domain());
    c.cases_budget(100);
    let case = c.case();
    c.check(fact!(case.result() + 1).expect(0));
    let r = c.report();
    assert!(r.stopped, "{r}");
    assert_eq!(r.failures.len(), 5);
    assert_eq!(calls.get(), r.cases_executed());
    assert!(calls.get() < 100);
    assert!(
        r.failures
            .iter()
            .filter(|f| f.category == CaseCategory::Positive)
            .count()
            <= 3
    );
    assert!(r
        .failures
        .iter()
        .any(|f| f.category == CaseCategory::SingleNegative));
    assert!(!r.to_string().contains("showing"));
}
#[test]
fn weighted_scheduler_interleaves_and_redistributes() {
    let mut s = WeightedScheduler::default();
    let mut counts = [0; 3];
    let mut order = vec![];
    for _ in 0..10 {
        let i = s.next([true; 3]).unwrap();
        counts[i] += 1;
        order.push(i);
    }
    assert_eq!(counts, [5, 3, 2]);
    assert_eq!(&order[..3], &[0, 1, 2]);
    assert_eq!(s.next([false, true, false]), Some(1));
}
#[test]
fn seed_selection_is_local_and_anchor_invariant() {
    fn run(seed: u64) -> Vec<i32> {
        let d = domain!(Single{value.in_range(1..=1000)}).compile().unwrap();
        let mut occupied = BTreeSet::new();
        let mut cursor = 0;
        let mut out = vec![];
        for _ in 0..20 {
            let (i, c) = d.next(0, &mut cursor, seed, &occupied).unwrap().unwrap();
            out.push(i.value);
            occupied.insert(super::domain::compilation::coordinate_key(&c));
        }
        out
    }
    let a = run(1);
    let other = run(9);
    assert_eq!(a, run(1));
    assert_eq!(&a[..9], &other[..9]);
    assert_ne!(a, other);
}
#[test]
fn seeded_range_slots_continue_breadth_before_depth_across_terciles() {
    let d = domain!(Single{value.in_range(1..=10_000)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let mut terciles = vec![];
    while terciles.len() < 3 {
        let (_, coordinate) = d.next(0, &mut cursor, 7, &occupied).unwrap().unwrap();
        let entry = &coordinate.entries[0];
        if matches!(entry.slot, CaseDomainCoordinateSlot::Stratum(_)) {
            if let DomainRegion::Range { tercile, .. } = &entry.sector.region {
                terciles.push(*tercile);
            }
        }
        occupied.insert(super::domain::compilation::coordinate_key(&coordinate));
    }
    assert_eq!(
        terciles,
        [DomainTercile::Low, DomainTercile::Mid, DomainTercile::High]
    );
}
#[test]
fn single_negative_slots_cover_distinct_constraints_before_deeper_variants() {
    #[derive(DomainInput)]
    struct PairRange {
        first: i32,
        second: i32,
    }
    let d = domain!(PairRange{first.in_range(1..=9),second.in_range(1..=9)})
        .compile()
        .unwrap();
    let mut occupied = BTreeSet::new();
    let mut cursor = 0;
    let (_, first) = d.next(1, &mut cursor, 3, &occupied).unwrap().unwrap();
    occupied.insert(super::domain::compilation::coordinate_key(&first));
    let (_, second) = d.next(1, &mut cursor, 3, &occupied).unwrap().unwrap();
    assert_eq!(first.violated, [0]);
    assert_eq!(second.violated, [1]);
}

#[test]
fn report_mode_is_compact_and_failure_descriptions_are_preserved() {
    let mut c = certification("reporting").executor(|i: &Single| Ok::<_, ()>(i.value));
    c.data_source([Single { value: 3 }]);
    let case = c.case();
    c.check(
        fact!(*case.result())
            .expect(4)
            .description("canonical count"),
    );
    let r = c.report();
    let text = r.to_string();
    assert!(text.contains("canonical count"));
    assert!(text.contains("observed: Ok(3)"));
    let mut c = certification("debug report").executor(|i: &Single| Ok::<_, ()>(i.value));
    c.data_source([Single { value: 3 }]);
    c.report_mode(true);
    let case = c.case();
    c.check(fact!(*case.result()).expect(3));
    let r = c.report();
    assert!(r.is_pass());
    assert!(r.to_string().contains("CERTIFICATION REPORT"));
    assert!(r.to_string().contains("checks:"));
}
