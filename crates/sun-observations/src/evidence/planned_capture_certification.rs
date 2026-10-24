use super::*;
use crate::certifications::*;
use crate::{observe, Observation};
use serde::{Deserialize, Serialize};
#[derive(DomainInput)]
struct Input {
    count: usize,
}
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "planned_capture::row")]
struct Row {
    index: usize,
}
fn execute(input: &Input) -> Result<Vec<usize>, &'static str> {
    if !(1..=3).contains(&input.count) {
        return Err("count outside contract");
    }
    for index in 0..input.count {
        observe!(Row { index }).map_err(|_| "emission failure")?;
    }
    Ok((0..input.count).collect())
}
#[test]
fn generated_capture_executes_production_and_checks_both_returned_and_observation_evidence() {
    let mut c = certification("production capture per input").executor(|input: &Input| {
        capture_observations(CaptureConfig::new().activate_all(), || execute(input))
    });
    c.domain(domain!(Input{count.in_range(1..=3)}));
    c.cases_budget(16);
    let case = c.case();
    c.check(fact!(case.result().len()).expect(fact!(case.input().count)));
    c.check(
        fact!(case.observations::<Row>().unwrap().values().len()).expect(fact!(case.input().count)),
    );
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert!(report.categories[0] > 0);
    assert!(report.categories[1] > 0);
}
#[test]
fn capture_completeness_cannot_be_bypassed_by_success_or_expected_rejection() {
    for rejected in [false, true] {
        let mut c = certification("incomplete capture").executor(|_: &Input| {
            capture_observations(
                CaptureConfig::new()
                    .activate_all()
                    .retention(crate::MemorySinkConfig::new(1, 1.0).unwrap()),
                || {
                    observe!(Row { index: 0 }).unwrap();
                    observe!(Row { index: 1 }).unwrap();
                    if rejected {
                        Err("rejected")
                    } else {
                        Ok(())
                    }
                },
            )
        });
        c.domain(domain!(Input{count.in_range(1..=3)}));
        c.cases_budget(8);
        let report = c.report();
        assert!(!report.is_pass());
        assert!(report
            .failures
            .iter()
            .all(|f| f.class == CheckClass::EvaluationError));
        assert!(report.to_string().contains("incomplete capture"));
    }
}
