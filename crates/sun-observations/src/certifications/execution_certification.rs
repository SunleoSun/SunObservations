use super::*;
use std::collections::BTreeMap;

#[test]
fn planned_sections_preserve_each_nested_failure_separately_from_description() {
    let mut c = certification("pipeline sections").executor(|n: &u64| Ok::<_, ()>(n * n));
    c.data_source([3]).diagnostic_bytes(32).report_mode(true);
    let case = c.case();
    let outside = fact!(*case.result()).expect(10).description("outside");
    c.section("transform", |s| {
        s.check(fact!(*case.result()).expect(8).description("same name"));
        s.section("range", |s| {
            s.check(fact!(*case.result()).expect(7).description("same name"));
            s.check(
                fact!({
                    let _ = case.input();
                    panic!("analysis")
                })
                .expect(()),
            );
        });
    });
    c.check(outside);
    let report = c.report();
    assert!(!report.is_pass());
    assert_eq!(report.failures.len(), 1);
    let failures = &report.failures[0].checks;
    assert_eq!(failures.len(), 4);
    assert_eq!(
        failures
            .iter()
            .map(|f| f.section.as_str())
            .collect::<Vec<_>>(),
        ["transform/", "transform/range/", "transform/range/", ""]
    );
    assert_eq!(failures[0].name, "same name");
    assert_eq!(failures[1].name, "same name");
    assert_eq!(failures[2].class, CheckClass::EvaluationError);
    assert_eq!(failures[3].name, "outside");
    assert_eq!(
        report
            .checks
            .iter()
            .map(|c| c.section.as_str())
            .collect::<Vec<_>>(),
        ["transform/", "transform/range/", "transform/range/", ""]
    );
    assert!(report.to_string().contains("transform/range/"));
    let mut compact = report.clone();
    compact.report_mode = false;
    assert!(compact.to_string().contains("transform/range/"));
}

#[test]
fn planned_sections_share_eager_paths_and_do_not_leak_into_siblings() {
    let mut base = certification("mixed checks");
    base.section("prerequisites", |s| {
        s.section("schema", |s| {
            s.check_fact_expect("version", 1, 2);
        });
    });
    let mut c = base.executor(|n: &u64| Ok::<_, ()>(n * n));
    c.data_source([3]);
    let case = c.case();
    c.section("transform", |s| {
        s.section("range", |s| {
            s.check(fact("eager value", 1).expect(2));
            s.check(fact!(*case.result()).expect(9));
        });
    });
    c.section("delivery", |s| {
        s.check(fact("eager delivery", false).expect(true));
    });
    c.check(fact("root", false).expect(true));
    let report = c.report();
    assert!(report.failures.is_empty());
    assert_eq!(
        report
            .eager
            .failures()
            .iter()
            .map(|f| f.section.as_str())
            .collect::<Vec<_>>(),
        ["prerequisites/schema/", "transform/range/", "delivery/", ""]
    );
    let text = report.to_string();
    for section in ["prerequisites/schema/", "transform/range/", "delivery/"] {
        assert!(text.contains(section), "{text}");
    }
}

#[test]
fn planned_section_registration_unwind_restores_parent_and_discards_child_checks() {
    let mut c = certification("registration unwind").executor(|n: &u64| Ok::<_, ()>(n * n));
    c.data_source([3]).report_mode(true);
    let case = c.case();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        c.section("abandoned", |s| {
            s.check(fact!(*case.result()).expect(0));
            s.check(fact("eager", false).expect(true));
            panic!("registration interrupted");
        });
    }));
    assert!(panic.is_err());
    c.section("accepted", |s| {
        s.check(fact!(*case.result()).expect(9));
    });
    c.check(fact!(*case.result()).expect(9));
    let report = c.report();
    report.assert_certify();
    assert_eq!(report.checks.len(), 2);
    assert_eq!(report.checks[0].section, "accepted/");
    assert_eq!(report.checks[1].section, "");
    assert!(report.to_string().contains("accepted/"));
    assert!(!report.to_string().contains("abandoned"));
}

#[test]
fn planned_sections_preserve_bounded_utf8_paths_and_domain_execution() {
    let mut c = certification("bounded section").executor_with_seed(|n: &u64, _seed| {
        if (1..=3).contains(n) {
            Ok(n * n)
        } else {
            Err(())
        }
    });
    c.domain(domain!(u64 { in_range(1..=3) }))
        .seed(71)
        .cases_budget(16);
    let case = c.case();
    c.section("я".repeat(200), |s| {
        s.section("界".repeat(200), |s| {
            s.check(fact!(*case.result() > 0).expect(true));
        });
    });
    let report = c.report();
    report.assert_certify();
    assert!(report.checks[0].section.len() <= 515);
    assert!(report.checks[0].section.ends_with('…'));
    assert!(report.categories[0] > 0);
    assert!(report.categories[1] > 0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, DomainInput)]
