use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

#[test]
fn checks_are_per_positive_case_and_aggregates_are_once_after_exploration() {
    let executed = Cell::new(0);
    let local = Cell::new(0);
    let aggregate = Cell::new(0);
    let mut runner = certification("two phases").executor(|n: &usize| {
        executed.set(executed.get() + 1);
        Ok::<_, ()>(n * 2)
    });
    runner.data_source([2, 2, 3]);
    let case = runner.case();
    let cases = runner.case_aggregate();
    runner.check(
        fact!({
            local.set(local.get() + 1);
            *case.result()
        })
        .expect(fact!(*case.input() * 2)),
    );
    runner.section("family", |s| {
        s.section("identity", |s| {
            s.check_aggregate(
                fact!({
                    aggregate.set(aggregate.get() + 1);
                    assert_eq!(executed.get(), 3);
                    assert_eq!(local.get(), 3);
                    let rows = cases.cases().unwrap();
                    assert_eq!(
                        rows.iter().map(|r| *r.input()).collect::<Vec<_>>(),
                        [2, 2, 3]
                    );
                    assert_eq!(
                        rows.iter().map(|r| r.id()).collect::<BTreeSet<_>>().len(),
                        3
                    );
                    assert_eq!(rows[0].execution(), &Ok(4));
                    rows.len()
                })
                .expect(3usize)
                .description("complete distinct executions"),
            );
        });
    });
    assert_eq!(executed.get(), 0);
    assert_eq!(aggregate.get(), 0);
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(local.get(), 3);
    assert_eq!(aggregate.get(), 1);
    assert_eq!(report.checks[1].phase, CheckPhase::Aggregate);
    assert_eq!(report.checks[1].section, "family/identity/");
}

#[test]
fn local_failures_keep_valid_evidence_and_independent_aggregate_failures_keep_sections() {
    let retained = Cell::new(0);
    let mut runner = certification("collect all phases").executor(|n: &usize| Ok::<_, ()>(*n));
    runner.data_source([1, 2]);
    let case = runner.case();
    let cases = runner.case_aggregate();
    runner.check(fact!(*case.result()).expect(0));
    runner.section("family", |s| {
        s.check_aggregate(
            fact!({
                retained.set(cases.cases().unwrap().len());
                retained.get()
            })
            .expect(9usize)
            .description("count"),
        );
        s.section("relation", |s| {
            s.check_aggregate(
                fact!({
                    let _ = cases.cases();
                    panic!("analysis blocked")
                })
                .expect(()),
            );
            s.check_aggregate(
                fact!(cases.cases().unwrap().len()).expect(fact!(cases.cases().unwrap().len() + 1)),
            );
        });
    });
    let report = runner.report();
    assert!(!report.is_pass());
    assert_eq!(retained.get(), 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.aggregate_failures.len(), 3);
    assert_eq!(report.aggregate_failures[0].name, "count");
    assert_eq!(
        report.aggregate_failures[1].class,
        CheckClass::EvaluationError
    );
    assert_eq!(report.aggregate_failures[2].section, "family/relation/");
    assert!(report.to_string().contains("aggregate family/relation/"));
}

#[derive(Clone, DomainInput)]
struct Input {
    value: i32,
}

#[test]
fn real_and_generated_positive_cases_share_one_aggregate_but_expected_rejections_do_not() {
    let positive = Cell::new(0);
    let rejected = Cell::new(0);
    let snapshot = RefCell::new(Vec::new());
    let mut runner = certification("positive family").executor(|r: &Input| {
        if !(1..=4).contains(&r.value) {
            rejected.set(rejected.get() + 1);
            Err(())
        } else {
            positive.set(positive.get() + 1);
            Ok(r.value * 3)
        }
    });
    runner.data_source([Input { value: 2 }, Input { value: 2 }]);
    runner.domain(domain!(Input { value.in_range(1..=4) }));
    runner.cases_budget(20);
    let cases = runner.case_aggregate();
    runner.check_aggregate(
        fact!({
            let rows = cases.cases().unwrap();
            snapshot.replace(rows.iter().map(|r| (*r.result(), r.id())).collect());
            rows.len()
        })
        .expect(criterion(
            "all positive executions",
            ViolationClass::Hard,
            |n: &usize| *n == positive.get(),
        )),
    );
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert!(report.executed[1] > 0);
    assert!(rejected.get() > 0);
    let rows = snapshot.borrow();
    assert_eq!(rows[0].0, rows[1].0);
    assert_ne!(rows[0].1, rows[1].1);
}

