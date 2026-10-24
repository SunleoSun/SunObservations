use std::{
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
};
use sun_observations::certifications::*;
#[derive(DomainInput)]
struct Input {
    value: i32,
}
const MODE: &str = "SUN_CERT_PROCESS_FIXTURE";
fn run_local(seed: u64) -> (PlannedCertificationReport, Vec<i32>) {
    let mut values = Vec::new();
    let mut c = certification("local overrides").executor(|i: &Input| {
        values.push(i.value);
        if (1..=1_000_000).contains(&i.value) {
            Ok(i.value)
        } else {
            Err(())
        }
    });
    c.domain(domain!(Input{value.in_range(1..=1_000_000)}));
    c.cases_budget(40).seed(seed);
    let report = c.report();
    (report, values)
}
fn check_panic(value: i32) -> bool {
    if value == 1 {
        panic!("planned check panic")
    }
    true
}
fn concurrent_hooks() {
    let hooks = Arc::new(AtomicUsize::new(0));
    let hook_counter = hooks.clone();
    std::panic::set_hook(Box::new(move |_| {
        hook_counter.fetch_add(1, Ordering::SeqCst);
    }));
    let entered = Arc::new(Barrier::new(5));
    let leave = Arc::new(Barrier::new(5));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let entered = entered.clone();
            let leave = leave.clone();
            std::thread::spawn(move || {
                let later = AtomicUsize::new(0);
                let mut c = certification("concurrent panic isolation").executor(
                    |i: &Input| -> Result<i32, ()> {
                        if i.value == 0 {
                            entered.wait();
                            leave.wait();
                            panic!("planned executor panic");
                        }
                        Ok(i.value)
                    },
                );
                c.data_source((0..3).map(|value| Input { value }));
                c.failure_budget(3);
                let case = c.case();
                c.check(fact!(check_panic(*case.result())).expect(true));
                c.check(
                    fact!({
                        let _ = case.result();
                        later.fetch_add(1, Ordering::SeqCst);
                        true
                    })
                    .expect(true),
                );
                let report = c.report();
                assert!(!report.is_pass());
                assert_eq!(report.cases_executed(), 3);
                assert_eq!(report.failures.len(), 2);
                assert_eq!(later.load(Ordering::SeqCst), 2);
                assert_eq!(report.failures[0].class, CheckClass::HardViolation);
                assert_eq!(report.failures[1].class, CheckClass::EvaluationError);
            })
        })
        .collect();
    entered.wait();
    assert!(std::panic::catch_unwind(|| panic!("unrelated concurrent panic")).is_err());
    assert_eq!(hooks.load(Ordering::SeqCst), 1);
    leave.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(hooks.load(Ordering::SeqCst), 1);
    assert!(std::panic::catch_unwind(|| panic!("outside restored depth")).is_err());
    assert_eq!(hooks.load(Ordering::SeqCst), 2);
}
#[test]
fn planned_process_contract() {
    if let Ok(mode) = std::env::var(MODE) {
        if mode == "concurrent" {
            concurrent_hooks();
            return;
        }
        if mode == "local" {
            let (a, x) = run_local(777);
            let (b, y) = run_local(777);
            let (_, z) = run_local(778);
            assert!(a.is_pass(), "{a}");
            assert!(b.is_pass(), "{b}");
            assert_eq!(a.seed, 777);
            assert_eq!(a.budget, 40);
            assert_eq!(a.cases_executed(), 40);
            assert_eq!(x, y);
            assert_ne!(x, z);
            return;
        }
        let mut c = certification("process contract").executor(|i: &Input| -> Result<i32, ()> {
            if mode == "panic" {
                panic!("production panic")
            }
            if (1..=9).contains(&i.value) {
                Ok(i.value)
            } else {
                Err(())
            }
        });
        c.domain(domain!(Input{value.in_range(1..=9)}));
        c.failure_budget(2);
        let case = c.case();
        c.check(fact!(*case.result()).expect(fact!(case.input().value)));
        c.assert_certify();
        return;
    }
    for mode in [
        "quiet",
        "report",
        "report_other",
        "panic",
        "local",
        "concurrent",
        "invalid_seed",
        "invalid_budget",
    ] {
        let invalid_values: &[&str] = if mode.starts_with("invalid") {
            &["", "not a number", "-1", "18446744073709551616"]
        } else {
            &["valid"]
        };
        for invalid in invalid_values {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "planned_process_contract", "--nocapture"])
                .env(MODE, mode)
                .env(
                    "SUN_CERT_SEED",
                    if mode == "invalid_seed" {
                        invalid
                    } else {
                        "321"
                    },
                )
                .env(
                    "SUN_CERT_CASES_BUDGET",
                    if mode == "invalid_budget" {
                        invalid
                    } else {
                        "5"
                    },
                )
                .env(
                    "SUN_CERT_REPORT",
                    match mode {
                        "report" => "1",
                        "report_other" => "true",
                        _ => "0",
                    },
                )
                .output()
                .unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert_eq!(
                output.status.success(),
                !matches!(mode, "panic" | "invalid_seed" | "invalid_budget"),
                "{mode} {invalid}: {stdout}
{stderr}"
            );
            match mode {
                "quiet" | "report_other" => {
                    assert!(stderr.is_empty(), "{stderr}");
                    assert!(!stdout.contains("CERTIFICATION"));
                }
                "report" => {
                    for text in [
                        "CERTIFICATION REPORT",
                        "seed: 321",
                        "cases executed: 5 / budget 5",
                        "coverage:",
                    ] {
                        assert!(stderr.contains(text), "{stderr}");
                    }
                }
                "panic" => {
                    assert!(stderr.contains("failure budget reached (2/2)"));
                    assert_eq!(stderr.matches("panicked at").count(), 1);
                    assert!(stderr.len() < 10000);
                }
                "invalid_seed" => assert!(stderr.contains("invalid SUN_CERT_SEED")),
                "invalid_budget" => assert!(stderr.contains("invalid SUN_CERT_CASES_BUDGET")),
                _ => (),
            }
        }
    }
}