enum Choice {
    A,
    B,
    C,
}
#[derive(Clone, Copy, DomainInput)]
struct Pair {
    left: Choice,
    right: Choice,
}
fn pair_domain() -> Domain<Pair> {
    domain!(Pair { left.all_enum_values(), right.all_enum_values() })
}
fn generated_seeds(
    seed: u64,
    name: &str,
    prefill: bool,
    budget: usize,
) -> Vec<((Choice, Choice), u64)> {
    let mut seen = vec![];
    let mut c = certification(name).executor_with_seed(|input: &Pair, seed| {
        seen.push(((input.left, input.right), seed));
        Ok::<_, ()>(input.left)
    });
    if prefill {
        c.data_source([Pair {
            left: Choice::C,
            right: Choice::C,
        }]);
    }
    c.domain(pair_domain());
    c.seed(seed)
        .cases_budget(budget)
        .require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(fact!(case.input().left)));
    c.assert_certify();
    seen
}
#[test]
fn generated_seeds_replay_vary_by_global_seed_and_identity_and_are_case_local() {
    let first = generated_seeds(71, "case seeds", false, 32);
    assert_eq!(first.len(), 9);
    assert_eq!(first, generated_seeds(71, "case seeds", false, 32));
    assert_eq!(first, generated_seeds(71, "case seeds", false, 9));
    assert_eq!(
        first
            .iter()
            .map(|(_, seed)| seed)
            .collect::<BTreeSet<_>>()
            .len(),
        9
    );
    let first: BTreeMap<_, _> = first.into_iter().collect();
    for changed in [
        generated_seeds(72, "case seeds", false, 32),
        generated_seeds(71, "other identity", false, 32),
    ] {
        let changed: BTreeMap<_, _> = changed.into_iter().collect();
        assert_eq!(
            first.keys().collect::<Vec<_>>(),
            changed.keys().collect::<Vec<_>>()
        );
        assert!(first.iter().all(|(case, seed)| changed[case] != *seed));
    }
}
#[test]
fn real_prefill_moves_generated_execution_ordinals_without_changing_case_seeds() {
    let first = generated_seeds(71, "prefilled case seeds", false, 32);
    let prefilled = generated_seeds(71, "prefilled case seeds", true, 32);
    assert_eq!(prefilled.len(), 9);
    assert_ne!(first[0].0, prefilled[0].0);
    let first_seeds: BTreeMap<_, _> = first.iter().copied().collect();
    let mut moved = false;
    for (ordinal, (case, seed)) in prefilled.iter().enumerate().skip(1) {
        assert_eq!(*seed, first_seeds[case]);
        moved |= first.iter().position(|(key, _)| key == case).unwrap() != ordinal;
    }
    assert!(moved);
}
#[test]
fn real_case_seeds_follow_source_ordinals_without_requiring_domain_input() {
    struct RealInput(u8);
    fn run(seed: u64, count: u8, split: bool) -> Vec<u64> {
        let mut seen = vec![];
        let mut c =
            certification("real case seeds").executor_with_seed(|input: &RealInput, seed| {
                seen.push(seed);
                Ok::<_, ()>(input.0)
            });
        if split {
            c.data_source([RealInput(0)]);
            c.data_source((1..count).map(RealInput));
        } else {
            c.data_source((0..count).map(RealInput));
        }
        c.seed(seed).cases_budget(1);
        let case = c.case();
        c.check(fact!(*case.result()).expect(fact!(case.input().0)));
        c.assert_certify();
        seen
    }
    let seeds = run(71, 3, false);
    assert_eq!(seeds, run(71, 3, true));
    assert_eq!(seeds, run(71, 5, false)[..3]);
    assert!(seeds.iter().zip(run(72, 3, false)).all(|(a, b)| *a != b));
    assert_eq!(seeds.iter().collect::<BTreeSet<_>>().len(), 3);
}
fn evaluate<
    F: FnMut(&i32) -> Result<i32, &'static str>,
    S: FnMut(&i32, u64) -> Result<i32, &'static str>,
