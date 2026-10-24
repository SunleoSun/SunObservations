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
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum CountClass {
    Empty,
    Singleton,
    Few,
    Many,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
enum PayloadClass {
    Positive,
    Negative,
    Offset,
}
#[derive(Clone, Debug, PartialEq, DomainInput)]
struct IsolationInput {
    count: CountClass,
    payload: PayloadClass,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Observation)]
#[observation(namespace = "planned_capture::isolated_reading")]
struct Reading {
    execution: u64,
    index: usize,
    value: i64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Observation)]
#[observation(namespace = "planned_capture::isolated_marker")]
struct Marker {
    execution: u64,
    count: usize,
}
struct IsolationOutput {
    execution: u64,
    values: Vec<i64>,
}
fn isolated_production(
    input: &IsolationInput,
    execution: u64,
) -> Result<IsolationOutput, &'static str> {
    let count = match input.count {
        CountClass::Empty => 0,
        CountClass::Singleton => 1,
        CountClass::Few => 3,
        CountClass::Many => 9,
    };
    let values: Vec<_> = (0..count)
        .map(|i| match input.payload {
            PayloadClass::Positive => i as i64 + 1,
            PayloadClass::Negative => -(i as i64) - 1,
            PayloadClass::Offset => 10000 + i as i64,
        })
        .collect();
    for (index, value) in values.iter().enumerate() {
        observe!(Reading {
            execution,
            index,
            value: *value
        })
        .map_err(|_| "reading emission")?;
    }
    observe!(Marker { execution, count }).map_err(|_| "marker emission")?;
    Ok(IsolationOutput { execution, values })
}
fn expected_marker(output: &IsolationOutput) -> Vec<(u64, usize)> {
    vec![(output.execution, output.values.len())]
}
#[test]
fn multiple_generated_and_real_cases_have_execution_local_multitype_capture_datasets() {
    let mut execution = 0;
    let mut c = certification("case local capture datasets").executor(|input: &IsolationInput| {
        execution += 1;
        capture_observations(CaptureConfig::new().activate_all(), || {
            isolated_production(input, execution)
        })
    });
    c.data_source([
        IsolationInput {
            count: CountClass::Many,
            payload: PayloadClass::Negative,
        },
        IsolationInput {
            count: CountClass::Empty,
            payload: PayloadClass::Offset,
        },
    ]);
    c.domain(domain!(IsolationInput{count.all_enum_values(),payload.all_enum_values()}));
    c.seed(914).cases_budget(24);
    let case = c.case();
    c.check(
        fact!(case
            .observations::<Reading>()
            .unwrap()
            .values()
            .iter()
            .map(|r| (r.execution, r.index, r.value))
            .collect::<Vec<_>>())
        .expect(fact!(case
            .result()
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| (case.result().execution, i, *v))
            .collect::<Vec<_>>())),
    );
    c.check(
        fact!(case
            .observations::<Marker>()
            .unwrap()
            .values()
            .iter()
            .map(|r| (r.execution, r.count))
            .collect::<Vec<_>>())
        .expect(fact!(expected_marker(case.result()))),
    );
    c.check(
        fact!(case.result().values.len()).expect(fact!(match case.input().count {
            CountClass::Empty => 0,
            CountClass::Singleton => 1,
            CountClass::Few => 3,
            CountClass::Many => 9,
        })),
    );
    let report = c.report();
    assert!(report.is_pass(), "{report}");
    assert_eq!(report.coverage.pairwise_combinations, (12, 12));
    assert_eq!(report.executed[0], 2);
    assert!(report.executed[1] >= 10);
    assert_eq!(execution, report.cases_executed() as u64);
}