#[test]
fn rejected_positive_panic_and_prerequisite_fail_closed_after_early_stop() {
    struct Execution {
        value: usize,
        valid: bool,
    }
    impl CertificationExecution for Execution {
        type Output = usize;
        type Error = ();
        fn outcome(&self) -> Result<&usize, &()> {
            Ok(&self.value)
        }
        fn prerequisite(&self) -> Result<(), AnalysisError> {
            if self.valid {
                Ok(())
            } else {
                Err(AnalysisError {
                    message: "unusable evidence".into(),
                })
            }
        }
    }
    for mode in 0..2 {
        let calls = Cell::new(0);
        let mut runner = certification("invalid positive").executor(|n: &usize| {
            if *n == 2 && mode == 1 {
                panic!("production panic")
            }
            Execution {
                value: *n,
                valid: *n != 2,
            }
        });
        runner.data_source([1, 2, 3]).failure_budget(1);
        let cases = runner.case_aggregate();
        runner.check_aggregate(
            fact!({
                calls.set(calls.get() + 1);
                cases.cases().unwrap().len()
            })
            .expect(1usize),
        );
        let report = runner.report();
        assert!(!report.is_pass());
        assert_eq!(report.cases_executed(), 2);
        assert!(report.stopped);
        assert_eq!(calls.get(), 1);
        assert_eq!(
            report.aggregate_failures[0].class,
            CheckClass::EvaluationError
        );
    }
    let mut runner =
        certification("rejected positive")
            .executor(|n: &usize| if *n == 2 { Err(()) } else { Ok(*n) });
    runner.data_source([1, 2, 3]);
    let cases = runner.case_aggregate();
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(2usize));
    let report = runner.report();
    assert!(!report.is_pass());
    assert_eq!(
        report.aggregate_failures[0].class,
        CheckClass::EvaluationError
    );
}

#[test]
fn bounded_exploration_retains_locally_failed_evidence_and_empty_policy_is_explicit() {
    let mut runner = certification("stopped family").executor(|n: &usize| Ok::<_, ()>(*n));
    runner.data_source([1, 2, 3]).failure_budget(1);
    let case = runner.case();
    let cases = runner.case_aggregate();
    runner.check(fact!(*case.result()).expect(0));
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(1usize));
    let report = runner.report();
    assert!(!report.is_pass());
    assert!(report.stopped);
    assert!(report.aggregate_failures.is_empty());
    let mut runner = certification("negative only").executor(|_: &Input| Err::<(), _>(()));
    runner.data_source([Input { value: 0 }]);
    runner.domain(domain!(Input { value.in_range(1..=4) }));
    runner.cases_budget(1);
    let cases = runner.case_aggregate();
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(0usize));
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(CountRule::NonEmpty));
    let report = runner.report();
    assert!(!report.is_pass());
    assert!(report.failures.is_empty());
    assert_eq!(report.cases_executed(), 1);
    assert_eq!(report.aggregate_failures.len(), 1);
    assert_eq!(
        report.aggregate_failures[0].class,
        CheckClass::HardViolation
    );
}

#[test]
fn plain_result_executions_support_prepared_cross_case_numeric_relations() {
    let mut runner =
        certification("generic case analysis").executor(|n: &usize| Ok::<_, ()>(n * 3 + 7));
    runner.data_source([1, 1, 2, 4]);
    let cases = runner.case_aggregate();
    runner.check_aggregate(
        fact!({
            let rows = collection("ordinary executions", cases.cases().unwrap());
            let paired = rows.compare_with(&rows).on(|r| r.id(), |r| r.id());
            paired
                .numeric(|r| *r.input() as f64, |r| *r.result() as f64)
                .linear_fit()
                .observed
                .unwrap()
        })
        .expect(
            LinearFitExpectation::slope_between(2.99, 3.01)
                .unwrap()
                .and_intercept_between(6.99, 7.01)
                .unwrap(),
        ),
    );
    runner.assert_certify();
}

#[test]
fn unwinding_registration_discards_aggregate_children_and_restores_parent() {
    let mut runner = certification("section rollback").executor(|_: &usize| Ok::<_, ()>(()));
    runner.data_source([1]);
    let cases = runner.case_aggregate();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runner.section("discard", |s| {
            s.check_aggregate(fact!(cases.cases().unwrap().len()).expect(0usize));
            panic!("registration");
        });
    }));
    assert!(panic.is_err());
    runner.check_aggregate(fact!(cases.cases().unwrap().len()).expect(1usize));
    let report = runner.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.checks[0].section, "");
}