>(
    mut c: PlannedCertification<'_, i32, Result<i32, &'static str>, F, S>,
) -> PlannedCertificationReport {
    c.data_source([-1, 1, 2, 3]);
    c.domain(domain!(i32 { in_range(1..=3) }));
    c.seed(71).cases_budget(12).failure_budget(2);
    let case = c.case();
    c.check(fact!(*case.result()).expect(fact!(*case.input())));
    c.report()
}
#[test]
fn executor_modes_share_results_rejection_panics_checks_and_failure_stops() {
    fn execute(input: &i32, behavior: u8) -> Result<i32, &'static str> {
        match behavior {
            0 if (1..=3).contains(input) => Ok(*input),
            0 | 1 => Err("rejected"),
            2 => Ok(*input),
            3 => panic!("executor panic"),
            _ => Ok(input + 1),
        }
    }
    for behavior in 0..5 {
        let ordinary =
            evaluate(certification("ordinary outcomes").executor(|i| execute(i, behavior)));
        let seeded = evaluate(
            certification("seeded outcomes").executor_with_seed(|i, _| execute(i, behavior)),
        );
        assert_eq!(ordinary.is_pass(), behavior == 0, "{ordinary}");
        assert_eq!(ordinary.is_pass(), seeded.is_pass(), "{seeded}");
        assert_eq!(ordinary.executed, seeded.executed);
        assert_eq!(ordinary.categories, seeded.categories);
        assert_eq!(ordinary.stopped, seeded.stopped);
        assert_eq!(ordinary.failures.len(), seeded.failures.len());
        for (a, b) in ordinary.failures.iter().zip(&seeded.failures) {
            assert_eq!(a.class, b.class);
            assert_eq!(a.category, b.category);
            assert_eq!(a.check, b.check);
            assert_eq!(a.diagnostic, b.diagnostic);
        }
    }
}
#[test]
fn ordinary_executor_does_not_request_a_case_seed() {
    let mut executor: Executor<_, fn(&i32, u64) -> _> =
        Executor::Ordinary(|input: &i32| Ok::<_, ()>(*input));
    let coordinate = CaseDomainCoordinate {
        entries: vec![],
        violated: vec![],
    };
    assert!(execute_case(
        &mut executor,
        1,
        || panic!("ordinary executor requested seed"),
        &coordinate,
        &[],
        128,
        None
    )
    .is_none());
}
fn pair_report(budget: usize, required: bool, real: Vec<Pair>) -> PlannedCertificationReport {
    let mut c = certification("pairwise requirement").executor(|_: &Pair| Ok::<_, ()>(true));
    c.domain(pair_domain());
    c.data_source(real);
    c.seed(71).cases_budget(budget);
    if required {
        c.require_pairwise_coverage().require_pairwise_coverage();
    }
    let case = c.case();
    c.check(fact!(*case.result()).expect(true));
    c.report()
}
#[test]
fn pairwise_requirement_passes_with_sufficient_budget_and_real_prefill() {
    let report = pair_report(
        9,
        true,
        vec![Pair {
            left: Choice::C,
            right: Choice::C,
        }],
    );
    report.assert_certify();
    assert_eq!(report.executed, [1, 8]);
    assert_eq!(report.coverage.enum_values, (6, 6));
    assert_eq!(report.coverage.pairwise_combinations, (9, 9));
}
#[test]
fn pairwise_requirement_fails_at_normal_budget_even_when_all_checks_pass() {
    let ordinary = pair_report(1, false, vec![]);
    ordinary.assert_certify();
    let report = pair_report(1, true, vec![]);
    assert!(!report.is_pass());
    assert_eq!(report.cases_executed(), 1);
    assert!(report.failures.is_empty());
    assert_eq!(report.coverage.pairwise_combinations, (1, 9));
    assert!(report
        .to_string()
        .contains("required pairwise coverage not reached: 1 / 9"));
    assert!(catch_unwind(AssertUnwindSafe(|| report.assert_certify())).is_err());
}
#[derive(Clone, DomainInput)]
enum Nested {
    AloneA,
    AloneB,
    AloneC,
    With { inner: Choice },
}
#[derive(DomainInput)]
struct NestedInput {
    value: Nested,
}
#[test]
fn complete_pairs_do_not_hide_uncovered_enum_values() {
    let mut c = certification("isolated enum values").executor(|_: &NestedInput| Ok::<_, ()>(true));
    c.domain(domain!(NestedInput { value { all_enum_values(), Nested::With { inner.all_enum_values() } } }));
    c.data_source([Choice::A, Choice::B, Choice::C].map(|inner| NestedInput {
        value: Nested::With { inner },
    }));
    c.cases_budget(4).require_pairwise_coverage();
    let case = c.case();
    c.check(fact!(*case.result()).expect(true));
    let report = c.report();
    assert_eq!(report.cases_executed(), 4);
    assert_eq!(report.coverage.pairwise_combinations, (3, 3));
    assert!(report.coverage.enum_values.0 < report.coverage.enum_values.1);
    assert!(report.failures.is_empty());
    assert!(!report.is_pass());
    assert!(report.to_string().contains("enum values:"));
}
#[test]
fn missing_or_zero_pairwise_catalog_cannot_pass_a_declared_requirement() {
    for registered in [false, true] {
        let mut c =
            certification("missing pairwise catalog").executor(|_: &Choice| Ok::<_, ()>(()));
        c.data_source([Choice::A]);
        if registered {
            c.domain(domain!(Choice { all_enum_values() }));
        }
        c.cases_budget(8).require_pairwise_coverage();
        let report = c.report();
        assert!(!report.is_pass());
        assert_eq!(report.coverage.pairwise_combinations, (0, 0));
        assert!(report
            .to_string()
            .contains("needs a Domain with feasible pairwise combinations"));
    }
}
#[test]
fn pairwise_requirement_does_not_bypass_failure_budget() {
    let mut c = certification("pairwise hard stop").executor(|_: &Pair| Err::<(), _>("rejected"));
    c.domain(pair_domain());
    c.cases_budget(32)
        .failure_budget(1)
        .require_pairwise_coverage();
    let report = c.report();
    assert!(!report.is_pass());
    assert!(report.stopped);
    assert_eq!(report.cases_executed(), 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.coverage.pairwise_combinations, (1, 9));
}

