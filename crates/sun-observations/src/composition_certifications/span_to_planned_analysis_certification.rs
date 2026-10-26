use crate::certifications::*;
use crate::evidence::*;
use crate::{observe_span, MemorySinkConfig};

#[allow(dead_code)]
#[path = "../../examples/profiling.rs"]
mod profiling;

#[test]
fn concrete_and_generated_executions_feed_per_case_and_aggregate_prepared_profiling() {
    profiling::certify();
}

#[test]
fn wrong_case_capture_cannot_satisfy_generated_profile_correspondence() {
    let mut runner =
        certification("wrong case negative control").executor(|_: &profiling::Request| {
            capture_observations(CaptureConfig::new().activate_all(), || {
                profiling::wait(&profiling::Request { delay_ms: 1 })
            })
        });
    runner.data_source([profiling::Request { delay_ms: 3 }]);
    runner.domain(domain!(profiling::Request { delay_ms.in_range(1..=3) }));
    runner.cases_budget(12);
    let case = runner.case();
    runner.check(
        fact!(
            case.spans::<profiling::Wait>().unwrap().values()[0]
                .value()
                .delay_ms
        )
        .expect(fact!(case.input().delay_ms)),
    );
    let report = runner.report();
    assert!(!report.is_pass());
    assert!(report
        .failures
        .iter()
        .any(|f| f.class == CheckClass::HardViolation));
}

#[test]
fn lost_complete_pairs_and_escaped_spans_fail_closed_before_profile_acceptance() {
    let run = capture_observations(
        CaptureConfig::new()
            .activate_all()
            .retention(MemorySinkConfig::new(2, 1.0).unwrap()),
        || {
            profiling::wait(&profiling::Request { delay_ms: 1 }).unwrap();
            profiling::wait(&profiling::Request { delay_ms: 1 })
        },
    );
    assert!(run.dataset().to_spans::<profiling::Wait>().is_ok());
    assert!(matches!(
        run.to_spans::<profiling::Wait>(),
        Err(SpanError::IncompleteCapture(_))
    ));
    let mut runner = certification("incomplete profiling").executor(|_: &usize| {
        capture_observations(CaptureConfig::new().activate_all(), || {
            let span = observe_span!(profiling::Wait { delay_ms: 1 }).unwrap();
            Ok::<_, ()>(span)
        })
    });
    runner.data_source([1usize]);
    let case = runner.case();
    runner.check(fact!(case.spans::<profiling::Wait>().unwrap().values().len()).expect(1usize));
    let report = runner.report();
    assert!(!report.is_pass());
    assert!(report
        .failures
        .iter()
        .all(|f| f.class == CheckClass::EvaluationError));
}

#[test]
fn elapsed_below_the_promised_delay_fails_the_same_prepared_acceptance_path() {
    let run = capture_observations(CaptureConfig::new().activate_all(), || {
        profiling::wait(&profiling::Request { delay_ms: 1 })
    });
    assert!(run.status().is_complete());
    let records = run
        .dataset()
        .records()
        .iter()
        .map(|r| {
            let mut r = (**r).clone();
            r.proc_ns = 0;
            std::sync::Arc::new(r)
        })
        .collect();
    let spans = ObservationDataset::try_from_records(records)
        .unwrap()
        .to_spans::<profiling::Wait>()
        .unwrap();
    let check = spans
        .source("early completion")
        .single_map(|m| m.duration())
        .expect(Criterion::at_least(std::time::Duration::from_millis(1)).unwrap());
    assert_eq!(check.class(), CheckClass::HardViolation);
}
