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