mod planned_controls {
    use super::*;
    use std::cell::Cell;

    #[derive(DomainInput)]
    struct Control {
        value: i32,
    }
    fn domain() -> Domain<Control> {
        domain!(Control{value.in_range(1..=10)})
    }

    #[test]
    fn zero_failure_budget_and_duplicate_domain_fail_before_any_execution() {
        for duplicate in [false, true] {
            let calls = Cell::new(0);
            let mut c = certification("invalid controls").executor(|_: &Control| {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(())
            });
            c.domain(domain());
            if duplicate {
                c.domain(domain());
            } else {
                c.failure_budget(0);
            }
            let report = c.report();
            assert_eq!(calls.get(), 0);
            assert_eq!(report.cases_executed(), 0);
            assert!(!report.is_pass());
            assert!(report
                .evaluation_error
                .unwrap()
                .message
                .contains(if duplicate {
                    "Domain already registered"
                } else {
                    "failure budget must be positive"
                }));
        }
    }

    #[test]
    fn planned_diagnostics_bound_utf8_evidence_with_one_truncation_marker() {
        for bytes in [0, 1, 17, 64, 257] {
            let mut c = certification("bounded diagnostic")
                .executor(|_: &Control| Err::<(), _>("Ж🙂".repeat(10000)));
            c.domain(domain());
            c.diagnostic_bytes(bytes).failure_budget(1);
            let report = c.report();
            assert!(!report.is_pass());
            assert!(report.stopped);
            for failure in &report.failures {
                assert!(failure.diagnostic.ends_with('…'));
                for text in [&failure.diagnostic, &failure.coordinate] {
                    assert!(
                        text.len() <= bytes + '…'.len_utf8(),
                        "{} > {bytes}",
                        text.len()
                    );
                    assert!(std::str::from_utf8(text.as_bytes()).is_ok());
                }
            }
        }
    }

    #[test]
    fn panicking_check_does_not_skip_later_checks_or_escape_case_failure_budget() {
        let later = Cell::new(0);
        let mut c =
            certification("check panic continuation").executor(|i: &Control| Ok::<_, ()>(i.value));
        c.data_source([
            Control { value: 1 },
            Control { value: 2 },
            Control { value: 3 },
        ]);
        c.failure_budget(1);
        let case = c.case();
        c.check(
            fact!({
                let _ = case.result();
                panic!("check panic");
                #[allow(unreachable_code)]
                true
            })
            .expect(true),
        );
        c.check(
            fact!({
                let _ = case.result();
                later.set(later.get() + 1);
                true
            })
            .expect(true),
        );
        let report = c.report();
        assert_eq!(later.get(), 1);
        assert_eq!(report.cases_executed(), 1);
        assert!(report.stopped);
        assert_eq!(report.failures[0].class, CheckClass::EvaluationError);
        assert!(report.failures[0].diagnostic.contains("check panic"));
    }
}
